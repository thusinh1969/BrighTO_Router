# B1 — RE-RUN (lan truoc bi cat giua chung, thieu toan bo handlers.rs)

Nhiem vu goc: docs/agents/b1_glue.md. Lan truoc ban tra main.rs chua xong (ket thuc o dong
`let cfg_for_poll = app_state.c`) va KHONG co src/handlers.rs.

Yeu cau lan nay:
1. Tra ve DUNG 2 code block: `// FILE: src/main.rs` va `// FILE: src/handlers.rs`, moi file HOAN CHINH
   (khong danh dau TODO/todo!(), khong "phan con lai tuong tu").
2. handlers.rs phai dung signature ma main.rs import:
   - `pub struct RouterState` (hoac tuong duong) giu AppState.
   - `pub fn router(state) -> axum::Router` voi 5 route: POST /v1/chat/completions, /v1/completions,
     /v1/embeddings, /v1/messages, GET /v1/models, GET /metrics (Prometheus), GET /healthz.
   - Pipeline theo plan muc 3: auth (A2) -> doc body Bytes (gioi han MAX_BODY_BYTES) -> chi lay model+stream
     -> authorize model (403 sai model) -> try_reserve (429) -> route pick (503) -> proxy forward (A4)
     -> SSE pump -> commit + ledger + metrics.
   - Header response: x-router-request-id, x-router-backend, x-router-overhead-ms.
3. main.rs: hoan thien phan con thieu (spawn poll config, spawn ledger writer, axum serve voi graceful
   shutdown, healthcheck subcommand).
4. Doc code theo quy tac: comment TAI SAO, khong comment LAM GI.

Doan main.rs da co (khong nhat thiet giu nguyen 100%, duoc sua de dung):
```
```rust
// FILE: src/main.rs
//! Vỏ binary: nạp config đầu tiên, khởi động poll config/ledger writer,
//! dựng axum server, graceful shutdown và healthcheck subcommand.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context};
use arc_swap::ArcSwap;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;

use brigto_router::budget::MemoryBudgetStore;
use brigto_router::config;
use brigto_router::contract::{AppState, BackendPool, BudgetStore, ConfigSnapshot};
use brigto_router::handlers::{self, RouterState};
use brigto_router::ledger;
use brigto_router::route::BackendPoolImpl;

const DEFAULT_LISTEN: &str = "0.0.0.0:8090";
const DEFAULT_STOP_GRACE_SECS: u64 = 5 * 60;
const DEFAULT_MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return healthcheck().await;
    }

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .try_init()
        .context("init tracing subscriber")?;

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let listen_addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| DEFAULT_LISTEN.to_string());
    let max_body_bytes = std::env::var("MAX_BODY_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_BODY_BYTES);
    let stop_grace_period = Duration::from_secs(
        std::env::var("STOP_GRACE_PERIOD")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_STOP_GRACE_SECS),
    );

    let cfg = Arc::new(ArcSwap::from_pointee(ConfigSnapshot::default()));

    // Snapshot đầu phải sẵn sàng trước khi listen: tránh phục vụ với route rỗng.
    config::bootstrap(&cfg, &db_url).await?;

    let (ledger_tx, ledger_rx) = mpsc::channel(8192);

    let budget_store: Arc<dyn BudgetStore> = Arc::new(MemoryBudgetStore::new());
    let pool: Arc<dyn BackendPool> = Arc::new(BackendPoolImpl::new(cfg.clone()));

    let app_state = AppState {
        cfg,
        budget: budget_store,
        pool,
        ledger_tx,
    };

    let cfg_for_poll = app_state.c
```

Tra ve DUNG format 2 code block `// FILE:` + code. KHONG giai thich ngoai le.
