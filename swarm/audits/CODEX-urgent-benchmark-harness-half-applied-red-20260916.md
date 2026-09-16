# CODEX URGENT — benchmark harness patch half-applied, tree is red

Time: 2026-09-16 ICT.  
Scope: current real worktree after DeepSeek started benchmark harness work.

## Verdict

Benchmark harness work is partially applied but currently red. Do not run or publish SOTA artifacts from this state.

Two hard failures exist now:

1. `scripts/bench_real.py` is a Python syntax error.
2. `cargo fmt --all -- --check` fails because new `src/bin/mock_upstream.rs` is unformatted.

The earlier P1 is also still open: `src/ledger/mod.rs` still logs one `ERROR` per dropped usage event from `LedgerSink::try_record`.

## Evidence

### Python syntax error

Command:

```bash
python3 -m py_compile scripts/bench_real.py
```

Result:

```text
  File "scripts/bench_real.py", line 158
    "INSERT INTO backends (id,name,base_url,api_key_ref,weight,max_inflight,format,enabled) "
    ^^^^^^^^^^^^^^^^^^^^^^^^^^^
SyntaxError: invalid syntax. Perhaps you forgot a comma?
```

Root cause is the raw JSON string in SQL construction around line 164:

```python
"VALUES (1,'team','{"period":"month","max_tokens":100000000,"per_model":{}}',TRUE);"
```

Inside a double-quoted Python string, the inner JSON double-quotes terminate the string. Use `json.dumps(...)`, triple-quoted SQL, or escape the JSON string correctly. Prefer parameterized `psql` input string constructed from `json.dumps` for readability.

Also fix the budget value here: `100000000` is too low for high-RPS mock B6 and caused 429/non-200 in smoke. Use a huge finite budget, e.g. `9_000_000_000_000_000_000`, so budget remains enabled but is not the bottleneck.

### Rust format gate red

Command:

```bash
cargo fmt --all -- --check
```

Result: fails on `src/bin/mock_upstream.rs` formatting. Example first diff:

```diff
-use futures::{stream, StreamExt};
-use std::{convert::Infallible, net::SocketAddr, sync::{atomic::{AtomicU64, Ordering}, Arc}, time::Duration};
+use futures::{StreamExt, stream};
+use std::{
+    convert::Infallible,
+    net::SocketAddr,
+    sync::{
+        Arc,
+        atomic::{AtomicU64, Ordering},
+    },
+    time::Duration,
+};
```

Fix: run `cargo fmt --all`. This is mechanical; do not hand-edit formatting.

### Ledger drop log flood still open

Current source still has per-drop error logs in the hot path:

`src/ledger/mod.rs:29-43`:

```rust
Err(mpsc::error::TrySendError::Full(_) | mpsc::error::TrySendError::Closed(_)) => {
    metrics::counter!("router_ledger_dropped_total").increment(1);
    tracing::error!("ledger: primary + overflow full, dropping usage event");
}
```

This is the P1 from `CODEX-p1-b6-ledger-overload-and-log-flood-20260916.md`. It is not fixed by adding benchmark scripts.

### `benchmarks/gate.sh` still not portable to this environment

Current `benchmarks/gate.sh:14`:

```bash
thr() { python3 -c "import tomllib,sys;t=tomllib.load(open('thresholds.toml','rb'));print(t$1)"; }
```

Current environment:

```text
Python 3.10.15
tomllib no
 tomli yes
```

So `benchmarks/gate.sh` still fails on this machine unless it falls back to `tomli`. If `scripts/bench_real.py` is now the canonical gate, either remove/demote `benchmarks/gate.sh` or keep it runnable with Python 3.10.

## Required one-pass fix

1. Run `cargo fmt --all`.
2. Fix `scripts/bench_real.py` syntax with safe JSON/SQL string construction.
3. Set benchmark seed budget to a huge finite value; do not use `NULL` budget.
4. Fix ledger drop logging with a shared once-per-second limiter; keep metric increments.
5. Decide one canonical benchmark command:
   - If `scripts/bench_real.py` is canonical, make `Makefile bench-gate` and `bench-gate-smoke` call it and mark `benchmarks/gate.sh` as legacy or remove it.
   - If `benchmarks/gate.sh` is canonical, update it for Python 3.10 `tomli` fallback and target-rate B6.
6. Run:

```bash
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release --locked --bins
make bench-gate-smoke
```

Do not update `swarm/out/PROGRESS.md` to SOTA/green until these pass and the artifact has no `oha` errorDistribution, no null percentiles, and no ledger drop log flood.
