# PROFILING ENVIRONMENT — use timer spans here, not perf

Time: 2026-09-16 24:05 ICT.  
Scope: current workspace environment.  
Codex rule in this repo: I do not edit `src/`; audit only.

## Verdict

Do not block on Linux `perf` in this environment. It is not installed and kernel perf is restricted:

```text
$ perf --version
/bin/bash: perf: command not found

$ cat /proc/sys/kernel/perf_event_paranoid
3
```

For the next 200k optimization pass, use temporary application-level timing spans instead. Keep them behind an env flag or remove them after measurement.

## Exact spans to measure

Add temporary monotonic timers around these spans, then run the truthful smoke benchmark from `CODEX-current-benchmark-truth-patch-regenerated-20260916.md`:

```text
handlers.rs:
  T0 request accepted
  T1 after auth + snapshot
  T2 after body read to Bytes
  T3 after RequestHead parse
  T4 after budget reserve + concurrency guard
  T5 before proxy_forward

proxy/mod.rs:
  P0 enter proxy_forward
  P1 after backend lease + snapshot backend clone
  P2 after body clone/splice decision
  P3 after build_reqwest_request
  P4 after state.client.execute returns headers
  P5 after response.bytes() read
  P6 after parse_usage_from_body + reporter.finish
```

Report per-payload median/p99 for 1k/50k/200k in `bench/results/<ts>/router-timings.jsonl` or equivalent artifact. Do not print per request at high RPS; aggregate in memory or sample at low rate.

## Why this is the right next measurement

Current evidence after benchmark-truth patch:

```text
1k/50k smoke gates pass
200k p50/p99 fail slightly
ledger drops are 0
B6 target-rate passes
naive reqwest::Body::wrap_stream prototype worsened 50k/200k overhead
```

So the next question is not “does the benchmark run?” but “which exact span grows with 800KB request body?” Timer spans can answer that inside this restricted environment without adding Redis/Postgres work to the hot path.

## Acceptance for the timing patch

```bash
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo build --release --locked --bins
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 ROUTER_TIMING=1 python3 scripts/bench_real.py
```

Required artifact:

```text
bench/results/<ts>/router-timings.jsonl or router-timings-summary.json
per-span values for 1k, 50k, 200k
no per-request log flood
ledger_dropped_total == 0
raw oha files retained
```

Use that artifact to choose the optimization. Do not guess from code shape alone.
