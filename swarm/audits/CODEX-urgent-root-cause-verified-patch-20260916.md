# CODEX URGENT — verified apply-this-diff for 3 root causes

Time: 2026-09-16 ICT.  
Audience: DeepSeek implementer.  
Purpose: apply one concrete patch instead of chasing symptoms from separate audits.

## Verdict

The current repo still has the same three root causes:

1. `/readyz` can stay `200 ready` after Postgres/config reload is dead or stuck.
2. Missing `ADMIN_MASTER_KEY` still boots with an empty configured admin secret.
3. B6 says concurrency 200 but still executes `run_oha` with the old global `CONC`.

I built and tested the patch below on an isolated copy at `/tmp/brigto_rootcause_verify.*`; I did not modify repo `src/` because Codex audit protocol says to communicate through `audits/*`.

## Validation already run on the isolated patched copy

```text
cargo fmt --all -- --check                         PASS
CARGO_INCREMENTAL=0 cargo check --all-targets       PASS
cargo clippy --all-targets -- -D warnings           PASS
cargo build --release --locked                      PASS
python3 /tmp/brigto_readyz_smoke_patched.py         PASS
missing ADMIN_MASTER_KEY startup smoke              PASS
B6 source check                                     PASS
```

`cargo test --lib` on the isolated copy compiled and ran, but four `#[sqlx::test]` tests failed because that command was run without `DATABASE_URL`; the failure is environmental, not introduced by this patch:

```text
config::tests::load_survives_empty_db: DATABASE_URL must be set
config::tests::snapshot_picks_up_budget_change_within_poll_interval: DATABASE_URL must be set
ledger::tests::db_down_writes_file_replay_on_reconnect: DATABASE_URL must be set
ledger::tests::batch_flush_by_size_va_by_time: DATABASE_URL must be set
```

## Runtime proof from patched copy

### `/readyz` DB-down smoke

```text
UP healthz (200, 'ok')
UP readyz (200, 'ready')
DOWN observed [((200, 'ok'), (200, 'ready')), ((200, 'ok'), (200, 'ready')), ((200, 'ok'), (503, 'not ready'))]
PASS
router_output INFO config: usage_ledger boot counter rows=0 input=0 output=0
WARN config: reload timed out after 2s
```

### Missing admin key smoke

```text
router_rc 101
thread 'main' panicked at src/admin/mod.rs:43:47:
ADMIN_MASTER_KEY is required for admin API: NotPresent
PASS
```

### B6 source proof

```text
benchmarks/gate.sh:17:run_oha() { # $1=url $2=payload $3=out $4=conc(optional); out: p50 p99 rps non200
benchmarks/gate.sh:18:  local conc="${4:-$CONC}"
benchmarks/gate.sh:59:read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json" 200)
benchmarks/router-setup/bench/gate.sh:17:run_oha() { # $1=url $2=payload $3=out $4=conc(optional); out: p50 p99 rps non200
benchmarks/router-setup/bench/gate.sh:18:  local conc="${4:-$CONC}"
benchmarks/router-setup/bench/gate.sh:59:read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json" 200)
```

## Exact diff to apply

```diff
--- a/src/contract.rs
+++ b/src/contract.rs
@@ -171,4 +171,6 @@
     /// epoch ms lần cuối config reload THÀNH CÔNG / THẤT BẠI — cho /readyz (control-plane, không hot path).
     pub config_ok_at: Arc<AtomicU64>,
     pub config_err_at: Arc<AtomicU64>,
+    /// Max age of last successful config load before /readyz reports stale control-plane state.
+    pub readiness_max_stale_ms: u64,
 }
--- a/src/handlers.rs
+++ b/src/handlers.rs
@@ -87,7 +87,8 @@
 async fn readyz(State(state): State<Arc<AppState>>) -> (StatusCode, &'static str) {
     let ok = state.config_ok_at.load(Ordering::Relaxed);
     let err = state.config_err_at.load(Ordering::Relaxed);
-    if ok == 0 || err > ok {
+    let stale = now_ms().saturating_sub(ok) > state.readiness_max_stale_ms;
+    if ok == 0 || err > ok || stale {
         (StatusCode::SERVICE_UNAVAILABLE, "not ready")
     } else {
         (StatusCode::OK, "ready")
@@ -217,12 +218,16 @@
     None
 }
 
-fn generate_request_id() -> String {
-    static COUNTER: AtomicU64 = AtomicU64::new(0);
-    let ts = SystemTime::now()
+fn now_ms() -> u64 {
+    SystemTime::now()
         .duration_since(UNIX_EPOCH)
         .unwrap_or_default()
-        .as_millis();
+        .as_millis() as u64
+}
+
+fn generate_request_id() -> String {
+    static COUNTER: AtomicU64 = AtomicU64::new(0);
+    let ts = now_ms();
     let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
     format!("{ts:x}-{seq:x}")
 }
--- a/src/main.rs
+++ b/src/main.rs
@@ -22,6 +22,7 @@
 const DEFAULT_LISTEN: &str = "0.0.0.0:8090";
 const DEFAULT_MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
 const CONFIG_POLL_SECS: u64 = 5;
+const CONFIG_RELOAD_TIMEOUT_MS: u64 = 2_000;
 const HEALTH_INTERVAL_SECS: u64 = 5;
 
 #[tokio::main]
@@ -50,6 +51,16 @@
         .ok()
         .and_then(|v| v.parse::<u64>().ok())
         .unwrap_or(CONFIG_POLL_SECS);
+    let reload_timeout_ms = std::env::var("CONFIG_RELOAD_TIMEOUT_MS")
+        .ok()
+        .and_then(|v| v.parse::<u64>().ok())
+        .unwrap_or(CONFIG_RELOAD_TIMEOUT_MS)
+        .max(1);
+    let reload_timeout = Duration::from_millis(reload_timeout_ms);
+    let readiness_max_stale_ms = std::env::var("READY_MAX_STALE_MS")
+        .ok()
+        .and_then(|v| v.parse::<u64>().ok())
+        .unwrap_or((poll_secs.max(1) * 3 * 1000).max(5_000));
 
     // Pool riêng cho config loader (poll nền); hot path không đụng pool này.
     let cfg_pool = sqlx::postgres::PgPoolOptions::new()
@@ -89,13 +100,14 @@
 
     // Poll config: swap snapshot + wire budget/backends mỗi poll (hoặc ngay khi admin notify).
     let reload_notify = Arc::new(tokio::sync::Notify::new());
-    let (poll_budget, poll_backends, poll_cfg, poll_notify, poll_ok, poll_err) = (
+    let (poll_budget, poll_backends, poll_cfg, poll_notify, poll_ok, poll_err, poll_reload_timeout) = (
         budget.clone(),
         backends.clone(),
         cfg.clone(),
         reload_notify.clone(),
         config_ok_at.clone(),
         config_err_at.clone(),
+        reload_timeout,
     );
     let poll_loader = DbConfigLoader::new(cfg_pool, poll_secs);
     tokio::spawn(async move {
@@ -104,16 +116,20 @@
                 _ = tokio::time::sleep(Duration::from_secs(poll_secs)) => {}
                 _ = poll_notify.notified() => {}
             }
-            match poll_loader.load_snapshot().await {
-                Ok(snap) => {
+            match tokio::time::timeout(poll_reload_timeout, poll_loader.load_snapshot()).await {
+                Ok(Ok(snap)) => {
                     wire_snapshot(&poll_budget, &poll_backends, &snap);
                     poll_cfg.store(Arc::new(snap));
                     poll_ok.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
                 }
-                Err(e) => {
+                Ok(Err(e)) => {
                     eprintln!("WARN config: reload failed: {e:#}");
                     poll_err.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
                 }
+                Err(_) => {
+                    eprintln!("WARN config: reload timed out after {poll_reload_timeout:?}");
+                    poll_err.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
+                }
             }
         }
     });
@@ -162,6 +178,7 @@
         reload_notify,
         config_ok_at,
         config_err_at,
+        readiness_max_stale_ms,
     });
 
     let app = handlers::router(app_state);
--- a/src/admin/mod.rs
+++ b/src/admin/mod.rs
@@ -39,7 +39,11 @@
 impl AdminState {
     fn from_env(runtime: Arc<AppState>) -> Self {
         let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required for admin API");
-        let master_key = std::env::var("ADMIN_MASTER_KEY").unwrap_or_default();
+        let master_key =
+            std::env::var("ADMIN_MASTER_KEY").expect("ADMIN_MASTER_KEY is required for admin API");
+        if master_key.trim().is_empty() {
+            panic!("ADMIN_MASTER_KEY must not be empty");
+        }
         let allow_cidrs = std::env::var("ADMIN_ALLOW_CIDR")
             .unwrap_or_else(|_| "127.0.0.1/32".to_string())
             .split(',')
--- a/tests/streaming_integration.rs
+++ b/tests/streaming_integration.rs
@@ -121,6 +121,7 @@
         reload_notify: Arc::new(tokio::sync::Notify::new()),
         config_ok_at: Arc::new(std::sync::atomic::AtomicU64::new(1)),
         config_err_at: Arc::new(std::sync::atomic::AtomicU64::new(0)),
+        readiness_max_stale_ms: u64::MAX,
     });
     (state, prx)
 }
--- a/benchmarks/gate.sh
+++ b/benchmarks/gate.sh
@@ -14,10 +14,11 @@
 thr() { python3 -c "import tomllib,sys;t=tomllib.load(open('thresholds.toml','rb'));print(t$1)"; }
 
 # p50/p99 (ms) từ oha JSON — oha đã xuất ms
-run_oha() { # $1=url $2=payload $3=out ; in: p50 p99 rps non200
-  oha -z "$WARM" -c "$CONC" -m POST --no-tui -H 'Content-Type: application/json' -H "Authorization: Bearer $ROUTER_KEY" \
+run_oha() { # $1=url $2=payload $3=out $4=conc(optional); out: p50 p99 rps non200
+  local conc="${4:-$CONC}"
+  oha -z "$WARM" -c "$conc" -m POST --no-tui -H 'Content-Type: application/json' -H "Authorization: Bearer $ROUTER_KEY" \
       -D "payloads/$2" "$1/v1/chat/completions" >/dev/null 2>&1 || true
-  oha -z "$DUR" -c "$CONC" -m POST --no-tui --latency-correction -H 'Content-Type: application/json' -H "Authorization: Bearer $ROUTER_KEY" \
+  oha -z "$DUR" -c "$conc" -m POST --no-tui --latency-correction -H 'Content-Type: application/json' -H "Authorization: Bearer $ROUTER_KEY" \
       -D "payloads/$2" -o "$3" --output-format json "$1/v1/chat/completions" >/dev/null
   jq -r '[.latencyPercentiles.p50, .latencyPercentiles.p99, .summary.requestsPerSec, ([.statusCodeDistribution|to_entries[]|select(.key!="200")|.value]|add//0)]|@tsv' "$3"
 }
@@ -55,7 +56,7 @@
 done
 
 echo "== B6: throughput bão hoà 1k (conc 200)"
-CONC=200 read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json")
+read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json" 200)
 gate B6 "throughput rps (min)" "$(python3 -c "print(-1*$rps)")" "$(python3 -c "print(-1*$(thr "['B6']['min_rps_4core']"))")"
 gate B6 "non200" "$rn" "$(thr "['B6']['max_non200']")"
 
--- a/benchmarks/router-setup/bench/gate.sh
+++ b/benchmarks/router-setup/bench/gate.sh
@@ -14,10 +14,11 @@
 thr() { python3 -c "import tomllib,sys;t=tomllib.load(open('thresholds.toml','rb'));print(t$1)"; }
 
 # p50/p99 (ms) từ oha JSON — oha đã xuất ms
-run_oha() { # $1=url $2=payload $3=out ; in: p50 p99 rps non200
-  oha -z "$WARM" -c "$CONC" -m POST --no-tui -H 'Content-Type: application/json' -H "Authorization: Bearer $ROUTER_KEY" \
+run_oha() { # $1=url $2=payload $3=out $4=conc(optional); out: p50 p99 rps non200
+  local conc="${4:-$CONC}"
+  oha -z "$WARM" -c "$conc" -m POST --no-tui -H 'Content-Type: application/json' -H "Authorization: Bearer $ROUTER_KEY" \
       -D "payloads/$2" "$1/v1/chat/completions" >/dev/null 2>&1 || true
-  oha -z "$DUR" -c "$CONC" -m POST --no-tui --latency-correction -H 'Content-Type: application/json' -H "Authorization: Bearer $ROUTER_KEY" \
+  oha -z "$DUR" -c "$conc" -m POST --no-tui --latency-correction -H 'Content-Type: application/json' -H "Authorization: Bearer $ROUTER_KEY" \
       -D "payloads/$2" -o "$3" --output-format json "$1/v1/chat/completions" >/dev/null
   jq -r '[.latencyPercentiles.p50, .latencyPercentiles.p99, .summary.requestsPerSec, ([.statusCodeDistribution|to_entries[]|select(.key!="200")|.value]|add//0)]|@tsv' "$3"
 }
@@ -55,7 +56,7 @@
 done
 
 echo "== B6: throughput bão hoà 1k (conc 200)"
-CONC=200 read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json")
+read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json" 200)
 gate B6 "throughput rps (min)" "$(python3 -c "print(-1*$rps)")" "$(python3 -c "print(-1*$(thr "['B6']['min_rps_4core']"))")"
 gate B6 "non200" "$rn" "$(thr "['B6']['max_non200']")"
```

## Required post-apply gate bundle in the real repo

Run after applying the diff to the real worktree:

```bash
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
python3 /tmp/brigto_readyz_smoke.py
python3 /tmp/brigto_p0_smoke.py
python3 /tmp/brigto_anthropic_header_smoke.py
python3 /tmp/brigto_stream_options_false_positive.py
python3 /tmp/brigto_invalid_json_smoke.py
```

For `/tmp/brigto_invalid_json_smoke.py`, judge by printed evidence because the old helper's exit semantics are inverted after the fix. Fixed evidence is:

```text
RESULT status 400 hits 0
```

After this, rerun the benchmark gate from the fixed script and keep raw `oha` JSON artifacts. Do not claim SOTA from the old Round 7 artifact because it was non-stream only and B6 concurrency was not actually 200.
