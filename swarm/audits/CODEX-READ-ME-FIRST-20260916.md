# CODEX READ ME FIRST — current audit priority

Time: 2026-09-17 03:15 +07
Scope: current worktree. Codex audit rule: do not edit `src/`; write findings and exact repair guidance in `audits/`.

Use this file to avoid following stale audit items or stale `swarm/out/PROGRESS.md` claims.

## Current source of truth

Read in this order:

1. `CODEX-open-source-readme-start-script-20260917.md`
   - Public README/start script are now present. Keep benchmark claims bounded to actual artifacts and keep `start.sh` behavior aligned with current code.

2. `CODEX-true-b10-ledger-lag-gate-patch-verified-20260917.md`
   - New stacked patch after B3/baseline/B10-rename: implements true BENCHMARK.md B10 ledger-lag p99 using `completed_at_ms` and DB `inserted_at_ms`, with a dedicated 1k/2000rps phase. Compile/fmt verified; needs final full rerun after apply.

3. `CODEX-apply-true-b10-ledger-lag-gate-20260917.patch`
   - Apply after `CODEX-apply-rename-false-b10-ledger-drops-gate-20260917.patch`; adds `migrations/0002_ledger_lag_timestamps.sql` and B10 harness query.

4. `CODEX-rename-false-b10-ledger-drops-gate-patch-verified-20260917.md`
   - New stacked harness honesty patch after baseline: rename current fake `B10` ledger-drop row to `INTERNAL_LEDGER_DROPS` so true BENCHMARK.md B10 ledger lag remains visibly unimplemented.

5. `CODEX-apply-rename-false-b10-ledger-drops-gate-20260917.patch`
   - Apply after B3/worst-run and baseline-regression patches; compile verified in temp.

6. `CODEX-benchmark-baseline-regression-gate-patch-verified-20260917.md`
   - New stacked harness patch after B3/worst-run: enforces `bench/baseline.json` 10% regression gate, adds bootstrap candidate mode, and disambiguates B6 rps/non200 metrics. Apply after B3/worst before final release proof.

7. `CODEX-apply-benchmark-baseline-regression-gate-20260917.patch`
   - Apply after `CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch`; then bootstrap/review `bench/baseline.json` before release `make gate`.

8. `CODEX-benchmark-contract-coverage-gaps-after-full-gate-20260917.md`
   - Current full gate is green for the existing local mock harness, but BENCHMARK.md has broader contract gaps. This card separates proven gates from missing release proof and gives the exact next test/harness slices.

9. `CODEX-benchmark-b3-worst-run-gates-patch-verified-20260917.md`
   - Latest benchmark-contract fix: current harness missed B3 `overhead_flat` and the 25% worst-run tolerance already required by `BENCHMARK.md`. Patch apply-checks, patched smoke emits B3/worst fields, and replay of the full green artifact still passes. Apply before the next full release gate.

10. `CODEX-make-gate-postgres-test-wrapper-patch-verified-20260917.md`
   - Latest release-command fix: `make gate` currently depends on ambient `DATABASE_URL` for sqlx integration tests. Patch adds `scripts/test_postgres.sh` so `make test` starts isolated Postgres when needed and uses `--locked`.

11. `CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch`
   - Apply to `scripts/bench_real.py`; then rerun `python3 scripts/bench_real.py` so the next `gate.json` contains B3 and worst-run fields.

12. `CODEX-apply-make-gate-postgres-test-wrapper-20260917.patch`
   - Apply to `Makefile` + `scripts/test_postgres.sh`; then `make gate` becomes a real one-command release gate in a clean shell.

13. `CODEX-canonical-full-release-gate-green-20260917.md`
   - Current source passed the canonical full release gate on actual workspace under the pre-B3/worst harness: default `python3 scripts/bench_real.py`, `DUR=60s`, `WARM=15s`, `RUNS=3`, `CONCS=1,50,200`, `BENCH_B6=1`, `gate.json.pass == true`, ledger drops 0. Strict replay with B3/worst also passes, but rerun after applying the harness patch for final release proof.

14. `CODEX-current-workspace-smoke-green-root-cause-stack-20260916.md`
   - Current source contains the root-cause patch stack and passed smoke plus canonical full gate. Use it for source markers and pre-release verification history.

15. `CODEX-tokio-worker-threads-guard-align-patch-verified-20260916.md`
   - Latest verified minimal fix for remaining `1k p50`: after the one-shot + exact-upload + small-response patches, set Tokio worker threads to 4 and align the hotpath guard. Focused `RUNS=3`, c=50 payload/B4 matrix, B6 smoke, compile/clippy, and Postgres integration are green.

16. `CODEX-apply-tokio-worker-threads-and-guard-align-20260916.patch`
   - Apply after `CODEX-apply-small-nonstream-response-fastpath-20260916.patch`. It is only 37 patch lines and avoids the rejected Hyper over-engineering path.

17. `CODEX-exact-upload-small-response-patches-verified-20260916.md`
   - Verified production patch set after the one-shot patch. Apply exact-length streaming upload, then small non-stream response fast path. 50k/200k pass and 1k p99 tail is fixed; then apply the worker-thread patch to close the remaining 1k p50 miss.

18. `CODEX-hyper-http-finalize-usage-scanner-prototype-negative-20260916.md`
   - Rejected prototype for remaining `1k p50`: Hyper HTTP internal fast path + deferred finalize + usage scanner got one focused green run but failed full c=50 matrix and `RUNS=3` focused median. Do not merge; worker_threads=4 fixes the blocker with much less code.

19. `CODEX-worker-count-tuning-evidence-keep-4-20260917.md`
   - New worker-count tradeoff evidence after current source smoke-green: 6 and 8 workers reduce some `50k c=200` tail, but both break the primary `1k c=50 p50` gate. Keep 4 workers unless a new explicit c=200 p99 gate is added.

20. `CODEX-apply-exact-length-streaming-upload-20260916.patch`
   - Apply after `CODEX-apply-one-shot-sota-root-cause-20260916.patch`. Verified patch for large non-stream request upload: prefix head parse + exact Content-Length custom HttpBody + full-buffer fallback.

21. `CODEX-apply-small-nonstream-response-fastpath-20260916.patch`
   - Apply after exact-upload patch. Verified 55-line proxy patch: buffer only small known-length non-stream backend responses; keep streaming for unknown/large responses.

22. `CODEX-one-shot-sota-root-cause-combined-patch-verified-20260916.md`
   - New top priority: single apply-ready patch `audits/CODEX-apply-one-shot-sota-root-cause-20260916.patch` bundles benchmark truth, spec alignment, gate wrapper, release entrypoint, no-gzip identity, route-noalloc, nonstream response streaming, and hot-path guard. Use this first; individual patches are fallback/debug only.

23. `CODEX-post-one-shot-200k-probe-root-cause-20260916.md`
   - New measured next root cause after the combined patch: 200k c=50 still red; dominant cost is request-upload serialization (`proxy.execute_to_headers`, `handler.body_read`, `handler.parse_head`). Next fix is exact-length streaming upload for non-mutated pass-through bodies plus explicit head-validation contract.

24. `CODEX-DEEPSEEK-ROOT-CAUSE-ONE-SHOT-20260916.md`
   - Current shortest instruction for DeepSeek: apply combined patch first, then if 200k still misses use measured probe and fix request-upload serialization; do not add Redis/Postgres/cache/queue to hot path.

25. `CODEX-benchmark-spec-align-canonical-harness-patch-verified-20260916.md`
   - New release-contract patch: `benchmarks/BENCHMARK.md` must match the canonical harness, including explicit load shape and canonical artifact paths.

26. `CODEX-gate-sh-canonical-wrapper-patch-verified-20260916.md`
   - New root-cause patch: `benchmarks/gate.sh` must not maintain a second benchmark implementation. Apply `audits/CODEX-apply-gate-sh-canonical-wrapper-20260916.patch` immediately after benchmark-truth.

27. `CODEX-make-gate-release-entrypoint-patch-verified-20260916.md`
   - New release-entrypoint patch: add executable `make gate`/`make gate-smoke` and stop advertising nonexistent local/cloud gate targets.

28. `CODEX-stale-benchmark-artifact-proves-false-pass-20260916.md`
   - Terminal false-pass proof from `bench/results/20260916-212030`: `gate.json.pass == true`, but raw JSON converted to ms shows real threshold failures. Do not use that artifact for SOTA claims.

29. `CODEX-current-source-functional-green-release-red-20260916.md`
   - Current verification: compile/tests pass with correct Postgres environment, but current source still has benchmark/hot-path release blockers. The stale benchmark process is gone; its terminal artifact is a false pass.

30. `CODEX-DEEPSEEK-FIX-ROOT-CAUSE-NOW-20260916.md`
   - Immediate instruction to DeepSeek: stop using the old harness, apply the V2 benchmark-truth patch, stop stale CPU-contaminating benchmark runs before measuring, then profile the real 200k miss. This is the current top priority.

31. `CODEX-hotpath-probe-patch-verified-20260916.md`
   - Temporary measured root-cause probe for focused `200k c=50`. Apply after benchmark-truth, payload-filter, route-noalloc, and nonstream-response-streaming. Disabled unless `BRIGTO_HOTPATH_PROBE=1`; do not ship as production architecture.

32. `CODEX-no-gzip-identity-backend-patch-verified-20260916.md`
   - Verified fastest-profile dependency/header patch: remove reqwest gzip auto-decode, drop client `accept-encoding`, and force backend `Accept-Encoding: identity`.

33. `CODEX-production-dependency-boundary-verified-20260916.md`
   - Current dependency boundary: PostgreSQL yes for control/background persistence; Redis/SQLite no in default fastest runtime; stale docs must not pull them back.

34. `CODEX-request-head-parse-measured-not-first-fix-20260916.md`
   - Release microbench: current RequestHead serde parse costs ~141.5µs on 800KB `200k.json`; do not rewrite parser unless probe shows it dominates.

35. `CODEX-hotpath-allocation-and-nonstream-response-audit-20260916.md`
   - New concrete architecture audit for the next optimization pass after benchmark truth: non-stream `response.bytes().await` blocks client response, route picker allocates `HashSet`/`Vec` per request, and metrics label allocation may still sit on non-stream finalization. Measure after applying the benchmark-truth patch; do not rewrite blindly.

36. `CODEX-metrics-hotpath-alloc-guardrail-20260916.md`
   - Metrics dynamic label allocation is real but should be optimized only if probe shows `finish.metrics_emit` dominates; use retained handles/shared labels, no infra.

37. `CODEX-hotpath-guard-script-patch-verified-20260916.md`
   - Verified CI/static guard patch: fails current source, passes after hot-path/no-gzip patches, and prevents regression in fastest request path.

38. `CODEX-hotpath-guard-make-check-patch-verified-20260916.md`
   - Verified follow-up: wires `python3 scripts/hotpath_guard.py` into `make check` after hot-path fixes are applied.

39. `CODEX-route-picker-noalloc-patch-verified-20260916.md`
   - Verified follow-up patch after benchmark truth: `audits/CODEX-apply-route-picker-noalloc-20260916.patch` removes route-picker HashSet/Vec allocations on the common path. Apply only after `CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch` so impact is measured honestly.

40. `CODEX-nonstream-response-streaming-patch-verified-20260916.md`
   - Verified production-latency patch after benchmark truth: `audits/CODEX-apply-nonstream-response-streaming-20260916.patch` removes normal-path `response.bytes().await`, streams non-stream upstream body to client before EOF, and adds a delayed two-chunk integration test. Applies cleanly with benchmark-truth + route-noalloc.

41. `CODEX-upload-streaming-exact-length-guardrail-20260916.md`
   - Local reqwest 0.13.5 evidence for upload streaming: async `Body::wrap_stream` has no public sized API and content length comes from exact `size_hint`; do not retry naive upload streaming unless exact `Content-Length` is preserved and benchmarked.

42. `CODEX-bench-payload-filter-patch-verified-20260916.md`
   - Follow-up harness patch after benchmark truth: `audits/CODEX-apply-bench-payload-filter-20260916.patch` adds `BENCH_PAYLOADS`, `BENCH_STREAM_PAYLOADS`, and `BENCH_B6` for fast focused 200k profiling while keeping full-run defaults unchanged.

43. `CODEX-benchmark-truth-patch-v2-conc1-fixed-20260916.md`
   - Latest actionable patch card. `audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch` was regenerated again as V2. It fixes the previous patch's `conc=1` target-rate artifact: `conc=1` now stays closed-loop; `conc>1` uses target offered-rate; B6 remains target-rate. Patch passes `git apply --check`; temp smoke shows 1k/50k/B6 pass and only 200k p50 still fails.

44. `CODEX-current-benchmark-truth-patch-regenerated-20260916.md`
   - Superseded by `CODEX-benchmark-truth-patch-v2-conc1-fixed-20260916.md` because it applied target-rate to `conc=1`, which produced bogus single-connection numbers. Keep for history only.

45. `CODEX-profiling-environment-and-next-measurement-20260916.md`
   - Latest measurement guidance. This environment has no `perf` and `perf_event_paranoid=3`; use temporary aggregate timer spans for the 200k bottleneck instead of waiting on kernel flamegraph tooling.

46. `CODEX-fast-upload-prototype-negative-result-20260916.md`
   - Latest architecture/prototype result. A naive upload streaming fast path using `reqwest::Body::wrap_stream` compiled and passed targeted tests but worsened 50k/200k overhead. Do not implement that blindly. Apply the benchmark-truth patch first, then profile or preserve exact Content-Length if attempting upload streaming.

47. `CODEX-benchmark-truth-and-200k-hotpath-root-cause-20260916.md`
   - Latest benchmark-truth source. DeepSeek fixed mock clippy, but the real benchmark harness is still wrong: it labels `oha` seconds as ms, can pass while ledger drops occur, uses unbounded B6, and hard-codes ports. Includes real smoke evidence and the next architecture root cause: 200k overhead mainly comes from request-body handling; the naive streaming attempt in `CODEX-fast-upload-prototype-negative-result-20260916.md` shows the fix must be measured.

48. `CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch`
   - Current patch to apply after DeepSeek's partial fix. Verified with `git apply --check` against the current real worktree. Changes `scripts/bench_real.py`, `benchmarks/gate.sh`, and `src/metrics.rs`; no mock diff remains. Includes unit fix: `oha` latency seconds -> ms.

49. `CODEX-verified-benchmark-root-cause-patch-20260916.md`
   - Superseded by `CODEX-benchmark-truth-patch-v2-conc1-fixed-20260916.md` because it predates the latency unit-fix discovery. Still useful for history: small `aborted due to deadline` counts are normal for `oha -z` and should be allowed when status distribution is non-empty and count <= concurrency.

50. `CODEX-apply-benchmark-root-cause-fix-20260916.patch`
   - Superseded by `CODEX-benchmark-truth-patch-v2-conc1-fixed-20260916.md` for current worktree. The current remaining patch contains the same benchmark/root-cause fixes plus the latency unit conversion.

51. `CODEX-urgent-fix-benchmark-root-cause-once-20260916.md`
   - Superseded by `CODEX-benchmark-truth-patch-v2-conc1-fixed-20260916.md` for the exact patch and corrected `oha` validation rule. Keep as historical evidence that the benchmark patch was previously half-fixed.

52. `CODEX-p1-b6-ledger-overload-and-log-flood-20260916.md`
   - Root-cause analysis from temp validation: unbounded B6 saturation against ~0 ms mock can flood ledger overflow/drop path and produce invalid/misleading `oha` JSON. Current source has rate-limited ledger drop logging; remaining action is the benchmark-truth patch and then real hot-path optimization.

53. `CODEX-next-benchmark-harness-root-cause-20260916.md`
   - Benchmark-surface audit after earlier code fixes. Some details are superseded because DeepSeek added `benchmarks/make_payloads.py`, `src/bin/mock_upstream.rs`, and root `Makefile` targets. Still relevant for the core requirement: one canonical SOTA harness with raw evidence, no stale benchmark paths.

54. `CODEX-current-root-causes-fixed-verified-green-20260916.md`
   - Verified green before benchmark-harness edits: `/readyz`, admin master key fail-fast, and B6 explicit concurrency source bug were fixed and runtime-smoked. Keep as proof for those three specific root causes, not as proof that current benchmark evidence is valid.

55. `CODEX-urgent-benchmark-harness-half-applied-red-20260916.md`
   - Historical blocker from an earlier DeepSeek patch: syntax/fmt/clippy were red. Superseded by current compile status and `CODEX-DEEPSEEK-FIX-ROOT-CAUSE-NOW-20260916.md`.

56. `CODEX-urgent-root-cause-verified-patch-20260916.md`
    - Historical implementation patch card. Superseded for `/readyz`, admin master key, and B6 explicit concurrency by `CODEX-current-root-causes-fixed-verified-green-20260916.md`.

57. `CODEX-urgent-fix-root-cause-once-20260916.md`
    - Historical urgent note for the earlier three root causes. Superseded by `CODEX-current-root-causes-fixed-verified-green-20260916.md`.

58. `CODEX-p1-readyz-db-down-stays-ready-20260916.md`
    - Superseded by `CODEX-current-root-causes-fixed-verified-green-20260916.md`. Keep as historical root-cause proof; `/readyz` DB-down smoke passed after stale-success + timeout fix.

59. `CODEX-p1-admin-master-key-empty-misconfig-20260916.md`
    - Superseded by `CODEX-current-root-causes-fixed-verified-green-20260916.md`. Keep as historical root-cause proof; missing `ADMIN_MASTER_KEY` startup smoke passed after fail-fast fix.

60. `CODEX-benchmark-gate-b6-concurrency-bug-20260916.md`
    - Superseded for explicit `conc=200` by `CODEX-current-root-causes-fixed-verified-green-20260916.md`. The newer B6 issue is unbounded saturation and invalid artifact handling, tracked by `CODEX-DEEPSEEK-FIX-ROOT-CAUSE-NOW-20260916.md`.

61. `CODEX-benchmark-artifact-and-readyz-release-blockers-20260916.md`
    - Partially superseded by `CODEX-current-root-causes-fixed-verified-green-20260916.md` for `/readyz`; still relevant for benchmark artifact auditability.

62. `CODEX-current-anthropic-header-verified-green-20260916.md`
    - Verified: Anthropic `anthropic-version` passthrough is fixed; runtime smoke preserved client version, beta header, backend key, and stripped client auth.

63. `CODEX-current-stream-options-p1-verified-green-20260916.md`
    - Verified: `stream_options` root detection P1 is fixed and runtime smoke passed at that checkpoint.

64. `CODEX-urgent-round7-sota-progress-overclaim-20260916.md`
    - Still relevant as a claim-quality warning: Round 7 benchmark artifact was smoke only, not full SOTA proof.

65. `CODEX-current-p0-admin-scanner-verified-green-20260916.md`
    - Verified P0s: admin sync reload race fixed; malformed JSON rejected with upstream hit count 0; SQLx PATCH builder and disabled-team auth remained fixed at that checkpoint.

66. `CODEX-p1-anthropic-version-header-overwrite-20260916.md`
    - Superseded by `CODEX-current-anthropic-header-verified-green-20260916.md`. Keep as historical root-cause proof and regression smoke design.

67. `CODEX-p1-stream-options-root-detection-false-positive-20260916.md`
    - Superseded by `CODEX-current-stream-options-p1-verified-green-20260916.md`. Keep as historical root-cause proof and regression test design.

68. `CODEX-request-scanner-regression-20260916.md`
    - Superseded for production path. Keep as warning not to reintroduce a weak custom scanner; any fast path must be strict where it claims strictness and measurably faster than serde/full-buffer.

69. `CODEX-sota-architecture-current-blockers-20260916.md`
    - Architecture audit: hot path is mostly right and Postgres-only is right. Remaining measured optimization topics include request body forwarding, route-selection allocations, and benchmark evidence.

70. `CODEX-cargo-audit-verified-clean-20260916.md`
    - Security gate verified earlier: cargo-audit had 0 vulnerabilities and 0 warnings at that checkpoint.

71. `CODEX-dependency-lock-sqlx-umbrella-root-cause-20260916.md`
    - Dependency policy root cause: Redis is absent. SQLite/MySQL lock/metadata entries come from the `sqlx` umbrella crate, not a simple stale lockfile.

72. `CODEX-dependency-packaging-drift-20260916.md`
    - Still relevant for benchmark/install copy drift and Redis/Valkey wording. Superseded on the specific Cargo.lock root cause by `CODEX-dependency-lock-sqlx-umbrella-root-cause-20260916.md`.

37. Older `CODEX-*` files
    - Treat as historical context only. Re-check current source before applying any old recommendation.

## Current verified gate status

Current gates from the real worktree after DeepSeek's partial benchmark fix:

```text
python3 -m py_compile scripts/bench_real.py                    PASS
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
```

Compile/clippy is green now, but benchmark evidence is still not trustworthy until the benchmark-truth patch is applied. Real smoke on the current harness produced `gate pass: True` while `ledger dropping usage events` was logged, and raw `oha` latency seconds were mislabeled as milliseconds.

Previously verified runtime smokes that remain useful unless related source regresses:

```text
anthropic-version passthrough                    PASS: client version preserved, beta preserved, backend key inserted, client auth stripped
stream_options root detection                     PASS: normal/string/nested cases all inject root include_usage=true
invalid JSON runtime smoke                        PASS: status 400, upstream hits 0
immediate post-disable admin smoke                PASS: status 401, upstream hits unchanged, no sleep
readyz DB-down behavior                           PASS: /readyz becomes 503 after Postgres stopped while /healthz stays 200
admin master key validation                       PASS: missing ADMIN_MASTER_KEY exits before serving
```

Known proof gates still required before claiming production/SOTA ready:

```text
benchmark truth patch                             READY FOR DEEPSEEK: audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch passes git apply --check
mock upstream clippy                              FIXED IN REAL TREE: cargo clippy --all-targets -- -D warnings passes
SOTA benchmark smoke artifact                     VERIFIED IN TEMP ONLY after V2 truth patch: harness validates; conc=1 sane, 1k/50k/B6 pass, 200k p50 still fails
SOTA benchmark matrix                             OPEN: no trustworthy full 60s x3 artifact yet
latency unit correctness                          PATCH PROVIDED: oha seconds -> ms for B1/B2 in Python and shell harness
B6 target-rate sustained throughput               PATCH PROVIDED: target-rate B6, default target 8500, threshold 8000
benchmark artifact validation                     PATCH PROVIDED: validates status/latency/rps and allows only normal deadline aborts <= concurrency
benchmarks/gate.sh Python compatibility           PATCH PROVIDED: tomllib with tomli fallback
200k hot-path architecture                         PATCH READY: exact-length streaming upload patch verified; apply after one-shot and keep full-buffer fallback for ambiguous/mutating cases
Cargo.lock dependency policy                       DECIDE: sqlx umbrella keeps sqlx-sqlite/libsqlite3-sys/sqlx-mysql in metadata/lock; not runtime Redis/DB hot-path
benchmark/install doc drift                        OPEN: re-check Redis/Valkey wording in copied harnesses
```

## Non-negotiable architecture constraints

- Production default: Postgres only. Redis/Valkey only if measured strict multi-instance quota requires it.
- Hot request path: no DB, Redis, filesystem, env reads, per-request client creation, full `serde_json::Value` parse/re-encode, or unbounded stream channel.
- Admin path may read DB/reload config synchronously; it is not the latency-critical proxy path.
- `/readyz` may report control-plane DB/config health, but it must not add DB work to proxy requests.
- Do not claim SOTA/fastest until benchmark artifacts prove router-minus-direct overhead across 1K/50K/200K bodies and relevant concurrencies with truthful units and raw evidence.
