//! Glue layer: axum router + pipeline auth -> body -> budget -> concurrency -> proxy.
//! Hot path không lock, không đọc disk/env, không parse full body (chỉ struct 2 field model+stream).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::{Body, BodyDataStream, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, Request, Response, StatusCode, Uri, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use bytes::BytesMut;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::value::RawValue;

use crate::auth;
use crate::contract::{ApiKey, AppState, BudgetError, ModelRoute, ProviderProtocol};
use crate::proxy::{self, ProxyContext, ProxyRequestBody};

/// Chỉ parse field top-level, bỏ qua toàn bộ phần còn lại (không cấp phát cho skipped fields).
/// stream_options dùng RawValue để biết top-level có key này hay không (serde chỉ map field top-level).
#[derive(Deserialize)]
struct RequestHead<'a> {
    #[serde(borrow)]
    model: &'a str,
    #[serde(default)]
    stream: bool,
    #[serde(default, borrow)]
    stream_options: Option<&'a RawValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestHeadOwned {
    model: String,
    stream: bool,
    stream_options_present: bool,
}

impl<'a> From<RequestHead<'a>> for RequestHeadOwned {
    fn from(h: RequestHead<'a>) -> Self {
        Self {
            model: h.model.to_string(),
            stream: h.stream,
            stream_options_present: h.stream_options.is_some(),
        }
    }
}

const FAST_UPLOAD_MIN_BYTES: u64 = 64 * 1024;
const HEAD_PREFIX_LIMIT: usize = 16 * 1024;

enum IncomingBody {
    Buffered {
        head: RequestHeadOwned,
        body: Bytes,
    },
    StreamingCandidate {
        head: RequestHeadOwned,
        prefix: Bytes,
        rest: BodyDataStream,
        content_length: u64,
    },
}

impl IncomingBody {
    fn body_len(&self) -> usize {
        match self {
            Self::Buffered { body, .. } => body.len(),
            Self::StreamingCandidate { content_length, .. } => *content_length as usize,
        }
    }

    fn head(&self) -> &RequestHeadOwned {
        match self {
            Self::Buffered { head, .. } | Self::StreamingCandidate { head, .. } => head,
        }
    }

    async fn into_proxy_body(
        self,
        allow_streaming_upload: bool,
        max_body_bytes: usize,
    ) -> Result<(RequestHeadOwned, ProxyRequestBody), BodyReadError> {
        match self {
            Self::Buffered { head, body } => Ok((head, ProxyRequestBody::Buffered(body))),
            Self::StreamingCandidate {
                head,
                prefix,
                rest,
                content_length,
            } if allow_streaming_upload => Ok((
                head,
                ProxyRequestBody::Streaming {
                    prefix,
                    rest,
                    content_length,
                },
            )),
            Self::StreamingCandidate { prefix, rest, .. } => {
                let body = collect_remaining_body(prefix, rest, max_body_bytes).await?;
                let parsed: RequestHead<'_> =
                    serde_json::from_slice(&body).map_err(|_| BodyReadError::InvalidJson)?;
                Ok((parsed.into(), ProxyRequestBody::Buffered(body)))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyReadError {
    TooLarge,
    ReadFailed,
    InvalidJson,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PrefixScan {
    NeedMore,
    Found(RequestHeadOwned),
}

async fn read_incoming_body(
    body: Body,
    max_body_bytes: usize,
    content_length: Option<u64>,
) -> Result<IncomingBody, BodyReadError> {
    if let Some(len) = content_length
        && len > max_body_bytes as u64
    {
        return Err(BodyReadError::TooLarge);
    }

    let try_fast = content_length.is_some_and(|len| len >= FAST_UPLOAD_MIN_BYTES);
    if !try_fast {
        let stream = body.into_data_stream();
        let full = collect_remaining_body(Bytes::new(), stream, max_body_bytes).await?;
        let head: RequestHead<'_> =
            serde_json::from_slice(&full).map_err(|_| BodyReadError::InvalidJson)?;
        return Ok(IncomingBody::Buffered {
            head: head.into(),
            body: full,
        });
    }

    let content_length = content_length.unwrap_or_default();
    let mut stream = body.into_data_stream();
    let mut prefix = BytesMut::new();
    while let Some(next) = stream.next().await {
        let chunk = next.map_err(|_| BodyReadError::ReadFailed)?;
        if prefix.len().saturating_add(chunk.len()) > max_body_bytes {
            return Err(BodyReadError::TooLarge);
        }
        prefix.extend_from_slice(&chunk);

        if let PrefixScan::Found(head) = scan_head_prefix(&prefix) {
            // Fast path is deliberately narrow: only non-stream requests with explicit top-level
            // stream=false. Stream=true may need body mutation for stream_options injection.
            if !head.stream {
                return Ok(IncomingBody::StreamingCandidate {
                    head,
                    prefix: prefix.freeze(),
                    rest: stream,
                    content_length,
                });
            }
        }

        if prefix.len() >= HEAD_PREFIX_LIMIT {
            break;
        }
    }

    let full = collect_remaining_body(prefix.freeze(), stream, max_body_bytes).await?;
    let head: RequestHead<'_> =
        serde_json::from_slice(&full).map_err(|_| BodyReadError::InvalidJson)?;
    Ok(IncomingBody::Buffered {
        head: head.into(),
        body: full,
    })
}

async fn collect_remaining_body(
    prefix: Bytes,
    mut rest: BodyDataStream,
    max_body_bytes: usize,
) -> Result<Bytes, BodyReadError> {
    let mut out = BytesMut::with_capacity(prefix.len().min(max_body_bytes));
    out.extend_from_slice(&prefix);
    while let Some(next) = rest.next().await {
        let chunk = next.map_err(|_| BodyReadError::ReadFailed)?;
        if out.len().saturating_add(chunk.len()) > max_body_bytes {
            return Err(BodyReadError::TooLarge);
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out.freeze())
}

fn parse_content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

fn scan_head_prefix(data: &[u8]) -> PrefixScan {
    let mut i = skip_ws(data, 0);
    if data.get(i) != Some(&b'{') {
        return PrefixScan::NeedMore;
    }
    i += 1;
    let mut model: Option<String> = None;
    let mut stream: Option<bool> = None;
    let mut stream_options_present = false;
    let mut depth: i32 = 1;

    while i < data.len() {
        i = skip_ws(data, i);
        if i >= data.len() {
            break;
        }
        match data[i] {
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                i += 1;
            }
            b'{' | b'[' => {
                depth += 1;
                i += 1;
            }
            b']' => {
                depth -= 1;
                i += 1;
            }
            b'"' => {
                let Some(end) = find_string_end(data, i) else {
                    return PrefixScan::NeedMore;
                };
                if depth == 1 {
                    let key_slice = &data[i..=end];
                    let Ok(key) = serde_json::from_slice::<String>(key_slice) else {
                        return PrefixScan::NeedMore;
                    };
                    let mut j = skip_ws(data, end + 1);
                    if data.get(j) != Some(&b':') {
                        i = end + 1;
                        continue;
                    }
                    j = skip_ws(data, j + 1);
                    match key.as_str() {
                        "model" => {
                            if data.get(j) == Some(&b'"') {
                                let Some(v_end) = find_string_end(data, j) else {
                                    return PrefixScan::NeedMore;
                                };
                                if let Ok(v) = serde_json::from_slice::<String>(&data[j..=v_end]) {
                                    model = Some(v);
                                }
                                i = v_end + 1;
                            } else {
                                i = j;
                            }
                        }
                        "stream" => {
                            if data.get(j..j + 4) == Some(b"true") {
                                stream = Some(true);
                                i = j + 4;
                            } else if data.get(j..j + 5) == Some(b"false") {
                                stream = Some(false);
                                i = j + 5;
                            } else {
                                i = j;
                            }
                        }
                        "stream_options" => {
                            stream_options_present = true;
                            i = j;
                        }
                        _ => i = end + 1,
                    }
                    if let (Some(model), Some(stream)) = (&model, stream) {
                        return PrefixScan::Found(RequestHeadOwned {
                            model: model.clone(),
                            stream,
                            stream_options_present,
                        });
                    }
                } else {
                    i = end + 1;
                }
            }
            _ => i += 1,
        }
    }

    PrefixScan::NeedMore
}

fn skip_ws(data: &[u8], mut i: usize) -> usize {
    while matches!(data.get(i), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        i += 1;
    }
    i
}

fn find_string_end(data: &[u8], start: usize) -> Option<usize> {
    let mut escaped = false;
    let mut i = start.checked_add(1)?;
    while i < data.len() {
        let b = data[i];
        if escaped {
            escaped = false;
        } else if b == b'\\' {
            escaped = true;
        } else if b == b'"' {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::<Arc<AppState>>::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/completions", post(completions))
        .route("/v1/embeddings", post(embeddings))
        .route("/v1/rerank", post(rerank))
        .route("/v1/audio/transcriptions", post(audio_transcriptions))
        .route("/v1/messages", post(messages))
        .route("/v1/models", get(models))
        .route("/metrics", get(metrics))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/", get(portal))
        .nest_service("/admin", crate::admin::router(state.clone()))
        .nest_service("/portal", crate::admin::user_router(state.clone()))
        .with_state(state)
}

async fn chat_completions(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
) -> Response<Body> {
    handle_generate(state, req, "/v1/chat/completions").await
}
async fn completions(State(state): State<Arc<AppState>>, req: Request<Body>) -> Response<Body> {
    handle_generate(state, req, "/v1/completions").await
}
async fn embeddings(State(state): State<Arc<AppState>>, req: Request<Body>) -> Response<Body> {
    handle_generate(state, req, "/v1/embeddings").await
}
async fn rerank(State(state): State<Arc<AppState>>, req: Request<Body>) -> Response<Body> {
    handle_generate(state, req, "/v1/rerank").await
}
async fn audio_transcriptions(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
) -> Response<Body> {
    handle_multipart_adapter(state, req, "/v1/audio/transcriptions").await
}
async fn messages(State(state): State<Arc<AppState>>, req: Request<Body>) -> Response<Body> {
    handle_generate(state, req, "/v1/messages").await
}

async fn models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let cfg = state.cfg.load_full();
    let data: Vec<serde_json::Value> = cfg
        .routes
        .keys()
        .map(|m| serde_json::json!({"id": m, "object": "model"}))
        .collect();
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({ "object": "list", "data": data })),
    )
}

async fn metrics(State(state): State<Arc<AppState>>) -> String {
    state.metrics.render()
}

async fn healthz() -> &'static str {
    "ok"
}

/// Control-plane readiness: 200 nếu config đã load thành công gần nhất; 503 nếu chưa load, lần reload
/// cuối thất bại (Postgres chết), HOẶC lần thành công cuối quá cũ (reload treo). KHÔNG query DB — đọc
/// 2 atomic + clock. Hot path không phụ thuộc.
async fn readyz(State(state): State<Arc<AppState>>) -> (StatusCode, &'static str) {
    let ok = state.config_ok_at.load(Ordering::Relaxed);
    let err = state.config_err_at.load(Ordering::Relaxed);
    let code = readiness_status(ok, err, state.readiness_max_stale_ms, now_ms());
    if code == StatusCode::OK {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready")
    }
}

/// Quyết định readiness thuần tuý (dễ unit-test). Stale-success: ok quá cũ -> không ready kể cả khi
/// reload đang treo (chưa kịp ghi config_err_at).
fn readiness_status(ok: u64, err: u64, stale_ms: u64, now: u64) -> StatusCode {
    let stale = ok == 0 || now.saturating_sub(ok) > stale_ms;
    if stale || err > ok {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn portal(_: State<Arc<AppState>>) -> impl IntoResponse {
    let body = std::env::var("PORTAL_STATIC_FILE")
        .ok()
        .filter(|path| !path.trim().is_empty())
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_else(|| include_str!("../static/index.html").to_string());
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], body)
}

async fn handle_generate(
    state: Arc<AppState>,
    req: Request<Body>,
    incoming_path: &'static str,
) -> Response<Body> {
    let started = Instant::now();
    let request_id = generate_request_id();

    let (mut parts, body) = req.into_parts();

    // 1. Auth key — 401 nhanh trước khi đọc body (không tốn chi phí đọc 200K body cho key sai).
    let Some(api_key_plain) = extract_api_key(&parts.headers) else {
        return build_error(&request_id, StatusCode::UNAUTHORIZED, "missing API key");
    };
    let key_hash = auth::hash_key(&api_key_plain);
    let snapshot = state.cfg.load_full();
    let key: ApiKey = match auth::authorize_key(&snapshot, &key_hash) {
        Ok(k) => k,
        Err(_) => return build_error(&request_id, StatusCode::UNAUTHORIZED, "invalid API key"),
    };

    // 2. Đọc request head. Large non-stream bodies can stay as a streaming upload candidate;
    // ambiguous or mutating cases fall back to the old full-buffer + serde validation path.
    let incoming = match read_incoming_body(
        body,
        state.max_body_bytes,
        parse_content_length(&parts.headers),
    )
    .await
    {
        Ok(b) => b,
        Err(BodyReadError::TooLarge) => {
            return build_error(
                &request_id,
                StatusCode::PAYLOAD_TOO_LARGE,
                "request body too large",
            );
        }
        Err(BodyReadError::ReadFailed) => {
            return build_error(
                &request_id,
                StatusCode::BAD_REQUEST,
                "request body read failed",
            );
        }
        Err(BodyReadError::InvalidJson) => {
            return build_error(&request_id, StatusCode::BAD_REQUEST, "invalid JSON");
        }
    };
    let head = incoming.head();
    let model_name = head.model.clone();
    let model = model_name.as_str();
    if model.is_empty() {
        return build_error(&request_id, StatusCode::BAD_REQUEST, "model is required");
    }

    // 3. Quyền model.
    if !auth::model_allowed(&key, model) {
        return build_error(
            &request_id,
            StatusCode::FORBIDDEN,
            "model not allowed for this key",
        );
    }

    // 4. Route.
    let Some(route) = snapshot.routes.get(model).cloned() else {
        return build_error(&request_id, StatusCode::NOT_FOUND, "model not configured");
    };
    if !route.enabled {
        return build_error(&request_id, StatusCode::FORBIDDEN, "model is disabled");
    }

    // Effective-enabled guard (CODEX lifecycle): route enabled nhưng mọi backend tham chiếu đều
    // disabled -> từ chối RÕ RÀNG trước khi acquire/forward, KHÔNG trả "503 no healthy backend".
    let any_backend_enabled = route.backend_ids.iter().any(|id| {
        snapshot
            .backends
            .get(id)
            .map(|b| b.enabled)
            .unwrap_or(false)
    }) || route
        .fallback_backend_id
        .map(|id| {
            snapshot
                .backends
                .get(&id)
                .map(|b| b.enabled)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if !any_backend_enabled {
        return build_error(
            &request_id,
            StatusCode::FORBIDDEN,
            "model is disabled: no enabled provider backend",
        );
    }

    // Protocol endpoint guard (CODEX provider-protocol taxonomy): route chỉ chấp nhận endpoint
    // đã khai báo. Gọi sai endpoint -> 400 rõ ràng, KHÔNG forward shape sai lên provider.
    let protocol = ProviderProtocol::parse(&route.protocol);
    if protocol.incoming_path() != incoming_path {
        return build_error(
            &request_id,
            StatusCode::BAD_REQUEST,
            &format!(
                "model '{model}' is a {} route (call {})",
                protocol.label(),
                protocol.incoming_path()
            ),
        );
    }

    // 5. Budget reserve + concurrency (RAII: nếu mọi đường return sau đây, tự rollback/release).
    let allow_streaming_upload =
        route.backend_ids.len() == 1 && route.fallback_backend_id.is_none();
    let body_len = incoming.body_len();
    let est_tokens = estimate_tokens_len(body_len, &route);
    let (head, proxy_body) = match incoming
        .into_proxy_body(allow_streaming_upload, state.max_body_bytes)
        .await
    {
        Ok(v) => v,
        Err(BodyReadError::TooLarge) => {
            return build_error(
                &request_id,
                StatusCode::PAYLOAD_TOO_LARGE,
                "request body too large",
            );
        }
        Err(BodyReadError::ReadFailed) => {
            return build_error(
                &request_id,
                StatusCode::BAD_REQUEST,
                "request body read failed",
            );
        }
        Err(BodyReadError::InvalidJson) => {
            return build_error(&request_id, StatusCode::BAD_REQUEST, "invalid JSON");
        }
    };
    // Viết lại top-level "model" từ public name -> provider model name (nếu khác) cho body buffered.
    // Adapter JSON protocols may also need tiny provider-specific normalization. Streaming upload
    // stays narrow: it must not need body mutation.
    let (head, mut proxy_body) = (head, proxy_body);
    match &proxy_body {
        ProxyRequestBody::Buffered(body) => {
            if let Some(b) = rewrite_json_proxy_body(body, &route, protocol) {
                proxy_body = ProxyRequestBody::Buffered(Bytes::from(b));
            } else if route.provider_model_name != model
                || protocol == ProviderProtocol::VoyageRerank
                || protocol == ProviderProtocol::QwenRerank
            {
                return build_error(
                    &request_id,
                    StatusCode::BAD_REQUEST,
                    "could not rewrite adapter request body",
                );
            }
        }
        ProxyRequestBody::Streaming { .. } => {
            if route.provider_model_name != model
                || protocol == ProviderProtocol::VoyageRerank
                || protocol == ProviderProtocol::QwenRerank
            {
                return build_error(
                    &request_id,
                    StatusCode::BAD_REQUEST,
                    "streaming upload requires no model or adapter body rewrite",
                );
            }
        }
    }
    if protocol == ProviderProtocol::QwenRerank {
        parts.uri = match qwen_rerank_upstream_uri(&route.provider_model_name) {
            Ok(uri) => uri,
            Err(_) => {
                return build_error(
                    &request_id,
                    StatusCode::BAD_REQUEST,
                    "could not build Qwen rerank upstream path",
                );
            }
        };
    }

    let stream = head.stream;
    let stream_options_present = head.stream_options_present;

    let reservation = match state.budget.reserve(&key, model, est_tokens) {
        Ok(r) => Some(r),
        Err(BudgetError::BudgetExceeded { .. }) => {
            return build_error(
                &request_id,
                StatusCode::TOO_MANY_REQUESTS,
                "budget exceeded",
            );
        }
        Err(BudgetError::RateLimited { retry_after }) => {
            let mut resp = build_error(&request_id, StatusCode::TOO_MANY_REQUESTS, "rate limited");
            if let Ok(v) = header::HeaderValue::from_str(&retry_after.as_secs().to_string()) {
                resp.headers_mut().insert(header::RETRY_AFTER, v);
            }
            return resp;
        }
    };
    let concurrency = match state.budget.acquire_concurrency(&key) {
        Some(g) => Some(g),
        None => {
            return build_error(
                &request_id,
                StatusCode::TOO_MANY_REQUESTS,
                "concurrency limit reached",
            );
        }
    };

    // 6. Forward qua proxy (nơi duy nhất forward/stream/tap).
    let ctx = ProxyContext {
        api_key: key,
        model_name: model.to_string(),
        route,
        request_id,
        stream,
        stream_options_present,
        reservation,
        concurrency,
        start: started,
        estimated_input_tokens: est_tokens,
    };
    let req = Request::from_parts(parts, proxy_body);
    proxy::proxy_forward(state, req, ctx).await
}

async fn handle_multipart_adapter(
    state: Arc<AppState>,
    req: Request<Body>,
    incoming_path: &'static str,
) -> Response<Body> {
    let started = Instant::now();
    let request_id = generate_request_id();
    let (parts, body) = req.into_parts();

    let Some(api_key_plain) = extract_api_key(&parts.headers) else {
        return build_error(&request_id, StatusCode::UNAUTHORIZED, "missing API key");
    };
    let key_hash = auth::hash_key(&api_key_plain);
    let snapshot = state.cfg.load_full();
    let key: ApiKey = match auth::authorize_key(&snapshot, &key_hash) {
        Ok(k) => k,
        Err(_) => return build_error(&request_id, StatusCode::UNAUTHORIZED, "invalid API key"),
    };

    let content_length = parse_content_length(&parts.headers);
    if let Some(len) = content_length
        && len > state.max_body_bytes as u64
    {
        return build_error(
            &request_id,
            StatusCode::PAYLOAD_TOO_LARGE,
            "request body too large",
        );
    }
    let body =
        match collect_remaining_body(Bytes::new(), body.into_data_stream(), state.max_body_bytes)
            .await
        {
            Ok(b) => b,
            Err(BodyReadError::TooLarge) => {
                return build_error(
                    &request_id,
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "request body too large",
                );
            }
            Err(BodyReadError::ReadFailed) | Err(BodyReadError::InvalidJson) => {
                return build_error(
                    &request_id,
                    StatusCode::BAD_REQUEST,
                    "request body read failed",
                );
            }
        };

    let Some(model_name) = extract_multipart_text_field(&body, "model") else {
        return build_error(&request_id, StatusCode::BAD_REQUEST, "model is required");
    };
    let model = model_name.trim();
    if model.is_empty() {
        return build_error(&request_id, StatusCode::BAD_REQUEST, "model is required");
    }
    if !auth::model_allowed(&key, model) {
        return build_error(
            &request_id,
            StatusCode::FORBIDDEN,
            "model not allowed for this key",
        );
    }

    let Some(route) = snapshot.routes.get(model).cloned() else {
        return build_error(&request_id, StatusCode::NOT_FOUND, "model not configured");
    };
    if !route.enabled {
        return build_error(&request_id, StatusCode::FORBIDDEN, "model is disabled");
    }
    let any_backend_enabled = route.backend_ids.iter().any(|id| {
        snapshot
            .backends
            .get(id)
            .map(|b| b.enabled)
            .unwrap_or(false)
    }) || route
        .fallback_backend_id
        .map(|id| {
            snapshot
                .backends
                .get(&id)
                .map(|b| b.enabled)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if !any_backend_enabled {
        return build_error(
            &request_id,
            StatusCode::FORBIDDEN,
            "model is disabled: no enabled provider backend",
        );
    }

    let protocol = ProviderProtocol::parse(&route.protocol);
    if protocol.incoming_path() != incoming_path {
        return build_error(
            &request_id,
            StatusCode::BAD_REQUEST,
            &format!(
                "model '{model}' is a {} route (call {})",
                protocol.label(),
                protocol.incoming_path()
            ),
        );
    }
    let body = if route.provider_model_name != model {
        match rewrite_multipart_text_field(&body, "model", &route.provider_model_name) {
            Some(b) => Bytes::from(b),
            None => {
                return build_error(
                    &request_id,
                    StatusCode::BAD_REQUEST,
                    "could not rewrite multipart model field",
                );
            }
        }
    } else {
        body
    };

    let est_tokens = estimate_tokens_len(body.len(), &route);
    let reservation = match state.budget.reserve(&key, model, est_tokens) {
        Ok(r) => Some(r),
        Err(BudgetError::BudgetExceeded { .. }) => {
            return build_error(
                &request_id,
                StatusCode::TOO_MANY_REQUESTS,
                "budget exceeded",
            );
        }
        Err(BudgetError::RateLimited { retry_after }) => {
            let mut resp = build_error(&request_id, StatusCode::TOO_MANY_REQUESTS, "rate limited");
            if let Ok(v) = header::HeaderValue::from_str(&retry_after.as_secs().to_string()) {
                resp.headers_mut().insert(header::RETRY_AFTER, v);
            }
            return resp;
        }
    };
    let concurrency = match state.budget.acquire_concurrency(&key) {
        Some(g) => Some(g),
        None => {
            return build_error(
                &request_id,
                StatusCode::TOO_MANY_REQUESTS,
                "concurrency limit reached",
            );
        }
    };

    let ctx = ProxyContext {
        api_key: key,
        model_name: model.to_string(),
        route,
        request_id,
        stream: false,
        stream_options_present: true,
        reservation,
        concurrency,
        start: started,
        estimated_input_tokens: est_tokens,
    };
    let req = Request::from_parts(parts, ProxyRequestBody::Buffered(body));
    proxy::proxy_forward(state, req, ctx).await
}

fn extract_multipart_text_field(body: &[u8], field: &str) -> Option<String> {
    let needle = format!("name=\"{field}\"");
    let start = memchr::memmem::find(body, needle.as_bytes())?;
    let after_headers = memchr::memmem::find(&body[start..], b"\r\n\r\n")? + start + 4;
    let end_rel = memchr::memmem::find(&body[after_headers..], b"\r\n--")?;
    let raw = &body[after_headers..after_headers + end_rel];
    std::str::from_utf8(raw).ok().map(|s| s.trim().to_string())
}

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    if let Some(auth) = headers.get(header::AUTHORIZATION)
        && let Ok(auth_str) = auth.to_str()
        && let Some(stripped) = auth_str.strip_prefix("Bearer ")
    {
        return Some(stripped.to_string());
    }
    if let Some(key) = headers.get(HeaderName::from_static("x-api-key"))
        && let Ok(key_str) = key.to_str()
    {
        return Some(key_str.to_string());
    }
    None
}

fn qwen_rerank_upstream_uri(model: &str) -> Result<Uri, axum::http::uri::InvalidUri> {
    let path = if model.trim().eq_ignore_ascii_case("qwen3-rerank") {
        "/compatible-api/v1/reranks"
    } else {
        "/api/v1/services/rerank/text-rerank/text-rerank"
    };
    path.parse()
}

fn rewrite_qwen_rerank_object(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    provider_model_name: &str,
) -> Option<Vec<u8>> {
    let query = obj.get("query")?.clone();
    let documents = obj.get("documents")?.clone();
    let mut flat = serde_json::Map::new();
    flat.insert(
        "model".to_string(),
        serde_json::Value::String(provider_model_name.to_string()),
    );
    if provider_model_name
        .trim()
        .eq_ignore_ascii_case("qwen3-rerank")
    {
        flat.insert("query".to_string(), query);
        flat.insert("documents".to_string(), documents);
        for key in ["top_n", "instruct"] {
            if let Some(value) = obj.get(key).cloned() {
                flat.insert(key.to_string(), value);
            }
        }
        return serde_json::to_vec(&serde_json::Value::Object(flat)).ok();
    }

    let mut input = serde_json::Map::new();
    input.insert("query".to_string(), query);
    input.insert("documents".to_string(), documents);
    flat.insert("input".to_string(), serde_json::Value::Object(input));

    let mut parameters = serde_json::Map::new();
    for key in ["top_n", "return_documents", "instruct", "fps"] {
        if let Some(value) = obj.get(key).cloned() {
            parameters.insert(key.to_string(), value);
        }
    }
    if !parameters.is_empty() {
        flat.insert(
            "parameters".to_string(),
            serde_json::Value::Object(parameters),
        );
    }
    serde_json::to_vec(&serde_json::Value::Object(flat)).ok()
}

/// Viết lại JSON buffered nhỏ. Không dùng cho streaming large body.
fn rewrite_json_proxy_body(
    body: &[u8],
    route: &ModelRoute,
    protocol: ProviderProtocol,
) -> Option<Vec<u8>> {
    let mut value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let mut changed = false;
    if let Some(obj) = value.as_object_mut() {
        if obj.get("model").and_then(|v| v.as_str()) != Some(route.provider_model_name.as_str()) {
            obj.insert(
                "model".to_string(),
                serde_json::Value::String(route.provider_model_name.clone()),
            );
            changed = true;
        }
        if protocol == ProviderProtocol::QwenRerank {
            return rewrite_qwen_rerank_object(obj, &route.provider_model_name);
        }
        // Voyage names the result-count field top_k; BrighTO's public rerank helper uses top_n.
        if protocol == ProviderProtocol::VoyageRerank {
            if obj.get("top_k").is_none()
                && let Some(top_n) = obj.get("top_n").cloned()
            {
                obj.insert("top_k".to_string(), top_n);
            }
            let _ = obj.remove("top_n");
            // Voyage is fully handled: return the (possibly unchanged) normalized body so the
            // caller never reports a false "could not rewrite adapter request body" 400.
            return serde_json::to_vec(&serde_json::Value::Object(obj.clone())).ok();
        }
    }
    if changed {
        serde_json::to_vec(&value).ok()
    } else {
        None
    }
}

/// Viết lại text field trong multipart body buffered nhỏ, dùng cho ASR model alias.
fn rewrite_multipart_text_field(body: &[u8], field: &str, new_value: &str) -> Option<Vec<u8>> {
    let needle = format!(r#"name="{field}""#);
    let start = memchr::memmem::find(body, needle.as_bytes())?;
    let after_headers = memchr::memmem::find(&body[start..], b"\r\n\r\n")? + start + 4;
    let end_rel = memchr::memmem::find(&body[after_headers..], b"\r\n--")?;
    let end = after_headers + end_rel;
    let mut out = Vec::with_capacity(body.len() + new_value.len());
    out.extend_from_slice(&body[..after_headers]);
    out.extend_from_slice(new_value.as_bytes());
    out.extend_from_slice(&body[end..]);
    Some(out)
}

/// Compatibility wrapper used by older unit tests.
#[cfg(test)]
fn rewrite_model_field(body: &[u8], new_model: &str) -> Option<Vec<u8>> {
    let route = ModelRoute {
        model_name: new_model.to_string(),
        backend_ids: vec![],
        fallback_backend_id: None,
        chars_per_token: 4.0,
        first_byte_timeout: std::time::Duration::from_secs(180),
        provider_model_name: new_model.to_string(),
        context_tokens: None,
        max_output_tokens: None,
        price_input_per_mtok_usd: None,
        price_output_per_mtok_usd: None,
        enabled: true,
        provider_key_ref: None,
        auth_mode: "bearer".to_string(),
        protocol: "openai_chat".to_string(),
        provider_key: None,
    };
    rewrite_json_proxy_body(body, &route, ProviderProtocol::OpenAiChat)
}

fn generate_request_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{ts:x}-{seq:x}")
}

fn estimate_tokens_len(body_len: usize, route: &ModelRoute) -> u64 {
    if route.chars_per_token <= 0.0 {
        return 1;
    }
    let estimate = (body_len as f64 / route.chars_per_token).ceil() as u64;
    estimate.max(1)
}

fn build_error(request_id: &str, status: StatusCode, message: &str) -> Response<Body> {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": "invalid_request_error"
        }
    });
    let mut resp = (status, axum::Json(body)).into_response();
    if let Ok(v) = header::HeaderValue::from_str(request_id) {
        resp.headers_mut().insert("x-router-request-id", v);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_head_detects_top_level_stream_options() {
        // Nested/string occurrence KHÔNG được tính là top-level (false-positive cũ).
        let nested = br#"{"model":"x","stream":true,"messages":[{"role":"user","content":"stream_options"}]}"#;
        let h: RequestHead<'_> = serde_json::from_slice(nested).unwrap();
        assert_eq!(h.model, "x");
        assert!(h.stream);
        assert!(h.stream_options.is_none(), "string content must not count");

        let nested_obj = br#"{"model":"x","stream":true,"messages":[{"role":"user","content":"hello","stream_options":{"include_usage":false}}]}"#;
        let h: RequestHead<'_> = serde_json::from_slice(nested_obj).unwrap();
        assert!(h.stream_options.is_none(), "nested object must not count");

        // Top-level stream_options phải được phát hiện.
        let top = br#"{"model":"x","stream":true,"stream_options":{"include_usage":false}}"#;
        let h: RequestHead<'_> = serde_json::from_slice(top).unwrap();
        assert!(
            h.stream_options.is_some(),
            "top-level stream_options must count"
        );
    }

    #[test]
    fn prefix_scan_finds_only_top_level_model_and_stream_false() {
        let body =
            br#"{"messages":[{"model":"nested","stream":true}],"model":"top","stream":false}"#;
        let PrefixScan::Found(h) = scan_head_prefix(body) else {
            panic!("head must be found");
        };
        assert_eq!(h.model, "top");
        assert!(!h.stream);
    }

    #[test]
    fn prefix_scan_does_not_find_missing_stream() {
        let body = br#"{"model":"top","messages":[{"role":"user","content":"x"}]}"#;
        assert_eq!(scan_head_prefix(body), PrefixScan::NeedMore);
    }

    #[test]
    fn multipart_text_field_extracts_model() {
        let body = b"--brighto\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-1\r\n--brighto\r\nContent-Disposition: form-data; name=\"file\"; filename=\"sample.wav\"\r\nContent-Type: audio/wav\r\n\r\nabc\r\n--brighto--\r\n";
        assert_eq!(
            extract_multipart_text_field(body, "model").as_deref(),
            Some("whisper-1")
        );
        assert_eq!(extract_multipart_text_field(body, "missing"), None);
    }

    fn route_for_test(provider_model_name: &str) -> ModelRoute {
        ModelRoute {
            model_name: "public".to_string(),
            backend_ids: vec![],
            fallback_backend_id: None,
            chars_per_token: 4.0,
            first_byte_timeout: std::time::Duration::from_secs(180),
            provider_model_name: provider_model_name.to_string(),
            context_tokens: None,
            max_output_tokens: None,
            price_input_per_mtok_usd: None,
            price_output_per_mtok_usd: None,
            enabled: true,
            provider_key_ref: None,
            auth_mode: "bearer".to_string(),
            protocol: "openai_chat".to_string(),
            provider_key: None,
        }
    }

    #[test]
    fn voyage_rerank_rewrite_maps_top_n_to_top_k() {
        let body = br#"{"model":"public-rerank","query":"router speed","documents":["fast","slow"],"top_n":1}"#;
        let route = route_for_test("rerank-2.5-lite");
        let out = rewrite_json_proxy_body(body, &route, ProviderProtocol::VoyageRerank)
            .expect("rewrite voyage rerank");
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["model"], "rerank-2.5-lite");
        assert_eq!(v["top_k"], 1);
        assert!(v.get("top_n").is_none());
    }

    #[test]
    fn qwen_text_rerank_rewrite_uses_dashscope_shape() {
        let body = br#"{"model":"public-rerank","query":"router speed","documents":["fast","slow"],"top_n":1}"#;
        let route = route_for_test("qwen3.7-text-rerank");
        let out = rewrite_json_proxy_body(body, &route, ProviderProtocol::QwenRerank)
            .expect("rewrite qwen rerank");
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["model"], "qwen3.7-text-rerank");
        assert_eq!(v["input"]["query"], "router speed");
        assert_eq!(v["input"]["documents"][0], "fast");
        assert_eq!(v["parameters"]["top_n"], 1);
        assert!(v.get("query").is_none());
    }

    #[test]
    fn qwen3_rerank_keeps_flat_compatible_shape() {
        let body = br#"{"model":"public-rerank","query":"router speed","documents":["fast","slow"],"top_n":1}"#;
        let route = route_for_test("qwen3-rerank");
        let out = rewrite_json_proxy_body(body, &route, ProviderProtocol::QwenRerank)
            .expect("rewrite qwen3 rerank");
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["model"], "qwen3-rerank");
        assert_eq!(v["query"], "router speed");
        assert_eq!(v["top_n"], 1);
        assert!(v.get("input").is_none());
    }

    #[test]
    fn qwen_rerank_uri_depends_on_model_family() {
        assert_eq!(
            qwen_rerank_upstream_uri("qwen3-rerank").unwrap().path(),
            "/compatible-api/v1/reranks"
        );
        assert_eq!(
            qwen_rerank_upstream_uri("qwen3.7-text-rerank")
                .unwrap()
                .path(),
            "/api/v1/services/rerank/text-rerank/text-rerank"
        );
    }

    #[test]
    fn multipart_text_field_rewrite_changes_model_alias() {
        let body = b"--brighto\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\npublic-asr\r\n--brighto\r\nContent-Disposition: form-data; name=\"file\"; filename=\"sample.wav\"\r\nContent-Type: audio/wav\r\n\r\nabc\r\n--brighto--\r\n";
        let out = rewrite_multipart_text_field(body, "model", "whisper-1").expect("rewrite");
        let text = std::str::from_utf8(&out).unwrap();
        assert!(text.contains("name=\"model\"\r\n\r\nwhisper-1"));
        assert!(text.contains("filename=\"sample.wav\""));
        assert!(!text.contains("public-asr"));
    }

    #[test]
    fn request_ids_are_unique() {
        let id1 = generate_request_id();
        let id2 = generate_request_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn readiness_flips_on_stale_success_and_error() {
        // Chưa từng load thành công -> 503.
        assert_eq!(
            readiness_status(0, 0, 5_000, 0),
            StatusCode::SERVICE_UNAVAILABLE
        );
        // Vừa load ok -> 200.
        assert_eq!(readiness_status(1_000, 0, 5_000, 1_000), StatusCode::OK);
        // Reload treo (ok cũ hơn cửa sổ stale) -> 503 dù chưa có err.
        assert_eq!(
            readiness_status(1_000, 0, 5_000, 6_500),
            StatusCode::SERVICE_UNAVAILABLE
        );
        // Reload lỗi mới hơn ok -> 503.
        assert_eq!(
            readiness_status(1_000, 2_000, 5_000, 2_500),
            StatusCode::SERVICE_UNAVAILABLE
        );
        // Phục hồi: ok mới hơn err -> 200.
        assert_eq!(readiness_status(3_000, 2_000, 5_000, 3_000), StatusCode::OK);
    }

    #[test]
    fn rewrite_model_field_swaps_top_level_model() {
        let body =
            br#"{"model":"public","stream":false,"messages":[{"role":"user","content":"hi"}]}"#;
        let out = rewrite_model_field(body, "provider-real-model").expect("rewrite");
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["model"], "provider-real-model");
        // Các field khác giữ nguyên.
        assert_eq!(v["stream"], false);
        assert_eq!(v["messages"][0]["content"], "hi");
    }

    #[test]
    fn estimate_tokens_positive() {
        let route = ModelRoute {
            model_name: "test".to_string(),
            backend_ids: vec![],
            fallback_backend_id: None,
            chars_per_token: 4.0,
            first_byte_timeout: std::time::Duration::from_secs(180),
            provider_model_name: "test".to_string(),
            context_tokens: None,
            max_output_tokens: None,
            price_input_per_mtok_usd: None,
            price_output_per_mtok_usd: None,
            enabled: true,
            provider_key_ref: None,
            auth_mode: "bearer".to_string(),
            protocol: "openai_chat".to_string(),
            provider_key: None,
        };
        let body = Bytes::from_static(b"hello world");
        let est = estimate_tokens_len(body.len(), &route);
        assert!(est > 0);
    }
}
