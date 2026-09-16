# CODEX gate-green architecture blockers — 2026-09-16

Scope: current worktree after DeepSeek's latest fixes. I did not edit `src/`.

## Current verified gate status

Commands run on current worktree:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Result:

- `cargo check --all-targets`: PASS.
- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --all-targets -- -D warnings`: PASS.
- `cargo test --all-targets`: PASS, 33 tests.

This is a real milestone: compile/fmt/clippy/unit tests are green. It is still not SOTA/production-ready because the architecture blockers below remain.

## P0 — Proxy commits zero tokens when backend does not return usage

Evidence:

- `src/proxy/mod.rs:503-511` finishes stream requests with `acc.input_tokens` and `acc.output_tokens`.
- `src/proxy/mod.rs:503` sets `estimated = !acc.seen_usage`, but if no usage was seen both token fields remain `0`.
- `src/proxy/mod.rs:523-531` does the same for non-stream responses: `parse_usage_from_body` returns default zero counters if no usage exists.
- `src/proxy/mod.rs:322` commits the budget reservation with `input_tokens + output_tokens`.

Impact:

If a backend omits `usage`, the router records `estimated=true` but `0` tokens and commits `0` tokens. Because `BudgetReservation::commit(0)` refunds the reserved estimate, that request becomes effectively free. This violates the plan requirement: “Không bao giờ ghi 0 rồi im.”

Fix once:

1. Add estimate metadata to `ProxyContext`:

   ```rust
   pub estimated_input_tokens: u64,
   pub chars_per_token: f64,
   ```

   Handler already computes `est_tokens` at `src/handlers.rs:128`; pass it into the context.

2. Track output bytes in the proxy accumulator:

   ```rust
   #[derive(Debug, Clone, Default)]
   pub struct UsageAccumulator {
       pub input_tokens: u64,
       pub output_tokens: u64,
       pub seen_usage: bool,
       pub output_bytes: u64,
   }
   ```

   Increment `output_bytes` for every streamed chunk before forwarding and for non-stream body bytes.

3. In `CompletionReporter::finish`, if `estimated == true`, replace zeros before commit/ledger:

   ```rust
   let (input_tokens, output_tokens) = if estimated {
       (
           input_tokens.max(self.estimated_input_tokens),
           output_tokens.max(estimate_tokens_from_bytes(self.output_bytes, self.chars_per_token)),
       )
   } else {
       (input_tokens, output_tokens)
   };
   ```

4. Keep this in memory only. Do not query PostgreSQL/Redis/filesystem in the request path.

Acceptance:

- Add test: stream fixture without usage returns/records `estimated=true` and non-zero input tokens.
- Add test: non-stream response without usage commits at least estimated input tokens.
- `cargo test --all-targets` must still pass.

## P0 — Streaming relay is unbounded and breaks backpressure

Evidence:

- `src/proxy/mod.rs:473` creates `mpsc::unbounded::<Result<Bytes, reqwest::Error>>()`.
- `src/proxy/mod.rs:477-512` spawns a task that pulls `response.bytes_stream()`.
- `src/proxy/mod.rs:487` calls `tx.unbounded_send(Ok(bytes))`.
- `src/proxy/mod.rs:519` returns `Body::from_stream(rx)`.

Impact:

The backend reader can run ahead of the client writer without limit. A slow client can make the router buffer an unbounded stream in RAM. For long LLM streams this is a production stability failure and a latency tail amplifier.

Minimal acceptable fix:

```rust
use futures::{SinkExt, StreamExt};

let (mut tx, rx) = mpsc::channel::<Result<Bytes, reqwest::Error>>(1);
...
if tx.send(Ok(bytes)).await.is_err() {
    client_aborted = true;
    break;
}
```

Better SOTA fix:

Remove the per-request relay task and channel. Build `Body::from_stream(futures::stream::unfold(...))` directly over `response.bytes_stream()` with `(stream, acc, reporter)` as stream state. `CompletionReporter::Drop` already handles client abort if the body stream is dropped before EOF.

Acceptance:

```bash
rg -n "mpsc::unbounded|unbounded_send" src/proxy/mod.rs
```

Expected: no hits.

## P0 — Config reload publishes new snapshot before runtime state is wired

Evidence:

- `src/main.rs:90-93` on reload:

  ```rust
  Ok(snap) => {
      poll_cfg.store(Arc::new(snap));
      wire_snapshot(&poll_budget, &poll_backends, &poll_cfg.load_full());
  }
  ```

Impact:

There is a window where request handlers can read the new `ConfigSnapshot` via ArcSwap, while `RamBackendPool` and `RamBudgetStore` still reflect the old snapshot. A new route can point at a backend that has not been upserted yet; a new/changed team budget can be visible to auth/route but missing from the budget store. That violates the “swap nguyên khối” architecture.

Fix once:

```rust
Ok(snap) => {
    wire_snapshot(&poll_budget, &poll_backends, &snap);
    poll_cfg.store(Arc::new(snap));
}
```

For boot, prefer the same order for readability:

```rust
let boot_snap = boot_loader.load_snapshot().await?;
let budget = Arc::new(RamBudgetStore::new());
let backends = Arc::new(RamBackendPool::new());
wire_snapshot(&budget, &backends, &boot_snap);
cfg.store(Arc::new(boot_snap));
```

Do not add a callback framework. This is a two-line ordering fix plus optional boot cleanup.

Acceptance:

- Add a unit/integration test around reload ordering if practical; otherwise code inspection is enough for this tiny race.
- The final reload branch must call `wire_snapshot(..., &snap)` before `poll_cfg.store(...)`.

## P0 — `fallback_backend_id` is loaded but never used

Evidence:

- `src/contract.rs:64` defines `ModelRoute::fallback_backend_id`.
- `src/config/mod.rs:113-129` loads it.
- `src/route/mod.rs:113-155` `acquire_excluding` exits with `?` when `choose_candidate(route, excluded)` returns `None`.
- `src/route/mod.rs:158-240` `choose_candidate` only iterates `route.backend_ids`.

Impact:

Explicit fallback config is dead. If primary backends are down/excluded, proxy returns error instead of trying the configured fallback.

Fix once:

1. Keep normal least-load selection limited to `route.backend_ids`.
2. After primary exhaustion, try `fallback_backend_id` only if present and not excluded.
3. Do not merge fallback into the primary candidate list.
4. Add tests:

   - `acquire_excluding_skips_tried_backend`
   - `fallback_used_after_primary_exhausted`
   - `fallback_not_used_while_primary_available`

Concrete shape:

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
    self.acquire_primary(&fallback_route, &mut HashSet::new())
}
```

`acquire_primary` should contain the current loop body from `acquire_excluding`.

## P0 production correctness — budget counters reset on restart

Evidence:

- `src/config/mod.rs:52-67` reads `usage_ledger` only to log total rows/input/output.
- `src/ledger/mod.rs:294-308` `boot_counter(pool) -> HashMap<i64, u64>` aggregates all-time by `key_id`.
- `src/main.rs:77` creates an empty `RamBudgetStore`.
- `src/main.rs:166-170` `wire_snapshot` only upserts backends and calls `budget.load_teams(&snap.teams)`.
- `src/budget/mod.rs:141-143` RAM usage counters have no seed API.

Impact:

After router restart, current day/month usage starts at zero. A team can exceed budget by restarting the router. This is not hot-path latency, but it is production correctness.

Fix once:

- Add a startup/background seed path that loads current-period aggregates from `usage_ledger` for the scopes the budget store enforces: key total, key+model, team total, team+model.
- Seed `RamBudgetStore.usage` before listen.
- On config reload, reseed only when budgets change, or run a background reseed; do not query DB from handler/proxy.
- Replace `ledger::boot_counter` or change its return shape; `HashMap<key_id,total_all_time>` is not enough.

Acceptance:

- Test: existing ledger usage reaches a team monthly budget; after boot/wire, the next request is rejected.
- `rg -n "usage_ledger" src/handlers.rs src/proxy/mod.rs` must remain empty.

## P1 — Runtime snapshot sync does not prune deleted config

Evidence:

- `src/main.rs:166-170` `wire_snapshot` only upserts current backends and loads current team budgets.
- `src/route/mod.rs:86-104` `upsert_backend` does not remove/disable backend ids absent from the new snapshot.
- `src/budget/mod.rs:158-168` `load_teams` does not remove team budget entries for teams absent from the new snapshot.

Fix once:

- Add `RamBackendPool::sync_backends(&HashMap<i64, Backend>)` that retains/disables only current backend ids, then upserts.
- Add `RamBudgetStore::sync_teams(&HashMap<i64, Team>)` that retains only current team ids, then upserts/removes budgets for present teams.
- Use those from `wire_snapshot`.

## P1 — LedgerSink still silently drops in primary-closed branch

Evidence:

`src/ledger/mod.rs:41-42`:

```rust
Err(mpsc::error::TrySendError::Closed(ev)) => {
    let _ = self.overflow.try_send(ev);
}
```

Fix:

```rust
Err(mpsc::error::TrySendError::Closed(ev)) => {
    if self.overflow.try_send(ev).is_err() {
        metrics::counter!("router_ledger_dropped_total").increment(1);
        tracing::error!("ledger: primary closed and overflow unavailable, dropping usage event");
    }
}
```

No await, no file write, no blocking in `try_record`.

## P1 — Redis/Valkey remains in default runtime despite no use

Evidence:

- `Cargo.toml:34` depends on `redis`.
- `docker-compose.yml:9` sets `REDIS_URL`.
- `docker-compose.yml:17` makes router depend on `valkey`.
- `docker-compose.yml:35-40` starts `valkey`.
- `rg -n "redis::|REDIS|valkey" src` shows no runtime Redis use.

Verdict:

PostgreSQL is justified for config/admin/ledger background paths. Redis is not justified for fastest/default production unless hard global quotas/concurrency across multiple router instances are a real product requirement.

Fix:

- Remove default `redis` dependency or make it optional behind a disabled feature like `hard-quotas-redis`.
- Remove default `REDIS_URL`, `depends_on: valkey`, and `valkey` service from default compose.
- Keep an ADR note for hard quota mode. Do not implement Redis now.

## Positive architecture already achieved

- Concrete `AppState` with `Arc<RamBudgetStore>`, `Arc<RamBackendPool>`, shared `reqwest::Client`, `LedgerSink`, and `Metrics`.
- Handler is thin glue and calls `proxy_forward`; old buffered handler path is gone.
- Backend keys are resolved at config load into `Backend.api_key`; handler/proxy no longer read backend env/file on request path.
- Budget reservation and concurrency guard are RAII.
- Backend inflight is RAII via `BackendLease`.
- Admin IP allowlist now uses `ConnectInfo<SocketAddr>` instead of spoofable `x-forwarded-for`.

## Next acceptance before SOTA claim

Run:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
rg -n "mpsc::unbounded|unbounded_send|reqwest::Client::new\\(|std::env::var\\(&backend\\.api_key_ref\\)|resp\\.bytes\\(\\)\\.await|Body::from\\(upstream_body\\)|ledger_tx|state\\.inner\\.pool|BudgetStore|BackendPool|RouteDecision|SkipReason|Decision" src
```

Expected:

- gates stay green;
- no unbounded stream relay;
- no per-request backend client/key lookup;
- no old contract API;
- fallback tests prove explicit fallback behavior;
- budget restart test proves ledger-seeded counters.
