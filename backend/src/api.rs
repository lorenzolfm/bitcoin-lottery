//! JSON API under `/api`, plus the built frontend for every other path.

use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequestParts, Path as UrlPath, State};
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDateTime, Utc};
use diesel::sqlite::SqliteConnection;
use serde::{Deserialize, Serialize};
use tower_http::services::{ServeDir, ServeFile};

use crate::chain::{ChainError, Esplora};
use crate::db::DbPool;
use crate::lottery::{self, Bet, BetName, Phase, Round, RoundTitle, ValidationError};
use crate::store::{self, StoreError};

pub struct AppState {
    pool: DbPool,
    pub chain: Esplora,
    admin_token: String,
    /// Last chain height seen, shown so the room can watch for the next block.
    tip_height: Mutex<Option<i32>>,
}

impl AppState {
    pub fn new(pool: DbPool, chain: Esplora, admin_token: String) -> Self {
        Self {
            pool,
            chain,
            admin_token,
            tip_height: Mutex::new(None),
        }
    }

    /// Runs blocking Diesel work off the async runtime.
    pub async fn db<T, F>(&self, work: F) -> Result<T, StoreError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, StoreError> + Send + 'static,
        T: Send + 'static,
    {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = pool.get()?;
            work(&mut conn)
        })
        .await
        .expect("database task panicked")
    }

    pub fn tip_height(&self) -> Option<i32> {
        *self.tip_height.lock().expect("tip lock poisoned")
    }

    pub fn record_tip(&self, height: i32) {
        *self.tip_height.lock().expect("tip lock poisoned") = Some(height);
    }
}

pub fn router(state: Arc<AppState>, static_dir: Option<&Path>) -> Router {
    let api = Router::new()
        .route("/rounds", get(list_rounds))
        .route("/rounds/current", get(current_round))
        .route("/rounds/{id}", get(get_round))
        .route("/rounds/{id}/bets", post(place_bet))
        .route("/admin/session", get(check_admin))
        .route("/admin/rounds", post(create_round))
        .route("/admin/rounds/{id}/close", post(close_round))
        .route("/admin/bets/{id}", delete(delete_bet))
        .fallback(|| async { ApiError::new(StatusCode::NOT_FOUND, "no such endpoint") })
        .with_state(state);
    let app = Router::new().nest("/api", api);
    match static_dir {
        // Unknown paths get the SPA shell so client-side routes survive a reload.
        Some(dir) => {
            app.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("200.html"))))
        }
        None => app,
    }
}

async fn current_round(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Option<RoundView>>, ApiError> {
    let current = state.db(store::current_round).await?;
    Ok(Json(current.map(|(round, bets)| {
        RoundView::new(&round, &bets, state.tip_height())
    })))
}

async fn get_round(
    State(state): State<Arc<AppState>>,
    UrlPath(round_id): UrlPath<i32>,
) -> Result<Json<RoundView>, ApiError> {
    let (round, bets) = state
        .db(move |conn| store::round_with_bets(conn, round_id))
        .await?;
    Ok(Json(RoundView::new(&round, &bets, state.tip_height())))
}

async fn list_rounds(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<RoundSummary>>, ApiError> {
    let history = state.db(store::history).await?;
    Ok(Json(
        history
            .iter()
            .map(|(round, bets)| RoundSummary::new(round, bets))
            .collect(),
    ))
}

#[derive(Deserialize)]
struct BetRequest {
    name: String,
    guess: i64,
}

async fn place_bet(
    State(state): State<Arc<AppState>>,
    UrlPath(round_id): UrlPath<i32>,
    request: Result<Json<BetRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<BetView>), ApiError> {
    let Json(request) = request?;
    let name = BetName::parse(&request.name)?;
    let guess = lottery::parse_guess(request.guess)?;
    let bet = state
        .db(move |conn| store::place_bet(conn, round_id, &name, guess))
        .await?;
    // The bettor sees their own guess; everyone else waits for betting to close.
    Ok((StatusCode::CREATED, Json(BetView::revealed(&bet))))
}

async fn check_admin(_: Admin) -> StatusCode {
    StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
struct CreateRoundRequest {
    title: String,
}

async fn create_round(
    _: Admin,
    State(state): State<Arc<AppState>>,
    request: Result<Json<CreateRoundRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<RoundView>), ApiError> {
    let Json(request) = request?;
    let title = RoundTitle::parse(&request.title)?;
    let round = state
        .db(move |conn| store::create_round(conn, &title))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(RoundView::new(&round, &[], state.tip_height())),
    ))
}

async fn close_round(
    _: Admin,
    State(state): State<Arc<AppState>>,
    UrlPath(round_id): UrlPath<i32>,
) -> Result<Json<RoundView>, ApiError> {
    // Fail before calling the explorer when the round cannot close anyway.
    let (round, _) = state
        .db(move |conn| store::round_with_bets(conn, round_id))
        .await?;
    if round.phase != Phase::Open {
        return Err(StoreError::BettingClosed.into());
    }
    let tip = state.chain.tip_height().await?;
    state.record_tip(tip);
    // The first block mined after betting closes decides the round, so no
    // bettor could have seen its nonce.
    let target_height = tip
        .checked_add(1)
        .ok_or_else(|| ChainError::Unexpected(format!("tip height {tip}")))?;
    let (round, bets) = state
        .db(move |conn| {
            store::close_round(conn, round_id, target_height)?;
            store::round_with_bets(conn, round_id)
        })
        .await?;
    Ok(Json(RoundView::new(&round, &bets, state.tip_height())))
}

async fn delete_bet(
    _: Admin,
    State(state): State<Arc<AppState>>,
    UrlPath(bet_id): UrlPath<i32>,
) -> Result<StatusCode, ApiError> {
    state
        .db(move |conn| store::delete_bet(conn, bet_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct RoundView {
    id: i32,
    title: String,
    status: &'static str,
    created_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    settled_at: Option<DateTime<Utc>>,
    target_height: Option<i32>,
    block: Option<BlockView>,
    tip_height: Option<i32>,
    /// Submission order while unsettled; closest first once settled.
    bets: Vec<BetView>,
}

#[derive(Serialize)]
struct BlockView {
    height: i32,
    hash: String,
    nonce: u32,
}

#[derive(Serialize)]
struct BetView {
    id: i32,
    name: String,
    /// Hidden while betting is open, so nobody can bet just beside someone else.
    guess: Option<u32>,
    distance: Option<u32>,
    rank: Option<usize>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct RoundSummary {
    id: i32,
    title: String,
    status: &'static str,
    created_at: DateTime<Utc>,
    target_height: Option<i32>,
    nonce: Option<u32>,
    bet_count: usize,
    winner: Option<String>,
}

impl RoundView {
    fn new(round: &Round, bets: &[Bet], tip_height: Option<i32>) -> Self {
        let (closed_at, settled_at, target_height, block, bets) = match &round.phase {
            Phase::Open => (
                None,
                None,
                None,
                None,
                bets.iter().map(BetView::sealed).collect(),
            ),
            Phase::Closed {
                target_height,
                closed_at,
            } => (
                Some(utc(*closed_at)),
                None,
                Some(*target_height),
                None,
                bets.iter().map(BetView::revealed).collect(),
            ),
            Phase::Settled {
                block,
                closed_at,
                settled_at,
            } => (
                Some(utc(*closed_at)),
                Some(utc(*settled_at)),
                Some(block.height),
                Some(BlockView {
                    height: block.height,
                    hash: block.hash.clone(),
                    nonce: block.nonce,
                }),
                lottery::standings(bets, block.nonce)
                    .into_iter()
                    .map(|placement| BetView {
                        rank: Some(placement.rank),
                        distance: Some(placement.distance),
                        ..BetView::revealed(placement.bet)
                    })
                    .collect(),
            ),
        };
        Self {
            id: round.id,
            title: round.title.clone(),
            status: status(&round.phase),
            created_at: utc(round.created_at),
            closed_at,
            settled_at,
            target_height,
            block,
            tip_height,
            bets,
        }
    }
}

impl BetView {
    fn sealed(bet: &Bet) -> Self {
        Self {
            id: bet.id,
            name: bet.name.clone(),
            guess: None,
            distance: None,
            rank: None,
            created_at: utc(bet.created_at),
        }
    }

    fn revealed(bet: &Bet) -> Self {
        Self {
            guess: Some(bet.guess),
            ..Self::sealed(bet)
        }
    }
}

impl RoundSummary {
    fn new(round: &Round, bets: &[Bet]) -> Self {
        let (target_height, nonce, winner) = match &round.phase {
            Phase::Open => (None, None, None),
            Phase::Closed { target_height, .. } => (Some(*target_height), None, None),
            Phase::Settled { block, .. } => (
                Some(block.height),
                Some(block.nonce),
                lottery::standings(bets, block.nonce)
                    .first()
                    .map(|placement| placement.bet.name.clone()),
            ),
        };
        Self {
            id: round.id,
            title: round.title.clone(),
            status: status(&round.phase),
            created_at: utc(round.created_at),
            target_height,
            nonce,
            bet_count: bets.len(),
            winner,
        }
    }
}

fn status(phase: &Phase) -> &'static str {
    match phase {
        Phase::Open => "open",
        Phase::Closed { .. } => "closed",
        Phase::Settled { .. } => "settled",
    }
}

/// SQLite stores naive UTC timestamps; mark them as UTC on the wire.
fn utc(timestamp: NaiveDateTime) -> DateTime<Utc> {
    timestamp.and_utc()
}

/// Proof that the request carried the admin token as a bearer token.
struct Admin;

impl FromRequestParts<Arc<AppState>> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, ApiError> {
        let presented = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        match presented {
            Some(token) if constant_time_eq(token.as_bytes(), state.admin_token.as_bytes()) => {
                Ok(Admin)
            }
            _ => Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "admin token required",
            )),
        }
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        let status = match &error {
            StoreError::RoundNotFound | StoreError::BetNotFound => StatusCode::NOT_FOUND,
            StoreError::ActiveRoundExists
            | StoreError::BettingClosed
            | StoreError::NotAwaitingBlock
            | StoreError::WrongBlock { .. }
            | StoreError::AlreadySettled
            | StoreError::NameTaken(_)
            | StoreError::RoundFull => StatusCode::CONFLICT,
            StoreError::Corrupt(_) | StoreError::Database(_) | StoreError::Pool(_) => {
                tracing::error!(%error, "database failure");
                return Self::new(StatusCode::INTERNAL_SERVER_ERROR, "database error");
            }
        };
        Self::new(status, error.to_string())
    }
}

impl From<ValidationError> for ApiError {
    fn from(error: ValidationError) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
    }
}

impl From<ChainError> for ApiError {
    fn from(error: ChainError) -> Self {
        tracing::warn!(%error, "block explorer failure");
        Self::new(StatusCode::BAD_GATEWAY, error.to_string())
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::new(rejection.status(), rejection.body_text())
    }
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::testing::{ADMIN_TOKEN, FakeExplorer};

    async fn send(
        app: &Router,
        method: Method,
        uri: &str,
        auth: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(auth) = auth {
            request = request.header(AUTHORIZATION, auth);
        }
        let request = match body {
            Some(body) => request
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        };
        let response = app.clone().oneshot(request.unwrap()).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    const BEARER: Option<&str> = Some("Bearer test-admin-token");

    #[tokio::test]
    async fn guesses_stay_hidden_until_betting_closes() {
        let explorer = FakeExplorer::start(900_000).await;
        let app = router(Arc::new(explorer.app_state()), None);

        let (status, round) = send(
            &app,
            Method::POST,
            "/api/admin/rounds",
            BEARER,
            Some(json!({ "title": "October" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let bets_uri = format!("/api/rounds/{}/bets", round["id"]);

        let (status, bet) = send(
            &app,
            Method::POST,
            &bets_uri,
            None,
            Some(json!({ "name": "alice", "guess": 123 })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(bet["guess"], 123, "the bettor gets their own guess back");

        let (_, open) = send(&app, Method::GET, "/api/rounds/current", None, None).await;
        assert_eq!(open["bets"][0]["name"], "alice");
        assert_eq!(
            open["bets"][0]["guess"],
            Value::Null,
            "nobody else sees it while open"
        );

        let close_uri = format!("/api/admin/rounds/{}/close", round["id"]);
        let (status, closed) = send(&app, Method::POST, &close_uri, BEARER, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            closed["target_height"], 900_001,
            "the next block after closing decides"
        );
        assert_eq!(closed["bets"][0]["guess"], 123);
    }

    #[tokio::test]
    async fn admin_routes_require_the_token() {
        let explorer = FakeExplorer::start(900_000).await;
        let app = router(Arc::new(explorer.app_state()), None);
        let body = || Some(json!({ "title": "October" }));

        for auth in [None, Some("Bearer wrong-token-value"), Some(ADMIN_TOKEN)] {
            let (status, _) = send(&app, Method::POST, "/api/admin/rounds", auth, body()).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "auth header {auth:?}");
        }
        let (_, current) = send(&app, Method::GET, "/api/rounds/current", None, None).await;
        assert_eq!(current, Value::Null, "rejected requests created nothing");

        let (status, _) = send(&app, Method::POST, "/api/admin/rounds", BEARER, body()).await;
        assert_eq!(status, StatusCode::CREATED);
    }
}
