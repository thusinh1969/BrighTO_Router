//! brighto-router-mock — upstream giả cho tầng A/B/E của BENCHMARK.md.
//! Trả lời ~0 ms để không che overhead của router. Điều khiển hành vi bằng header:
//!   x-mock-first-byte-delay-ms  : giả prefill (mặc định 0)
//!   x-mock-chunk-delay-ms       : giả tốc độ decode (mặc định 0)
//!   x-mock-chunks               : số chunk stream (mặc định 64)
//!   x-mock-fail-after-chunks    : đóng kết nối sau N chunk (giả backend chết giữa stream)
//!   x-mock-status               : trả thẳng status này (503, 429, ...) không body
//!   x-mock-no-usage             : không trả usage (test ước lượng)
//!   x-mock-hang                 : nhận rồi im (test first-byte timeout)
//! Endpoint: POST /v1/chat/completions (OpenAI), POST /v1/messages (Anthropic), GET /v1/models, GET /health
//! Thống kê để test so sánh: GET /_stats → {"requests":N,"prompt_tokens_total":..,"completion_tokens_total":..}
use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Json, Response,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use futures::{StreamExt, stream};
use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Default)]
struct Stats {
    requests: AtomicU64,
    prompt: AtomicU64,
    completion: AtomicU64,
}
type S = Arc<Stats>;

fn h(headers: &HeaderMap, k: &str) -> Option<u64> {
    headers.get(k)?.to_str().ok()?.parse().ok()
}

#[tokio::main]
async fn main() {
    let addr: SocketAddr = std::env::var("MOCK_ADDR")
        .unwrap_or("0.0.0.0:9000".into())
        .parse()
        .unwrap();
    let st: S = Arc::new(Stats::default());
    let app = Router::new()
        .route("/v1/chat/completions", post(openai))
        .route("/v1/messages", post(anthropic))
        .route("/v1/models", get(|| async { Json(serde_json::json!({"object":"list","data":[{"id":"mock-model","object":"model","owned_by":"mock"}]})) }))
        .route("/health", get(|| async { "ok" }))
        .route("/_stats", get(|State(s): State<S>| async move {
            Json(serde_json::json!({"requests": s.requests.load(Ordering::Relaxed),
                "prompt_tokens_total": s.prompt.load(Ordering::Relaxed),
                "completion_tokens_total": s.completion.load(Ordering::Relaxed)}))
        }))
        .with_state(st);
    let l = tokio::net::TcpListener::bind(addr).await.unwrap();
    eprintln!("brighto-router-mock listening on {addr}");
    axum::serve(l, app).await.unwrap();
}

/// Đếm token = body_len / 4. Deterministic, để test so Σ ledger == Σ mock.
fn count(body: &Bytes) -> u64 {
    (body.len() as u64 / 4).max(1)
}

async fn common(
    headers: &HeaderMap,
    st: &S,
    body: &Bytes,
) -> Result<(u64, u64, u64, u64, Option<u64>, bool), Box<Response>> {
    if headers.contains_key("x-mock-hang") {
        std::future::pending::<()>().await;
    }
    if let Some(code) = h(headers, "x-mock-status") {
        return Err(Box::new(
            StatusCode::from_u16(code as u16)
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
                .into_response(),
        ));
    }
    let first = h(headers, "x-mock-first-byte-delay-ms").unwrap_or(0);
    let chunk_delay = h(headers, "x-mock-chunk-delay-ms").unwrap_or(0);
    let chunks = h(headers, "x-mock-chunks").unwrap_or(64);
    let fail_after = h(headers, "x-mock-fail-after-chunks");
    let no_usage = headers.contains_key("x-mock-no-usage");
    let p = count(body);
    if first > 0 {
        tokio::time::sleep(Duration::from_millis(first)).await;
    }
    st.requests.fetch_add(1, Ordering::Relaxed);
    st.prompt.fetch_add(p, Ordering::Relaxed);
    st.completion.fetch_add(chunks, Ordering::Relaxed);
    Ok((p, chunks, chunk_delay, first, fail_after, no_usage))
}

fn is_stream(body: &Bytes) -> bool {
    #[derive(serde::Deserialize)]
    struct P {
        #[serde(default)]
        stream: bool,
    }
    serde_json::from_slice::<P>(body)
        .map(|p| p.stream)
        .unwrap_or(false)
}

async fn openai(State(st): State<S>, headers: HeaderMap, body: Bytes) -> Response {
    let (p, n, delay, _, fail_after, no_usage) = match common(&headers, &st, &body).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !is_stream(&body) {
        let mut j = serde_json::json!({"id":"chatcmpl-mock","object":"chat.completion","model":"mock-model",
            "choices":[{"index":0,"message":{"role":"assistant","content":"x".repeat(n as usize * 4)},"finish_reason":"stop"}]});
        if !no_usage {
            j["usage"] =
                serde_json::json!({"prompt_tokens":p,"completion_tokens":n,"total_tokens":p+n});
        }
        return Json(j).into_response();
    }
    let events = (0..=n + 1)
        .map(move |i| (i, n, p, no_usage, fail_after))
        .collect::<Vec<_>>();
    let s = stream::iter(events).then(move |(i, n, p, no_usage, fail_after)| async move {
        if delay > 0 && i > 0 { tokio::time::sleep(Duration::from_millis(delay)).await; }
        if let Some(f) = fail_after && i >= f { std::process::abort(); } // đóng kết nối thô, không [DONE]
        let e = if i < n {
            Event::default().data(serde_json::json!({"id":"chatcmpl-mock","object":"chat.completion.chunk","model":"mock-model",
                "choices":[{"index":0,"delta":{"content":"tok "},"finish_reason":null}]}).to_string())
        } else if i == n {
            let mut j = serde_json::json!({"id":"chatcmpl-mock","object":"chat.completion.chunk","model":"mock-model","choices":[]});
            if !no_usage { j["usage"] = serde_json::json!({"prompt_tokens":p,"completion_tokens":n,"total_tokens":p+n}); }
            Event::default().data(j.to_string())
        } else {
            Event::default().data("[DONE]")
        };
        Ok::<_, Infallible>(e)
    });
    Sse::new(s).into_response()
}

async fn anthropic(State(st): State<S>, headers: HeaderMap, body: Bytes) -> Response {
    let (p, n, delay, _, fail_after, _) = match common(&headers, &st, &body).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !is_stream(&body) {
        return Json(serde_json::json!({"id":"msg_mock","type":"message","role":"assistant","model":"mock-model",
            "content":[{"type":"text","text":"x".repeat(n as usize * 4)}],"stop_reason":"end_turn",
            "usage":{"input_tokens":p,"output_tokens":n}})).into_response();
    }
    let events = (0..n + 4).collect::<Vec<u64>>();
    let s = stream::iter(events).then(move |i| async move {
        if delay > 0 && i > 1 { tokio::time::sleep(Duration::from_millis(delay)).await; }
        if let Some(f) = fail_after && i >= f + 2 { std::process::abort(); }
        let (ev, data) = match i {
            0 => ("message_start", serde_json::json!({"type":"message_start","message":{"id":"msg_mock","type":"message","role":"assistant","model":"mock-model","content":[],"usage":{"input_tokens":p,"output_tokens":0}}})),
            1 => ("content_block_start", serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})),
            k if k < n + 2 => ("content_block_delta", serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"tok "}})),
            k if k == n + 2 => ("message_delta", serde_json::json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":n}})),
            _ => ("message_stop", serde_json::json!({"type":"message_stop"})),
        };
        Ok::<_, Infallible>(Event::default().event(ev).data(data.to_string()))
    });
    Sse::new(s).into_response()
}
