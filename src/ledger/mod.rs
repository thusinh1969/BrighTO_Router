//! Ledger: channel -> batch INSERT (Postgres) -> background. DB chết -> file JSONL, replay.
//! Hot path chỉ gọi LedgerSink::try_record (không block, không drop im lặng). KHÔNG BAO GIỜ block response.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use sqlx::Row;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::time::{self, MissedTickBehavior};

use crate::contract::{UsageEvent, UsageScope, UsageSeed};

/// Sink từ hot path. primary đầy -> overflow -> writer ghi file ngoài request task.
/// Cả hai đầy -> error log + counter (không bao giờ im lặng mất usage).
#[derive(Clone)]
pub struct LedgerSink {
    primary: mpsc::Sender<UsageEvent>,
    overflow: mpsc::Sender<UsageEvent>,
}

impl LedgerSink {
    pub fn new(primary: mpsc::Sender<UsageEvent>, overflow: mpsc::Sender<UsageEvent>) -> Self {
        Self { primary, overflow }
    }

    pub fn try_record(&self, event: UsageEvent) {
        match self.primary.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(ev)) => match self.overflow.try_send(ev) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_) | mpsc::error::TrySendError::Closed(_)) => {
                    record_drop("primary + overflow full");
                }
            },
            Err(mpsc::error::TrySendError::Closed(ev)) => {
                if self.overflow.try_send(ev).is_err() {
                    record_drop("primary closed and overflow unavailable");
                }
            }
        }
    }
}

/// Log thưa (≤1/s) khi drop usage event — tránh spam log + giữ hot path rẻ lúc quá tải.
/// Counter metric vẫn tăng MỖI drop để giám sát chính xác số usage bị mất.
fn record_drop(reason: &str) {
    // Cache handle metric (tránh lookup registry + alloc Key trên MỖI drop ở hot path overload).
    static DROPPED: std::sync::OnceLock<metrics::Counter> = std::sync::OnceLock::new();
    DROPPED
        .get_or_init(|| metrics::counter!("router_ledger_dropped_total"))
        .increment(1);
    static LAST_LOG_MS: AtomicU64 = AtomicU64::new(0);
    let now = now_ms();
    let last = LAST_LOG_MS.load(Ordering::Relaxed);
    if now.saturating_sub(last) >= 1_000
        && LAST_LOG_MS
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        tracing::error!("ledger dropping usage events (rate-limited): {reason}");
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub struct LedgerWriter {
    pub batch_size: usize,
    pub flush_secs: u64,
}

impl LedgerWriter {
    pub fn new(batch_size: usize, flush_secs: u64) -> Self {
        Self {
            batch_size,
            flush_secs,
        }
    }

    /// Task nền. primary -> DB batch; overflow -> file JSONL ngay. DB chết -> primary cũng ghi file.
    pub async fn run(
        &self,
        primary: mpsc::Receiver<UsageEvent>,
        overflow: mpsc::Receiver<UsageEvent>,
    ) -> anyhow::Result<()> {
        let fallback = fallback_path();
        let initial_pool = connect_pool().await.ok();
        self.run_with_channels(primary, overflow, initial_pool, &fallback, true)
            .await
    }

    async fn run_with_channels(
        &self,
        mut primary: mpsc::Receiver<UsageEvent>,
        mut overflow: mpsc::Receiver<UsageEvent>,
        mut pool: Option<PgPool>,
        fallback: &Path,
        reconnect: bool,
    ) -> anyhow::Result<()> {
        let mut batch: Vec<UsageEvent> = Vec::with_capacity(self.batch_size.max(1));
        let mut interval = time::interval(Duration::from_secs(self.flush_secs.max(1)));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut overflow_open = true;

        loop {
            tokio::select! {
                maybe = primary.recv() => {
                    match maybe {
                        Some(ev) => {
                            if pool.is_some() {
                                batch.push(ev);
                                if batch.len() >= self.batch_size {
                                    if let Err(e) = flush_batch(pool.as_ref().unwrap(), &batch, fallback).await {
                                        eprintln!("ledger insert failed, switching to fallback: {e}");
                                        pool = None;
                                    }
                                    batch.clear();
                                }
                            } else {
                                append_fallback(fallback, &ev).await?;
                            }
                        }
                        None => {
                            self.flush_remaining(&mut batch, &mut pool, fallback).await;
                            return Ok(());
                        }
                    }
                }
                maybe = overflow.recv(), if overflow_open => {
                    match maybe {
                        Some(ev) => { append_fallback(fallback, &ev).await?; }
                        None => { overflow_open = false; }
                    }
                }
                _ = interval.tick() => {
                    if !batch.is_empty() {
                        if let Some(p) = pool.as_ref() {
                            if let Err(e) = flush_batch(p, &batch, fallback).await {
                                eprintln!("ledger timer flush failed: {e}");
                                pool = None;
                            }
                            batch.clear();
                        } else {
                            for ev in batch.drain(..) {
                                append_fallback(fallback, &ev).await?;
                            }
                        }
                    }

                    if reconnect && pool.is_none() {
                        match connect_pool().await {
                            Ok(p) => {
                                if let Err(e) = replay_fallback(&p, fallback).await {
                                    eprintln!("ledger replay failed: {e}");
                                }
                                pool = Some(p);
                            }
                            Err(e) => {
                                eprintln!("ledger reconnect failed: {e}");
                            }
                        }
                    }
                }
            }
        }
    }

    async fn flush_remaining(
        &self,
        batch: &mut Vec<UsageEvent>,
        pool: &mut Option<PgPool>,
        fallback: &Path,
    ) {
        if batch.is_empty() {
            return;
        }
        if let Some(p) = pool.as_ref() {
            let _ = flush_batch(p, batch, fallback).await;
        } else {
            for ev in batch.drain(..) {
                let _ = append_fallback(fallback, &ev).await;
            }
        }
        batch.clear();
    }
}

async fn connect_pool() -> anyhow::Result<PgPool> {
    let url = std::env::var("DATABASE_URL")
        .or_else(|_| std::env::var("LEDGER_DATABASE_URL"))
        .context("DATABASE_URL is required for ledger writer")?;

    PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .context("connect ledger DB")
}

async fn flush_batch(pool: &PgPool, batch: &[UsageEvent], fallback: &Path) -> anyhow::Result<()> {
    if let Err(db_err) = insert_batch(pool, batch).await {
        for ev in batch {
            append_fallback(fallback, ev).await?;
        }
        return Err(db_err);
    }
    Ok(())
}

async fn insert_batch(pool: &PgPool, batch: &[UsageEvent]) -> anyhow::Result<()> {
    if batch.is_empty() {
        return Ok(());
    }

    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "INSERT INTO usage_ledger (ts, request_id, key_id, team_id, model, backend_id, status, \
         input_tokens, output_tokens, estimated, ttfb_ms, total_ms, router_overhead_ms, completed_at_ms, stream, \
         client_aborted, error_class) ",
    );
    qb.push_values(batch.iter(), |mut b, ev| {
        b.push_bind(ev.ts)
            .push_bind(ev.request_id.as_str())
            .push_bind(ev.key_id)
            .push_bind(ev.team_id)
            .push_bind(ev.model.as_str())
            .push_bind(ev.backend_id)
            .push_bind(ev.status as i64)
            .push_bind(u64_to_i64(ev.input_tokens))
            .push_bind(u64_to_i64(ev.output_tokens))
            .push_bind(ev.estimated)
            .push_bind(u64_to_i64(ev.ttfb_ms))
            .push_bind(u64_to_i64(ev.total_ms))
            .push_bind(u64_to_i64(ev.router_overhead_ms))
            .push_bind(u64_to_i64(ev.completed_at_ms))
            .push_bind(ev.stream)
            .push_bind(ev.client_aborted)
            .push_bind(ev.error_class.as_deref());
    });
    qb.build().execute(pool).await?;
    Ok(())
}

fn u64_to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

async fn append_fallback(path: &Path, ev: &UsageEvent) -> anyhow::Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_context(|| format!("open ledger fallback {}", path.display()))?;
    let line = serde_json::to_string(ev)?;
    file.write_all(line.as_bytes()).await?;
    file.write_all(b"\n").await?;
    Ok(())
}

async fn replay_fallback(pool: &PgPool, fallback: &Path) -> anyhow::Result<()> {
    if !fallback.exists() {
        return Ok(());
    }

    let replay = fallback.with_extension("replay");
    tokio::fs::rename(fallback, &replay).await?;

    let result = replay_lines_to_db(pool, &replay).await;

    match result {
        Ok(()) => {
            tokio::fs::remove_file(&replay).await?;
            Ok(())
        }
        Err(e) => {
            let _ = tokio::fs::rename(&replay, fallback).await;
            Err(e)
        }
    }
}

async fn replay_lines_to_db(pool: &PgPool, replay: &Path) -> anyhow::Result<()> {
    use tokio::io::AsyncBufReadExt;

    let file = tokio::fs::File::open(replay).await?;
    let mut lines = tokio::io::BufReader::new(file).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let ev: UsageEvent = serde_json::from_str(&line)?;
        let exists: i64 = sqlx::query::<sqlx::Postgres>(
            "SELECT COUNT(*) FROM usage_ledger WHERE request_id = $1",
        )
        .bind(ev.request_id.as_str())
        .fetch_one(pool)
        .await?
        .try_get::<i64, _>(0)?;
        if exists == 0 {
            insert_batch(pool, &[ev]).await?;
        }
    }
    Ok(())
}

fn fallback_path() -> PathBuf {
    std::env::var_os("LEDGER_FALLBACK_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ledger_fallback.jsonl"))
}

/// Seed RAM budget counters từ usage_ledger cho period hiện tại (day + month), đủ 4 scope.
/// Chỉ chạy ở boot (trước khi listen), KHÔNG BAO GIỜ gọi từ handler/proxy.
pub async fn load_usage_seeds(pool: &PgPool) -> anyhow::Result<Vec<UsageSeed>> {
    use crate::budget::period_start;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let periods = [period_start("day", now), period_start("month", now)];

    let mut seeds = Vec::new();
    for period in periods {
        for row in sqlx::query::<sqlx::Postgres>(
            "SELECT key_id, CAST(COALESCE(SUM(input_tokens + output_tokens), 0) AS BIGINT) AS used \
             FROM usage_ledger WHERE ts >= $1 GROUP BY key_id",
        )
        .bind(period)
        .fetch_all(pool)
        .await?
        {
            let id: i64 = row.try_get(0)?;
            let used: i64 = row.try_get(1)?;
            seeds.push(UsageSeed {
                scope: UsageScope::KeyTotal,
                id,
                model: String::new(),
                period_start: period,
                used_tokens: used.max(0) as u64,
            });
        }

        for row in sqlx::query::<sqlx::Postgres>(
            "SELECT key_id, model, CAST(COALESCE(SUM(input_tokens + output_tokens), 0) AS BIGINT) AS used \
             FROM usage_ledger WHERE ts >= $1 GROUP BY key_id, model",
        )
        .bind(period)
        .fetch_all(pool)
        .await?
        {
            let id: i64 = row.try_get(0)?;
            let model: String = row.try_get(1)?;
            let used: i64 = row.try_get(2)?;
            seeds.push(UsageSeed {
                scope: UsageScope::KeyModel,
                id,
                model,
                period_start: period,
                used_tokens: used.max(0) as u64,
            });
        }

        for row in sqlx::query::<sqlx::Postgres>(
            "SELECT team_id, CAST(COALESCE(SUM(input_tokens + output_tokens), 0) AS BIGINT) AS used \
             FROM usage_ledger WHERE ts >= $1 GROUP BY team_id",
        )
        .bind(period)
        .fetch_all(pool)
        .await?
        {
            let id: i64 = row.try_get(0)?;
            let used: i64 = row.try_get(1)?;
            seeds.push(UsageSeed {
                scope: UsageScope::TeamTotal,
                id,
                model: String::new(),
                period_start: period,
                used_tokens: used.max(0) as u64,
            });
        }

        for row in sqlx::query::<sqlx::Postgres>(
            "SELECT team_id, model, CAST(COALESCE(SUM(input_tokens + output_tokens), 0) AS BIGINT) AS used \
             FROM usage_ledger WHERE ts >= $1 GROUP BY team_id, model",
        )
        .bind(period)
        .fetch_all(pool)
        .await?
        {
            let id: i64 = row.try_get(0)?;
            let model: String = row.try_get(1)?;
            let used: i64 = row.try_get(2)?;
            seeds.push(UsageSeed {
                scope: UsageScope::TeamModel,
                id,
                model,
                period_start: period,
                used_tokens: used.max(0) as u64,
            });
        }
    }
    Ok(seeds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_millis()
    }

    fn temp_fallback(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "brigto_b2_fallback_{}_{}_{}.jsonl",
            tag,
            std::process::id(),
            unique_suffix()
        ))
    }

    fn make_event(id: i64) -> UsageEvent {
        UsageEvent {
            ts: 1_700_000_000,
            request_id: format!("req-{}-{}", id, unique_suffix()),
            key_id: 1,
            team_id: 1,
            model: "m".into(),
            backend_id: 1,
            status: 200,
            input_tokens: 10,
            output_tokens: 20,
            estimated: false,
            ttfb_ms: 100,
            total_ms: 200,
            router_overhead_ms: 5,
            completed_at_ms: now_ms(),
            stream: false,
            client_aborted: false,
            error_class: None,
        }
    }

    async fn count_events(pool: &PgPool) -> anyhow::Result<i64> {
        let n: i64 = sqlx::query::<sqlx::Postgres>("SELECT COUNT(*) FROM usage_ledger")
            .fetch_one(pool)
            .await?
            .try_get::<i64, _>(0)?;
        Ok(n)
    }

    fn channels(cap: usize) -> (mpsc::Sender<UsageEvent>, mpsc::Receiver<UsageEvent>) {
        mpsc::channel(cap)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn batch_flush_by_size_va_by_time(pool: PgPool) -> anyhow::Result<()> {
        let writer = LedgerWriter::new(2, 60);
        let (tx, rx) = channels(16);
        let (_ox, orx) = channels(16);
        let fallback = temp_fallback("size");
        let pool_for_assert = pool.clone();
        let handle = tokio::spawn(async move {
            writer
                .run_with_channels(rx, orx, Some(pool), &fallback, false)
                .await
        });

        for i in 0..2 {
            tx.send(make_event(i)).await?;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(count_events(&pool_for_assert).await?, 2);
        drop(tx);
        handle.await??;

        let writer = LedgerWriter::new(10, 1);
        let (tx, rx) = channels(16);
        let (_ox, orx) = channels(16);
        let fallback = temp_fallback("time");
        let pool_writer = pool_for_assert.clone();
        let handle = tokio::spawn(async move {
            writer
                .run_with_channels(rx, orx, Some(pool_writer), &fallback, false)
                .await
        });

        // Cùng một pool với phần đầu (2 events đã flush) -> đếm delta, không đếm tuyệt đối.
        let before = count_events(&pool_for_assert).await?;
        tx.send(make_event(100)).await?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(count_events(&pool_for_assert).await?, before + 1);
        drop(tx);
        handle.await??;

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn db_down_writes_file_replay_on_reconnect(pool: PgPool) -> anyhow::Result<()> {
        let fallback = temp_fallback("down");
        let writer = LedgerWriter::new(100, 1);
        let (tx, rx) = channels(16);
        let (_ox, orx) = channels(16);
        let fallback_for_task = fallback.clone();
        let handle = tokio::spawn(async move {
            writer
                .run_with_channels(rx, orx, None, &fallback_for_task, false)
                .await
        });

        for i in 0..3 {
            tx.send(make_event(i)).await?;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;

        let content = tokio::fs::read_to_string(&fallback).await?;
        assert_eq!(content.lines().count(), 3);
        assert!(fallback.exists());

        replay_fallback(&pool, &fallback).await?;
        assert_eq!(count_events(&pool).await?, 3);
        assert!(!fallback.exists());

        drop(tx);
        handle.await??;
        Ok(())
    }

    #[tokio::test]
    async fn sink_overflow_never_blocks_or_drops_silently() {
        let (ptx, prx) = channels(1);
        let (otx, mut orx) = channels(2);
        let sink = LedgerSink::new(ptx, otx);

        for i in 0..3 {
            let start = std::time::Instant::now();
            sink.try_record(make_event(i));
            assert!(start.elapsed().as_millis() < 500);
        }

        drop(prx);
        assert!(orx.recv().await.is_some());
        assert!(orx.recv().await.is_some());
        drop(orx);
    }
}
