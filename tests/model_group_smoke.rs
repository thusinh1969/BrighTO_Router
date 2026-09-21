//! Preview-3 Model Group smoke: three OpenAI-compatible chat endpoints behind one public model.
//! This is a safe mock smoke: it measures router/load-balancer path, not paid model inference.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use axum::body::{Body, Bytes, to_bytes};
use axum::http::Request;
use sqlx::postgres::PgPool;
use tower::ServiceExt;

use brighto_router::budget::RamBudgetStore;
use brighto_router::contract::AppState;
use brighto_router::ledger::LedgerSink;
use brighto_router::metrics::Metrics;
use brighto_router::route::RamBackendPool;

fn metrics() -> Metrics {
    static M: std::sync::OnceLock<Metrics> = std::sync::OnceLock::new();
    M.get_or_init(Metrics::install).clone()
}

async fn spawn_model_backend(
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
                    .body(Body::from(format!(
                        r#"{{"id":"cmpl-{backend_name}","choices":[{{"message":{{"role":"assistant","content":"ok from {backend_name}"}}}}],"usage":{{"prompt_tokens":2,"completion_tokens":3}}}}"#
                    )))
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

#[sqlx::test(migrations = "./migrations")]
async fn model_group_weighted_round_robin_three_endpoint_smoke(pool: PgPool) {
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let deepseek_base = spawn_model_backend("deepseek", tx.clone()).await;
    let local_base = spawn_model_backend("local-llamacpp", tx.clone()).await;
    let openai_base = spawn_model_backend("openai", tx).await;

    let key_file = std::env::temp_dir().join(format!("brighto_mg_key_{}", std::process::id()));
    std::fs::write(&key_file, "mock-provider-key").unwrap();
    let key_ref = format!("file:{}", key_file.display());

    sqlx::query(
        "INSERT INTO backends (id, name, base_url, api_key_ref, weight, max_inflight, format, enabled) \
         VALUES (1, 'deepseek-endpoint', $1, $4, 1, 100, 'openai', TRUE), \
                (2, 'local-llamacpp', $2, 'env:NONE', 1, 4, 'openai', TRUE), \
                (3, 'openai-endpoint', $3, $4, 1, 100, 'openai', TRUE)",
    )
    .bind(&deepseek_base)
    .bind(&local_base)
    .bind(&openai_base)
    .bind(&key_ref)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO model_routes \
         (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, \
          provider_model_name, enabled, auth_mode, protocol, routing_policy) \
         VALUES ('coding-fast', '[1,2,3]', NULL, 4.0, 180, 'coding-fast', TRUE, 'bearer', 'openai_chat', 'weighted_round_robin')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO model_route_endpoints \
         (model_name, backend_id, provider_model_name, provider_key_ref, auth_mode, protocol, weight, max_inflight, enabled) \
         VALUES ('coding-fast', 1, 'deepseek-v4-pro', $1, 'bearer', 'openai_chat', 3, 100, TRUE), \
                ('coding-fast', 2, 'qwen3.8-flash-next', NULL, 'none', 'openai_chat', 1, 4, TRUE), \
                ('coding-fast', 3, 'gpt-5.6-mini', $1, 'bearer', 'openai_chat', 2, 100, TRUE)",
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

    let mut latencies = Vec::new();
    for _ in 0..6 {
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("authorization", "Bearer test-key")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"model":"coding-fast","messages":[{"role":"user","content":"hello"}],"stream":false}"#,
            ))
            .unwrap();
        let start = Instant::now();
        let resp = app.clone().oneshot(req).await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(resp.status().as_u16(), 200);
        let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap()
                .starts_with("ok from")
        );
        latencies.push(elapsed);
    }

    let mut captured = Vec::new();
    for _ in 0..6 {
        captured.push(
            tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    assert_eq!(
        captured,
        vec![
            ("deepseek".to_string(), "deepseek-v4-pro".to_string()),
            ("deepseek".to_string(), "deepseek-v4-pro".to_string()),
            ("deepseek".to_string(), "deepseek-v4-pro".to_string()),
            (
                "local-llamacpp".to_string(),
                "qwen3.8-flash-next".to_string()
            ),
            ("openai".to_string(), "gpt-5.6-mini".to_string()),
            ("openai".to_string(), "gpt-5.6-mini".to_string()),
        ]
    );

    for _ in 0..6 {
        let ev = tokio::time::timeout(Duration::from_secs(2), prx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ev.model, "coding-fast");
        assert_eq!(ev.status, 200);
    }

    latencies.sort();
    let p50 = latencies[latencies.len() / 2];
    let max = *latencies.last().unwrap();
    println!(
        "MODEL_GROUP_SMOKE weighted_round_robin endpoints=3 requests=6 p50_ms={:.3} max_ms={:.3}",
        p50.as_secs_f64() * 1000.0,
        max.as_secs_f64() * 1000.0
    );
}
