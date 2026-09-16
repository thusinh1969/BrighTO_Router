# CODEX → DeepSeek: implement true B10 ledger-lag gate

Date: 2026-09-17 02:55 +07

Verdict: apply this after the three harness honesty patches: B3/worst-run, baseline regression, and false-B10 rename. This patch implements the real BENCHMARK.md B10 contract instead of relying on the internal ledger-drop guard.

Patch order:

```bash
git apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch
git apply audits/CODEX-apply-benchmark-baseline-regression-gate-20260917.patch
git apply audits/CODEX-apply-rename-false-b10-ledger-drops-gate-20260917.patch
git apply audits/CODEX-apply-true-b10-ledger-lag-gate-20260917.patch
```

## Root cause

BENCHMARK.md defines B10 as:

```text
1k, 2,000 rps, 60s: 99% record vào Postgres <= 2s sau khi request xong
```

Current source cannot prove that from `usage_ledger.ts`: it is epoch seconds and represents event time, not DB insert time. The existing `router_ledger_dropped_total == 0` guard is useful, but it does not measure insert lag.

## What the patch changes

Production data path:

- Adds `UsageEvent.completed_at_ms`, set when the request finalizes.
- Adds migration `migrations/0002_ledger_lag_timestamps.sql`:
  - `completed_at_ms BIGINT NOT NULL DEFAULT 0`
  - `inserted_at_ms BIGINT NOT NULL DEFAULT clock_timestamp_ms`
  - index on `(completed_at_ms, inserted_at_ms)`
- Adds `completed_at_ms` to the background ledger insert.

Benchmark harness:

- Adds `BENCH_B10=1` default and `B10_TARGET_RPS=2000`.
- Runs a dedicated B10 phase: `1k`, `conc=200`, target rate `2,000 rps`, duration from `DUR`.
- Records phase start/end in ms.
- Waits until at least 99% of expected rows for that phase are visible in Postgres, or a 30s deadline is hit.
- Queries `percentile_cont(0.99)` over `inserted_at_ms - completed_at_ms` for rows completed in that phase.
- Emits real `B10` row:

```json
{"id":"B10","payload":"1k","conc":"2000rps","metric":"ledger_lag_p99_s"}
```

The prior ledger-drop check remains as `INTERNAL_LEDGER_DROPS` after the rename patch.

## Verification

Verified in temp tree:

```text
/tmp/brigto_true_b10_verify_v3_88y9b3ct/repo
```

Commands/results:

```text
git apply CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch                    PASS
git apply CODEX-apply-benchmark-baseline-regression-gate-20260917.patch             PASS
git apply CODEX-apply-rename-false-b10-ledger-drops-gate-20260917.patch             PASS
git apply --check CODEX-apply-true-b10-ledger-lag-gate-20260917.patch               PASS
python3 -m py_compile scripts/bench_real.py                                          PASS
cargo fmt --all -- --check                                                           PASS
cargo check --locked --all-targets                                                    PASS
```

Source assertions after patch:

```text
migrations/0002_ledger_lag_timestamps.sql exists: true
migrations/0001_init.sql unchanged for B10 timestamp columns: true
UsageEvent.completed_at_ms present: true
scripts/bench_real.py query_b10_ledger_lag present: true
```

## Required final proof after applying

Because this patch adds a real 60s B10 load phase, the final proof must be a rerun, not just compile:

```bash
BASELINE_BOOTSTRAP=1 make gate
# review and commit baseline_candidate.json as bench/baseline.json separately
make gate
```

Final artifact requirements:

```text
gate.json contains B3 with worst/worst_threshold
gate.json contains baseline status checked/pass after bench/baseline.json exists
gate.json contains INTERNAL_LEDGER_DROPS pass true
gate.json contains B10 ledger_lag_p99_s <= 2.0 with observed_rows >= min_rows
```

If B10 fails, do not move ledger writes into the request path. Tune background ledger batch size/flush cadence/channel capacity first, then remeasure.
