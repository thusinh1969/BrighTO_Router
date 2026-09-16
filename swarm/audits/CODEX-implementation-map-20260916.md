# CODEX implementation map — fix once, keep fastest profile small

Timestamp: 2026-09-16 18:30 Asia/Ho_Chi_Minh.

This is a coder-facing map. It does not replace `CODEX-SOTA-fastest-contract.md`; it turns that architecture verdict into concrete file edits and tests.

## Current Verified State

Fresh invariant check:

```text
src/handlers.rs: buffers_stream, full_request_value_parse, client_per_request, raw_ledger_sender_error_ignored
src/proxy/mod.rs: client_per_request, raw_ledger_sender_error_ignored
src/contract.rs: raw_ledger_sender_in_state, budget_no_reservation
migrations/0001_init.sql: markdown_fence, open_ai_enum_mismatch, first_byte_timeout_secs_name, key_hash_blob
```

Current gate from the previous run:

```text
fmt: pass
clippy -D warnings: pass
cargo test --all-targets: fail 1 test, ledger replay
```

Do not spend time on old clippy findings. They are stale.

## Target Module Shape

### `src/contract.rs`

Make `AppState` production-shaped, not half-trait-shaped.

Recommended fields:

```rust
pub struct AppState {
    pub cfg: Arc<ArcSwap<ConfigSnapshot>>,
    pub budget: Arc<RamBudgetStore>,
    pub backends: Arc<RamBackendPool>,
    pub client: reqwest::Client,
    pub ledger: LedgerSink,
    pub max_body_bytes: usize,
}
```

If keeping traits for tests, traits must expose the same behavior:

```rust
fn reserve(&self, key: &ApiKey, model: &str, est_tokens: u64) -> Result<BudgetReservation, Decision>;
fn acquire_concurrency(&self, key: &ApiKey) -> Result<ConcurrencyGuard, Decision>;
fn acquire(&self, route: &ModelRoute) -> Result<BackendLease, NoBackend>;
fn record(&self, event: UsageEvent);
```

Hard rule: handler/proxy must not hold raw `mpsc::Sender<UsageEvent>`.

### `src/budget/mod.rs`

Add these public types:

```rust
pub struct BudgetReservation { ... }
pub struct ConcurrencyGuard { ... }
```

Required semantics:

- `reserve` atomically CAS-adds estimated tokens into every active scope.
- If one scope fails, rollback prior scopes before returning `BudgetExceeded`.
- `BudgetReservation::commit(actual)` adjusts delta/refund.
- `Drop for BudgetReservation` refunds estimate if not committed.
- `acquire_concurrency` returns guard or rate/limit decision.
- `Drop for ConcurrencyGuard` releases exactly once.

Scopes:

- key total when key budget exists.
- key model when key budget has `per_model[model]`.
- team total when no key budget and team budget exists.
- team model when team budget has `per_model[model]`.

Do not use Redis in this implementation. This is the `FastestInternal` profile.

Tests to add in this file:

```text
concurrent_reserve_allows_only_one_when_budget_would_be_exceeded
reservation_refunds_on_drop
reservation_commit_refunds_unused_estimate
reservation_commit_accounts_actual_over_estimate
concurrency_guard_releases_on_drop
```

### `src/route/mod.rs`

Add:

```rust
pub struct BackendLease { ... }
```

Required semantics:

- `acquire(route)` calls existing pick logic.
- On success, increments backend inflight before returning lease.
- `Drop for BackendLease` decrements inflight exactly once.
- Lease stores backend id and can expose `backend_id()`.
- Circuit result remains explicit: proxy calls `note_result(backend_id, ok)` once per attempt.

Do not add vLLM metrics scheduling yet. Local `inflight / weight` is the correct first production scheduler.

Tests to add:

```text
backend_lease_increments_and_drops_inflight
backend_lease_respects_max_inflight
backend_lease_half_open_allows_single_probe
```

### `src/ledger/mod.rs`

Add:

```rust
#[derive(Clone)]
pub struct LedgerSink { ... }
```

Required API:

```rust
impl LedgerSink {
    pub fn new(primary: mpsc::Sender<UsageEvent>, fallback: FallbackHandle) -> Self;
    pub fn try_record(&self, ev: UsageEvent);
}
```

`try_record` must not block request latency. If primary channel is full, send to a fallback writer path or overflow queue. Do not silently drop.

Replay fix:

- Process both `ledger_fallback.jsonl` and `ledger_fallback.replay`.
- If replay fails, keep the pending file for the next attempt.
- Add unique index on `usage_ledger(request_id)`.

Tests to add:

```text
try_record_full_primary_is_not_dropped
replay_handles_existing_replay_file
replay_preserves_pending_file_on_insert_error
```

Keep PostgreSQL writes in the writer task only.

### `src/proxy/mod.rs`

Keep proxy as the only forwarding owner.

Required changes:

- Remove per-request `Client::builder()` / `Client::new()`.
- Use shared client from `AppState`.
- Do not use raw ledger sender; use `LedgerSink`.
- Stream response with `Body::from_stream`.
- Keep existing `memmem` usage tap.
- Keep OpenAI byte-splice for missing `stream_options`.
- Use `route.first_byte_timeout`, not hardcoded 180s.
- Retry only before the first byte is sent.
- Do not treat `RouteDecision::Skip` as a backend to try.

Current specific bug:

```rust
RouteDecision::Skip { backend_id, .. } => backend_id
```

That is wrong. Skip means do not use that backend. In the current trait impl `pick` does not return `Skip`, so the branch should be unreachable or deleted.

Tests to add:

```text
stream_returns_first_chunk_before_backend_completes
stream_splices_include_usage_only_when_needed
stream_tap_records_usage_from_final_usage_chunk
retry_happens_before_first_byte_only
no_retry_after_first_byte
```

### `src/handlers.rs`

Make it thin. Handler responsibilities:

1. Extract API key from header.
2. Hash key via `auth::hash_key`.
3. Load `cfg` snapshot.
4. Read body `Bytes` with max limit.
5. Parse only request head:

```rust
#[derive(Deserialize)]
struct RequestHead {
    model: String,
    #[serde(default)]
    stream: bool,
}
```

6. Authorize via `auth::authorize_detailed`.
7. Find route.
8. Reserve budget and concurrency.
9. Build `ProxyContext`.
10. Call `proxy::proxy_forward`.

Remove from handler:

- local `forward_to_backend`
- local `extract_usage`
- full `serde_json::Value` parse
- `reqwest::Client::new`
- `resp.bytes().await`
- manual ledger event creation for proxied success path

Handler can still create ledger event for early rejects if desired, but that must also go through `LedgerSink`.

Tests to add:

```text
wrong_key_rejected_before_body_is_read
wrong_model_returns_403
unknown_route_returns_404_or_503_consistently
handler_calls_proxy_stream_path_without_buffering
```

### `src/main.rs`

Startup sequence must be:

```text
install sqlx any drivers
connect config DB
load initial snapshot
create Arc<RamBudgetStore>
create Arc<RamBackendPool>
sync team budgets into budget store
sync backends into backend pool
build shared reqwest Client
create LedgerSink + spawn LedgerWriter
spawn config reload task that syncs snapshot, budget store, backend pool
start health loop
serve router
```

Important: do not cast stores to trait objects before sync. That is how current code lost required methods.

### `src/config/mod.rs`

Unify backend secret resolution:

```rust
pub fn resolve_backend_key_ref(ref_name: &str) -> Result<String>
```

Support exactly:

- `env:NAME`
- `file:/absolute/path`
- raw env var name as backwards-compatible shorthand

The fastest request path should not read env/disk. Resolve during config load or reload and store the resolved backend secret in runtime config. If the team does not want plaintext in `Backend`, create `ResolvedBackend` internal to runtime state. Do not resolve per request.

### `src/admin/mod.rs`

Admin is not hot path, so DB access is fine.

Required changes:

- Do not trust client-supplied `X-Forwarded-For` by default.
- Use `ConnectInfo<SocketAddr>` or equivalent peer address.
- Trust forwarded headers only when peer is a configured trusted proxy.
- Ensure generated key hash uses the same representation as migration/config loader.
- Trigger config reload after create/update/delete; 5s poll is fallback, not the only mechanism.

### `migrations/0001_init.sql`

Make this plain SQL and align with code.

Recommended schema choices:

```sql
format TEXT NOT NULL CHECK (format IN ('openai','anthropic'))
first_byte_timeout_secs INTEGER NOT NULL DEFAULT 180
key_hash TEXT NOT NULL
request_id TEXT NOT NULL UNIQUE
```

Then update loader to query `first_byte_timeout_secs` and read `key_hash` as hex text.

If keeping SQLite dev + Postgres prod in one migration is too awkward, split dialect-specific migrations or write SQL acceptable to SQLite for tests and document Postgres DDL separately. Do not leave Markdown fences inside `.sql`.

## Final Verification Commands

Run these after the patch:

```bash
python3 - <<'PY'
import sqlite3, pathlib
sql = pathlib.Path("migrations/0001_init.sql").read_text()
con = sqlite3.connect(":memory:")
con.executescript(sql)
print("migration ok")
PY
```

```bash
docker run --rm -v "$PWD":/app -v brigto-cargo-registry:/usr/local/cargo/registry \
  -v brigto-rustup:/usr/local/rustup -w /app rust:1.98.1-bookworm bash -c \
  "cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets"
```

Structural checks:

```bash
rg -n "serde_json::from_slice::<Value>|resp\\.bytes\\(\\)\\.await|Body::from\\(upstream_body\\)|reqwest::Client::new\\(|Client::builder\\(|try_send\\(event\\)" src
```

Expected after the fix:

- no handler full-body `Value` parse
- no buffered stream response
- no per-request reqwest client
- no ignored raw ledger `try_send`

## Keep Out Of Scope

Do not add these now:

- Redis default path.
- vLLM metrics scheduler.
- provider translation.
- semantic cache.
- prompt/response logging.
- complex frontend stack.
- DB-backed per-request budget check.

The fastest maintainable patch is smaller, not bigger.
