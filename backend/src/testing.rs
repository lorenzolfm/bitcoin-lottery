//! Test support: an in-process Esplora server with a chain the test can mine.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::api::AppState;
use crate::chain::Esplora;
use crate::db;

pub const ADMIN_TOKEN: &str = "test-admin-token";

/// Nonce by height. Hashes are the height in hex, so they round-trip.
type Chain = Arc<Mutex<BTreeMap<i32, u32>>>;

pub struct FakeExplorer {
    url: String,
    chain: Chain,
}

impl FakeExplorer {
    pub async fn start(tip_height: i32) -> Self {
        let chain = Chain::default();
        chain.lock().unwrap().insert(tip_height, 0);
        let app = Router::new()
            .route("/blocks/tip/height", get(tip_height_route))
            .route("/block-height/{height}", get(block_hash_route))
            .route("/block/{hash}", get(block_route))
            .with_state(chain.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { url, chain }
    }

    /// Adds a block on top of the tip and returns its height.
    pub fn mine(&self, nonce: u32) -> i32 {
        let mut chain = self.chain.lock().unwrap();
        let height = chain.last_key_value().unwrap().0 + 1;
        chain.insert(height, nonce);
        height
    }

    pub fn app_state(&self) -> AppState {
        AppState::new(
            db::test_pool(),
            Esplora::new(&self.url).unwrap(),
            ADMIN_TOKEN.to_owned(),
        )
    }
}

pub fn block_hash(height: i32) -> String {
    format!("{height:064x}")
}

async fn tip_height_route(State(chain): State<Chain>) -> String {
    chain
        .lock()
        .unwrap()
        .last_key_value()
        .unwrap()
        .0
        .to_string()
}

async fn block_hash_route(
    State(chain): State<Chain>,
    Path(height): Path<i32>,
) -> Result<String, StatusCode> {
    match chain.lock().unwrap().contains_key(&height) {
        true => Ok(block_hash(height)),
        false => Err(StatusCode::NOT_FOUND),
    }
}

/// Same shape as mempool.space's `/api/block/:hash`, extra fields included.
async fn block_route(
    State(chain): State<Chain>,
    Path(hash): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let height = i32::from_str_radix(&hash, 16).map_err(|_| StatusCode::BAD_REQUEST)?;
    let nonce = *chain
        .lock()
        .unwrap()
        .get(&height)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(json!({
        "id": hash,
        "height": height,
        "version": 537174016,
        "timestamp": 1792144580,
        "tx_count": 5944,
        "size": 1572095,
        "weight": 3993809,
        "merkle_root": "ebd317455acdf46adbd2b2616e57f6933a0f2e2e324dd9afb2d2dbbd760e4ad8",
        "previousblockhash": block_hash(height - 1),
        "mediantime": 1792142776,
        "nonce": nonce,
        "bits": 386014960,
        "difficulty": 132716002350731.27
    })))
}
