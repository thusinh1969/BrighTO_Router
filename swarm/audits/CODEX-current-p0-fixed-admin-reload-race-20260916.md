# CODEX current verdict — P0 SQL/auth fixed, admin reload acknowledgement race remains

Time: 2026-09-16 Asia/Ho_Chi_Minh.
Scope: current worktree after DeepSeek updated `src/auth.rs` and `src/admin/mod.rs`.
Rule from `audits/README.md`: Codex does not edit `src/`; this is audit/test evidence plus exact next repair guidance.

## Verdict

The previous P0 root causes are now mostly fixed:

- `src/auth.rs::authorize_key` checks `snapshot.teams.get(&key.team_id)` and rejects missing/disabled team.
- `src/admin/mod.rs::update_team` no longer uses `sqlx::QueryBuilder::Separated`; it builds comma-separated assignments manually and keeps bind params.
- Empty PATCH now returns 400.

Current gates are green, and eventual admin reload works. One production semantics race remains: admin mutation endpoints return 200 after `reload_notify.notify_one()`, but before the runtime snapshot is guaranteed to contain the DB mutation. An immediate request after `PATCH enabled=false` can still hit upstream once before the background reload task applies the snapshot.

If production contract is “admin 200 means mutation is effective on this router instance,” this is still production-red. If eventual consistency after admin changes is acceptable, document it explicitly and keep it as a conscious tradeoff. For a production router, I recommend fixing it now because the patch is on the admin path only and does not add Redis or hot-path DB calls.

## Verified green gates

Current source:

```bash
cargo check --all-targets                         PASS
cargo fmt --all -- --check                        PASS
cargo clippy --all-targets -- -D warnings         PASS
```

Postgres-backed full tests on fresh `pgvector/pgvector:pg16`:

```bash
sqlx migrate run                                  PASS
cargo test --all-targets --no-fail-fast           PASS
```

Test count:

```text
lib tests: 40/40 PASS
streaming integration: 2/2 PASS
```

Release build:

```bash
cargo build --release --locked                    PASS
```

## Verified P0 smoke after waiting for reload

Fresh `pgvector/pgvector:pg16`, migrated schema, `target/release/brigto-router`, local mock OpenAI backend with upstream POST hit counter.

With a 300ms wait after `PATCH enabled=false`:

```text
PASS healthz
PASS admin_create_team status=200
PASS admin_create_key status=200
PASS proxy_before_disable_nonstream status=200 post_hits=1
PASS proxy_before_disable_stream status=200
PASS admin_patch_team_name status=200
PASS admin_patch_empty_400 status=400 body=no fields to update
PASS admin_patch_team_disable status=200
PASS proxy_after_disable_denied_after_300ms_no_upstream_hit status=401 hits_before=2 hits_after=2
PASS ledger_rows_after_flush 2|1|18|31
PASS team_state_persisted team-renamed|f
```

This proves the auth/team-disable logic is correct once the snapshot reload has happened.

## Verified race without waiting

Same smoke, but sending the next request immediately after `PATCH enabled=false` returned 200:

```text
PASS admin_patch_team_disable status=200
FAIL proxy_after_disable_denied_no_upstream_hit denied=(401, {...}) hits_before=2 hits_after=3
FAIL ledger_rows_after_flush 3|1|25|40
```

Interpretation:

- The first post-disable request can still use the old snapshot and hit upstream.
- A later retry is denied with 401 after reload completes.
- The extra upstream hit produces a third ledger row.

This is an acknowledgement race, not a broken auth check.

## Exact root cause

Admin mutation handlers currently do this pattern:

```rust
// create_team / create_key / update_team / disable_key
state.reload_notify.notify_one();
return Ok(...);
```

`notify_one()` wakes the background reload task but does not wait until:

1. `DbConfigLoader::load_snapshot()` reads the new DB state;
2. `wire_snapshot` updates `budget` and `backends`;
3. `cfg.store(Arc::new(snap))` publishes the new snapshot.

The handler can return 200 before those steps complete.

## Exact fix recommended

Keep the current async poll/notify mechanism, but make admin mutation handlers synchronously reload this process before returning success.

Minimal architecture:

1. Pass a reload handle into admin router, not only `Arc<Notify>`.
2. The handle should contain only existing runtime objects:

```rust
struct RuntimeReload {
    cfg: Arc<ArcSwap<ConfigSnapshot>>,
    budget: Arc<RamBudgetStore>,
    backends: Arc<RamBackendPool>,
    pool: PgPool,
}
```

3. Add one admin-path method:

```rust
async fn reload_now(&self) -> Result<(), ApiError> {
    let loader = DbConfigLoader::new(self.pool.clone(), 0);
    let snap = loader.load_snapshot().await.map_err(|e| ApiError::internal(e.to_string()))?;
    self.backends.sync_backends(&snap.backends);
    self.budget.sync_teams(&snap.teams);
    self.cfg.store(Arc::new(snap));
    Ok(())
}
```

4. After each successful admin mutation, call `reload_now().await?` before returning 200/204:

```rust
let result = query.execute(pool).await?;
// rows_affected check...
state.runtime_reload.reload_now().await?;
state.reload_notify.notify_one(); // optional; harmless for existing poll loop
Ok(...)
```

5. Apply this to all config-changing admin endpoints:

```text
create_team
create_key
update_team
disable_key
```

No Redis. No DB in request forwarding path. This is admin/control-plane only.

## Alternative if strict multi-instance propagation is required later

Use PostgreSQL `LISTEN/NOTIFY` as a control-plane invalidation signal because Postgres is already a required production dependency. Do not add Redis just for config invalidation unless there is a separate measured quota/distributed-rate-limit requirement.

For now, local synchronous reload before admin response is enough to fix the verified race on the current single-process router.

## Required verification after reload acknowledgement fix

Run:

```bash
cargo fmt --all
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
DATABASE_URL=postgres://... cargo test --all-targets --no-fail-fast
cargo build --release --locked
```

Then smoke without any post-disable sleep. Required result:

```text
admin create team -> 200
admin create key -> 200
proxy before disable non-stream -> 200 and upstream hit +1
proxy before disable stream -> 200 and upstream hit +1
PATCH /admin/teams/{id} {"name":"team-renamed"} -> 200
PATCH /admin/teams/{id} {} -> 400
PATCH /admin/teams/{id} {"enabled":false} -> 200
immediate next proxy with same key -> 401/403
mock upstream POST count unchanged after immediate denied request
ledger rows after flush -> exactly 2 successful pre-disable rows
team row -> team-renamed|false
```

## Remaining after this

After immediate admin reload is proven, move to the already-filed P1/P2 items:

- packaging/benchmark drift cleanup in `CODEX-dependency-packaging-drift-20260916.md`;
- SOTA proof matrix in `CODEX-sota-architecture-current-blockers-20260916.md`;
- request-head scanner / route allocation cleanup only if benchmark/profile proves they matter.
