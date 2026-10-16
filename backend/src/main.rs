mod api;
mod chain;
mod config;
mod db;
mod lottery;
mod schema;
mod settler;
mod store;
#[cfg(test)]
mod testing;

use std::sync::Arc;

use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = config::Config::from_env()?;
    let pool = db::connect(&config.database_url)?;
    let chain = chain::Esplora::new(&config.esplora_url)?;
    let state = Arc::new(api::AppState::new(pool, chain, config.admin_token));

    tokio::spawn(settler::run(state.clone(), config.poll_interval));

    let static_dir = config
        .static_dir
        .is_dir()
        .then_some(config.static_dir.as_path());
    if static_dir.is_none() {
        tracing::warn!(dir = %config.static_dir.display(), "frontend build not found; serving the API only");
    }
    let app = api::router(state, static_dir).layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, explorer = %config.esplora_url, "lottery is up");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    Ok(())
}
