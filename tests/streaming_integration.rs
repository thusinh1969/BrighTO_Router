//! Integration test: full pipeline (auth -> budget -> route -> proxy stream -> tap usage)
//! với mock backend SSE + Postgres thật (qua #[sqlx::test]). Chứng minh streaming thật + tap usage + header verify.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use axum::body::{Body, Bytes, to_bytes};
use axum::http::{HeaderMap, Request};
use futures::StreamExt;
use sqlx::postgres::PgPool;
use tower::ServiceExt;

use brighto_router::budget::RamBudgetStore;
use brighto_router::contract::{AppState, UsageEvent};
use brighto_router::ledger::LedgerSink;
use brighto_router::metrics::Metrics;
use brighto_router::route::RamBackendPool;

fn metrics() -> Metrics {
    static M: std::sync::OnceLock<Metrics> = std::sync::OnceLock::new();
    M.get_or_init(Metrics::install).clone()
}

/// Admin router fail-fast đòi ADMIN_MASTER_KEY; test set 1 lần cho cả process.
fn set_test_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Rust 2024: set_var là unsafe; chỉ set cố định 1 lần, không đọc giá trị khác trong test.
        unsafe {
            std::env::set_var("ADMIN_MASTER_KEY", "test-admin");
        }
    });
}

/// Mock backend trả SSE theo body cho trước.
async fn spawn_mock_backend(body: &'static str) -> String {
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(move || async move {
            axum::response::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .body(Body::from(body))
                .unwrap()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

#[derive(Debug)]
struct CapturedUpload {
    content_length: Option<String>,
    transfer_encoding: Option<String>,
    body_len: usize,
}

async fn spawn_model_capture_backend(
    backend_name: &'static str,
    tx: tokio::sync::mpsc::Sender<(String, String)>,
) -> String {
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(move |body: Bytes| {
            let tx = tx.clone();
            async move {
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let model = body["model"].as_str().unwrap().to_string();
                tx.send((backend_name.to_string(), model)).await.unwrap();
                axum::response::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"id":"cmpl-group","choices":[{"message":{"role":"assistant","content":"ok"}}],"usage":{"prompt_tokens":2,"completion_tokens":3}}"#,
                    ))
                    .unwrap()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

async fn spawn_capture_backend() -> (String, tokio::sync::oneshot::Receiver<CapturedUpload>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(move |headers: HeaderMap, body: Body| {
            let tx = tx.clone();
            async move {
                let body = to_bytes(body, 10 * 1024 * 1024).await.unwrap();
                let captured = CapturedUpload {
                    content_length: headers
                        .get("content-length")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string),
                    transfer_encoding: headers
                        .get("transfer-encoding")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string),
                    body_len: body.len(),
                };
                if let Some(tx) = tx.lock().await.take() {
                    let _ = tx.send(captured);
                }
                axum::response::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"id":"cmpl-test","choices":[],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#,
                    ))
                    .unwrap()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), rx)
}

const SSE_WITH_USAGE: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":20}}\n\ndata: [DONE]\n\n";
const SSE_NO_USAGE: &str =
    "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: [DONE]\n\n";

async fn spawn_adapter_backend() -> String {
    let app = axum::Router::new()
        .route(
            "/v1/embeddings",
            axum::routing::post(|body: Bytes| async move {
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["model"], "mock-embedding");
                axum::Json(serde_json::json!({
                    "object":"list",
                    "model":"mock-embedding",
                    "data":[{"object":"embedding","index":0,"embedding":[0.1,0.2,0.3]}],
                    "usage":{"prompt_tokens":3,"total_tokens":3}
                }))
            }),
        )
        .route(
            "/v1/rerank",
            axum::routing::post(|body: Bytes| async move {
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["model"], "mock-rerank");
                assert_eq!(body["query"], "router speed");
                axum::Json(serde_json::json!({
                    "id":"rerank-test",
                    "results":[{"index":0,"relevance_score":0.98}],
                    "usage":{"prompt_tokens":9,"total_tokens":9}
                }))
            }),
        )
        .route(
            "/v1/audio/transcriptions",
            axum::routing::post(|body: Bytes| async move {
                assert!(
                    std::str::from_utf8(&body)
                        .unwrap()
                        .contains("name=\"model\"\r\n\r\nmock-asr")
                );
                axum::Json(serde_json::json!({
                    "text":"mock transcription ok",
                    "usage":{"prompt_tokens":4,"total_tokens":4}
                }))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

#[sqlx::test(migrations = "./migrations")]
async fn model_group_round_robin_rewrites_per_endpoint_provider_model(pool: PgPool) {
    set_test_env();
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let deepseek_base = spawn_model_capture_backend("deepseek", tx.clone()).await;
    let local_base = spawn_model_capture_backend("local", tx).await;

    let key_file = std::env::temp_dir().join(format!("brighto_group_key_{}", std::process::id()));
    std::fs::write(&key_file, "deepseek-mock-key").unwrap();
    let key_ref = format!("file:{}", key_file.display());

    sqlx::query(
        "INSERT INTO backends (id, name, base_url, api_key_ref, weight, max_inflight, format, enabled) \
         VALUES (1, 'deepseek-endpoint', $1, $2, 1, 100, 'openai', TRUE), \
                (2, 'local-llamacpp', $3, 'env:NONE', 1, 4, 'openai', TRUE)",
    )
    .bind(&deepseek_base)
    .bind(&key_ref)
    .bind(&local_base)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO model_routes \
         (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, \
          provider_model_name, enabled, auth_mode, protocol, routing_policy) \
         VALUES ('coding-fast', '[1,2]', NULL, 4.0, 180, 'coding-fast', TRUE, 'bearer', 'openai_chat', 'round_robin')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO model_route_endpoints \
         (model_name, backend_id, provider_model_name, provider_key_ref, auth_mode, protocol, weight, max_inflight, enabled) \
         VALUES ('coding-fast', 1, 'deepseek-v4-pro', $1, 'bearer', 'openai_chat', 99, 100, TRUE), \
                ('coding-fast', 2, 'qwen3.8-flash-next', NULL, 'none', 'openai_chat', 1, 4, TRUE)",
    )
    .bind(&key_ref)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO teams (id, name, budget, enabled) VALUES (1, 'team-1', $1, TRUE)")
        .bind(r#"{"period":"day","max_tokens":1000000,"per_model":{}}"#)
        .execute(&pool)
        .await
        .unwrap();
    let key_hash = hex::encode(brighto_router::auth::hash_key("test-key"));
    sqlx::query(
        "INSERT INTO api_keys (id, key_hash, key_prefix, team_id, owner, allowed_models, budget, rpm_limit, concurrency_limit, expires_at, enabled) \
         VALUES (1, $1, 'sk-brigh', 1, 'tester', '[]', NULL, NULL, NULL, NULL, TRUE)",
    )
    .bind(&key_hash)
    .execute(&pool)
    .await
    .unwrap();

    let loader = brighto_router::config::DbConfigLoader::new(pool.clone(), 5);
    let snap = loader.load_snapshot().await.unwrap();
    let cfg = Arc::new(ArcSwap::from_pointee(snap));
    let budget = Arc::new(RamBudgetStore::new());
    budget.load_teams(&cfg.load_full().teams);
    let backends = Arc::new(RamBackendPool::new_with_counter_pool(pool.clone(), 1));
    for b in cfg.load_full().backends.values() {
        backends.upsert_backend(b.clone());
    }
    let (ptx, mut prx) = tokio::sync::mpsc::channel(8192);
    let (otx, mut orx) = tokio::sync::mpsc::channel(8192);
    tokio::spawn(async move { while orx.recv().await.is_some() {} });
    let state = Arc::new(AppState {
        cfg,
        budget,
        backends,
        client: reqwest::Client::new(),
        ledger: LedgerSink::new(ptx, otx),
        metrics: metrics(),
        max_body_bytes: 10 * 1024 * 1024,
        reload_notify: Arc::new(tokio::sync::Notify::new()),
        config_ok_at: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        config_err_at: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        readiness_max_stale_ms: 5_000,
    });
    let app = brighto_router::handlers::router(state);

    for _ in 0..2 {
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("authorization", "Bearer test-key")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"model":"coding-fast","messages":[{"role":"user","content":"hello"}],"stream":false}"#,
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
    }

    let first = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let second = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        first,
        ("deepseek".to_string(), "deepseek-v4-pro".to_string())
    );
    assert_eq!(
        second,
        ("local".to_string(), "qwen3.8-flash-next".to_string())
    );

    let mut ledgers = Vec::new();
    for _ in 0..2 {
        ledgers.push(
            tokio::time::timeout(Duration::from_secs(2), prx.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    assert_eq!(ledgers[0].model, "coding-fast");
    assert_eq!(ledgers[1].model, "coding-fast");
    assert_ne!(ledgers[0].backend_id, ledgers[1].backend_id);
}

#[sqlx::test(migrations = "./migrations")]
async fn embeddings_adapter_proxies_and_records_usage(pool: PgPool) {
    let backend_base = spawn_adapter_backend().await;
    let (state, mut ledger_rx) = build_state_for_route(
        pool,
        backend_base,
        "public-embedding",
        "mock-embedding",
        "openai_embeddings",
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/embeddings")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"public-embedding","input":"hello"}"#,
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["model"], "mock-embedding");
    assert_eq!(json["data"][0]["embedding"].as_array().unwrap().len(), 3);

    let ev = tokio::time::timeout(Duration::from_secs(2), ledger_rx.recv())
        .await
        .expect("ledger event within 2s")
        .expect("ledger event present");
    assert_eq!(ev.model, "public-embedding");
    assert_eq!(ev.input_tokens, 3);
    assert_eq!(ev.output_tokens, 0);
    assert!(!ev.estimated);
}

#[sqlx::test(migrations = "./migrations")]
async fn rerank_adapter_proxies_rewrites_model_and_records_usage(pool: PgPool) {
    let backend_base = spawn_adapter_backend().await;
    let (state, mut ledger_rx) = build_state_for_route(
        pool,
        backend_base,
        "public-rerank",
        "mock-rerank",
        "cohere_rerank",
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/rerank")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"public-rerank","query":"router speed","documents":["fast rust router","slow proxy"],"top_n":1}"#,
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["results"][0]["index"], 0);

    let ev = tokio::time::timeout(Duration::from_secs(2), ledger_rx.recv())
        .await
        .expect("ledger event within 2s")
        .expect("ledger event present");
    assert_eq!(ev.model, "public-rerank");
    assert_eq!(ev.input_tokens, 9);
    assert_eq!(ev.output_tokens, 0);
    assert!(!ev.estimated);
}

#[sqlx::test(migrations = "./migrations")]
async fn asr_adapter_proxies_multipart_and_records_usage(pool: PgPool) {
    let backend_base = spawn_adapter_backend().await;
    let (state, mut ledger_rx) = build_state_for_route(
        pool,
        backend_base,
        "mock-asr",
        "mock-asr",
        "openai_audio_transcriptions",
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let boundary = "----brighto-test";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nmock-asr\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"sample.wav\"\r\nContent-Type: audio/wav\r\n\r\nabc\r\n--{boundary}--\r\n"
    );
    let req = Request::builder()
        .method("POST")
        .uri("/v1/audio/transcriptions")
        .header("authorization", "Bearer test-key")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["text"], "mock transcription ok");

    let ev = tokio::time::timeout(Duration::from_secs(2), ledger_rx.recv())
        .await
        .expect("ledger event within 2s")
        .expect("ledger event present");
    assert_eq!(ev.model, "mock-asr");
    assert_eq!(ev.input_tokens, 4);
    assert_eq!(ev.output_tokens, 0);
    assert!(!ev.estimated);
}

async fn build_state(
    pool: PgPool,
    backend_base: String,
) -> (Arc<AppState>, tokio::sync::mpsc::Receiver<UsageEvent>) {
    build_state_for_route(
        pool,
        backend_base,
        "test-model",
        "test-model",
        "openai_chat",
    )
    .await
}

async fn build_state_for_route(
    pool: PgPool,
    backend_base: String,
    public_model: &str,
    provider_model: &str,
    protocol: &str,
) -> (Arc<AppState>, tokio::sync::mpsc::Receiver<UsageEvent>) {
    set_test_env();
    // key file cho backend (không dùng env để test không phụ thuộc môi trường).
    let key_file = std::env::temp_dir().join(format!("brighto_it_key_{}", std::process::id()));
    std::fs::write(&key_file, "mockkey").unwrap();
    let key_ref = format!("file:{}", key_file.display());

    sqlx::query(
        "INSERT INTO backends (id, name, base_url, api_key_ref, weight, max_inflight, format, enabled) \
         VALUES (1, 'backend-1', $1, $2, 1, 100, 'openai', TRUE)",
    )
    .bind(&backend_base)
    .bind(&key_ref)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO model_routes (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, provider_model_name, protocol) \
         VALUES ($1, '[1]', NULL, 4.0, 180, $2, $3)",
    )
    .bind(public_model)
    .bind(provider_model)
    .bind(protocol)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO teams (id, name, budget, enabled) \
         VALUES (1, 'team-1', '{\"period\":\"day\",\"max_tokens\":1000000,\"per_model\":{}}', TRUE)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let key_hash = hex::encode(brighto_router::auth::hash_key("test-key"));
    sqlx::query(
        "INSERT INTO api_keys (id, key_hash, key_prefix, team_id, owner, allowed_models, budget, rpm_limit, concurrency_limit, expires_at, enabled) \
         VALUES (1, $1, 'sk-brigh', 1, 'tester', '[]', NULL, NULL, NULL, NULL, TRUE)",
    )
    .bind(&key_hash)
    .execute(&pool)
    .await
    .unwrap();

    let loader = brighto_router::config::DbConfigLoader::new(pool, 5);
    let snap = loader.load_snapshot().await.unwrap();
    let cfg = Arc::new(ArcSwap::from_pointee(snap));

    let budget = Arc::new(RamBudgetStore::new());
    budget.load_teams(&cfg.load_full().teams);
    let backends = Arc::new(RamBackendPool::new());
    for b in cfg.load_full().backends.values() {
        backends.upsert_backend(b.clone());
    }

    let client = reqwest::Client::new();
    let (ptx, prx) = tokio::sync::mpsc::channel(8192);
    let (otx, orx) = tokio::sync::mpsc::channel(8192);
    let ledger = LedgerSink::new(ptx, otx);
    tokio::spawn(async move {
        let mut o = orx;
        while o.recv().await.is_some() {}
    });

    let state = Arc::new(AppState {
        cfg,
        budget,
        backends,
        client,
        ledger,
        metrics: metrics(),
        max_body_bytes: 10 * 1024 * 1024,
        reload_notify: Arc::new(tokio::sync::Notify::new()),
        config_ok_at: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        config_err_at: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        readiness_max_stale_ms: 5_000,
    });
    (state, prx)
}

#[sqlx::test(migrations = "./migrations")]
async fn stream_request_taps_usage_and_forwards_sse(pool: PgPool) {
    let backend_base = spawn_mock_backend(SSE_WITH_USAGE).await;
    let (state, mut ledger_rx) = build_state(pool, backend_base).await;

    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"test-model","stream":true,"messages":[{"role":"user","content":"hi"}]}"#,
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(
        resp.headers()
            .get("x-router-backend")
            .unwrap()
            .to_str()
            .unwrap(),
        "backend-1"
    );
    assert!(resp.headers().contains_key("x-router-overhead-ms"));
    assert!(resp.headers().contains_key("x-router-request-id"));

    let body = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("data: [DONE]"), "SSE body forwarded: {text}");

    let ev = tokio::time::timeout(Duration::from_secs(2), ledger_rx.recv())
        .await
        .expect("ledger event within 2s")
        .expect("ledger event present");
    assert_eq!(ev.input_tokens, 10);
    assert_eq!(ev.output_tokens, 20);
    assert!(!ev.estimated);
    assert_eq!(ev.status, 200);
    assert!(!ev.client_aborted);
}

#[sqlx::test(migrations = "./migrations")]
async fn stream_without_usage_records_estimate_not_zero(pool: PgPool) {
    let backend_base = spawn_mock_backend(SSE_NO_USAGE).await;
    let (state, mut ledger_rx) = build_state(pool, backend_base).await;

    let app = brighto_router::handlers::router(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"test-model","stream":true,"messages":[{"role":"user","content":"hello world, this is a long enough prompt for a non-zero estimate"}]}"#,
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("data: [DONE]"), "SSE body forwarded: {text}");

    let ev = tokio::time::timeout(Duration::from_secs(2), ledger_rx.recv())
        .await
        .expect("ledger event within 2s")
        .expect("ledger event present");
    assert!(ev.estimated);
    assert!(ev.input_tokens > 0, "estimate must be non-zero: {ev:?}");
    assert_eq!(ev.output_tokens, 0);
}

/// Mock backend returns a non-stream JSON body in two chunks, with delayed EOF.
async fn spawn_delayed_nonstream_backend() -> String {
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(|| async move {
            let body = futures::stream::unfold(0u8, |state| async move {
                match state {
                    0 => Some((
                        Ok::<Bytes, std::convert::Infallible>(Bytes::from_static(
                            br#"{"id":"cmpl-test","choices":[{"message":{"content":"hi"}}],"#,
                        )),
                        1,
                    )),
                    1 => {
                        tokio::time::sleep(Duration::from_millis(800)).await;
                        Some((
                            Ok::<Bytes, std::convert::Infallible>(Bytes::from_static(
                                br#""usage":{"prompt_tokens":11,"completion_tokens":7}}"#,
                            )),
                            2,
                        ))
                    }
                    _ => None,
                }
            });
            axum::response::Response::builder()
                .status(200)
                .header("content-type", "application/json")
                .body(Body::from_stream(body))
                .unwrap()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

#[sqlx::test(migrations = "./migrations")]
async fn large_nonstream_upload_preserves_exact_content_length(pool: PgPool) {
    let (backend_base, capture_rx) = spawn_capture_backend().await;
    let (state, _ledger_rx) = build_state(pool, backend_base).await;
    let app = brighto_router::handlers::router(state);

    let payload = format!(
        r#"{{"model":"test-model","stream":false,"messages":[{{"role":"user","content":"{}"}}]}}"#,
        "x".repeat(96 * 1024)
    );
    let len = payload.len();
    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .header("content-length", len.to_string())
        .body(Body::from(payload))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let captured = tokio::time::timeout(Duration::from_secs(2), capture_rx)
        .await
        .expect("backend capture within 2s")
        .expect("backend capture sent");
    assert_eq!(captured.body_len, len);
    assert_eq!(
        captured.content_length.as_deref(),
        Some(len.to_string().as_str())
    );
    assert!(
        captured.transfer_encoding.is_none(),
        "exact-length fast path must not use chunked transfer: {captured:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn nonstream_response_starts_before_upstream_eof(pool: PgPool) {
    let backend_base = spawn_delayed_nonstream_backend().await;
    let (state, mut ledger_rx) = build_state(pool, backend_base).await;

    let app = brighto_router::handlers::router(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"test-model","stream":false,"messages":[{"role":"user","content":"hi"}]}"#,
        ))
        .unwrap();

    let start = Instant::now();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert!(
        start.elapsed() < Duration::from_millis(400),
        "router buffered non-stream response before returning headers/body: {:?}",
        start.elapsed()
    );

    let mut body_stream = resp.into_body().into_data_stream();
    let first = tokio::time::timeout(Duration::from_millis(400), body_stream.next())
        .await
        .expect("first non-stream chunk before upstream EOF")
        .expect("first chunk item")
        .expect("first chunk bytes");
    assert!(
        first.starts_with(br#"{"id":"cmpl-test""#),
        "unexpected first chunk: {:?}",
        first
    );

    let mut body = first.to_vec();
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(2), body_stream.next())
        .await
        .expect("next body chunk")
    {
        body.extend_from_slice(&chunk.expect("body chunk bytes"));
    }
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains(r#""usage":{"prompt_tokens":11,"completion_tokens":7}"#));

    let ev = tokio::time::timeout(Duration::from_secs(2), ledger_rx.recv())
        .await
        .expect("ledger event within 2s")
        .expect("ledger event present");
    assert_eq!(ev.input_tokens, 11);
    assert_eq!(ev.output_tokens, 7);
    assert!(!ev.estimated);
    assert_eq!(ev.status, 200);
    assert!(!ev.stream);
    assert!(!ev.client_aborted);
}
