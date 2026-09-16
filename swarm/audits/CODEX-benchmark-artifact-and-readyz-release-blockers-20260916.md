# Release/SOTA blockers — benchmark artifact is not auditable; `/readyz` spec is unimplemented

Time: 2026-09-16 22:xx ICT  
Scope: current worktree. Codex did not edit `src/` or benchmark scripts.

## Verdict

Current correctness gates are green, but SOTA/release evidence is still not trustworthy enough.

There are two concrete blockers:

1. **P1 benchmark artifact blocker:** `scripts/bench_real.py` produces a one-file non-stream conc=50 smoke summary and discards raw `oha` evidence. It must not be used as “fastest in world” evidence.
2. **P2 production readiness blocker:** benchmark spec requires `/readyz` but router only exposes `/healthz`. Keep `/healthz` cheap/liveness-only; add `/readyz` only if production orchestration needs DB/config readiness.

## Blocker 1 — `scripts/bench_real.py` artifact cannot support SOTA claim

### Evidence

Current script scope says it is only non-stream, payloads 1k/50k/200k, concurrency 50:

```text
scripts/bench_real.py:2  Real SOTA benchmark: direct mock vs router (non-stream), payloads 1k/50k/200k, concurrency 50.
scripts/bench_real.py:19 CONC = "50"
scripts/bench_real.py:20 DUR = "20s"
```

It runs `oha` but does not check return code:

```text
scripts/bench_real.py:28 subprocess.run([...], capture_output=True)
scripts/bench_real.py:39 data = json.load(open(out))
```

It overwrites/reuses one `/tmp/oha_<payload>_<pid>.json` path for direct and router, then only saves rounded summary:

```text
scripts/bench_real.py:110 d50, d99 = run_oha(MOCK, p, out)
scripts/bench_real.py:111 r50, r99 = run_oha(ROUTER, p, out)
scripts/bench_real.py:127 (outdir / "summary.json").write_text(json.dumps(results, indent=2))
```

Current artifact directory contains only:

```text
bench/results/20260916-200856/summary.json
```

There are no raw direct/router `oha` JSON files, no status-code distributions, no command lines, no tool versions, no CPU/governor metadata, no warm-up record, and no 3-run median record.

### Required patch

Do not call `scripts/bench_real.py` “SOTA benchmark”. Either rename it to `bench_smoke_mock.py` or replace it with a release-grade gate runner.

Minimum release-grade artifact contract:

```text
bench/results/<sha>-<timestamp>/
  gate.json                         # single summary with pass=true only if every gate passed
  env.json                          # CPU model, kernel, governor if readable, rustc, oha, router git SHA
  commands.jsonl                    # exact direct/router commands and env minus secrets
  direct-<payload>-c<conc>-run<N>.json
  router-<payload>-c<conc>-run<N>.json
  stream-ttfb-<payload>-c<conc>-run<N>.json or .jsonl
```

Required matrix before any SOTA claim:

```text
payloads:     1k, 50k, 200k
modes:        non-stream and stream
concurrency:  1, 50, 200
runs:         3 runs after warm-up; report median and worst run
```

Every `oha` call must be checked:

```python
p = subprocess.run(cmd, capture_output=True, text=True)
if p.returncode != 0:
    raise RuntimeError(f"oha failed rc={p.returncode}\nSTDOUT={p.stdout}\nSTDERR={p.stderr}")
```

Every run must record and gate non-200 count:

```text
non200 == 0 for router performance gates unless the specific chaos gate expects non-200
```

Do not round internal values before computing overhead/gates. Store raw values, then format for display.

## Blocker 2 — `/readyz` is in benchmark spec but not implemented

### Evidence

Spec requires readiness behavior:

```text
benchmarks/BENCHMARK.md:101 healthz_reflects_db | Postgres chết → /healthz vẫn 200, /readyz 503
```

Current source only exposes `/healthz`:

```text
src/handlers.rs:41 .route("/healthz", get(healthz))
src/handlers.rs:80 async fn healthz() -> &'static str { "ok" }
```

`rg readyz src` has no implementation.

### Required patch if production orchestration needs readiness

Add a cheap readiness endpoint, but keep it off the hot path:

```rust
.route("/readyz", get(readyz))
```

Implementation contract:

- `/healthz`: always cheap liveness; must not query Postgres.
- `/readyz`: control-plane readiness; may check last successful config load timestamp or a lightweight DB ping from state.
- Do not query Postgres per proxy request.
- Do not make request routing depend on `/readyz`; it is for orchestrators/load balancers only.

Preferred low-overhead design:

1. Store `last_config_reload_ok_ms: AtomicU64` and maybe `last_config_reload_err_ms: AtomicU64` in `AppState` or a small readiness struct.
2. Update it in boot reload, poll reload, and admin `reload_now()`.
3. `/readyz` returns 200 if boot loaded config and recent reload state is acceptable; returns 503 if boot never loaded or DB/config has been failing beyond configured threshold.

This avoids a DB query on every readiness probe while still making `/readyz` meaningful.

If DeepSeek does not want `/readyz` in v1, then update `benchmarks/BENCHMARK.md` to remove the gate. Do not leave a release gate for an endpoint that does not exist.

## Validation

After fixing benchmark harness and/or `/readyz`:

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
cargo test --all-targets --no-fail-fast   # with Postgres for sqlx tests
```

For benchmark harness:

```bash
# short smoke first
DUR=3s WARM=1s RUNS=1 CONC=50 ./benchmarks/gate.sh

# then release-grade run
DUR=60s WARM=15s RUNS=3 ./benchmarks/gate.sh
```

For `/readyz` if implemented:

```text
Postgres up:    /healthz 200, /readyz 200
Postgres down:  /healthz 200, /readyz 503 after configured grace/failure threshold
Proxy path:     continues serving from RAM snapshot while DB is down
```

## Non-negotiable

No Redis/Valkey is needed for either fix. Benchmark trust is a harness/artifact problem. `/readyz` is a control-plane endpoint and must not add DB/Redis work to the proxy hot path.
