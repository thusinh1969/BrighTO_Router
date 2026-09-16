# CODEX NEXT — benchmark harness root cause after code fixes

Time: 2026-09-16 ICT.  
Scope: current real worktree after `/readyz`, admin key, and B6 script fixes are verified.

## Verdict

The Rust router root-cause blockers are now fixed. The next blocker is not hot-path code. It is benchmark/package truthfulness.

Current repo has three benchmark surfaces that disagree:

1. `bench/` — used by root `Makefile bench`, has payloads and a simple real-backend runner.
2. `benchmarks/` — has `BENCHMARK.md`, `thresholds.toml`, `gate.sh`, `mock_upstream.rs`, but root `gate.sh` cannot run because `benchmarks/make_payloads.py` is missing.
3. `benchmarks/router-setup/` — stale copied package: its `src/main.rs` is only a `/healthz` skeleton and its compose/Cargo still include Redis/Valkey.

This split makes SOTA artifacts untrustworthy even if router code is fast. Fix the benchmark harness once before any “fastest/SOTA” claim.

## Current hard evidence

### Root benchmark gate cannot start

Command:

```bash
cd benchmarks
ROUTER_KEY=dummy ROUTER_URL=http://127.0.0.1:1 MOCK_URL=http://127.0.0.1:1 DUR=1s WARM=1s RUNS=1 bash gate.sh
```

Result:

```text
gate.sh: line 10: ./make_payloads.py: No such file or directory
```

Root cause: `benchmarks/gate.sh:10` calls `./make_payloads.py mock-model`, but `benchmarks/` does not contain `make_payloads.py`. The generator currently exists under `bench/make_payloads.py` and `benchmarks/router-setup/bench/make_payloads.py`.

### `scripts/bench_real.py` is not a SOTA artifact generator

Specific issues:

- `scripts/bench_real.py:2` documents only non-stream 1k/50k/200k at concurrency 50.
- `scripts/bench_real.py:19-20` hardcodes `CONC = "50"`, `DUR = "20s"`; no 1/50/200 matrix, no 3-run median, no B6 true saturation, no streaming TTFB matrix.
- `scripts/bench_real.py:27-39` calls `subprocess.run(..., capture_output=True)` without `check=True` or return-code handling. A failed `oha` can be hidden until JSON open/parse, and stdout/stderr are discarded from the final artifact.
- `scripts/bench_real.py:86-95` starts router without `ADMIN_MASTER_KEY`; after the correct admin fail-fast fix, this script can fail before serving.
- `scripts/bench_real.py:109-111` uses one temp output path for direct and router, so the router run overwrites direct raw JSON. The final artifact only stores rounded summary at lines 125-127.

Treat old `bench/results/20260916-200856/summary.json` as a useful smoke only, not as SOTA proof.

### `benchmarks/router-setup/` is stale and violates Postgres-only production direction

Evidence:

- `benchmarks/router-setup/src/main.rs:1-2` literally says it is a minimal skeleton, and `src/main.rs:37` serves only `/healthz`.
- `benchmarks/router-setup/Cargo.toml:34` still has `redis = ...`.
- `benchmarks/router-setup/docker-compose.yml:2` says `postgres + valkey`, line 17 sets `REDIS_URL`, lines 24-26 make dev depend on `valkey`, and lines 45-54 define a Valkey service.
- `benchmarks/router-setup/README.md:34`, `70`, `99`, and `101` still instruct Valkey/Redis usage.

This directly contradicts the current root architecture: production default is Postgres-only; Redis/Valkey only belongs to a future optional strict multi-instance quota feature after measurement proves it is needed.

### `BENCHMARK.md` and executable gates still disagree

`benchmarks/BENCHMARK.md` demands a broad release gate: payloads 1K/50K/200K, concurrency 1/50/200, warmup 15s, duration 60s, 3 runs, B1-B11 plus A/E, with raw artifacts.

Current executable `benchmarks/gate.sh` only covers B1/B2/B3, a sequential curl B4 approximation, B6, and self-report. It does not implement B5/B7/B8/B9/B10/B11 or E-tier chaos, and it is not currently wired into root `Makefile`.

Do not solve this by deleting the spec or lowering the bar silently. Either implement the gate subset and label it honestly, or build the full release gate in phases with explicit pass/fail coverage.

## Required repair shape — no over-engineering

Pick one canonical benchmark path. Recommended:

- `benchmarks/` = official release gate and artifacts.
- `bench/` = remove, or keep only as a thin compatibility wrapper that calls `benchmarks/`.
- `benchmarks/router-setup/` = remove from source of truth or regenerate from root after the router is stable. Do not keep a skeleton router package under a benchmark/release directory.

Minimum patch set:

1. Make root benchmark gate runnable.
   - Move or copy `bench/make_payloads.py` to `benchmarks/make_payloads.py`.
   - Ensure it generates `benchmarks/payloads/{1k,50k,200k}{,-stream}.json` for model `mock-model`.

2. Wire the Rust mock correctly.
   - Either add `src/bin/mock_upstream.rs` in the real root crate and a `[[bin]] name = "llm-router-mock"` entry, or add a tiny dedicated mock crate under `benchmarks/mock/`.
   - Prefer the root `src/bin` route for lowest moving parts.
   - Do not rely on `benchmarks/router-setup` for the mock because that package is stale and has Redis baggage.

3. Add one orchestrator command for official mock benchmark.
   - Root `Makefile` should have a clear target like `gate` or `bench-gate` that builds release router + mock, starts fresh Postgres, migrates, seeds `mock-model`, starts router with `ADMIN_MASTER_KEY`, starts mock, then runs `benchmarks/gate.sh`.
   - Keep `benchmarks/gate.sh` as the measurement script; keep process orchestration outside it if simpler.

4. Fix or demote `scripts/bench_real.py`.
   - If kept, rename/describe it as smoke only until it implements the release matrix.
   - Must set `ADMIN_MASTER_KEY` when starting router.
   - Must call `subprocess.run(..., check=True, text=True, capture_output=True)` and include stderr on failure.
   - Must write separate raw files: `direct-<payload>-run<i>.json`, `router-<payload>-run<i>.json`.
   - Must not overwrite direct raw with router raw.

5. Remove active Redis/Valkey from benchmark package docs/config.
   - Delete `redis` dependency from `benchmarks/router-setup/Cargo.toml` if that directory remains.
   - Delete `REDIS_URL`, `valkey` service, and valkey dependency from `benchmarks/router-setup/docker-compose.yml` if that directory remains.
   - Update README lines that say Valkey/Redis is required.
   - Keep Redis only as optional future note in docs, not in active default commands.

6. Make artifact schema audit-friendly.
   - For every gate run, save raw `oha` JSON and a summary `gate.json` containing: git SHA, binary path/hash, CPU model, kernel, env knobs (`CONC`, `DUR`, `WARM`, `RUNS`), thresholds, per-run values, median, pass/fail.
   - If a run fails, preserve its raw stdout/stderr and return nonzero.

## Acceptance checks

Run these after patch:

```bash
# Canonical gate starts, does not fail from missing payload generator.
cd benchmarks
ROUTER_KEY=dummy ROUTER_URL=http://127.0.0.1:1 MOCK_URL=http://127.0.0.1:1 DUR=1s WARM=1s RUNS=1 bash gate.sh
# Expected next failure should be connection refused, not "./make_payloads.py: No such file".
```

```bash
# No active Redis/Valkey in default benchmark/package path.
rg -n 'redis|valkey|REDIS_URL|Valkey' Cargo.toml docker-compose.yml install benchmarks/router-setup -S
# Expected: only explicit optional-future documentation, no active dependency/service/env in default run path.
```

```bash
# Stale skeleton must not be a release/benchmark package.
rg -n 'skeleton|Khung tối thiểu|route\("/healthz"' benchmarks/router-setup/src -S
# Expected: no match, or the whole stale router-setup package is removed from source of truth.
```

```bash
# Official smoke gate, short duration. This proves orchestration and artifact writing, not SOTA numbers.
make bench-gate-smoke
# Expected: fresh result directory with raw direct/router JSON and gate.json; exit nonzero only on real threshold/connection failures.
```

Full SOTA claim requires the real-duration benchmark after this smoke:

```bash
make bench-gate DUR=60s WARM=15s RUNS=3 CONC=50
```

Then inspect the generated `gate.json` before updating `swarm/out/PROGRESS.md`.
