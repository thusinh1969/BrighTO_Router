# CODEX -> DeepSeek: fix root cause one pass — current compile red + admin runtime red

Time: 2026-09-16 Asia/Ho_Chi_Minh.
Scope: repo root `/mnt/data02/BrigTO_Router`.
Rule from `audits/README.md`: Codex does not edit `src/`; this is an exact repair card for coder.

## Verdict

Current worktree is still **RED**. Do not continue feature work until these three concrete fixes land together:

1. `tests/streaming_integration.rs` still constructs `AppState` without the new `reload_notify` field.
2. `src/main.rs` fails `cargo fmt --all -- --check` because the tuple clone assignment was not formatted.
3. `src/admin/mod.rs` PATCH team SQL builder still appends `WHERE` through `Separated`, generating invalid SQL like `UPDATE teams SET name = $1, WHERE id = $2`.

This is one root-cause class: the `reload_notify` refactor and admin hot-reload patch were applied partially. Finish all callsites and verify admin PATCH at runtime, not just compile.

## Proof from gate

`cargo check --all-targets`:

```text
error[E0063]: missing field `reload_notify` in initializer of `AppState`
   --> tests/streaming_integration.rs:113:26
```

`cargo fmt --all -- --check`:

```text
Diff in /mnt/data02/BrigTO_Router/src/main.rs:86
let (poll_budget, poll_backends, poll_cfg, poll_notify) =
    (budget.clone(), backends.clone(), cfg.clone(), reload_notify.clone());
```

`cargo clippy --all-targets -- -D warnings` fails for the same missing `reload_notify` field.

Static source check still shows the admin PATCH bug:

```text
src/admin/mod.rs:406 let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE teams SET ");
src/admin/mod.rs:407 let mut separated = builder.separated(", ");
src/admin/mod.rs:429 separated.push(" WHERE id = ").push_bind(id);
```

## Exact fix required

### 1) `tests/streaming_integration.rs`

At `AppState` initializer around line 113, add the field:

```rust
reload_notify: Arc::new(tokio::sync::Notify::new()),
```

The file already uses `Arc`, so a fully-qualified `tokio::sync::Notify` is enough.

### 2) `src/main.rs`

Run `cargo fmt --all` after the code fix. The relevant block should format as:

```rust
let (poll_budget, poll_backends, poll_cfg, poll_notify) = (
    budget.clone(),
    backends.clone(),
    cfg.clone(),
    reload_notify.clone(),
);
```

Do not create a second notify for admin. The correct architecture is one process-wide `Arc<Notify>`:

- `main.rs` creates it once.
- config reload task awaits clones of it.
- `AppState` carries the same clone.
- `handlers::router(state)` passes `state.reload_notify.clone()` into `admin::router(...)`.

### 3) `src/admin/mod.rs` PATCH SQL builder

Do not push `WHERE` through `Separated`. Also handle empty PATCH as 400 instead of building invalid SQL.

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
            None => {
                separated.push("budget = NULL");
            }
        }
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
let query = builder.build();
```

Reason: `Separated` owns separator insertion. If `WHERE` is pushed through it after at least one set expression, it inserts `, ` before `WHERE`. Scoping `separated` before `builder.push(" WHERE...")` is the minimal, correct fix.

## Required verification before calling this green

Run these in order:

```bash
cargo fmt --all
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
```

Then run Postgres-backed tests with a real temporary Postgres/pgvector instance:

```bash
cargo test --all-targets --no-fail-fast
```

Then do a production smoke that includes **admin PATCH**:

```bash
# with clean Postgres and migrated schema
POST /admin/teams
POST /admin/keys
PATCH /admin/teams/{id} {"name":"team-renamed"}
GET /admin/usage
POST /v1/chat/completions non-stream through mock backend
POST /v1/chat/completions stream through mock backend
```

Accept criteria:

- PATCH returns 200, not 500.
- Route reload after admin mutation works without waiting for poll interval.
- Proxy still returns 200 for non-stream and stream.
- `usage_ledger` has persisted rows for both stream and non-stream.

## Do not regress

- Do not re-enable `sqlx::Any`, SQLite, or `AnyPool` to make tests easier. Root is already Postgres-only and that is the correct production direction.
- Do not add Redis/Valkey back into runtime unless there is a measured production requirement for strict multi-instance quota. Current code path is in-process budget + Postgres ledger; keep the hot path lean.
- Do not treat `cargo check` green as enough. The admin PATCH issue is runtime SQL correctness and was already reproduced against real Postgres.
