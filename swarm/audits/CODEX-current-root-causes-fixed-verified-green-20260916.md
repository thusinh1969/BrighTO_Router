# CODEX CURRENT — 3 root causes fixed and verified in real worktree

Time: 2026-09-16 ICT.  
Scope: current real worktree `/mnt/data02/BrigTO_Router`, not isolated copy.

## Verdict

The three urgent root causes from `CODEX-urgent-root-cause-verified-patch-20260916.md` are now fixed in the real repo and verified.

Fixed:

1. `/readyz` no longer stays ready forever when config reload is dead/stuck.
2. Missing/empty `ADMIN_MASTER_KEY` no longer boots a router with an empty admin secret.
3. B6 benchmark no longer lies about concurrency 200; concurrency is passed explicitly into `run_oha`.

Do not keep reworking these unless a regression appears. Move to the remaining SOTA proof work: full benchmark matrix and raw artifact quality.

## Current source evidence

### `/readyz` stale-success + bounded reload

`src/handlers.rs` now calls pure readiness logic with stale threshold:

```rust
let code = readiness_status(ok, err, state.readiness_max_stale_ms, now_ms());
```

`src/main.rs` now has reload timeout and stale window envs:

```rust
std::env::var("CONFIG_RELOAD_TIMEOUT_MS")
let readiness_max_stale_ms = std::env::var("READY_MAX_STALE_MS")
```

The background reload is bounded:

```rust
match tokio::time::timeout(reload_timeout, poll_loader.load_snapshot()).await {
```

`src/contract.rs` now carries:

```rust
pub readiness_max_stale_ms: u64,
```

### Admin master key fail-fast

`src/admin/mod.rs` now requires env and validates non-empty:

```rust
let master_key = validate_master_key(
    std::env::var("ADMIN_MASTER_KEY").expect("ADMIN_MASTER_KEY is required for admin API"),
);
```

Unit helper exists:

```rust
fn validate_master_key(value: String) -> String {
    assert!(
        !value.trim().is_empty(),
        "ADMIN_MASTER_KEY must not be empty"
    );
    value
}
```

### B6 explicit concurrency

Both benchmark copies now use an explicit optional concurrency arg:

```bash
run_oha() { # $1=url $2=payload $3=out $4=conc(optional) ; in: p50 p99 rps non200
  local conc="${4:-$CONC}"
  oha -z "$WARM" -c "$conc" ...
```

B6 now calls:

```bash
read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json" 200)
```

No `CONC=200 read ... < <(...)` remains.

## Verification run on real worktree

```text
cargo fmt --all -- --check                         PASS
CARGO_INCREMENTAL=0 cargo check --all-targets       PASS
cargo clippy --all-targets -- -D warnings           PASS
cargo build --release --locked                      PASS
fresh pgvector/pgvector:pg16 cargo test --all-targets PASS: 45 lib tests + 2 streaming integration tests
cargo audit --file Cargo.lock                       PASS: 0 vulnerabilities, 0 warnings
```

Runtime smokes on real release binary:

```text
/tmp/brigto_readyz_smoke.py                         PASS: /readyz became 503 after DB stop while /healthz stayed 200
/tmp/brigto_p0_smoke.py                             PASS: admin create/patch/disable, no-sleep disabled team denial, ledger rows
missing ADMIN_MASTER_KEY startup smoke              PASS: process exits before serving
ADMIN_MASTER_KEY=smoke-admin /tmp/brigto_anthropic_header_smoke.py PASS
ADMIN_MASTER_KEY=smoke-admin /tmp/brigto_stream_options_false_positive.py PASS
ADMIN_MASTER_KEY=smoke-admin /tmp/brigto_invalid_json_smoke.py evidence PASS: RESULT status 400 hits 0
```

Note: old ad-hoc `/tmp/*_smoke.py` scripts that start the router must set `ADMIN_MASTER_KEY` now. If they do not, startup failure is expected and correct after the admin fix.

## Remaining blocker before any SOTA/fastest claim

The code root causes above are fixed. The repo still must not claim SOTA from the old Round 7 artifact because that artifact was:

- non-stream only,
- concurrency 50 only,
- generated before the B6 concurrency fix,
- and not a complete raw artifact matrix.

Next required work for DeepSeek:

1. Run the fixed benchmark gate from `benchmarks/gate.sh` or `benchmarks/router-setup/bench/gate.sh`.
2. Preserve raw direct/router `oha` JSON for every run, not only rounded summaries.
3. Produce at least the agreed matrix: 1K/50K/200K, non-stream overhead, streaming TTFB, saturation B6 with true conc 200, and enough repeated runs to report median.
4. Only after that, update `swarm/out/PROGRESS.md` with exact artifact path and gate pass/fail.

Architecture note remains unchanged: production default should stay Postgres-only. Redis/Valkey is still not justified unless a measured multi-instance quota/counter requirement appears.
