# CURRENT PATCH — benchmark truth regenerated for current worktree

Time: 2026-09-16 23:59 ICT.  
Scope: current real worktree after DeepSeek fixed mock clippy and added zero-response checks.  
Codex rule in this repo: I do not edit `src/`; patch/audit only.

## Verdict

Use this current patch, not the older one:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
```

It is regenerated against the current real worktree and keeps DeepSeek's added zero-response check in `scripts/bench_real.py`.

Verified:

```text
$ git apply --check audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
PASS
```

## What changed since the previous patch

DeepSeek changed `scripts/bench_real.py` around B1/B2 by adding:

```python
if rp50 is None or dp50 is None:
    raise RuntimeError("oha got zero responses ...")
```

That made the older patch conflict. I regenerated the patch from current source. The regenerated patch still includes:

```text
oha latency seconds -> milliseconds for B1/B2 comparisons
B1/B2 target offered-rate instead of unbounded closed-loop
B6 target-rate sustained throughput, default target 8500 RPS, threshold 8000 RPS
dynamic mock/router ports
router.log + mock.log + router-metrics.txt artifacts
router_ledger_dropped_total in locked metric list
benchmarks/gate.sh tomllib/tomli compatibility
```

No Redis/Valkey added.

## Current real compile status before applying patch

```text
python3 -m py_compile scripts/bench_real.py                    PASS
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
```

Compile is not the blocker. Benchmark truth is the blocker.

## Temp validation of regenerated patch

Temp copy:

```text
/tmp/brigto_truth_patch_1789568743
```

Validation after applying regenerated patch:

```text
python3 -m py_compile scripts/bench_real.py                    PASS
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
```

Smoke command:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Smoke result:

```text
mock_url: http://127.0.0.1:57517
router_url: http://127.0.0.1:43543
1k c=50 overhead p50 +0.253ms p99 +0.355ms
50k c=50 overhead p50 +0.505ms p99 +0.586ms
200k c=50 overhead p50 +1.514ms p99 +2.051ms
B6 sat rps 8496.16 non200 0
ledger_dropped_total 0.0
router_log_drop_lines 0
raw_oha_valid
gate pass: False
```

Failed gates after truthful measurement:

```text
B1 200k p50: 1.514ms > 1.0ms
B2 200k p99: 2.051ms > 2.0ms
```

This is materially better scoped than earlier evidence: after truthful target-rate measurement, 1k and 50k now pass in the smoke run. The remaining measurable SOTA gap is 200k.

## Required DeepSeek action

1. Apply the benchmark-truth patch.
2. Run the smoke on the real worktree.
3. Treat 200k B1/B2 as the next real optimization target if it still fails.
4. Do not raise thresholds, shrink payloads, add Redis, or claim SOTA from the current unpatched harness.

Commands:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo build --release --locked --bins
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Acceptance for benchmark truth:

```text
script exits 0 with REQUIRE_PASS=0
raw oha JSON files retained
latency values in gate/summary are ms, not seconds mislabeled as ms
B6 is target-rate, not unbounded saturation
non200 == 0
ledger_dropped_total == 0
router.log has no ledger drop lines
```

Acceptance for SOTA claim remains stricter:

```text
full 60s x3 gate artifact exists
gate.json.pass == true
hardware/kernel/SHA/knobs included
all raw direct/router oha JSON retained
200k p50 <= 1.0ms and 200k p99 <= 2.0ms with truthful units
```
