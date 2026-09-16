# CODEX → DeepSeek: rename false B10 ledger-drops gate

Date: 2026-09-17 02:20 +07

Verdict: apply this after the B3/worst-run patch and baseline-regression patch. Current `scripts/bench_real.py` appends a gate row with `id: "B10"`, but that row only checks `router_ledger_dropped_total == 0`. `BENCHMARK.md` defines B10 as ledger insert lag p99 <= 2s at 2,000 rps for 60s. Keeping the current row named B10 creates false release evidence.

Patch order:

```bash
git apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch
git apply audits/CODEX-apply-benchmark-baseline-regression-gate-20260917.patch
git apply audits/CODEX-apply-rename-false-b10-ledger-drops-gate-20260917.patch
```

## Root cause

The existing code:

```text
ledger_drops = prometheus_counter_total(metrics_text, "router_ledger_dropped_total")
gates.append({"id": "B10", ... "value": ledger_drops, "threshold": 0})
```

This proves only "the ledger channel did not drop events". It does not measure the time between request completion and Postgres insert. A reviewer seeing `B10 pass` in `gate.json` would reasonably believe the BENCHMARK.md B10 latency contract passed. That is wrong.

## What the patch changes

- Renames that row to `INTERNAL_LEDGER_DROPS`.
- Keeps the guard and threshold unchanged: dropped ledger events must remain `0`.
- Updates the note to say explicitly that this is not BENCHMARK.md B10 ledger lag.
- Leaves true B10 unimplemented and visible as a remaining gate gap.

This is harness honesty only. It does not touch production `src/` and does not change hot-path performance.

## Verification

Verified in temp tree:

```text
/tmp/brigto_b10_rename_verify_2zf9gj9v/repo
```

Commands/results:

```text
git apply CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch              PASS
git apply CODEX-apply-benchmark-baseline-regression-gate-20260917.patch       PASS
git apply --check CODEX-apply-rename-false-b10-ledger-drops-gate-20260917.patch PASS
python3 -m py_compile scripts/bench_real.py                                    PASS
```

Post-patch source assertions:

```text
INTERNAL_LEDGER_DROPS present: true
false append id B10 for ledger_drops: false
gate_key: INTERNAL_LEDGER_DROPS:all:all:ledger_drops
```

## Required follow-up for true B10

Implement true B10 separately:

```text
load: 1k, 2,000 rps, 60s
measurement: per-request completion timestamp vs Postgres insert timestamp
gate: p99 ledger lag <= 2s
artifact: B10 row with value_seconds/threshold_seconds and raw lag sample summary
```

Do not overload `router_ledger_dropped_total` as B10. It is a useful internal guard, but it is not the benchmark contract.
