# CODEX current verdict — gates green, Postgres smoke red only at admin PATCH

Time: 2026-09-16 Asia/Ho_Chi_Minh.
Scope: current worktree after DeepSeek fixed `reload_notify` callsites.
Rule from `audits/README.md`: Codex does not edit `src/`; this file is audit/test evidence plus exact repair instruction.

## Verdict

Current source is **not production-green yet**.

The compile/test/build foundation is now green, and the Postgres-only runtime path is mostly correct. The remaining verified P0 runtime bug is `PATCH /admin/teams/{id}`. It returns 500 against real Postgres because the SQL builder emits a comma before `WHERE`.

Do not claim SOTA/production-ready until this PATCH bug is fixed and the smoke below passes end-to-end.

## Verified green gates

These commands were run on current worktree:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Result: all PASS.

Postgres-backed test gate was run against fresh `pgvector/pgvector:pg16` with clean migration:

```bash
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:55432/llm_router sqlx migrate run
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:55432/llm_router cargo test --all-targets --no-fail-fast
```

Result:

- migration: PASS
- lib tests: 38/38 PASS
- `tests/streaming_integration.rs`: 2/2 PASS

Release build:

```bash
cargo build --release --locked
```

Result: PASS.

## Verified production smoke on clean Postgres

Environment:

- DB: fresh `pgvector/pgvector:pg16`
- router binary: `target/release/brigto-router`
- backend: local mock OpenAI-compatible server
- config: backend + model route seeded in Postgres before boot
- team/key: created through admin API after boot, proving admin notify path updates runtime snapshot without waiting for long poll

Smoke results:

```text
PASS healthz
PASS admin_create_team status=200 body={'id': 1, 'name': 'smoke-team', 'budget': None, 'enabled': True}
PASS admin_create_key status=200 body={'id': 1, 'key': 'lc-...', 'prefix': 'lc-...'}
PASS proxy_nonstream_after_admin_reload status=200 usage prompt=7 completion=9
PASS proxy_stream status=200 usage prompt=11 completion=22
FAIL admin_patch_team_name status=500 body=error returned from database: syntax error at or near "," at line 1244
PASS ledger_rows after 2.5s flush wait: 2|1|18|31
```

Ledger note: an earlier check immediately after proxy returned zero rows because writer flushes by timer/batch; after a 2.5s wait it persisted 2 rows. That is expected for async ledger writer, not a current P0 bug.

## Exact root cause still in source

`src/admin/mod.rs` still has this shape:

```rust
let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE teams SET ");
let mut separated = builder.separated(", ");
// fields...
separated.push(" WHERE id = ").push_bind(id);
```

`Separated` inserts `, ` before every pushed segment after the first. If any field is updated, pushing `WHERE` through it builds invalid SQL:

```sql
UPDATE teams SET name = $1, WHERE id = $2
```

## Exact fix DeepSeek should apply

In `src/admin/mod.rs::update_team`:

1. Track whether at least one field is present.
2. Scope/drop `separated` before appending `WHERE`.
3. Return 400 for empty PATCH.

Patch shape:

```rust
let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE teams SET ");
let mut changed = 0usize;
{
    let mut separated = builder.separated(", ");

    if let Some(name) = payload.name {
        separated.push("name = ").push_bind(name);
        changed += 1;
    }

    if let Some(budget) = payload.budget {
        match budget {
            Some(b) => {
                let json = serde_json::to_string(&b)?;
                separated.push("budget = ").push_bind(json);
            }
            None => separated.push("budget = NULL"),
        };
        changed += 1;
    }

    if let Some(enabled) = payload.enabled {
        separated.push("enabled = ").push_bind(enabled);
        changed += 1;
    }
}

if changed == 0 {
    return Err(ApiError::new(StatusCode::BAD_REQUEST, "no fields to update"));
}

builder.push(" WHERE id = ").push_bind(id);
```

Existing `ApiError::new` and `StatusCode` are already in this module, so no new abstraction is needed.

## Required verification after the patch

Run:

```bash
cargo fmt --all
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
DATABASE_URL=postgres://... cargo test --all-targets --no-fail-fast
cargo build --release --locked
```

Then re-run production smoke and require:

```text
PATCH /admin/teams/{id} {"name":"team-renamed"} -> 200
GET /admin/usage after ledger flush -> includes non-stream and stream rows
Proxy non-stream -> 200
Proxy stream -> 200 with usage tap
```

## Remaining non-P0 cleanup after PATCH is green

These do not block the immediate PATCH fix, but block a clean production/SOTA claim:

- `scripts/bench_smoke.py` is stale SQLite-era code. Replace it with Postgres-only smoke/bench or remove it from official proof path.
- `install/router-setup/bench/run.sh` still has broken jq `|.*100`; root `bench/run.sh` is already fixed.
- `install/` and `install/router-setup/` still mention/package Redis/Valkey. Keep Redis out of the default production install unless measured strict multi-instance quota requires it.
- Root benchmark matrix artifact is still not proven. Need router-vs-direct runs across realistic payload/concurrency before claiming “fastest”.
