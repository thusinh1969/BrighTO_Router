# CODEX current integration plan — finish RAII refactor, do not rollback budget

## Superseded-state notice

Part of this plan has already been implemented in the current worktree: `src/ledger/mod.rs` now defines `LedgerSink`, `src/metrics.rs` now defines public `Metrics`, `src/budget/mod.rs` has `BudgetReservation`/`ConcurrencyGuard`, and `src/route/mod.rs` has `BackendLease`/`RamBackendPool::acquire`.

Use `CODEX-current-compile-frontier-20260916.md` as the current source of truth for the remaining compile blockers. Do not recreate `LedgerSink` or `Metrics`; integrate the existing ones.

Time: 2026-09-16 18:xx ICT. Scope: current worktree. I did not edit `src/`.

## Current verdict

The repo is still red at `cargo check --all-targets`, but the state has moved since the previous audit:

- `src/budget/mod.rs` now has `BudgetReservation` with Drop rollback and `ConcurrencyGuard`.
- `src/route/mod.rs` now has `BackendLease` and `RamBackendPool::acquire`.
- `src/contract.rs` still imports concrete runtime types and still exposes `AppState`.
- `src/handlers.rs`, `src/proxy/mod.rs`, and `src/main.rs` still use old field names and old APIs.

Do not rollback `BudgetReservation` and `BackendLease`. They are the right direction for SOTA correctness because they close the budget/concurrency/inflight TOCTOU holes. The next patch should finish the integration cut.

Proof command:

```bash
cargo check --all-targets
```

Current first errors:

```text
error[E0432]: unresolved import `crate::ledger::LedgerSink`
  --> src/contract.rs:16:5
error[E0432]: unresolved import `crate::metrics::Metrics`
  --> src/contract.rs:17:5
error[E0432]: unresolved imports `crate::contract::BudgetStore`, `crate::contract::Decision`, `crate::contract::RouteDecision`, `crate::contract::SkipReason`
  --> src/handlers.rs:19:47
error[E0609]: no field `pool` on type `Arc<AppState>`
  --> src/handlers.rs:259:40
error[E0609]: no field `ledger_tx` on type `Arc<AppState>`
  --> src/handlers.rs:338:25
error[E0599]: no method named `try_reserve` found for struct `Arc<RamBudgetStore>`
  --> src/handlers.rs:242:30
```

## Required architecture cut

### 1. Move concrete runtime `AppState` out of `contract.rs`

`contract.rs` should keep pure shared data types only:

- `BackendFormat`
- `Backend`
- `ModelRoute`
- `Team`
- `Budget`
- `ApiKey`
- `ConfigSnapshot`
- `BudgetError`
- `UsageEvent`

Remove these imports from `contract.rs`:

```rust
use crate::budget::RamBudgetStore;
use crate::ledger::LedgerSink;
use crate::metrics::Metrics;
use crate::route::RamBackendPool;
```

Create `src/state.rs` and export it from `src/lib.rs`:

```rust
pub mod state;
```

`src/state.rs` should own concrete runtime wiring:

```rust
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::budget::RamBudgetStore;
use crate::contract::ConfigSnapshot;
use crate::ledger::LedgerSink;
use crate::metrics::Metrics;
use crate::route::RamBackendPool;

pub struct AppState {
    pub cfg: Arc<ArcSwap<ConfigSnapshot>>,
    pub budget: Arc<RamBudgetStore>,
    pub backends: Arc<RamBackendPool>,
    pub client: reqwest::Client,
    pub ledger: LedgerSink,
    pub metrics: Metrics,
    pub max_body_bytes: usize,
}
```

Then update imports:

- `handlers.rs`: import `crate::state::AppState`.
- `proxy.rs`: import `crate::state::AppState`.
- `main.rs`: import `brigto_router::state::AppState`.

This removes the current cyclic concrete dependency from `contract.rs`.

### 2. Add `LedgerSink` before touching handler/proxy

Add in `src/ledger/mod.rs`:

```rust
#[derive(Clone)]
pub struct LedgerSink {
    tx: mpsc::Sender<UsageEvent>,
    queued: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
}

impl LedgerSink {
    pub fn new(tx: mpsc::Sender<UsageEvent>) -> Self { ... }
    pub fn try_record(&self, ev: UsageEvent) {
        match self.tx.try_send(ev) {
            Ok(()) => { self.queued.fetch_add(1, Ordering::Relaxed); }
            Err(_) => { self.dropped.fetch_add(1, Ordering::Relaxed); }
        }
    }
}
```

Rules:

- No fallback file write in `try_record`.
- No async/await in `try_record`.
- Handler/proxy must not see raw `mpsc::Sender<UsageEvent>` after this patch.

### 3. Move public `Metrics` into `src/metrics.rs`

Current `src/metrics.rs` only has `METRICS`, while `handlers.rs:43` has a private `Metrics`. Move a public atomic metrics struct to `src/metrics.rs`.

Pick one metric namespace now. Prefer existing locked names in `src/metrics.rs`:

- `router_requests_total`
- `router_tokens_total`
- `router_ttfb_seconds`
- `router_overhead_seconds`
- `router_backend_inflight`
- `router_budget_remaining`
- `router_circuit_open`

Do not keep handler-local `brigto_router_*` metrics and module-level `router_*` metrics at the same time.

### 4. Update handler to use new budget and route guards

Replace old calls:

```rust
state.inner.budget.try_reserve(...)
state.inner.budget.commit(...)
state.inner.pool.pick(...)
state.inner.ledger_tx.try_send(...)
```

with the new guard flow:

```rust
let reservation = match state.inner.budget.reserve(&key, &model, est_tokens) {
    Ok(r) => r,
    Err(BudgetError::RateLimited { retry_after }) => return 429,
    Err(BudgetError::BudgetExceeded { .. }) => return 429,
};

let concurrency = match state.inner.budget.acquire_concurrency(&key) {
    Ok(g) => g,
    Err(()) => return 429,
};
```

Then call the streaming proxy path. Do not keep the buffered local `forward_to_backend` path.

Important ownership rule:

- `reservation` and `concurrency` must live until the upstream response is finished or aborted.
- For streaming, this means they must move into the stream/reporter task, not drop right after `proxy_forward` returns the `Response<Body>`.
- `BudgetReservation::commit(actual_tokens)` should happen when usage is known.
- If upstream errors or client aborts before completion, Drop rollback is acceptable.

Do not commit budget in `handlers.rs` after the proxy is moved to streaming. The proxy/reporter sees final token usage and must own final commit/refund.

### 5. Update proxy to use `BackendLease` and shared client

Replace:

```rust
let client = Client::builder()...
state.pool.pick(...)
state.pool.note_result(...)
state.ledger_tx.clone()
```

with:

```rust
let client = &state.client;
let lease = match state.backends.acquire(&ctx.route) {
    Some(l) => l,
    None => return 503,
};
let backend_id = lease.backend_id();
state.backends.note_result(backend_id, ok);
state.ledger.try_record(event);
```

Ownership rule:

- `BackendLease` must live until response streaming ends.
- For non-streaming, holding it in `forward_backend_response` until body read completes is enough.
- For streaming, move it into the spawned stream task/reporter so inflight decrements after the stream completes or aborts.

This avoids the previous bug where route selection and inflight increment were separate operations.

### 6. Update `ProxyContext` without making guards cloneable

Current `ProxyContext` is `#[derive(Debug, Clone)]`. Do not put `BudgetReservation`, `ConcurrencyGuard`, or `BackendLease` directly inside a cloned context.

Use one of these shapes:

```rust
pub struct ProxyGuards {
    pub budget: BudgetReservation,
    pub concurrency: ConcurrencyGuard,
}

pub async fn proxy_forward(
    state: Arc<AppState>,
    req: Request<Bytes>,
    ctx: ProxyContext,
    guards: ProxyGuards,
) -> Response<Body>
```

or let `proxy_forward` acquire the budget/concurrency itself and keep `ProxyContext` as pure metadata. The first option is better because handler already has auth/model/estimate context.

### 7. Update `main.rs` runtime wiring

Concrete setup should look like:

```rust
let snapshot = boot_loader.load_snapshot().await?;

let budget = Arc::new(RamBudgetStore::new());
budget.load_teams(&snapshot.teams);

let backends = Arc::new(RamBackendPool::new());
for backend in snapshot.backends.values().cloned() {
    backends.upsert_backend(backend);
}

let client = reqwest::Client::builder()
    .connect_timeout(Duration::from_secs(2))
    .pool_idle_timeout(Duration::from_secs(90))
    .build()
    .context("build shared reqwest client")?;

let (ledger_tx, ledger_rx) = mpsc::channel(8192);
let ledger = LedgerSink::new(ledger_tx);

cfg.store(Arc::new(snapshot));

let app_state = AppState {
    cfg: Arc::clone(&cfg),
    budget: Arc::clone(&budget),
    backends: Arc::clone(&backends),
    client: client.clone(),
    ledger,
    metrics: Metrics::default(),
    max_body_bytes,
};
```

Also sync runtime state on config poll:

- After each successful `load_snapshot`, call `budget.load_teams(&snapshot.teams)`.
- Upsert all `snapshot.backends` into `backends`.
- Then `cfg.store(Arc::new(snapshot))`.

Do not only update `cfg`; otherwise route config changes are visible while `RamBackendPool` and team budgets remain stale.

## Hot path constraints during this patch

The patch is acceptable only if these remain true:

- No PostgreSQL/SQLite call in request path.
- No Redis in fastest profile.
- No filesystem/env lookup in request path.
- No `reqwest::Client::new()` or `Client::builder()` per request.
- No `resp.bytes().await` for streaming responses.
- No raw `mpsc::Sender<UsageEvent>` in handler/proxy.

## Acceptance commands

Run in this order:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

If compile becomes green, expect the next real failures to be ledger replay/reconnect and handler/proxy streaming semantics. Those should be fixed after this integration patch, not hidden by reverting the RAII work.
