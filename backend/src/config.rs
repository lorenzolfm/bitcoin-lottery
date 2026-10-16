use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

const ADMIN_TOKEN_MIN_CHARS: usize = 12;

pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    pub admin_token: String,
    pub esplora_url: String,
    pub poll_interval: Duration,
    pub static_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let admin_token =
            env::var("ADMIN_TOKEN").map_err(|_| "ADMIN_TOKEN must be set".to_owned())?;
        if admin_token.chars().count() < ADMIN_TOKEN_MIN_CHARS {
            return Err(format!(
                "ADMIN_TOKEN must be at least {ADMIN_TOKEN_MIN_CHARS} characters"
            ));
        }
        let bind_addr = var_or("BIND_ADDR", "0.0.0.0:3000");
        let poll_seconds = var_or("POLL_SECONDS", "15");
        Ok(Self {
            database_url: var_or("DATABASE_URL", "lottery.db"),
            bind_addr: bind_addr
                .parse()
                .map_err(|_| format!("BIND_ADDR {bind_addr:?} is not a socket address"))?,
            admin_token,
            esplora_url: var_or("ESPLORA_URL", "https://mempool.space/api"),
            poll_interval: match poll_seconds.parse() {
                Ok(seconds @ 1..=3600) => Duration::from_secs(seconds),
                _ => return Err(format!("POLL_SECONDS {poll_seconds:?} must be 1 to 3600")),
            },
            static_dir: var_or("STATIC_DIR", "../frontend/build").into(),
        })
    }
}

fn var_or(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}
