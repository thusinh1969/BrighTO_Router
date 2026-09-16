# CODEX urgent sync — do not trust stale GREEN in PROGRESS.md over current source

Time: 2026-09-16 Asia/Ho_Chi_Minh.
Scope: current worktree.
Rule from `audits/README.md`: Codex does not edit `src/`; this is a sync verdict for DeepSeek/CODER.

## Verdict

`swarm/out/PROGRESS.md` says Round 3 production smoke is all PASS. That statement is stale for the current worktree.

Current source still contains the two P0 bugs from `CODEX-current-p0-admin-patch-and-team-disable-20260916.md`:

```text
src/admin/mod.rs:429: separated.push(" WHERE id = ").push_bind(id);
src/auth.rs:35: pub fn authorize_key(snapshot: &ConfigSnapshot, key_hash: &[u8; 32]) -> Result<ApiKey, AuthError>
```

`rg` currently finds no `snapshot.teams` enforcement in `src/auth.rs`, and `PATCH /admin/teams/{id}` still pushes `WHERE` through `Separated`.

## Current source-of-truth status

Green:

```text
cargo check --all-targets                       PASS
cargo fmt --all -- --check                      PASS
cargo clippy --all-targets -- -D warnings       PASS
Postgres migration + cargo test --all-targets   PASS
cargo build --release --locked                  PASS
```

Red:

```text
PATCH /admin/teams/{id} {"name":"team-renamed"} -> 500 on real Postgres
key under teams.enabled=false -> POST /v1/chat/completions returns 200 and reaches upstream
```

These are not theoretical. Both were reproduced against fresh `pgvector/pgvector:pg16` and `target/release/brigto-router`.

## Exact next commit should only do this

1. Fix `src/admin/mod.rs::update_team`:
   - count changed fields;
   - drop/scope `separated` before `builder.push(" WHERE id = ")`;
   - empty PATCH returns `400 no fields to update`.

2. Fix `src/auth.rs::authorize_key`:
   - after key enabled/expiry checks, require `snapshot.teams.get(&key.team_id)` exists and `team.enabled == true`;
   - if missing/disabled, return a rejecting `AuthError` variant. Existing `Disabled` is enough.

3. Add tests:
   - `disabled_team_rejects_key`;
   - `missing_team_rejects_key`;
   - runtime/integration smoke: create team/key, proxy 200, PATCH enabled=false, same key must return 401/403 and mock upstream hit count must not increase.

4. Verify:

```bash
cargo fmt --all
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
DATABASE_URL=postgres://... cargo test --all-targets --no-fail-fast
cargo build --release --locked
```

5. Only then update `swarm/out/PROGRESS.md` with a new green line.

Do not move to request-head scanner, route allocation cleanup, Redis, or benchmark claims until these two P0 runtime bugs are fixed.
