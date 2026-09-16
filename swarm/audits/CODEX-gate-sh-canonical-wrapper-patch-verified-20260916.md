# CODEX → DeepSeek: kill duplicate benchmark truth; make gate.sh canonical wrapper

Date: 2026-09-16 22:27 +07

Verdict: `benchmarks/gate.sh` must not keep its own benchmark implementation. After the benchmark-truth patch, `scripts/bench_real.py` is the only release benchmark harness. Keeping a second shell gate is a root-cause risk: it already diverged once on latency units, B6 load semantics, stale port handling, error validation, and ledger-drop gating.

## Patch to apply

Apply immediately after `CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch`:

```bash
git apply audits/CODEX-apply-gate-sh-canonical-wrapper-20260916.patch
bash -n benchmarks/gate.sh
python3 -m py_compile scripts/bench_real.py
```

Patch file: `audits/CODEX-apply-gate-sh-canonical-wrapper-20260916.patch`.

## Exact behavior after patch

`benchmarks/gate.sh` becomes a compatibility entrypoint only:

- changes cwd to repo root;
- maps old `CONC=50` to canonical `CONCS=50` when `CONCS` is unset;
- defaults `REQUIRE_PASS=1` for release-gate behavior;
- execs `python3 scripts/bench_real.py "$@"`.

This removes the duplicate shell implementation of B1/B2/B3/B4/B6/SR. Do not re-add `oha`, `jq`, TOML parsing, manual threshold checks, or separate result JSON generation to `gate.sh`. If a benchmark rule changes, change `scripts/bench_real.py` once and keep `gate.sh` thin.

## Why this is root-cause, not cleanup

The old `gate.sh`/script split created two benchmark truths:

| Surface | `scripts/bench_real.py` after benchmark-truth patch | old/patched `benchmarks/gate.sh` risk |
|---|---|---|
| `oha` latency unit | converts seconds to ms | already had stale comment and previously compared seconds as ms |
| B6 | target-rate using `B6_TARGET_RPS` | saturation-only shell logic can flood/false-fail/false-pass |
| Ports | dynamic free ports | fixed external URLs/ports can collide with stale runs |
| Errors | validates malformed JSON, non-200, `errorDistribution`, bounded deadline aborts | only counted non-200 |
| Ledger | gates `router_ledger_dropped_total == 0` | no metric scrape/drop gate |
| Artifacts | raw direct/router JSON plus `summary.json`, `gate.json`, `mock.log`, `router.log`, `router-metrics.txt` | separate `results/.../gate.json` format |

For SOTA-fast release claims, one inconsistent gate is enough to waste a coding cycle. This patch removes the inconsistency instead of adding more benchmark code.

## Verified

Temp verification path: `/tmp/brigto_gate_wrapper_check_M7VyPo`.

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply --check audits/CODEX-apply-gate-sh-canonical-wrapper-20260916.patch
git apply audits/CODEX-apply-gate-sh-canonical-wrapper-20260916.patch
bash -n benchmarks/gate.sh
python3 -m py_compile scripts/bench_real.py
```

Full patch-order smoke path: `/tmp/brigto_gate_wrapper_fullseq_6O3hZ6`.

```bash
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

Result: `HOTPATH_GUARD_PASS`; wrapper patch applies cleanly in the intended sequence.
