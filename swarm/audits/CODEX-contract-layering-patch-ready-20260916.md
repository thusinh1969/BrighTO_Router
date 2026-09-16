# CODEX patch-ready verdict — fix contract layering first

## Superseded-state notice

This file was correct when `contract.rs` was the only visible partial refactor. Current worktree has since moved further: `src/budget/mod.rs` now has `BudgetReservation`/`ConcurrencyGuard`, and `src/route/mod.rs` now has `BackendLease`/`RamBackendPool::acquire`. Do not use the “Path A restore old contract” option from older audits unless intentionally rolling back those working RAII changes.

Use `CODEX-refactor-integration-plan-20260916.md` as the current patch plan: finish the RAII integration cut, move concrete `AppState` out of `contract.rs`, add `LedgerSink`/public `Metrics`, and update `main.rs`, `handlers.rs`, and `proxy.rs` together.

Time: 2026-09-16 18:xx ICT. Scope: current worktree. I did not edit `src/`.

## Verdict

The current red build is caused by a partial SOTA refactor in `src/contract.rs`. The intent is good: concrete runtime state, shared client, ledger sink, unified metrics. The implementation location is wrong.

`contract.rs` should stay the low-level shared type/trait boundary. It must not import concrete implementations from `budget`, `route`, `ledger`, or `metrics`, because those modules already import `contract`. Keep the boundary acyclic; put concrete runtime wiring in `main.rs`, `handlers.rs`, or a small `state.rs` later.

Current proof:

```bash
cargo check --all-targets
```

Current result:

```text
error[E0432]: unresolved import `crate::ledger::LedgerSink`
  --> src/contract.rs:16:5
error[E0432]: unresolved import `crate::metrics::Metrics`
  --> src/contract.rs:17:5
error[E0432]: unresolved imports `crate::contract::BudgetStore`, `crate::contract::Decision`
  --> src/budget/mod.rs:10:39
error[E0432]: unresolved imports `crate::contract::BudgetStore`, `crate::contract::Decision`, `crate::contract::RouteDecision`, `crate::contract::SkipReason`
  --> src/handlers.rs:19:47
error[E0432]: unresolved import `crate::contract::RouteDecision`
  --> src/proxy/mod.rs:20:59
error[E0432]: unresolved imports `crate::contract::BackendPool`, `crate::contract::RouteDecision`
  --> src/route/mod.rs:13:32
```

## Required next patch

Do this as the next patch. Do not benchmark or continue handler/proxy refactor while compile is red.

### 1. Restore `contract.rs` as the shared boundary

Remove these imports from `src/contract.rs`:

```rust
use crate::budget::RamBudgetStore;
use crate::ledger::LedgerSink;
use crate::metrics::Metrics;
use crate::route::RamBackendPool;
```

Add this import back:

```rust
use tokio::sync::mpsc;
```

Keep `use std::sync::Arc;` if `AppState.cfg` remains `Arc<ArcSwap<ConfigSnapshot>>`.

Replace the current `BudgetError` block with the old `Decision` API for now:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    RateLimited { retry_after: Duration },
    BudgetExceeded { remaining: u64 },
}
```

Reinsert `RouteDecision`, `SkipReason`, `BudgetStore`, and `BackendPool` exactly where the old code had them:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    Pick(i64),
    Skip { backend_id: i64, reason: SkipReason },
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    CircuitOpen,
    AtMaxInflight,
    Disabled,
}

pub trait BudgetStore: Send + Sync + 'static {
    fn try_reserve(&self, key: &ApiKey, model: &str, est_tokens: u64) -> Decision;
    fn commit(&self, key: &ApiKey, model: &str, tokens: u64);
}

pub trait BackendPool: Send + Sync + 'static {
    fn pick(&self, route: &ModelRoute) -> RouteDecision;
    fn note_result(&self, backend_id: i64, ok: bool);
    fn inflight(&self, backend_id: i64) -> u32;
}
```

Make `AppState` compatible with current `main.rs`, `handlers.rs`, and `proxy.rs`:

```rust
pub struct AppState {
    pub cfg: Arc<ArcSwap<ConfigSnapshot>>,
    pub budget: Arc<dyn BudgetStore>,
    pub pool: Arc<dyn BackendPool>,
    pub ledger_tx: mpsc::Sender<UsageEvent>,
}
```

Do not keep `BudgetError` in `contract.rs` unless the entire budget module is also migrated in the same patch. A half-migrated API is what broke the repo.

### 2. Then run this exact gate

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Expected state after this patch: compile/clippy should return to green, then the next failures will expose the real runtime bugs again.

## Next architecture patch after build recovery

After the gate is green or at least back to test-only failures, continue with the SOTA architecture refactor in small complete patches:

1. Add `LedgerSink` in `src/ledger/mod.rs`.
   - Add it as an implementation detail behind `AppState` only when handler/proxy are migrated in the same patch.
   - It wraps `mpsc::Sender<UsageEvent>` and exposes `try_record`.
   - It must count `queued` and `dropped`.
   - It must not write fallback files in the request path.
2. Move public `Metrics` into `src/metrics.rs`.
   - Delete private `handlers::Metrics`.
   - Pick one namespace: use the locked `router_*` names unless dashboards already use `brigto_router_*`.
3. Replace check-then-commit budget only when all call sites migrate together.
   - New API should be reservation based.
   - `reserve` increments estimated usage/concurrency before upstream.
   - `commit` reconciles actual usage.
   - `refund/drop` releases on upstream error, timeout, or client abort.
4. Replace trait-object `AppState` with concrete state only when every call site is updated in the same patch.
   - Concrete state is fine for fastest path.
   - The state type should live outside `contract.rs` if it imports concrete modules.

## Architecture line to keep

For fastest production profile:

- PostgreSQL is allowed for config/admin/ledger in background paths only.
- Redis is not allowed in the fastest single-router profile.
- Redis is only justified later for strict multi-instance global quota/concurrency.
- No DB, Redis, filesystem, env lookup, full JSON parse, or per-request `reqwest::Client` in the request forwarding path.

## Why this is the right cut

This is not over-engineering. It is the smallest repair that restores a coherent module graph:

```text
contract types/traits
  ↑ used by
budget / route / ledger / config / handlers / proxy

main wires concrete implementations
```

The alternative graph in the current worktree is cyclic and incomplete:

```text
contract imports budget/route/ledger/metrics
budget/route/handlers/proxy import contract
```

That graph makes small refactors fragile and blocks the gate before tests can evaluate ledger, streaming, budget, or routing correctness.
