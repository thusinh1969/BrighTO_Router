# CODEX post compile-green SOTA audit — 2026-09-16

Superseded note: this file's fmt/clippy failure status is stale. DeepSeek later made check/fmt/clippy/test all pass. Use `CODEX-gate-green-architecture-blockers-20260916.md` as the current source of truth.

Scope: current worktree after DeepSeek fixed handler/main/route integration enough for `cargo check`. I did not edit `src/`.

## Current gate status

Commands run:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Results:

- `cargo check --all-targets`: PASS.
- `cargo test --all-targets`: PASS, 33 tests.
- `cargo fmt --all -- --check`: FAIL, formatting only.
- `cargo clippy --all-targets -- -D warnings`: FAIL.

Do not declare SOTA/production-ready yet. The repo is compile-green and unit-test-green, but not gate-green and not architecture-green.

## P0 — Fix gate red without changing architecture

### P0-1 — `cargo fmt --check` fails

Evidence: `cargo fmt --all -- --check` prints diffs in:

- `src/admin/mod.rs`
- `src/config/mod.rs`
- `src/handlers.rs`
- `src/ledger/mod.rs`
- `src/metrics.rs`
- `src/proxy/mod.rs`
- `src/route/mod.rs`
- `src/main.rs`

Fix once:

```bash
cargo fmt --all
```

Do not manually fight rustfmt.

### P0-2 — Clippy dead/unused items

Evidence from `cargo clippy --all-targets -- -D warnings`:

```text
src/config/mod.rs:304 unused import: sqlx::any::AnyPoolOptions
src/config/mod.rs:271 function check_api_key_ref is never used
src/proxy/mod.rs:272 field lease is never read
src/route/mod.rs:330 method inc_inflight is never used
```

Required fixes:

1. In `src/config/mod.rs` test module, remove `use sqlx::any::AnyPoolOptions;` because the code calls `sqlx::any::AnyPoolOptions::new()` fully qualified.
2. Mark `check_api_key_ref` as test-only because it is only used by tests:

   ```rust
   #[cfg(test)]
   fn check_api_key_ref(ref_name: &str) -> Result<()> { ... }
   ```

3. In `src/proxy/mod.rs`, do not remove the `lease` field. It is the RAII guard that keeps backend inflight accounting correct until response completion/drop. Rename the field to `_lease`:

   ```rust
   _lease: BackendLease,
   ```

   and set `_lease: lease` in `CompletionReporter::new`.

4. In `src/route/mod.rs`, make the test helper method test-only:

   ```rust
   #[cfg(test)]
   fn inc_inflight(&self, backend_id: i64) { ... }
   ```

### P0-3 — Clippy budget API warnings

Evidence:

```text
src/budget/mod.rs:200 collapsible_if
src/budget/mod.rs:216 collapsible_if
src/budget/mod.rs:232 result_unit_err
src/budget/mod.rs:308 too_many_arguments
```

Required fixes:

1. Collapse the nested `if` blocks in `reserve`.
2. Replace `Result<ConcurrencyGuard, ()>` with a typed error. Minimal maintainable shape:

   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub struct ConcurrencyLimitExceeded;

   pub fn acquire_concurrency(
       self: &Arc<Self>,
       key: &ApiKey,
   ) -> Result<ConcurrencyGuard, ConcurrencyLimitExceeded> {
       ...
       return Err(ConcurrencyLimitExceeded);
   }
   ```

   Existing handler can keep `Err(_) => 429`.

3. Replace `reserve_scope`'s argument list with a tiny scope spec instead of suppressing clippy:

   ```rust
   struct ScopeReservation<'a> {
       scope: u8,
       id: i64,
       model: &'a str,
       period_start: i64,
       limit: u64,
       amount: u64,
   }

   fn reserve_scope(
       &self,
       spec: ScopeReservation<'_>,
       scopes: &mut Vec<ReservedScope>,
   ) -> Result<(), u64> { ... }
   ```

   This is not over-engineering; it names the atomic reservation tuple and removes a fragile 8-argument internal call.

### P0-4 — Clippy test bool assert

Evidence:

```text
src/ledger/mod.rs:500 assert_eq!(orx.recv().await.is_some(), true)
src/ledger/mod.rs:501 assert_eq!(orx.recv().await.is_some(), true)
```

Fix:

```rust
assert!(orx.recv().await.is_some());
assert!(orx.recv().await.is_some());
```

Acceptance after P0 gate fixes:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## P0 architecture — Streaming currently loses backpressure

Evidence:

- `src/proxy/mod.rs:459` uses `mpsc::unbounded::<Result<Bytes, reqwest::Error>>()`.
- `src/proxy/mod.rs:463` spawns a task that reads `response.bytes_stream()`.
- `src/proxy/mod.rs:473` calls `tx.unbounded_send(Ok(bytes))`.
- `src/proxy/mod.rs:505` returns `Body::from_stream(rx)`.

Why this blocks SOTA/production:

The backend reader is decoupled from the client writer by an unbounded in-memory queue. If the backend streams faster than the client reads, the router keeps pulling chunks from backend and can buffer without limit. Under slow clients or network hiccups this turns streaming into unbounded RAM growth. It also hides backpressure from vLLM/llama-server, which is the opposite of a fastest stable router under load.

Fix once, minimal acceptable:

```rust
use futures::{SinkExt, StreamExt};

let (mut tx, rx) = mpsc::channel::<Result<Bytes, reqwest::Error>>(1);
...
if tx.send(Ok(bytes)).await.is_err() {
    client_aborted = true;
    break;
}
```

This preserves the current spawned reporter design but makes the backend reader wait for the response body consumer. Capacity `1` is enough because this is a stream, not a queue.

Better SOTA shape, still no new dependency:

Use `futures::stream::unfold` over `response.bytes_stream()` and store `(stream, acc, reporter)` inside the returned body stream. Then no per-request relay task and no channel are needed. `CompletionReporter::Drop` already handles client abort when the body stream is dropped before EOF.

Acceptance:

```bash
rg -n "mpsc::unbounded|unbounded_send" src/proxy/mod.rs
```

Expected: no hits.

## P0 architecture — `fallback_backend_id` is loaded but never used

Evidence:

- `src/contract.rs:64` defines `ModelRoute::fallback_backend_id`.
- `src/config/mod.rs:113-129` loads `fallback_backend_id`.
- `src/route/mod.rs:113-155` implements `acquire_excluding`, but line 121 exits with `?` when `choose_candidate(route, excluded)` returns `None`.
- `src/route/mod.rs:158-240` `choose_candidate` iterates only `route.backend_ids`.

Impact:

Declared fallback is dead config. If all primary backends fail or are excluded, `proxy_forward` returns 503/502/504 instead of trying the explicitly configured fallback. This contradicts plan §3 retry/failover and makes production behavior surprising.

Fix once:

1. Keep primary selection exactly as-is for `route.backend_ids`.
2. After primary candidates are exhausted, try `fallback_backend_id` only if it is `Some(id)` and not already excluded.
3. Do not mix fallback into the normal least-load primary candidate set, or an idle paid fallback can beat healthy primary GPUs.
4. Add tests:

   - `acquire_excluding_skips_tried_backend`
   - `fallback_used_after_primary_exhausted`
   - `fallback_not_used_while_primary_available`

Concrete route shape:

```rust
pub fn acquire_excluding(
    self: &Arc<Self>,
    route: &ModelRoute,
    initial_excluded: &HashSet<i64>,
) -> Option<BackendLease> {
    let mut excluded = initial_excluded.clone();
    if let Some(lease) = self.acquire_primary(route, &mut excluded) {
        return Some(lease);
    }
    let fallback = route.fallback_backend_id?;
    if excluded.contains(&fallback) {
        return None;
    }
    let fallback_route = ModelRoute {
        model_name: route.model_name.clone(),
        backend_ids: vec![fallback],
        fallback_backend_id: None,
        chars_per_token: route.chars_per_token,
        first_byte_timeout: route.first_byte_timeout,
    };
    let mut fallback_excluded = HashSet::new();
    self.acquire_primary(&fallback_route, &mut fallback_excluded)
}
```

`acquire_primary` should contain the current loop body from `acquire_excluding`.

## P0 production correctness — budget counters reset on restart

Evidence:

- `src/config/mod.rs:52-67` reads `usage_ledger` only to log total row/input/output counts.
- `src/ledger/mod.rs:296-310` has `boot_counter(pool) -> HashMap<i64, u64>`, but it aggregates all-time only by `key_id`.
- `src/main.rs:77` creates a new empty `RamBudgetStore`.
- `src/main.rs:82` calls `wire_snapshot`, and `src/main.rs:165-169` only calls `budget.load_teams(&snap.teams)`.
- `src/budget/mod.rs:141-143` stores usage counters in RAM only; there is no seed API for current-period ledger totals.

Impact:

After router restart, day/month budget usage starts from zero even if `usage_ledger` already contains usage for the current period. A team can exceed budget by restarting the process. This is not a hot-path problem, but it is a production correctness blocker.

Fix once:

1. Add a background/startup-only seed API to `RamBudgetStore`, not a request-path DB call:

   ```rust
   pub struct UsageSeed {
       pub scope: u8,
       pub id: i64,
       pub model: String,
       pub period_start: i64,
       pub used_tokens: u64,
   }

   pub fn load_usage_seeds(&self, seeds: impl IntoIterator<Item = UsageSeed>) {
       self.usage.clear();
       for seed in seeds {
           self.usage.insert(
               UsageKey {
                   scope: seed.scope,
                   id: seed.id,
                   model: seed.model,
                   period_start: seed.period_start,
               },
               AtomicU64::new(seed.used_tokens),
           );
       }
   }
   ```

   If keeping `scope` private is preferred, expose constructors like `UsageSeed::key_total(...)` instead.

2. Replace or extend `ledger::boot_counter`. The current `HashMap<i64, u64>` is insufficient because budgets exist for key total, key+model, team total, and team+model, and periods can be day/month. Load current-period aggregates for every effective budget in the current snapshot.
3. Call the seed path during boot before listen. On config reload, reseed only if budget definitions changed, or accept a short background reseed. Do not block requests and do not query DB from handlers/proxy.

Acceptance:

- Add a test: insert usage ledger rows for the current period, boot/wire budget, then verify the next request is rejected when existing ledger usage already reaches the budget.
- `rg -n "usage_ledger" src/handlers.rs src/proxy/mod.rs src/budget/mod.rs` must show no DB query in request handling.

## P1 — Runtime snapshot sync does not prune deleted config

Evidence:

- `src/main.rs:165-169` `wire_snapshot` only upserts backends and loads team budgets.
- `src/route/mod.rs:86-104` `upsert_backend` never removes a backend missing from the new snapshot.
- `src/budget/mod.rs:158-168` `load_teams` removes budget only for teams that still exist with `budget=None`; it does not remove teams deleted from the snapshot.

Impact:

Deleted/renamed config can leave stale RAM state. Route selection usually still checks `snapshot.routes`, but stale backend states can cause extra 503s and misleading health/metrics. Deleted team budgets can keep applying after reload.

Fix once:

- Replace `load_teams` with `sync_teams`: retain only team ids present in the snapshot, then upsert/remove budgets for present teams.
- Replace `upsert_backend` calls in `wire_snapshot` with `sync_backends`: retain or disable only backend ids present in snapshot, then upsert current rows.
- Keep this in RAM and in background reload only.

## P1 — Redis/Valkey still violates the default fastest profile

Evidence:

- `Cargo.toml:34` depends on `redis`.
- `docker-compose.yml:9` sets `REDIS_URL`.
- `docker-compose.yml:17` makes router depend on `valkey`.
- `docker-compose.yml:35-40` starts `valkey`.
- `rg -n "redis::|REDIS|valkey" src` shows no runtime Redis use.

Verdict:

Redis is still not justified for default production. PostgreSQL is justified for config/admin/ledger background paths. Redis is only justified for hard global quotas/concurrency across multiple router instances. Keep that as an optional feature/ADR, not as a default dependency and default service.

Fix once:

- Remove default `redis` dependency or make it optional behind `hard-quotas-redis`.
- Remove default `REDIS_URL`, `depends_on: valkey`, and `valkey` service from default compose.

## Current positive findings

- Handler is now thin glue and calls `proxy::proxy_forward` (`src/handlers.rs:151-162`).
- Handler no longer uses old `BudgetStore`/`Decision`/`RouteDecision` symbols.
- Handler no longer reads backend keys from env and no longer constructs a per-request `reqwest::Client`.
- `cargo check --all-targets` passes.
- `cargo test --all-targets` passes 33 tests.

## Required next acceptance

After fixes, run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
rg -n "mpsc::unbounded|unbounded_send|reqwest::Client::new\\(|std::env::var\\(&backend\\.api_key_ref\\)|resp\\.bytes\\(\\)\\.await|Body::from\\(upstream_body\\)|ledger_tx|state\\.inner\\.pool|BudgetStore|BackendPool|RouteDecision|SkipReason|Decision" src
```

Expected:

- fmt/clippy/test all pass;
- no unbounded streaming relay;
- no per-request client/env/backend-key lookup;
- no old contract API;
- no buffered streaming handler path.
