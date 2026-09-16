//! B3 — Admin API + portal 1 trang HTML tĩnh.
//! Auth: master key từ env + IP allowlist. Admin có thể xem lại client key qua /admin/keys/{id}/reveal.

use std::{
    fmt::Write as _,
    io::Read,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
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

use crate::auth;
use crate::config::{DbConfigLoader, resolve_backend_key};
use crate::contract::{ApiKey, AppState, Budget, KeyHash};

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
        .route("/backends", get(list_backends).post(create_backend))
        .route("/backends/{id}", patch(update_backend))
        .route("/backends/{id}/models", get(fetch_backend_models))
        .route("/routes", get(list_routes).post(upsert_route))
        .route("/teams/{id}", patch(update_team))
        .route("/keys/{id}", delete(disable_key))
        .route("/keys/{id}/reveal", get(reveal_key))
        .route("/teams", get(list_teams))
        .route("/keys", get(list_keys))
        .route("/stats", get(get_stats))
        .route("/usage", get(get_usage))
        .route("/settings", get(get_settings))
        .layer(Extension(state))
}

/// Router con cho /portal/* — user tự phục vụ (auth bằng API key, KHÔNG phải admin key).
/// User xem key/team của mình + usage/charts/logs. Không lộ plaintext key.
pub fn user_router(runtime: Arc<AppState>) -> Router {
    let state = Arc::new(AdminState::from_env(runtime));
    Router::new()
        .route("/me", get(me))
        .route("/me/usage", get(me_usage))
        .route("/me/stats", get(me_stats))
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

#[derive(Serialize)]
struct BackendResponse {
    id: i64,
    name: String,
    base_url: String,
    api_key_ref: String,
    key_resolved: bool,
    weight: i64,
    max_inflight: i64,
    format: String,
    enabled: bool,
}

#[derive(Deserialize)]
struct PatchBackend {
    name: Option<String>,
    base_url: Option<String>,
    api_key_ref: Option<String>,
    weight: Option<u32>,
    max_inflight: Option<u32>,
    format: Option<String>,
    enabled: Option<bool>,
}

#[derive(Deserialize)]
struct CreateBackend {
    name: String,
    base_url: String,
    api_key_ref: String,
    format: String,
    weight: Option<u32>,
    max_inflight: Option<u32>,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

#[derive(Serialize)]
struct ModelListResponse {
    backend_id: i64,
    backend_name: String,
    models: Vec<String>,
}

#[derive(Deserialize)]
struct UpsertRoute {
    model_name: String,
    backend_ids: Vec<i64>,
    fallback_backend_id: Option<i64>,
    chars_per_token: Option<f64>,
    first_byte_timeout: Option<u64>,
    #[serde(default)]
    provider_model_name: Option<String>,
    context_tokens: Option<i64>,
    max_output_tokens: Option<i64>,
    price_input_per_mtok_usd: Option<f64>,
    price_output_per_mtok_usd: Option<f64>,
    enabled: Option<bool>,
}

#[derive(Serialize)]
struct RouteResponse {
    model_name: String,
    backend_ids: Vec<i64>,
    fallback_backend_id: Option<i64>,
    chars_per_token: f64,
    first_byte_timeout: u64,
    provider_model_name: String,
    context_tokens: Option<i64>,
    max_output_tokens: Option<i64>,
    price_input_per_mtok_usd: Option<f64>,
    price_output_per_mtok_usd: Option<f64>,
    enabled: bool,
}

#[derive(Deserialize)]
struct UsageQuery {
    team: Option<i64>,
    key: Option<i64>,
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

#[derive(Serialize)]
struct TeamListRow {
    id: i64,
    name: String,
    budget: Option<Budget>,
    enabled: bool,
}

#[derive(Serialize)]
struct KeyListRow {
    id: i64,
    prefix: String,
    team_id: i64,
    team_name: String,
    owner: String,
    allowed_models: Vec<String>,
    budget: Option<Budget>,
    rpm_limit: Option<i64>,
    concurrency_limit: Option<i64>,
    expires_at: Option<i64>,
    enabled: bool,
}

#[derive(Serialize)]
struct StatRow {
    /// epoch giây đầu ngày (bucket 1 ngày)
    day: i64,
    model: String,
    input_tokens: i64,
    output_tokens: i64,
    requests: i64,
}

#[derive(Deserialize)]
struct StatsQuery {
    team: Option<i64>,
    days: Option<u32>,
}

#[derive(Deserialize)]
struct MeStatsQuery {
    days: Option<u32>,
}

#[derive(Serialize)]
struct MeResponse {
    key: MeKey,
    team: MeTeam,
}

#[derive(Serialize)]
struct MeKey {
    id: i64,
    prefix: String,
    owner: String,
    allowed_models: Vec<String>,
    budget: Option<Budget>,
    rpm_limit: Option<u32>,
    concurrency_limit: Option<u32>,
    expires_at: Option<i64>,
    enabled: bool,
}

#[derive(Serialize)]
struct MeTeam {
    id: i64,
    name: String,
    budget: Option<Budget>,
    enabled: bool,
}

#[derive(Serialize)]
struct KeyRevealResponse {
    id: i64,
    prefix: String,
    owner: String,
    key: String,
}

#[derive(Serialize)]
struct SettingsResponse {
    listen_addr: String,
    database_ok: bool,
    config_reload_ok: bool,
    max_body_bytes: usize,
    version: String,
}

// ===== Handlers =====

fn normalize_backend_format(value: &str) -> Result<&'static str, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" | "open_ai" => Ok("openai"),
        "anthropic" => Ok("anthropic"),
        _ => Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "format must be openai or anthropic",
        )),
    }
}

fn non_empty_trimmed(value: String, field: &str) -> Result<String, ApiError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("{field} must not be empty"),
        ));
    }
    Ok(trimmed.to_owned())
}

fn join_provider_url(base_url: &str, route: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    let has_path_prefix = trimmed
        .split_once("://")
        .and_then(|(_, rest)| rest.split_once('/'))
        .is_some();
    let path = if has_path_prefix {
        route.strip_prefix("/v1").unwrap_or(route)
    } else {
        route
    };
    if path.starts_with('/') {
        format!("{trimmed}{path}")
    } else {
        format!("{trimmed}/{path}")
    }
}

fn parse_backend_ids(value: &str) -> Result<Vec<i64>, ApiError> {
    serde_json::from_str::<Vec<i64>>(value).map_err(|_| {
        ApiError::internal(format!("invalid backend_ids JSON in model_routes: {value}"))
    })
}

async fn list_routes_from_pool(pool: &PgPool) -> Result<Vec<RouteResponse>, ApiError> {
    let rows = sqlx::query::<sqlx::Postgres>(
        "SELECT model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, \
         provider_model_name, context_tokens, max_output_tokens, \
         price_input_per_mtok_usd, price_output_per_mtok_usd, enabled \
         FROM model_routes ORDER BY model_name",
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let backend_ids_json: String = row.try_get("backend_ids")?;
        out.push(RouteResponse {
            model_name: row.try_get("model_name")?,
            backend_ids: parse_backend_ids(&backend_ids_json)?,
            fallback_backend_id: row.try_get("fallback_backend_id")?,
            chars_per_token: row.try_get("chars_per_token")?,
            first_byte_timeout: row.try_get::<i64, _>("first_byte_timeout")? as u64,
            provider_model_name: row.try_get("provider_model_name")?,
            context_tokens: row.try_get("context_tokens")?,
            max_output_tokens: row.try_get("max_output_tokens")?,
            price_input_per_mtok_usd: row.try_get("price_input_per_mtok_usd")?,
            price_output_per_mtok_usd: row.try_get("price_output_per_mtok_usd")?,
            enabled: row.try_get("enabled")?,
        });
    }
    Ok(out)
}

async fn create_backend(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<CreateBackend>,
) -> Result<Json<BackendResponse>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let name = non_empty_trimmed(payload.name, "name")?;
    let base_url = non_empty_trimmed(payload.base_url, "base_url")?;
    let api_key_ref = non_empty_trimmed(payload.api_key_ref, "api_key_ref")?;
    let format = normalize_backend_format(&payload.format)?;
    let weight = payload.weight.unwrap_or(1).max(1);
    let max_inflight = payload.max_inflight.unwrap_or(0);
    let pool = state.pool().await?;
    let row = sqlx::query::<sqlx::Postgres>(
        "INSERT INTO backends (name, base_url, api_key_ref, weight, max_inflight, format, enabled) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(&name)
    .bind(&base_url)
    .bind(&api_key_ref)
    .bind(weight as i64)
    .bind(max_inflight as i64)
    .bind(format)
    .bind(payload.enabled)
    .fetch_one(pool)
    .await?;
    let id: i64 = row.try_get("id")?;
    state.reload_now().await?;
    Ok(Json(BackendResponse {
        id,
        name,
        base_url,
        key_resolved: resolve_backend_key(&api_key_ref)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false),
        api_key_ref,
        weight: weight as i64,
        max_inflight: max_inflight as i64,
        format: format.to_string(),
        enabled: payload.enabled,
    }))
}

async fn list_backends(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<Vec<BackendResponse>>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;
    let rows = sqlx::query::<sqlx::Postgres>(
        "SELECT id, name, base_url, api_key_ref, weight, max_inflight, format, enabled FROM backends ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let api_key_ref: String = row.try_get("api_key_ref")?;
        out.push(BackendResponse {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            base_url: row.try_get("base_url")?,
            key_resolved: resolve_backend_key(&api_key_ref)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false),
            api_key_ref,
            weight: row.try_get("weight")?,
            max_inflight: row.try_get("max_inflight")?,
            format: row.try_get("format")?,
            enabled: row.try_get("enabled")?,
        });
    }
    Ok(Json(out))
}

async fn update_backend(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<PatchBackend>,
) -> Result<Json<serde_json::Value>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;

    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE backends SET ");
    let mut first = true;

    if let Some(name) = payload.name {
        if !first {
            builder.push(", ");
        }
        builder
            .push("name = ")
            .push_bind(non_empty_trimmed(name, "name")?);
        first = false;
    }
    if let Some(base_url) = payload.base_url {
        if !first {
            builder.push(", ");
        }
        builder
            .push("base_url = ")
            .push_bind(non_empty_trimmed(base_url, "base_url")?);
        first = false;
    }
    if let Some(api_key_ref) = payload.api_key_ref {
        if !first {
            builder.push(", ");
        }
        builder
            .push("api_key_ref = ")
            .push_bind(non_empty_trimmed(api_key_ref, "api_key_ref")?);
        first = false;
    }
    if let Some(weight) = payload.weight {
        if weight == 0 {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "weight must be at least 1",
            ));
        }
        if !first {
            builder.push(", ");
        }
        builder.push("weight = ").push_bind(weight as i64);
        first = false;
    }
    if let Some(max_inflight) = payload.max_inflight {
        if !first {
            builder.push(", ");
        }
        builder
            .push("max_inflight = ")
            .push_bind(max_inflight as i64);
        first = false;
    }
    if let Some(format) = payload.format {
        if !first {
            builder.push(", ");
        }
        builder
            .push("format = ")
            .push_bind(normalize_backend_format(&format)?);
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
    let result = builder.build().execute(pool).await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("backend not found"));
    }

    state.reload_now().await?;
    Ok(Json(json!({ "id": id })))
}

async fn fetch_backend_models(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<ModelListResponse>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;
    let row = sqlx::query::<sqlx::Postgres>(
        "SELECT name, base_url, api_key_ref, format FROM backends WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::not_found("backend not found"))?;

    let name: String = row.try_get("name")?;
    let base_url: String = row.try_get("base_url")?;
    let api_key_ref: String = row.try_get("api_key_ref")?;
    let format: String = row.try_get("format")?;
    let key = resolve_backend_key(&api_key_ref)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            ApiError::new(StatusCode::BAD_REQUEST, "backend API key is not configured")
        })?;
    let url = join_provider_url(&base_url, "/v1/models");
    let mut req = state
        .runtime
        .client
        .get(url)
        .timeout(Duration::from_secs(15));
    match normalize_backend_format(&format)? {
        "openai" => {
            req = req.bearer_auth(key);
        }
        "anthropic" => {
            req = req
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01");
        }
        _ => unreachable!(),
    }
    let resp = req
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("fetch models: {e}")))?;
    if !resp.status().is_success() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("provider models endpoint returned {}", resp.status()),
        ));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("parse models response: {e}")))?;
    let models = body
        .get("data")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(|v| v.as_str()).map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(Json(ModelListResponse {
        backend_id: id,
        backend_name: name,
        models,
    }))
}

async fn list_routes(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<Vec<RouteResponse>>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;
    Ok(Json(list_routes_from_pool(pool).await?))
}

async fn upsert_route(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<UpsertRoute>,
) -> Result<Json<RouteResponse>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let model_name = non_empty_trimmed(payload.model_name, "model_name")?;
    if payload.backend_ids.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "backend_ids must not be empty",
        ));
    }
    if payload.backend_ids.iter().any(|id| *id <= 0) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "backend_ids must be positive integers",
        ));
    }
    let chars_per_token = payload.chars_per_token.unwrap_or(4.0);
    if !chars_per_token.is_finite() || chars_per_token <= 0.0 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "chars_per_token must be greater than zero",
        ));
    }
    let first_byte_timeout = payload.first_byte_timeout.unwrap_or(180);
    if first_byte_timeout == 0 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "first_byte_timeout must be greater than zero",
        ));
    }
    if let Some(p) = payload.price_input_per_mtok_usd
        && (p < 0.0 || !p.is_finite())
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "price_input_per_mtok_usd must be zero or positive",
        ));
    }
    if let Some(p) = payload.price_output_per_mtok_usd
        && (p < 0.0 || !p.is_finite())
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "price_output_per_mtok_usd must be zero or positive",
        ));
    }
    let provider_model_name = payload
        .provider_model_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| model_name.clone());
    let enabled = payload.enabled.unwrap_or(true);
    let backend_ids_json = serde_json::to_string(&payload.backend_ids)?;
    let pool = state.pool().await?;

    sqlx::query::<sqlx::Postgres>(
        "INSERT INTO model_routes (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, \
         provider_model_name, context_tokens, max_output_tokens, \
         price_input_per_mtok_usd, price_output_per_mtok_usd, enabled) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         ON CONFLICT (model_name) DO UPDATE SET \
         backend_ids = EXCLUDED.backend_ids, fallback_backend_id = EXCLUDED.fallback_backend_id, \
         chars_per_token = EXCLUDED.chars_per_token, first_byte_timeout = EXCLUDED.first_byte_timeout, \
         provider_model_name = EXCLUDED.provider_model_name, context_tokens = EXCLUDED.context_tokens, \
         max_output_tokens = EXCLUDED.max_output_tokens, \
         price_input_per_mtok_usd = EXCLUDED.price_input_per_mtok_usd, \
         price_output_per_mtok_usd = EXCLUDED.price_output_per_mtok_usd, enabled = EXCLUDED.enabled",
    )
    .bind(&model_name)
    .bind(&backend_ids_json)
    .bind(payload.fallback_backend_id)
    .bind(chars_per_token)
    .bind(first_byte_timeout as i64)
    .bind(&provider_model_name)
    .bind(payload.context_tokens)
    .bind(payload.max_output_tokens)
    .bind(payload.price_input_per_mtok_usd)
    .bind(payload.price_output_per_mtok_usd)
    .bind(enabled)
    .execute(pool)
    .await?;

    state.reload_now().await?;
    Ok(Json(RouteResponse {
        model_name,
        backend_ids: payload.backend_ids,
        fallback_backend_id: payload.fallback_backend_id,
        chars_per_token,
        first_byte_timeout,
        provider_model_name,
        context_tokens: payload.context_tokens,
        max_output_tokens: payload.max_output_tokens,
        price_input_per_mtok_usd: payload.price_input_per_mtok_usd,
        price_output_per_mtok_usd: payload.price_output_per_mtok_usd,
        enabled,
    }))
}

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
         (key_hash, key_prefix, team_id, owner, allowed_models, budget, rpm_limit, concurrency_limit, expires_at, enabled, key_secret) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
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
    .bind(Some(&key))
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

async fn query_usage_rows(
    pool: &PgPool,
    team: Option<i64>,
    key: Option<i64>,
    from: Option<i64>,
    to: Option<i64>,
) -> Result<Vec<UsageRow>, ApiError> {
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT ts, request_id, key_id, team_id, model, backend_id, status, \
         input_tokens, output_tokens, estimated, ttfb_ms, total_ms, router_overhead_ms, \
         stream, client_aborted, error_class \
         FROM usage_ledger WHERE 1=1",
    );
    if let Some(team) = team {
        builder.push(" AND team_id = ").push_bind(team);
    }
    if let Some(key) = key {
        builder.push(" AND key_id = ").push_bind(key);
    }
    if let Some(from) = from {
        builder.push(" AND ts >= ").push_bind(from);
    }
    if let Some(to) = to {
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
    Ok(result)
}

async fn get_usage(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(params): Query<UsageQuery>,
) -> Result<Json<Vec<UsageRow>>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;
    Ok(Json(
        query_usage_rows(pool, params.team, params.key, params.from, params.to).await?,
    ))
}

// ===== Portal: user tự phục vụ (auth bằng API key, không phải admin key) =====

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    if let Some(auth) = headers.get(header::AUTHORIZATION)
        && let Ok(auth_str) = auth.to_str()
        && let Some(stripped) = auth_str.strip_prefix("Bearer ")
    {
        return Some(stripped.trim().to_string());
    }
    if let Some(key) = headers.get("x-api-key")
        && let Ok(key_str) = key.to_str()
    {
        return Some(key_str.trim().to_string());
    }
    None
}

/// Xác thực user bằng client API key. Trả ApiKey nếu hợp lệ + team còn enabled.
fn authorize_user_key(state: &AdminState, headers: &HeaderMap) -> Result<ApiKey, ApiError> {
    let plaintext =
        extract_api_key(headers).ok_or_else(|| ApiError::unauthorized("missing API key"))?;
    let hash = auth::hash_key(&plaintext);
    let snap = state.runtime.cfg.load_full();
    auth::authorize_key(&snap, &hash)
        .map_err(|_| ApiError::unauthorized("invalid or disabled API key"))
}

async fn list_teams(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<Vec<TeamListRow>>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;
    let rows =
        sqlx::query::<sqlx::Postgres>("SELECT id, name, budget, enabled FROM teams ORDER BY id")
            .fetch_all(pool)
            .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let budget_json: Option<String> = row.try_get("budget")?;
        out.push(TeamListRow {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            budget: budget_json.map(|s| serde_json::from_str(&s)).transpose()?,
            enabled: row.try_get("enabled")?,
        });
    }
    Ok(Json(out))
}

async fn list_keys(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<Vec<KeyListRow>>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;
    let rows = sqlx::query::<sqlx::Postgres>(
        "SELECT k.id, k.key_prefix, k.team_id, COALESCE(t.name, '') AS team_name, k.owner, \
         k.allowed_models, k.budget, k.rpm_limit, k.concurrency_limit, k.expires_at, k.enabled \
         FROM api_keys k LEFT JOIN teams t ON t.id = k.team_id ORDER BY k.id",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let allowed_json: String = row.try_get("allowed_models")?;
        let budget_json: Option<String> = row.try_get("budget")?;
        out.push(KeyListRow {
            id: row.try_get("id")?,
            prefix: row.try_get("key_prefix")?,
            team_id: row.try_get("team_id")?,
            team_name: row.try_get("team_name")?,
            owner: row.try_get("owner")?,
            allowed_models: serde_json::from_str(&allowed_json).unwrap_or_default(),
            budget: budget_json.map(|s| serde_json::from_str(&s)).transpose()?,
            rpm_limit: row.try_get("rpm_limit")?,
            concurrency_limit: row.try_get("concurrency_limit")?,
            expires_at: row.try_get("expires_at")?,
            enabled: row.try_get("enabled")?,
        });
    }
    Ok(Json(out))
}

async fn query_stats(
    pool: &PgPool,
    team: Option<i64>,
    key: Option<i64>,
    days: u32,
) -> Result<Vec<StatRow>, ApiError> {
    let days = days.clamp(1, 365);
    let since = now_secs() - i64::from(days) * 86_400;
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT (ts / 86400) * 86400 AS day, model, \
         CAST(COALESCE(SUM(input_tokens), 0) AS BIGINT) AS input_tokens, \
         CAST(COALESCE(SUM(output_tokens), 0) AS BIGINT) AS output_tokens, \
         COUNT(*) AS requests \
         FROM usage_ledger WHERE ts >= ",
    );
    builder.push_bind(since);
    if let Some(team) = team {
        builder.push(" AND team_id = ").push_bind(team);
    }
    if let Some(key) = key {
        builder.push(" AND key_id = ").push_bind(key);
    }
    builder.push(" GROUP BY 1, 2 ORDER BY 1, 2");
    let query = builder.build();
    let rows = query.fetch_all(pool).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(StatRow {
            day: row.try_get("day")?,
            model: row.try_get("model")?,
            input_tokens: row.try_get("input_tokens")?,
            output_tokens: row.try_get("output_tokens")?,
            requests: row.try_get("requests")?,
        });
    }
    Ok(out)
}

async fn get_stats(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(params): Query<StatsQuery>,
) -> Result<Json<Vec<StatRow>>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;
    Ok(Json(
        query_stats(pool, params.team, None, params.days.unwrap_or(30)).await?,
    ))
}

async fn me(
    Extension(state): Extension<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, ApiError> {
    let key = authorize_user_key(&state, &headers)?;
    let snap = state.runtime.cfg.load_full();
    let team = snap
        .teams
        .get(&key.team_id)
        .cloned()
        .ok_or_else(|| ApiError::unauthorized("team not found or disabled"))?;
    Ok(Json(MeResponse {
        key: MeKey {
            id: key.id,
            prefix: key.key_prefix.clone(),
            owner: key.owner.clone(),
            allowed_models: key.allowed_models.clone(),
            budget: key.budget.clone(),
            rpm_limit: key.rpm_limit,
            concurrency_limit: key.concurrency_limit,
            expires_at: key.expires_at,
            enabled: key.enabled,
        },
        team: MeTeam {
            id: team.id,
            name: team.name,
            budget: team.budget,
            enabled: team.enabled,
        },
    }))
}

async fn me_usage(
    Extension(state): Extension<Arc<AdminState>>,
    headers: HeaderMap,
    Query(params): Query<UsageQuery>,
) -> Result<Json<Vec<UsageRow>>, ApiError> {
    let key = authorize_user_key(&state, &headers)?;
    let pool = state.pool().await?;
    Ok(Json(
        query_usage_rows(pool, None, Some(key.id), params.from, params.to).await?,
    ))
}

async fn me_stats(
    Extension(state): Extension<Arc<AdminState>>,
    headers: HeaderMap,
    Query(params): Query<MeStatsQuery>,
) -> Result<Json<Vec<StatRow>>, ApiError> {
    let key = authorize_user_key(&state, &headers)?;
    let pool = state.pool().await?;
    Ok(Json(
        query_stats(pool, None, Some(key.id), params.days.unwrap_or(30)).await?,
    ))
}

/// Admin xem lại plaintext client key (yêu cầu user). Chỉ key tạo SAU migration 0003 có key_secret.
async fn reveal_key(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<KeyRevealResponse>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;
    let row = sqlx::query::<sqlx::Postgres>(
        "SELECT key_prefix, owner, key_secret FROM api_keys WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::not_found("key not found"))?;
    let secret: Option<String> = row.try_get("key_secret")?;
    let key = secret.ok_or_else(|| {
        ApiError::new(
            StatusCode::GONE,
            "plaintext not stored (key created before key-reveal); disable and recreate it",
        )
    })?;
    Ok(Json(KeyRevealResponse {
        id,
        prefix: row.try_get("key_prefix")?,
        owner: row.try_get("owner")?,
        key,
    }))
}

/// Read-only runtime settings cho màn Settings (admin path, không phải hot path).
async fn get_settings(
    Extension(state): Extension<Arc<AdminState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<SettingsResponse>, ApiError> {
    check_admin_auth(&state.master_key, &state.allow_cidrs, &headers, peer.ip())?;
    let pool = state.pool().await?;
    let database_ok = sqlx::query::<sqlx::Postgres>("SELECT 1")
        .fetch_optional(pool)
        .await
        .is_ok();
    let ok = state
        .runtime
        .config_ok_at
        .load(std::sync::atomic::Ordering::Relaxed);
    let err = state
        .runtime
        .config_err_at
        .load(std::sync::atomic::Ordering::Relaxed);
    Ok(Json(SettingsResponse {
        listen_addr: std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:18080".to_string()),
        database_ok,
        config_reload_ok: ok > 0 && ok > err,
        max_body_bytes: state.runtime.max_body_bytes,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
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
    fn provider_url_join_handles_host_and_sdk_base_urls() {
        assert_eq!(
            join_provider_url("https://api.openai.com", "/v1/models"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            join_provider_url("https://api.moonshot.ai/v1", "/v1/models"),
            "https://api.moonshot.ai/v1/models"
        );
        assert_eq!(
            join_provider_url(
                "https://dashscope.example.com/compatible-mode/v1",
                "/v1/models"
            ),
            "https://dashscope.example.com/compatible-mode/v1/models"
        );
    }

    #[test]
    fn backend_ids_parser_accepts_json_integer_array() {
        assert_eq!(parse_backend_ids("[1,2,3]").unwrap(), vec![1, 2, 3]);
        assert!(parse_backend_ids("not-json").is_err());
        assert!(parse_backend_ids("[1,\"bad\"]").is_err());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_routes_from_pool_returns_sorted_routes(pool: PgPool) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO model_routes \
             (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout) \
             VALUES ($1, $2, $3, $4, $5), ($6, $7, $8, $9, $10)",
        )
        .bind("z-model")
        .bind("[2,3]")
        .bind(3_i64)
        .bind(4.0_f64)
        .bind(180_i64)
        .bind("a-model")
        .bind("[1]")
        .bind(None::<i64>)
        .bind(3.5_f64)
        .bind(90_i64)
        .execute(&pool)
        .await?;

        let routes = list_routes_from_pool(&pool).await.expect("list routes");
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].model_name, "a-model");
        assert_eq!(routes[0].backend_ids, vec![1]);
        assert_eq!(routes[0].fallback_backend_id, None);
        assert_eq!(routes[0].chars_per_token, 3.5);
        assert_eq!(routes[0].first_byte_timeout, 90);
        assert_eq!(routes[1].model_name, "z-model");
        assert_eq!(routes[1].backend_ids, vec![2, 3]);
        assert_eq!(routes[1].fallback_backend_id, Some(3));
        Ok(())
    }

    #[test]
    fn backend_format_validation_is_strict() {
        assert_eq!(normalize_backend_format("openai").unwrap(), "openai");
        assert_eq!(normalize_backend_format("Open_AI").unwrap(), "openai");
        assert_eq!(normalize_backend_format("anthropic").unwrap(), "anthropic");
        assert!(normalize_backend_format("gemini").is_err());
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

    #[test]
    fn extract_api_key_prefers_bearer_then_x_api_key() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer secret-key".parse().unwrap());
        assert_eq!(extract_api_key(&headers).as_deref(), Some("secret-key"));

        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "xkey".parse().unwrap());
        assert_eq!(extract_api_key(&headers).as_deref(), Some("xkey"));

        assert!(extract_api_key(&HeaderMap::new()).is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn stats_aggregate_daily_by_model(pool: PgPool) -> anyhow::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let today = (now / 86_400) * 86_400;

        let insert = |rid: &str, model: &str, input: i64, output: i64| {
            sqlx::query(
                "INSERT INTO usage_ledger \
                 (ts, request_id, key_id, team_id, model, backend_id, status, \
                  input_tokens, output_tokens, estimated, ttfb_ms, total_ms, router_overhead_ms, stream, client_aborted) \
                 VALUES ($1, $2, 1, 1, $3, 1, 200, $4, $5, false, 0, 0, 0, false, false)",
            )
            .bind(today)
            .bind(rid)
            .bind(model)
            .bind(input)
            .bind(output)
            .execute(&pool)
        };
        insert("r1", "model-a", 100, 10).await?;
        insert("r2", "model-a", 50, 5).await?;
        insert("r3", "model-b", 30, 3).await?;

        let stats = query_stats(&pool, None, None, 30).await.expect("stats");
        let a = stats
            .iter()
            .find(|s| s.model == "model-a")
            .expect("model-a present");
        assert_eq!(a.input_tokens, 150);
        assert_eq!(a.output_tokens, 15);
        assert_eq!(a.requests, 2);
        let b = stats
            .iter()
            .find(|s| s.model == "model-b")
            .expect("model-b present");
        assert_eq!(b.input_tokens, 30);
        assert_eq!(b.requests, 1);
        Ok(())
    }
}
