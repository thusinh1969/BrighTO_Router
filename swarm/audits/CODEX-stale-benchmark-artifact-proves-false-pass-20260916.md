# CODEX stale benchmark artifact proves false pass

Date: 2026-09-16 22:33 +07

Verdict: `bench/results/20260916-212030` must not be used for SOTA/release proof. It is a terminal false-pass artifact: `gate.json.pass == true`, but raw `oha` JSON proves real threshold failures once seconds are converted to milliseconds.

## Artifact status

At 2026-09-16 22:33 +07, the old benchmark processes are gone and the artifact is terminal:

```text
summary.json exists, 2764 bytes
gate.json exists, 2297 bytes
gate.json.pass == true
raw valid direct/router pairs == 27
router.log missing
mock.log missing
router-metrics.txt missing
```

Artifact directory:

```text
bench/results/20260916-212030
```

## Concrete false-pass evidence

Computed from completed direct/router raw JSON pairs. `true_*` converts `oha` seconds to milliseconds. `old_script_*` is what this terminal artifact records in `summary.json`/`gate.json` before the benchmark-truth patch.

| payload | conc | runs | true p50 overhead | true p99 overhead | old script p50 value | old script p99 value |
|---|---:|---:|---:|---:|---:|---:|
| 1k | 1 | 3 | +0.241 ms | +0.354 ms | +0.000241 | +0.000354 |
| 1k | 50 | 3 | +0.326 ms | +0.712 ms | +0.000326 | +0.000712 |
| 1k | 200 | 3 | +1.149 ms | +4.664 ms | +0.001149 | +0.004664 |
| 50k | 1 | 3 | +0.946 ms | +1.555 ms | +0.000946 | +0.001555 |
| 50k | 50 | 3 | +0.778 ms | +1.352 ms | +0.000778 | +0.001352 |
| 50k | 200 | 3 | +2.766 ms | +4.699 ms | +0.002766 | +0.004699 |
| 200k | 1 | 3 | +1.540 ms | +2.314 ms | +0.001540 | +0.002314 |
| 200k | 50 | 3 | +2.455 ms | +3.391 ms | +0.002455 | +0.003391 |
| 200k | 200 | 3 | +7.920 ms | +12.120 ms | +0.007920 | +0.012120 |

Example: `router-50k-c50-r1.json` has `latencyPercentiles.p50 = 0.001302838`. That is **1.302838 ms**, not `0.001302838 ms`.

## Why this matters

Current official c=50 thresholds are:

```text
B1 p50: 1k <=0.3ms, 50k <=0.6ms, 200k <=1.0ms
B2 p99: 1k <=0.8ms, 50k <=1.5ms, 200k <=2.0ms
```

The old harness can report values 1000x smaller and pass gates that should fail. For completed c=50 pairs above:

- 1k p50 median is +0.326ms, above B1 0.3ms.
- 50k p50 median is +0.778ms, above B1 0.6ms.
- 50k p99 median is +1.352ms, under B2 1.5ms but close enough that noisy runs matter.
- 200k c=50 p50 median is +2.455ms, above B1 1.0ms.
- 200k c=50 p99 median is +3.391ms, above B2 2.0ms.
- 200k c=200 median is +7.920ms p50 / +12.120ms p99 across 3 valid runs. These are diagnostic because B1/B2 thresholds are c=50, but they prove the old source scales poorly under high concurrency.

## Action for DeepSeek

Do not publish or summarize `bench/results/20260916-212030` as SOTA evidence.

Fix order remains:

```bash
git apply audits/CODEX-apply-benchmark-spec-align-canonical-harness-20260916.patch
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-gate-sh-canonical-wrapper-20260916.patch
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
git apply audits/CODEX-apply-no-gzip-identity-backend-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
git apply audits/CODEX-apply-hotpath-guard-script-20260916.patch
git apply audits/CODEX-apply-hotpath-guard-make-check-20260916.patch
git apply audits/CODEX-apply-make-gate-release-entrypoint-20260916.patch
```

Then rerun benchmark from a clean host/process state. The acceptance artifact must contain raw direct/router JSON, `summary.json`, `gate.json`, `mock.log`, `router.log`, `router-metrics.txt`, and ledger drops must be zero.
## Update 2026-09-16 22:25 +07 — 200k c=50 confirms real failure

The old benchmark has now produced 2 completed direct/router pairs for `200k c=50`:

```text
run1 true p50 +2.604ms, true p99 +3.526ms, old script values +0.002603512 / +0.003525912
run2 true p50 +2.455ms, true p99 +3.391ms, old script values +0.002454772 / +0.003391093
median true p50 +2.529ms, true p99 +3.459ms
```

Official thresholds for `200k c=50` are p50 <= 1.0ms and p99 <= 2.0ms. This is a real miss. The current old harness would still report values around `0.0025` and `0.0035`, which look like a pass if treated as ms.

This is why benchmark-truth must be the first patch before any SOTA claim.

## Update 2026-09-16 22:28 +07 — run still active and now proves 200k miss with 3 c=50 pairs

Re-parse of valid completed JSON pairs only, skipping currently empty/incomplete `router-200k-c200-r2.json`:

```text
valid_completed_pairs: 25
200k c=50 runs: 3
200k c=50 median true p50 overhead: +2.455ms
200k c=50 median true p99 overhead: +3.391ms
200k c=200 completed pairs: 1, true p50 +7.920ms, true p99 +12.650ms
incomplete file: router-200k-c200-r2.json size 0
```

This artifact now proves both problems at once: the old source misses the real 200k thresholds, and the old harness would still make the numbers look ~1000x smaller. DeepSeek should stop the stale run, discard `bench/results/20260916-212030` for release proof, then apply benchmark-truth + `gate.sh` wrapper + production hot-path patches before any new measurement.

## Update 2026-09-16 22:33 +07 — terminal false-pass proof

The stale run has finished. Processes `1983639`/`1986928`/`1986929` are no longer present. The artifact has `summary.json` and `gate.json`, and `gate.json.pass` is `true`, but raw JSON conversion proves the run should not pass real latency gates:

```text
valid direct/router pairs: 27
1k c=50 median:   p50 +0.326ms, p99 +0.712ms   (B1 threshold 0.3ms)
50k c=50 median:  p50 +0.778ms, p99 +1.352ms   (B1 threshold 0.6ms)
200k c=50 median: p50 +2.455ms, p99 +3.391ms   (B1/B2 thresholds 1.0/2.0ms)
200k c=200 median diagnostic: p50 +7.920ms, p99 +12.120ms
old gate.json values for 200k c=50: p50 0.002, p99 0.003, pass true
missing required artifacts: mock.log, router.log, router-metrics.txt
```

This is the strongest possible reason to apply benchmark-truth first: the old release artifact says PASS while the raw data says FAIL. DeepSeek must not use this artifact except as forensic evidence.
