# PATCH V2 — benchmark truth, conc=1 fixed

Time: 2026-09-17 00:14 ICT.  
Scope: current real worktree + temp validation.  
Codex rule in this repo: I do not edit `src/`; patch/audit only.

## Verdict

Use this patch now:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
```

It is regenerated against the current real worktree and verified apply-clean:

```text
$ git apply --check audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
PASS
```

This supersedes the previous benchmark-truth patch version. The earlier version applied target-rate to every B1/B2 concurrency, including `conc=1`. That produced garbage single-connection numbers. V2 fixes that.

## Why V2 exists

A validation run with the previous truth patch and `CONCS=1,50` produced an invalid `conc=1` result:

```text
1k c=1 overhead p50 +523.100ms p99 +906.152ms
```

Root cause: `oha -q 4000 -c 1` creates client-side backlog for a single connection. That is not a useful router-minus-direct latency probe.

V2 rule:

```python
def latency_rate(payload, conc):
    if int(conc) <= 1:
        return None
    return BENCH_TARGET_RPS[payload]
```

So:

```text
conc=1: closed-loop single-request latency probe
conc>1: target offered-rate by payload, to avoid ledger overflow / false pass under unbounded mock load
B6: always target-rate sustained throughput
```

## What V2 still fixes

```text
oha latency seconds -> milliseconds before B1/B2 threshold compare
B1/B2 target-rate for conc > 1
B6 target-rate, default 8500 RPS, threshold 8000 RPS
dynamic mock/router ports by default
raw oha JSON validation with sane deadline-abort handling
router.log, mock.log, router-metrics.txt artifacts
router_ledger_dropped_total added to locked metric list
benchmarks/gate.sh tomllib/tomli compatibility and seconds->ms conversion
```

No Redis/Valkey added.

## Current real tree status before applying V2

```text
python3 -m py_compile scripts/bench_real.py                    PASS
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
```

Real tree still lacks these V2 markers:

```text
B6_TARGET_RPS: missing
BENCH_TARGET_RPS: missing
parse_oha_json: missing
free_tcp_port: missing
latency seconds -> ms for oha p50/p99: missing
benchmarks/gate.sh tomli fallback: missing
```

Compile green does not mean benchmark truth is fixed.

## Temp validation of V2

Temp path:

```text
/tmp/brigto_truth_patch_1789568743
```

Validation gates:

```text
python3 -m py_compile scripts/bench_real.py                    PASS
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
```

Smoke command:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=1,50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Smoke result:

```text
1k c=1 overhead p50 +0.221ms p99 +0.306ms
1k c=50 overhead p50 +0.252ms p99 +0.317ms
50k c=1 overhead p50 +0.546ms p99 +0.640ms
50k c=50 overhead p50 +0.525ms p99 +0.728ms
200k c=1 overhead p50 +2.618ms p99 +4.222ms
200k c=50 overhead p50 +1.459ms p99 +1.527ms
B6 sat rps 8495.05 non200 0
ledger_dropped_total 0.0
router_log_drop_lines 0
gate pass: False
```

Failed gate after truthful V2 smoke:

```text
B1 200k c=50 p50: 1.459ms > 1.0ms
```

This is now the cleanest current signal: with truthful units, sane load policy, and no ledger drops, 1k/50k/B6 pass in smoke; 200k p50 remains the next measured optimization target.

## Required DeepSeek action

Apply V2 patch and run exactly:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo build --release --locked --bins
DUR=5s WARM=1s RUNS=1 CONCS=1,50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Acceptance for benchmark truth:

```text
conc=1 numbers are sane, no target-rate backlog
B1/B2 values are real milliseconds
B6 is target-rate, not unbounded saturation
non200 == 0
ledger_dropped_total == 0
router.log has no ledger drop lines
raw oha JSON files retained and parseable
```

After this, optimize 200k p50. Do not raise threshold, shrink payload, add Redis, or ship naive `wrap_stream` upload streaming; that prototype already measured worse.
## Update 2026-09-16 22:12 +07 — patch rebased to current DeepSeek worktree

DeepSeek changed benchmark files after the previous verification, so I regenerated `audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch` against the current tree. Direct `git apply --check audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch` passes again. Dependent follow-up patches must be applied sequentially, not checked as one combined `git apply --check` command.

Verified sequence in temp tree `/tmp/brigto_sequence_verify_eVjK6pf3`:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
git apply audits/CODEX-apply-hotpath-probe-temporary-20260916.patch
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo test -q route --lib
```

Result: all passed; route tests 8/8 passed.
## Update 2026-09-16 22:33 +07 — sequence validation strengthened

Full patched sequence in `/tmp/brigto_sequence_verify_eVjK6pf3` now has fresh-target integration evidence:

- `cargo test --lib` with Postgres: PASS, 45/45
- `CARGO_TARGET_DIR=/tmp/brigto_seq_target_wCFxxWFf cargo test --test streaming_integration` with Postgres: PASS, 3/3

This removes the earlier ambiguity from shared Cargo target cache.

