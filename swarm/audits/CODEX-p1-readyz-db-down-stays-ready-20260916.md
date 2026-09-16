# P1 — `/readyz` exists but stays 200 when Postgres is down

Time: 2026-09-16 22:xx ICT  
Scope: current worktree. Codex did not edit `src/`.

## Verdict

DeepSeek implemented `/readyz` in the right low-overhead direction, but current runtime behavior does not satisfy the release gate. When Postgres is stopped, `/healthz` stays 200 as desired, but `/readyz` also stays 200 for at least 8 seconds with `CONFIG_POLL_SECS=1`.

This is a real runtime failure, not just a missing endpoint.

## Runtime proof

Command used:

```bash
cargo build --release --locked
python3 /tmp/brigto_readyz_smoke.py
```

Smoke behavior:

```text
UP healthz (200, 'ok')
UP readyz (200, 'ready')
DOWN observed [
  ((200, 'ok'), (200, 'ready')),
  ((200, 'ok'), (200, 'ready')),
  ((200, 'ok'), (200, 'ready')),
  ((200, 'ok'), (200, 'ready')),
  ((200, 'ok'), (200, 'ready')),
  ((200, 'ok'), (200, 'ready')),
  ((200, 'ok'), (200, 'ready')),
  ((200, 'ok'), (200, 'ready'))
]
FAIL
```

Expected release-gate behavior:

```text
Postgres up:    /healthz 200, /readyz 200
Postgres down:  /healthz 200, /readyz 503 after configured poll/timeout threshold
Proxy path:     continues serving from RAM snapshot while DB is down
```

## Source root cause

Current `/readyz` only checks whether the last recorded reload error timestamp is newer than the last success timestamp:

```rust
let ok = state.config_ok_at.load(Ordering::Relaxed);
let err = state.config_err_at.load(Ordering::Relaxed);
if ok == 0 || err > ok {
    (StatusCode::SERVICE_UNAVAILABLE, "not ready")
} else {
    (StatusCode::OK, "ready")
}
```

The poll loop only updates `config_err_at` after `load_snapshot().await` returns `Err`:

```rust
match poll_loader.load_snapshot().await {
    Ok(snap) => { ... poll_ok.store(now_ms(), Relaxed); }
    Err(e) => { ... poll_err.store(now_ms(), Relaxed); }
}
```

When Postgres disappears, the SQLx pool/query path may wait longer than the readiness expectation before returning an error. During that window `err` remains older than `ok`, so `/readyz` returns 200 even though config reload is no longer healthy.

## Exact patch contract

Keep `/readyz` atomic-only. Do **not** query Postgres inside `/readyz`.

Add one of these minimal fixes.

### Option A — stale-success threshold in `/readyz` (recommended)

Add a readiness stale threshold, e.g. `READY_MAX_STALE_MS = max(3 * CONFIG_POLL_SECS, 5s)` or env-configured `READY_MAX_STALE_SECS`.

Pseudo-code:

```rust
async fn readyz(State(state): State<Arc<AppState>>) -> (StatusCode, &'static str) {
    let ok = state.config_ok_at.load(Ordering::Relaxed);
    let err = state.config_err_at.load(Ordering::Relaxed);
    let now = now_ms();
    let stale = ok == 0 || now.saturating_sub(ok) > state.ready_max_stale_ms;
    if stale || err > ok {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready")
    } else {
        (StatusCode::OK, "ready")
    }
}
```

For current struct style, add `ready_max_stale_ms: u64` to `AppState` or use a small constant. If adding a field, update all test `AppState` initializers.

Why this is right: if reload task is stuck waiting on DB/network, readiness flips to 503 based on stale `config_ok_at` even before SQLx returns an error.

### Option B — timeout reload attempts

Wrap `load_snapshot()` in the poll loop:

```rust
match tokio::time::timeout(reload_timeout, poll_loader.load_snapshot()).await {
    Ok(Ok(snap)) => { ... poll_ok.store(now_ms(), Relaxed); }
    Ok(Err(e)) => { ... poll_err.store(now_ms(), Relaxed); }
    Err(_) => { poll_err.store(now_ms(), Relaxed); }
}
```

This is useful even with Option A. Keep timeout on control-plane reload only, not proxy requests.

## Validation

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
python3 /tmp/brigto_readyz_smoke.py
```

Expected smoke after fix:

```text
UP healthz (200, 'ok')
UP readyz (200, 'ready')
DOWN observed ... at least one /readyz 503 while /healthz remains 200
PASS
```

Then rerun full Postgres tests because `AppState` changed recently:

```bash
DATABASE_URL=postgres://... CARGO_INCREMENTAL=0 cargo test --all-targets --no-fail-fast
```

## Non-negotiable

Do not add Redis or per-request DB checks. `/readyz` is control-plane state derived from the existing config reload loop. The proxy hot path must continue using the last good RAM snapshot while Postgres is down.
