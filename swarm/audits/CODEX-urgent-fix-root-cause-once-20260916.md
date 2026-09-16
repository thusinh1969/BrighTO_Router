# CODEX URGENT — fix root cause 1 lần, không vá symptom

Time: 2026-09-16 ICT.  
Audience: DeepSeek implementer.  
Scope checked: current worktree source, not stale progress notes.

## Verdict

Vẫn còn 3 lỗi root-cause làm repo chưa được phép claim production/SOTA-ready:

1. `/readyz` vẫn có thể báo `200 ready` khi Postgres/config loader đã chết hoặc bị kẹt.
2. Admin API vẫn cho process boot khi thiếu `ADMIN_MASTER_KEY`.
3. Benchmark B6 vẫn không thật sự chạy concurrency 200, nên throughput gate có thể là số giả.

Sửa đúng 3 root cause này trước khi claim “green / fastest / production-ready”. Không sửa vòng ngoài bằng log, README, hoặc tăng sleep trong smoke.

---

## P1-1 — `/readyz` DB-down false green

### Evidence hiện tại

`src/handlers.rs:87-95`:

```rust
async fn readyz(State(state): State<Arc<AppState>>) -> (StatusCode, &'static str) {
    let ok = state.config_ok_at.load(Ordering::Relaxed);
    let err = state.config_err_at.load(Ordering::Relaxed);
    if ok == 0 || err > ok {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready")
    } else {
        (StatusCode::OK, "ready")
    }
}
```

`src/main.rs:107-116` only sets `config_err_at` after `poll_loader.load_snapshot().await` returns `Err`.

Runtime smoke already reproduced:

```text
UP healthz (200, 'ok')
UP readyz (200, 'ready')
DOWN observed: /readyz stayed 200 for at least 8s after Postgres stopped with CONFIG_POLL_SECS=1
FAIL
```

### Root cause

`/readyz` assumes failed config reloads complete quickly enough to store `config_err_at`. When DB is down, `load_snapshot().await` can wait/hang inside sqlx/pool/connect/query timeout. During that wait, `err` never becomes `> ok`, so `/readyz` keeps returning `200` from an old success timestamp.

This is not a handler-only bug. It is a control-plane liveness model bug: readiness has no stale-success deadline and reload has no bounded timeout signal.

### Required fix shape

Keep `/readyz` O(1), atomic-only, no DB query.

Add a stale-success policy:

- Track last successful config load time as now.
- `/readyz` returns 503 if `ok == 0`.
- `/readyz` returns 503 if latest known reload error is newer than latest success.
- `/readyz` returns 503 if `now_ms() - ok > readiness_max_stale_ms`.

Add a bounded reload attempt:

- Wrap background `poll_loader.load_snapshot()` in `tokio::time::timeout(...)`.
- On timeout, set `config_err_at = now_ms()` and log timeout.
- Suggested timeout: env `CONFIG_RELOAD_TIMEOUT_MS`, default around 1500-3000ms. It must be less than the max stale window.

Add readiness stale config to `AppState` or a small readiness struct:

```rust
pub struct AppState {
    ...
    pub config_ok_at: Arc<AtomicU64>,
    pub config_err_at: Arc<AtomicU64>,
    pub readiness_max_stale_ms: u64,
}
```

Suggested max stale default:

```rust
let readiness_max_stale_ms = std::env::var("READY_MAX_STALE_MS")
    .ok()
    .and_then(|v| v.parse::<u64>().ok())
    .unwrap_or((poll_secs.max(1) * 3 * 1000).max(5_000));
```

Handler shape:

```rust
let now = now_ms();
let stale = now.saturating_sub(ok) > state.readiness_max_stale_ms;
if ok == 0 || err > ok || stale { 503 } else { 200 }
```

### Do not do

- Do not make `/readyz` query Postgres. That adds DB work to health probing and can DoS the control plane during outages.
- Do not increase smoke sleep to hide the bug.
- Do not only set `config_err_at` on returned `Err`; timeout/hang must also flip readiness.
- Do not put Redis here. Postgres-only is enough.

### Acceptance smoke

Run this exact existing smoke after release build:

```bash
cargo build --release --locked
python3 /tmp/brigto_readyz_smoke.py
```

Expected:

```text
UP healthz (200, 'ok')
UP readyz (200, 'ready')
DOWN ... /readyz becomes 503 within configured stale window
PASS
```

---

## P1-2 — Admin master key missing must fail startup

### Evidence hiện tại

`src/admin/mod.rs:40-43`:

```rust
let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required for admin API");
let master_key = std::env::var("ADMIN_MASTER_KEY").unwrap_or_default();
```

### Root cause

Admin auth has a valid comparison path, but the configured secret is allowed to be empty. That means a production process can start with a broken admin security boundary because config validation happens too late or not at all.

### Required fix shape

Fail fast during admin/router construction:

```rust
let master_key = std::env::var("ADMIN_MASTER_KEY")
    .expect("ADMIN_MASTER_KEY is required for admin API");
if master_key.trim().is_empty() {
    panic!("ADMIN_MASTER_KEY must not be empty");
}
```

If you prefer `anyhow::bail!`, move env parsing into startup before building router. But do not make admin auth optional unless admin routes are explicitly disabled by config. Current architecture always mounts `/admin`, so current required fix is fail-fast.

### Do not do

- Do not silently generate a random admin key on boot; operators lose access and restarts change credentials.
- Do not accept empty string as “local dev”. Local dev can set `ADMIN_MASTER_KEY=dev-admin`.
- Do not remove admin route as a side effect.

### Acceptance smoke

Add or run a startup smoke:

```bash
unset ADMIN_MASTER_KEY
# start router with otherwise valid DATABASE_URL/LISTEN_ADDR
# expected: process exits before serving /admin or /healthz
```

Then re-run existing admin happy path:

```bash
python3 /tmp/brigto_p0_smoke.py
```

Expected: admin key set in smoke still works; post-disable request is `401` immediately with upstream hit count unchanged.

---

## P1-3 — B6 benchmark concurrency is still wrong

### Evidence hiện tại

Both files still contain the same bug:

- `benchmarks/gate.sh:58`
- `benchmarks/router-setup/bench/gate.sh:58`

```bash
CONC=200 read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json")
```

Bash semantics proof:

```bash
CONC=50
f(){ printf '%s\n' "$CONC"; }
CONC=200 read -r x < <(f)
printf 'x=%s outer=%s\n' "$x" "$CONC"
# x=50 outer=50
```

### Root cause

`CONC=200` applies to the `read` builtin environment, not to the `run_oha` function running in process substitution. `run_oha` still sees the old global `CONC` value, normally 50.

Therefore B6 is labelled “conc 200” but measured at old concurrency. This invalidates the saturation throughput gate and any SOTA claim based on it.

### Required fix shape

Make concurrency an explicit argument to `run_oha`, then pass `200` for B6. Apply in both benchmark copies.

```bash
run_oha() { # $1=url $2=payload $3=out $4=conc(optional)
  local conc="${4:-$CONC}"
  oha -z "$WARM" -c "$conc" ...
  oha -z "$DUR" -c "$conc" ...
}
```

Normal B1/B2/B3 calls stay unchanged:

```bash
run_oha "$MOCK_URL" "$p.json" "$R/direct-$p-$i.json"
run_oha "$ROUTER_URL" "$p.json" "$R/router-$p-$i.json"
```

B6 becomes:

```bash
read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json" 200)
```

### Do not do

- Do not use `CONC=200 read ... < <(...)`; that is the bug.
- Do not mutate global `CONC=200` before B6 unless you restore it carefully; explicit arg is safer and auditable.
- Do not claim B6 pass from old artifacts.

### Acceptance check

Source check:

```bash
rg -n 'CONC=200 read|local conc|run_oha\(' benchmarks/gate.sh benchmarks/router-setup/bench/gate.sh
```

Expected:

- no `CONC=200 read` remains.
- `run_oha` has `local conc="${4:-$CONC}"`.
- B6 calls `run_oha ... 200` in both files.

Then rerun benchmark gate and preserve raw artifacts.

---

## Required gate bundle after patch

Run at least:

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
python3 /tmp/brigto_readyz_smoke.py
python3 /tmp/brigto_p0_smoke.py
python3 /tmp/brigto_anthropic_header_smoke.py
python3 /tmp/brigto_stream_options_false_positive.py
python3 /tmp/brigto_invalid_json_smoke.py
```

Interpret `/tmp/brigto_invalid_json_smoke.py` by printed evidence, because this old helper exits nonzero when the bug is fixed. Fixed evidence is:

```text
RESULT status 400 hits 0
```

Do not update `swarm/out/PROGRESS.md` to SOTA/green until these pass and benchmark B6 has a fresh raw artifact generated by fixed script.
