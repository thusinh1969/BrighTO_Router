# VERIFIED PATCH — benchmark root cause fixed as one coherent change

Time: 2026-09-16 23:08 ICT.  
Scope: current real worktree + verified temp copy.  
Codex rule in this repo: I do not edit `src/`; patch is provided under `audits/` for DeepSeek to apply.

## Verdict

Use this patch:

```bash
git apply audits/CODEX-apply-benchmark-root-cause-fix-20260916.patch
```

I verified the patch applies cleanly to the current real worktree:

```text
$ git apply --check audits/CODEX-apply-benchmark-root-cause-fix-20260916.patch
PASS
```

The patch was tested in a temp copy at `/tmp/brigto_codex_bench_patch_1789567007`. It fixes the benchmark root cause as a single coherent change:

1. `src/bin/mock_upstream.rs` passes `cargo clippy --all-targets -- -D warnings`.
2. B1/B2 latency overhead no longer runs unbounded against a ~0 ms mock; it uses explicit target RPS per payload.
3. B6 no longer runs unbounded saturation; it uses target-rate sustained throughput with default target 8500 RPS and pass threshold 8000 RPS.
4. `oha` JSON validation rejects invalid artifacts but correctly allows small normal `"aborted due to deadline"` counts at or below concurrency.
5. The harness uses dynamic free ports instead of fixed 9000/8090, so stale benchmark processes cannot poison measurements.
6. Router/mock logs and `/metrics` scrape are saved into the artifact directory.
7. `router_ledger_dropped_total` is added to the locked metric name list and the benchmark records/gates ledger drops when the metric is present.
8. `benchmarks/gate.sh` no longer requires Python 3.11 `tomllib`; it falls back to `tomli`.

No Redis/Valkey is introduced. This stays aligned with the production constraint: Postgres only unless strict multi-instance quota proves Redis is necessary.

## Current real worktree status before patch

```text
$ python3 -m py_compile scripts/bench_real.py
PASS

$ cargo fmt --all -- --check
PASS

$ CARGO_INCREMENTAL=0 cargo check --all-targets
PASS

$ CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
FAIL
src/bin/mock_upstream.rs:78  clippy::result_large_err
src/bin/mock_upstream.rs:132 clippy::collapsible_if
src/bin/mock_upstream.rs:161 clippy::collapsible_if
```

Current real `scripts/bench_real.py` also still has these measurement issues:

```text
B1/B2: oha_run(..., rate=None) -> unbounded closed-loop against mock
B6:    oha_run(router, "1k", "200", ..., rate=None) -> unbounded saturation
Ports: hard-coded 127.0.0.1:9000 and 127.0.0.1:8090
JSON:  reads latency/status fields before robust artifact validation
```

## Evidence that the patch is better

In the verified temp copy, these commands passed:

```text
python3 -m py_compile scripts/bench_real.py                         PASS
cargo fmt --all -- --check                                          PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                       PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings       PASS
cargo build --release --locked --bins                               PASS
```

Smoke command:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Smoke output summary:

```text
mock_url:   http://127.0.0.1:60825
router_url: http://127.0.0.1:46807
1k c=50 overhead p50 +0.000ms p99 +0.000ms
50k c=50 overhead p50 +0.001ms p99 +0.001ms
200k c=50 overhead p50 +0.001ms p99 +0.002ms
B4 1k ttfb delta +0.019ms
B4 50k ttfb delta -0.026ms
B4 200k ttfb delta +0.053ms
B6 sat rps 8494.37 non200 0
gate pass: True
```

Artifact inspection:

```text
gate_pass True
b6 {'target_rps': 8500, 'rps': 8494.37, 'non200': 0}
ledger_dropped_total 0.0
router_log_drop_lines 0
```

Raw `oha` files retained and validated:

```text
direct-1k-c50-r1.json      status {'200': 19997} errors {'aborted due to deadline': 3} rps 3998.83
direct-50k-c50-r1.json     status {'200': 5000}  errors {'aborted due to deadline': 1} rps 999.78
direct-200k-c50-r1.json    status {'200': 1250}  errors {'aborted due to deadline': 4} rps 250.72
router-1k-c50-r1.json      status {'200': 19999} errors {'aborted due to deadline': 3} rps 3999.15
router-50k-c50-r1.json     status {'200': 5000}  errors {'aborted due to deadline': 3} rps 1000.38
router-200k-c50-r1.json    status {'200': 1250}  errors {'aborted due to deadline': 1} rps 250.14
router-sat-c200.json       status {'200': 42491} errors {'aborted due to deadline': 1} rps 8494.37
```

## Important correction to the previous audit

Do **not** reject every non-empty `errorDistribution` from `oha`. Direct evidence shows `oha -z` normally cancels a few in-flight requests at the duration deadline even when the service is healthy:

```text
mock-only, c=50, 5s: status {'200': 1729209}, errors {'aborted due to deadline': 33}
mock-only, c=200, -q 8000, 5s: status {'200': 39994}, errors {'aborted due to deadline': 4}
```

Correct validation rule:

- fail if `statusCodeDistribution` is empty;
- fail if any error key other than `"aborted due to deadline"` appears;
- fail if deadline-abort count exceeds concurrency;
- otherwise keep the raw error count in the artifact and continue.

This still catches the previous bad artifact shape: empty status distribution with only deadline aborts.

## Patch contents

File: `audits/CODEX-apply-benchmark-root-cause-fix-20260916.patch`.

Changed files:

```text
scripts/bench_real.py
benchmarks/gate.sh
src/metrics.rs
src/bin/mock_upstream.rs
```

Key behavior after applying:

```text
B1/B2 target rates:
  BENCH_RPS_1K   default 4000
  BENCH_RPS_50K  default 1000
  BENCH_RPS_200K default 250

B6:
  B6_MIN_RPS     8000
  B6_TARGET_RPS  default 8500
  pass if actual requestsPerSec >= 8000 and non200 == 0

Ports:
  default dynamic free ports
  optional env override: MOCK_PORT / ROUTER_PORT

Artifacts:
  bench/results/<timestamp>/gate.json
  bench/results/<timestamp>/summary.json
  bench/results/<timestamp>/router.log
  bench/results/<timestamp>/mock.log
  bench/results/<timestamp>/router-metrics.txt
  raw direct/router/sat oha JSON files
```

## Required DeepSeek acceptance on the real worktree

After applying the patch, run exactly:

```bash
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo build --release --locked --bins
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Then inspect the newest artifact:

```bash
python3 - <<'PY'
import json, pathlib
root = pathlib.Path('bench/results')
latest = max([p for p in root.iterdir() if p.is_dir()], key=lambda p: p.stat().st_mtime)
print('latest', latest)
gate = json.load(open(latest / 'gate.json'))
summary = json.load(open(latest / 'summary.json'))
print('gate_pass', gate.get('pass'))
print('b6', summary.get('b6_saturation'))
print('ledger_dropped_total', summary.get('ledger_dropped_total'))
for f in sorted(latest.glob('*.json')):
    if f.name in ('gate.json', 'summary.json'):
        continue
    d = json.load(open(f))
    statuses = d.get('statusCodeDistribution') or {}
    errors = d.get('errorDistribution') or {}
    assert statuses, f'{f}: empty statusCodeDistribution; errors={errors}'
    deadline = int(errors.get('aborted due to deadline', 0))
    others = {k: v for k, v in errors.items() if k != 'aborted due to deadline' and v}
    assert not others, f'{f}: unexpected errors={others}'
    assert deadline <= 200, f'{f}: too many deadline aborts={deadline}'
print('raw_oha_valid')
print('router_log_drop_lines', sum(1 for line in open(latest / 'router.log', errors='ignore') if 'dropping usage events' in line))
PY
```

Expected smoke acceptance:

```text
all compile/clippy/build gates pass
script exits 0
gate.json and summary.json exist
gate_pass true for smoke or, at minimum with REQUIRE_PASS=0, every failed gate has explicit numeric evidence
b6.non200 == 0
ledger_dropped_total == 0
router_log_drop_lines == 0
raw oha files have non-empty statusCodeDistribution and no unexpected transport errors
```

Only after this smoke passes should DeepSeek run the full gate:

```bash
python3 scripts/bench_real.py
```

Do not claim SOTA/fastest until the full 60s x3 artifact passes and keeps raw `oha` evidence, hardware/kernel/SHA/knobs, B6 sustained throughput, and ledger-drop proof.
