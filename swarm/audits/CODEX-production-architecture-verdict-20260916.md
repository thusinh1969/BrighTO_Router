# CODEX production architecture verdict — 2026-09-16

Scope: current worktree at 2026-09-16 ICT. I did not edit `src/`; this is an auditor/advisor handoff for the coder.

## Verified gates on current worktree

Commands run successfully:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo test --test streaming_integration -- --nocapture
cargo build --release --locked
```

Current result:

- `cargo check --all-targets`: PASS.
- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --all-targets -- -D warnings`: PASS.
- `cargo test --all-targets`: PASS, 33 lib tests + 1 integration test.
- `cargo test --test streaming_integration -- --nocapture`: PASS.
- `cargo build --release --locked`: PASS after waiting for a previous build lock.
- `DATABASE_URL=postgres://... sqlx migrate run` on a clean `pgvector/pgvector:pg16` container: FAIL, `syntax error at or near "PRAGMA"`. See `CODEX-postgres-migration-gate-red-20260916.md`.
- `cargo audit`: NOT RUN because `cargo-audit` is not installed in this environment.

This is a Rust compile/test milestone only. Production DB migration is red, so it is not a production/SOTA verdict.

DB update after live Postgres probes: the DB-layer recommendation in this file is superseded by `CODEX-postgres-only-db-layer-verdict-20260916.md`. The stronger current recommendation is Postgres-only production code, not `sqlx::Any` dual dialect. Keep the rest of this file for non-DB architecture blockers.

## Architecture verdict

The hot-path direction is right: concrete `AppState`, shared `reqwest::Client`, `ArcSwap<ConfigSnapshot>`, RAM budget counters, RAII budget/concurrency/backend leases, and ledger writer off the request path. That is the right shape for the fastest profile.

The repo cannot claim production or “fastest in the world” yet because it still has production correctness blockers and lacks a runnable root benchmark artifact. The fixes below are small and should stay small. Do not add Redis, a service mesh, a scheduler, a plugin system, or provider abstraction work to solve these.

PostgreSQL is justified for production config/admin/ledger background paths. Redis is not justified in the default fastest profile. Redis is only justified later for strict cross-instance global quota/concurrency; if that requirement appears, it must be a separate optional implementation and must not touch route selection, config reads, health, ledger, or metrics.

## Fix order — do these in this order

1. Fix production migration/schema so Postgres can actually run.
2. Fix graceful shutdown so SIGTERM stops accepting immediately.
3. Remove unbounded stream relay.
4. Ensure estimated usage never commits/ledgers zero total tokens when backend omits usage.
5. Wire config reload before publishing snapshot and prune deleted runtime config.
6. Make `fallback_backend_id` live.
7. Seed RAM budget counters from current-period ledger on boot.
8. Remove Redis/Valkey from default fastest profile.
9. Restore root benchmark scripts and produce router-vs-direct results.

Do not reorder by convenience. The first two are production deploy blockers; the next five are SOTA correctness/latency blockers.

## P0 production deploy — root migration is SQLite, but production/default DB is Postgres

Superseded direction: see `CODEX-postgres-only-db-layer-verdict-20260916.md` for the full DB-layer fix. The evidence below remains valid, but the fix should now be Postgres-only instead of preserving `sqlx::Any` in production.

Evidence:

- `.env.example:2` and `docker-compose.yml:8` point `DATABASE_URL` at Postgres.
- `migrations/0001_init.sql:3` contains `PRAGMA foreign_keys = ON`, which is SQLite-specific.
- `migrations/0001_init.sql:6`, `:25`, and `:32` use `INTEGER PRIMARY KEY AUTOINCREMENT`, which is SQLite-specific.
- `migrations/0001_init.sql:18` comments that Postgres should use `BIGINT[]`, but `src/config/mod.rs:119-124` reads `backend_ids` as `String` and parses JSON text.
- `migrations/0001_init.sql:27` and `:38` comment that Postgres should use `JSONB`, but `src/config/mod.rs:146-150` and `:177-190` read budget as `Option<String>` and parse JSON text.
- `src/config/mod.rs:79-87`, `:144-148`, and `:171-181` read `enabled` through `g_i64`, while a real Postgres schema would naturally use `BOOLEAN`.
- `src/admin/mod.rs:328-334`, `:369-383`, `:421-422`, and `:445` bind Rust `bool` for `enabled`; that conflicts with an integer-only Postgres schema.

Impact:

Production migration is not currently runnable against the declared production database. Even after syntactic fixes, schema choices can break the loader/admin code if `BIGINT[]`, `JSONB`, or boolean/int representations are mixed without explicit code support.

Fix once:

- Make the root `migrations/0001_init.sql` a Postgres migration because root `.env.example`, compose, and production use Postgres.
- Keep JSON-shaped fields as `TEXT` for now to match the current loader/admin with zero dialect branching: `model_routes.backend_ids`, `api_keys.allowed_models`, `teams.budget`, and `api_keys.budget` stay JSON-encoded text.
- Use Postgres native IDs and booleans; then update config loader to decode booleans as booleans instead of `i64`.

Concrete Postgres DDL shape:

```sql
CREATE TABLE IF NOT EXISTS backends (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    api_key_ref TEXT NOT NULL,
    weight INTEGER NOT NULL DEFAULT 1 CHECK (weight >= 1),
    max_inflight INTEGER NOT NULL DEFAULT 0,
    format TEXT NOT NULL CHECK (format IN ('openai','anthropic')),
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS model_routes (
    model_name TEXT PRIMARY KEY,
    backend_ids TEXT NOT NULL DEFAULT '[]',
    fallback_backend_id BIGINT,
    chars_per_token DOUBLE PRECISION NOT NULL DEFAULT 4.0,
    first_byte_timeout BIGINT NOT NULL DEFAULT 180
);

CREATE TABLE IF NOT EXISTS teams (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    budget TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS api_keys (
    id BIGSERIAL PRIMARY KEY,
    key_hash TEXT NOT NULL,
    key_prefix TEXT NOT NULL,
    team_id BIGINT NOT NULL REFERENCES teams(id),
    owner TEXT NOT NULL,
    allowed_models TEXT NOT NULL DEFAULT '[]',
    budget TEXT,
    rpm_limit INTEGER,
    concurrency_limit INTEGER,
    expires_at BIGINT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS usage_ledger (
    ts BIGINT NOT NULL,
    request_id TEXT NOT NULL,
    key_id BIGINT NOT NULL,
    team_id BIGINT NOT NULL,
    model TEXT NOT NULL,
    backend_id BIGINT NOT NULL,
    status INTEGER NOT NULL,
    input_tokens BIGINT NOT NULL,
    output_tokens BIGINT NOT NULL,
    estimated BOOLEAN NOT NULL,
    ttfb_ms BIGINT NOT NULL,
    total_ms BIGINT NOT NULL,
    router_overhead_ms BIGINT NOT NULL,
    stream BOOLEAN NOT NULL,
    client_aborted BOOLEAN NOT NULL,
    error_class TEXT
);
```

Code changes required with that schema:

- Add `fn g_bool(row: &AnyRow, idx: usize) -> anyhow::Result<bool> { Ok(row.try_get::<bool, _>(idx)?) }` in `src/config/mod.rs`.
- Change `load_backends`, `load_teams`, and `load_api_keys` to use `g_bool` for `enabled`.
- Keep `backend_ids` and `allowed_models` as JSON text until there is a proven need for Postgres arrays. Arrays/JSONB add dialect handling without helping the request path.

Acceptance:

```bash
DATABASE_URL=postgres://... sqlx migrate run
cargo test --all-targets
cargo build --release --locked
```

Also add one Postgres smoke test or CI job that boots `DbConfigLoader::load_snapshot()` against the migrated Postgres schema. Do not rely only on SQLite inline schemas.

## P0 production shutdown — SIGTERM handling delays graceful shutdown instead of starting it

Evidence:

- `src/main.rs:146-150` passes `shutdown_signal(stop_grace_period)` into `with_graceful_shutdown`.
- `src/main.rs:173-184` waits for SIGTERM/SIGINT and then sleeps for `grace_period` before the future resolves.

Impact:

`with_graceful_shutdown` starts graceful shutdown only after the future resolves. The current code receives SIGTERM, sleeps for the whole grace period while the server is still accepting traffic, then returns. In Docker/Kubernetes this can lead to SIGKILL arriving right when the router finally begins graceful shutdown, cutting active streams.

Fix once:

- Make the shutdown future resolve immediately after signal.
- Let Docker/Kubernetes `stop_grace_period`/terminationGracePeriodSeconds provide the outer kill deadline.
- Keep the code simple; no custom drain manager until metrics prove it is needed.

Concrete shape:

```rust
async fn shutdown_signal() {
    let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }
    tracing::info!("shutdown signal received");
}

axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
    .with_graceful_shutdown(shutdown_signal())
    .await?;
```

Acceptance:

- Code inspection: no `tokio::time::sleep(grace_period)` inside the shutdown future passed to `with_graceful_shutdown`.
- Optional integration: open a streaming response, send SIGTERM, verify listener stops accepting new connections while the active stream is allowed to finish until orchestrator grace kills it.

## P0 SOTA stability — streaming relay is unbounded and breaks backpressure

Evidence:

- `src/proxy/mod.rs:473` creates `mpsc::unbounded::<Result<Bytes, reqwest::Error>>()`.
- `src/proxy/mod.rs:477-512` spawns a task that reads `response.bytes_stream()`.
- `src/proxy/mod.rs:487` calls `tx.unbounded_send(Ok(bytes))`.
- `src/proxy/mod.rs:519` returns `Body::from_stream(rx)`.

Impact:

A slow client lets the backend-reader task run ahead without limit. Long LLM streams can accumulate unbounded bytes in router RAM. This is a production stability failure and a tail-latency amplifier.

Minimal fix:

```rust
use futures::{SinkExt, StreamExt};

let (mut tx, rx) = mpsc::channel::<Result<Bytes, reqwest::Error>>(1);
...
if tx.send(Ok(bytes)).await.is_err() {
    client_aborted = true;
    break;
}
```

Better fix, still small:

Use `Body::from_stream(futures::stream::unfold(...))` directly over `response.bytes_stream()` and keep `(stream, acc, reporter)` in the stream state. That removes the per-request task and channel completely. `CompletionReporter::Drop` already gives the right abort cleanup when the body stream is dropped.

Acceptance:

```bash
rg -n "mpsc::unbounded|unbounded_send" src/proxy/mod.rs
```

Expected: no hits.

Keep the existing integration test `tests/streaming_integration.rs`, then add a slow-client/backpressure test only if it can be deterministic. Do not add a large async harness just for this.

## P0 accounting — backend omitted usage still commits zero total tokens

Evidence:

- `src/proxy/mod.rs:503-511` finishes stream requests with `acc.input_tokens` and `acc.output_tokens`.
- `src/proxy/mod.rs:503` sets `estimated = !acc.seen_usage`, but if no usage was seen both counters remain `0`.
- `src/proxy/mod.rs:523-531` does the same for non-stream responses when `parse_usage_from_body` sees no usage.
- `src/proxy/mod.rs:324-327` commits budget with `input_tokens + output_tokens`.
- `src/budget/mod.rs:350-357` refunds the reservation when actual is lower than reserved, so `commit(0)` makes the request free.
- `tests/fixtures/stream_no_usage_option.sse` proves real streams without usage exist.

Impact:

A backend that omits `usage` produces `estimated=true` but commits and ledgers `0` tokens. That violates the stated invariant: “Không bao giờ ghi 0 rồi im.” It also lets restart-free budget enforcement be bypassed by providers that omit usage.

Fix once:

- Pass the existing request estimate from handler into proxy metadata.
- When `estimated == true` and total tokens are zero, commit and ledger at least the request estimate.
- Do not query PostgreSQL/Redis/filesystem and do not parse the full prompt again.

Concrete shape:

```rust
pub struct ProxyContext {
    ...
    pub estimated_input_tokens: u64,
}
```

In `src/handlers.rs`, after `let est_tokens = estimate_tokens(&body_bytes, &route);`, set:

```rust
estimated_input_tokens: est_tokens,
```

In `CompletionReporter`, store `estimated_input_tokens`. At the start of `finish` before `actual`:

```rust
if estimated && input_tokens.saturating_add(output_tokens) == 0 {
    input_tokens = input_tokens.max(self.estimated_input_tokens.max(1));
}
let actual = input_tokens.saturating_add(output_tokens);
```

This is intentionally conservative and cheap. It does not need output-token estimation yet. If later cost accuracy matters, add a provider-specific output estimator behind the same `estimated=true` flag; do not block the current fix on that.

Acceptance:

- Add test: stream fixture without usage records `estimated=true` and `input_tokens + output_tokens > 0`.
- Add test: non-stream response without usage commits at least the input estimate.
- `cargo test --all-targets` stays green.

## P0 config consistency — reload publishes snapshot before runtime stores are wired

Evidence:

- `src/main.rs:89-93` on reload does:

```rust
Ok(snap) => {
    poll_cfg.store(Arc::new(snap));
    wire_snapshot(&poll_budget, &poll_backends, &poll_cfg.load_full());
}
```

Impact:

Handlers can observe the new `ConfigSnapshot` before `RamBackendPool` and `RamBudgetStore` are updated. A new route can point at a backend not yet in the pool; a new/changed team budget can be visible to auth/route but absent from the budget store.

Fix once:

```rust
Ok(snap) => {
    wire_snapshot(&poll_budget, &poll_backends, &snap);
    poll_cfg.store(Arc::new(snap));
}
```

Boot can use the same order for readability:

```rust
let boot_snap = boot_loader.load_snapshot().await?;
wire_snapshot(&budget, &backends, &boot_snap);
cfg.store(Arc::new(boot_snap));
```

Acceptance:

- Code inspection: reload branch calls `wire_snapshot(..., &snap)` before `poll_cfg.store(...)`.
- No callback framework. No DB or Redis call in handler/proxy.

## P0 routing correctness — `fallback_backend_id` is loaded but dead

Evidence:

- `src/contract.rs:64` defines `ModelRoute::fallback_backend_id`.
- `src/config/mod.rs:111-132` loads it from DB.
- `src/route/mod.rs:120-155` exits when `choose_candidate(route, &excluded)?` returns `None`.
- `src/route/mod.rs:158-240` `choose_candidate` iterates only `route.backend_ids`.

Impact:

Explicit fallback config has no production effect. After primary exhaustion, the router returns an error instead of trying the configured fallback.

Fix once:

- Keep primary least-load selection limited to `route.backend_ids`.
- After primary exhaustion, try `fallback_backend_id` only if present and not already excluded.
- Do not merge fallback into the primary list; fallback must not steal normal traffic.

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

Move the current loop body into `acquire_primary`.

Acceptance tests:

- `fallback_not_used_while_primary_available`.
- `fallback_used_after_primary_exhausted`.
- `acquire_excluding_skips_tried_backend`.
- `fallback_not_retried_if_already_excluded`.

## P0 production quota — RAM budget counters reset on restart

Evidence:

- `src/main.rs:77` creates an empty `RamBudgetStore`.
- `src/main.rs:166-170` `wire_snapshot` only upserts backends and calls `budget.load_teams(&snap.teams)`.
- `src/config/mod.rs:52-67` reads `usage_ledger` only to log totals.
- `src/ledger/mod.rs:293-308` `boot_counter(pool) -> HashMap<i64, u64>` aggregates all-time by `key_id` only.
- `src/budget/mod.rs:141-143` has RAM usage counters but no seed API.

Impact:

After restart, current day/month usage starts at zero. Any real budget can be exceeded by restarting the router. This does not affect hot-path latency, but it blocks production quota correctness.

Fix once:

- Add `RamBudgetStore::seed_usage(...)` for the scopes it enforces: key total, key+model, team total, team+model.
- Replace `ledger::boot_counter` with current-period aggregates; all-time `HashMap<key_id,total>` is not enough.
- Seed before listening.
- On config reload, reseed only when effective budget scope changes or run a slow background refresh; do not query DB from handler/proxy.

Concrete aggregate shape:

```rust
pub struct UsageSeed {
    pub scope: UsageScope,        // KeyTotal | KeyModel | TeamTotal | TeamModel
    pub id: i64,
    pub model: String,            // "" for total scopes
    pub period_start: i64,
    pub used_tokens: u64,
}
```

Query only the current period required by loaded budgets. Example for team total:

```sql
SELECT team_id, COALESCE(SUM(input_tokens + output_tokens), 0) AS used
FROM usage_ledger
WHERE ts >= $1 AND team_id = ANY($2)
GROUP BY team_id;
```

If staying with `sqlx::Any`, build SQLite/Postgres-compatible `IN (?, ?, ...)` with `QueryBuilder` instead of Postgres `ANY($2)`.

Acceptance:

- Test: ledger has current-month usage at the team budget cap; after boot/seed the next request is rejected.
- Test: previous-period usage is ignored.
- `rg -n "usage_ledger" src/handlers.rs src/proxy/mod.rs` remains empty.

## P1 config lifecycle — runtime stores never prune deleted config

Evidence:

- `src/main.rs:166-170` `wire_snapshot` only upserts current backends and loads current team budgets.
- `src/route/mod.rs:86-104` `upsert_backend` does not remove/disable backend IDs absent from the new snapshot.
- `src/budget/mod.rs:158-168` `load_teams` only iterates current teams; it does not remove budget entries for teams absent from the new snapshot.

Impact:

Deleted backends and team budget entries can remain live inside runtime stores after config reload. Routes normally come from the new snapshot, so this is less severe than the reload ordering race, but it is still stale runtime state.

Fix once:

- Replace `RamBackendPool::upsert_backend` calls in `wire_snapshot` with `RamBackendPool::sync_backends(&HashMap<i64, Backend>)`.
- Implement `sync_backends` as: retain only IDs present in the snapshot, then upsert each backend. If physical removal risks racing with an existing `BackendLease::Drop`, mark absent states disabled first and remove only when inflight is zero.
- Replace `RamBudgetStore::load_teams` with `sync_teams(&HashMap<i64, Team>)`: retain only present team IDs, insert/update budgets for enabled teams with budgets, remove for disabled/no-budget teams.

Acceptance:

- Test: remove a backend from snapshot, call sync, route cannot acquire it.
- Test: remove a team budget from snapshot, budget gauge/snapshot no longer reports that team.

## P1 ledger durability — primary-closed branch still drops silently

Evidence:

`src/ledger/mod.rs:39-41`:

```rust
Err(mpsc::error::TrySendError::Closed(ev)) => {
    let _ = self.overflow.try_send(ev);
}
```

Impact:

If the primary channel is closed and the overflow channel is also closed/full, the event is dropped without the `router_ledger_dropped_total` counter and without an error log.

Fix:

```rust
Err(mpsc::error::TrySendError::Closed(ev)) => {
    if self.overflow.try_send(ev).is_err() {
        metrics::counter!("router_ledger_dropped_total").increment(1);
        tracing::error!("ledger: primary closed and overflow unavailable, dropping usage event");
    }
}
```

Acceptance:

- Extend `ledger::tests::sink_overflow_never_blocks_or_drops_silently` to cover primary-closed plus overflow-closed/full.
- `try_record` must stay synchronous and non-blocking. No file write in `try_record`.

## P1 default footprint — Redis/Valkey remains in default runtime despite no use

Evidence:

- `Cargo.toml:34` depends on `redis`.
- `docker-compose.yml:9` sets `REDIS_URL`.
- `docker-compose.yml:17` makes router depend on `valkey`.
- `docker-compose.yml:35-40` starts `valkey`.
- `install/docker-compose.yml` and `install/router-setup/docker-compose.yml` repeat Valkey.
- `rg -n "redis::|Redis|REDIS|valkey" src` shows no runtime Redis use.

Verdict:

Remove Redis/Valkey from the default fastest profile. Keeping an unused network service in default compose makes the architecture ambiguous and increases production operating surface without improving latency or correctness.

Fix:

- Remove the default `redis` dependency from `Cargo.toml`, or make it optional behind a disabled feature named `hard-quotas-redis`.
- Remove `REDIS_URL`, `depends_on: valkey`, and `valkey` service from root and install compose files.
- Update install docs: fastest/default profile uses PostgreSQL only; Redis is reserved for optional strict global quota mode.

Acceptance:

```bash
rg -n "redis|Redis|REDIS|valkey|Valkey" Cargo.toml docker-compose.yml install docs src
```

Expected after fix: only an ADR/doc section describing optional hard global quota mode, no default dependency/service/env.

## P1 SOTA evidence — root benchmark path is broken/missing

Evidence:

- `Makefile:37-38` defines `bench` as `bench/run.sh`.
- `rg --files` shows no root `bench/run.sh`; the full scripts exist only under `install/router-setup/bench/`.
- `scripts/bench_smoke.py` exists, but its own header says it is only directional smoke and official SOTA numbers need payload 1K/50K/200K plus concurrency 1/50/200.
- There is no committed `bench/results/` artifact in the root repo.

Impact:

The repo cannot substantiate “fastest in the world” or even its own documented threshold. Compile tests prove correctness for small cases; they do not prove router overhead, TTFB delta, p99 delta, memory under slow clients, or direct-vs-router performance.

Fix once:

- Move or copy `install/router-setup/bench/` to root `bench/`, or change `Makefile:37-38` to call a maintained root benchmark command.
- Keep `scripts/bench_smoke.py` as smoke only; do not use its numbers for the SOTA claim.
- Make `make bench` runnable from root.
- Record raw output under `bench/results/<timestamp>/` with enough metadata: router commit, Rust version, CPU, kernel, backend URL/model, direct URL/model, payload size, concurrency, duration.
- Bench direct backend first, then router with the same payload and concurrency.

Minimum SOTA acceptance:

```bash
make bench ROUTER_URL=http://... DIRECT_URL=http://... ROUTER_KEY=... DIRECT_KEY=... MODEL=... CONC=50 DUR=60s
```

Target from existing docs remains reasonable:

- streaming TTFB delta: router minus direct < 3 ms;
- non-stream p99 delta at 200K payload: router minus direct < 2 ms;
- no unbounded memory growth under slow client streaming.

Do not claim “world fastest” until at least router-vs-direct artifacts exist. If comparing against external routers, add that later as a separate benchmark matrix.

## P1 security gate — `cargo audit` is declared but unavailable here

Evidence:

- `Makefile:29-35` includes `cargo audit` in `make check`.
- In this environment, `cargo-audit` is missing; command output: `cargo-audit missing`.

Fix:

- Install `cargo-audit` in the dev/Docker toolchain or remove it from `make check` and move it to CI where it is available.
- Do not report `make check` as complete until this is resolved.

Acceptance:

```bash
cargo audit
```

Expected: command runs and exits 0, or CI has a documented equivalent.

## Positive architecture to preserve

Keep these choices:

- `src/contract.rs:141-149` concrete `AppState`; no trait-object maze on hot path.
- `src/main.rs:156-162` shared `reqwest::Client`; no per-request client creation.
- `src/handlers.rs:93-102` auth before body read; wrong keys fail before reading large payloads.
- `src/handlers.rs:139-181` handler is thin glue and hands forwarding to proxy.
- `src/budget/mod.rs:33-64` budget reservation rollback-on-drop.
- `src/budget/mod.rs:67-80` concurrency guard rollback-on-drop.
- `src/route/mod.rs:26-41` backend inflight lease rollback-on-drop.
- `src/proxy/mod.rs:55-72` drops client auth headers before backend forward.
- `src/proxy/mod.rs:632-637` injects OpenAI stream usage request without parsing/re-encoding full JSON.
- `src/admin/mod.rs:347-353`, `:392-399`, `:436-442`, and `:457-464` use `ConnectInfo<SocketAddr>` for admin IP auth; do not go back to spoofable forwarded headers.

## Final acceptance before any SOTA claim

Run and save outputs:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release --locked
cargo audit
DATABASE_URL=postgres://... sqlx migrate run
rg -n "mpsc::unbounded|unbounded_send" src/proxy/mod.rs
rg -n "usage_ledger" src/handlers.rs src/proxy/mod.rs
rg -n "redis|Redis|REDIS|valkey|Valkey" Cargo.toml docker-compose.yml install docs src
make bench ROUTER_URL=http://... DIRECT_URL=http://... ROUTER_KEY=... DIRECT_KEY=... MODEL=... CONC=50 DUR=60s
```

Expected:

- all Rust gates pass;
- Postgres migration passes on a clean Postgres database;
- no unbounded stream relay;
- no ledger DB query in handler/proxy;
- Redis/Valkey absent from default fastest profile;
- benchmark artifacts prove router-vs-direct overhead within target.
