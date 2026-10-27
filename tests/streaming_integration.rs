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
use brighto_router::contract::{AppState, BackendFormat, ConfigSnapshot, UsageEvent};
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
            "/v1/responses",
            axum::routing::post(|body: Bytes| async move {
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["model"], "mock-responses");
                assert_eq!(body["input"], "hello responses");
                axum::Json(serde_json::json!({
                    "id":"resp-test",
                    "object":"response",
                    "model":"mock-responses",
                    "output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"OK"}]}],
                    "output_text":"OK",
                    "usage":{"input_tokens":6,"output_tokens":2,"total_tokens":8}
                }))
            }),
        )
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
        )
        .route(
            "/v1/systemone",
            axum::routing::post(|headers: HeaderMap, body: Bytes| async move {
                assert_eq!(
                    headers.get("authorization").and_then(|v| v.to_str().ok()),
                    Some("Bearer mockkey")
                );
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["model"], "mock-systemone");
                assert!(body.get("state").is_some());
                assert!(body.get("questions").is_some());
                axum::Json(serde_json::json!({
                    "id":"systemone-test",
                    "model":"mock-systemone",
                    "answers":{
                        "duplicate_charge":{"noul":0.91},
                        "team":{"choice":"billing","confidence":0.9,"probabilities":{"billing":0.9,"support":0.1}}
                    },
                    "usage":{"input_tokens":7,"output_tokens":0,"total_tokens":7}
                }))
            }),
        )
        .route(
            "/v1/decisions",
            axum::routing::post(|headers: HeaderMap, body: Bytes| async move {
                assert_eq!(
                    headers.get("authorization").and_then(|v| v.to_str().ok()),
                    Some("Bearer mockkey")
                );
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["model"], "mock-systemone");
                axum::Json(serde_json::json!({
                    "id":"decision-test",
                    "model":"mock-systemone",
                    "answers":{"ok":{"noul":0.99}},
                    "usage":{"input_tokens":5,"output_tokens":0,"total_tokens":5}
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
async fn model_group_large_rewrite_preserves_exact_content_length(pool: PgPool) {
    set_test_env();
    let (backend_base, capture_rx) = spawn_capture_backend().await;
    let key_file =
        std::env::temp_dir().join(format!("brighto_group_exact_key_{}", std::process::id()));
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
        "INSERT INTO model_routes \
         (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, \
          provider_model_name, enabled, auth_mode, protocol, routing_policy) \
         VALUES ('group-model', '[1]', NULL, 4.0, 180, 'group-model', TRUE, 'bearer', 'openai_chat', 'round_robin')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO model_route_endpoints \
         (model_name, backend_id, provider_model_name, provider_key_ref, auth_mode, protocol, weight, max_inflight, enabled) \
         VALUES ('group-model', 1, 'provider-model-longer', $1, 'bearer', 'openai_chat', 1, 100, TRUE)",
    )
    .bind(&key_ref)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO teams (id, name, budget, enabled) \
         VALUES (1, 'team-1', $1, TRUE)",
    )
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
    let backends = Arc::new(RamBackendPool::new_with_counter_pool(pool, 1));
    for b in cfg.load_full().backends.values() {
        backends.upsert_backend(b.clone());
    }
    let (ptx, _prx) = tokio::sync::mpsc::channel(8192);
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

    let payload = format!(
        r#"{{"model":"group-model","stream":false,"messages":[{{"role":"user","content":"{}"}}]}}"#,
        "x".repeat(96 * 1024)
    );
    let original_len = payload.len();
    let expected_rewritten_len = original_len - "group-model".len() + "provider-model-longer".len();
    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .header("content-length", original_len.to_string())
        .body(Body::from(payload))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let captured = tokio::time::timeout(Duration::from_secs(2), capture_rx)
        .await
        .expect("backend capture within 2s")
        .expect("backend capture sent");
    assert_eq!(captured.body_len, expected_rewritten_len);
    assert_eq!(
        captured.content_length.as_deref(),
        Some(expected_rewritten_len.to_string().as_str())
    );
    assert!(
        captured.transfer_encoding.is_none(),
        "Model Group rewrite must not switch to chunked transfer: {captured:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn responses_proxy_accepts_sdk_v1_base_url_and_records_usage(pool: PgPool) {
    let backend_base = format!("{}/v1", spawn_adapter_backend().await);
    let (state, mut ledger_rx) = build_state_for_route(
        pool,
        backend_base,
        "public-responses",
        "mock-responses",
        "openai_responses",
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/responses")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"public-responses","input":"hello responses","stream":false}"#,
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["object"], "response");
    assert_eq!(json["output_text"], "OK");

    let ev = tokio::time::timeout(Duration::from_secs(2), ledger_rx.recv())
        .await
        .expect("ledger event within 2s")
        .expect("ledger event present");
    assert_eq!(ev.model, "public-responses");
    assert_eq!(ev.input_tokens, 6);
    assert_eq!(ev.output_tokens, 2);
    assert!(!ev.estimated);
}

#[sqlx::test(migrations = "./migrations")]
async fn responses_connection_probe_leaves_output_limit_unset_and_exposes_provider_error(
    pool: PgPool,
) {
    let mock = axum::Router::new().route(
        "/v1/responses",
        axum::routing::post(|axum::Json(body): axum::Json<serde_json::Value>| async move {
            if body.get("max_output_tokens").is_some() {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    axum::Json(serde_json::json!({"error":{"message":"probe must not force max_output_tokens"}})),
                );
            }
            if body["model"] == "missing-model" {
                return (
                    axum::http::StatusCode::NOT_FOUND,
                    axum::Json(serde_json::json!({"error":{"message":"The requested Responses model does not exist"}})),
                );
            }
            (
                axum::http::StatusCode::OK,
                axum::Json(serde_json::json!({"object":"response","output":[],"usage":{"input_tokens":1,"output_tokens":1}})),
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let (state, _) = build_state(pool, base_url.clone()).await;
    let app = brighto_router::handlers::router(state);
    for (model, expected_ok) in [("valid-model", true), ("missing-model", false)] {
        let mut request = Request::builder()
            .method("POST")
            .uri("/admin/test-connection")
            .header("x-admin-key", "test-admin")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "base_url":base_url,"protocol":"openai_responses","dialect":"openai",
                    "auth_mode":"none","provider_model_name":model
                }))
                .unwrap(),
            ))
            .unwrap();
        request.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:12345".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), 200);
        let body = to_bytes(response.into_body(), 8192).await.unwrap();
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["ok"], expected_ok, "{result}");
        if !expected_ok {
            assert!(
                result["detail"]
                    .as_str()
                    .unwrap()
                    .contains("does not exist"),
                "{result}"
            );
            assert!(!result["detail"].as_str().unwrap().contains("OK"));
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn cloud_probe_preserves_each_protocols_output_token_contract(pool: PgPool) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(String, serde_json::Value)>(12);
    let chat_tx = tx.clone();
    let completion_tx = tx.clone();
    let responses_tx = tx.clone();
    let mock = axum::Router::new()
        .route(
            "/v1/chat/completions",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let tx = chat_tx.clone();
                async move {
                    tx.send(("chat".to_string(), body)).await.unwrap();
                    axum::Json(serde_json::json!({"choices":[{"message":{"role":"assistant","content":"OK"}}]}))
                }
            }),
        )
        .route(
            "/v1/completions",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let tx = completion_tx.clone();
                async move {
                    tx.send(("completion".to_string(), body)).await.unwrap();
                    axum::Json(serde_json::json!({"choices":[{"text":"OK"}]}))
                }
            }),
        )
        .route(
            "/v1/responses",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let tx = responses_tx.clone();
                async move {
                    tx.send(("responses".to_string(), body)).await.unwrap();
                    axum::Json(serde_json::json!({"object":"response","output":[]}))
                }
            }),
        )
        .route(
            "/v1/messages",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let tx = tx.clone();
                async move {
                    tx.send(("messages".to_string(), body)).await.unwrap();
                    axum::Json(serde_json::json!({"content":[{"type":"text","text":"OK"}]}))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let (state, _) = build_state(pool, base_url.clone()).await;
    let app = brighto_router::handlers::router(state);

    for (dialect, protocol, model, path) in [
        ("openai", "openai_chat", "gpt-6.1-sol", "chat"),
        ("openai", "openai_chat", "deepseek-v4-pro", "chat"),
        ("openai", "openai_chat", "kimi-k3", "chat"),
        ("openai", "openai_chat", "qwen3.8-max", "chat"),
        ("openai", "openai_chat", "glm-5", "chat"),
        ("openai", "openai_chat", "gemini-3", "chat"),
        ("openai", "openai_chat", "meta-muse", "chat"),
        (
            "openai",
            "openai_completions",
            "legacy-completion",
            "completion",
        ),
        ("openai", "openai_responses", "gpt-6.1-sol", "responses"),
        (
            "anthropic",
            "anthropic_messages",
            "claude-haiku-5-5",
            "messages",
        ),
    ] {
        let mut request = Request::builder()
            .method("POST")
            .uri("/admin/test-connection")
            .header("x-admin-key", "test-admin")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "base_url":base_url,"protocol":protocol,"dialect":dialect,
                    "auth_mode":"none","provider_model_name":model
                }))
                .unwrap(),
            ))
            .unwrap();
        request.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:12345".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), 200, "{model} {protocol}");
        let data = to_bytes(response.into_body(), 8192).await.unwrap();
        let result: serde_json::Value = serde_json::from_slice(&data).unwrap();
        assert_eq!(result["ok"], true, "{model} {protocol}: {result}");
        let (received_path, body) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received_path, path);
        assert_eq!(body["model"], model);
        if dialect == "anthropic" {
            assert!(body["max_tokens"].as_u64().is_some_and(|v| v > 0));
        } else {
            for field in ["max_tokens", "max_completion_tokens", "max_output_tokens"] {
                assert!(body.get(field).is_none(), "{model} {protocol}: {body}");
            }
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn chat_gateway_preserves_caller_output_limit_or_absence(pool: PgPool) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<serde_json::Value>(2);
    let mock = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
            let tx = tx.clone();
            async move {
                tx.send(body).await.unwrap();
                axum::Json(serde_json::json!({
                    "choices":[{"message":{"role":"assistant","content":"OK"}}],
                    "usage":{"prompt_tokens":1,"completion_tokens":1}
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let (state, _) = build_state_for_route(
        pool,
        base_url,
        "public-model",
        "provider-model",
        "openai_chat",
    )
    .await;
    let app = brighto_router::handlers::router(state);

    for caller_cap in [None, Some(123_i64)] {
        let mut body = serde_json::json!({
            "model":"public-model","messages":[{"role":"user","content":"OK?"}],
            "stream":false
        });
        if let Some(cap) = caller_cap {
            body["max_completion_tokens"] = serde_json::json!(cap);
        }
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("authorization", "Bearer test-key")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), 200);
        let forwarded = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(forwarded["model"], "provider-model");
        assert_eq!(
            forwarded.get("max_completion_tokens"),
            body.get("max_completion_tokens")
        );
        assert!(forwarded.get("max_tokens").is_none());
        assert!(forwarded.get("max_output_tokens").is_none());
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn responses_stream_preserves_tool_events_and_records_terminal_usage(pool: PgPool) {
    const SSE: &str = concat!(
        "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"id\":\"fc_1\",\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"read_file\",\"arguments\":\"\"}}\n\n",
        "event: response.function_call_arguments.delta\ndata: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":0,\"item_id\":\"fc_1\",\"delta\":\"{}\"}\n\n",
        "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"usage\":{\"input_tokens\":23,\"output_tokens\":7}}}\n\n"
    );
    let mock = axum::Router::new().route(
        "/v1/responses",
        axum::routing::post(
            |axum::Json(body): axum::Json<serde_json::Value>| async move {
                assert_eq!(body["model"], "native-responses");
                assert_eq!(body["input"][0]["call_id"], "call_previous");
                assert_eq!(body["tools"][0]["name"], "read_file");
                assert!(body.get("stream_options").is_none());
                let chunks: Vec<_> = SSE
                    .as_bytes()
                    .chunks(7)
                    .map(|chunk| Ok::<_, std::convert::Infallible>(Bytes::copy_from_slice(chunk)))
                    .collect();
                axum::response::Response::builder()
                    .status(200)
                    .header("content-type", "text/event-stream")
                    .body(Body::from_stream(futures::stream::iter(chunks)))
                    .unwrap()
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let (state, mut ledger_rx) = build_state_for_route(
        pool,
        base_url,
        "public-responses",
        "native-responses",
        "openai_responses",
    )
    .await;
    let request = Request::builder().method("POST").uri("/v1/responses")
        .header("authorization", "Bearer test-key").header("content-type", "application/json")
        .body(Body::from(r#"{"model":"public-responses","stream":true,"input":[{"type":"function_call_output","call_id":"call_previous","output":"OK"}],"tools":[{"type":"function","name":"read_file","parameters":{"type":"object"}}]}"#)).unwrap();
    let response = brighto_router::handlers::router(state)
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = to_bytes(response.into_body(), 8192).await.unwrap();
    assert_eq!(
        body.as_ref(),
        SSE.as_bytes(),
        "native tool events must be byte-identical"
    );
    let usage = tokio::time::timeout(Duration::from_secs(2), ledger_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (23, 7));
    assert!(!usage.estimated);
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

#[sqlx::test(migrations = "./migrations")]
async fn systemone_adapter_proxies_rewrites_model_and_records_usage(pool: PgPool) {
    let backend_base = spawn_adapter_backend().await;
    let (state, mut ledger_rx) = build_state_for_route(
        pool,
        backend_base,
        "public-decision",
        "mock-systemone",
        "systemone",
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/systemone")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"public-decision","state":{"message":"charged twice"},"questions":{"duplicate_charge":{"type":"noul","instructions":"Duplicate charge?"},"team":{"type":"choice","instructions":"Team?","criteria":{"billing":"payments","support":"help"}}}}"#,
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["model"], "mock-systemone");
    assert_eq!(json["answers"]["duplicate_charge"]["noul"], 0.91);

    let ev = tokio::time::timeout(Duration::from_secs(2), ledger_rx.recv())
        .await
        .expect("ledger event within 2s")
        .expect("ledger event present");
    assert_eq!(ev.model, "public-decision");
    assert_eq!(ev.input_tokens, 7);
    assert_eq!(ev.output_tokens, 0);
    assert!(!ev.estimated);
}

#[sqlx::test(migrations = "./migrations")]
async fn decisions_alias_accepts_systemone_route(pool: PgPool) {
    let backend_base = spawn_adapter_backend().await;
    let (state, _ledger_rx) = build_state_for_route(
        pool,
        backend_base,
        "public-decision",
        "mock-systemone",
        "systemone",
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/decisions")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"public-decision","state":"ok","questions":{"ok":{"type":"noul","instructions":"OK?"}}}"#,
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
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
    build_state_for_route_flags(
        pool,
        backend_base,
        public_model,
        provider_model,
        protocol,
        false,
    )
    .await
}

async fn build_state_for_route_flags(
    pool: PgPool,
    backend_base: String,
    public_model: &str,
    provider_model: &str,
    protocol: &str,
    accept_anthropic_messages: bool,
) -> (Arc<AppState>, tokio::sync::mpsc::Receiver<UsageEvent>) {
    set_test_env();
    // key file cho backend (không dùng env để test không phụ thuộc môi trường).
    // Parallel sqlx tests each need their own file: a shared path is truncated
    // and rewritten concurrently, so config load can occasionally read it empty.
    static KEY_FILE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let key_file = std::env::temp_dir().join(format!(
        "brighto_it_key_{}_{}",
        std::process::id(),
        KEY_FILE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
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
        "INSERT INTO model_routes (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, provider_model_name, protocol, accept_anthropic_messages) \
         VALUES ($1, '[1]', NULL, 4.0, 180, $2, $3, $4)",
    )
    .bind(public_model)
    .bind(provider_model)
    .bind(protocol)
    .bind(accept_anthropic_messages)
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
async fn large_nonstream_alias_buffers_and_rewrites_provider_model(pool: PgPool) {
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let backend_base = spawn_model_capture_backend("backend-1", tx).await;
    let (state, _ledger_rx) = build_state_for_route(
        pool,
        backend_base,
        "public-alias",
        "provider-model",
        "openai_chat",
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let payload = format!(
        r#"{{"model":"public-alias","stream":false,"messages":[{{"role":"user","content":"{}"}}]}}"#,
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
    assert_eq!(
        resp.status().as_u16(),
        200,
        "large non-stream body with a provider alias must be buffered and rewritten, not rejected"
    );
    let (backend, model) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("backend capture within 2s")
        .expect("backend capture present");
    assert_eq!(backend, "backend-1");
    assert_eq!(model, "provider-model");
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

#[derive(Debug)]
struct BridgeCapture {
    body: serde_json::Value,
    path: String,
    anthropic_version: Option<String>,
    anthropic_beta: Option<String>,
}

async fn spawn_bridge_backend(
    response_body: &'static str,
    content_type: &'static str,
) -> (String, tokio::sync::oneshot::Receiver<BridgeCapture>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(
            move |uri: axum::http::Uri, headers: HeaderMap, body: Bytes| {
                let tx = tx.clone();
                async move {
                    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    let captured = BridgeCapture {
                        body,
                        path: uri
                            .path_and_query()
                            .map(|value| value.as_str().to_string())
                            .unwrap_or_default(),
                        anthropic_version: headers
                            .get("anthropic-version")
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_string),
                        anthropic_beta: headers
                            .get("anthropic-beta")
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_string),
                    };
                    if let Some(tx) = tx.lock().await.take() {
                        let _ = tx.send(captured);
                    }
                    axum::response::Response::builder()
                        .status(200)
                        .header("content-type", content_type)
                        .body(Body::from(response_body))
                        .unwrap()
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), rx)
}

#[sqlx::test(migrations = "./migrations")]
async fn anthropic_bridge_translates_nonstream_request_and_response(pool: PgPool) {
    let (backend_base, capture_rx) = spawn_bridge_backend(
        r#"{"id":"cmpl-bridge","choices":[{"message":{"role":"assistant","content":"Hello from upstream"},"finish_reason":"stop"}],"usage":{"prompt_tokens":11,"completion_tokens":4}}"#,
        "application/json",
    )
    .await;
    let (state, mut ledger_rx) = build_state_for_route_flags(
        pool,
        backend_base,
        "claude-local",
        "provider-model",
        "openai_chat",
        true,
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages?beta=true")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "prompt-caching-2024-07-31")
        .body(Body::from(
            r#"{"model":"claude-local","max_tokens":64,"system":"Be brief","messages":[{"role":"user","content":"Hello"}]}"#,
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("application/json")
    );
    assert!(resp.headers().contains_key("request-id"));
    let body = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["type"], "message");
    assert_eq!(value["role"], "assistant");
    assert_eq!(value["model"], "claude-local");
    assert_eq!(value["content"][0]["type"], "text");
    assert_eq!(value["content"][0]["text"], "Hello from upstream");
    assert_eq!(value["stop_reason"], "end_turn");
    assert_eq!(value["usage"]["input_tokens"], 11);
    assert_eq!(value["usage"]["output_tokens"], 4);

    let captured = capture_rx.await.unwrap();
    assert_eq!(captured.path, "/v1/chat/completions");
    assert_eq!(captured.body["model"], "provider-model");
    assert_eq!(captured.body["max_tokens"], 64);
    assert_eq!(captured.body["stream"], false);
    assert_eq!(captured.body["messages"][0]["role"], "system");
    assert_eq!(captured.body["messages"][0]["content"], "Be brief");
    assert_eq!(captured.body["messages"][1]["role"], "user");
    assert_eq!(captured.body["messages"][1]["content"], "Hello");
    assert!(captured.body.get("stream_options").is_none());
    assert!(captured.anthropic_version.is_none());
    assert!(captured.anthropic_beta.is_none());

    let ev = tokio::time::timeout(Duration::from_secs(2), ledger_rx.recv())
        .await
        .expect("ledger event within 2s")
        .expect("ledger event present");
    assert_eq!(ev.input_tokens, 11);
    assert_eq!(ev.output_tokens, 4);
    assert!(!ev.estimated);
    assert_eq!(ev.status, 200);
    assert!(!ev.stream);
}

const BRIDGE_SSE_TOOL: &str = "data: {\"id\":\"cmpl-stream\",\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"Read\",\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"a.txt\\\"}\"}}]}}]}\n\n\
data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: {\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":5}}\n\n\
data: [DONE]\n\n";

#[sqlx::test(migrations = "./migrations")]
async fn anthropic_bridge_streams_text_and_tool_calls(pool: PgPool) {
    let (backend_base, capture_rx) =
        spawn_bridge_backend(BRIDGE_SSE_TOOL, "text/event-stream").await;
    let (state, mut ledger_rx) = build_state_for_route_flags(
        pool,
        backend_base,
        "claude-stream",
        "claude-stream",
        "openai_chat",
        true,
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .body(Body::from(
            r#"{"model":"claude-stream","max_tokens":64,"stream":true,"messages":[{"role":"user","content":"Read a.txt"}]}"#,
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("text/event-stream")
    );
    let body = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("event: message_start"), "{text}");
    assert!(text.contains("\"text\":\"Hel\""), "{text}");
    assert!(text.contains("\"text\":\"lo\""), "{text}");
    assert!(text.contains("\"type\":\"tool_use\""), "{text}");
    assert!(text.contains("\"id\":\"call_1\""), "{text}");
    assert!(text.contains("input_json_delta"), "{text}");
    assert!(text.contains("\"stop_reason\":\"tool_use\""), "{text}");
    assert!(text.contains("\"output_tokens\":5"), "{text}");
    assert!(text.contains("event: message_stop"), "{text}");
    assert!(!text.contains("data: [DONE]"), "{text}");
    assert!(!text.contains("\"choices\""), "{text}");

    let captured = capture_rx.await.unwrap();
    assert_eq!(captured.body["stream"], true);
    assert_eq!(captured.body["stream_options"]["include_usage"], true);

    let ev = tokio::time::timeout(Duration::from_secs(2), ledger_rx.recv())
        .await
        .expect("ledger event within 2s")
        .expect("ledger event present");
    assert_eq!(ev.input_tokens, 9);
    assert_eq!(ev.output_tokens, 5);
    assert!(!ev.estimated);
    assert_eq!(ev.status, 200);
    assert!(ev.stream);
}

#[sqlx::test(migrations = "./migrations")]
async fn anthropic_bridge_requires_the_route_toggle(pool: PgPool) {
    let (backend_base, _capture_rx) = spawn_bridge_backend(
        r#"{"choices":[{"message":{"role":"assistant","content":"unused"}}]}"#,
        "application/json",
    )
    .await;
    let (state, _ledger_rx) = build_state_for_route_flags(
        pool,
        backend_base,
        "claude-off",
        "claude-off",
        "openai_chat",
        false,
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"claude-off","max_tokens":8,"messages":[{"role":"user","content":"hi"}]}"#,
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 400);
    let body = to_bytes(resp.into_body(), 8192).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["type"], "error");
    assert_eq!(value["error"]["type"], "invalid_request_error");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("does not accept Anthropic Messages")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn anthropic_bridge_translates_upstream_error_envelope(pool: PgPool) {
    let (backend_base, _capture_rx) = spawn_bridge_backend(
        r#"{"error":{"message":"provider rejected key","type":"authentication_error"}}"#,
        "application/json",
    )
    .await;
    // The mock always returns 200; use a dedicated status-aware backend here.
    let status_backend = {
        let app = axum::Router::new().route(
            "/v1/chat/completions",
            axum::routing::post(|| async {
                axum::response::Response::builder()
                    .status(401)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"error":{"message":"provider rejected key","type":"authentication_error"}}"#,
                    ))
                    .unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}")
    };
    let _ = backend_base;
    let (state, _ledger_rx) = build_state_for_route_flags(
        pool,
        status_backend,
        "claude-error",
        "claude-error",
        "openai_chat",
        true,
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"claude-error","max_tokens":8,"messages":[{"role":"user","content":"hi"}]}"#,
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 401);
    let body = to_bytes(resp.into_body(), 8192).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["type"], "error");
    assert_eq!(value["error"]["type"], "authentication_error");
    assert_eq!(value["error"]["message"], "provider rejected key");
}

#[sqlx::test(migrations = "./migrations")]
async fn anthropic_bridge_count_tokens_estimates_without_ledger(pool: PgPool) {
    let (state, mut ledger_rx) = build_state_for_route_flags(
        pool,
        "http://127.0.0.1:9".to_string(),
        "claude-count",
        "provider-count",
        "openai_chat",
        true,
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages/count_tokens")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"claude-count","system":"Be brief","messages":[{"role":"user","content":"Count these words in the request"}],"tools":[{"name":"Read","description":"Read a file","input_schema":{"type":"object","properties":{"path":{"type":"string"}}}}]}"#,
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert!(resp.headers().contains_key("request-id"));
    let body = to_bytes(resp.into_body(), 8192).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        value["input_tokens"]
            .as_u64()
            .is_some_and(|tokens| tokens > 0),
        "{value}"
    );
    let observed = tokio::time::timeout(Duration::from_millis(150), ledger_rx.recv()).await;
    assert!(
        observed.is_err(),
        "count_tokens must not write a usage ledger event: observed={observed:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn anthropic_bridge_group_rewrites_endpoint_provider_model(pool: PgPool) {
    let (tx, mut capture_rx) = tokio::sync::mpsc::channel(4);
    let backend_base = spawn_model_capture_backend("group", tx).await;
    let key_file =
        std::env::temp_dir().join(format!("brighto_group_bridge_key_{}", std::process::id()));
    std::fs::write(&key_file, "mockkey").unwrap();
    let key_ref = format!("file:{}", key_file.display());

    sqlx::query(
        "INSERT INTO backends (id, name, base_url, api_key_ref, weight, max_inflight, format, enabled) \
         VALUES (1, 'group-backend', $1, $2, 1, 100, 'openai', TRUE)",
    )
    .bind(&backend_base)
    .bind(&key_ref)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_routes \
         (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, \
          provider_model_name, enabled, auth_mode, protocol, routing_policy, accept_anthropic_messages) \
         VALUES ('claude-group', '[1]', NULL, 4.0, 180, 'claude-group', TRUE, 'bearer', 'openai_chat', 'round_robin', TRUE)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_route_endpoints \
         (model_name, backend_id, provider_model_name, provider_key_ref, auth_mode, protocol, weight, max_inflight, enabled) \
         VALUES ('claude-group', 1, 'provider-group', $1, 'bearer', 'openai_chat', 1, 100, TRUE)",
    )
    .bind(&key_ref)
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

    let loader = brighto_router::config::DbConfigLoader::new(pool.clone(), 5);
    let snap = loader.load_snapshot().await.unwrap();
    let cfg = Arc::new(ArcSwap::from_pointee(snap));
    let budget = Arc::new(RamBudgetStore::new());
    budget.load_teams(&cfg.load_full().teams);
    let backends = Arc::new(RamBackendPool::new_with_counter_pool(pool.clone(), 1));
    for backend in cfg.load_full().backends.values() {
        backends.upsert_backend(backend.clone());
    }
    let (ptx, _prx) = tokio::sync::mpsc::channel(8192);
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

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"claude-group","max_tokens":32,"messages":[{"role":"user","content":"hello group"}]}"#,
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["type"], "message");
    assert_eq!(value["content"][0]["text"], "ok");

    let captured = tokio::time::timeout(Duration::from_secs(2), capture_rx.recv())
        .await
        .expect("capture within 2s")
        .expect("capture present");
    assert_eq!(
        captured,
        ("group".to_string(), "provider-group".to_string())
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn anthropic_bridge_maps_image_blocks(pool: PgPool) {
    let (backend_base, capture_rx) = spawn_bridge_backend(
        r#"{"choices":[{"message":{"role":"assistant","content":"seen"},"finish_reason":"stop"}]}"#,
        "application/json",
    )
    .await;
    let (state, _ledger_rx) = build_state_for_route_flags(
        pool,
        backend_base,
        "claude-image",
        "provider-image",
        "openai_chat",
        true,
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"claude-image","max_tokens":32,"messages":[{"role":"user","content":[{"type":"text","text":"what is this?"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}]}]}"#,
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let captured = capture_rx.await.unwrap();
    let content = &captured.body["messages"][0]["content"];
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image_url");
    assert_eq!(content[1]["image_url"]["url"], "data:image/png;base64,AAAA");
}

#[sqlx::test(migrations = "./migrations")]
async fn anthropic_bridge_synthesizes_sse_when_upstream_ignores_stream(pool: PgPool) {
    let (backend_base, capture_rx) = spawn_bridge_backend(
        r#"{"id":"cmpl-json","choices":[{"message":{"role":"assistant","content":"fallback text"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2}}"#,
        "application/json",
    )
    .await;
    let (state, _ledger_rx) = build_state_for_route_flags(
        pool,
        backend_base,
        "claude-json-stream",
        "claude-json-stream",
        "openai_chat",
        true,
    )
    .await;
    let app = brighto_router::handlers::router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"claude-json-stream","max_tokens":32,"stream":true,"messages":[{"role":"user","content":"hi"}]}"#,
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("text/event-stream")
    );
    let body = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("event: message_start"), "{text}");
    assert!(text.contains("fallback text"), "{text}");
    assert!(text.contains("event: message_stop"), "{text}");
    let captured = capture_rx.await.unwrap();
    assert_eq!(captured.body["stream"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn anthropic_bridge_rejects_a_non_openai_backend_with_a_clear_error(pool: PgPool) {
    let (backend_base, _capture_rx) = spawn_bridge_backend(
        r#"{"choices":[{"message":{"role":"assistant","content":"unused"}}]}"#,
        "application/json",
    )
    .await;
    let (state, _ledger_rx) = build_state_for_route_flags(
        pool,
        backend_base,
        "claude-bad-backend",
        "claude-bad-backend",
        "openai_chat",
        true,
    )
    .await;
    // Simulate a legacy/misconfigured DB row after the snapshot was loaded.
    let old = state.cfg.load_full();
    let mut backends = old.backends.clone();
    backends.get_mut(&1).unwrap().format = BackendFormat::Anthropic;
    let snapshot = ConfigSnapshot {
        backends,
        routes: old.routes.clone(),
        teams: old.teams.clone(),
        keys_by_hash: old.keys_by_hash.clone(),
    };
    state.cfg.store(Arc::new(snapshot));

    let app = brighto_router::handlers::router(state);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("authorization", "Bearer test-key")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"claude-bad-backend","max_tokens":8,"messages":[{"role":"user","content":"hi"}]}"#,
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 502);
    let body = to_bytes(resp.into_body(), 8192).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["type"], "error");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Claude bridge requires an OpenAI Chat backend"),
        "{value}"
    );
}
