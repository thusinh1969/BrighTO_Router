# CODEX urgent correction — sqlx `Separated` root cause still not fixed

Time: 2026-09-16 Asia/Ho_Chi_Minh.
Scope: current worktree after DeepSeek moved `WHERE` outside the separated block.
Rule from `audits/README.md`: Codex does not edit `src/`; exact repair card only.

## Verdict

`src/admin/mod.rs::update_team` is **still runtime-red**. Moving `WHERE` outside the `Separated` block was necessary but not sufficient.

Root cause: `sqlx::QueryBuilder::Separated::push_bind()` also inserts the separator. Therefore this current code is invalid:

```rust
separated.push("name = ").push_bind(name);
```

It generates:

```sql
UPDATE teams SET name = , $1 WHERE id = $2
```

I verified this with a minimal `sqlx 0.9.0` `QueryBuilder<Postgres>` repro using `query.sql()`.

## Proof from sqlx source

`sqlx-core-0.9.0/src/query_builder.rs` documents this explicitly:

```text
Separated exposes identical .push() and .push_bind() methods which push `separator`
before their normal behavior. .push_unseparated() and .push_bind_unseparated()
are also provided to push a SQL fragment without the separator.
```

Implementation confirms it:

```rust
pub fn push_bind(...) -> &mut Self {
    if self.push_separator {
        self.query_builder.push(&self.separator);
    }
    self.query_builder.push_bind(value);
    self.push_separator = true;
}
```

So `separated.push("name = ").push_bind(name)` treats `"name = "` as one separated item and `$1` as another separated item.

## Current runtime proof

Production smoke on fresh `pgvector/pgvector:pg16` and `target/release/brigto-router`:

```text
PASS healthz
PASS admin_create_team
PASS admin_create_key
PASS proxy_before_disable_nonstream
PASS proxy_before_disable_stream
FAIL admin_patch_team_name status=500 body=error returned from database: syntax error at or near ","
PASS admin_patch_empty_400 status=400 body=no fields to update
FAIL admin_patch_team_disable status=500 body=error returned from database: syntax error at or near ","
```

Because disable PATCH failed, the next request kept proxying and hit upstream repeatedly. That part is a consequence of PATCH still failing, not proof that the new auth team check is broken.

## Exact fix

Keep the current `changed` count and `builder.push(" WHERE...")` structure, but change every `push_bind` that is part of the same assignment expression to `push_bind_unseparated`.

Use this shape:

```rust
if let Some(name) = payload.name {
    separated.push("name = ").push_bind_unseparated(name);
    changed += 1;
}

if let Some(budget) = payload.budget {
    match budget {
        Some(b) => {
            let json = serde_json::to_string(&b)?;
            separated.push("budget = ").push_bind_unseparated(json);
        }
        None => {
            separated.push("budget = NULL");
        }
    }
    changed += 1;
}

if let Some(enabled) = payload.enabled {
    separated.push("enabled = ").push_bind_unseparated(enabled);
    changed += 1;
}
```

Do not use raw string interpolation. Keep bind params.

Alternative acceptable fix: remove `Separated` entirely and push comma manually based on `changed > 0`, but the `push_bind_unseparated` patch is smaller.

## Required verification

After patch:

```bash
cargo fmt --all
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
DATABASE_URL=postgres://... cargo test --all-targets --no-fail-fast
cargo build --release --locked
```

Then re-run production smoke. Accept only this result:

```text
PATCH /admin/teams/{id} {"name":"team-renamed"} -> 200
PATCH /admin/teams/{id} {} -> 400
PATCH /admin/teams/{id} {"enabled":false} -> 200
same API key after disable -> 401/403
mock upstream POST count unchanged after denied request
team row persisted as team-renamed|false
usage_ledger contains only successful pre-disable proxy rows after flush
```

Do not proceed to benchmark/SOTA claim until this exact smoke passes.
