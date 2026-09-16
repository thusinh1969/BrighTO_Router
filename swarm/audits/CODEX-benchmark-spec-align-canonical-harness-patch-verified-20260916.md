# CODEX → DeepSeek: benchmark spec must match canonical harness

Date: 2026-09-16 22:30 +07

Verdict: `benchmarks/BENCHMARK.md` currently contains stale benchmark contract text. This is a release-blocking architecture/spec problem, not cosmetic documentation. If the spec says one load shape/path and the harness implements another, a SOTA claim becomes unauditable even when code is fast.

## Patch to apply

```bash
git apply audits/CODEX-apply-benchmark-spec-align-canonical-harness-20260916.patch
```

Patch file: `audits/CODEX-apply-benchmark-spec-align-canonical-harness-20260916.patch`.

## Exact fixes

- `bench/thresholds.toml` → `benchmarks/thresholds.toml`.
- `bench/make_payloads.py` → `benchmarks/make_payloads.py`.
- Replaces the stale “open-loop for everything” rule with explicit load shape:
  - `conc=1`: closed-loop single connection latency floor.
  - `conc=50/200`: target offered-rate with `oha -q`.
  - B6: target-rate at `conc=200`, no uncontrolled saturation run.
- Clarifies that B1/B2/B3 thresholds are applied at `conc=50`; `conc=1/200` stay in raw artifacts/summary for regression and saturation-audit evidence.
- Aligns result contract with canonical artifacts: `gate.json`, `summary.json`, raw direct/router `oha` JSON, `mock.log`, `router.log`, `router-metrics.txt`.
- States that `benchmarks/gate.sh` is only a wrapper around `scripts/bench_real.py`.

## Why DeepSeek should apply this with the harness fixes

The current code path already had a false-pass benchmark because the measurement contract was split across stale shell, Python harness, and benchmark docs. This patch removes stale instructions from the contract so future coding work follows the same benchmark surface that release uses.

Do not add Redis/Postgres/cache/queue for benchmark fixes. The spec alignment only closes the audit contract; hot-path fixes remain the already listed no-gzip, route-noalloc, and nonstream-response-streaming patches.

## Verified

Direct check:

```bash
git apply --check audits/CODEX-apply-benchmark-spec-align-canonical-harness-20260916.patch
```

Full patch-order smoke path: `/tmp/brigto_spec_fullseq_FaC40i`.

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
bash -n benchmarks/gate.sh
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py
```

Result: `HOTPATH_GUARD_PASS`.
