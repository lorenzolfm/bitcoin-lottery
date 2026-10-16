//! The game itself: what a round is, what a valid bet is, and who wins.

use chrono::NaiveDateTime;

pub const NAME_MAX_CHARS: usize = 32;
pub const TITLE_MAX_CHARS: usize = 80;
/// Caps how much a single public round can grow if someone scripts the form.
pub const MAX_BETS_PER_ROUND: i64 = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    pub id: i32,
    pub title: String,
    pub created_at: NaiveDateTime,
    pub phase: Phase,
}

/// Where a round is in its life. Each phase carries exactly the data that
/// exists at that point, so an open round with a nonce cannot be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Taking bets. Guesses stay hidden from everyone but the bettor.
    Open,
    /// Betting closed; waiting for the block at `target_height` to be mined.
    Closed {
        target_height: i32,
        closed_at: NaiveDateTime,
    },
    /// The deciding block is known.
    Settled {
        block: Block,
        closed_at: NaiveDateTime,
        settled_at: NaiveDateTime,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub height: i32,
    pub hash: String,
    pub nonce: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bet {
    pub id: i32,
    pub name: String,
    pub guess: u32,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Placement<'a> {
    /// 1 is the winner.
    pub rank: usize,
    pub distance: u32,
    pub bet: &'a Bet,
}

/// Orders bets from closest to furthest from `nonce`. When two guesses are
/// equally close, the one placed first ranks higher.
pub fn standings(bets: &[Bet], nonce: u32) -> Vec<Placement<'_>> {
    let mut by_distance: Vec<(u32, &Bet)> = bets
        .iter()
        .map(|bet| (bet.guess.abs_diff(nonce), bet))
        .collect();
    // Bet ids grow with insertion order, so they double as submission order.
    by_distance.sort_by_key(|&(distance, bet)| (distance, bet.id));
    by_distance
        .into_iter()
        .enumerate()
        .map(|(index, (distance, bet))| Placement {
            rank: index + 1,
            distance,
            bet,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ValidationError(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetName(String);

impl BetName {
    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        clean_text(raw, "name", NAME_MAX_CHARS).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundTitle(String);

impl RoundTitle {
    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        clean_text(raw, "title", TITLE_MAX_CHARS).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A block nonce is a 32-bit unsigned integer, so that is the guess range.
pub fn parse_guess(raw: i64) -> Result<u32, ValidationError> {
    u32::try_from(raw).map_err(|_| {
        ValidationError(format!(
            "guess must be a whole number from 0 to {}",
            u32::MAX
        ))
    })
}

fn clean_text(raw: &str, field: &str, max_chars: usize) -> Result<String, ValidationError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(ValidationError(format!("{field} cannot be empty")));
    }
    // Counted in chars to match SQLite's length() on TEXT.
    if text.chars().count() > max_chars {
        return Err(ValidationError(format!(
            "{field} must be at most {max_chars} characters"
        )));
    }
    if text.chars().any(char::is_control) {
        return Err(ValidationError(format!(
            "{field} cannot contain control characters"
        )));
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bet(id: i32, name: &str, guess: u32) -> Bet {
        Bet {
            id,
            name: name.to_owned(),
            guess,
            created_at: NaiveDateTime::default(),
        }
    }

    fn ranked_names(bets: &[Bet], nonce: u32) -> Vec<(&str, u32)> {
        standings(bets, nonce)
            .into_iter()
            .map(|placement| (placement.bet.name.as_str(), placement.distance))
            .collect()
    }

    #[test]
    fn closest_guess_wins_in_either_direction() {
        let bets = [
            bet(1, "far", 100),
            bet(2, "above", 1_010),
            bet(3, "below", 995),
        ];
        assert_eq!(
            ranked_names(&bets, 1_000),
            [("below", 5), ("above", 10), ("far", 900)]
        );
    }

    #[test]
    fn equal_distance_goes_to_the_earlier_bet() {
        // Inserted out of id order so the result cannot come from input order.
        let bets = [bet(7, "late-above", 1_010), bet(3, "early-below", 990)];
        assert_eq!(
            ranked_names(&bets, 1_000),
            [("early-below", 10), ("late-above", 10)]
        );
    }

    #[test]
    fn distance_spans_the_whole_nonce_range() {
        let bets = [bet(1, "zero", 0), bet(2, "max", u32::MAX)];
        assert_eq!(ranked_names(&bets, 0), [("zero", 0), ("max", u32::MAX)]);
        assert_eq!(
            ranked_names(&bets, u32::MAX),
            [("max", 0), ("zero", u32::MAX)]
        );
    }

    #[test]
    fn guess_must_fit_in_a_nonce() {
        assert_eq!(parse_guess(0), Ok(0));
        assert_eq!(parse_guess(i64::from(u32::MAX)), Ok(u32::MAX));
        assert!(parse_guess(-1).is_err());
        assert!(parse_guess(i64::from(u32::MAX) + 1).is_err());
    }

    #[test]
    fn names_are_trimmed_and_bounded_in_characters() {
        assert_eq!(BetName::parse("  satoshi \n").unwrap().as_str(), "satoshi");
        // 32 multi-byte characters is 128 bytes but still a valid name.
        assert!(BetName::parse(&"₿".repeat(NAME_MAX_CHARS)).is_ok());
        assert!(BetName::parse(&"₿".repeat(NAME_MAX_CHARS + 1)).is_err());
        assert!(BetName::parse("   ").is_err());
        assert!(BetName::parse("tab\tinside").is_err());
    }
}
