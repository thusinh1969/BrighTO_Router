# CODEX current production smoke RED — 2026-09-16

Scope: current worktree after DeepSeek's partial Postgres/shutdown/reload/Redis edits. I did not edit production code.

## Verdict

Current production Postgres smoke is still red.

Good: `migrations/0001_init.sql` now applies to clean Postgres.

Bad: the router built from current source does not boot against that migrated Postgres DB because `src/config/mod.rs` still uses `sqlx::Any` and decodes a Postgres `SUM(BIGINT)` aggregate as `i64`.

Also bad: Rust unit tests are red because `g_bool` now expects bool while the inline SQLite test schema still uses integer flags.

## Current verified commands

Rust gate run:

```bash
cargo check --all-targets && \
cargo fmt --all -- --check && \
cargo clippy --all-targets -- -D warnings && \
cargo test --all-targets
```

Result:

- `cargo check --all-targets`: PASS.
- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --all-targets -- -D warnings`: PASS.
- `cargo test --all-targets`: FAIL.

Failing unit test:

```text
config::tests::snapshot_picks_up_budget_change_within_poll_interval
WARN config: reload failed: error occurred while decoding column 3: mismatched types; Rust type `bool` is not compatible with SQL type `BIGINT`
thread 'config::tests::snapshot_picks_up_budget_change_within_poll_interval' panicked at src/config/mod.rs:395:18:
called `Option::unwrap()` on a `None` value
```

Production smoke run:

```bash
cargo build --locked
# start clean pgvector/pgvector:pg16 container
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:<port>/llm_router sqlx migrate run
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:<port>/llm_router \
  ADMIN_MASTER_KEY=adminkey \
  ADMIN_ALLOW_CIDR=127.0.0.1/32 \
  LISTEN_ADDR=127.0.0.1:18096 \
  CONFIG_POLL_SECS=3600 \
  RUST_LOG=error \
  target/debug/brigto-router
```

Observed migration result:

```text
Applied 1/migrate init (...ms)
```

Observed router boot result:

```text
Error: load usage_ledger boot counter

Caused by:
    0: error occurred while decoding column coalesce: error in Any driver mapping: Any driver does not support the Postgres type PgTypeInfo(Numeric)
    1: error in Any driver mapping: Any driver does not support the Postgres type PgTypeInfo(Numeric)
    2: Any driver does not support the Postgres type PgTypeInfo(Numeric)
```

## Exact current root causes

### A. Test gate red

- `src/config/mod.rs:86`, `:147`, `:181` use `g_bool`.
- `src/config/mod.rs:314-367` test schema still declares `enabled INTEGER` and `estimated/stream/client_aborted INTEGER`.
- `src/config/mod.rs:377-379` inserts `enabled = 1`.

Minimal immediate test fix:

```rust
fn g_bool(row: &AnyRow, idx: usize) -> anyhow::Result<bool> {
    if let Ok(v) = row.try_get::<bool, _>(idx) {
        return Ok(v);
    }
    Ok(row.try_get::<i64, _>(idx)? != 0)
}
```

This is only a bridge while `sqlx::Any` remains.

### B. Production boot red

- `src/config/mod.rs:53-54`:

```rust
"SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0) FROM usage_ledger"
```

Postgres `SUM(BIGINT)` returns `NUMERIC`; `sqlx::Any` does not decode that into `i64`.

Minimal immediate production boot fix while `Any` remains:

```sql
SELECT
  COUNT(*)::BIGINT,
  COALESCE(SUM(input_tokens), 0)::BIGINT,
  COALESCE(SUM(output_tokens), 0)::BIGINT
FROM usage_ledger
```

Also fix `src/ledger/mod.rs:295-297` similarly if `boot_counter` survives:

```sql
SELECT key_id, COALESCE(SUM(input_tokens + output_tokens), 0)::BIGINT AS total
FROM usage_ledger
GROUP BY key_id
```

### C. Production admin/ledger bind queries still risky/red after boot

Even after aggregate cast, these are still not production-proven:

- `src/admin/mod.rs:328-329` uses `VALUES (?, ?, ?) RETURNING id`.
- `src/admin/mod.rs:369-372` uses `VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id`.
- `src/admin/mod.rs:402`, `:466` use `QueryBuilder::<sqlx::Any>`.
- `src/admin/mod.rs:445` uses `WHERE id = ?`.
- `src/ledger/mod.rs:195` uses `QueryBuilder::<sqlx::Any>` for ledger batch insert.
- `src/ledger/mod.rs:275` uses `request_id = ?`.

Earlier live probe with a boot-compatible Postgres schema reached admin and returned:

```text
HTTP/1.1 500 Internal Server Error
error returned from database: syntax error at or near "," at line 1244
```

That came from Postgres seeing `?` placeholders. This is why `CODEX-postgres-only-db-layer-verdict-20260916.md` remains the right architecture direction.

## Correct fix direction

Do not add compatibility patches forever. The fastest maintainable production architecture should be Postgres-only for config/admin/ledger:

- Replace `Pool<Any>` / `AnyPool` / `AnyPoolOptions` with `PgPool` or `Pool<Postgres>` in production modules.
- Replace `QueryBuilder::<sqlx::Any>` with `QueryBuilder::<sqlx::Postgres>`.
- Use `$1`, `$2`, ... placeholders in raw bound queries.
- Keep SQLite only in isolated tests if still useful, or move config/admin/ledger tests to a Postgres container fixture.

This does not affect hot-path latency because DB is not on the request forwarding path.

## Immediate acceptance for next patch

First restore local gates:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Then prove current production DB:

```bash
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:<port>/llm_router sqlx migrate run
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:<port>/llm_router \
  ADMIN_MASTER_KEY=adminkey \
  ADMIN_ALLOW_CIDR=127.0.0.1/32 \
  LISTEN_ADDR=127.0.0.1:18096 \
  CONFIG_POLL_SECS=3600 \
  RUST_LOG=error \
  target/debug/brigto-router
curl -fsS http://127.0.0.1:18096/healthz
curl -fsS -X POST http://127.0.0.1:18096/admin/teams \
  -H 'authorization: Bearer adminkey' \
  -H 'content-type: application/json' \
  --data '{"name":"newteam","enabled":true}'
```

Expected: migration applies, router boots, healthz returns `ok`, admin create team returns 200 JSON.
