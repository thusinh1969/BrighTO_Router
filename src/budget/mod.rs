//! Budget + rate limit + concurrency, RAM atomic. Hot path chỉ thao tác AtomicU64 trên DashMap.
//! Reserve dùng CAS để chống TOCTOU (C1/GLM P0); Reservation tự hoàn trả khi Drop nếu chưa commit.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;

use crate::contract::{ApiKey, Budget, BudgetError, Team, UsageScope, UsageSeed};

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
struct UsageKey {
    scope: u8,
    id: i64,
    model: String,
    period_start: i64,
}

const SCOPE_KEY_TOTAL: u8 = 0;
const SCOPE_KEY_MODEL: u8 = 1;
const SCOPE_TEAM_TOTAL: u8 = 2;
const SCOPE_TEAM_MODEL: u8 = 3;

/// Một scope budget đã được reserve (đã tăng counter atomic). Commit điều chỉnh est -> actual.
#[derive(Debug)]
struct ReservedScope {
    key: UsageKey,
    amount: u64,
}

/// Reservation trả về từ reserve(). Commit hoặc rollback tường minh; nếu bị Drop mà chưa
/// commit thì tự rollback (an toàn khi request fail giữa chừng).
#[derive(Debug)]
pub struct BudgetReservation {
    store: Arc<RamBudgetStore>,
    scopes: Vec<ReservedScope>,
    consumed: bool,
}

impl BudgetReservation {
    /// Điều chỉnh reservation từ est_tokens -> actual_tokens (cộng thêm nếu vượt, hoàn trả nếu hụt).
    pub fn commit(mut self, actual_tokens: u64) {
        let scopes = std::mem::take(&mut self.scopes);
        self.store.commit_scopes(&scopes, actual_tokens);
        self.consumed = true;
    }

    /// Hoàn trả toàn bộ phần đã reserve.
    pub fn rollback(mut self) {
        let scopes = std::mem::take(&mut self.scopes);
        self.store.rollback_scopes(&scopes);
        self.consumed = true;
    }
}

impl Drop for BudgetReservation {
    fn drop(&mut self) {
        if !self.consumed {
            let scopes = std::mem::take(&mut self.scopes);
            self.store.rollback_scopes(&scopes);
        }
    }
}

/// RAII guard cho concurrency limit: release slot khi Drop.
#[derive(Debug)]
pub struct ConcurrencyGuard {
    store: Arc<RamBudgetStore>,
    key_id: Option<i64>,
}

impl Drop for ConcurrencyGuard {
    fn drop(&mut self) {
        if let Some(id) = self.key_id {
            self.store.release_concurrency_id(id);
        }
    }
}

#[derive(Debug)]
struct RpmBucket {
    tokens: AtomicU64,
    last_refill_ms: AtomicU64,
}

impl RpmBucket {
    fn new(limit: u32) -> Self {
        Self {
            tokens: AtomicU64::new(limit as u64),
            last_refill_ms: AtomicU64::new(now_ms()),
        }
    }

    /// Lấy 1 request token. Err(retry_after) nếu bucket rỗng.
    fn try_take(&self, limit: u32) -> Result<(), Duration> {
        let lim = u64::from(limit);
        let now = now_ms();
        let mut last = self.last_refill_ms.load(Ordering::Acquire);
        let mut tokens = self.tokens.load(Ordering::Acquire);
        loop {
            let elapsed = now.saturating_sub(last);
            let add = ((elapsed as u128 * lim as u128) / 60_000) as u64;
            let new_last = if add > 0 { now } else { last };
            let replenished = (tokens + add).min(lim);
            if replenished == 0 {
                let retry_after =
                    Duration::from_millis((last + 60_000 / lim.max(1)).saturating_sub(now));
                return Err(retry_after);
            }
            let new_tokens = replenished - 1;
            match self.tokens.compare_exchange_weak(
                tokens,
                new_tokens,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    if add > 0 {
                        let _ = self.last_refill_ms.compare_exchange_weak(
                            last,
                            new_last,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        );
                    }
                    return Ok(());
                }
                Err(cur) => {
                    tokens = cur;
                    last = self.last_refill_ms.load(Ordering::Acquire);
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct RamBudgetStore {
    rpm: DashMap<i64, RpmBucket>,
    usage: DashMap<UsageKey, AtomicU64>,
    inflight: DashMap<i64, AtomicU64>,
    team_budgets: DashMap<i64, Budget>,
}

impl RamBudgetStore {
    pub fn new() -> Self {
        Self {
            rpm: DashMap::new(),
            usage: DashMap::new(),
            inflight: DashMap::new(),
            team_budgets: DashMap::new(),
        }
    }

    /// Nạp team budgets từ snapshot. Gọi sau bootstrap và mỗi lần config reload.
    pub fn load_teams(&self, teams: &HashMap<i64, Team>) {
        for (id, team) in teams {
            match &team.budget {
                Some(b) => {
                    self.team_budgets.insert(*id, b.clone());
                }
                None => {
                    self.team_budgets.remove(id);
                }
            }
        }
    }

    /// Đồng bộ toàn bộ teams từ snapshot: prune team không còn, chỉ giữ budget của team enabled.
    pub fn sync_teams(&self, teams: &HashMap<i64, Team>) {
        let current: Vec<i64> = self.team_budgets.iter().map(|entry| *entry.key()).collect();
        for id in current {
            if !teams.contains_key(&id) {
                self.team_budgets.remove(&id);
            }
        }
        for (id, team) in teams {
            if team.enabled {
                if let Some(b) = &team.budget {
                    self.team_budgets.insert(*id, b.clone());
                } else {
                    self.team_budgets.remove(id);
                }
            } else {
                self.team_budgets.remove(id);
            }
        }
    }

    /// Atomic reserve est_tokens trên mọi scope budget + lấy 1 slot RPM.
    /// Trả BudgetReservation (RAII) để commit/rollback.
    pub fn reserve(
        self: &Arc<Self>,
        key: &ApiKey,
        model: &str,
        est_tokens: u64,
    ) -> Result<BudgetReservation, BudgetError> {
        let mut scopes: Vec<ReservedScope> = Vec::new();

        if let Some((is_key, scope_id, budget)) = self.effective_budget(key) {
            let period = period_start(&budget.period, now_secs());
            let (total_scope, model_scope) = if is_key {
                (SCOPE_KEY_TOTAL, SCOPE_KEY_MODEL)
            } else {
                (SCOPE_TEAM_TOTAL, SCOPE_TEAM_MODEL)
            };
            if let Err(remaining) = self.reserve_scope(
                total_scope,
                scope_id,
                "",
                period,
                budget.max_tokens,
                est_tokens,
                &mut scopes,
            ) {
                self.rollback_scopes(&scopes);
                return Err(BudgetError::BudgetExceeded { remaining });
            }
            if let Some(cap) = budget.per_model.get(model)
                && let Err(remaining) = self.reserve_scope(
                    model_scope,
                    scope_id,
                    model,
                    period,
                    *cap,
                    est_tokens,
                    &mut scopes,
                )
            {
                self.rollback_scopes(&scopes);
                return Err(BudgetError::BudgetExceeded { remaining });
            }
        }

        if let Some(limit) = key.rpm_limit.filter(|l| *l > 0)
            && let Err(retry_after) = self.take_rpm(key.id, limit)
        {
            self.rollback_scopes(&scopes);
            return Err(BudgetError::RateLimited { retry_after });
        }

        Ok(BudgetReservation {
            store: Arc::clone(self),
            scopes,
            consumed: false,
        })
    }

    /// Acquire concurrency slot cho key. Some(guard) — guard tự release khi Drop.
    /// None nếu đã chạm concurrency_limit. Không giới hạn -> guard no-op (key_id = None).
    pub fn acquire_concurrency(self: &Arc<Self>, key: &ApiKey) -> Option<ConcurrencyGuard> {
        match key.concurrency_limit {
            None | Some(0) => Some(ConcurrencyGuard {
                store: Arc::clone(self),
                key_id: None,
            }),
            Some(limit) => {
                let gauge = self
                    .inflight
                    .entry(key.id)
                    .or_insert_with(|| AtomicU64::new(0));
                let mut current = gauge.load(Ordering::Acquire);
                loop {
                    if current >= u64::from(limit) {
                        return None;
                    }
                    match gauge.compare_exchange_weak(
                        current,
                        current + 1,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => {
                            return Some(ConcurrencyGuard {
                                store: Arc::clone(self),
                                key_id: Some(key.id),
                            });
                        }
                        Err(observed) => current = observed,
                    }
                }
            }
        }
    }

    /// Seed counter từ usage_ledger lúc boot (chống reset budget sau restart). Gọi trước khi listen.
    pub fn seed_usage(&self, seed: &UsageSeed) {
        let scope = match seed.scope {
            UsageScope::KeyTotal => SCOPE_KEY_TOTAL,
            UsageScope::KeyModel => SCOPE_KEY_MODEL,
            UsageScope::TeamTotal => SCOPE_TEAM_TOTAL,
            UsageScope::TeamModel => SCOPE_TEAM_MODEL,
        };
        let key = UsageKey {
            scope,
            id: seed.id,
            model: seed.model.clone(),
            period_start: seed.period_start,
        };
        let counter = self.usage.entry(key).or_insert_with(|| AtomicU64::new(0));
        counter.store(seed.used_tokens, Ordering::Release);
    }

    /// Snapshot remaining của team budgets cho metrics gauge (task nền gọi, không phải hot path).
    pub fn budget_remaining_snapshot(&self) -> Vec<(i64, String, u64)> {
        let mut out = Vec::new();
        for entry in self.team_budgets.iter() {
            let team_id = *entry.key();
            let budget = entry.value();
            let period = period_start(&budget.period, now_secs());
            let used = self.current_usage(SCOPE_TEAM_TOTAL, team_id, "", period);
            out.push((
                team_id,
                "total".to_string(),
                budget.max_tokens.saturating_sub(used),
            ));
            for (model, cap) in &budget.per_model {
                let used = self.current_usage(SCOPE_TEAM_MODEL, team_id, model, period);
                out.push((team_id, model.clone(), cap.saturating_sub(used)));
            }
        }
        out
    }

    // ----- internal -----

    fn release_concurrency_id(&self, key_id: i64) {
        if let Some(gauge) = self.inflight.get(&key_id) {
            let _ = gauge.fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                Some(v.saturating_sub(1))
            });
        }
    }

    fn effective_budget(&self, key: &ApiKey) -> Option<(bool, i64, Budget)> {
        if let Some(b) = &key.budget {
            return Some((true, key.id, b.clone()));
        }
        if let Some(b) = self.team_budgets.get(&key.team_id) {
            return Some((false, key.team_id, b.clone()));
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn reserve_scope(
        &self,
        scope: u8,
        id: i64,
        model: &str,
        period_start: i64,
        limit: u64,
        amount: u64,
        scopes: &mut Vec<ReservedScope>,
    ) -> Result<(), u64> {
        let key = UsageKey {
            scope,
            id,
            model: model.to_string(),
            period_start,
        };
        // get-first: đọc (read lock) cho case key đã tồn tại (hot path ~mọi request);
        // entry() (write lock) CHỈ khi miss. Tránh write-lock shard DashMap trên mỗi request.
        let guard = match self.usage.get(&key) {
            Some(g) => g,
            None => {
                // miss hiếm (lần đầu / period mới): get-or-insert atomic (write lock ngắn) rồi release.
                drop(
                    self.usage
                        .entry(key.clone())
                        .or_insert_with(|| AtomicU64::new(0)),
                );
                self.usage.get(&key).expect("just inserted")
            }
        };
        let counter = &*guard;
        let mut current = counter.load(Ordering::Acquire);
        loop {
            let remaining = limit.saturating_sub(current);
            if amount > remaining {
                return Err(remaining);
            }
            match counter.compare_exchange_weak(
                current,
                current + amount,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    scopes.push(ReservedScope { key, amount });
                    return Ok(());
                }
                Err(observed) => current = observed,
            }
        }
    }

    fn commit_scopes(&self, scopes: &[ReservedScope], actual_tokens: u64) {
        for s in scopes {
            if actual_tokens >= s.amount {
                self.add(&s.key, actual_tokens - s.amount);
            } else {
                self.sub(&s.key, s.amount - actual_tokens);
            }
        }
    }

    fn rollback_scopes(&self, scopes: &[ReservedScope]) {
        for s in scopes {
            self.sub(&s.key, s.amount);
        }
    }

    fn add(&self, key: &UsageKey, amount: u64) {
        if amount == 0 {
            return;
        }
        if let Some(c) = self.usage.get(key) {
            c.fetch_add(amount, Ordering::AcqRel);
        }
    }

    fn sub(&self, key: &UsageKey, amount: u64) {
        if amount == 0 {
            return;
        }
        if let Some(c) = self.usage.get(key) {
            let _ = c.fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                Some(v.saturating_sub(amount))
            });
        }
    }

    fn current_usage(&self, scope: u8, id: i64, model: &str, period_start: i64) -> u64 {
        let key = UsageKey {
            scope,
            id,
            model: model.to_string(),
            period_start,
        };
        self.usage
            .get(&key)
            .map(|v| v.load(Ordering::Acquire))
            .unwrap_or(0)
    }

    fn take_rpm(&self, key_id: i64, limit: u32) -> Result<(), Duration> {
        let bucket = self
            .rpm
            .entry(key_id)
            .or_insert_with(|| RpmBucket::new(limit));
        bucket.try_take(limit)
    }
}

impl Default for RamBudgetStore {
    fn default() -> Self {
        Self::new()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_millis() as u64
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_secs() as i64
}

pub fn period_start(period: &str, now_secs: i64) -> i64 {
    match period {
        "month" => {
            let days = now_secs.div_euclid(86_400);
            let (year, month, _) = civil_from_days(days);
            let start_days = days_from_civil(year, month, 1);
            start_days * 86_400
        }
        _ => {
            let days = now_secs.div_euclid(86_400);
            days * 86_400
        }
    }
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 {
        z / 146097
    } else {
        (z - 146096) / 146097
    };
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ApiKey, Budget};

    fn make_key(budget: Option<Budget>, rpm_limit: Option<u32>) -> ApiKey {
        ApiKey {
            id: 1,
            key_hash: [0; 32],
            key_prefix: "test".into(),
            team_id: 10,
            owner: "tester".into(),
            allowed_models: vec![],
            budget,
            rpm_limit,
            concurrency_limit: None,
            expires_at: None,
            enabled: true,
        }
    }

    #[test]
    fn budget_reserves_commits_and_blocks() {
        let store = Arc::new(RamBudgetStore::new());
        let budget = Budget {
            period: "day".into(),
            max_tokens: 10_000,
            max_usd_cents: None,
            per_model: HashMap::new(),
        };
        let key = make_key(Some(budget), None);

        assert!(matches!(
            store.reserve(&key, "model-a", 10_001),
            Err(BudgetError::BudgetExceeded { remaining: 10_000 })
        ));

        let res = store.reserve(&key, "model-a", 10_000).unwrap();
        res.commit(10_000);
        assert!(matches!(
            store.reserve(&key, "model-a", 1),
            Err(BudgetError::BudgetExceeded { remaining: 0 })
        ));
    }

    #[test]
    fn partial_scope_failure_rolls_back_total() {
        let store = Arc::new(RamBudgetStore::new());
        let budget = Budget {
            period: "day".into(),
            max_tokens: 1000,
            max_usd_cents: None,
            per_model: HashMap::from([("model-a".to_string(), 50)]),
        };
        let key = make_key(Some(budget), None);

        assert!(matches!(
            store.reserve(&key, "model-a", 70),
            Err(BudgetError::BudgetExceeded { .. })
        ));
        let res = store.reserve(&key, "model-b", 1000).unwrap();
        res.commit(1000);
    }

    #[test]
    fn commit_refunds_when_actual_lower_than_estimate() {
        let store = Arc::new(RamBudgetStore::new());
        let budget = Budget {
            period: "day".into(),
            max_tokens: 1000,
            max_usd_cents: None,
            per_model: HashMap::new(),
        };
        let key = make_key(Some(budget), None);

        let res = store.reserve(&key, "model-a", 100).unwrap();
        res.commit(30);

        let res2 = store.reserve(&key, "model-a", 970).unwrap();
        res2.commit(970);
        assert!(matches!(
            store.reserve(&key, "model-a", 1),
            Err(BudgetError::BudgetExceeded { remaining: 0 })
        ));
    }

    #[test]
    fn dropped_reservation_rolls_back() {
        let store = Arc::new(RamBudgetStore::new());
        let budget = Budget {
            period: "day".into(),
            max_tokens: 100,
            max_usd_cents: None,
            per_model: HashMap::new(),
        };
        let key = make_key(Some(budget), None);

        {
            let _res = store.reserve(&key, "model-a", 100).unwrap();
        }
        let res = store.reserve(&key, "model-a", 100).unwrap();
        res.commit(0);
    }

    #[test]
    fn rpm_bucket_refills() {
        let store = Arc::new(RamBudgetStore::new());
        let key = make_key(None, Some(60));

        for _ in 0..60 {
            let res = store.reserve(&key, "model-a", 1).unwrap();
            res.commit(0);
        }
        assert!(matches!(
            store.reserve(&key, "model-a", 1),
            Err(BudgetError::RateLimited { .. })
        ));

        if let Some(bucket) = store.rpm.get(&key.id) {
            let now = now_ms();
            bucket.last_refill_ms.store(now - 60_000, Ordering::Release);
        }
        let res = store.reserve(&key, "model-a", 1).unwrap();
        res.commit(0);
    }

    #[test]
    fn concurrency_guard_acquire_release() {
        let store = Arc::new(RamBudgetStore::new());
        let mut key = make_key(None, None);
        key.concurrency_limit = Some(1);

        let g1 = store.acquire_concurrency(&key).unwrap();
        assert!(store.acquire_concurrency(&key).is_none());
        drop(g1);
        let g2 = store.acquire_concurrency(&key).unwrap();
        drop(g2);
    }
}
