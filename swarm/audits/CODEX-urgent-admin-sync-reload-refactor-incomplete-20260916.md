# URGENT — Admin sync-reload refactor is incomplete; fix these exact call-sites first

Time: 2026-09-16 21:xx ICT  
Scope: current worktree only. Codex did not edit `src/`.

## Verdict

Current source is **not green**. The intended architecture is correct — admin mutations should synchronously reload `AppState` before returning success — but the refactor was left half-applied.

Do **not** redesign this with Redis, background ack waiting, or a new state subsystem. Root cause is smaller: `AdminState` now owns `runtime: Arc<AppState>` and `admin::router()` now expects `Arc<AppState>`, but two old call-sites still use the previous `reload_notify` shape.

## Repro command

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
```

Current failure:

```text
error[E0560]: struct `admin::AdminState` has no field named `reload_notify`
   --> src/admin/mod.rs:605:13

error[E0061]: this function takes 4 arguments but 3 arguments were supplied
   --> src/admin/mod.rs:611:19

error[E0308]: mismatched types
   --> src/handlers.rs:39:54
    |
39 | .nest_service("/admin", crate::admin::router(state.reload_notify.clone()))
    |                                                      ^ expected Arc<AppState>, found Arc<Notify>
```

Use `CARGO_INCREMENTAL=0` while validating this patch. A normal incremental check also printed a rustc ICE after the real type errors, which makes the log noisy. The real actionable errors are the three above.

## Exact required patch

### 1. Fix router wiring in `src/handlers.rs`

Current broken line:

```rust
.nest_service("/admin", crate::admin::router(state.reload_notify.clone()))
```

Replace with:

```rust
.nest_service("/admin", crate::admin::router(state.clone()))
```

Reason: `admin::router(runtime: Arc<AppState>)` needs full runtime so `AdminState::reload_now()` can atomically update `backends`, `budget`, and `cfg` before admin mutation returns. Passing only `Notify` reintroduces the old async race by construction.

### 2. Fix stale unit test in `src/admin/mod.rs`

Current stale test still constructs old `AdminState { reload_notify: ... }` and calls old `check_admin_auth(&state, ...)` shape:

```rust
let state = AdminState {
    db_url: "postgres://unused-in-this-test".into(),
    master_key: "secret".into(),
    allow_cidrs: vec!["10.0.0.0/8".into()],
    pool: Arc::new(Default::default()),
    reload_notify: Arc::new(tokio::sync::Notify::new()),
};

let err = check_admin_auth(&state, &headers, "1.2.3.4".parse().unwrap()).unwrap_err();
```

Replace the test body with the pure helper call. Do not construct `AdminState`; this unit test does not need runtime or DB:

```rust
let allow_cidrs = vec!["10.0.0.0/8".to_string()];
let mut headers = HeaderMap::new();
headers.insert("x-admin-key", "secret".parse().unwrap());

let err = check_admin_auth(
    "secret",
    &allow_cidrs,
    &headers,
    "1.2.3.4".parse().unwrap(),
)
.unwrap_err();
assert_eq!(err.status, StatusCode::FORBIDDEN);
```

Reason: `check_admin_auth()` signature is now:

```rust
fn check_admin_auth(
    master_key: &str,
    allow_cidrs: &[String],
    headers: &HeaderMap,
    peer_ip: IpAddr,
) -> Result<(), ApiError>
```

All production handlers already use this correct signature. Only the test remains stale.

## Root-cause contract for this fix

After the two edits above, the admin sync-reload architecture should stay as currently intended:

- `AdminState` keeps `runtime: Arc<AppState>`.
- `AdminState::reload_now()` loads fresh DB config and then updates, in this order:
  1. `runtime.backends.sync_backends(&snap.backends)`
  2. `runtime.budget.sync_teams(&snap.teams)`
  3. `runtime.cfg.store(Arc::new(snap))`
- Admin mutations call `reload_now().await?` **before** returning success:
  - `create_team`
  - `create_key`
  - `update_team`
  - `disable_key`
- `create_key` keeps the rollback guard: if reload fails after insert, disable that key before returning error.

This is the right production architecture because admin path may hit Postgres and reload synchronously; proxy hot path must stay RAM-only.

## Validation required after patch

Run these in order:

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
```

Then run the runtime smoke that previously caught the race:

```bash
python3 /tmp/brigto_p0_smoke.py
```

Expected result after the patch: immediate request after `PATCH /admin/teams/{id} {"enabled":false}` must return `401` and upstream mock hit count must not increase. Passing only after a sleep is not acceptable.

Also rerun malformed JSON smoke because `src/request.rs` was removed and handler appears back on `serde_json::from_slice`:

```bash
python3 /tmp/brigto_invalid_json_smoke.py
```

Expected production behavior: malformed JSON returns `400` and upstream mock receives `0` hits. Note: this script used to exit success when reproducing the old bug, so read the printed status/hit count rather than trusting exit code.

## Non-negotiable

Do not add Redis/Valkey/Postgres access to the proxy request path for this. The bug is admin-control-plane visibility after mutation. Synchronous admin reload fixes root cause without touching hot-path latency.
