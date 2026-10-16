//! Background loop that watches the chain and settles the closed round as
//! soon as its target block exists.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::MissedTickBehavior;

use crate::api::AppState;
use crate::chain::ChainError;
use crate::lottery::Phase;
use crate::store::{self, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum SettleError {
    #[error(transparent)]
    Chain(#[from] ChainError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

pub async fn run(state: Arc<AppState>, every: Duration) {
    let mut ticker = tokio::time::interval(every);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        if let Err(error) = tick(&state).await {
            tracing::warn!(%error, "settlement check failed; retrying next tick");
        }
    }
}

/// One pass: refresh the tip and settle the active round if its block is in.
/// The explorer is only polled while a round is running.
pub async fn tick(state: &AppState) -> Result<(), SettleError> {
    let Some(round) = state.db(store::active_round).await? else {
        return Ok(());
    };
    let tip = state.chain.tip_height().await?;
    state.record_tip(tip);
    let Phase::Closed { target_height, .. } = round.phase else {
        return Ok(());
    };
    if tip < target_height {
        return Ok(());
    }
    let block = state.chain.block_at(target_height).await?;
    let nonce = block.nonce;
    state
        .db(move |conn| store::settle_round(conn, round.id, &block))
        .await?;
    tracing::info!(
        round = round.id,
        height = target_height,
        nonce,
        "round settled"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lottery::{BetName, Block, RoundTitle};
    use crate::testing::{FakeExplorer, block_hash};

    #[tokio::test]
    async fn settles_with_the_first_block_after_betting_closed() {
        let explorer = FakeExplorer::start(900_000).await;
        let state = explorer.app_state();
        let round = state
            .db(|conn| {
                let round = store::create_round(conn, &RoundTitle::parse("October").unwrap())?;
                store::place_bet(conn, round.id, &BetName::parse("alice").unwrap(), 500)?;
                store::close_round(conn, round.id, 900_001)
            })
            .await
            .unwrap();

        tick(&state).await.unwrap();
        let (waiting, _) = state
            .db(move |conn| store::round_with_bets(conn, round.id))
            .await
            .unwrap();
        assert!(
            matches!(waiting.phase, Phase::Closed { .. }),
            "target block not mined yet"
        );
        assert_eq!(state.tip_height(), Some(900_000));

        // Two blocks land between polls; the earlier one decides the round.
        explorer.mine(498);
        explorer.mine(7);
        tick(&state).await.unwrap();
        let (settled, _) = state
            .db(move |conn| store::round_with_bets(conn, round.id))
            .await
            .unwrap();
        let Phase::Settled { block, .. } = settled.phase else {
            panic!("expected settled, got {:?}", settled.phase);
        };
        assert_eq!(
            block,
            Block {
                height: 900_001,
                hash: block_hash(900_001),
                nonce: 498
            }
        );
    }
}
