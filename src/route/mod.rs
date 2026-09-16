//! Chọn backend least-load (inflight/weight, hoà -> random), circuit breaker, health.
//! acquire() trả BackendLease (RAII): pick + inc inflight trong một guard, Drop -> dec inflight.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use arc_swap::ArcSwapOption;
use dashmap::DashMap;
use rand::RngExt; // rand 0.10: random_range nằm trên trait Rng

use crate::contract::{Backend, ModelRoute};

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
    open_duration: Duration,
}

impl RamBackendPool {
    pub fn new() -> Self {
        Self {
            states: DashMap::new(),
            open_duration: Duration::from_secs(30),
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
    pub fn acquire(self: &Arc<Self>, route: &ModelRoute) -> Option<BackendLease> {
        let mut excluded = BackendExclusions::default();
        self.acquire_excluding(route, &mut excluded)
    }

    /// Primary least-load trước; hết primary mới thử fallback_backend_id (nếu có, chưa thử).
    pub fn acquire_excluding(
        self: &Arc<Self>,
        route: &ModelRoute,
        excluded: &mut BackendExclusions,
    ) -> Option<BackendLease> {
        if let Some(lease) = self.acquire_primary(&route.backend_ids, excluded) {
            return Some(lease);
        }
        let fallback = route.fallback_backend_id?;
        if excluded.contains(fallback) {
            return None;
        }
        self.acquire_primary(std::slice::from_ref(&fallback), excluded)
    }

    fn acquire_primary(
        self: &Arc<Self>,
        backend_ids: &[i64],
        excluded: &mut BackendExclusions,
    ) -> Option<BackendLease> {
        loop {
            let candidate = self.choose_candidate(backend_ids, excluded)?;
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
            let max = state.max_inflight.load(Ordering::Relaxed);
            let prev = state.inflight.fetch_add(1, Ordering::Relaxed);
            if max > 0 && prev >= max {
                state.inflight.fetch_sub(1, Ordering::Relaxed);
                if candidate.half_open {
                    state.half_open_probe.store(true, Ordering::Release);
                }
                excluded.insert(candidate.backend_id);
                continue;
            }

            return Some(BackendLease {
                pool: Arc::clone(self),
                backend_id: candidate.backend_id,
            });
        }
    }

    /// Chọn candidate least-load (chưa giữ slot; circuit-aware). None = không còn ứng viên.
    fn choose_candidate(
        &self,
        backend_ids: &[i64],
        excluded: &BackendExclusions,
    ) -> Option<Candidate> {
        let now = tokio::time::Instant::now();
        let mut best: Option<Candidate> = None;
        let mut ties = 0usize;

        for &backend_id in backend_ids {
            if excluded.contains(backend_id) {
                continue;
            }
            let Some(state) = self.states.get(&backend_id) else {
                continue;
            };
            if !state.enabled.load(Ordering::Relaxed) {
                continue;
            }

            let mut half_open = false;
            match state.circuit.load(Ordering::Acquire) {
                CIRCUIT_CLOSED => {}
                CIRCUIT_OPEN => {
                    let until = state.opened_until.load();
                    let expired = until.as_ref().map(|i| **i <= now).unwrap_or(true);
                    drop(until);
                    if !expired {
                        continue;
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
                        continue;
                    }
                }
                CIRCUIT_HALF_OPEN => {
                    if state.half_open_probe.load(Ordering::Acquire) {
                        half_open = true;
                    } else {
                        continue;
                    }
                }
                _ => continue,
            }

            let inflight = state.inflight.load(Ordering::Relaxed);
            let max = state.max_inflight.load(Ordering::Relaxed);
            if max > 0 && inflight >= max {
                continue;
            }

            let weight = state.weight.load(Ordering::Relaxed).max(1) as f64;
            let candidate = Candidate {
                backend_id,
                score: inflight as f64 / weight,
                half_open,
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
        let ids: Vec<i64> = self.states.iter().map(|entry| *entry.key()).collect();
        for id in ids {
            if let Some(state) = self.states.get(&id) {
                if !state.enabled.load(Ordering::Relaxed) {
                    continue;
                }
                let base = state
                    .base_url
                    .read()
                    .ok()
                    .and_then(|url| url.clone())
                    .unwrap_or_default();
                if base.is_empty() {
                    continue;
                }
                let ok = check_backend_health(client, &base).await;
                self.note_result(id, ok);
            }
        }
    }
}

impl Default for RamBackendPool {
    fn default() -> Self {
        Self::new()
    }
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
    use crate::contract::BackendFormat;

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
        }
    }

    #[test]
    fn least_load_picks_idle_backend() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 100, true));
        pool.upsert_backend(backend(2, 1, 100, true));

        for _ in 0..5 {
            pool.inc_inflight(1);
        }

        let lease = pool.acquire(&route(vec![1, 2])).expect("pick backend");
        assert_eq!(lease.backend_id(), 2);
    }

    #[test]
    fn max_inflight_skips_backend() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 1, true));
        pool.upsert_backend(backend(2, 1, 10, true));

        pool.inc_inflight(1); // backend 1 đạt max_inflight

        let lease = pool.acquire(&route(vec![1, 2])).expect("pick backend 2");
        assert_eq!(lease.backend_id(), 2);

        assert!(pool.acquire(&route(vec![1])).is_none());
    }

    #[test]
    fn no_healthy_backend_returns_none() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 10, false));

        assert!(pool.acquire(&route(vec![1])).is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn circuit_opens_after_3_failures_and_half_opens_after_30s() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 10, true));
        let route = route(vec![1]);

        assert_eq!(pool.acquire(&route).unwrap().backend_id(), 1);

        pool.note_result(1, false);
        pool.note_result(1, false);
        assert_eq!(pool.acquire(&route).unwrap().backend_id(), 1); // chưa đủ 3 lỗi

        pool.note_result(1, false);
        assert!(pool.acquire(&route).is_none()); // circuit mở

        tokio::time::advance(Duration::from_secs(29)).await;
        assert!(pool.acquire(&route).is_none()); // vẫn mở

        tokio::time::advance(Duration::from_secs(2)).await; // tổng 31 giây
        assert_eq!(pool.acquire(&route).unwrap().backend_id(), 1); // half-open 1 request
        assert!(pool.acquire(&route).is_none()); // request thứ hai bị chặn

        pool.note_result(1, true);
        assert_eq!(pool.acquire(&route).unwrap().backend_id(), 1); // circuit đóng lại
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
        }
    }

    #[test]
    fn fallback_not_used_while_primary_available() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 100, true));
        pool.upsert_backend(backend(2, 1, 100, true)); // fallback

        let lease = pool.acquire(&route_with_fallback(vec![1], 2)).unwrap();
        assert_eq!(lease.backend_id(), 1);
    }

    #[test]
    fn fallback_used_after_primary_exhausted() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 100, false)); // primary disabled
        pool.upsert_backend(backend(2, 1, 100, true)); // fallback

        let lease = pool.acquire(&route_with_fallback(vec![1], 2)).unwrap();
        assert_eq!(lease.backend_id(), 2);
    }

    #[test]
    fn acquire_excluding_skips_tried_backend() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 100, true));
        pool.upsert_backend(backend(2, 1, 100, true));

        let mut excluded = BackendExclusions::default();
        excluded.insert(1);
        let lease = pool
            .acquire_excluding(&route(vec![1, 2]), &mut excluded)
            .unwrap();
        assert_eq!(lease.backend_id(), 2);
    }

    #[test]
    fn fallback_not_retried_if_already_excluded() {
        let pool = Arc::new(RamBackendPool::new());
        pool.upsert_backend(backend(1, 1, 100, false)); // primary disabled
        pool.upsert_backend(backend(2, 1, 100, true)); // fallback

        let mut excluded = BackendExclusions::default();
        excluded.insert(2);
        assert!(
            pool.acquire_excluding(&route_with_fallback(vec![1], 2), &mut excluded)
                .is_none()
        );
    }
}
