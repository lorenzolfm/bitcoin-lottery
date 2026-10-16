//! Persistence for rounds and bets. Every state change runs in an immediate
//! transaction, so the phase check and the write cannot be interleaved with
//! another writer.

use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel::sqlite::SqliteConnection;

use crate::lottery::{Bet, BetName, Block, MAX_BETS_PER_ROUND, Phase, Round, RoundTitle};
use crate::schema::{bets, rounds};

const STATUS_OPEN: &str = "open";
const STATUS_CLOSED: &str = "closed";
const STATUS_SETTLED: &str = "settled";
const HISTORY_LIMIT: i64 = 120;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("round not found")]
    RoundNotFound,
    #[error("bet not found")]
    BetNotFound,
    #[error("another round is still running; wait for it to settle first")]
    ActiveRoundExists,
    #[error("betting is closed for this round")]
    BettingClosed,
    #[error("this round is not waiting for a block")]
    NotAwaitingBlock,
    #[error("block {got} does not decide this round; it is waiting for block {expected}")]
    WrongBlock { expected: i32, got: i32 },
    #[error("this round is already settled")]
    AlreadySettled,
    #[error("someone already bet as \"{0}\" in this round")]
    NameTaken(String),
    #[error("this round already has {MAX_BETS_PER_ROUND} bets")]
    RoundFull,
    #[error("stored data breaks an invariant: {0}")]
    Corrupt(String),
    #[error(transparent)]
    Database(#[from] DieselError),
    #[error(transparent)]
    Pool(#[from] diesel::r2d2::PoolError),
}

pub type RoundWithBets = (Round, Vec<Bet>);

pub fn create_round(conn: &mut SqliteConnection, title: &RoundTitle) -> Result<Round, StoreError> {
    let row = diesel::insert_into(rounds::table)
        .values((
            rounds::title.eq(title.as_str()),
            rounds::status.eq(STATUS_OPEN),
        ))
        .returning(RoundRow::as_returning())
        .get_result(conn)
        .map_err(|error| match error {
            // The only unique index on rounds allows a single unsettled round.
            DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => {
                StoreError::ActiveRoundExists
            }
            other => other.into(),
        })?;
    row.try_into()
}

/// The newest round. Only the newest round can be unsettled, because a new
/// round cannot start while another is running.
pub fn current_round(conn: &mut SqliteConnection) -> Result<Option<RoundWithBets>, StoreError> {
    let row = rounds::table
        .order(rounds::id.desc())
        .select(RoundRow::as_select())
        .first(conn)
        .optional()?;
    row.map(|row| with_bets(conn, row)).transpose()
}

/// The round that is taking bets or waiting for its block, if any.
pub fn active_round(conn: &mut SqliteConnection) -> Result<Option<Round>, StoreError> {
    rounds::table
        .filter(rounds::status.ne(STATUS_SETTLED))
        .select(RoundRow::as_select())
        .first(conn)
        .optional()?
        .map(Round::try_from)
        .transpose()
}

pub fn round_with_bets(
    conn: &mut SqliteConnection,
    round_id: i32,
) -> Result<RoundWithBets, StoreError> {
    let row = find_round_row(conn, round_id)?;
    with_bets(conn, row)
}

/// Recent rounds, newest first, each with its bets in submission order.
pub fn history(conn: &mut SqliteConnection) -> Result<Vec<RoundWithBets>, StoreError> {
    let round_rows: Vec<RoundRow> = rounds::table
        .order(rounds::id.desc())
        .limit(HISTORY_LIMIT)
        .select(RoundRow::as_select())
        .load(conn)?;
    let bet_rows: Vec<BetRow> = BetRow::belonging_to(&round_rows)
        .order(bets::id.asc())
        .select(BetRow::as_select())
        .load(conn)?;
    let grouped = bet_rows.grouped_by(&round_rows);
    round_rows
        .into_iter()
        .zip(grouped)
        .map(|(round_row, bet_rows)| {
            let bets = bet_rows
                .into_iter()
                .map(Bet::try_from)
                .collect::<Result<_, _>>()?;
            Ok((round_row.try_into()?, bets))
        })
        .collect()
}

pub fn place_bet(
    conn: &mut SqliteConnection,
    round_id: i32,
    name: &BetName,
    guess: u32,
) -> Result<Bet, StoreError> {
    conn.immediate_transaction(|conn| {
        let round = Round::try_from(find_round_row(conn, round_id)?)?;
        if round.phase != Phase::Open {
            return Err(StoreError::BettingClosed);
        }
        let count: i64 = bets::table
            .filter(bets::round_id.eq(round_id))
            .count()
            .get_result(conn)?;
        if count >= MAX_BETS_PER_ROUND {
            return Err(StoreError::RoundFull);
        }
        let row = diesel::insert_into(bets::table)
            .values((
                bets::round_id.eq(round_id),
                bets::name.eq(name.as_str()),
                bets::guess.eq(i64::from(guess)),
            ))
            .returning(BetRow::as_returning())
            .get_result(conn)
            .map_err(|error| match error {
                // Names are unique per round, ignoring case.
                DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => {
                    StoreError::NameTaken(name.as_str().to_owned())
                }
                other => other.into(),
            })?;
        row.try_into()
    })
}

/// Stops betting. `target_height` is the block that will decide the round.
pub fn close_round(
    conn: &mut SqliteConnection,
    round_id: i32,
    target_height: i32,
) -> Result<Round, StoreError> {
    conn.immediate_transaction(|conn| {
        let round = Round::try_from(find_round_row(conn, round_id)?)?;
        if round.phase != Phase::Open {
            return Err(StoreError::BettingClosed);
        }
        diesel::update(rounds::table.find(round_id))
            .set((
                rounds::status.eq(STATUS_CLOSED),
                rounds::target_height.eq(target_height),
                rounds::closed_at.eq(now()),
            ))
            .returning(RoundRow::as_returning())
            .get_result(conn)?
            .try_into()
    })
}

pub fn settle_round(
    conn: &mut SqliteConnection,
    round_id: i32,
    block: &Block,
) -> Result<Round, StoreError> {
    conn.immediate_transaction(|conn| {
        let round = Round::try_from(find_round_row(conn, round_id)?)?;
        match round.phase {
            Phase::Closed { target_height, .. } if target_height == block.height => {}
            Phase::Closed { target_height, .. } => {
                return Err(StoreError::WrongBlock {
                    expected: target_height,
                    got: block.height,
                });
            }
            Phase::Open => return Err(StoreError::NotAwaitingBlock),
            Phase::Settled { .. } => return Err(StoreError::AlreadySettled),
        }
        diesel::update(rounds::table.find(round_id))
            .set((
                rounds::status.eq(STATUS_SETTLED),
                rounds::block_hash.eq(&block.hash),
                rounds::nonce.eq(i64::from(block.nonce)),
                rounds::settled_at.eq(now()),
            ))
            .returning(RoundRow::as_returning())
            .get_result(conn)?
            .try_into()
    })
}

/// Removes a bet, for moderating names on a projected screen. Settled rounds
/// are final: removing a bet there would change who won after the fact.
pub fn delete_bet(conn: &mut SqliteConnection, bet_id: i32) -> Result<(), StoreError> {
    conn.immediate_transaction(|conn| {
        let round_id: i32 = bets::table
            .find(bet_id)
            .select(bets::round_id)
            .first(conn)
            .optional()?
            .ok_or(StoreError::BetNotFound)?;
        let round = Round::try_from(find_round_row(conn, round_id)?)?;
        if matches!(round.phase, Phase::Settled { .. }) {
            return Err(StoreError::AlreadySettled);
        }
        diesel::delete(bets::table.find(bet_id)).execute(conn)?;
        Ok(())
    })
}

fn now() -> NaiveDateTime {
    Utc::now().naive_utc()
}

fn find_round_row(conn: &mut SqliteConnection, round_id: i32) -> Result<RoundRow, StoreError> {
    rounds::table
        .find(round_id)
        .select(RoundRow::as_select())
        .first(conn)
        .optional()?
        .ok_or(StoreError::RoundNotFound)
}

fn with_bets(conn: &mut SqliteConnection, row: RoundRow) -> Result<RoundWithBets, StoreError> {
    let bets = BetRow::belonging_to(&row)
        .order(bets::id.asc())
        .select(BetRow::as_select())
        .load(conn)?
        .into_iter()
        .map(Bet::try_from)
        .collect::<Result<_, _>>()?;
    Ok((row.try_into()?, bets))
}

#[derive(Debug, Queryable, Selectable, Identifiable)]
#[diesel(table_name = rounds)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct RoundRow {
    id: i32,
    title: String,
    status: String,
    target_height: Option<i32>,
    block_hash: Option<String>,
    nonce: Option<i64>,
    created_at: NaiveDateTime,
    closed_at: Option<NaiveDateTime>,
    settled_at: Option<NaiveDateTime>,
}

#[derive(Debug, Queryable, Selectable, Identifiable, Associations)]
#[diesel(table_name = bets)]
#[diesel(belongs_to(RoundRow, foreign_key = round_id))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct BetRow {
    id: i32,
    round_id: i32,
    name: String,
    guess: i64,
    created_at: NaiveDateTime,
}

impl TryFrom<RoundRow> for Round {
    type Error = StoreError;

    fn try_from(row: RoundRow) -> Result<Self, StoreError> {
        let corrupt = || StoreError::Corrupt(format!("round {} has an inconsistent state", row.id));
        let phase = match (
            row.status.as_str(),
            row.target_height,
            row.closed_at,
            &row.block_hash,
            row.nonce,
            row.settled_at,
        ) {
            (STATUS_OPEN, None, None, None, None, None) => Phase::Open,
            (STATUS_CLOSED, Some(target_height), Some(closed_at), None, None, None) => {
                Phase::Closed {
                    target_height,
                    closed_at,
                }
            }
            (
                STATUS_SETTLED,
                Some(height),
                Some(closed_at),
                Some(hash),
                Some(nonce),
                Some(settled_at),
            ) => Phase::Settled {
                block: Block {
                    height,
                    hash: hash.clone(),
                    nonce: u32::try_from(nonce).map_err(|_| corrupt())?,
                },
                closed_at,
                settled_at,
            },
            _ => return Err(corrupt()),
        };
        Ok(Round {
            id: row.id,
            title: row.title,
            created_at: row.created_at,
            phase,
        })
    }
}

impl TryFrom<BetRow> for Bet {
    type Error = StoreError;

    fn try_from(row: BetRow) -> Result<Self, StoreError> {
        let guess = u32::try_from(row.guess).map_err(|_| {
            StoreError::Corrupt(format!(
                "bet {} has guess {} out of range",
                row.id, row.guess
            ))
        })?;
        Ok(Bet {
            id: row.id,
            name: row.name,
            guess,
            created_at: row.created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn title(text: &str) -> RoundTitle {
        RoundTitle::parse(text).unwrap()
    }

    fn name(text: &str) -> BetName {
        BetName::parse(text).unwrap()
    }

    fn block(height: i32, nonce: u32) -> Block {
        Block {
            height,
            hash: format!("{:064x}", height),
            nonce,
        }
    }

    #[test]
    fn round_runs_from_open_to_settled() {
        let pool = db::test_pool();
        let conn = &mut pool.get().unwrap();

        let round = create_round(conn, &title("October")).unwrap();
        assert_eq!(round.phase, Phase::Open);
        place_bet(conn, round.id, &name("alice"), 10).unwrap();

        let closed = close_round(conn, round.id, 900_001).unwrap();
        assert!(matches!(
            closed.phase,
            Phase::Closed {
                target_height: 900_001,
                ..
            }
        ));

        let settled = settle_round(conn, round.id, &block(900_001, 7)).unwrap();
        let Phase::Settled { block: decided, .. } = settled.phase else {
            panic!("expected settled, got {:?}", settled.phase);
        };
        assert_eq!(decided, block(900_001, 7));

        // Reading back through a fresh query proves the state was persisted.
        let (stored, bets) = current_round(conn).unwrap().unwrap();
        assert!(matches!(stored.phase, Phase::Settled { .. }));
        assert_eq!(bets.iter().map(|bet| bet.guess).collect::<Vec<_>>(), [10]);
    }

    #[test]
    fn only_one_round_runs_at_a_time() {
        let pool = db::test_pool();
        let conn = &mut pool.get().unwrap();

        let first = create_round(conn, &title("first")).unwrap();
        assert!(matches!(
            create_round(conn, &title("second")),
            Err(StoreError::ActiveRoundExists)
        ));
        close_round(conn, first.id, 100).unwrap();
        assert!(matches!(
            create_round(conn, &title("second")),
            Err(StoreError::ActiveRoundExists)
        ));

        settle_round(conn, first.id, &block(100, 1)).unwrap();
        let second = create_round(conn, &title("second")).unwrap();
        assert_eq!(
            active_round(conn).unwrap().map(|round| round.id),
            Some(second.id)
        );
    }

    #[test]
    fn bets_are_rejected_once_betting_closes() {
        let pool = db::test_pool();
        let conn = &mut pool.get().unwrap();

        let round = create_round(conn, &title("October")).unwrap();
        close_round(conn, round.id, 100).unwrap();
        assert!(matches!(
            place_bet(conn, round.id, &name("late"), 1),
            Err(StoreError::BettingClosed)
        ));
        assert!(matches!(
            close_round(conn, round.id, 101),
            Err(StoreError::BettingClosed)
        ));
    }

    #[test]
    fn names_are_unique_per_round_ignoring_case() {
        let pool = db::test_pool();
        let conn = &mut pool.get().unwrap();

        let round = create_round(conn, &title("October")).unwrap();
        place_bet(conn, round.id, &name("Satoshi"), 1).unwrap();
        assert!(matches!(
            place_bet(conn, round.id, &name("satoshi"), 2),
            Err(StoreError::NameTaken(taken)) if taken == "satoshi"
        ));
        // The longest valid multi-byte name also passes the database CHECK.
        place_bet(
            conn,
            round.id,
            &name(&"₿".repeat(crate::lottery::NAME_MAX_CHARS)),
            3,
        )
        .unwrap();
    }

    #[test]
    fn only_the_target_block_settles_a_round() {
        let pool = db::test_pool();
        let conn = &mut pool.get().unwrap();

        let round = create_round(conn, &title("October")).unwrap();
        assert!(matches!(
            settle_round(conn, round.id, &block(100, 1)),
            Err(StoreError::NotAwaitingBlock)
        ));
        close_round(conn, round.id, 100).unwrap();
        assert!(matches!(
            settle_round(conn, round.id, &block(101, 1)),
            Err(StoreError::WrongBlock {
                expected: 100,
                got: 101
            })
        ));
        settle_round(conn, round.id, &block(100, 1)).unwrap();
        assert!(matches!(
            settle_round(conn, round.id, &block(100, 2)),
            Err(StoreError::AlreadySettled)
        ));
    }

    #[test]
    fn bets_can_be_removed_until_the_round_settles() {
        let pool = db::test_pool();
        let conn = &mut pool.get().unwrap();

        let round = create_round(conn, &title("October")).unwrap();
        let rude = place_bet(conn, round.id, &name("rude"), 1).unwrap();
        let kept = place_bet(conn, round.id, &name("kept"), 2).unwrap();
        delete_bet(conn, rude.id).unwrap();
        assert!(matches!(
            delete_bet(conn, rude.id),
            Err(StoreError::BetNotFound)
        ));

        close_round(conn, round.id, 100).unwrap();
        settle_round(conn, round.id, &block(100, 2)).unwrap();
        assert!(matches!(
            delete_bet(conn, kept.id),
            Err(StoreError::AlreadySettled)
        ));

        let (_, bets) = round_with_bets(conn, round.id).unwrap();
        assert_eq!(
            bets.iter().map(|bet| bet.name.as_str()).collect::<Vec<_>>(),
            ["kept"]
        );
    }

    #[test]
    fn a_round_stops_taking_bets_when_full() {
        let pool = db::test_pool();
        let conn = &mut pool.get().unwrap();

        let round = create_round(conn, &title("October")).unwrap();
        for index in 0..MAX_BETS_PER_ROUND {
            place_bet(conn, round.id, &name(&format!("bettor {index}")), 0).unwrap();
        }
        assert!(matches!(
            place_bet(conn, round.id, &name("one too many"), 0),
            Err(StoreError::RoundFull)
        ));
    }
}
