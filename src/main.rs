//! Vỏ binary: nạp config, wire concrete state, spawn task nền (poll/health/metrics/ledger),
//! dựng axum server với connect-info (admin dùng peer addr), graceful shutdown.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use arc_swap::ArcSwap;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

use brighto_router::budget::RamBudgetStore;
use brighto_router::config::DbConfigLoader;
use brighto_router::contract::{AppState, ConfigSnapshot};
use brighto_router::handlers;
use brighto_router::ledger::{LedgerSink, LedgerWriter};
use brighto_router::metrics::Metrics;
use brighto_router::route::RamBackendPool;

const DEFAULT_LISTEN: &str = "0.0.0.0:8090";
const DEFAULT_MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
const CONFIG_POLL_SECS: u64 = 5;
const HEALTH_INTERVAL_SECS: u64 = 5;

#[tokio::main(worker_threads = 4)]
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
        .map_err(|e| anyhow::anyhow!("init tracing subscriber: {e}"))?;

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let listen_addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| DEFAULT_LISTEN.to_string());
    let max_body_bytes = std::env::var("MAX_BODY_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_BODY_BYTES);
    let poll_secs = std::env::var("CONFIG_POLL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(CONFIG_POLL_SECS);
    // Reload timeout phải < cửa sổ stale để /readyz lật 503 ngay cả khi reload bị treo.
    let reload_timeout = Duration::from_millis(
        std::env::var("CONFIG_RELOAD_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(2_000),
    );
    let readiness_max_stale_ms = std::env::var("READY_MAX_STALE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or_else(|| (poll_secs.max(1) * 3_000).max(5_000));

    // Pool riêng cho config loader (poll nền); hot path không đụng pool này.
    let cfg_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db_url)
        .await
        .context("connect config DB")?;
    let cfg: Arc<ArcSwap<ConfigSnapshot>> =
        Arc::new(ArcSwap::from_pointee(ConfigSnapshot::default()));

    // Concrete runtime state.
    let budget = Arc::new(RamBudgetStore::new());
    let backends = Arc::new(RamBackendPool::new());
    let client = build_client();
    let metrics = Metrics::install();
    let config_ok_at = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let config_err_at = Arc::new(std::sync::atomic::AtomicU64::new(0));

    // Snapshot đầu: wire runtime stores TRƯỚC, publish snapshot SAU (tránh race).
    let boot_loader = DbConfigLoader::new(cfg_pool.clone(), poll_secs);
    let boot_snap = boot_loader.load_snapshot().await?;
    wire_snapshot(&budget, &backends, &boot_snap);

    // Usage-ledger boot counter: chạy MỘT lần lúc boot để log tổng. Không nằm trong
    // load_snapshot — poll 5s phải chỉ chạm bảng cấu hình (CODEX config-reload audit).
    if let Err(e) = boot_loader.log_usage_boot_counter().await {
        tracing::warn!(error = %e, "usage_ledger boot counter skipped");
    }

    // Seed budget counters từ ledger period hiện tại — chống reset budget sau restart.
    match brighto_router::ledger::load_usage_seeds(&cfg_pool).await {
        Ok(seeds) => {
            for seed in &seeds {
                budget.seed_usage(seed);
            }
            tracing::info!(seeds = seeds.len(), "seeded budget counters from ledger");
        }
        Err(e) => tracing::warn!(error = %e, "budget seed skipped (usage_ledger unavailable?)"),
    }

    config_ok_at.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
    cfg.store(Arc::new(boot_snap));

    // Poll config: swap snapshot + wire budget/backends mỗi poll (hoặc ngay khi admin notify).
    let reload_notify = Arc::new(tokio::sync::Notify::new());
    let (poll_budget, poll_backends, poll_cfg, poll_notify, poll_ok, poll_err) = (
        budget.clone(),
        backends.clone(),
        cfg.clone(),
        reload_notify.clone(),
        config_ok_at.clone(),
        config_err_at.clone(),
    );
    let poll_loader = DbConfigLoader::new(cfg_pool, poll_secs);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(poll_secs)) => {}
                _ = poll_notify.notified() => {}
            }
            match tokio::time::timeout(reload_timeout, poll_loader.load_snapshot()).await {
                Ok(Ok(snap)) => {
                    wire_snapshot(&poll_budget, &poll_backends, &snap);
                    poll_cfg.store(Arc::new(snap));
                    poll_ok.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
                }
                Ok(Err(e)) => {
                    eprintln!("WARN config: reload failed: {e:#}");
                    poll_err.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
                }
                Err(_) => {
                    eprintln!("WARN config: reload timed out after {reload_timeout:?}");
                    poll_err.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
                }
            }
        }
    });

    // Active health: mở/đóng circuit theo /health của backend.
    backends
        .clone()
        .start_health_loop(client.clone(), Duration::from_secs(HEALTH_INTERVAL_SECS));

    // Ledger: hot path -> LedgerSink; writer nền -> DB batch / file fallback.
    let (primary_tx, primary_rx) = mpsc::channel(8192);
    let (overflow_tx, overflow_rx) = mpsc::channel(16_384);
    let ledger = LedgerSink::new(primary_tx, overflow_tx);
    let writer = LedgerWriter::new(100, 1);
    tokio::spawn(async move {
        if let Err(e) = writer.run(primary_rx, overflow_rx).await {
            tracing::error!(error = %e, "ledger writer stopped");
        }
    });

    // Metrics gauge sync (backend inflight/circuit, budget remaining).
    let (m_budget, m_backends) = (budget.clone(), backends.clone());
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(HEALTH_INTERVAL_SECS));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            for (id, inflight, open) in m_backends.snapshot() {
                brighto_router::metrics::set_backend_inflight(id, inflight);
                brighto_router::metrics::set_circuit_open(id, open);
            }
            for (team, model, remaining) in m_budget.budget_remaining_snapshot() {
                brighto_router::metrics::set_budget_remaining(team, &model, remaining);
            }
        }
    });

    let app_state = Arc::new(AppState {
        cfg: cfg.clone(),
        budget,
        backends,
        client,
        ledger,
        metrics,
        max_body_bytes,
        reload_notify,
        config_ok_at,
        config_err_at,
        readiness_max_stale_ms,
    });

    let app = handlers::router(app_state);
    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    tracing::info!(addr = %listen_addr, "brighto-router listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .pool_max_idle_per_host(64)
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .expect("build reqwest client")
}

/// Nạp snapshot vào budget + backends (bootstrap + mỗi reload). Prune config đã xoá.
fn wire_snapshot(budget: &RamBudgetStore, backends: &RamBackendPool, snap: &ConfigSnapshot) {
    backends.sync_backends(&snap.backends);
    budget.sync_teams(&snap.teams);
}

async fn shutdown_signal() {
    let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }
    // Graceful shutdown bắt đầu NGAY sau signal; deadline ngoài do Docker/K8s stop_grace_period lo.
    tracing::info!("shutdown signal received, starting graceful shutdown");
}

async fn healthcheck() -> anyhow::Result<()> {
    let addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| DEFAULT_LISTEN.to_string());
    let url = format!("http://{}/healthz", addr);
    let resp = reqwest::get(&url).await?;
    if resp.status().is_success() {
        Ok(())
    } else {
        bail!("healthcheck failed with status {}", resp.status());
    }
}
