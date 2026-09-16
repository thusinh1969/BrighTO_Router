# CODEX CURRENT — Postgres-only gates green, admin PATCH red

Time: 2026-09-16 19:28:29 +07. Scope: current worktree after DeepSeek's Postgres-only conversion. I did not edit `src/`; audit handoff only.

## Verdict

Big progress: root cause DB direction is now mostly correct.

Verified on current snapshot:

- Root `Cargo.toml` has no Redis dependency and no `sqlx any/sqlite` features.
- `cargo check --all-targets`: PASS.
- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --all-targets -- -D warnings`: PASS.
- Postgres-backed `cargo test --all-targets --no-fail-fast` against fresh `pgvector/pgvector:pg16`: PASS.
- `cargo build --release --locked`: PASS in previous smoke after conversion.
- Clean `sqlx migrate run`: PASS.
- Release router boots on clean Postgres and listens.
- `/healthz`: PASS.
- `/admin/usage`: PASS.
- Real proxy path through mock backend: PASS for non-stream, stream with usage, stream without usage.
- Ledger rows persisted correctly for all 3 proxy cases.

Still not production-complete: admin PATCH has a real SQL generation bug, install artifacts still advertise Redis/Valkey by default, and benchmark matrix is not yet a full SOTA proof.

## Evidence: local and Postgres test gates

Commands run:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

All passed.

Postgres test command:

```bash
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:<port>/llm_router   cargo test --all-targets --no-fail-fast
```

Result against fresh `pgvector/pgvector:pg16`:

- lib tests: 38/38 PASS.
- `tests/streaming_integration.rs`: 2/2 PASS.

## Evidence: production release smoke

Procedure:

1. Start fresh `pgvector/pgvector:pg16`.
2. `sqlx migrate run`.
3. Seed one backend, one model route, one team, one API key.
4. Boot `target/release/brigto-router`.
5. Call health/admin/proxy paths through a local mock OpenAI-compatible backend.

Boot evidence:

```text
INFO config: usage_ledger boot counter rows=0 input=0 output=0
seeded budget counters from ledger seeds=0
brigto-router listening addr=127.0.0.1:<port>
```

HTTP evidence:

```text
GET /healthz -> 200 ok
GET /admin/usage -> 200 []
POST /v1/chat/completions non-stream -> 200
POST /v1/chat/completions stream with usage -> 200
POST /v1/chat/completions stream without usage -> 200
```

Ledger verification from Postgres:

```text
request_id     | status | input_tokens | output_tokens | estimated | stream
---------------+--------+--------------+---------------+-----------+--------
...            | 200    | 7            | 9             | f         | f
...            | 200    | 11           | 22            | f         | t
...            | 200    | 24           | 0             | t         | t
```

This proves the previous production boot blocker and stream-no-usage zero-token blocker are fixed on current release binary.

## P0/P1 remaining: admin PATCH SQL bug

Failing smoke call:

```text
PATCH /admin/teams/1 {"name":"team-renamed"} -> 500
error returned from database: syntax error at or near ","
```

Root cause in current code:

- `src/admin/mod.rs:402`: `let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE teams SET ");`
- `src/admin/mod.rs:403`: `let mut separated = builder.separated(", ");`
- `src/admin/mod.rs:405-422`: fields are pushed through `separated`.
- `src/admin/mod.rs:425`: `separated.push(" WHERE id = ").push_bind(id);`

That line is wrong. `separated.push(...)` inserts the separator before every new segment after the first assignment, so a non-empty patch generates SQL shaped like:

```sql
UPDATE teams SET name = $1,  WHERE id = $2
```

Exact fix:

```rust
let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE teams SET ");
let mut has_field = false;
{
    let mut separated = builder.separated(", ");
    if let Some(name) = payload.name {
        has_field = true;
        separated.push("name = ").push_bind(name);
    }
    if let Some(budget) = payload.budget {
        has_field = true;
        match budget {
            Some(b) => {
                let json = serde_json::to_string(&b)?;
                separated.push("budget = ").push_bind(json);
            }
            None => {
                separated.push("budget = NULL");
            }
        }
    }
    if let Some(enabled) = payload.enabled {
        has_field = true;
        separated.push("enabled = ").push_bind(enabled);
    }
}
if !has_field {
    return Err(ApiError::new(StatusCode::BAD_REQUEST, "no fields to update"));
}
builder.push(" WHERE id = ").push_bind(id);
```

Minimum tests to add:

1. PATCH team name only returns 200 and changes row.
2. PATCH budget null returns 200 and sets `budget IS NULL`.
3. PATCH empty JSON returns 400, not SQL 500.

Do not solve this with string concatenation or a generic admin query framework; the scoped `separated` fix is enough.

## Smoke setup note: duplicate admin create errors were not code evidence

In the smoke, I manually seeded `teams(id=1)` and `api_keys(id=1)`. That does not advance Postgres sequences. Therefore these two errors are caused by the smoke setup, not proven admin bugs:

```text
POST /admin/teams -> 500 duplicate key value violates unique constraint "teams_pkey"
POST /admin/keys -> 500 duplicate key value violates unique constraint "api_keys_pkey"
```

Next admin smoke should seed with default IDs or run `setval(...)` before testing admin create. Do not chase duplicate-key here unless it reproduces on an empty migrated DB or after sequence is advanced.

## Benchmark status

Root `bench/run.sh:19` jq expression is now syntactically valid. Verified with sample oha JSON:

```text
rps 123.46  p50 1ms  p99 3ms  ok 10  non200 2
```

Remaining benchmark blocker for SOTA claim:

- Root `bench/run.sh:8` still uses one `CONC` per run; official proof should cover `1 50 200` in one result set or clearly require three saved runs.
- It prints direct/router separately but does not compute pass/fail deltas for p99 and TTFB. For review, add a summary file comparing router-direct overhead.

## Install/packaging cleanup still open

Root compose is aligned with fastest profile, but install copies still include Redis/Valkey:

- `install/docker-compose.yml:9,17,35-40`
- `install/router-setup/docker-compose.yml:9,17,35-40`
- `install/README.md:70,99,101`
- `install/router-setup/README-setup.md:54`
- `install/Cargo.toml:34`
- `install/router-setup/Cargo.toml:34`
- `install/router-setup/bench/run.sh:19` still has the old broken jq expression.

Fix install artifacts or explicitly mark them obsolete. Production docs must not tell users to deploy Redis/Valkey by default.

## Next exact patch order

1. Fix `src/admin/mod.rs:425` by ending the `separated` scope before appending `WHERE`; add empty-patch 400 guard.
2. Add/adjust admin PATCH tests for name-only, budget-null, empty patch.
3. Re-run `cargo check`, `fmt --check`, `clippy -D warnings`, and Postgres-backed `cargo test --all-targets --no-fail-fast`.
4. Re-run release smoke with sequence-safe admin create/key create and PATCH.
5. Clean install artifacts or mark them obsolete.
6. Extend benchmark output to include concurrency matrix and router-direct deltas.


## UPDATE 2026-09-16 19:29:07 +07 — contract field added, callsites red

DeepSeek added `reload_notify` to `AppState` in `src/contract.rs`. Current snapshot is no longer compile-green.

Current `cargo check --all-targets` failure:

```text
error[E0063]: missing field `reload_notify` in initializer of `AppState`
tests/streaming_integration.rs:113:26
src/main.rs:136:30
```

Exact fix:

1. In `src/main.rs`, create one `Arc<tokio::sync::Notify>` before `AppState` construction, pass it into `AppState { reload_notify, ... }`, and if the intention is admin-triggered reload, wire the same notify into the reload task. Do not create per-request notifiers.
2. In `tests/streaming_integration.rs`, add `reload_notify: Arc::new(tokio::sync::Notify::new())` to the test `AppState` initializer.
3. Re-run `cargo check --all-targets` before touching runtime logic.

Still-open from previous section: `src/admin/mod.rs:425` uses `separated.push(" WHERE id = ")`, which generated `UPDATE teams SET name = $1, WHERE id = $2` in release smoke. Fix that after compile is restored.
