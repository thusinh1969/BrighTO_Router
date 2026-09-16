# CODEX -> DeepSeek: SOTA fastest contract, no over-engineering

Timestamp: 2026-09-16 18:20 Asia/Ho_Chi_Minh.

This file is intentionally named `CODEX-*.md` because `scripts/audit_watch.sh` only ingests `audits/CODEX-*.md` and `audits/audit-*.md`.

## Current Gate Evidence

I ran the repo-approved Docker gate on the current worktree:

```bash
docker run --rm -v "$PWD":/app -v brigto-cargo-registry:/usr/local/cargo/registry \
  -v brigto-rustup:/usr/local/rustup -w /app rust:1.98.1-bookworm bash -c \
  "cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets"
```

Result:

- `cargo fmt --all -- --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test --all-targets`: FAIL, 29/30 pass
- failing test: `ledger::tests::db_down_writes_file_replay_on_reconnect`
- failing output: `ledger replay failed: error returned from database: (code: 1) no such table: usage_ledger` and `Error: No such file or directory (os error 2)`

I also ran a direct SQLite migration parse:

```bash
python3 - <<'PY'
import sqlite3, pathlib
sql = pathlib.Path("migrations/0001_init.sql").read_text()
con = sqlite3.connect(":memory:")
con.executescript(sql)
PY
```

Result: `OperationalError: near "```sql"` because `migrations/0001_init.sql:1` starts with a Markdown fence.

## Verdict

The target architecture should be a very small userspace streaming proxy with RAM-only decisions. That is the right SOTA path.

Do not add Redis to the default production path. Do not add vLLM metrics scheduling yet. Do not add provider translation, semantic cache, prompt logging, or a complex portal. These are not needed to make the router fast.

Use PostgreSQL in production, but only for background config/ledger/admin. PostgreSQL must never be in the inference request path.

Use Redis only behind an optional `StrictGlobalQuota` implementation when the business requirement says budgets/RPM/concurrency must be hard across multiple router instances. Fastest profile is RAM-only.

## Non-Negotiable Hot Path

Every inference request must do exactly this shape:

```text
auth header
-> sha256
-> ArcSwap snapshot lookup
-> read body Bytes with max limit
-> extract top-level model + stream only
-> permission check
-> RAM budget reservation
-> RAM key concurrency guard
-> RAM backend lease, increments inflight
-> optional OpenAI stream_options byte splice
-> shared reqwest Client execute
-> immediate Body::from_stream passthrough
-> tap only chunks containing "usage"
-> on completion/drop: commit/refund, release guards, ledger sink, metrics
```

Forbidden in the hot path:

- PostgreSQL.
- Redis in fastest profile.
- `std::fs`.
- env lookup.
- new `reqwest::Client`.
- full request `serde_json::Value`.
- buffering stream response with `resp.bytes().await`.
- raw `mpsc::Sender<UsageEvent>` exposed to handler/proxy.
- manual early returns that skip release of budget/concurrency/backend inflight.

## One-Pass Fix Order

### 1. Fix migration first

Files:

- `migrations/0001_init.sql`
- `src/config/mod.rs`
- `src/admin/mod.rs`

Required changes:

- Remove the Markdown fences from the SQL migration.
- Use one backend format string everywhere. Pick `openai` and `anthropic`, because the plan and loader already expect `openai`.
- Align timeout column. Either migration uses `first_byte_timeout`, or loader queries `first_byte_timeout_secs`. Pick one and keep it everywhere.
- Align key hash storage. For `sqlx::Any`, use `TEXT` hex for `key_hash` unless you intentionally rewrite loader/admin for BLOB bytes. Simpler: `key_hash TEXT NOT NULL`.
- If keeping demo key `lc-dev0001`, store real SHA-256 hex:
  `6f48c439d85e6ec2cdf3cb47bfe0b0853e8278724b56140a9edb7d3e31a78819`.
- Add `UNIQUE(request_id)` or unique index on `usage_ledger.request_id` for idempotent replay.

Acceptance:

```bash
python3 - <<'PY'
import sqlite3, pathlib
sql = pathlib.Path("migrations/0001_init.sql").read_text()
con = sqlite3.connect(":memory:")
con.executescript(sql)
print("migration ok")
PY
```

Expected: `migration ok`.

### 2. Replace half-traits with concrete runtime state or complete traits

Files:

- `src/contract.rs`
- `src/main.rs`
- `src/handlers.rs`
- `src/budget/mod.rs`
- `src/route/mod.rs`
- `src/ledger/mod.rs`

Current problem:

- `AppState` stores `Arc<dyn BudgetStore>` and `Arc<dyn BackendPool>`.
- Required methods exist only on concrete structs: `RamBudgetStore::load_teams`, `try_acquire_concurrency`, `release_concurrency`, `RamBackendPool::upsert_backend`, `inc_inflight`, `dec_inflight`.
- Because the traits do not expose those methods, the real server cannot use them.

Preferred fix:

```rust
pub struct AppState {
    pub cfg: Arc<ArcSwap<ConfigSnapshot>>,
    pub budget: Arc<RamBudgetStore>,
    pub backends: Arc<RamBackendPool>,
    pub client: reqwest::Client,
    pub ledger: LedgerSink,
    pub metrics: MetricsHandle,
    pub max_body_bytes: usize,
}
```

If you insist on traits, traits must return guards:

```rust
trait BudgetStore {
    fn reserve(&self, key: &ApiKey, model: &str, est_tokens: u64) -> Result<BudgetReservation, Decision>;
    fn acquire_concurrency(&self, key: &ApiKey) -> Result<ConcurrencyGuard, Decision>;
}

trait BackendPool {
    fn acquire(&self, route: &ModelRoute) -> Result<BackendLease, NoBackend>;
}
```

Do not keep the current half-trait shape. It is the direct reason the server compiles while missing production behavior.

Acceptance:

- `rg -n "load_teams|upsert_backend|try_acquire_concurrency|release_concurrency|inc_inflight|dec_inflight" src` must show calls from runtime code, not only tests.
- A unit test must prove a configured backend can be picked after bootstrap sync.
- A unit test must prove `concurrency_limit=1` rejects the second concurrent request and releases after drop.

### 3. Build one shared reqwest client

Files:

- `src/main.rs`
- `src/proxy/mod.rs`
- `src/handlers.rs`

Required:

- Build one `reqwest::Client` at startup.
- Store it in `AppState`.
- Proxy uses `state.client.clone()` or reference. `reqwest::Client` clone is cheap and shares pool.
- No `Client::new()` or `Client::builder()` inside request handling.

Acceptance:

```bash
rg -n "Client::new\\(|Client::builder\\(" src
```

Expected: only startup/tests, not `handle_generate` or `proxy_forward`.

### 4. Make `handlers.rs` thin; `proxy/mod.rs` owns forwarding

Files:

- `src/handlers.rs`
- `src/proxy/mod.rs`

Required:

- Delete local `forward_to_backend` from handlers.
- Delete local `extract_usage` from handlers.
- Handler only performs auth/body/model/budget/route setup and calls `proxy_forward`.
- `proxy_forward` handles backend request, retry before first byte, stream pump, usage tap, response body.

Current bad evidence:

- `src/handlers.rs:282` calls local `forward_to_backend`.
- `src/handlers.rs:288` buffers upstream with `resp.bytes().await`.
- `src/handlers.rs:346` returns buffered `Body::from(upstream_body)`.

Acceptance:

```bash
rg -n "forward_to_backend|extract_usage|resp\\.bytes\\(\\)\\.await|Body::from\\(upstream_body\\)" src/handlers.rs
```

Expected: no matches.

### 5. Fix request-head parsing

File:

- `src/handlers.rs`

Current bad evidence:

- `src/handlers.rs:212` parses full request into `serde_json::Value`.

Required v1:

```rust
#[derive(serde::Deserialize)]
struct RequestHead {
    model: String,
    #[serde(default)]
    stream: bool,
}
```

Use this as a short-term pass. Later, benchmark a top-level scanner. Do not write a complex scanner now unless the serde head parse misses the overhead target.

Acceptance:

```bash
rg -n "serde_json::from_slice::<Value>|serde_json::Value" src/handlers.rs
```

Expected: no full request `Value` parse in handler.

### 6. Implement RAM reservation, not check-then-commit

Files:

- `src/contract.rs`
- `src/budget/mod.rs`
- `src/handlers.rs`
- `src/proxy/mod.rs` if stream completion owns commit

Current bad evidence:

- `src/contract.rs:135` `try_reserve(...) -> Decision`
- `src/contract.rs:137` `commit(...)`
- `src/budget/mod.rs:245` checks current usage.
- `src/budget/mod.rs:266` commits only after response.

Required:

```text
reserve(est_tokens):
  CAS add est_tokens for every applicable scope
  rollback previous scopes if any scope fails
  return BudgetReservation

commit(actual_tokens):
  actual > est: add delta
  actual < est: subtract refund
```

This remains RAM-only. It is fast. It is not Redis.

Acceptance:

- Add a concurrent unit test: budget 100, two concurrent reservations of 80. Exactly one succeeds.
- Add refund test: reserve 100, commit 30, remaining becomes 70.
- Add delta test: reserve 30, commit 50, remaining decreases by 50 total.

### 7. Add RAII guards

Files:

- `src/budget/mod.rs`
- `src/route/mod.rs`
- `src/proxy/mod.rs`
- `src/handlers.rs`

Required guards:

- `ConcurrencyGuard`: releases key inflight on `Drop`.
- `BackendLease`: increments backend inflight on acquire, decrements on `Drop`.
- `BudgetReservation`: commits/refunds explicitly; if dropped without commit, refund estimate.

Reason: fastest code still needs correctness on every return path and client abort. Manual release after awaits will be missed.

Acceptance:

- Tests must cover upstream error, bad model, client/proxy error, and ensure gauges return to zero.
- `rg -n "release_concurrency|dec_inflight" src/handlers.rs src/proxy/mod.rs` should show guard internals, not scattered manual cleanup.

### 8. LedgerSink, no silent drop

Files:

- `src/contract.rs`
- `src/ledger/mod.rs`
- `src/handlers.rs`
- `src/proxy/mod.rs`

Current bad evidence:

- `src/handlers.rs:338` ignores `try_send` failure.
- `src/proxy/mod.rs:324` and `src/proxy/mod.rs:375` ignore `try_send` failure.

Required:

```rust
pub struct LedgerSink { ... }

impl LedgerSink {
    pub fn try_record(&self, ev: UsageEvent);
}
```

`try_record` must not block the response. If primary channel is full, it must send to a fallback writer path or at least an overflow queue with explicit metrics. Silent drop is not acceptable for production usage accounting.

Replay requirements:

- handle both `ledger_fallback.jsonl` and `ledger_fallback.replay`.
- idempotency uses `request_id` unique index.
- crash during replay must not strand all events forever in `.replay`.

Acceptance:

- Current failing test must pass.
- Add channel-full test that proves the event is written to fallback path or overflow writer, not dropped.

### 9. Streaming is the product

Files:

- `src/proxy/mod.rs`
- `src/handlers.rs`

Required:

- Return `Body::from_stream` for stream requests.
- Forward each backend chunk immediately.
- `memmem::find(chunk, b"\"usage\"")` before parsing.
- OpenAI stream requests without `"stream_options"` must splice `include_usage:true`.
- Ledger for streaming finalizes from stream task/reporter.
- Client abort marks `client_aborted=true` and drops upstream stream.

Acceptance:

- A test with an artificial delayed SSE backend must prove first client chunk arrives before backend completes.
- Existing fixture `tests/fixtures/stream_with_usage.sse` must still parse usage.
- Existing fixture `tests/fixtures/stream_no_usage_option.sse` documents why splice is required.

### 10. Metrics and overhead must not lie

Files:

- `src/metrics.rs`
- `src/handlers.rs`
- `src/proxy/mod.rs`

Required:

- Use `metrics` + `metrics-exporter-prometheus`.
- Do not hand-render `brigto_router_*` metrics in handler.
- Keep metric names from `src/metrics.rs`: `router_requests_total`, `router_tokens_total`, `router_ttfb_seconds`, `router_overhead_seconds`, `router_backend_inflight`, `router_budget_remaining`, `router_circuit_open`.
- `x-router-overhead-ms` must not be backend total time. Current `ttfb_ms = total_ms` and header `x-router-overhead-ms = total_ms` are misleading.

Acceptance:

- `/metrics` contains the locked names.
- Non-stream and stream tests assert `x-router-request-id`, `x-router-backend`, `x-router-overhead-ms`.
- A delayed backend test proves overhead header stays small while total backend time is large.

### 11. Admin is background, but secure

Files:

- `src/admin/mod.rs`
- `src/main.rs`
- `src/handlers.rs`

Current bad evidence:

- admin auth reads spoofable `x-forwarded-for` / `x-real-ip` from client.
- admin router is not nested by main router.

Required:

- Nest `/admin/*`.
- Serve static portal if still in v1 scope.
- Use peer socket addr for IP allowlist.
- Trust XFF only if peer is a configured trusted proxy CIDR.
- Admin DB access is allowed because admin is not hot path.

Acceptance:

- Test direct request with forged `X-Forwarded-For: 127.0.0.1` from non-allowed peer is rejected.
- Test admin create key, then config reload makes key usable.

## Production Profiles

### FastestInternal

Use this as default:

- PostgreSQL: yes, background only.
- Redis: no.
- Budget/RPM/concurrency: RAM atomic per instance.
- Ledger: async background, fallback writer.
- Routing: least-load by local inflight/weight.
- Claim allowed: fastest candidate after direct-vs-router benchmark.

### StrictGlobalQuota

Use only if business requires hard global quota:

- PostgreSQL: same as above.
- Redis: yes, only budget/RPM/concurrency reservation.
- One Lua round trip for `check_and_reserve`.
- Commit/refund after response.
- No Redis for routing, health, config, ledger, metrics.

Do not mix these profiles accidentally. The fastest profile must not pay Redis cost.

## Final Gate Before Claiming SOTA

Code correctness gate:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Migration gate:

```bash
python3 - <<'PY'
import sqlite3, pathlib
sql = pathlib.Path("migrations/0001_init.sql").read_text()
con = sqlite3.connect(":memory:")
con.executescript(sql)
print("migration ok")
PY
```

Latency gate:

```text
Payloads: 1K, 50K, 200K token equivalent.
Modes: stream and non-stream.
Concurrency: 1, 50, 200.
Compare: direct backend vs router.
Pass: TTFB delta p99 < 3 ms at 200K stream.
Pass: total-time delta < 1%.
Pass: router overhead p99 < 2 ms or measured and explained with hard evidence.
```

Do not write “fastest in the world” in README until the latency gate has raw results checked into `bench/results/` or equivalent.

## Advisor Note

The fastest architecture here is not a bigger architecture. It is a stricter smaller one:

```text
ArcSwap config + RAM atomic guards + shared client pool + streaming proxy + background ledger
```

Everything else must prove it belongs before entering production path.
