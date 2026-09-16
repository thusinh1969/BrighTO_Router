# CODEX current compile frontier — integrate existing RAII pieces

## Superseded-state notice

This file remains broadly correct, but `CODEX-next-patch-callsite-migration-20260916.md` is now the tighter current checklist for the exact next patch. Use that file first.

Time: 2026-09-16 18:xx ICT. Scope: current worktree. I did not edit `src/`.

## Current gate

```bash
cargo check --all-targets
```

Current result: FAIL before clippy/test.

Important status changes since older audits:

- `migrations/0001_init.sql` is fixed and direct SQLite `executescript` passes.
- `src/budget/mod.rs` already has `BudgetReservation` and `ConcurrencyGuard`.
- `src/route/mod.rs` already has `BackendLease` and `RamBackendPool::acquire`.
- `src/ledger/mod.rs` already has `LedgerSink`.
- `src/metrics.rs` already has public `Metrics`.

Do not redo those pieces. The remaining work is integration.

## P0-1 — `handlers.rs` still uses old BudgetStore/Decision/RouteDecision API

Files/lines:

- `src/handlers.rs:18-20` imports removed symbols: `BudgetStore`, `Decision`, `RouteDecision`, `SkipReason`.
- `src/handlers.rs:242` calls removed `budget.try_reserve`.
- `src/handlers.rs:259` reads removed field `state.inner.pool`.
- `src/handlers.rs:311` calls removed `budget.commit`.
- `src/handlers.rs:338` reads removed field `state.inner.ledger_tx`.

Compiler proof:

```text
error[E0432]: unresolved imports `crate::contract::BudgetStore`, `crate::contract::Decision`, `crate::contract::RouteDecision`, `crate::contract::SkipReason`
  --> src/handlers.rs:19:47

error[E0599]: no method named `try_reserve` found for struct `Arc<RamBudgetStore>`
  --> src/handlers.rs:242:30

error[E0609]: no field `pool` on type `Arc<AppState>`
  --> src/handlers.rs:259:40

error[E0599]: no method named `commit` found for struct `Arc<RamBudgetStore>`
  --> src/handlers.rs:311:24

error[E0609]: no field `ledger_tx` on type `Arc<AppState>`
  --> src/handlers.rs:338:25
```

Required patch:

1. Import only current types:

```rust
use crate::contract::{ApiKey, AppState, BackendFormat, BudgetError, ModelRoute};
```

Keep `Backend`/`UsageEvent` only if the local buffered path still exists during the same patch. Final SOTA path should remove them from handler.

2. Replace budget decision flow:

```rust
let reservation = match state.inner.budget.reserve(&key, &model, est_tokens) {
    Ok(r) => r,
    Err(BudgetError::RateLimited { retry_after }) => { /* 429 + Retry-After */ }
    Err(BudgetError::BudgetExceeded { .. }) => { /* 429 */ }
};

let concurrency = match state.inner.budget.acquire_concurrency(&key) {
    Ok(g) => g,
    Err(()) => { /* 429 concurrency limited */ }
};
```

3. Stop using `state.inner.pool.pick`. Use `state.inner.backends.acquire(&route)` if handler still chooses backend locally, or better move backend acquire into `proxy_forward` and pass guards.

4. Stop calling `state.inner.budget.commit(&key, ...)`. The current API commits through `BudgetReservation::commit(actual_tokens)`.

5. Stop calling raw `state.inner.ledger_tx.try_send`. Use `state.inner.ledger.try_record(event)` if handler still writes an event. Final SOTA path should let proxy/reporter write the event because proxy sees final stream usage.

SOTA ownership rule:

- `BudgetReservation`, `ConcurrencyGuard`, and `BackendLease` must live until upstream body is fully consumed or client aborts.
- For streaming, move those guards into the spawned stream task/reporter. If handler creates guards and drops them immediately after returning `Response<Body>`, the budget/concurrency/inflight protections are false.

## P0-2 — `proxy.rs` still uses old RouteDecision/pool/ledger_tx API

Files/lines:

- `src/proxy/mod.rs:20` imports removed `RouteDecision`.
- `src/proxy/mod.rs:442`, `503`, `516` call `UsageReporter::new(state.ledger_tx.clone(), ...)`.
- `src/proxy/mod.rs:554` calls removed `state.pool.pick`.
- `src/proxy/mod.rs:625`, `646`, `661`, `677`, `694` call removed `state.pool.note_result`.
- `src/proxy/mod.rs:542-545` still creates per-request `Client::builder()`.

Compiler proof:

```text
error[E0432]: unresolved import `crate::contract::RouteDecision`
  --> src/proxy/mod.rs:20:59

error[E0609]: no field `ledger_tx` on type `Arc<AppState>`
  --> src/proxy/mod.rs:442:53

error[E0609]: no field `pool` on type `Arc<AppState>`
  --> src/proxy/mod.rs:554:38
```

Required patch:

1. Remove `RouteDecision` import.
2. Change `UsageReporter` to hold `LedgerSink`, not `tokio_mpsc::Sender<UsageEvent>`:

```rust
struct UsageReporter {
    ledger: LedgerSink,
    ...
}

fn finish(...) {
    self.ledger.try_record(event);
}
```

3. Replace local per-request client:

```rust
let client = &state.client;
```

4. Replace route selection:

```rust
let lease = match state.backends.acquire(&ctx.route) {
    Some(l) => l,
    None => return 503,
};
let backend_id = lease.backend_id();
```

5. Replace `state.pool.note_result(...)` with `state.backends.note_result(...)`.

6. For retry, do not acquire a backend and then drop the `BackendLease` before the response finishes. Each attempt needs its own lease. The lease for the winning response must be held until:
   - non-streaming body read completes, or
   - streaming task completes/aborts.

7. Delete the old `RouteDecision::Skip { backend_id, .. } => backend_id` behavior. `Skip` no longer exists in the new `route.rs`, and forwarding to a skipped backend was wrong anyway.

## P0-3 — `main.rs` still wires old AppState and old LedgerWriter signature

Files/lines:

- `src/main.rs:14` imports removed `BackendPool`, `BudgetStore`.
- `src/main.rs:72` creates only one ledger channel.
- `src/main.rs:74-75` casts stores to removed trait objects.
- `src/main.rs:77-82` builds old `AppState` fields `pool` and `ledger_tx`.
- `src/main.rs:85-86` calls `writer.run(ledger_rx)`, but current `LedgerWriter::run` takes primary and overflow receivers.

Required patch:

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

let (primary_tx, primary_rx) = mpsc::channel(8192);
let (overflow_tx, overflow_rx) = mpsc::channel(8192);
let ledger = LedgerSink::new(primary_tx, overflow_tx);

let metrics = Metrics::install();

cfg.store(Arc::new(snapshot));

let app_state = AppState {
    cfg: Arc::clone(&cfg),
    budget: Arc::clone(&budget),
    backends: Arc::clone(&backends),
    client: client.clone(),
    ledger,
    metrics,
    max_body_bytes,
};

let writer = LedgerWriter::new(100, 1);
tokio::spawn(async move { writer.run(primary_rx, overflow_rx).await });
```

Also update config polling so runtime stores do not go stale:

- on each successful snapshot, call `budget.load_teams(&snapshot.teams)`,
- upsert each `snapshot.backends` into `backends`,
- then `cfg.store(Arc::new(snapshot))`.

Current `DbConfigLoader::run(cfg)` only updates `cfg`. Add a wrapper loop in `main.rs` or extend loader with a callback. Keep it simple; do not add Redis/Postgres hot path.

## P0-4 — `contract.rs` still owns concrete runtime `AppState`

Current status:

- `src/contract.rs:15-18` imports concrete implementations from `budget`, `ledger`, `metrics`, and `route`.
- `src/contract.rs:123-130` defines concrete `AppState`.

This may compile after call-site fixes, but it is the wrong module boundary. Keep `contract.rs` as shared data. Move `AppState` to `src/state.rs` unless this would delay compile recovery. If kept temporarily, create a follow-up patch immediately after compile green.

Patch shape:

```rust
// src/lib.rs
pub mod state;
```

```rust
// src/state.rs
pub struct AppState { ... concrete runtime fields ... }
```

Then update imports in `handlers.rs`, `proxy.rs`, `main.rs`.

## P0-5 — Mechanical compile error in `budget.rs`

File/line:

- `src/budget/mod.rs:138-140`

Compiler proof:

```text
error[E0277]: `RpmBucket` doesn't implement `std::fmt::Debug`
    --> src/budget/mod.rs:140:5
help: consider annotating `RpmBucket` with `#[derive(Debug)]`
```

Required patch:

Either add `#[derive(Debug)]` to `RpmBucket`, or remove `#[derive(Debug)]` from `RamBudgetStore`, `BudgetReservation`, and `ConcurrencyGuard`. Prefer deriving `Debug` on `RpmBucket`; it is mechanical and useful for tests.

## P1 — Handler still blocks SOTA streaming if left in place

Even after compile is fixed, do not leave this in the final hot path:

- `src/handlers.rs:212` full-parses request body as `serde_json::Value`.
- `src/handlers.rs:277` reads backend key from env per request.
- `src/handlers.rs:282` calls local `forward_to_backend`.
- `src/handlers.rs:288` buffers upstream with `resp.bytes().await`.
- `src/handlers.rs:346` returns `Body::from(upstream_body)`.
- `src/handlers.rs:446` creates `reqwest::Client::new()` per request.

The correct SOTA cut is: handler does auth + tiny model/stream extraction + budget/concurrency guards, then calls streaming proxy with shared client and moves guards into the response lifecycle.

## Acceptance commands

Run exactly:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Do not claim architecture is SOTA until these are also true by code inspection or tests:

- no per-request client construction,
- streaming response uses `Body::from_stream`,
- budget reservation is committed/refunded after actual usage,
- concurrency/backend leases live until response completion/abort,
- no DB/Redis/filesystem/env lookup in request path.
