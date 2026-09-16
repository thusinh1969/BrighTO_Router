# CODEX → DeepSeek: add real release entrypoint for canonical gate

Date: 2026-09-16 22:32 +07

Verdict: `benchmarks/BENCHMARK.md` referenced `make gate`, `make gate-local`, and `make gate-cloud`, but the current Makefile only exposes `bench-gate` and `bench-gate-smoke`. A release gate that is documented but not executable is an auditability bug. Do not leave the release command split across prose and ad-hoc scripts.

## Patch to apply

Apply after `CODEX-apply-benchmark-spec-align-canonical-harness-20260916.patch`; it also works with the hot-path guard Makefile patch:

```bash
git apply audits/CODEX-apply-make-gate-release-entrypoint-20260916.patch
make -n gate
make -n gate-smoke
```

Patch file: `audits/CODEX-apply-make-gate-release-entrypoint-20260916.patch`.

## Exact behavior after patch

Makefile:

```make
gate:
	$(MAKE) check
	$(MAKE) test
	$(MAKE) bench-gate

gate-smoke:
	$(MAKE) check
	$(MAKE) bench-gate-smoke
```

`gate` is the release mock gate: static hot-path guard/fmt/clippy, tests, then canonical B benchmark. `gate-smoke` is a local fast smoke and is explicitly not release proof.

`BENCHMARK.md` is changed to stop listing nonexistent `gate-local`/`gate-cloud` commands as if they exist. Tầng C/D remain real-backend/manual until the environment and artifact contract are automated. This avoids a fake target that silently does less than the spec says.

## Why this is root-cause relevant

DeepSeek needs one command that proves the local mock release gate. Otherwise the team will keep choosing between `make bench-gate`, `benchmarks/gate.sh`, and prose instructions. That is how stale benchmark paths survived long enough to create the current false-pass artifact.

No Redis/Postgres hot-path change is involved. This is release orchestration only.

## Verified

Full patch-order smoke path: `/tmp/brigto_all_patches_recheck_ACY6ng`.

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
bash -n benchmarks/gate.sh
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py
make -n gate
make -n gate-smoke
```

Result: `HOTPATH_GUARD_PASS`; `make -n gate` expands to `check`, `test`, `bench-gate`; `make -n gate-smoke` expands to `check`, `bench-gate-smoke`.
