# CODEX current aggregate verdict — root gates green, two production blockers remain

Time: 2026-09-16 Asia/Ho_Chi_Minh.
Scope: current worktree after DeepSeek cleaned part of install/dependency surface and added `src/request.rs`.
Rule from `audits/README.md`: Codex does not edit `src/`; this file is audit evidence and exact next actions.

## Verdict

Current root build/test surface is green, and root runtime dependency direction is now mostly lean. But the repo is still **not production/SOTA green** because two production-visible blockers remain:

1. `src/request.rs` scanner accepts malformed JSON and lets `handlers.rs` forward it upstream.
2. Admin mutation endpoints still return success before the new snapshot is applied on this router instance; immediate traffic after `PATCH enabled=false` can slip through once.

Do not run or publish SOTA benchmark claims until these two are fixed and verified. Bench numbers taken with malformed input acceptance or eventual admin semantics are not production-ready evidence.

## Current verified green gates

Run on current source:

```text
cargo check --all-targets                    PASS
cargo fmt --all -- --check                   PASS
cargo clippy --all-targets -- -D warnings    PASS
make check                                   PASS
```

Important nuance: root `make check` currently runs fmt + clippy only. It does not run tests and does not run `cargo audit`.

Last Postgres full test run after scanner was added:

```text
sqlx migrate run on fresh pgvector/pgvector:pg16     PASS
cargo test --all-targets --no-fail-fast              PASS
lib tests                                            46/46 PASS
streaming integration                                2/2 PASS
```

## Current blocker #1 — malformed JSON forwarded upstream

Current hot path:

```rust
// src/handlers.rs
let (model, stream, has_stream_options) = match request::scan(&body_bytes) {
    Some(h) => (h.model, h.stream, h.has_stream_options),
    None => {
        let h: RequestHead<'_> = serde_json::from_slice(&body_bytes)?;
        (h.model, h.stream, proxy::contains_stream_options(&body_bytes))
    }
};
```

Since `Some` bypasses serde validation, `request::scan` must return `None` for malformed JSON. It does not.

Unit-level repro:

```text
trailing_garbage: scan=Some(("x", false, false)) serde_ok=false
trailing_comma: scan=Some(("x", false, false)) serde_ok=false
double_comma: scan=Some(("x", true, false)) serde_ok=false
bad_unknown_literal: scan=Some(("x", false, false)) serde_ok=false
literal_newline_in_string: scan=Some(("x\n", false, false)) serde_ok=false
```

Runtime repro on current release binary with fresh Postgres and mock backend:

```text
request body: {"model":"x","messages":[]} trailing-garbage
MOCK_HIT raw_suffix= b'[]} trailing-garbage'
RESULT status 200 hits 1 resp {"ok":true,"usage":{"prompt_tokens":1,"completion_tokens":1}}
```

Exact action:

- Either rollback scanner from `handlers.rs` and use serde `RequestHead` until scanner is strict and faster, or
- make scanner conservative enough that every malformed/uncertain case returns `None` and serde fallback rejects it.

Minimum tests to add before scanner can stay in hot path:

```rust
assert!(scan(br#"{"model":"x"} garbage"#).is_none());
assert!(scan(br#"{"model":"x",}"#).is_none());
assert!(scan(br#"{"model":"x",,"stream":true}"#).is_none());
assert!(scan(br#"{"model":"x","foo":nul}"#).is_none());
assert!(scan(b"{\"model\":\"x\n\"}").is_none());
```

Also prove scanner is faster than serde for 200K. Current release micro-bench showed the opposite:

```text
scan  200k-model-first-no-stream-options:  380.790 us/iter
serde 200k-model-first-no-stream-options:  133.183 us/iter
scan  200k-model-last:                     446.605 us/iter
serde 200k-model-last:                     128.459 us/iter
```

## Current blocker #2 — admin mutation acknowledgement race

Current source still has this shape:

```text
src/handlers.rs: .nest_service("/admin", crate::admin::router(state.reload_notify.clone()))
src/admin/mod.rs: pub fn router(reload_notify: Arc<tokio::sync::Notify>) -> Router
src/admin/mod.rs: mutation handlers call state.reload_notify.notify_one()
```

There is no `reload_now`/synchronous snapshot apply before admin returns success.

Runtime proof from current source after admin SQL/auth fixes:

```text
PATCH /admin/teams/{id} {"enabled":false} -> 200
immediate next proxy with same key -> one upstream POST can still happen
later proxy after ~300ms -> 401 and no upstream hit
```

Exact action:

- Change admin router to receive the runtime state or a narrow reload handle that includes `cfg`, `budget`, `backends`, and a `PgPool`.
- After successful config mutation, synchronously `load_snapshot`, `sync_backends`, `sync_teams`, and `cfg.store(...)` before returning 200/204.
- Apply to `create_team`, `create_key`, `update_team`, and `disable_key`.
- Keep this DB work on admin path only. Do not add Redis. Do not add DB reads to proxy path.

Required no-sleep smoke after fix:

```text
PATCH /admin/teams/{id} {"enabled":false} -> 200
immediate next proxy with same key -> 401/403
mock upstream POST count unchanged after immediate denied request
ledger rows after flush -> exactly successful pre-disable rows only
```

## Dependency/packaging status changed

DeepSeek improved root/install cleanup:

- `scripts/fix_a1.py` is deleted.
- root `Cargo.toml` remains Postgres-only and Redis-free.
- root `make check` passes.
- install compose copies are now Postgres-only service-wise.

Still open:

```text
Cargo.lock still contains stale sqlx-sqlite/libsqlite3-sys entries, but cargo tree says they are not active.
benchmarks/router-setup/* still carries Redis/Valkey copy and redis dependency.
docs/agents/* still reference old SQLite/sqlx::Any agent instructions.
cargo audit is not installed: `cargo audit` -> error: no such command: audit.
```

Exact action after the two production blockers:

- Prune/regenerate `Cargo.lock` if the team wants lockfile to reflect current active graph.
- Either delete `benchmarks/router-setup` copy or sync it to root Postgres-only profile; do not maintain a stale Redis benchmark tree as official proof.
- Keep historical `docs/agents/*` clearly marked stale or move under archive.
- Install `cargo-audit` in CI/toolchain or stop advertising audit as a local green gate.

## Correct next order

1. Fix/rollback request scanner so malformed JSON cannot be forwarded.
2. Fix admin synchronous reload acknowledgement.
3. Re-run full compile/fmt/clippy/Postgres tests/release smoke.
4. Run invalid JSON runtime smoke and immediate post-disable smoke.
5. Clean remaining benchmark copy drift.
6. Run SOTA benchmark matrix and store artifacts.
