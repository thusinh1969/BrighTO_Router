# CODEX current P0 verdict — admin PATCH SQL + disabled team auth bypass

Time: 2026-09-16 Asia/Ho_Chi_Minh.
Scope: current worktree after `reload_notify` compile/fmt fixes.
Rule from `audits/README.md`: Codex does not edit `src/`; this is a concrete repair card for DeepSeek.

## Verdict

Current worktree has green compile/test/build gates, but it is still **production-red** for two P0 runtime issues:

1. `PATCH /admin/teams/{id}` returns 500 on real Postgres because `WHERE` is pushed through `sqlx::QueryBuilder::Separated`, generating `SET name = $1, WHERE id = $2`.
2. A key belonging to a disabled team is still accepted and proxied with 200. Team disable currently prunes team budget only; it does not enforce traffic stop.

Fix these together. The admin portal semantics are unsafe if PATCH can disable a team but auth keeps serving that team's keys.

## Current green evidence

Run on current source:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Result: PASS.

Postgres test gate on clean `pgvector/pgvector:pg16`:

```bash
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:55432/llm_router sqlx migrate run
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:55432/llm_router cargo test --all-targets --no-fail-fast
```

Result: migration PASS, lib 38/38 PASS, streaming integration 2/2 PASS.

Release build:

```bash
cargo build --release --locked
```

Result: PASS.

## P0 #1 — admin PATCH SQL builder bug

Runtime smoke on fresh Postgres:

```text
PASS healthz
PASS admin_create_team -> 200
PASS admin_create_key -> 200
PASS proxy_nonstream_after_admin_reload -> 200
PASS proxy_stream -> 200
FAIL admin_patch_team_name -> 500: error returned from database: syntax error at or near ","
PASS ledger_rows after 2.5s flush wait: 2|1|18|31
```

Source root cause:

```rust
let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE teams SET ");
let mut separated = builder.separated(", ");
// fields...
separated.push(" WHERE id = ").push_bind(id);
```

`Separated` injects `, ` before later pushes. `WHERE` must be pushed on `builder` after `separated` is dropped.

Exact fix:

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

## P0 #2 — disabled team auth bypass

Verified smoke on fresh Postgres:

- Seeded `teams(id=1, enabled=false)`.
- Seeded enabled API key for `team_id=1`.
- Booted release router from that DB.
- Sent `POST /v1/chat/completions` with that key.

Actual result:

```text
RESULT status 200 body {'choices': [{'message': {'content': 'ok'}}], 'usage': {'prompt_tokens': 1, 'completion_tokens': 1}}
```

Expected result: reject before proxy, either 401 or 403. The important invariant is no upstream request for disabled/missing team.

Source root cause:

```rust
// src/auth.rs
pub fn authorize_key(snapshot: &ConfigSnapshot, key_hash: &[u8; 32]) -> Result<ApiKey, AuthError> {
    let key = snapshot.keys_by_hash.get(key_hash).ok_or(AuthError::InvalidKey)?;
    if !key.enabled { return Err(AuthError::Disabled); }
    // expiry check...
    Ok(key.clone())
}
```

`ConfigSnapshot` already contains `teams`, but auth does not use it. `RamBudgetStore::sync_teams` only controls budget counters; it is not authorization.

Exact fix:

```rust
let Some(team) = snapshot.teams.get(&key.team_id) else {
    return Err(AuthError::Disabled);
};
if !team.enabled {
    return Err(AuthError::Disabled);
}
```

Place it in `authorize_key` after `key.enabled` check and before returning the key. This adds one immutable `HashMap` lookup on the hot path; no DB/Redis/env/file access and no new abstraction.

Add/adjust tests in `src/auth.rs`:

```rust
#[test]
fn disabled_team_rejects_key() {
    // snapshot has enabled key, team exists but enabled=false
    // authorize_key(...) must return Err(AuthError::Disabled)
}

#[test]
fn missing_team_rejects_key() {
    // snapshot has enabled key whose team_id is absent
    // authorize_key(...) must return Err(AuthError::Disabled) or InvalidKey, but must not Ok
}
```

Also add one integration/runtime check after PATCH is fixed:

```text
PATCH /admin/teams/{id} {"enabled":false} -> 200
POST /v1/chat/completions with existing key -> 401/403, no upstream hit
```

## Required verification after both P0 fixes

Run:

```bash
cargo fmt --all
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
DATABASE_URL=postgres://... cargo test --all-targets --no-fail-fast
cargo build --release --locked
```

Then production smoke must prove:

```text
admin create team -> 200
admin create key -> 200
proxy before disable -> 200
PATCH team name -> 200
PATCH team enabled=false -> 200
proxy after disable -> 401/403 and mock upstream request count unchanged
GET /admin/usage after ledger flush -> contains only successful pre-disable proxy rows
```

## Do not over-engineer this patch

- Do not add Redis for team disable propagation. Existing `reload_notify + ArcSwap` is enough inside one router process; multi-instance propagation can remain poll-based unless measured product requirements demand stricter latency.
- Do not add DB reads to request path. Use `snapshot.teams` already in memory.
- Do not create a new auth service abstraction. This is a small invariant in `authorize_key` plus tests.
