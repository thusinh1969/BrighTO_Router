//! Chọn backend least-load (inflight/weight, hoà -> random), circuit breaker, health.
//! acquire() trả BackendLease (RAII): pick + inc inflight trong một guard, Drop -> dec inflight.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arc_swap::ArcSwapOption;
use dashmap::DashMap;
use rand::RngExt;
use sqlx::{Row, postgres::PgPool};
use tokio::sync::Mutex; // rand 0.10: random_range nằm trên trait Rng

use crate::contract::{Backend, ModelRoute, RoutingPolicy};

const CIRCUIT_CLOSED: u8 = 0;
const CIRCUIT_OPEN: u8 = 1;
const CIRCUIT_HALF_OPEN: u8 = 2;

#[derive(Clone, Copy)]
struct Candidate {
    backend_id: i64,
    score: f64,
    half_open: bool,
}

const INLINE_EXCLUSIONS: usize = 8;

/// Stack-backed retry/exclusion set for the hot route-pick path.
/// Common case (one backend, no retry) performs no heap allocation.
pub struct BackendExclusions {
    inline: [i64; INLINE_EXCLUSIONS],
    len: usize,
    overflow: Vec<i64>,
}

impl BackendExclusions {
    pub fn insert(&mut self, backend_id: i64) {
        if self.contains(backend_id) {
            return;
        }
        if self.len < INLINE_EXCLUSIONS {
            self.inline[self.len] = backend_id;
            self.len += 1;
        } else {
            self.overflow.push(backend_id);
        }
    }

    fn contains(&self, backend_id: i64) -> bool {
        self.inline[..self.len].contains(&backend_id) || self.overflow.contains(&backend_id)
    }
}

impl Default for BackendExclusions {
    fn default() -> Self {
        Self {
            inline: [0; INLINE_EXCLUSIONS],
            len: 0,
            overflow: Vec::new(),
        }
    }
}

/// Lease một backend đang chạy. Drop -> giải phóng 1 slot inflight.
pub struct BackendLease {
    pool: Arc<RamBackendPool>,
    backend_id: i64,
}

impl BackendLease {
    pub fn backend_id(&self) -> i64 {
        self.backend_id
    }
}

impl Drop for BackendLease {
    fn drop(&mut self) {
        self.pool.dec_inflight(self.backend_id);
    }
}

struct BackendState {
    inflight: AtomicU32,
    consecutive_failures: AtomicU32,
    circuit: AtomicU8,
    half_open_probe: AtomicBool,
    opened_until: ArcSwapOption<tokio::time::Instant>,
    weight: AtomicU32,
    max_inflight: AtomicU32,
    enabled: AtomicBool,
    base_url: RwLock<Option<String>>,
}

impl BackendState {
    fn new(backend: &Backend) -> Self {
        Self {
            inflight: AtomicU32::new(0),
            consecutive_failures: AtomicU32::new(0),
            circuit: AtomicU8::new(CIRCUIT_CLOSED),
            half_open_probe: AtomicBool::new(true),
            opened_until: ArcSwapOption::empty(),
            weight: AtomicU32::new(backend.weight),
            max_inflight: AtomicU32::new(backend.max_inflight),
            enabled: AtomicBool::new(backend.enabled),
            base_url: RwLock::new(Some(backend.base_url.clone())),
        }
    }
}

pub struct RamBackendPool {
    states: DashMap<i64, BackendState>,
    rr_counters: DashMap<String, Arc<CounterBlock>>,
    counter_pool: Option<PgPool>,
    counter_block_size: u64,
    open_duration: Duration,
}

struct CounterBlock {
    next: AtomicU64,
    end: AtomicU64,
    refill: Mutex<()>,
}

impl CounterBlock {
    fn empty() -> Self {
        Self {
            next: AtomicU64::new(0),
            end: AtomicU64::new(0),
            refill: Mutex::new(()),
        }
    }
}

#[derive(Debug)]
pub enum AcquireError {
    CounterUnavailable(String),
}

impl RamBackendPool {
    pub fn new() -> Self {
        Self::new_without_counter_pool()
    }

    pub fn new_without_counter_pool() -> Self {
        Self::new_without_counter_pool_with_open_duration(Duration::from_secs(30))
    }

    pub fn new_without_counter_pool_with_open_duration(open_duration: Duration) -> Self {
        Self {
            states: DashMap::new(),
            rr_counters: DashMap::new(),
            counter_pool: None,
            counter_block_size: 1024,
            open_duration,
        }
    }

    pub fn new_with_counter_pool(counter_pool: PgPool, counter_block_size: u64) -> Self {
        Self::new_with_counter_pool_and_open_duration(
            counter_pool,
            counter_block_size,
            Duration::from_secs(30),
        )
    }

    pub fn new_with_counter_pool_and_open_duration(
        counter_pool: PgPool,
        counter_block_size: u64,
        open_duration: Duration,
    ) -> Self {
        Self {
            states: DashMap::new(),
            rr_counters: DashMap::new(),
            counter_pool: Some(counter_pool),
            counter_block_size: counter_block_size.max(1),
            open_duration,
        }
    }

    /// Cập nhật cấu hình backend. Gọi sau bootstrap và mỗi lần config reload.
    pub fn upsert_backend(&self, backend: Backend) {
        use dashmap::mapref::entry::Entry;

        match self.states.entry(backend.id) {
            Entry::Occupied(occupied) => {
                let state = occupied.get();
                state.weight.store(backend.weight, Ordering::Relaxed);
                state
                    .max_inflight
                    .store(backend.max_inflight, Ordering::Relaxed);
                state.enabled.store(backend.enabled, Ordering::Relaxed);
                if let Ok(mut base) = state.base_url.write() {
                    *base = Some(backend.base_url.clone());
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(BackendState::new(&backend));
            }
        }
    }

    /// Đồng bộ toàn bộ backends từ snapshot: upsert cái có, disable/xoá cái không còn.
    /// Xoá vật lý chỉ khi inflight == 0 (tránh race với BackendLease::Drop).
    pub fn sync_backends(&self, snapshot: &HashMap<i64, Backend>) {
        let current: Vec<i64> = self.states.iter().map(|entry| *entry.key()).collect();
        for id in current {
            if !snapshot.contains_key(&id) {
                let removable = match self.states.get(&id) {
                    Some(state) => {
                        state.enabled.store(false, Ordering::Relaxed);
                        state.inflight.load(Ordering::Relaxed) == 0
                    }
                    None => true,
                };
                if removable {
                    self.states.remove(&id);
                }
            }
        }
        for backend in snapshot.values() {
            self.upsert_backend(backend.clone());
        }
    }

    /// Chọn backend khoẻ, ít việc nhất và giữ 1 slot inflight (RAII). None = không còn backend.
    pub async fn acquire(
        self: &Arc<Self>,
        route: &ModelRoute,
    ) -> Result<Option<BackendLease>, AcquireError> {
        let mut excluded = BackendExclusions::default();
        self.acquire_excluding(route, &mut excluded).await
    }

    /// Primary group trước; hết primary mới thử fallback_backend_id (nếu có, chưa thử).
    pub async fn acquire_excluding(
        self: &Arc<Self>,
        route: &ModelRoute,
        excluded: &mut BackendExclusions,
    ) -> Result<Option<BackendLease>, AcquireError> {
        if let Some(lease) = self
            .acquire_primary(route, &route.backend_ids, excluded)
            .await?
        {
            return Ok(Some(lease));
        }
        let Some(fallback) = route.fallback_backend_id else {
            return Ok(None);
        };
        if excluded.contains(fallback) {
            return Ok(None);
        }
        self.acquire_primary(route, std::slice::from_ref(&fallback), excluded)
            .await
    }

    async fn acquire_primary(
        self: &Arc<Self>,
        route: &ModelRoute,
        backend_ids: &[i64],
        excluded: &mut BackendExclusions,
    ) -> Result<Option<BackendLease>, AcquireError> {
        loop {
            let Some(candidate) = self.choose_candidate(route, backend_ids, excluded).await? else {
                return Ok(None);
            };
            let Some(state) = self.states.get(&candidate.backend_id) else {
                excluded.insert(candidate.backend_id);
                continue;
            };

            // Half-open: chỉ một request duy nhất được phép đi qua.
            if candidate.half_open
                && state
                    .half_open_probe
                    .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
            {
                excluded.insert(candidate.backend_id);
                continue;
            }

            // Atomic inflight reserve + re-check max (đóng TOCTOU pick/inc).
            let global_max = state.max_inflight.load(Ordering::Relaxed);
            let endpoint_max = route.endpoint_max_inflight(candidate.backend_id);
            let max = match (global_max, endpoint_max) {
                (0, 0) => 0,
                (0, x) => x,
                (x, 0) => x,
                (x, y) => x.min(y),
            };
            let prev = state.inflight.fetch_add(1, Ordering::Relaxed);
            if max > 0 && prev >= max {
                state.inflight.fetch_sub(1, Ordering::Relaxed);
                if candidate.half_open {
                    state.half_open_probe.store(true, Ordering::Release);
                }
                excluded.insert(candidate.backend_id);
                continue;
            }

            return Ok(Some(BackendLease {
                pool: Arc::clone(self),
                backend_id: candidate.backend_id,
            }));
        }
    }

    /// Chọn candidate theo policy của Model Group (chưa giữ slot; circuit-aware).
    async fn choose_candidate(
        &self,
        route: &ModelRoute,
        backend_ids: &[i64],
        excluded: &BackendExclusions,
    ) -> Result<Option<Candidate>, AcquireError> {
        match route.routing_policy {
            RoutingPolicy::RoundRobin => {
                self.choose_round_robin_candidate(route, backend_ids, excluded)
                    .await
            }
            RoutingPolicy::WeightedRoundRobin => {
                self.choose_weighted_round_robin_candidate(route, backend_ids, excluded)
                    .await
            }
            RoutingPolicy::LeastLoadedWeighted => {
                Ok(self.choose_least_loaded_candidate(route, backend_ids, excluded))
            }
        }
    }

    fn candidate_for_backend(
        &self,
        route: &ModelRoute,
        backend_id: i64,
        excluded: &BackendExclusions,
        now: tokio::time::Instant,
    ) -> Option<Candidate> {
        if excluded.contains(backend_id) || !route.endpoint_enabled(backend_id) {
            return None;
        }
        let state = self.states.get(&backend_id)?;
        if !state.enabled.load(Ordering::Relaxed) {
            return None;
        }

        let mut half_open = false;
        match state.circuit.load(Ordering::Acquire) {
            CIRCUIT_CLOSED => {}
            CIRCUIT_OPEN => {
                let until = state.opened_until.load();
                let expired = until.as_ref().map(|i| **i <= now).unwrap_or(true);
                drop(until);
                if !expired {
                    return None;
                }
                if state
                    .circuit
                    .compare_exchange(
                        CIRCUIT_OPEN,
                        CIRCUIT_HALF_OPEN,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    state.half_open_probe.store(true, Ordering::Release);
                    half_open = true;
                } else {
                    return None;
                }
            }
            CIRCUIT_HALF_OPEN if state.half_open_probe.load(Ordering::Acquire) => {
                half_open = true;
            }
            CIRCUIT_HALF_OPEN => return None,
            _ => return None,
        }

        let inflight = state.inflight.load(Ordering::Relaxed);
        let global_max = state.max_inflight.load(Ordering::Relaxed);
        let endpoint_max = route.endpoint_max_inflight(backend_id);
        let max = match (global_max, endpoint_max) {
            (0, 0) => 0,
            (0, x) => x,
            (x, 0) => x,
            (x, y) => x.min(y),
        };
        if max > 0 && inflight >= max {
            return None;
        }

        let weight = route
            .endpoints
            .get(&backend_id)
            .filter(|e| e.enabled)
            .map(|e| e.weight.max(1))
            .unwrap_or_else(|| state.weight.load(Ordering::Relaxed).max(1));
        Some(Candidate {
            backend_id,
            score: inflight as f64 / weight as f64,
            half_open,
        })
    }

    async fn choose_round_robin_candidate(
        &self,
        route: &ModelRoute,
        backend_ids: &[i64],
        excluded: &BackendExclusions,
    ) -> Result<Option<Candidate>, AcquireError> {
        let now = tokio::time::Instant::now();
        let candidate_count = backend_ids
            .iter()
            .filter(|id| {
                self.candidate_for_backend(route, **id, excluded, now)
                    .is_some()
            })
            .count();
        if candidate_count == 0 {
            return Ok(None);
        }
        let selected =
            self.next_persistent_counter(&route.model_name).await? as usize % candidate_count;
        let mut seen = 0usize;
        for &backend_id in backend_ids {
            let Some(candidate) = self.candidate_for_backend(route, backend_id, excluded, now)
            else {
                continue;
            };
            if seen == selected {
                return Ok(Some(candidate));
            }
            seen += 1;
        }
        Ok(None)
    }

    async fn choose_weighted_round_robin_candidate(
        &self,
        route: &ModelRoute,
        backend_ids: &[i64],
        excluded: &BackendExclusions,
    ) -> Result<Option<Candidate>, AcquireError> {
        let now = tokio::time::Instant::now();
        let total_weight = backend_ids.iter().fold(0u64, |sum, id| {
            if self
                .candidate_for_backend(route, *id, excluded, now)
                .is_some()
            {
                sum.saturating_add(u64::from(route.endpoint_weight(*id).max(1)))
            } else {
                sum
            }
        });
        if total_weight == 0 {
            return Ok(None);
        }
        let mut slot = self.next_persistent_counter(&route.model_name).await? % total_weight;
        for &backend_id in backend_ids {
            let Some(candidate) = self.candidate_for_backend(route, backend_id, excluded, now)
            else {
                continue;
            };
            let weight = u64::from(route.endpoint_weight(backend_id).max(1));
            if slot < weight {
                return Ok(Some(candidate));
            }
            slot -= weight;
        }
        Ok(None)
    }

    async fn next_persistent_counter(&self, model_name: &str) -> Result<u64, AcquireError> {
        let block = self
            .rr_counters
            .entry(model_name.to_string())
            .or_insert_with(|| Arc::new(CounterBlock::empty()))
            .clone();

        loop {
            let next = block.next.load(Ordering::Relaxed);
            let end = block.end.load(Ordering::Acquire);
            if next < end {
                if block
                    .next
                    .compare_exchange(next, next + 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return Ok(next);
                }
                continue;
            }

            let _guard = block.refill.lock().await;
            let next = block.next.load(Ordering::Relaxed);
            let end = block.end.load(Ordering::Acquire);
            if next < end {
                continue;
            }

            let Some(pool) = &self.counter_pool else {
                return Err(AcquireError::CounterUnavailable(
                    "route counter unavailable: PostgreSQL pool is not configured".to_string(),
                ));
            };
            let size = i64::try_from(self.counter_block_size).unwrap_or(i64::MAX);
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let row = sqlx::query(
                "INSERT INTO model_route_counters (model_name, next_value, updated_at_ms) \
                 VALUES ($1, $2, $3) \
                 ON CONFLICT (model_name) DO UPDATE SET \
                   next_value = model_route_counters.next_value + EXCLUDED.next_value, \
                   updated_at_ms = EXCLUDED.updated_at_ms \
                 RETURNING next_value - $2 AS start_value, next_value AS end_value",
            )
            .bind(model_name)
            .bind(size)
            .bind(now_ms)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                AcquireError::CounterUnavailable(format!("route counter unavailable: {e}"))
            })?;
            let start: i64 = row.try_get("start_value").map_err(|e| {
                AcquireError::CounterUnavailable(format!("route counter decode failed: {e}"))
            })?;
            let end: i64 = row.try_get("end_value").map_err(|e| {
                AcquireError::CounterUnavailable(format!("route counter decode failed: {e}"))
            })?;
            let start = u64::try_from(start.max(0)).unwrap_or(0);
            let end = u64::try_from(end.max(0)).unwrap_or(start);
            if start >= end {
                return Err(AcquireError::CounterUnavailable(
                    "route counter unavailable: empty allocated block".to_string(),
                ));
            }
            block.next.store(start + 1, Ordering::Release);
            block.end.store(end, Ordering::Release);
            return Ok(start);
        }
    }

    fn choose_least_loaded_candidate(
        &self,
        route: &ModelRoute,
        backend_ids: &[i64],
        excluded: &BackendExclusions,
    ) -> Option<Candidate> {
        let now = tokio::time::Instant::now();
        let mut best: Option<Candidate> = None;
        let mut ties = 0usize;

        for &backend_id in backend_ids {
            let Some(candidate) = self.candidate_for_backend(route, backend_id, excluded, now)
            else {
                continue;
            };

            match best {
                None => {
                    best = Some(candidate);
                    ties = 1;
                }
                Some(current) if candidate.score < current.score => {
                    best = Some(candidate);
                    ties = 1;
                }
                Some(current) if candidate.score == current.score => {
                    ties += 1;
                    if rand::rng().random_range(0..ties) == 0 {
                        best = Some(candidate);
                    }
                }
                Some(_) => {}
            }
        }

        best
    }

    /// Ghi nhận kết quả 1 request tới backend: ok = success, false = connect fail/5xx/429 trước byte đầu.
    pub fn note_result(&self, backend_id: i64, ok: bool) {
        let Some(state) = self.states.get(&backend_id) else {
            return;
        };

        if ok {
            state.circuit.store(CIRCUIT_CLOSED, Ordering::Release);
            state.opened_until.store(None);
            state.half_open_probe.store(true, Ordering::Release);
            state.consecutive_failures.store(0, Ordering::Relaxed);
            return;
        }

        let failures = state.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        let current = state.circuit.load(Ordering::Acquire);

        match current {
            CIRCUIT_CLOSED => {
                if failures >= 3
                    && state
                        .circuit
                        .compare_exchange(
                            CIRCUIT_CLOSED,
                            CIRCUIT_OPEN,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                {
                    state.opened_until.store(Some(Arc::new(
                        tokio::time::Instant::now() + self.open_duration,
                    )));
                    state.half_open_probe.store(false, Ordering::Release);
                    state.consecutive_failures.store(0, Ordering::Relaxed);
                }
            }
            CIRCUIT_HALF_OPEN => {
                if state
                    .circuit
                    .compare_exchange(
                        CIRCUIT_HALF_OPEN,
                        CIRCUIT_OPEN,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    state.opened_until.store(Some(Arc::new(
                        tokio::time::Instant::now() + self.open_duration,
                    )));
                    state.half_open_probe.store(false, Ordering::Release);
                    state.consecutive_failures.store(0, Ordering::Relaxed);
                }
            }
            CIRCUIT_OPEN => {
                state.opened_until.store(Some(Arc::new(
                    tokio::time::Instant::now() + self.open_duration,
                )));
            }
            _ => {}
        }
    }

    pub fn inflight(&self, backend_id: i64) -> u32 {
        self.states
            .get(&backend_id)
            .map(|state| state.inflight.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Snapshot (backend_id, inflight, circuit_open) cho metrics gauge (task nền gọi).
    pub fn snapshot(&self) -> Vec<(i64, u32, bool)> {
        self.states
            .iter()
            .map(|entry| {
                let state = entry.value();
                (
                    *entry.key(),
                    state.inflight.load(Ordering::Relaxed),
                    state.circuit.load(Ordering::Acquire) != CIRCUIT_CLOSED,
                )
            })
            .collect()
    }

    #[cfg(test)]
    fn inc_inflight(&self, backend_id: i64) {
        if let Some(state) = self.states.get(&backend_id) {
            state.inflight.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn dec_inflight(&self, backend_id: i64) {
        if let Some(state) = self.states.get(&backend_id) {
            let _ = state
                .inflight
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                    Some(v.saturating_sub(1))
                });
        }
    }

    /// Task active health: GET /health hoặc /v1/models mỗi interval.
    pub fn start_health_loop(self: Arc<Self>, client: reqwest::Client, interval: Duration) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                self.health_check_all(&client).await;
            }
        });
    }

    async fn health_check_all(&self, client: &reqwest::Client) {
        // Không giữ DashMap entry guard qua .await (CODEX runtime-blocker): thu thập target
        // trước, thả guard, rồi mới await network I/O — tránh chặn Tokio worker / hang /healthz.
        // CHỈ health-check backend local/no-auth (llama.cpp/vLLM/Ollama). Cloud provider cần key,
        // unauthenticated /health + /v1/models trả 401/403 -> nếu tính là unhealthy sẽ mở circuit
        // sai và mọi route cloud dính "503 no healthy backend". Cloud dựa vào request success/fail.
        let targets: Vec<(i64, String)> = self
            .states
            .iter()
            .filter_map(|entry| {
                let id = *entry.key();
                let state = entry.value();
                if !state.enabled.load(Ordering::Relaxed) {
                    return None;
                }
                let base = state
                    .base_url
                    .read()
                    .ok()
                    .and_then(|url| url.clone())
                    .unwrap_or_default();
                if base.is_empty() || !is_local_host(&base) {
                    None
                } else {
                    Some((id, base))
                }
            })
            .collect();

        for (id, base) in targets {
            let ok = check_backend_health(client, &base).await;
            self.note_result(id, ok);
        }
    }
}

impl Default for RamBackendPool {
    fn default() -> Self {
        Self::new()
    }
}

/// True nếu base_url là local/private (loopback hoặc RFC1918) — tức là backend không cần
/// key để health-check. Cloud providers (https://api.openai.com ...) trả 401/403 khi gọi
/// unauthenticated, nên KHÔNG được health-check kiểu này (CODEX: cloud circuit dựa vào request).
fn is_local_host(base_url: &str) -> bool {
    let u = base_url.trim().to_ascii_lowercase();
    let host = u
        .split_once("://")
        .map(|(_, rest)| rest.split('/').next().unwrap_or(""))
        .unwrap_or("");
    // Strip port and IPv6 brackets so "127.0.0.1:8088" and "[::1]:8088" match the checks below.
    let host = host.trim_start_matches('[');
    let host = match host.split_once(']') {
        Some((inside, _)) => inside, // bracketed IPv6 [::1]:port
        None => match host.rsplit_once(':') {
            Some((h, port)) if !port.is_empty() && !h.contains(':') => h, // IPv4/hostname:port
            _ => host, // bare hostname/IPv4, or bare IPv6
        },
    };
    host == "127.0.0.1"
        || host == "localhost"
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("172.16.")
        || host.starts_with("172.17.")
        || host.starts_with("172.18.")
        || host.starts_with("172.19.")
        || host.starts_with("172.2")
        || host.starts_with("172.3")
        || host == "::1"
}

async fn check_backend_health(client: &reqwest::Client, base_url: &str) -> bool {
    let base = base_url.trim_end_matches('/');

    if let Ok(resp) = client
        .get(format!("{base}/health"))
        .timeout(Duration::from_secs(3))
        .send()
        .await
        && resp.status().is_success()
    {
        return true;
    }

    if let Ok(resp) = client
        .get(format!("{base}/v1/models"))
        .timeout(Duration::from_secs(3))
        .send()
        .await
    {
        return resp.status().is_success();
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{BackendFormat, ModelEndpoint};

    fn backend(id: i64, weight: u32, max_inflight: u32, enabled: bool) -> Backend {
        Backend {
            id,
            name: format!("backend-{id}"),
            base_url: format!("http://127.0.0.1:{}", 8000 + id),
            api_key_ref: "TEST_KEY_REF".into(),
            api_key: None,
            weight,
            max_inflight,
            format: BackendFormat::OpenAi,
            enabled,
        }
    }

    fn endpoint(backend_id: i64, weight: u32, max_inflight: u32) -> ModelEndpoint {
        ModelEndpoint {
            backend_id,
            provider_model_name: format!("provider-{backend_id}"),
            provider_key_ref: None,
            auth_mode: "bearer".into(),
            protocol: "openai_chat".into(),
            weight,
            max_inflight,
            enabled: true,
            provider_key: None,
        }
    }

    async fn seed_counter_route(pool: &PgPool, model_name: &str) {
        sqlx::query(
            "INSERT INTO model_routes \
             (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, \
              provider_model_name, enabled, auth_mode, protocol, routing_policy) \
             VALUES ($1, '[1,2]', NULL, 4.0, 180, $1, true, 'bearer', 'openai_chat', 'round_robin')",
        )
        .bind(model_name)
        .execute(pool)
        .await
        .expect("seed model route");
    }

    fn route(ids: Vec<i64>) -> ModelRoute {
        ModelRoute {
            model_name: "test-model".into(),
            backend_ids: ids,
            fallback_backend_id: None,
            chars_per_token: 4.0,
            first_byte_timeout: Duration::from_secs(180),
            provider_model_name: "test-model".into(),
            context_tokens: None,
            max_output_tokens: None,
            price_input_per_mtok_usd: None,
            price_output_per_mtok_usd: None,
            enabled: true,
            provider_key_ref: None,
            auth_mode: "bearer".into(),
            protocol: "openai_chat".into(),
            provider_key: None,
            routing_policy: RoutingPolicy::LeastLoadedWeighted,
            endpoints: std::collections::HashMap::new(),
        }
    }

    #[tokio::test]
    async fn least_load_picks_idle_backend() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 100, true));
        pool.upsert_backend(backend(2, 1, 100, true));

        for _ in 0..5 {
            pool.inc_inflight(1);
        }

        let lease = pool
            .acquire(&route(vec![1, 2]))
            .await
            .unwrap()
            .expect("pick backend");
        assert_eq!(lease.backend_id(), 2);
    }

    #[tokio::test]
    async fn max_inflight_skips_backend() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 1, true));
        pool.upsert_backend(backend(2, 1, 10, true));

        pool.inc_inflight(1); // backend 1 đạt max_inflight

        let lease = pool
            .acquire(&route(vec![1, 2]))
            .await
            .unwrap()
            .expect("pick backend 2");
        assert_eq!(lease.backend_id(), 2);

        assert!(pool.acquire(&route(vec![1])).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn no_healthy_backend_returns_none() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 10, false));

        assert!(pool.acquire(&route(vec![1])).await.unwrap().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn circuit_opens_after_3_failures_and_half_opens_after_30s() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 10, true));
        let route = route(vec![1]);

        assert_eq!(pool.acquire(&route).await.unwrap().unwrap().backend_id(), 1);

        pool.note_result(1, false);
        pool.note_result(1, false);
        assert_eq!(pool.acquire(&route).await.unwrap().unwrap().backend_id(), 1); // chưa đủ 3 lỗi

        pool.note_result(1, false);
        assert!(pool.acquire(&route).await.unwrap().is_none()); // circuit mở

        tokio::time::advance(Duration::from_secs(29)).await;
        assert!(pool.acquire(&route).await.unwrap().is_none()); // vẫn mở

        tokio::time::advance(Duration::from_secs(2)).await; // tổng 31 giây
        assert_eq!(pool.acquire(&route).await.unwrap().unwrap().backend_id(), 1); // half-open 1 request
        assert!(pool.acquire(&route).await.unwrap().is_none()); // request thứ hai bị chặn

        pool.note_result(1, true);
        assert_eq!(pool.acquire(&route).await.unwrap().unwrap().backend_id(), 1); // circuit đóng lại
    }

    fn route_with_fallback(ids: Vec<i64>, fallback: i64) -> ModelRoute {
        ModelRoute {
            model_name: "test-model".into(),
            backend_ids: ids,
            fallback_backend_id: Some(fallback),
            chars_per_token: 4.0,
            first_byte_timeout: Duration::from_secs(180),
            provider_model_name: "test-model".into(),
            context_tokens: None,
            max_output_tokens: None,
            price_input_per_mtok_usd: None,
            price_output_per_mtok_usd: None,
            enabled: true,
            provider_key_ref: None,
            auth_mode: "bearer".into(),
            protocol: "openai_chat".into(),
            provider_key: None,
            routing_policy: RoutingPolicy::LeastLoadedWeighted,
            endpoints: std::collections::HashMap::new(),
        }
    }

    #[tokio::test]
    async fn fallback_not_used_while_primary_available() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 100, true));
        pool.upsert_backend(backend(2, 1, 100, true)); // fallback

        let lease = pool
            .acquire(&route_with_fallback(vec![1], 2))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(lease.backend_id(), 1);
    }

    #[tokio::test]
    async fn fallback_used_after_primary_exhausted() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 100, false)); // primary disabled
        pool.upsert_backend(backend(2, 1, 100, true)); // fallback

        let lease = pool
            .acquire(&route_with_fallback(vec![1], 2))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(lease.backend_id(), 2);
    }

    #[tokio::test]
    async fn acquire_excluding_skips_tried_backend() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 100, true));
        pool.upsert_backend(backend(2, 1, 100, true));

        let mut excluded = BackendExclusions::default();
        excluded.insert(1);
        let lease = pool
            .acquire_excluding(&route(vec![1, 2]), &mut excluded)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(lease.backend_id(), 2);
    }

    #[tokio::test]
    async fn round_robin_without_counter_pool_returns_error() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 100, true));
        let mut r = route(vec![1]);
        r.routing_policy = RoutingPolicy::RoundRobin;
        match pool.acquire(&r).await {
            Err(AcquireError::CounterUnavailable(_)) => {}
            Ok(_) => panic!("round_robin without PostgreSQL counter pool must fail"),
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn round_robin_uses_postgres_counter_and_ignores_weight(pg: PgPool) {
        seed_counter_route(&pg, "rr-model").await;
        let pool = Arc::new(RamBackendPool::new_with_counter_pool(pg.clone(), 1));
        pool.upsert_backend(backend(1, 1, 100, true));
        pool.upsert_backend(backend(2, 1, 100, true));
        let mut r = route(vec![1, 2]);
        r.model_name = "rr-model".into();
        r.routing_policy = RoutingPolicy::RoundRobin;
        r.endpoints.insert(1, endpoint(1, 99, 0));
        r.endpoints.insert(2, endpoint(2, 1, 0));

        let mut seen = Vec::new();
        for _ in 0..4 {
            let lease = pool.acquire(&r).await.unwrap().unwrap();
            seen.push(lease.backend_id());
            drop(lease);
        }
        assert_eq!(seen, vec![1, 2, 1, 2]);
        let next_value: i64 = sqlx::query_scalar(
            "SELECT next_value FROM model_route_counters WHERE model_name = 'rr-model'",
        )
        .fetch_one(&pg)
        .await
        .unwrap();
        assert_eq!(next_value, 4);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn weighted_round_robin_honors_endpoint_weight(pg: PgPool) {
        seed_counter_route(&pg, "wrr-model").await;
        let pool = Arc::new(RamBackendPool::new_with_counter_pool(pg.clone(), 1));
        pool.upsert_backend(backend(1, 1, 100, true));
        pool.upsert_backend(backend(2, 1, 100, true));
        let mut r = route(vec![1, 2]);
        r.model_name = "wrr-model".into();
        r.routing_policy = RoutingPolicy::WeightedRoundRobin;
        r.endpoints.insert(1, endpoint(1, 3, 0));
        r.endpoints.insert(2, endpoint(2, 1, 0));

        let mut seen = Vec::new();
        for _ in 0..4 {
            let lease = pool.acquire(&r).await.unwrap().unwrap();
            seen.push(lease.backend_id());
            drop(lease);
        }
        assert_eq!(seen, vec![1, 1, 1, 2]);
    }

    #[tokio::test]
    async fn fallback_not_retried_if_already_excluded() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 100, false)); // primary disabled
        pool.upsert_backend(backend(2, 1, 100, true)); // fallback

        let mut excluded = BackendExclusions::default();
        excluded.insert(2);
        assert!(
            pool.acquire_excluding(&route_with_fallback(vec![1], 2), &mut excluded)
                .await
                .unwrap()
                .is_none()
        );
    }
}
