# CODEX patch-ready repair card — admin mutation must be effective before 200/204

Time: 2026-09-16 Asia/Ho_Chi_Minh.
Scope: current worktree where admin PATCH SQL/auth team checks are fixed, but admin reload acknowledgement race remains.
Rule from `audits/README.md`: Codex does not edit `src/`; this is exact coding guidance for DeepSeek.

## Verdict

Current source still has this race:

```text
PATCH /admin/teams/{id} {"enabled":false} -> 200
immediate next POST /v1/chat/completions with same key can still reach upstream once
later request -> 401 after background reload catches up
```

Root cause: admin mutation handlers call `state.reload_notify.notify_one()` then return success. `notify_one()` is only a wakeup; it is not an acknowledgement that `cfg.store(new_snapshot)` has happened.

For production, admin mutation success should mean effective on this router instance immediately. Fix it in admin/control-plane path only. Do not add Redis. Do not add DB to proxy hot path.

## Patch option A — recommended minimal full reload on admin path

This is the smallest correct architecture if accepting one extra DB read after admin mutations.

### 1) Change admin router signature

Current:

```rust
// src/handlers.rs
.nest_service("/admin", crate::admin::router(state.reload_notify.clone()))
```

Change to:

```rust
.nest_service("/admin", crate::admin::router(state.clone()))
```

Current:

```rust
// src/admin/mod.rs
pub fn router(reload_notify: Arc<tokio::sync::Notify>) -> Router {
    let state = Arc::new(AdminState::from_env(reload_notify));
```

Change to:

```rust
pub fn router(runtime: Arc<AppState>) -> Router {
    let state = Arc::new(AdminState::from_env(runtime));
```

### 2) Store runtime handle in `AdminState`

Current:

```rust
use crate::contract::{Budget, KeyHash};

struct AdminState {
    db_url: String,
    master_key: String,
    allow_cidrs: Vec<String>,
    pool: Arc<tokio::sync::OnceCell<PgPool>>,
    reload_notify: Arc<tokio::sync::Notify>,
}
```

Change to:

```rust
use crate::config::DbConfigLoader;
use crate::contract::{AppState, Budget, KeyHash};

struct AdminState {
    db_url: String,
    master_key: String,
    allow_cidrs: Vec<String>,
    pool: Arc<tokio::sync::OnceCell<PgPool>>,
    runtime: Arc<AppState>,
}
```

Then:

```rust
fn from_env(runtime: Arc<AppState>) -> Self {
    // existing env parsing unchanged
    Self { db_url, master_key, allow_cidrs, pool: Arc::new(Default::default()), runtime }
}
```

Unit test `admin_auth_rejects_bad_ip` currently manually constructs `AdminState`; update it by creating a small helper `test_admin_state()` or by testing `check_admin_auth` through a lighter helper. Do not compromise runtime code for the test.

### 3) Add synchronous reload method in `AdminState`

```rust
impl AdminState {
    async fn reload_now(&self) -> Result<(), ApiError> {
        let pool = self.pool().await?;
        let loader = DbConfigLoader::new(pool.clone(), 0);
        let snap = loader
            .load_snapshot()
            .await
            .map_err(|e| ApiError::internal(format!("reload config after admin mutation: {e:#}")))?;

        self.runtime.backends.sync_backends(&snap.backends);
        self.runtime.budget.sync_teams(&snap.teams);
        self.runtime.cfg.store(Arc::new(snap));
        self.runtime.reload_notify.notify_one(); // optional; keeps background poll loop awake for consistency
        Ok(())
    }
}
```

This duplicates the `wire_snapshot` logic from `main.rs`; that is acceptable for one helper. If you want to avoid duplication, move `wire_snapshot` into a library module later, but do not introduce a broad trait/service abstraction.

### 4) Call `reload_now().await?` before returning mutation success

Apply to all config-changing admin endpoints:

```text
create_team
create_key
update_team
disable_key
```

Concrete placements:

```rust
// create_team: after id extracted, before Ok(Json(...))
state.reload_now().await?;

// create_key: after id extracted, before Ok(Json(KeyResponse { id, key, prefix }))
state.reload_now().await?;

// update_team: after rows_affected != 0, before Ok(Json(...))
state.reload_now().await?;

// disable_key: after rows_affected != 0, before Ok(StatusCode::NO_CONTENT)
state.reload_now().await?;
```

Replace old direct `state.reload_notify.notify_one()` calls. If keeping notify, put it inside `reload_now`, not duplicated at every handler.

### 5) Create-key failure caveat

If `create_key` inserts the key and then `reload_now()` fails, returning 500 loses the plaintext secret because only the hash is stored. To avoid orphan unusable keys, either:

- run `reload_now()` after insertion and, on reload error, immediately `UPDATE api_keys SET enabled=false WHERE id=$1` before returning the error; or
- document that reload failure after key creation is operator-visible and requires admin cleanup.

Recommended minimal production-safe version:

```rust
if let Err(e) = state.reload_now().await {
    let _ = sqlx::query::<sqlx::Postgres>("UPDATE api_keys SET enabled = false WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await;
    return Err(e);
}
```

Only do this special cleanup for `create_key`, because it is the only endpoint that creates an unrecoverable plaintext secret.

## Patch option B — incremental in-memory snapshot patch

This avoids the extra DB reload and gives immediate local consistency by patching `ConfigSnapshot` directly after DB mutation.

Do not choose this unless you are willing to add `Clone` to `ConfigSnapshot` and update each map carefully. It is faster but more bug-prone than option A:

- `create_team`: clone snapshot, insert new `Team`, `budget.sync_teams`, `cfg.store`.
- `create_key`: clone snapshot, insert new `ApiKey`, `cfg.store`.
- `update_team`: use `UPDATE ... RETURNING id,name,budget,enabled`, clone snapshot, update team, `budget.sync_teams`, `cfg.store`.
- `disable_key`: clone snapshot, set key enabled=false, `cfg.store`.

Option A is simpler and admin path latency is irrelevant to proxy p99.

## Required verification after patch

Run gates:

```bash
cargo fmt --all
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
DATABASE_URL=postgres://... cargo test --all-targets --no-fail-fast
cargo build --release --locked
```

Then run runtime smoke **without any sleep after PATCH disable**:

```text
admin create team -> 200
admin create key -> 200
proxy before disable non-stream -> 200 and upstream POST count +1
proxy before disable stream -> 200 and upstream POST count +1
PATCH /admin/teams/{id} {"name":"team-renamed"} -> 200
PATCH /admin/teams/{id} {} -> 400
PATCH /admin/teams/{id} {"enabled":false} -> 200
immediate next proxy with same key -> 401/403
mock upstream POST count unchanged after immediate denied request
ledger rows after flush -> exactly 2 successful pre-disable rows
team row -> team-renamed|false
```

## Do not do

- Do not add Redis/Valkey for this. PostgreSQL already stores config and one-process immediate consistency does not require another service.
- Do not put a DB read in `handlers.rs`/`proxy.rs` request path.
- Do not make admin success eventual unless that is explicitly documented as product behavior.
- Do not start SOTA benchmark claim until this immediate admin-disable smoke passes.
