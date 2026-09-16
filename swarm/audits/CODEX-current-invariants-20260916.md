# CODEX current invariant audit — 2026-09-16 18:24

Purpose: current-state audit after recent mechanical fixes. This supersedes the stale clippy part of `GLM-pass1.md`; architecture findings still stand.

## Gate

Command:

```bash
docker run --rm -v "$PWD":/app -v brigto-cargo-registry:/usr/local/cargo/registry \
  -v brigto-rustup:/usr/local/rustup -w /app rust:1.98.1-bookworm bash -c \
  "cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets"
```

Current result:

- `fmt`: PASS
- `clippy -D warnings`: PASS
- `test --all-targets`: FAIL
- failing test: `ledger::tests::db_down_writes_file_replay_on_reconnect`
- output: `ledger replay failed: error returned from database: (code: 1) no such table: usage_ledger` and `Error: No such file or directory (os error 2)`

## SOTA Hot Path Invariants

### P0: migration is still not executable SQL

Evidence:

```bash
python3 - <<'PY'
import sqlite3, pathlib
sql = pathlib.Path("migrations/0001_init.sql").read_text()
con = sqlite3.connect(":memory:")
con.executescript(sql)
PY
```

Current output: `OperationalError: near "```sql"`.

Current file evidence:

- `migrations/0001_init.sql:1` is ` ```sql `
- `migrations/0001_init.sql:88` is closing ` ``` `

Fix once:

- Remove both fences.
- Align `format`, `first_byte_timeout`, and `key_hash` representation in the same patch.

### P0: backend pool still cannot route real traffic

Evidence:

- `src/main.rs:74-75` creates empty `RamBudgetStore` and empty `RamBackendPool`.
- `src/route/mod.rs:67-87` has `upsert_backend`.
- Runtime code does not call `upsert_backend`; only tests do.

Verifier:

```bash
rg -n "upsert_backend" src
```

Expected after fix: at least one runtime call from bootstrap/config reload, not only tests.

Fix once:

- Build `Arc<RamBackendPool>` as concrete state.
- After first snapshot, call `upsert_backend` for every backend.
- On each successful config reload, sync backend states again.

### P0: team budget and key/backend concurrency are still not active

Evidence:

- `src/budget/mod.rs:100-152` defines `load_teams`, `try_acquire_concurrency`, `release_concurrency`.
- `src/route/mod.rs:89-106` defines `inc_inflight`, `dec_inflight`.
- `src/contract.rs:148-152` hides stores behind traits that do not expose these methods.
- Runtime code does not call these methods.

Verifier:

```bash
rg -n "load_teams|try_acquire_concurrency|release_concurrency|inc_inflight|dec_inflight" src
```

Expected after fix: runtime calls plus tests.

Fix once:

- Add `BudgetReservation`, `ConcurrencyGuard`, and `BackendLease`.
- Guards release on `Drop`.
- Do not scatter manual cleanup after awaits.

### P0: streaming is still buffered in handler

Evidence:

- `src/handlers.rs:282` calls local `forward_to_backend`.
- `src/handlers.rs:288` calls `resp.bytes().await`.
- `src/handlers.rs:346` returns `Body::from(upstream_body)`.
- `src/proxy/mod.rs:536` has `proxy_forward`, but handler does not call it.

Verifier:

```bash
rg -n "forward_to_backend|resp\\.bytes\\(\\)\\.await|Body::from\\(upstream_body\\)|proxy_forward" src/handlers.rs src/proxy/mod.rs
```

Expected after fix:

- no local handler forwarding
- no stream buffering in handler
- handler calls proxy
- stream body comes from `Body::from_stream`

Fix once:

- Make handler thin.
- Put all forwarding/splice/usage tap/retry logic in `proxy/mod.rs`.

### P0: budget is still check-then-commit

Evidence:

- `src/contract.rs:135` returns `Decision` from `try_reserve`, no reservation handle.
- `src/budget/mod.rs:245-263` checks and returns `Allow`.
- `src/budget/mod.rs:266-282` commits after upstream.

Race:

Two concurrent requests can both see remaining budget before either commits.

Fix once:

- `reserve(est_tokens)` atomically CAS-adds estimate for every applicable scope.
- Return `BudgetReservation`.
- `commit(actual_tokens)` adjusts delta/refund.
- Drop without commit refunds estimate.

Required tests:

- budget 100, concurrent reserve 80 + 80 -> exactly one succeeds.
- reserve 100, commit 30 -> 70 available.
- reserve 30, commit 50 -> total used is 50.

### P0: ledger still silently drops if channel full

Evidence:

- `src/handlers.rs:338` ignores `try_send`.
- `src/proxy/mod.rs:324` ignores `try_send`.
- `src/proxy/mod.rs:375` ignores `try_send`.

Fix once:

- Replace raw `mpsc::Sender<UsageEvent>` in app/proxy with `LedgerSink`.
- `LedgerSink::try_record` must route full-channel events to fallback writer path or overflow queue.
- Add metric for fallback/overflow.

Required test:

- fill ledger primary queue, send one more event, prove it is persisted to fallback/overflow and not dropped.

### P1: replay `.replay` file can strand events

Evidence:

- `src/ledger/mod.rs:191-220` only checks original fallback path.
- It renames fallback to `.replay`, then removes `.replay` only at the end.
- If crash happens after rename, next start ignores `.replay`.

Fix once:

- On startup/replay, process both fallback and `.replay`.
- If replay fails, preserve pending file for next attempt.
- Add unique index on `usage_ledger(request_id)`.

### P1: backend key resolution is inconsistent

Evidence:

- `src/config/mod.rs` accepts `env:` / `file:`.
- `src/handlers.rs:277` calls `std::env::var(&backend.api_key_ref)` directly.
- `src/proxy/mod.rs:56-72` has a separate resolver that does not support the same prefixes.

Fix once:

- One public resolver.
- Prefer resolving backend secret into runtime snapshot at config load, so request path does not read env/disk.

### P1: admin IP allowlist is spoofable once admin is wired

Evidence:

- `src/admin/mod.rs:118-129` trusts client-supplied `x-forwarded-for` / `x-real-ip`.
- `src/admin/mod.rs:187` uses that value for allowlist.

Fix once:

- Use peer socket address.
- Trust forwarded headers only when peer is a configured trusted proxy.

## Current Non-Issues

- `cargo fmt` and `clippy -D warnings` are currently clean.
- Do not spend another fix round on the old clippy errors from `GLM-pass1.md`; that section is stale.
- Redis is not required to fix any current P0 in fastest profile.

## Minimal Next Patch Shape

Do this in one integrated patch, not scattered micro-patches:

1. Valid migration + schema/loader/admin alignment.
2. Concrete `AppState` with shared `reqwest::Client`, `RamBudgetStore`, `RamBackendPool`, `LedgerSink`.
3. Bootstrap sync for backends/team budgets and reload sync.
4. Handler thin pipeline calling proxy.
5. RAM reservation + RAII guards.
6. LedgerSink no-drop semantics and replay repair.
7. Tests for migration, concurrency guard, reservation race, stream first chunk, ledger full-channel fallback.

This keeps architecture small and production-ready without adding Redis to the fastest path.
