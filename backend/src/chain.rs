//! Reads the chain through an Esplora HTTP API, which mempool.space and
//! Blockstream both serve. A self-hosted mempool instance works too.

use std::time::Duration;

use serde::Deserialize;

use crate::lottery::Block;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    #[error("block explorer request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("block explorer sent an unexpected response: {0}")]
    Unexpected(String),
}

#[derive(Debug, Clone)]
pub struct Esplora {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Deserialize)]
struct BlockResponse {
    id: String,
    height: i32,
    nonce: u32,
}

impl Esplora {
    pub fn new(base_url: &str) -> Result<Self, ChainError> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("bitdevs-lottery/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }

    pub async fn tip_height(&self) -> Result<i32, ChainError> {
        let body = self.get_text("/blocks/tip/height").await?;
        match body.trim().parse::<i32>() {
            Ok(height) if height > 0 => Ok(height),
            _ => Err(ChainError::Unexpected(format!("tip height {body:?}"))),
        }
    }

    /// The block at `height` on the explorer's best chain.
    pub async fn block_at(&self, height: i32) -> Result<Block, ChainError> {
        let body = self.get_text(&format!("/block-height/{height}")).await?;
        let hash = body.trim();
        if !is_block_hash(hash) {
            return Err(ChainError::Unexpected(format!("block hash {body:?}")));
        }
        let block: BlockResponse = self
            .http
            .get(format!("{}/block/{hash}", self.base_url))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if block.id != hash || block.height != height {
            return Err(ChainError::Unexpected(format!(
                "asked for block {hash} at height {height}, got {} at height {}",
                block.id, block.height
            )));
        }
        Ok(Block {
            height,
            hash: block.id,
            nonce: block.nonce,
        })
    }

    async fn get_text(&self, path: &str) -> Result<String, ChainError> {
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .send()
            .await?
            .error_for_status()?;
        Ok(response.text().await?)
    }
}

fn is_block_hash(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}
