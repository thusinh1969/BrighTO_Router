//! B3 — Admin API + portal 1 trang HTML tĩnh.
//! Auth: master key từ env + IP allowlist. Key plaintext trả đúng 1 lần.

use std::{
    fmt::Write as _,
    io::Read,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::{ConnectInfo, Extension, Path, Query},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config::DbConfigLoader;
use crate::contract::{AppState, Budget, KeyHash};

// ===== Admin state =====

#[derive(Clone)]
struct AdminState {
    db_url: String,
    master_key: String,
    allow_cidrs: Vec<String>,
    pool: Arc<tokio::sync::OnceCell<PgPool>>,
    runtime: Arc<AppState>,
}

impl AdminState {
    fn from_env(runtime: Arc<AppState>) -> Self {
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required for admin API");
        // Fail-fast: thiếu/trống ADMIN_MASTER_KEY -> process không được boot với admin secret rỗng.
        let master_key = validate_master_key(
            std::env::var("ADMIN_MASTER_KEY").expect("ADMIN_MASTER_KEY is required for admin API"),
        );
        let allow_cidrs = std::env::var("ADMIN_ALLOW_CIDR")
            .unwrap_or_else(|_| "127.0.0.1/32".to_string())
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();

        Self {
            db_url,
            master_key,
            allow_cidrs,
            pool: Arc::new(Default::default()),
            runtime,
        }
    }

    async fn pool(&self) -> Result<&PgPool, ApiError> {
        self.pool
            .get_or_try_init(|| async {
                PgPoolOptions::new()
                    .max_connections(5)
                    .connect(&self.db_url)
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))
            })
            .await
    }

    /// Reload config snapshot vào runtime state (cfg/budget/backends) đồng bộ, trước khi trả
    /// 200/204 cho mutation. Chỉ chạy trên admin path — KHÔNG phải proxy hot path.
    async fn reload_now(&self) -> Result<(), ApiError> {
        let pool = self.pool().await?;
        let loader = DbConfigLoader::new(pool.clone(), 0);
        let snap = loader.load_snapshot().await.map_err(|e| {
            ApiError::internal(format!("reload config after admin mutation: {e:#}"))
        })?;

        self.runtime.backends.sync_backends(&snap.backends);
        self.runtime.budget.sync_teams(&snap.teams);
        self.runtime.cfg.store(Arc::new(snap));
        self.runtime.reload_notify.notify_one();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.runtime
            .config_ok_at
            .store(now, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

// ===== API error =====

#[derive(Debug)]
struct ApiError {
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

    fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::internal(e.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::internal(e.to_string())
    }
}

// ===== Auth helpers =====

fn parse_cidr(s: &str) -> Option<(IpAddr, u8)> {
    let (addr, prefix) = s.split_once('/')?;
    let ip: IpAddr = addr.parse().ok()?;
    let prefix: u8 = prefix.parse().ok()?;

    match ip {
        IpAddr::V4(v4) if prefix <= 32 => Some((IpAddr::V4(v4), prefix)),
        IpAddr::V6(v6) if prefix <= 128 => Some((IpAddr::V6(v6), prefix)),
        _ => None,
    }
}

fn ip_matches_cidr(ip: IpAddr, cidr: &str) -> bool {
    let (network, prefix) = match parse_cidr(cidr) {
        Some(x) => x,
        None => return false,
    };

    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(net)) => {
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            (u32::from_be_bytes(ip.octets()) & mask) == (u32::from_be_bytes(net.octets()) & mask)
        }
        (IpAddr::V6(ip), IpAddr::V6(net)) => {
            let mask = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix)
            };
            (u128::from_be_bytes(ip.octets()) & mask) == (u128::from_be_bytes(net.octets()) & mask)
        }
        _ => false,
    }
}

/// Chặn admin secret rỗng ngay lúc khởi tạo (footgun production). Thuần tuý để unit-test không cần env.
fn validate_master_key(value: String) -> String {
    assert!(
        !value.trim().is_empty(),
        "ADMIN_MASTER_KEY must not be empty"
    );
    value
}

fn check_admin_auth(
    master_key: &str,
    allow_cidrs: &[String],
    headers: &HeaderMap,
    peer_ip: IpAddr,
) -> Result<(), ApiError> {
    let provided = headers
        .get("x-admin-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or_else(|| {
            headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer ").map(str::to_owned))
        });

    if provided.as_deref() != Some(master_key) {
        return Err(ApiError::unauthorized("invalid admin key"));
    }

    // Nguồn IP duy nhất đáng tin = socket peer addr (chống spoof X-Forwarded-For).
    if !allow_cidrs
        .iter()
        .any(|cidr| ip_matches_cidr(peer_ip, cidr))
    {
        return Err(ApiError::forbidden("ip not allowed"));
    }

    Ok(())
}

// ===== Key helpers =====

fn generate_key() -> Result<String, ApiError> {
    let mut bytes = [0u8; 16];
    let f = std::fs::File::open("/dev/urandom").map_err(|e| ApiError::internal(e.to_string()))?;
    f.take(16)
        .read_exact(&mut bytes)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(format!("lc-{}", hex_encode(&bytes)))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(&mut s, "{:02x}", b).expect("write into String cannot fail");
    }
    s
}

// ===== Router =====

/// Router con cho /admin/* — được nest_service vào router chính.
pub fn router(runtime: Arc<AppState>) -> Router {
    let state = Arc::new(AdminState::from_env(runtime));

    Router::new()
        .route("/", get(portal))
        .route("/teams", post(create_team))
        .route("/keys", post(create_key))
        .route("/teams/{id}", patch(update_team))
        .route("/keys/{id}", delete(disable_key))
        .route("/usage", get(get_usage))
        .layer(Extension(state))
}

async fn portal() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../../static/index.html"),
    )
}

// ===== Request/response types =====

#[derive(Deserialize)]
struct CreateTeam {
    name: String,
    budget: Option<Budget>,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Serialize)]
struct TeamResponse {
    id: i64,
    name: String,
    budget: Option<Budget>,
    enabled: bool,
}

#[derive(Deserialize)]
struct CreateKey {
    team_id: i64,
    owner: String,
    #[serde(default)]
    allowed_models: Vec<String>,
    budget: Option<Budget>,
    rpm_limit: Option<u32>,
    concurrency_limit: Option<u32>,
    expires_at: Option<i64>,
}

#[derive(Serialize)]
struct KeyResponse {
    id: i64,
    key: String,
    prefix: String,
}

#[derive(Deserialize)]
struct PatchTeam {
    name: Option<String>,
    budget: Option<Option<Budget>>,
    enabled: Option<bool>,
}

#[derive(Deserialize)]
struct UsageQuery {
    team: Option<i64>,
    from: Option<i64>,
    to: Option<i64>,
}

#[derive(Serialize)]
struct UsageRow {
    ts: i64,
    request_id: String,
    key_id: i64,
    team_id: i64,
    model: String,
    backend_id: i64,
    status: i64,
    input_tokens: i64,
    output_tokens: i64,
    estimated: bool,
    ttfb_ms: i64,
    total_ms: i64,
    router_overhead_ms: i64,
    stream: bool,
    client_aborted: bool,
    error_class: Option<String>,
}

// ===== Handlers =====

async fn create_team(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<CreateTeam>,
) -> Result<Json<TeamResponse>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;

    let budget_json = payload
        .budget
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    let row = sqlx::query::<sqlx::Postgres>(
        "INSERT INTO teams (name, budget, enabled) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(&payload.name)
    .bind(budget_json)
    .bind(payload.enabled)
    .fetch_one(pool)
    .await?;

    let id: i64 = row.try_get("id")?;

    state.reload_now().await?;
    Ok(Json(TeamResponse {
        id,
        name: payload.name,
        budget: payload.budget,
        enabled: payload.enabled,
    }))
}

async fn create_key(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<CreateKey>,
) -> Result<Json<KeyResponse>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;

    let key = generate_key()?;
    let hash_bytes = Sha256::digest(key.as_bytes());
    let mut hash: KeyHash = [0u8; 32];
    hash.copy_from_slice(&hash_bytes);

    let prefix: String = key.chars().take(8).collect();
    let allowed_json = serde_json::to_string(&payload.allowed_models)?;
    let budget_json = payload
        .budget
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    let row = sqlx::query::<sqlx::Postgres>(
        "INSERT INTO api_keys \
         (key_hash, key_prefix, team_id, owner, allowed_models, budget, rpm_limit, concurrency_limit, expires_at, enabled) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(hex_encode(&hash))
    .bind(&prefix)
    .bind(payload.team_id)
    .bind(&payload.owner)
    .bind(allowed_json)
    .bind(budget_json)
    .bind(payload.rpm_limit.map(|v| v as i64))
    .bind(payload.concurrency_limit.map(|v| v as i64))
    .bind(payload.expires_at)
    .bind(true)
    .fetch_one(pool)
    .await?;

    let id: i64 = row.try_get("id")?;

    if let Err(e) = state.reload_now().await {
        // Tạo key đã sinh plaintext secret; reload fail -> disable key để không tạo key mồ côi.
        let _ = sqlx::query::<sqlx::Postgres>("UPDATE api_keys SET enabled = false WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await;
        return Err(e);
    }
    Ok(Json(KeyResponse { id, key, prefix }))
}

async fn update_team(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<PatchTeam>,
) -> Result<Json<serde_json::Value>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;

    // Dựng SET clause thủ công (không dùng Separated — nó chèn ", " trước bind, tạo SQL sai).
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE teams SET ");
    let mut first = true;

    if let Some(name) = payload.name {
        if !first {
            builder.push(", ");
        }
        builder.push("name = ").push_bind(name);
        first = false;
    }

    if let Some(budget) = payload.budget {
        if !first {
            builder.push(", ");
        }
        match budget {
            Some(b) => {
                let json = serde_json::to_string(&b)?;
                builder.push("budget = ").push_bind(json);
            }
            None => {
                builder.push("budget = NULL");
            }
        }
        first = false;
    }

    if let Some(enabled) = payload.enabled {
        if !first {
            builder.push(", ");
        }
        builder.push("enabled = ").push_bind(enabled);
        first = false;
    }

    if first {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "no fields to update",
        ));
    }

    builder.push(" WHERE id = ").push_bind(id);
    let query = builder.build();
    let result = query.execute(pool).await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("team not found"));
    }

    state.reload_now().await?;
    Ok(Json(json!({ "id": id })))
}

async fn disable_key(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;

    let result = sqlx::query::<sqlx::Postgres>("UPDATE api_keys SET enabled = false WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("key not found"));
    }

    state.reload_now().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_usage(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(params): Query<UsageQuery>,
) -> Result<Json<Vec<UsageRow>>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;

    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT ts, request_id, key_id, team_id, model, backend_id, status, \
         input_tokens, output_tokens, estimated, ttfb_ms, total_ms, router_overhead_ms, \
         stream, client_aborted, error_class \
         FROM usage_ledger WHERE 1=1",
    );

    if let Some(team) = params.team {
        builder.push(" AND team_id = ").push_bind(team);
    }
    if let Some(from) = params.from {
        builder.push(" AND ts >= ").push_bind(from);
    }
    if let Some(to) = params.to {
        builder.push(" AND ts <= ").push_bind(to);
    }

    builder.push(" ORDER BY ts DESC LIMIT 1000");
    let query = builder.build();
    let rows = query.fetch_all(pool).await?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        result.push(UsageRow {
            ts: row.try_get("ts")?,
            request_id: row.try_get("request_id")?,
            key_id: row.try_get("key_id")?,
            team_id: row.try_get("team_id")?,
            model: row.try_get("model")?,
            backend_id: row.try_get("backend_id")?,
            status: row.try_get("status")?,
            input_tokens: row.try_get("input_tokens")?,
            output_tokens: row.try_get("output_tokens")?,
            estimated: row.try_get("estimated")?,
            ttfb_ms: row.try_get("ttfb_ms")?,
            total_ms: row.try_get("total_ms")?,
            router_overhead_ms: row.try_get("router_overhead_ms")?,
            stream: row.try_get("stream")?,
            client_aborted: row.try_get("client_aborted")?,
            error_class: row.try_get("error_class")?,
        });
    }

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_cidr_checks() {
        assert!(ip_matches_cidr(
            "127.0.0.1".parse().unwrap(),
            "127.0.0.1/32"
        ));
        assert!(!ip_matches_cidr(
            "127.0.0.2".parse().unwrap(),
            "127.0.0.1/32"
        ));
        assert!(ip_matches_cidr("10.0.0.5".parse().unwrap(), "10.0.0.0/8"));
        assert!(ip_matches_cidr("::1".parse().unwrap(), "::1/128"));
    }

    #[test]
    fn generated_key_format() {
        let key = generate_key().expect("generate key");
        assert!(key.starts_with("lc-"));
        assert_eq!(key.len(), 3 + 32);
        assert!(key[3..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_prefix_length() {
        let key = "lc-0123456789abcdef0123456789abcdef".to_string();
        let hash_bytes = Sha256::digest(key.as_bytes());
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&hash_bytes);
        assert_eq!(hash.len(), 32);

        let prefix: String = key.chars().take(8).collect();
        assert_eq!(prefix.len(), 8);
    }

    #[test]
    fn master_key_validation_rejects_empty() {
        assert_eq!(validate_master_key("secret".into()), "secret");
        // whitespace-only phải bị chặn (footgun: key trông có vẻ set nhưng thực ra rỗng).
        let r = std::panic::catch_unwind(|| validate_master_key("   ".into()));
        assert!(r.is_err(), "empty/whitespace master key must panic");
    }

    #[test]
    fn admin_auth_rejects_bad_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-admin-key", "secret".parse().unwrap());

        let err = check_admin_auth(
            "secret",
            &["10.0.0.0/8".to_string()],
            &headers,
            "1.2.3.4".parse().unwrap(),
        )
        .unwrap_err();
        assert_eq!(err.status, StatusCode::FORBIDDEN);
    }
}
