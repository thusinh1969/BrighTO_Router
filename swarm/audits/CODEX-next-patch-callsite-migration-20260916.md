# CODEX next patch — migrate remaining call sites, no API rollback

Superseded note, 2026-09-16: `src/proxy/mod.rs` has since moved forward and now uses `ProxyContext`, `CompletionReporter`, shared `state.client`, `Body::from_stream`, `LedgerSink`, and RAII guard ownership. Use `CODEX-current-frontier-after-proxy-20260916.md` as the current frontier. The high-level direction below remains correct, but proxy-specific old-line advice is stale.

Time: 2026-09-16 18:xx ICT. Scope: current worktree. I did not edit `src/`.

## Decision

Do not restore old `Decision`, `BudgetStore`, `BackendPool`, `RouteDecision`, or `SkipReason`.

Reason: current `src/budget/mod.rs`, `src/route/mod.rs`, `src/ledger/mod.rs`, and `src/metrics.rs` already moved in the correct SOTA direction:

- `BudgetReservation` + rollback on Drop exists.
- `ConcurrencyGuard` exists.
- `BackendLease` + `RamBackendPool::acquire` exists.
- `LedgerSink::try_record` exists.
- public `Metrics` exists.

The next patch must migrate stale call sites in `main.rs`, `handlers.rs`, and `proxy.rs`.

## Current red command

```bash
cargo check --all-targets
```

Current first blocker group:

```text
src/handlers.rs:19: imports removed BudgetStore, Decision, RouteDecision, SkipReason
src/proxy/mod.rs:20: imports removed RouteDecision
src/handlers.rs:242: calls removed budget.try_reserve
src/handlers.rs:259: reads removed state.inner.pool
src/handlers.rs:311: calls removed budget.commit
src/handlers.rs:338: reads removed state.inner.ledger_tx
src/proxy/mod.rs:442/503/516: reads removed state.ledger_tx
src/proxy/mod.rs:554/625/646/661/677/694: reads removed state.pool
src/main.rs:14/74/75/77-82/85-86: still wires old trait-object AppState and old one-channel LedgerWriter
```

## Patch 1 — `main.rs` must construct the runtime state that `contract.rs` now declares

Current `AppState` fields in `src/contract.rs:123-130`:

```rust
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

Update `src/main.rs`:

1. Remove imports of removed traits:

```rust
use brigto_router::contract::{AppState, ConfigSnapshot};
use brigto_router::ledger::{LedgerSink, LedgerWriter};
use brigto_router::metrics::Metrics;
```

2. Store the boot snapshot in a variable before `cfg.store`.

3. Initialize `budget` and `backends` from that snapshot:

```rust
let snapshot = boot_loader.load_snapshot().await?;

let budget = Arc::new(RamBudgetStore::new());
budget.load_teams(&snapshot.teams);

let backends = Arc::new(RamBackendPool::new());
for backend in snapshot.backends.values().cloned() {
    backends.upsert_backend(backend);
}

cfg.store(Arc::new(snapshot));
```

4. Build one shared HTTP client:

```rust
let client = reqwest::Client::builder()
    .connect_timeout(Duration::from_secs(2))
    .pool_idle_timeout(Duration::from_secs(90))
    .build()
    .context("build shared reqwest client")?;
```

5. Build two ledger channels because current `LedgerSink::new` takes primary + overflow:

```rust
let (primary_tx, primary_rx) = mpsc::channel(8192);
let (overflow_tx, overflow_rx) = mpsc::channel(8192);
let ledger = LedgerSink::new(primary_tx, overflow_tx);
let writer = LedgerWriter::new(100, 1);
tokio::spawn(async move { writer.run(primary_rx, overflow_rx).await });
```

6. Build `AppState` with current field names:

```rust
let app_state = AppState {
    cfg: Arc::clone(&cfg),
    budget: Arc::clone(&budget),
    backends: Arc::clone(&backends),
    client,
    ledger,
    metrics: Metrics::install(),
    max_body_bytes,
};
```

7. `RouterState::new` should no longer need a separate `max_body_bytes` argument if `AppState` owns it. Either update `RouterState::new(Arc<AppState>)`, or keep the duplicate temporarily but remove the duplicate in the next cleanup. Prefer single source of truth in `AppState`.

## Patch 2 — `handlers.rs` must stop using the removed contract API

Current stale lines:

- `src/handlers.rs:18-20`
- `src/handlers.rs:242-257`
- `src/handlers.rs:259-270`
- `src/handlers.rs:311`
- `src/handlers.rs:338`

Required import change:

```rust
use crate::contract::{ApiKey, AppState, BudgetError, ModelRoute};
```

If handler still has local buffered forwarding during this patch, keep `Backend`, `BackendFormat`, and `UsageEvent` temporarily. Final SOTA patch should remove them from handler.

Required budget flow:

```rust
let reservation = match state.inner.budget.reserve(&key, &model, est_tokens) {
    Ok(r) => r,
    Err(BudgetError::RateLimited { retry_after }) => {
        state.metrics.inc_rate_limited();
        let mut resp = build_error(StatusCode::TOO_MANY_REQUESTS, "rate limited");
        resp.headers_mut().insert(
            header::RETRY_AFTER,
            header::HeaderValue::from_str(&retry_after.as_secs().to_string()).unwrap(),
        );
        return resp;
    }
    Err(BudgetError::BudgetExceeded { .. }) => {
        state.metrics.inc_budget_exceeded();
        return build_error(StatusCode::TOO_MANY_REQUESTS, "budget exceeded");
    }
};

let concurrency = match state.inner.budget.acquire_concurrency(&key) {
    Ok(g) => g,
    Err(()) => {
        state.metrics.inc_rate_limited();
        return build_error(StatusCode::TOO_MANY_REQUESTS, "concurrency limited");
    }
};
```

Correct SOTA next step:

- Build `ProxyContext`.
- Pass `reservation` and `concurrency` into `proxy_forward` through a `ProxyGuards` struct.
- Do not select backend in handler if proxy owns retries and backend leases.
- Do not keep `forward_to_backend` + `resp.bytes().await` for `/v1/chat/completions` streaming.

Temporary compile-only patch, if needed:

- Use `let lease = state.inner.backends.acquire(&route)` instead of `state.inner.pool.pick(&route)`.
- Use `reservation.commit(total_tokens)` instead of `state.inner.budget.commit(...)`.
- Use `state.inner.ledger.try_record(event)` instead of `state.inner.ledger_tx.try_send(event)`.

But this temporary patch still buffers streaming, so it is not final SOTA.

## Patch 3 — `proxy.rs` must own backend lease and use shared client

Current stale lines:

- `src/proxy/mod.rs:20`
- `src/proxy/mod.rs:269-280`
- `src/proxy/mod.rs:324`
- `src/proxy/mod.rs:375`
- `src/proxy/mod.rs:442`
- `src/proxy/mod.rs:503`
- `src/proxy/mod.rs:516`
- `src/proxy/mod.rs:542-545`
- `src/proxy/mod.rs:554-557`
- `src/proxy/mod.rs:625/646/661/677/694`

Required changes:

1. Remove `RouteDecision` import.
2. Import `LedgerSink`.
3. Change `UsageReporter`:

```rust
struct UsageReporter {
    ledger: LedgerSink,
    ...
}

fn new(ledger: LedgerSink, ...) -> Self { ... }

fn finish(...) {
    self.ledger.try_record(event);
}
```

4. Use the shared client:

```rust
let client = &state.client;
```

5. Acquire backend with lease:

```rust
let lease = match state.backends.acquire(&ctx.route) {
    Some(l) => l,
    None => return build_response(StatusCode::SERVICE_UNAVAILABLE, ..., Body::from("No backend available")),
};
let backend_id = lease.backend_id();
```

6. Replace every `state.pool.note_result(...)` with `state.backends.note_result(...)`.

7. Move the winning `BackendLease` into the response lifecycle:

- non-stream: keep it alive through `response.bytes().await`;
- stream: move it into the spawned stream task along with reporter/guards.

If the lease drops before `Body::from_stream` finishes, inflight metrics and max-inflight protection are wrong.

## Patch 4 — existing `LedgerSink` needs one semantic cleanup

Current `src/ledger/mod.rs:41-43`:

```rust
Err(mpsc::error::TrySendError::Closed(ev)) => {
    let _ = self.overflow.try_send(ev);
}
```

This still silently ignores overflow closed/full in the primary-closed case.

Required patch:

```rust
Err(mpsc::error::TrySendError::Closed(ev)) => {
    if self.overflow.try_send(ev).is_err() {
        metrics::counter!("router_ledger_dropped_total").increment(1);
        tracing::error!("ledger: primary closed and overflow unavailable, dropping usage event");
    }
}
```

No file write, no await, no blocking in `try_record`.

## Patch 5 — mechanical compile issue in `budget.rs`

Add `#[derive(Debug)]` to `RpmBucket`, or remove `#[derive(Debug)]` from `RamBudgetStore`.

Current compiler hint:

```text
error[E0277]: `RpmBucket` doesn't implement `std::fmt::Debug`
help: consider annotating `RpmBucket` with `#[derive(Debug)]`
```

Prefer:

```rust
#[derive(Debug)]
struct RpmBucket {
    tokens: AtomicU64,
    last_refill_ms: AtomicU64,
}
```

## Acceptance gate

Run:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Do not claim “fastest” or “production ready” after only compile green. After compile green, the next audit must verify:

- `handlers.rs` no longer uses `resp.bytes().await` for streaming,
- proxy uses shared `state.client`,
- budget reservation commits/refunds after actual usage,
- `ConcurrencyGuard`/`BackendLease` live until response completion or client abort,
- config poll syncs `RamBudgetStore` and `RamBackendPool`, not only `ArcSwap<ConfigSnapshot>`.
