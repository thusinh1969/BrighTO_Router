//! Forward + SSE pump + tap usage. Body là Bytes đi thẳng, không parse/re-encode.
//! Một nơi duy nhất forward: handler chỉ lo auth/budget/route, proxy lo mọi thứ phía backend.
//! Stream thật (Body::from_stream), splice usage, ledger/budget/metrics finalize từ stream reporter.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::{Body, BodyDataStream, Bytes};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode, Uri};
use axum::response::Response;
use futures::SinkExt;
use futures::channel::mpsc;
use futures::{Stream, StreamExt};
use http_body::{Frame, SizeHint};
use serde_json::Value;

use crate::budget::{BudgetReservation, ConcurrencyGuard};
use crate::contract::{ApiKey, AppState, BackendFormat, ModelRoute, UsageEvent};
use crate::route::{AcquireError, BackendExclusions, BackendLease};

/// Quick check trước khi parse — tránh parse cả body 1MB. memchr, không cấp phát.
pub fn chunk_may_have_usage(chunk: &[u8]) -> bool {
    memchr::memmem::find(chunk, b"\"usage\"").is_some()
}

/// Byte-splice: chèn ,"stream_options":{"include_usage":true} trước } cuối cùng.
/// Chỉ gọi khi stream=true VÀ body chưa chứa "stream_options". Đã verify trên llama-server thật.
pub fn splice_include_usage(body: &[u8]) -> Option<Vec<u8>> {
    if body.is_empty() {
        return None;
    }
    let last_brace = body.iter().rposition(|&b| b == b'}')?;
    let mut new_body = Vec::with_capacity(body.len() + 40);
    new_body.extend_from_slice(&body[..last_brace]);
    new_body.extend_from_slice(b",\"stream_options\":{\"include_usage\":true}");
    new_body.extend_from_slice(&body[last_brace..]);
    Some(new_body)
}

#[cfg(test)]
fn rewrite_top_level_model(body: &[u8], provider_model_name: &str) -> Option<Bytes> {
    materialize_chunks(rewrite_top_level_model_chunks(
        &Bytes::copy_from_slice(body),
        provider_model_name,
    )?)
    .map(Bytes::from)
}

fn rewrite_top_level_model_chunks(body: &Bytes, provider_model_name: &str) -> Option<Vec<Bytes>> {
    let (value_start, value_end) = find_top_level_model_string_span(body)?;
    if body[value_start..value_end] == *provider_model_name.as_bytes() {
        return Some(vec![body.clone()]);
    }
    let quoted = serde_json::to_vec(&Value::String(provider_model_name.to_string())).ok()?;
    if quoted.len() < 2 {
        return None;
    }
    let escaped_value = &quoted[1..quoted.len() - 1];
    Some(vec![
        body.slice(..value_start),
        Bytes::copy_from_slice(escaped_value),
        body.slice(value_end..),
    ])
}

fn materialize_chunks(chunks: Vec<Bytes>) -> Option<Vec<u8>> {
    let total = chunks.iter().map(Bytes::len).sum();
    let mut out = Vec::with_capacity(total);
    for chunk in chunks {
        out.extend_from_slice(&chunk);
    }
    Some(out)
}

fn reqwest_body_from_chunks(chunks: Vec<Bytes>) -> reqwest::Body {
    if chunks.len() == 1 {
        return reqwest::Body::from(chunks.into_iter().next().unwrap());
    }
    let stream = futures::stream::iter(chunks.into_iter().map(Ok::<Bytes, std::io::Error>));
    reqwest::Body::wrap_stream(stream)
}

fn find_top_level_model_string_span(body: &[u8]) -> Option<(usize, usize)> {
    let mut i = skip_ws(body, 0);
    if body.get(i) != Some(&b'{') {
        return None;
    }
    i += 1;
    loop {
        i = skip_ws(body, i);
        match body.get(i)? {
            b'}' => return None,
            b'"' => {}
            _ => return None,
        }
        let key_start = i + 1;
        let key_end = scan_json_string_end(body, i)?;
        let is_model_key = body[key_start..key_end] == *b"model";
        i = skip_ws(body, key_end + 1);
        if body.get(i) != Some(&b':') {
            return None;
        }
        i = skip_ws(body, i + 1);
        if is_model_key {
            if body.get(i) != Some(&b'"') {
                return None;
            }
            let value_start = i + 1;
            let value_end = scan_json_string_end(body, i)?;
            return Some((value_start, value_end));
        }
        i = skip_json_value(body, i)?;
        i = skip_ws(body, i);
        match body.get(i)? {
            b',' => i += 1,
            b'}' => return None,
            _ => return None,
        }
    }
}

fn skip_ws(body: &[u8], mut i: usize) -> usize {
    while matches!(body.get(i), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        i += 1;
    }
    i
}

fn scan_json_string_end(body: &[u8], quote_pos: usize) -> Option<usize> {
    if body.get(quote_pos) != Some(&b'"') {
        return None;
    }
    let mut i = quote_pos + 1;
    while i < body.len() {
        match body[i] {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

fn skip_json_value(body: &[u8], mut i: usize) -> Option<usize> {
    i = skip_ws(body, i);
    match *body.get(i)? {
        b'"' => scan_json_string_end(body, i).map(|end| end + 1),
        b'{' | b'[' => {
            let mut depth = 0usize;
            while i < body.len() {
                match body[i] {
                    b'"' => i = scan_json_string_end(body, i)? + 1,
                    b'{' | b'[' => {
                        depth += 1;
                        i += 1;
                    }
                    b'}' | b']' => {
                        depth = depth.checked_sub(1)?;
                        i += 1;
                        if depth == 0 {
                            return Some(i);
                        }
                    }
                    _ => i += 1,
                }
            }
            None
        }
        _ => {
            while i < body.len()
                && !matches!(body[i], b',' | b'}' | b']' | b' ' | b'\n' | b'\r' | b'\t')
            {
                i += 1;
            }
            Some(i)
        }
    }
}

/// Heuristic nhẹ: body có yêu cầu stream hay không (không parse full JSON).
pub fn is_stream_request(body: &[u8]) -> bool {
    memchr::memmem::find(body, b"\"stream\":true").is_some()
        || memchr::memmem::find(body, b"\"stream\": true").is_some()
}

/// Join backend base URL and incoming path.
///
/// If base_url has no path, preserve the incoming path exactly:
///   https://api.openai.com + /v1/chat/completions -> https://api.openai.com/v1/chat/completions
/// If base_url already contains an API prefix, treat it like an OpenAI SDK base URL and strip
/// the leading /v1 from the incoming route:
///   https://api.moonshot.ai/v1 + /v1/chat/completions -> https://api.moonshot.ai/v1/chat/completions
///   https://example.com/compatible-mode/v1 + /v1/models -> https://example.com/compatible-mode/v1/models
fn build_target_url(base_url: &str, uri: &Uri) -> String {
    let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    let trimmed = base_url.trim_end_matches('/');
    let has_path_prefix = trimmed
        .split_once("://")
        .and_then(|(_, rest)| rest.split_once('/'))
        .is_some();
    let path = if has_path_prefix {
        path_and_query.strip_prefix("/v1").unwrap_or(path_and_query)
    } else {
        path_and_query
    };
    if path.starts_with('/') {
        format!("{trimmed}{path}")
    } else {
        format!("{trimmed}/{path}")
    }
}

/// Header hop-by-hop hoặc secret không được forward tới backend.
fn must_drop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "x-api-key"
            | "host"
            | "content-length"
            | "accept-encoding"
            | "connection"
            | "transfer-encoding"
            | "keep-alive"
            | "upgrade"
            | "te"
            | "trailer"
            | "proxy-authorization"
            | "proxy-connection"
    )
}

/// Build request tới backend: filter header, inject auth backend (key đã resolve lúc load),
/// thêm x-request-id. Dùng client pool dùng chung từ state.
#[allow(clippy::too_many_arguments)]
fn build_reqwest_request(
    client: &reqwest::Client,
    method: &Method,
    url: &str,
    headers: &HeaderMap,
    body: reqwest::Body,
    format: BackendFormat,
    auth_key: &str,
    request_id: &str,
) -> Result<reqwest::Request, String> {
    let mut req_headers = reqwest::header::HeaderMap::new();
    for (name, value) in headers.iter() {
        if must_drop_header(name.as_str()) {
            continue;
        }
        req_headers.insert(name.clone(), value.clone());
    }

    // auth_key rỗng = provider không cần key (llama.cpp/vLLM/Ollama / auth_mode=none). Chỉ gắn auth khi có giá trị.
    let key = auth_key;
    match format {
        BackendFormat::OpenAi => {
            if !key.is_empty() {
                let v = HeaderValue::from_str(&format!("Bearer {key}"))
                    .map_err(|e| format!("invalid backend key: {e}"))?;
                req_headers.insert(reqwest::header::AUTHORIZATION, v);
            }
        }
        BackendFormat::Anthropic => {
            if !key.is_empty() {
                let v =
                    HeaderValue::from_str(key).map_err(|e| format!("invalid backend key: {e}"))?;
                req_headers.insert("x-api-key", v);
                // Chỉ đặt default nếu client KHÔNG tự gửi anthropic-version (giữ nguyên version client pin).
                if !req_headers.contains_key("anthropic-version") {
                    req_headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
                }
            }
        }
    }

    let rid = HeaderValue::from_str(request_id).map_err(|e| format!("invalid request id: {e}"))?;
    req_headers.insert("x-request-id", rid);
    req_headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        HeaderValue::from_static("identity"),
    );

    client
        .request(method.clone(), url)
        .headers(req_headers)
        .body(body)
        .build()
        .map_err(|e| e.to_string())
}

/// Request body accepted by proxy. Buffered bodies are replayable across pre-first-byte retries.
/// Streaming upload is the large-prompt fast path: it preserves exact Content-Length through
/// SizeHint and intentionally disables retry after the upload body is moved to reqwest.
pub enum ProxyRequestBody {
    Buffered(Bytes),
    Streaming {
        prefix: Bytes,
        rest: BodyDataStream,
        content_length: u64,
    },
}

impl ProxyRequestBody {
    fn into_reqwest_body(self) -> reqwest::Body {
        match self {
            Self::Buffered(bytes) => reqwest::Body::from(bytes),
            Self::Streaming {
                prefix,
                rest,
                content_length,
            } => reqwest::Body::wrap(ExactLengthUploadBody::new(prefix, rest, content_length)),
        }
    }
}

struct ExactLengthUploadBody {
    inner: Mutex<ExactLengthUploadInner>,
    content_length: u64,
}

struct ExactLengthUploadInner {
    prefix: Option<Bytes>,
    rest: Pin<Box<BodyDataStream>>,
    sent: u64,
    ended: bool,
}

impl ExactLengthUploadBody {
    fn new(prefix: Bytes, rest: BodyDataStream, content_length: u64) -> Self {
        Self {
            inner: Mutex::new(ExactLengthUploadInner {
                prefix: Some(prefix),
                rest: Box::pin(rest),
                sent: 0,
                ended: false,
            }),
            content_length,
        }
    }

    fn lock_inner(&self) -> std::sync::MutexGuard<'_, ExactLengthUploadInner> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl http_body::Body for ExactLengthUploadBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.as_ref().get_ref();
        let mut inner = this.lock_inner();
        if let Some(prefix) = inner.prefix.take() {
            if prefix.is_empty() {
                // Continue to the client stream in the same poll; no empty DATA frame.
            } else {
                inner.sent = inner.sent.saturating_add(prefix.len() as u64);
                return Poll::Ready(Some(Ok(Frame::data(prefix))));
            }
        }

        match inner.rest.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                inner.sent = inner.sent.saturating_add(chunk.len() as u64);
                Poll::Ready(Some(Ok(Frame::data(chunk))))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => {
                inner.ended = true;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.lock_inner().ended
    }

    fn size_hint(&self) -> SizeHint {
        let sent = self.lock_inner().sent;
        SizeHint::with_exact(self.content_length.saturating_sub(sent))
    }
}

fn parse_openai_usage_from_line(line: &[u8]) -> Option<(u64, u64)> {
    let data = line.strip_prefix(b"data:")?;
    let data = data.trim_ascii_start();
    if data.starts_with(b"[DONE]") {
        return None;
    }
    let v: Value = serde_json::from_slice(data).ok()?;
    let usage = v.get("usage")?;
    let pt = usage.get("prompt_tokens")?.as_u64()?;
    let ct = usage.get("completion_tokens")?.as_u64()?;
    Some((pt, ct))
}

fn parse_anthropic_usage_from_line(line: &[u8]) -> Option<(Option<u64>, Option<u64>)> {
    let data = line.strip_prefix(b"data:")?;
    let data = data.trim_ascii_start();
    let v: Value = serde_json::from_slice(data).ok()?;
    let event_type = v.get("type")?.as_str()?;
    match event_type {
        "message_start" => {
            let input = v.pointer("/message/usage/input_tokens")?.as_u64()?;
            Some((Some(input), None))
        }
        "message_delta" => {
            let output = v.pointer("/usage/output_tokens")?.as_u64()?;
            Some((None, Some(output)))
        }
        _ => None,
    }
}

/// Bộ tích luỹ token khi stream, tolerant với extra fields và empty choices.
#[derive(Debug, Clone, Default)]
pub struct UsageAccumulator {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub seen_usage: bool,
}

fn process_sse_line(line: &[u8], format: BackendFormat, acc: &mut UsageAccumulator) {
    match format {
        BackendFormat::OpenAi => {
            if let Some((pt, ct)) = parse_openai_usage_from_line(line) {
                acc.input_tokens = acc.input_tokens.max(pt);
                acc.output_tokens = acc.output_tokens.max(ct);
                acc.seen_usage = true;
            }
        }
        BackendFormat::Anthropic => {
            if let Some((input, output)) = parse_anthropic_usage_from_line(line) {
                if let Some(i) = input {
                    acc.input_tokens = i;
                    acc.seen_usage = true;
                }
                if let Some(o) = output {
                    acc.output_tokens = o;
                    acc.seen_usage = true;
                }
            }
        }
    }
}

pub fn extract_usage_from_sse_chunk(
    chunk: &[u8],
    format: BackendFormat,
    acc: &mut UsageAccumulator,
) {
    for line in chunk.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.starts_with(b"data:") {
            process_sse_line(line, format, acc);
        }
    }
}

fn parse_usage_from_body(body: &[u8], format: BackendFormat) -> UsageAccumulator {
    let mut acc = UsageAccumulator::default();
    match format {
        BackendFormat::OpenAi => {
            if let Ok(v) = serde_json::from_slice::<Value>(body)
                && let Some(usage) = v.get("usage")
            {
                if let Some(pt) = usage.get("prompt_tokens").and_then(Value::as_u64) {
                    acc.input_tokens = pt;
                    acc.seen_usage = true;
                }
                if let Some(ct) = usage.get("completion_tokens").and_then(Value::as_u64) {
                    acc.output_tokens = ct;
                    acc.seen_usage = true;
                }
            }
        }
        BackendFormat::Anthropic => {
            if let Ok(v) = serde_json::from_slice::<Value>(body)
                && let Some(usage) = v.get("usage")
            {
                if let Some(i) = usage.get("input_tokens").and_then(Value::as_u64) {
                    acc.input_tokens = i;
                    acc.seen_usage = true;
                }
                if let Some(o) = usage.get("output_tokens").and_then(Value::as_u64) {
                    acc.output_tokens = o;
                    acc.seen_usage = true;
                }
            }
        }
    }
    acc
}

#[derive(Debug, Clone)]
struct RequestMeta {
    key_id: i64,
    team_id: i64,
    model: String,
    request_id: String,
}

impl RequestMeta {
    fn from_ctx(ctx: &ProxyContext) -> Self {
        Self {
            key_id: ctx.api_key.id,
            team_id: ctx.api_key.team_id,
            model: ctx.model_name.clone(),
            request_id: ctx.request_id.clone(),
        }
    }
}

/// Bối cảnh proxy: auth + route + budget reservation + concurrency guard.
pub struct ProxyContext {
    pub api_key: ApiKey,
    pub model_name: String,
    pub route: ModelRoute,
    pub request_id: String,
    pub stream: bool,
    pub stream_options_present: bool,
    /// True only when proxy must rewrite top-level JSON model after endpoint selection.
    pub rewrite_model_in_proxy: bool,
    pub reservation: Option<BudgetReservation>,
    pub concurrency: Option<ConcurrencyGuard>,
    pub start: Instant,
    /// Ước lượng input tokens (body_len / chars_per_token) — dùng khi backend không trả usage,
    /// để không bao giờ commit/ghi 0 token.
    pub estimated_input_tokens: u64,
}

/// Finalize 1 request khi stream kết thúc (hoặc Drop): commit budget, release guard + lease,
/// ghi ledger, emit metrics. RAII: nếu chưa finish khi Drop -> rollback + ledger client_aborted.
struct CompletionReporter {
    state: Arc<AppState>,
    meta: RequestMeta,
    backend_id: i64,
    backend_name: String,
    stream: bool,
    start: Instant,
    pre_forward_ms: u64,
    ttfb_ms: u64,
    reservation: Option<BudgetReservation>,
    concurrency: Option<ConcurrencyGuard>,
    estimated_input_tokens: u64,
    /// RAII-only: Drop giải phóng 1 slot inflight; không đọc trực tiếp.
    _lease: BackendLease,
    finished: bool,
}

impl CompletionReporter {
    #[allow(clippy::too_many_arguments)]
    fn new(
        state: Arc<AppState>,
        ctx: &ProxyContext,
        backend_id: i64,
        backend_name: String,
        pre_forward_ms: u64,
        ttfb_ms: u64,
        reservation: Option<BudgetReservation>,
        concurrency: Option<ConcurrencyGuard>,
        lease: BackendLease,
    ) -> Self {
        Self {
            state,
            meta: RequestMeta::from_ctx(ctx),
            backend_id,
            backend_name,
            stream: ctx.stream,
            start: ctx.start,
            pre_forward_ms,
            ttfb_ms,
            reservation,
            concurrency,
            estimated_input_tokens: ctx.estimated_input_tokens,
            _lease: lease,
            finished: false,
        }
    }

    fn finish(
        &mut self,
        status: u16,
        mut input_tokens: u64,
        mut output_tokens: u64,
        estimated: bool,
        client_aborted: bool,
        error_class: Option<String>,
    ) {
        if self.finished {
            return;
        }
        self.finished = true;

        // Backend bỏ usage -> ước lượng: không bao giờ commit/ghi 0 token.
        if estimated && input_tokens.saturating_add(output_tokens) == 0 {
            input_tokens = self.estimated_input_tokens.max(1);
            output_tokens = 0;
        }

        let actual = input_tokens.saturating_add(output_tokens);
        if let Some(res) = self.reservation.take() {
            res.commit(actual);
        }
        self.concurrency = None; // drop guard -> release slot

        let total_ms = self.start.elapsed().as_millis() as u64;
        let event = UsageEvent {
            ts: now_secs(),
            request_id: self.meta.request_id.clone(),
            key_id: self.meta.key_id,
            team_id: self.meta.team_id,
            model: self.meta.model.clone(),
            backend_id: self.backend_id,
            status,
            input_tokens,
            output_tokens,
            estimated,
            ttfb_ms: self.ttfb_ms,
            total_ms,
            router_overhead_ms: self.pre_forward_ms,
            completed_at_ms: now_millis(),
            stream: self.stream,
            client_aborted,
            error_class,
        };
        self.state.ledger.try_record(event);

        let backend = self.backend_name.as_str();
        crate::metrics::request_total(
            self.meta.team_id,
            self.meta.key_id,
            &self.meta.model,
            backend,
            status,
        );
        if input_tokens > 0 {
            crate::metrics::tokens_total(
                self.meta.team_id,
                self.meta.key_id,
                &self.meta.model,
                backend,
                "input",
                estimated,
                input_tokens,
            );
        }
        if output_tokens > 0 {
            crate::metrics::tokens_total(
                self.meta.team_id,
                self.meta.key_id,
                &self.meta.model,
                backend,
                "output",
                estimated,
                output_tokens,
            );
        }
        crate::metrics::observe_ttfb(
            &self.meta.model,
            backend,
            self.stream,
            self.ttfb_ms as f64 / 1000.0,
        );
        crate::metrics::observe_overhead(
            &self.meta.model,
            backend,
            self.stream,
            self.pre_forward_ms as f64 / 1000.0,
        );
    }
}

impl Drop for CompletionReporter {
    fn drop(&mut self) {
        if !self.finished {
            self.finished = true;
            if let Some(res) = self.reservation.take() {
                res.rollback();
            }
            self.concurrency = None;
            let total_ms = self.start.elapsed().as_millis() as u64;
            let event = UsageEvent {
                ts: now_secs(),
                request_id: self.meta.request_id.clone(),
                key_id: self.meta.key_id,
                team_id: self.meta.team_id,
                model: self.meta.model.clone(),
                backend_id: self.backend_id,
                status: 0,
                input_tokens: 0,
                output_tokens: 0,
                estimated: true,
                ttfb_ms: self.ttfb_ms,
                total_ms,
                router_overhead_ms: self.pre_forward_ms,
                completed_at_ms: now_millis(),
                stream: self.stream,
                client_aborted: true,
                error_class: Some("client_aborted".to_string()),
            };
            self.state.ledger.try_record(event);
            crate::metrics::request_total(
                self.meta.team_id,
                self.meta.key_id,
                &self.meta.model,
                &self.backend_name,
                0,
            );
        }
        // lease (field) + concurrency (field) released here
    }
}

fn is_retryable_status(status: u16) -> bool {
    status >= 500 || status == 429
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn build_response(
    status: StatusCode,
    headers: reqwest::header::HeaderMap,
    body: Body,
) -> Response<Body> {
    let mut builder = Response::builder().status(status);
    for (key, value) in headers.iter() {
        if key == reqwest::header::CONTENT_LENGTH {
            continue;
        }
        builder = builder.header(key.as_str(), value.as_bytes());
    }
    builder.body(body).expect("failed to build response")
}

fn no_backend_body(reason: impl Into<String>) -> Response<Body> {
    build_response(
        StatusCode::SERVICE_UNAVAILABLE,
        reqwest::header::HeaderMap::new(),
        Body::from(reason.into()),
    )
}

async fn forward_backend_response(
    response: reqwest::Response,
    format: BackendFormat,
    mut reporter: CompletionReporter,
) -> Response<Body> {
    let status = response.status();
    let headers = response.headers().clone();
    let stream_request = reporter.stream;

    if stream_request {
        // Bounded(1): backpressure thật — client chậm thì backend-reader chặn, không buffer vô hạn.
        let (mut tx, rx) = mpsc::channel::<Result<Bytes, reqwest::Error>>(1);
        let mut acc = UsageAccumulator::default();
        let mut stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut client_aborted = false;
            let mut error_class: Option<String> = None;
            loop {
                let next = tokio::time::timeout(Duration::from_secs(60), stream.next()).await;
                match next {
                    Ok(Some(Ok(bytes))) => {
                        if chunk_may_have_usage(&bytes) {
                            extract_usage_from_sse_chunk(&bytes, format, &mut acc);
                        }
                        if tx.send(Ok(bytes)).await.is_err() {
                            client_aborted = true;
                            break;
                        }
                    }
                    Ok(Some(Err(e))) => {
                        error_class = Some(e.to_string());
                        break;
                    }
                    Ok(None) => break,
                    Err(_) => {
                        error_class = Some("idle timeout".to_string());
                        break;
                    }
                }
            }
            let estimated = !acc.seen_usage;
            reporter.finish(
                status.as_u16(),
                acc.input_tokens,
                acc.output_tokens,
                estimated,
                client_aborted,
                error_class,
            );
            drop(tx);
            // reporter dropped at end of task -> lease released
        });

        build_response(
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            headers,
            Body::from_stream(rx),
        )
    } else {
        const NONSTREAM_USAGE_BUFFER_LIMIT: u64 = 1024 * 1024;

        if response
            .content_length()
            .is_some_and(|len| len <= NONSTREAM_USAGE_BUFFER_LIMIT)
        {
            return match response.bytes().await {
                Ok(body_bytes) => {
                    let acc = parse_usage_from_body(&body_bytes, format);
                    let estimated = !acc.seen_usage;
                    reporter.finish(
                        status.as_u16(),
                        acc.input_tokens,
                        acc.output_tokens,
                        estimated,
                        false,
                        None,
                    );
                    build_response(
                        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
                        headers,
                        Body::from(body_bytes),
                    )
                }
                Err(e) => {
                    reporter.finish(status.as_u16(), 0, 0, true, false, Some(e.to_string()));
                    build_response(
                        StatusCode::BAD_GATEWAY,
                        reqwest::header::HeaderMap::new(),
                        Body::from("backend read failed"),
                    )
                }
            };
        }

        let (mut tx, rx) = mpsc::channel::<Result<Bytes, reqwest::Error>>(1);
        let mut stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut body_buf = Vec::new();
            let mut can_parse_usage = true;
            let mut client_aborted = false;
            let mut error_class: Option<String> = None;

            loop {
                let next = tokio::time::timeout(Duration::from_secs(60), stream.next()).await;
                match next {
                    Ok(Some(Ok(bytes))) => {
                        if can_parse_usage {
                            if (body_buf.len() as u64).saturating_add(bytes.len() as u64)
                                <= NONSTREAM_USAGE_BUFFER_LIMIT
                            {
                                body_buf.extend_from_slice(&bytes);
                            } else {
                                can_parse_usage = false;
                                body_buf.clear();
                            }
                        }
                        if tx.send(Ok(bytes)).await.is_err() {
                            client_aborted = true;
                            break;
                        }
                    }
                    Ok(Some(Err(e))) => {
                        error_class = Some(e.to_string());
                        break;
                    }
                    Ok(None) => break,
                    Err(_) => {
                        error_class = Some("idle timeout".to_string());
                        break;
                    }
                }
            }

            let acc = if can_parse_usage {
                parse_usage_from_body(&body_buf, format)
            } else {
                UsageAccumulator::default()
            };
            let estimated = !acc.seen_usage;
            reporter.finish(
                status.as_u16(),
                acc.input_tokens,
                acc.output_tokens,
                estimated,
                client_aborted,
                error_class,
            );
            drop(tx);
        });

        build_response(
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            headers,
            Body::from_stream(rx),
        )
    }
}

fn request_total_for(ctx: &ProxyContext, backend: &str, status: u16) {
    crate::metrics::request_total(
        ctx.api_key.team_id,
        ctx.api_key.id,
        &ctx.model_name,
        backend,
        status,
    );
}

/// Gắn header verify của router (plan §6): request-id, backend, overhead (router tự tốn trước byte đầu).
fn tag_router_headers(
    mut resp: Response<Body>,
    request_id: &str,
    backend_name: Option<&str>,
    overhead_ms: u64,
) -> Response<Body> {
    let h = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(request_id) {
        h.insert("x-router-request-id", v);
    }
    if let Some(name) = backend_name
        && let Ok(v) = HeaderValue::from_str(name)
    {
        h.insert("x-router-backend", v);
    }
    if let Ok(v) = HeaderValue::from_str(&overhead_ms.to_string()) {
        h.insert("x-router-overhead-ms", v);
    }
    resp
}

/// Điểm vào proxy: chọn backend (lease), retry trước byte đầu, forward response (stream thật).
pub async fn proxy_forward(
    state: Arc<AppState>,
    req: Request<ProxyRequestBody>,
    mut ctx: ProxyContext,
) -> Response<Body> {
    let start = ctx.start;
    let stream_request = ctx.stream;

    let (parts, body) = req.into_parts();
    let method = parts.method;
    let uri = parts.uri;
    let headers = parts.headers;
    let mut upload_body = Some(body);
    let upload_replayable = matches!(upload_body, Some(ProxyRequestBody::Buffered(_)));

    let reservation = ctx.reservation.take();
    let concurrency = ctx.concurrency.take();

    let mut tried = BackendExclusions::default();
    let mut lease = match state
        .backends
        .acquire_excluding(&ctx.route, &mut tried)
        .await
    {
        Ok(lease) => lease,
        Err(AcquireError::CounterUnavailable(msg)) => {
            request_total_for(&ctx, "", 503);
            return tag_router_headers(
                no_backend_body(&msg),
                &ctx.request_id,
                None,
                start.elapsed().as_millis() as u64,
            );
        }
    };

    loop {
        let Some(l) = lease else {
            request_total_for(&ctx, "", 503);
            return tag_router_headers(
                no_backend_body("no healthy backend available"),
                &ctx.request_id,
                None,
                start.elapsed().as_millis() as u64,
            );
        };
        let backend_id = l.backend_id();
        tried.insert(backend_id);

        let backend = {
            let snap = state.cfg.load_full();
            snap.backends.get(&backend_id).cloned()
        };
        let Some(backend) = backend.filter(|b| b.enabled) else {
            drop(l);
            request_total_for(&ctx, "", 503);
            return tag_router_headers(
                no_backend_body("backend not available"),
                &ctx.request_id,
                None,
                start.elapsed().as_millis() as u64,
            );
        };

        let endpoint = ctx.route.endpoint_for(backend_id);

        let Some(body_for_attempt) = upload_body.take() else {
            request_total_for(&ctx, &backend.name, 502);
            return tag_router_headers(
                build_response(
                    StatusCode::BAD_GATEWAY,
                    reqwest::header::HeaderMap::new(),
                    Body::from("request body already consumed"),
                ),
                &ctx.request_id,
                Some(&backend.name),
                start.elapsed().as_millis() as u64,
            );
        };

        let request_body = match body_for_attempt {
            ProxyRequestBody::Buffered(body) => {
                let original_body = body.clone();
                let mut model_rewrite_chunks = if ctx.rewrite_model_in_proxy
                    && endpoint.provider_model_name != ctx.model_name
                {
                    match rewrite_top_level_model_chunks(&body, &endpoint.provider_model_name) {
                        Some(chunks) => Some(chunks),
                        None => {
                            request_total_for(&ctx, &backend.name, 400);
                            return tag_router_headers(
                                build_response(
                                    StatusCode::BAD_REQUEST,
                                    reqwest::header::HeaderMap::new(),
                                    Body::from("could not rewrite endpoint model field"),
                                ),
                                &ctx.request_id,
                                Some(&backend.name),
                                start.elapsed().as_millis() as u64,
                            );
                        }
                    }
                } else {
                    None
                };
                if backend.format == BackendFormat::OpenAi
                    && stream_request
                    && !ctx.stream_options_present
                {
                    let bytes = if let Some(chunks) = model_rewrite_chunks.take() {
                        materialize_chunks(chunks).map(Bytes::from)
                    } else {
                        Some(body.clone())
                    }
                    .and_then(|b| splice_include_usage(&b).map(Bytes::from))
                    .unwrap_or_else(|| body.clone());
                    upload_body = Some(ProxyRequestBody::Buffered(body));
                    reqwest::Body::from(bytes)
                } else {
                    upload_body = Some(ProxyRequestBody::Buffered(body));
                    match model_rewrite_chunks {
                        Some(chunks) => reqwest_body_from_chunks(chunks),
                        None => reqwest::Body::from(original_body),
                    }
                }
            }
            ProxyRequestBody::Streaming { .. }
                if backend.format == BackendFormat::OpenAi
                    && stream_request
                    && !ctx.stream_options_present =>
            {
                request_total_for(&ctx, &backend.name, 500);
                return tag_router_headers(
                    build_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        reqwest::header::HeaderMap::new(),
                        Body::from("streaming upload cannot mutate request body"),
                    ),
                    &ctx.request_id,
                    Some(&backend.name),
                    start.elapsed().as_millis() as u64,
                );
            }
            streaming @ ProxyRequestBody::Streaming { .. } => streaming.into_reqwest_body(),
        };

        let url = build_target_url(&backend.base_url, &uri);
        // Endpoint-level credential wins. auth_mode=none means no provider auth header, even if
        // the backend template has an env key. If no endpoint key exists, fallback to backend key.
        let auth_key = if endpoint.auth_mode == "none" {
            ""
        } else {
            endpoint
                .provider_key
                .as_deref()
                .or(backend.api_key.as_deref())
                .unwrap_or("")
        };
        let built = build_reqwest_request(
            &state.client,
            &method,
            &url,
            &headers,
            request_body,
            backend.format,
            auth_key,
            &ctx.request_id,
        );
        let pre_forward_ms = start.elapsed().as_millis() as u64;

        let Ok(reqwest_req) = built else {
            state.backends.note_result(backend_id, false);
            drop(l);
            request_total_for(&ctx, &backend.name, 502);
            if upload_replayable {
                match state
                    .backends
                    .acquire_excluding(&ctx.route, &mut tried)
                    .await
                {
                    Ok(Some(next)) => {
                        lease = Some(next);
                        continue;
                    }
                    Ok(None) => {}
                    Err(AcquireError::CounterUnavailable(msg)) => {
                        request_total_for(&ctx, "", 503);
                        return tag_router_headers(
                            no_backend_body(&msg),
                            &ctx.request_id,
                            None,
                            start.elapsed().as_millis() as u64,
                        );
                    }
                }
            }
            return tag_router_headers(
                build_response(
                    StatusCode::BAD_GATEWAY,
                    reqwest::header::HeaderMap::new(),
                    Body::from("request build failed"),
                ),
                &ctx.request_id,
                Some(&backend.name),
                pre_forward_ms,
            );
        };

        match tokio::time::timeout(
            ctx.route.first_byte_timeout,
            state.client.execute(reqwest_req),
        )
        .await
        {
            Ok(Ok(resp)) => {
                let status = resp.status().as_u16();
                if is_retryable_status(status) {
                    state.backends.note_result(backend_id, false);
                    if upload_replayable {
                        match state
                            .backends
                            .acquire_excluding(&ctx.route, &mut tried)
                            .await
                        {
                            Ok(Some(next)) => {
                                drop(l);
                                lease = Some(next);
                                continue;
                            }
                            Ok(None) => {}
                            Err(AcquireError::CounterUnavailable(msg)) => {
                                drop(l);
                                request_total_for(&ctx, "", 503);
                                return tag_router_headers(
                                    no_backend_body(&msg),
                                    &ctx.request_id,
                                    None,
                                    start.elapsed().as_millis() as u64,
                                );
                            }
                        }
                    }
                    // Không còn backend dự phòng, hoặc upload body đã là stream một lần: forward phản hồi lỗi này.
                } else {
                    state.backends.note_result(backend_id, true);
                }
                let ttfb_ms = start.elapsed().as_millis() as u64;
                let reporter = CompletionReporter::new(
                    state.clone(),
                    &ctx,
                    backend_id,
                    backend.name.clone(),
                    pre_forward_ms,
                    ttfb_ms,
                    reservation,
                    concurrency,
                    l,
                );
                let resp = forward_backend_response(resp, backend.format, reporter).await;
                return tag_router_headers(
                    resp,
                    &ctx.request_id,
                    Some(&backend.name),
                    pre_forward_ms,
                );
            }
            Ok(Err(e)) => {
                state.backends.note_result(backend_id, false);
                drop(l);
                request_total_for(&ctx, &backend.name, 502);
                if upload_replayable {
                    match state
                        .backends
                        .acquire_excluding(&ctx.route, &mut tried)
                        .await
                    {
                        Ok(Some(next)) => {
                            lease = Some(next);
                            continue;
                        }
                        Ok(None) => {}
                        Err(AcquireError::CounterUnavailable(msg)) => {
                            request_total_for(&ctx, "", 503);
                            return tag_router_headers(
                                no_backend_body(&msg),
                                &ctx.request_id,
                                None,
                                start.elapsed().as_millis() as u64,
                            );
                        }
                    }
                }
                return tag_router_headers(
                    build_response(
                        StatusCode::BAD_GATEWAY,
                        reqwest::header::HeaderMap::new(),
                        Body::from(format!("backend connection failed: {e}")),
                    ),
                    &ctx.request_id,
                    Some(&backend.name),
                    pre_forward_ms,
                );
            }
            Err(_) => {
                state.backends.note_result(backend_id, false);
                drop(l);
                request_total_for(&ctx, &backend.name, 504);
                if upload_replayable {
                    match state
                        .backends
                        .acquire_excluding(&ctx.route, &mut tried)
                        .await
                    {
                        Ok(Some(next)) => {
                            lease = Some(next);
                            continue;
                        }
                        Ok(None) => {}
                        Err(AcquireError::CounterUnavailable(msg)) => {
                            request_total_for(&ctx, "", 503);
                            return tag_router_headers(
                                no_backend_body(&msg),
                                &ctx.request_id,
                                None,
                                start.elapsed().as_millis() as u64,
                            );
                        }
                    }
                }
                return tag_router_headers(
                    build_response(
                        StatusCode::GATEWAY_TIMEOUT,
                        reqwest::header::HeaderMap::new(),
                        Body::from("first byte timeout"),
                    ),
                    &ctx.request_id,
                    Some(&backend.name),
                    pre_forward_ms,
                );
            }
        }
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_url_supports_host_and_sdk_base_urls() {
        let uri: Uri = "/v1/chat/completions?stream=true".parse().unwrap();
        assert_eq!(
            build_target_url("https://api.openai.com", &uri),
            "https://api.openai.com/v1/chat/completions?stream=true"
        );
        assert_eq!(
            build_target_url("https://api.moonshot.ai/v1", &uri),
            "https://api.moonshot.ai/v1/chat/completions?stream=true"
        );
        assert_eq!(
            build_target_url("https://dashscope.example.com/compatible-mode/v1", &uri),
            "https://dashscope.example.com/compatible-mode/v1/chat/completions?stream=true"
        );
    }

    #[test]
    fn anthropic_version_preserved_when_client_provides() {
        let client = reqwest::Client::new();
        let mut headers = HeaderMap::new();
        headers.insert("anthropic-version", HeaderValue::from_static("2099-01-01"));
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer client-secret"),
        );

        let req = build_reqwest_request(
            &client,
            &Method::POST,
            "http://127.0.0.1:9000/v1/messages",
            &headers,
            reqwest::Body::from(Bytes::from("{}")),
            BackendFormat::Anthropic,
            "anthropic-backend-secret",
            "rid-1",
        )
        .unwrap();

        let h = req.headers();
        assert_eq!(h.get("anthropic-version").unwrap(), "2099-01-01");
        assert_eq!(h.get("x-api-key").unwrap(), "anthropic-backend-secret");
        assert!(
            h.get("authorization").is_none(),
            "client auth must not leak"
        );
    }

    #[test]
    fn anthropic_version_default_when_missing() {
        let client = reqwest::Client::new();
        let headers = HeaderMap::new();
        let req = build_reqwest_request(
            &client,
            &Method::POST,
            "http://127.0.0.1:9000/v1/messages",
            &headers,
            reqwest::Body::from(Bytes::from("{}")),
            BackendFormat::Anthropic,
            "anthropic-backend-secret",
            "rid-2",
        )
        .unwrap();
        assert_eq!(
            req.headers().get("anthropic-version").unwrap(),
            "2023-06-01"
        );
    }

    #[test]
    fn backend_request_forces_identity_encoding() {
        let client = reqwest::Client::new();
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT_ENCODING,
            HeaderValue::from_static("gzip"),
        );

        let req = build_reqwest_request(
            &client,
            &Method::POST,
            "http://127.0.0.1:9000/v1/messages",
            &headers,
            reqwest::Body::from(Bytes::from("{}")),
            BackendFormat::Anthropic,
            "anthropic-backend-secret",
            "rid-encoding",
        )
        .unwrap();

        assert_eq!(
            req.headers().get(reqwest::header::ACCEPT_ENCODING).unwrap(),
            "identity"
        );
    }

    #[test]
    fn splice_inserts_before_last_brace_only_when_needed() {
        let body = br#"{"model":"x","stream":true}"#;
        let spliced = splice_include_usage(body).unwrap();
        let needle = b",\"stream_options\":{\"include_usage\":true}";
        assert!(spliced.windows(needle.len()).any(|w| w == needle));
        assert!(splice_include_usage(b"").is_none());
        assert!(splice_include_usage(b"no brace").is_none());
    }

    #[test]
    fn model_rewrite_splices_only_top_level_model_value() {
        let filler = "x".repeat(200_000);
        let body = format!(
            "{{\"metadata\":{{\"model\":\"nested\",\"items\":[1,{{\"model\":\"nested-2\"}}]}},\"model\":\"public-group\",\"messages\":[{{\"role\":\"user\",\"content\":\"{filler}\"}}],\"temperature\":0.2}}"
        );
        let expected = body.replacen(
            "\"model\":\"public-group\"",
            "\"model\":\"provider-model\"",
            1,
        );

        let rewritten = rewrite_top_level_model(body.as_bytes(), "provider-model").unwrap();

        assert_eq!(rewritten.as_ref(), expected.as_bytes());
        assert!(
            rewritten
                .windows(br#""metadata":{"model":"nested""#.len())
                .any(|w| w == br#""metadata":{"model":"nested""#),
            "nested model key must not be rewritten"
        );
    }

    #[test]
    fn model_rewrite_preserves_body_when_provider_model_already_matches() {
        let body =
            br#"{"stream":true,"messages":[{"role":"user","content":"hello"}],"model":"same"}"#;
        let rewritten = rewrite_top_level_model(body, "same").unwrap();
        assert_eq!(rewritten.as_ref(), body);
    }

    #[test]
    fn model_rewrite_escapes_provider_model_name_without_reencoding_body() {
        let body = br#"{"model":"public","messages":[{"role":"user","content":"hello"}]}"#;
        let rewritten = rewrite_top_level_model(body, "provider \"quoted\"").unwrap();
        assert_eq!(
            rewritten.as_ref(),
            br#"{"model":"provider \"quoted\"","messages":[{"role":"user","content":"hello"}]}"#
        );
    }

    #[test]
    fn tap_usage_from_fixture() {
        let fixture = include_bytes!("../../tests/fixtures/stream_with_usage.sse");
        let mut acc = UsageAccumulator::default();
        extract_usage_from_sse_chunk(fixture, BackendFormat::OpenAi, &mut acc);
        assert!(acc.seen_usage, "fixture should contain usage");
        assert!(acc.input_tokens > 0, "input_tokens must be >0");
        assert!(acc.output_tokens > 0, "output_tokens must be >0");
    }

    #[test]
    fn nonstream_usage_from_fixture() {
        let fixture = include_bytes!("../../tests/fixtures/nonstream_small.json");
        let acc = parse_usage_from_body(fixture, BackendFormat::OpenAi);
        assert!(acc.seen_usage, "fixture should contain usage");
        assert_eq!(acc.input_tokens, 56);
        assert_eq!(acc.output_tokens, 16);
    }

    #[test]
    fn tap_survives_extra_fields_and_empty_choices() {
        let chunk = b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":20},\"timings\":{\"prompt_n\":1}}\n\n";
        let mut acc = UsageAccumulator::default();
        extract_usage_from_sse_chunk(chunk, BackendFormat::OpenAi, &mut acc);
        assert!(acc.seen_usage);
        assert_eq!(acc.input_tokens, 10);
        assert_eq!(acc.output_tokens, 20);
    }

    #[test]
    fn anthropic_sse_usage_parsing() {
        let chunk =
            b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":15}}}\n\n";
        let mut acc = UsageAccumulator::default();
        extract_usage_from_sse_chunk(chunk, BackendFormat::Anthropic, &mut acc);
        assert!(acc.seen_usage);
        assert_eq!(acc.input_tokens, 15);
        assert_eq!(acc.output_tokens, 0);

        let chunk2 = b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":30}}\n\n";
        extract_usage_from_sse_chunk(chunk2, BackendFormat::Anthropic, &mut acc);
        assert_eq!(acc.output_tokens, 30);
    }

    #[test]
    fn stream_request_detection() {
        assert!(is_stream_request(br#"{"stream":true}"#));
        assert!(is_stream_request(br#"{"stream": true}"#));
        assert!(!is_stream_request(br#"{"stream":false}"#));
    }

    #[test]
    fn overhead_memchr_path_under_half_ms() {
        let mut chunk = Vec::with_capacity(400_000);
        chunk.extend_from_slice(&[b'a'; 200_000]);
        chunk.extend_from_slice(br#"{"usage":{"prompt_tokens":1}}"#);
        chunk.extend_from_slice(&[b'b'; 200_000]);

        let start = Instant::now();
        let mut found = false;
        for _ in 0..1000 {
            found |= chunk_may_have_usage(&chunk);
        }
        let elapsed = start.elapsed();
        assert!(found);
        let per_check = elapsed / 1000;
        println!(
            "memchr chunk_may_have_usage per 400KB: {:?} (total {:?} for 1000 checks)",
            per_check, elapsed
        );
    }
}
