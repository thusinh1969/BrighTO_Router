# CODEX current breakage — contract refactor stopped halfway

Time: 2026-09-16 18:xx ICT. Scope: current worktree after migration was fixed. I did not edit `src/`.

## Gate verdict

`cargo fmt --all -- --check`: PASS.

`cargo check --all-targets`: FAIL before clippy/test.

Relevant output:

```text
error[E0432]: unresolved import `crate::ledger::LedgerSink`
  --> src/contract.rs:16:5
   |
16 | use crate::ledger::LedgerSink;
   |                    no `LedgerSink` in `ledger`

error[E0432]: unresolved import `crate::metrics::Metrics`
  --> src/contract.rs:17:5
   |
17 | use crate::metrics::Metrics;
   |                     no `Metrics` in `metrics`

error[E0432]: unresolved imports `crate::contract::BudgetStore`, `crate::contract::Decision`
  --> src/budget/mod.rs:10:39

error[E0432]: unresolved imports `crate::contract::BudgetStore`, `crate::contract::Decision`,
`crate::contract::RouteDecision`, `crate::contract::SkipReason`
  --> src/handlers.rs:19:47

error[E0432]: unresolved import `crate::contract::RouteDecision`
  --> src/proxy/mod.rs:20:59

error[E0432]: unresolved imports `crate::contract::BackendPool`, `crate::contract::RouteDecision`
  --> src/route/mod.rs:13:32
```

## What changed correctly

`migrations/0001_init.sql` is no longer the blocker:

- no Markdown SQL fences,
- `format` now uses `openai`/`anthropic`,
- route column is `first_byte_timeout`,
- `key_hash` is hex `TEXT`,
- seed hash for `lc-dev0001` is `6f48c439d85e6ec2cdf3cb47bfe0b0853e8278724b56140a9edb7d3e31a78819`,
- direct SQLite `executescript` passes.

Keep this migration direction.

## P0 — `src/contract.rs` now declares a new architecture but dependent modules still use the old contract

Current `src/contract.rs`:

- `src/contract.rs:15` imports `crate::budget::RamBudgetStore`.
- `src/contract.rs:16` imports `crate::ledger::LedgerSink`, but no such type exists in `src/ledger/mod.rs`.
- `src/contract.rs:17` imports `crate::metrics::Metrics`, but `src/metrics.rs` only exports `METRICS`; the actual `Metrics` struct is private in `src/handlers.rs:43`.
- `src/contract.rs:92-97` defines `BudgetError`, but removes old `Decision`.
- `src/contract.rs:121-130` changes `AppState` to concrete fields: `budget`, `backends`, `client`, `ledger`, `metrics`, `max_body_bytes`.
- Old exported symbols `BudgetStore`, `BackendPool`, `RouteDecision`, `SkipReason`, and `Decision` are gone.

Dependent modules still expect old symbols:

- `src/budget/mod.rs:10` imports `BudgetStore` and `Decision`.
- `src/route/mod.rs:13` imports `BackendPool` and `RouteDecision`.
- `src/handlers.rs:19-20` imports `BudgetStore`, `Decision`, `RouteDecision`, `SkipReason`.
- `src/main.rs:14` imports `BackendPool` and `BudgetStore`.
- `src/proxy/mod.rs:20` imports `RouteDecision`.

This is an integration break, not a lint problem.

## Required repair decision

Pick exactly one path. Do not mix the two.

### Path A — fastest compile recovery

Restore the old contract exports temporarily:

- put back `Decision`,
- put back `RouteDecision`,
- put back `SkipReason`,
- put back `BudgetStore`,
- put back `BackendPool`,
- keep `AppState` fields compatible with existing `main.rs`, `handlers.rs`, `proxy.rs`, `budget.rs`, `route.rs`,
- defer `LedgerSink`, shared client, and concrete-state refactor to the next patch.

Then run:

```bash
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

This path gets the repo back to the previous red state quickly: compile/clippy green, one ledger test failing.

### Path B — correct SOTA architecture refactor

Finish the concrete-state refactor everywhere in the same patch:

1. Add `pub struct LedgerSink` in `src/ledger/mod.rs`.
   - Wrap `tokio::sync::mpsc::Sender<UsageEvent>`.
   - Expose `try_record(&self, ev: UsageEvent)`.
   - Track at least queued and dropped counters.
   - Do not write files in the request path.
2. Add a real `pub struct Metrics` in `src/metrics.rs`, or move the private `handlers::Metrics` there and make it public.
   - Unify metric names. Do not keep both `brigto_router_*` and `router_*` contracts.
3. Update `src/budget/mod.rs`.
   - If `BudgetStore` is removed, remove `impl BudgetStore for RamBudgetStore`.
   - Expose inherent methods on `RamBudgetStore`.
   - Prefer the SOTA-safe API now: `reserve(...) -> Result<BudgetReservation, BudgetError>`, `commit(reservation, actual_tokens)`, `refund(reservation)`.
4. Update `src/route/mod.rs`.
   - If `BackendPool` is removed, remove `impl BackendPool for RamBackendPool`.
   - Move `RouteDecision`/`SkipReason` into `route` or keep them in `contract`; choose one public location and update all imports.
   - A skip decision must never be forwarded to the skipped backend.
5. Update `src/main.rs`.
   - Build `RamBackendPool` from the boot `ConfigSnapshot`.
   - Store a single shared `reqwest::Client` in `AppState`.
   - Store `LedgerSink`, not raw `mpsc::Sender`.
   - Store `max_body_bytes` in `AppState` if `RouterState` no longer owns it.
6. Update `src/handlers.rs`.
   - Remove private `Metrics` if metrics moved to `src/metrics.rs`.
   - Replace `state.inner.pool` with `state.inner.backends`.
   - Replace raw `ledger_tx.try_send` with `state.inner.ledger.try_record`.
   - Stop creating `reqwest::Client::new()` per request.
   - Stop buffering streaming upstream with `resp.bytes().await`; call the streaming proxy path.
7. Update `src/proxy/mod.rs`.
   - Use `state.client.clone()` or `&state.client`, not `Client::builder()` per request.
   - Replace `state.pool` with `state.backends`.
   - Replace raw ledger sender with `LedgerSink`.

Then run the same full gate:

```bash
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Path B is preferred for architecture if it can be completed in one coherent patch. Path A is acceptable only as a temporary recovery step before doing Path B immediately after.

## Non-negotiable SOTA constraints while fixing

- No PostgreSQL/SQLite call in the request forwarding path.
- No Redis in fastest profile.
- No filesystem/env secret resolution per request.
- No per-request `reqwest::Client`.
- No `resp.bytes().await` for streaming.
- No raw `mpsc::Sender<UsageEvent>` exposed to handler/proxy.
- No check-then-commit budget API under concurrent requests.

## Current next command after repair

The next proof command must start with:

```bash
cargo check --all-targets
```

Do not spend time on benchmark claims until compile, clippy, and tests are green again.
