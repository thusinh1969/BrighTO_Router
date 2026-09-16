# CODEX current frontier after runtime fixes — 2026-09-16

Scope: current worktree after DeepSeek's latest runtime/schema edits. I did not edit production code.

## Verdict

Progress is real, but the repo is currently gate-red and production-smoke-red.

Do not claim SOTA or production yet. First restore local gates, then make the DB layer production-clean.

## Current verified gate status

Commands run:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test config::tests::snapshot_picks_up_budget_change_within_poll_interval -- --nocapture
cargo build --locked
# clean pgvector/pgvector:pg16 + sqlx migrate run + target/debug/brigto-router boot
```

Results:

- `cargo check --all-targets`: PASS.
- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --all-targets -- -D warnings`: FAIL.
- Focused config test: FAIL.
- `cargo build --locked`: PASS.
- Clean Postgres `sqlx migrate run`: PASS.
- Router boot against that migrated Postgres DB: FAIL at `usage_ledger` aggregate decode through `sqlx::Any`.

## Positive changes now present — keep these

These previous blockers are partially or fully fixed in current source:

- Root `migrations/0001_init.sql` is now Postgres DDL; clean Postgres migration applies.
- `src/main.rs:70-86` wires boot runtime state and seeds usage before publishing snapshot.
- `src/main.rs:93-96` wires reload runtime state before publishing snapshot.
- `src/main.rs:140-173` starts graceful shutdown immediately; no pre-shutdown sleep.
- `src/proxy/mod.rs:484-500` replaced unbounded stream relay with bounded `mpsc::channel(..., 1)` and `send(...).await`.
- `src/proxy/mod.rs:253-264`, `:280`, `:310`, `:330-334` pass estimated input tokens and avoid committing/ledgering `0 + 0` when usage is missing.
- `src/route/mod.rs:112-133` now attempts `fallback_backend_id` after primary exhaustion.
- `src/budget/mod.rs:267-283` added `seed_usage`.
- Root `Cargo.toml` removed the direct `redis` dependency.

## P0 gate-red — clippy failure in route fallback helper

Current clippy output:

```text
error: this expression creates a reference which is immediately dereferenced by the compiler
   --> src/route/mod.rs:142:58
    |
142 |             let candidate = self.choose_candidate(route, &excluded)?;
    |                                                          ^^^^^^^^^ help: change this to: `excluded`
```

Exact fix:

```rust
let candidate = self.choose_candidate(route, excluded)?;
```

Then re-run:

```bash
cargo clippy --all-targets -- -D warnings
```

## P0 test-red — config test still uses SQLite integer flags while loader expects bool

Focused command:

```bash
cargo test config::tests::snapshot_picks_up_budget_change_within_poll_interval -- --nocapture
```

Observed output:

```text
WARN config: reload failed: error occurred while decoding column 3: mismatched types; Rust type `bool` is not compatible with SQL type `BIGINT`
thread 'config::tests::snapshot_picks_up_budget_change_within_poll_interval' panicked at src/config/mod.rs:395:18:
called `Option::unwrap()` on a `None` value
```

Evidence:

- `src/config/mod.rs:86`, `:147`, `:181` use `g_bool` for enabled columns.
- `src/config/mod.rs:246-248` defines `g_bool` as `row.try_get::<bool, _>(idx)` only.
- `src/config/mod.rs:314-367` test schema still uses `enabled INTEGER NOT NULL`.
- `src/config/mod.rs:377-379` inserts `enabled = 1`.

Exact bridge fix if SQLite tests remain:

```rust
fn g_bool(row: &AnyRow, idx: usize) -> anyhow::Result<bool> {
    if let Ok(v) = row.try_get::<bool, _>(idx) {
        return Ok(v);
    }
    Ok(row.try_get::<i64, _>(idx)? != 0)
}
```

This is acceptable only as a bridge while `sqlx::Any` still exists. The production direction remains Postgres-only.

## P0 production-smoke-red — router boot still fails after clean Postgres migration

Command shape used:

```bash
cargo build --locked
# start clean pgvector/pgvector:pg16 container
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:<port>/llm_router sqlx migrate run
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:<port>/llm_router \
  ADMIN_MASTER_KEY=adminkey \
  ADMIN_ALLOW_CIDR=127.0.0.1/32 \
  LISTEN_ADDR=127.0.0.1:18101 \
  CONFIG_POLL_SECS=3600 \
  RUST_LOG=error \
  timeout 5s target/debug/brigto-router
```

Observed output:

```text
Error: load usage_ledger boot counter

Caused by:
    0: error occurred while decoding column coalesce: error in Any driver mapping: Any driver does not support the Postgres type PgTypeInfo(Numeric)
    1: error in Any driver mapping: Any driver does not support the Postgres type PgTypeInfo(Numeric)
    2: Any driver does not support the Postgres type PgTypeInfo(Numeric)
```

Evidence:

- `src/config/mod.rs:53-54` still uses:

```rust
"SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0) FROM usage_ledger"
```

Postgres `SUM(BIGINT)` returns `NUMERIC`. `sqlx::Any` cannot decode that into `i64` here.

Exact bridge fix:

```sql
SELECT
  COUNT(*)::BIGINT,
  COALESCE(SUM(input_tokens), 0)::BIGINT,
  COALESCE(SUM(output_tokens), 0)::BIGINT
FROM usage_ledger
```

Also fix the new seed queries in `src/ledger/mod.rs:305-361`; each `COALESCE(SUM(input_tokens + output_tokens), 0)` needs `::BIGINT` while `Any` remains.

## P0 production DB layer still not clean

Even after the bridge casts, production admin/ledger paths remain risky because they still use `sqlx::Any` and `?` placeholders.

Evidence:

- `src/main.rs:55-58` uses `sqlx::any::AnyPoolOptions`.
- `src/config/mod.rs:6-7`, `:13-19`, `:228-229`, `:304-308` still use `AnyRow`, `Any`, `Pool<Any>`, and `AnyPoolOptions`.
- `src/admin/mod.rs:22`, `:33`, `:57-63`, `:328-329`, `:369-372`, `:402`, `:445`, `:466` still use `AnyPool`, `sqlx::Any`, `QueryBuilder::<sqlx::Any>`, or `?` placeholders.
- `src/ledger/mod.rs:8`, `:74`, `:166-172`, `:189-194`, `:274`, `:294-361` still use `AnyPool`, `QueryBuilder::<sqlx::Any>`, or `?` placeholders.

Correct production fix:

- Convert production DB modules to Postgres types: `PgPool` / `Pool<Postgres>`.
- Convert dynamic builders to `QueryBuilder::<sqlx::Postgres>`.
- Convert raw bind SQL to `$1`, `$2`, ... placeholders.
- Keep SQLite only in isolated tests if useful.
- Do not add a repository abstraction. DB is outside request forwarding path; typed Postgres is simpler and safer.

## P0 budget seed correctness is not proven yet

The shape is better now, but not production-proven.

Evidence:

- `src/main.rs:75-84` calls `ledger::load_usage_seeds(&cfg_pool)` and `budget.seed_usage(seed)` before listen.
- `src/ledger/mod.rs:294-379` loads day and month seeds for key/team and model scopes.
- The seed queries still use `?` placeholders and uncast Postgres aggregates while `Any` remains.
- There is no visible test proving “ledger has current-month usage at cap -> after boot/seed next request is rejected.”

Acceptance needed:

- Fix Postgres boot first.
- Add a budget restart/seed test with current-period usage at cap and previous-period usage ignored.
- Keep `rg -n "usage_ledger" src/handlers.rs src/proxy/mod.rs` empty.

## P1 fallback behavior needs tests

The code now attempts fallback, but tests have not been added yet.

Evidence:

- `src/route/mod.rs:112-133` has fallback logic.
- `src/route/mod.rs:463-523` tests only least-load, max-inflight, no healthy backend, and circuit behavior. No fallback tests are present.

Required tests:

- `fallback_not_used_while_primary_available`.
- `fallback_used_after_primary_exhausted`.
- `acquire_excluding_skips_tried_backend`.
- `fallback_not_retried_if_already_excluded`.

## P1 ledger drop visibility still has one silent branch

Evidence:

`src/ledger/mod.rs:38-40` still has:

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

## P1 Redis/Valkey cleanup is partial

Root `Cargo.toml` removed `redis`, good. Remaining default-footprint evidence:

- `docker-compose.yml:9` still sets `REDIS_URL`.
- `docker-compose.yml:17` still depends on `valkey`.
- `docker-compose.yml:35-40` still starts Valkey.
- `Makefile:46` still says dev stack includes Valkey.
- `install/docker-compose.yml` and `install/router-setup/docker-compose.yml` still include Valkey.
- `install/Cargo.toml` and `install/router-setup/Cargo.toml` still include the old Redis dependency.
- install docs still tell users to configure Redis.

Fix:

- Remove Valkey/Redis from default compose/install artifacts.
- Keep only a short ADR/doc note: Redis is reserved for strict global quota mode if that product requirement appears later.

## P1 benchmark evidence still insufficient

Evidence:

- `Makefile:37-38` still calls `bench/run.sh`.
- Root repo still has no `bench/run.sh`.
- `scripts/bench_smoke.py` exists, but it is labeled smoke only and does not run payload/concurrency matrix.

Fix:

- Put the maintained benchmark script at root `bench/run.sh`, or change `Makefile` to call a maintained benchmark command.
- Save raw results under `bench/results/<timestamp>/`.
- Required before SOTA claim: router-vs-direct TTFB/p99 delta on 1K/50K/200K payloads, concurrency 1/50/200, plus slow-client memory stability.

## Next exact patch sequence

1. Fix `src/route/mod.rs:142` clippy: pass `excluded`, not `&excluded`.
2. Fix `g_bool` bridge or move config tests to bool schema.
3. Cast config and ledger seed aggregates to `::BIGINT` while `Any` remains, or preferably complete the Postgres-only DB conversion now.
4. Run local gates:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

5. Run production Postgres smoke: migration, boot, `/healthz`, admin create team, ledger insert/replay path.
6. Then continue fallback tests, ledger drop branch, Valkey cleanup, and root benchmark artifact.
