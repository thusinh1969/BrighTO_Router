# CODEX current gate RED after partial Postgres patch — 2026-09-16

Scope: current worktree after DeepSeek's latest partial fixes. I did not edit `src/`.

## Verdict

Current Rust gate is red. Do not continue architecture cleanup until this is green again.

The partial Postgres patch fixed some real P0s but left the DB layer split-brained: root migration now uses Postgres booleans, while unit tests still create SQLite integer booleans and production code still uses `sqlx::Any`.

## What changed positively

Keep these fixes:

- `migrations/0001_init.sql` is now Postgres DDL instead of SQLite `PRAGMA`/`AUTOINCREMENT`.
- `src/main.rs:70-84` wires runtime stores before publishing the boot/reload snapshot.
- `src/main.rs:140-173` starts graceful shutdown immediately; the old `sleep(grace_period)` inside the shutdown future is gone.
- Root `Cargo.toml` removed the direct `redis` dependency.

## Current gate command and failure

Command run:

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

Failing test:

```text
---- config::tests::snapshot_picks_up_budget_change_within_poll_interval stdout ----
WARN config: reload failed: error occurred while decoding column 3: mismatched types; Rust type `bool` is not compatible with SQL type `BIGINT`: mismatched types; Rust type `bool` is not compatible with SQL type `BIGINT`
WARN config: reload failed: error occurred while decoding column 3: mismatched types; Rust type `bool` is not compatible with SQL type `BIGINT`: mismatched types; Rust type `bool` is not compatible with SQL type `BIGINT`

thread 'config::tests::snapshot_picks_up_budget_change_within_poll_interval' panicked at src/config/mod.rs:395:18:
called `Option::unwrap()` on a `None` value
```

## Root cause

- `src/config/mod.rs:86`, `:147`, and `:181` now use `g_bool(...)` for `enabled`.
- `src/config/mod.rs:318-344` test schema still creates `enabled INTEGER NOT NULL` for `backends`, `teams`, and `api_keys`.
- The poll test then fails to load any teams and panics at `src/config/mod.rs:395`.

## Minimal immediate fix to restore tests

If keeping SQLite tests temporarily, update the inline SQLite schema in `src/config/mod.rs` tests to use boolean-compatible declarations/values, or make `g_bool` tolerant of integer flags.

Recommended short-term helper if `sqlx::Any` stays temporarily:

```rust
fn g_bool(row: &AnyRow, idx: usize) -> anyhow::Result<bool> {
    if let Ok(v) = row.try_get::<bool, _>(idx) {
        return Ok(v);
    }
    Ok(row.try_get::<i64, _>(idx)? != 0)
}
```

This restores SQLite tests while Postgres migration uses `BOOLEAN`.

## Correct production fix remains stronger

This green fix is only a bridge. The production DB direction should still follow `CODEX-postgres-only-db-layer-verdict-20260916.md`:

- replace production `sqlx::Any` with `PgPool` / `Pool<Postgres>`;
- use `$1`, `$2` placeholders or `QueryBuilder<Postgres>`;
- cast Postgres aggregates, e.g. `COALESCE(SUM(input_tokens), 0)::BIGINT`;
- keep SQLite only as isolated test support if useful.

## Remaining verified blockers after this test fix

Still present in current source:

- `src/proxy/mod.rs:473` uses `mpsc::unbounded`.
- `src/proxy/mod.rs:487` uses `unbounded_send`.
- `src/proxy/mod.rs:503-511` can finish estimated usage with `0 + 0` tokens.
- `src/route/mod.rs:158-240` still ignores `fallback_backend_id` in candidate selection.
- `src/ledger/mod.rs:39-41` still silently ignores overflow failure when primary channel is closed.
- `src/ledger/mod.rs` and `src/admin/mod.rs` still use `sqlx::Any`/`AnyPool` and `?` placeholders.
- `docker-compose.yml` and install compose/docs still include Valkey/Redis even though root `Cargo.toml` removed the dependency.

## Acceptance for the next coder patch

Run:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Expected: all pass again before proceeding to benchmark or further refactors.
