# CODEX MOVING SNAPSHOT — route green, DB production still red

Time: 2026-09-16 19:12:52 +07. Scope: current worktree while DeepSeek is still coding. I did not edit `src/`; this file is an audit handoff only.

## Verdict

Current snapshot is **not production-ready** and must not be described as SOTA/fastest yet. Runtime routing work moved in the right direction: route fallback has concrete tests and passes. The remaining hard blocker is the DB architecture: production is still wired through `sqlx::Any` and the router cannot boot on a clean Postgres migration.

The fastest production architecture should stay simple:

- Hot path: RAM snapshot + RAM budget/concurrency + shared `reqwest::Client` + streaming tap. No DB, Redis, filesystem, env read, full body JSON parse, or per-request client construction.
- Control plane/background: Postgres only for config/admin/ledger. Use `PgPool`/Postgres placeholders/types in production code. Do not keep fixing `sqlx::Any` edge cases.
- Redis/Valkey: not in default fastest profile. Add only later behind an explicit feature/config if benchmarked multi-instance strict global quota proves it is required.

## Current gates I ran

### Rust gate

Command:

`cargo check --all-targets && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets --no-fail-fast`

Result:

- `cargo check --all-targets`: PASS.
- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --all-targets -- -D warnings`: PASS.
- lib tests: PASS, 38/38.
- integration tests: FAIL, 1/2.

Failing integration test:

`tests/streaming_integration.rs:90`

Output:

`stream_request_taps_usage_and_forwards_sse ... FAILED`

`called Result::unwrap() on an Err value: Database(SqliteError { code: 1, message: "table backends already exists" })`

Cause: `build_state` creates temp DB names with `process::id() + SystemTime::now().as_millis()` at `tests/streaming_integration.rs:51-58`. Two tokio tests can start in the same millisecond, connect to the same SQLite file, and both run `CREATE TABLE` at line 90. This is a test-isolation bug, not router runtime behavior.

Smallest fix: make DB path unique with an atomic counter, UUID, tempfile NamedTempFile/TempDir, or include test-specific suffix. Do not serialize all tests just to hide this; keep parallel tests useful.

### Route gate after DeepSeek patch

Command:

`cargo test route::tests -- --nocapture`

Result: PASS, 8/8.

New useful coverage present:

- `fallback_not_used_while_primary_available`
- `fallback_used_after_primary_exhausted`
- `acquire_excluding_skips_tried_backend`
- `fallback_not_retried_if_already_excluded`

Route verdict: this area is acceptable for this milestone. Do not churn it unless integration behavior reveals a real bug.

### Clean Postgres production smoke

Procedure:

1. `cargo build --locked`: PASS.
2. Start fresh `pgvector/pgvector:pg16`.
3. `DATABASE_URL=postgres://... sqlx migrate run`: PASS.
4. Boot `target/debug/brigto-router` against that clean Postgres.

Boot result: FAIL before listening.

Output:

`Error: load usage_ledger boot counter`

`Any driver does not support the Postgres type PgTypeInfo(Numeric)`

Direct cause: `src/config/mod.rs:53-55` still uses:

`SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0) FROM usage_ledger`

On Postgres, `SUM(BIGINT)` returns `NUMERIC`; `sqlx::Any` cannot decode that as `i64` at `src/config/mod.rs:59-63`.

Tactical patch if DeepSeek wants immediate boot: cast aggregates explicitly:

`COALESCE(SUM(input_tokens), 0)::BIGINT`

`COALESCE(SUM(output_tokens), 0)::BIGINT`

But that is only a bridge. The production architecture fix is below.

## P0 architecture fix: remove sqlx::Any from production DB code

Do not keep dual-dialect production code. It is already causing all current DB failures and forces weak typing across config/admin/ledger.

Current evidence:

- `Cargo.toml:39` still enables `postgres`, `sqlite`, and `any` together.
- `src/main.rs:55-58` installs Any drivers and opens config DB with `AnyPoolOptions`.
- `src/config/mod.rs:6-7` uses `AnyRow`, `Any`, `Pool`.
- `src/admin/mod.rs:22,33,57-64` uses `AnyPool` and `AnyPoolOptions`.
- `src/admin/mod.rs:328-333`, `369-383`, `445-447` use raw `?` placeholders in queries that must run on Postgres.
- `src/admin/mod.rs:402` and `466` use `QueryBuilder::<sqlx::Any>`.
- `src/ledger/mod.rs:8,79,171-184,194-223,268-285,299-374` uses `AnyPool`, `QueryBuilder::<sqlx::Any>`, raw `?`, and Postgres-unsafe aggregate decode.

Recommended one-pass patch:

1. Change production DB imports to `sqlx::{PgPool, Row}` and pool creation to `sqlx::postgres::PgPoolOptions`.
2. Change QueryBuilder to `sqlx::QueryBuilder::<sqlx::Postgres>`.
3. Change raw placeholders from `?` to `$1/$2/...` for raw `query` calls, or use QueryBuilder push_bind for dynamic statements.
4. Cast all aggregate sums that are read as `i64`:
   - `COALESCE(SUM(input_tokens), 0)::BIGINT`
   - `COALESCE(SUM(output_tokens), 0)::BIGINT`
   - `COALESCE(SUM(input_tokens + output_tokens), 0)::BIGINT`
5. Remove `sqlite` and `any` from default `sqlx` features in root `Cargo.toml` after tests are moved off SQLite or isolated behind test-only helpers.
6. Update tests to use Postgres for DB integration or keep SQLite only in isolated unit helpers that do not exercise production DB code. For this repo’s production target, Postgres integration tests are more valuable than dual-dialect unit tests.

Why this is less over-engineered: one concrete production DB dialect gives compile/runtime type clarity and deletes the recurring workaround layer (`g_bool`, Any aggregate casts, `?` placeholder surprises). It does not touch hot path latency because DB remains outside request forwarding.

## P1 cleanup: Redis/Valkey still appears in install artifacts

Root `docker-compose.yml` is now fixed: it runs router + Postgres only, with no Valkey service. Good.

Still stale:

- `install/docker-compose.yml:1,9,17,35-44` still includes Valkey and `REDIS_URL`.
- `install/router-setup/docker-compose.yml:9,17,35-40` still includes Valkey and `REDIS_URL`.
- `install/README.md:70,99,101` still tells production users to configure Redis/Valkey.
- `install/router-setup/Cargo.toml:34` and `install/Cargo.toml:34` still include `redis` dependency copies.
- `Makefile:46` comment still says `valkey` even though root compose no longer has it.

Fix: make install artifacts match the root fastest profile: Postgres only. Keep a short note that Redis/Valkey is optional future work for strict global quota across multiple router instances, not part of default production run.

## P1 benchmark artifact missing

`Makefile:37-38` runs `bench/run.sh`, but `bench/run.sh` does not exist. `scripts/bench_smoke.py` exists, but its own header says it is only a directional smoke and official SOTA proof needs payload 1K/50K/200K and concurrency 1/50/200.

Fix: add real `bench/run.sh` plus payload generator/results output, or update Makefile to point to the actual benchmark artifact. Until this exists and runs, do not claim fastest/SOTA.

Minimum benchmark acceptance for this repo:

- direct backend vs router, same host/network path where possible;
- streaming and non-streaming;
- payload sizes 1K, 50K, 200K;
- concurrency 1, 50, 200;
- report router overhead, TTFB delta, p50/p95/p99 latency, error rate;
- save raw outputs under `bench/results/<timestamp>/`.

## P1 production proof after DB patch

After replacing `sqlx::Any` or at least fixing all Postgres casts/placeholders, run this exact sequence before claiming readiness:

`cargo check --all-targets`

`cargo fmt --all -- --check`

`cargo clippy --all-targets -- -D warnings`

`cargo test --all-targets --no-fail-fast`

`cargo build --release --locked`

Then on a clean Postgres container:

1. `sqlx migrate run`
2. boot router until it listens
3. `GET /healthz`
4. admin create/list/update/disable paths
5. one non-stream completion through mock backend, ledger row written
6. one stream completion with usage and one stream without usage, ledger rows non-zero
7. restart router and verify budget counters are seeded from ledger

## Current patch order for DeepSeek

1. Fix production DB boot immediately: cast `src/config/mod.rs:53-55` aggregates, or preferably begin the `PgPool` migration now.
2. Convert admin/ledger/config/main DB code from `sqlx::Any` to `PgPool`/Postgres. This is the main architecture blocker.
3. Fix `tests/streaming_integration.rs:51-58` DB temp name collision.
4. Run full Rust gate; keep route tests green.
5. Run clean Postgres smoke with admin and ledger paths, not just migration.
6. Clean install artifacts to remove default Redis/Valkey.
7. Add real benchmark artifact before using SOTA/fastest language.

## P1 benchmark script syntax bug added after this snapshot

DeepSeek added `bench/run.sh` and `bench/make_payloads.py`. This is the right direction, but the script still fails at the jq summary line before it can be used as evidence.

Checks run:

- `bash -n bench/run.sh`: PASS.
- `python3 -m py_compile bench/make_payloads.py`: PASS.
- sample `jq` evaluation of the expression from `bench/run.sh:19`: FAIL.

Current broken line:

`bench/run.sh:19`

`jq -r '"  rps \(.summary.requestsPerSec|.*100|round/100) ...' ...`

Output:

`jq: error: syntax error, unexpected INVALID_CHARACTER`

Root cause: `|.*100` is not valid jq syntax. Use an explicit expression around the value.

Smallest fix:

`((.summary.requestsPerSec * 100 | round) / 100)`

The full line should be shaped like:

`jq -r '"  rps \((.summary.requestsPerSec * 100 | round) / 100)  p50 \(.latencyPercentiles.p50|round)ms  p99 \(.latencyPercentiles.p99|round)ms  ok \(.statusCodeDistribution["200"]//0)  non200 \([.statusCodeDistribution|to_entries[]|select(.key!="200")|.value]|add//0)"' ...`

Additional bench verdict: file existence is no longer the blocker, but this script is not usable as SOTA evidence until the jq line runs and one real `make bench` result is saved under `bench/results/<timestamp>/`.
