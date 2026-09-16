# CODEX gate repair card — 2026-09-16 18:xx ICT

## Superseded-state notice

This file captured the state before the later partial contract refactor. As of the follow-up audit `CODEX-contract-refactor-breakage-20260916.md`, `migrations/0001_init.sql` has been fixed and direct SQLite `executescript` passes. Do not spend time on the migration P0 below unless it regresses.

Current priority is now compile recovery: `src/contract.rs` imports non-existent `LedgerSink`/`Metrics` and removed `BudgetStore`, `BackendPool`, `Decision`, `RouteDecision`, and `SkipReason` while other modules still import them.

Scope: current worktree only. I did not edit `src/`; this file is a repair card for DeepSeek/coders.

## Current gate verdict

`cargo fmt --all -- --check` passes.

`cargo clippy --all-targets -- -D warnings` passes.

`cargo test --all-targets` fails 1/30:

```text
ledger::tests::db_down_writes_file_replay_on_reconnect ... FAILED
ledger replay failed: error returned from database: (code: 1) no such table: usage_ledger
Error: No such file or directory (os error 2)
test result: FAILED. 29 passed; 1 failed
```

Extra environment note: the Docker command in `audits/README.md` is not currently a valid proof command in this environment because the image starts without `/usr/local/cargo/bin` in `PATH`, and after adding that path the container reports `cargo-fmt` missing for toolchain `1.98.1-x86_64-unknown-linux-gnu`. Local toolchain is `cargo 1.98.1` and was used for the gate above.

## P0-1 — Migration is not executable SQL and cannot bootstrap DB

Files/lines:

- `migrations/0001_init.sql:1` starts with ```sql.
- `migrations/0001_init.sql:88` ends with ```.
- `migrations/0001_init.sql:12` uses `format IN ('open_ai','anthropic')`.
- `src/config/mod.rs:112` selects `first_byte_timeout`.
- `migrations/0001_init.sql:21` creates `first_byte_timeout_secs`.
- `src/config/mod.rs:165-181` reads `key_hash` as a 64-char hex string.
- `migrations/0001_init.sql:33` creates `key_hash BLOB`.
- `migrations/0001_init.sql:77` seeds the key hash as all-zero bytes.

Evidence command:

```bash
python3 - <<'PY'
import pathlib, sqlite3, tempfile, hashlib
s = pathlib.Path('migrations/0001_init.sql').read_text()
print('starts', repr(s[:12]))
print('ends', repr(s[-12:]))
print('lc-dev0001_sha256', hashlib.sha256(b'lc-dev0001').hexdigest())
try:
    with tempfile.NamedTemporaryFile(suffix='.db') as f:
        conn = sqlite3.connect(f.name)
        conn.executescript(s)
        conn.close()
    print('sqlite_executescript PASS')
except Exception as e:
    print('sqlite_executescript FAIL', type(e).__name__, str(e))
PY
```

Observed output:

```text
starts '```sql\n-- Br'
ends '   1\n);\n```\n'
lc-dev0001_sha256 6f48c439d85e6ec2cdf3cb47bfe0b0853e8278724b56140a9edb7d3e31a78819
sqlite_executescript FAIL OperationalError near "```sql
```

Required fix:

1. Remove Markdown fences from the SQL file.
2. Make migration and loader agree on one schema:
   - `backends.format` values must be `openai` and `anthropic`, matching `BackendFormat` serde snake-case parsing.
   - Rename `model_routes.first_byte_timeout_secs` to `first_byte_timeout`, or change loader query and struct mapping consistently. Prefer `first_byte_timeout` because loader already uses it.
   - Store `api_keys.key_hash` as `TEXT NOT NULL CHECK (length(key_hash)=64)` for SQLite dev and Postgres-compatible design docs, or update loader/admin to consistently handle bytes. Prefer hex `TEXT` because `ConfigSnapshot.keys_by_hash` needs `[u8;32]` and current loader already has `hex_to_key_hash`.
   - Replace the seed all-zero hash with `6f48c439d85e6ec2cdf3cb47bfe0b0853e8278724b56140a9edb7d3e31a78819` for demo key plaintext `lc-dev0001`, or remove the misleading seed.
3. Add a migration smoke test that executes `migrations/0001_init.sql` against a fresh SQLite DB and then calls `DbConfigLoader::load_snapshot()`. This prevents future schema drift between migration and loader.

This is P0 because the router cannot boot a fresh DB, admin-created keys and config-loader keys disagree on type, and the demo key cannot authenticate.

## P0-2 — Ledger reconnect test fails because reconnects to an uninitialized DB and then strands replay state

Files/lines:

- `src/ledger/mod.rs:97-104` reconnects and calls `replay_fallback(&p, fallback)`, then sets `pool = Some(p)` even if replay failed.
- `src/ledger/mod.rs:191-220` renames fallback to `.replay`, reads it, and removes it only after full success.
- `src/ledger/mod.rs:199-200` only handles the live fallback file; an already existing `.replay` is not replayed first.
- `src/ledger/mod.rs:381-390` test starts writer with `pool=None`; reconnect uses `connect_pool()` and env/default DB, not the test pool created later.

Evidence from gate:

```text
ledger replay failed: error returned from database: (code: 1) no such table: usage_ledger
Error: No such file or directory (os error 2)
```

Required fix:

1. Make reconnect target deterministic in the test:
   - Set `LEDGER_DATABASE_URL` to a temp SQLite DB URL before spawning the writer, create `usage_ledger` in that same DB before allowing reconnect, and restore the env var after the test; or
   - Refactor `run_with_pool` to accept a reconnect function/handle under `#[cfg(test)]`, so the test can reconnect to the pool it controls.
2. Do not set `pool = Some(p)` when `replay_fallback()` fails because the writer then resumes DB inserts while the fallback/replay backlog remains unreplayed. Keep `pool=None` on replay error unless the error is explicitly classified as non-retryable after preserving the file.
3. Before renaming `fallback` to `.replay`, check whether `.replay` already exists and replay it first. Crash sequence today can strand the renamed file forever:
   - fallback exists
   - `rename(fallback, fallback.replay)` succeeds
   - process dies before `remove_file(replay)`
   - next boot ignores `fallback.replay` because `fallback.exists()` is false
4. On replay failure after rename, leave `.replay` in place and retry it on the next reconnect cycle.

Acceptance test:

```bash
cargo test ledger::tests::db_down_writes_file_replay_on_reconnect -- --nocapture
cargo test --all-targets
```

This is P0 because ledger durability is the production reason to use PostgreSQL in this repo. A “fastest router” can be async on writes, but it cannot silently strand usage records across DB outages.

## P0-3 — Binary boots with an empty backend pool, so real requests cannot route

Files/lines:

- `src/main.rs:64-65` loads `ConfigSnapshot`.
- `src/main.rs:74-75` creates `RamBudgetStore::new()` and `RamBackendPool::new()`.
- `src/main.rs:75` never calls `pool.upsert_backend(...)`.
- `src/route/mod.rs:68` exposes `RamBackendPool::upsert_backend`, but it is only used by tests.

Evidence command:

```bash
rg -n "RamBackendPool::new|upsert_backend|load_snapshot" src/main.rs src/route/mod.rs
```

Observed relevant output:

```text
src/main.rs:64:    let boot_loader = DbConfigLoader::new(cfg_pool.clone(), CONFIG_POLL_SECS);
src/main.rs:65:    cfg.store(Arc::new(boot_loader.load_snapshot().await?));
src/main.rs:75:    let pool: Arc<dyn BackendPool> = Arc::new(RamBackendPool::new());
src/route/mod.rs:68:    pub fn upsert_backend(&self, backend: Backend) {
```

Required fix:

1. Build the `RamBackendPool` from `snapshot.backends` before casting to `Arc<dyn BackendPool>`.
2. On each config poll, sync pool state with the new snapshot:
   - upsert new/changed backends,
   - disable/remove deleted backends, or mark them disabled without losing useful circuit state for existing ids.
3. Wire team/key budget state from snapshot into `RamBudgetStore`; otherwise `teams` and `api_keys` DB budget limits exist but runtime starts empty.

Minimal shape:

```text
let snapshot = boot_loader.load_snapshot().await?;
let ram_pool = Arc::new(RamBackendPool::new());
for backend in snapshot.backends.values().cloned() {
    ram_pool.upsert_backend(backend);
}
let pool: Arc<dyn BackendPool> = ram_pool.clone();
cfg.store(Arc::new(snapshot));
```

Then make the poller update both `cfg` and `ram_pool`. Do not add Redis for this; local RAM sync is enough for the fastest single-router profile.

This is P0 because the compiled binary can pass unit tests while no configured backend is present in the live route pool.

## P0-4 — Handler bypasses streaming proxy and buffers upstream response

Files/lines:

- `src/handlers.rs:282` calls local `forward_to_backend(...)`.
- `src/handlers.rs:288` calls `resp.bytes().await`.
- `src/handlers.rs:346` returns `Body::from(upstream_body)`.
- `src/handlers.rs:446` creates a new `reqwest::Client` per request.
- `src/proxy/mod.rs:536` already has `proxy_forward(...)`, but handler does not call it.

Evidence command:

```bash
rg -n "forward_to_backend|resp\\.bytes\\(\\)\\.await|Body::from\\(upstream_body\\)|reqwest::Client::new|proxy_forward" src/handlers.rs src/proxy/mod.rs
```

Observed relevant output:

```text
src/handlers.rs:282:    let upstream = forward_to_backend(&backend, &parts, body_bytes.clone(), &backend_key).await;
src/handlers.rs:288:            let bytes = match resp.bytes().await {
src/handlers.rs:346:    let mut resp = Response::new(Body::from(upstream_body));
src/handlers.rs:446:    reqwest::Client::new()
src/proxy/mod.rs:536:pub async fn proxy_forward(
```

Required fix:

1. Delete or stop using the local `forward_to_backend` path in `handlers.rs`.
2. Make `handlers.rs` perform only:
   - auth,
   - tiny top-level extraction of `model` and `stream`,
   - budget reservation,
   - route context construction,
   - call into `proxy_forward`.
3. `proxy_forward` must use a shared `reqwest::Client` stored in `AppState`, not `Client::builder()` per request at `src/proxy/mod.rs:542`.
4. The response body for streaming must be `Body::from_stream(resp.bytes_stream().map(...))`, with usage tap/release/ledger on chunk completion/drop. The router must not await the whole upstream body before the client receives first bytes.

This is P0 for SOTA latency. Buffering the whole response makes time-to-first-token equal to full generation time.

## P0-5 — Budget API is check-then-commit, so concurrent requests can overspend

Files/lines:

- `src/contract.rs:135` has `fn try_reserve(...) -> Decision`.
- `src/contract.rs:137` has `fn commit(...)`.
- No reservation handle exists in `src/contract.rs`.
- `src/handlers.rs:242-257` allows request after `try_reserve`.
- `src/handlers.rs:311` commits only after upstream completion.

Required fix:

Replace check-then-commit with an atomic reservation handle:

```text
trait BudgetStore {
    fn reserve(&self, key: &ApiKey, model: &str, est_tokens: u64) -> Result<BudgetReservation, Decision>;
    fn commit(&self, reservation: BudgetReservation, actual_tokens: u64);
    fn refund(&self, reservation: BudgetReservation);
}
```

Implementation requirement:

- `reserve` must increment in-flight/concurrency and estimated token counters atomically before the upstream request starts.
- `commit` must reconcile actual usage against reserved estimate.
- `refund/drop` must release reservation on upstream error, timeout, or client abort.

This is P0 because overspend is a correctness bug under concurrency. It also explains why Redis should not be added now: first make local RAM semantics correct; add Redis only for optional cross-instance strict quotas later.

## P1 — Hot path still contains avoidable latency and maintainability risks

These are not the immediate red gate, but they block the “fastest production router” claim:

1. `src/handlers.rs:212` full-parses request into `serde_json::Value`. Replace with a small top-level extractor for `model`, `stream`, and existing `stream_options` detection. Full parsing is acceptable for admin/config, not inference hot path.
2. `src/handlers.rs:277` does `std::env::var(&backend.api_key_ref)` per request. `src/proxy/mod.rs:56-72` also has a separate resolver and may read files. Resolve backend secrets when loading/syncing config and keep the resolved `HeaderValue`/secret in RAM, or centralize resolver outside the hot path.
3. `src/contract.rs:152`, `src/handlers.rs:338`, and `src/proxy/mod.rs:324/375` expose raw `mpsc::Sender<UsageEvent>` and ignore `try_send` errors. Introduce a `LedgerSink` abstraction with counters for queued, dropped, and fallback enqueue failures. The handler should not silently lose usage events when the channel is full.
4. `src/proxy/mod.rs:556` treats `RouteDecision::Skip { backend_id, .. }` as a backend to call. A skip decision means “do not use this backend.” Make `pick()` return only `Pick` or `None`, or treat `Skip` as internal telemetry and never forward to that backend.
5. Metrics names are split: `src/handlers.rs` exports `brigto_router_*`, while `src/metrics.rs` tests lock `router_*`. Use one metrics module and one naming contract.

## Fix order that avoids churn

1. Fix `migrations/0001_init.sql` and add migration-to-loader smoke test.
2. Fix ledger replay/reconnect semantics until `cargo test --all-targets` is green.
3. Wire snapshot backends and budget state into runtime on boot and poll.
4. Move handler to the real streaming proxy path and shared client.
5. Replace budget check/commit with reservation/refund semantics.
6. Only after those are green, benchmark hot path. Do not add PostgreSQL calls, Redis calls, request-body full parse, filesystem reads, or per-request clients to improve any item above.

## Production dependency verdict

Use PostgreSQL for config/admin/ledger in background paths. Do not put PostgreSQL in the request forwarding path.

Do not add Redis for the fastest profile. Redis is justified only for strict multi-instance global quota/concurrency. If added later, it must be one optional Lua reserve call before upstream plus commit/refund after; no Redis for route selection, backend health, config reads, or ledger writes.
