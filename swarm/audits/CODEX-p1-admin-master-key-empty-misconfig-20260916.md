# P1 — Admin API accepts an empty configured master key if `ADMIN_MASTER_KEY` is missing

Time: 2026-09-16 22:xx ICT  
Scope: current worktree. Codex did not edit `src/`.

## Verdict

Admin auth should fail closed at startup if `ADMIN_MASTER_KEY` is missing or empty. Current code silently defaults the master key to `""`.

This is a production misconfiguration footgun. The admin API is IP-allowlisted, but localhost/container/sidecar exposure is common; an empty admin secret should never be a valid runtime state.

## Source evidence

Current code:

```text
src/admin/mod.rs:40 fn from_env(runtime: Arc<AppState>) -> Self {
src/admin/mod.rs:41     let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required for admin API");
src/admin/mod.rs:42     let master_key = std::env::var("ADMIN_MASTER_KEY").unwrap_or_default();
```

Auth check compares request header against that configured value:

```text
src/admin/mod.rs:198 if provided.as_deref() != Some(master_key) {
src/admin/mod.rs:199     return Err(ApiError::unauthorized("invalid admin key"));
```

So the process can boot with `master_key == ""`. That state should be impossible.

## Exact patch

In `AdminState::from_env`, replace default-empty behavior with fail-fast validation:

```rust
let master_key = std::env::var("ADMIN_MASTER_KEY")
    .expect("ADMIN_MASTER_KEY is required for admin API");
if master_key.trim().is_empty() {
    panic!("ADMIN_MASTER_KEY must not be empty");
}
```

If avoiding `panic!` style inside helper is preferred, change `from_env` to return `Result<Self, anyhow::Error>` and propagate at startup. But do not create a new auth subsystem for this; the root cause is one bad default.

## Test required

Add a small unit test for `check_admin_auth` or env parsing behavior:

```rust
assert!(check_admin_auth("secret", &allow_cidrs, &headers_with_secret, ip).is_ok());
assert_eq!(check_admin_auth("", &allow_cidrs, &headers_empty, ip), Err/forbidden-startup-state);
```

Better: factor env validation into a pure helper so it can be tested without mutating process env:

```rust
fn validate_master_key(value: String) -> String {
    if value.trim().is_empty() { panic!("ADMIN_MASTER_KEY must not be empty"); }
    value
}
```

## Validation

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets --no-fail-fast
```

Runtime smoke should still pass when `ADMIN_MASTER_KEY` is configured:

```bash
python3 /tmp/brigto_p0_smoke.py
```

## Non-negotiable

Do not make admin auth optional for “dev convenience” in the production binary. Dev setup scripts already generate/write an admin key into `.env`; missing key should stop the process before serving `/admin`.
