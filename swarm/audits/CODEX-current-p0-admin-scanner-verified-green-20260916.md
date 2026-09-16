# Current P0 verification — admin sync reload and malformed JSON are green

Time: 2026-09-16 21:xx ICT  
Scope: current worktree. Codex did not edit `src/`.

## Verdict

The previous P0s are now fixed in current source:

1. Admin mutation visibility race: **fixed and runtime verified**.
2. Custom request scanner malformed-JSON forwarding: **fixed by rollback from hot path and runtime verified**.
3. SQLx admin PATCH builder issue: still fixed.
4. Disabled-team auth bypass: still fixed.

Do not reopen these unless a new failing repro appears. Move to remaining architecture/perf/packaging blockers.

## Source evidence

Current admin router wiring is correct:

```text
src/handlers.rs:39 .nest_service("/admin", crate::admin::router(state.clone()))
src/admin/mod.rs:235 pub fn router(runtime: Arc<AppState>) -> Router
```

Current admin mutation handlers call sync reload before returning success:

```text
src/admin/mod.rs:358 state.reload_now().await?;          # create_team
src/admin/mod.rs:409 if let Err(e) = state.reload_now().await { ... disable inserted key ... }  # create_key
src/admin/mod.rs:481 state.reload_now().await?;          # update_team
src/admin/mod.rs:503 state.reload_now().await?;          # disable_key
```

Current request parsing is back on serde and the custom scanner module is gone from production module tree:

```text
src/handlers.rs:116 serde_json::from_slice(&body_bytes)
src/lib.rs: no `pub mod request`
src/request.rs: MISSING
```

## Gate evidence

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
```

Result: all exit 0 on current worktree.

Full Postgres-backed test run:

```bash
# fresh docker: pgvector/pgvector:pg16
cargo sqlx migrate run
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:<port>/postgres \
  CARGO_INCREMENTAL=0 cargo test --all-targets --no-fail-fast
```

Result:

```text
src/lib.rs unit tests:                 40 passed, 0 failed
src/main.rs unit tests:                 0 passed, 0 failed
tests/streaming_integration.rs:         2 passed, 0 failed
```

## Runtime smoke evidence

Malformed JSON smoke:

```text
RESULT status 400 hits 0 resp {"error":{"message":"invalid JSON","type":"invalid_request_error"}}
```

Interpretation: malformed body is rejected by router and not forwarded upstream. The script returned rc=1 only because it was originally written to return success when reproducing the old bug. The printed status/hit count is the authoritative result.

Admin no-sleep disable smoke:

```text
PASS admin_patch_team_disable status=200 body={'id': 1}
PASS proxy_after_disable_denied_no_upstream_hit denied=(401, {...}) hits_before=2 hits_after=2
PASS ledger_rows_after_flush 2|1|18|31
PASS team_state_persisted team-renamed|f
FAILURES 0
```

Interpretation: after `PATCH /admin/teams/{id}` with `enabled=false`, the very next proxied request is denied and does not hit upstream. This proves the sync reload architecture works for the race that previously failed.

## Remaining work after this green point

Do these next, in order:

1. Clean stale dependency/package drift:
   - `cargo tree -i sqlx-sqlite`: currently prints `warning: nothing to print`.
   - `cargo tree -i libsqlite3-sys`: currently prints `warning: nothing to print`.
   - `cargo tree -i redis`: currently errors because no Redis package is present.
   - But `Cargo.lock` still contains stale `sqlx-sqlite` and `libsqlite3-sys` entries. Regenerate or update lockfile so source-of-truth packaging does not imply SQLite remains.
2. Re-check benchmark/install copies under `benchmarks/router-setup/*` and `install/router-setup/*`; some still mention Valkey/Redis. Keep them only if clearly marked as external benchmark harness or optional future multi-instance quota, not production default.
3. Run or create a real SOTA benchmark artifact. Do not claim fastest until router-minus-direct overhead is measured for at least 1K/50K/200K bodies under relevant concurrencies.
4. Optional P1 correctness/perf audit: current `proxy::contains_stream_options(body)` appears to detect `stream_options` by raw body scan. If it treats nested/string occurrences as root-level presence, router may skip usage injection incorrectly. Verify before changing.
