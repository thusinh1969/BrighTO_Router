# CODEX → DeepSeek: add baseline regression gate after B3/worst-run

Date: 2026-09-17 02:05 +07

Verdict: apply this only after `CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch`. `BENCHMARK.md` requires every performance number to stay within 10% of the previous-release baseline, but the current harness has no `bench/baseline.json` enforcement. Absolute thresholds alone are not enough for release.

Patch:

```bash
git apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch
git apply audits/CODEX-apply-benchmark-baseline-regression-gate-20260917.patch
```

## Root cause

`benchmarks/BENCHMARK.md` says:

```text
mỗi số không được xấu hơn baseline của release trước quá 10% (regression gate, baseline = bench/baseline.json)
```

Current `scripts/bench_real.py` only checks absolute thresholds. That allows a release to pass while being slower than the previous reviewed release. For a SOTA-fastest router, that is a release-contract bug.

## What the patch changes

- Adds `BENCH_BASELINE`, default `bench/baseline.json`.
- Adds `BASELINE_BOOTSTRAP=1` mode that writes `baseline_candidate.json` in the current artifact directory for a separate reviewed baseline PR.
- In release mode (`REQUIRE_PASS=1`), missing `bench/baseline.json` fails the gate with a clear message.
- In smoke mode (`REQUIRE_PASS=0`), missing baseline is skipped so local smoke remains useful.
- Supports lower-is-better gates: B1/B2/B3/B4/current B10 ledger drop check.
- Supports higher-is-better B6 rps using the exact 10% rule: current rps must be at least `baseline * 0.90`.
- Adds explicit B6 metric keys (`rps`, `non200`) so baseline comparison cannot collide two B6 rows with the same id/payload/conc.
- Adds `baseline` details into both `summary.json` and `gate.json`.

Near-zero/negative latency deltas get a tiny absolute slack of `0.05ms`; otherwise a baseline like `-0.017ms` would create an impossible percentage gate. Absolute thresholds still remain enforced separately.

## Verification

Verified in temp tree:

```text
/tmp/brigto_baseline_verify_v2b_yab2szj7/repo
```

Commands/results:

```text
git apply CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch      PASS
git apply --check CODEX-apply-benchmark-baseline-regression-gate.patch PASS
python3 -m py_compile scripts/bench_real.py                            PASS
```

Synthetic behavior checks passed:

```text
lower-is-better boundary: baseline 1.0, current 1.10       PASS
lower-is-better regression: baseline 1.0, current 1.101    FAIL
near-zero delta slack: baseline -0.017, current 0.034       FAIL
higher-is-better B6 boundary: baseline 100, current 90.0    PASS
higher-is-better B6 regression: baseline 100, current 89.99 FAIL
missing baseline in smoke mode REQUIRE_PASS=0              PASS / skipped
missing baseline in release mode REQUIRE_PASS=1            FAIL
BASELINE_BOOTSTRAP=1 writes baseline_candidate.json         PASS
```

I did not run the full 60s x 3 matrix for this patch because it is a stacked harness patch and the current source has not applied B3/worst yet. After applying B3/worst + this patch, the required release sequence is:

```bash
BASELINE_BOOTSTRAP=1 make gate
# review bench/results/<timestamp>/baseline_candidate.json
# commit reviewed candidate as bench/baseline.json in a separate baseline PR
make gate
```

Do not silently commit a generated baseline from a red or noisy run. The baseline must come from a reviewed green artifact.

## Remaining related issue

This patch does not fix the misleading current `B10` row. Current `B10` in `scripts/bench_real.py` means `router_ledger_dropped_total == 0`, while `BENCHMARK.md` defines B10 as ledger insert lag p99 <= 2s. Fix that separately by either implementing true B10 or renaming the current internal check so the gate ID is not false evidence.
