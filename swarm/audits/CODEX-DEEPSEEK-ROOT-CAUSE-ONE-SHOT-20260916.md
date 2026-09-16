# CODEX → DeepSeek: fix root cause một lần, không đo sai nữa

Date: 2026-09-17 01:26 +07

Verdict: chưa được claim “fastest/SOTA” bằng benchmark hiện tại. Run cũ đã terminal với `gate.json.pass == true`, nhưng raw `oha` JSON chứng minh đó là false pass do latency seconds bị report như milliseconds. Fix theo đúng thứ tự dưới đây; không thêm Redis/Postgres/cache/queue để chữa hot path latency.

## 0. Discard terminal false-pass artifact trước

Run cũ `bench/results/20260916-212030` đã kết thúc. Không còn process `python3 scripts/bench_real.py` / `llm-router-mock` / `brigto-router` của run đó. Nhưng artifact này là false pass:

- `gate.json.pass == true`.
- Raw JSON sau khi convert seconds→ms: `200k c=50` median p50 `+2.455ms`, p99 `+3.391ms`, vượt threshold `1.0ms`/`2.0ms`.
- `1k c=50` p50 `+0.326ms` cũng vượt threshold `0.3ms`.
- Missing required release artifacts: `mock.log`, `router.log`, `router-metrics.txt`.

Không dùng artifact này làm SOTA/release proof. Chỉ dùng nó như forensic proof rằng benchmark-truth patch là bắt buộc.

## 1. Use combined patch first

```bash
git apply audits/CODEX-apply-one-shot-sota-root-cause-20260916.patch
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py
bash -n benchmarks/gate.sh
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings
cargo test -q route --lib
```

This is the preferred path. It applies directly to current source and bundles benchmark truth, benchmark spec, `gate.sh` wrapper, focused benchmark knobs, no-gzip identity, route-noalloc, nonstream response streaming, hot-path guard, and `make gate`. The individual patches below are fallback/debug only if the current worktree has drifted. See `CODEX-one-shot-sota-root-cause-combined-patch-verified-20260916.md`.


## 1.6. 2026-09-16 23:16 +07 — follow-up patches after one-shot are now verified

Apply after the combined one-shot patch:

```bash
git apply audits/CODEX-apply-exact-length-streaming-upload-20260916.patch
git apply audits/CODEX-apply-small-nonstream-response-fastpath-20260916.patch
```

Measured result after both follow-ups: 50k and 200k B1/B2 pass, 1k p99 tail drops from ~44ms to <0.6ms, but full gate is still red because 1k p50 is ~0.350ms vs 0.300ms. Do not claim release/SOTA yet. Next root-cause work is only 1k p50 microspan profiling; do not add Redis/Postgres/cache/queue. See `CODEX-exact-upload-small-response-patches-verified-20260916.md`.



## 1.7. 2026-09-16 23:41 +07 — rejected near-pass prototype; do not merge Hyper yet

I tested the next obvious `1k p50` direction in a temp tree after the three verified patches: HTTP-only Hyper backend client, deferred small-response finalize, and byte-scanned non-stream usage extraction. It compiled and had one focused green run, but it failed the broader c=50 matrix and `RUNS=3` focused median.

Concrete evidence is in `CODEX-hyper-http-finalize-usage-scanner-prototype-negative-20260916.md`:

```text
Hyper-only focused:                         1k p50 +0.321ms / gate false
Hyper + deferred finalize:                  1k p50 +0.301ms / gate false
Hyper + deferred finalize + fast request-id: 1k p50 +0.301ms, p99 +0.907ms / gate false
Hyper + deferred finalize + usage scanner:  one focused pass at +0.273ms, then matrix fail at +0.325ms, RUNS=3 fail at +0.301ms
```

Decision: do **not** merge that prototype as production code. Its extra dependencies and duplicated proxy path are only justified if full gate is repeatedly green. The correct next step remains narrower: apply the three verified patches first, then add temporary microspan profiling around the remaining `1k c=50` path and keep only a patch that passes focused `RUNS=3` plus c=50 matrix.



## 1.8. 2026-09-16 23:51 +07 — minimal worker-thread patch closes remaining 1k p50

Apply after the three verified patches:

```bash
git apply audits/CODEX-apply-tokio-worker-threads-and-guard-align-20260916.patch
```

This is the current correct fix for the remaining release blocker. It changes only `#[tokio::main(worker_threads = 4)]` plus a hotpath guard correction for the bounded small-response fast path.

Evidence from `CODEX-tokio-worker-threads-guard-align-patch-verified-20260916.md`:

```text
Focused RUNS=3: 1k c=50 p50 +0.285ms p99 +0.515ms / gate true
c=50 matrix:    1k +0.275/+0.544, 50k +0.423/+0.795, 200k +0.774/+0.771 / gate true
B4 TTFB:        +0.006ms, +0.093ms, +0.068ms / gate true
B6 smoke:       8495.86 rps, non200 0 / gate true
Integration:    streaming_integration 4/4 passed
```

Do **not** merge the larger Hyper/deferred-finalize/usage-scanner prototype now. Worker threads fixed the blocker with a 37-line patch and no new proxy implementation.



## 1.9. 2026-09-17 01:18 +07 — canonical full gate is green

Current workspace passed the default canonical release gate:

```text
Artifact: /mnt/data02/BrigTO_Router/bench/results/20260917-000839
Command: python3 scripts/bench_real.py
DUR=60s WARM=15s RUNS=3 CONCS=1,50,200 BENCH_B6=1
gate pass: True
ledger_dropped_total: 0.0
```

Use `CODEX-canonical-full-release-gate-green-20260917.md` as the release proof for the current local benchmark contract. Do not add Redis/Postgres/cache/Hyper to the inference hot path after this result.



## 1.10. 2026-09-17 01:22 +07 — benchmark-contract patch required before final release artifact

After the full gate passed, I audited `scripts/bench_real.py` against `benchmarks/BENCHMARK.md` and found a harness gap: B3 `overhead_flat` and the 25% worst-run tolerance were specified but not enforced in `gate.json`.

Apply:

```bash
git apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch
```

Evidence from `CODEX-benchmark-b3-worst-run-gates-patch-verified-20260917.md`:

```text
git apply --check                                                   PASS
patched harness py_compile                                          PASS
patched smoke emits B3 + worst/worst_threshold fields                PASS
strict replay of /mnt/data02/BrigTO_Router/bench/results/20260917-000839 PASS
```

Then rerun:

```bash
python3 scripts/bench_real.py
```

Required final release artifact: new `gate.json` generated by patched harness contains B3 and worst-run fields, and `gate.json.pass == true`.







## 1.15. 2026-09-17 02:55 +07 — true B10 ledger-lag patch is ready

After renaming the fake B10 ledger-drop row, apply the true B10 patch:

```bash
git apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch
git apply audits/CODEX-apply-benchmark-baseline-regression-gate-20260917.patch
git apply audits/CODEX-apply-rename-false-b10-ledger-drops-gate-20260917.patch
git apply audits/CODEX-apply-true-b10-ledger-lag-gate-20260917.patch
```

It adds `UsageEvent.completed_at_ms`, a new migration `0002_ledger_lag_timestamps.sql` with DB-side `inserted_at_ms`, and a dedicated `1k / 2,000 rps / 60s` B10 phase querying Postgres p99 insert lag. Verification: temp tree `/tmp/brigto_true_b10_verify_v3_88y9b3ct/repo`; stacked apply-check, py_compile, fmt, and cargo check passed. See `CODEX-true-b10-ledger-lag-gate-patch-verified-20260917.md`.

## 1.14. 2026-09-17 02:20 +07 — current B10 row is false evidence; rename it

Current `scripts/bench_real.py` calls `router_ledger_dropped_total == 0` gate `B10`, but `BENCHMARK.md` defines B10 as ledger insert lag p99 <= 2s. Apply the stacked honesty patch after B3/worst-run and baseline:

```bash
git apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch
git apply audits/CODEX-apply-benchmark-baseline-regression-gate-20260917.patch
git apply audits/CODEX-apply-rename-false-b10-ledger-drops-gate-20260917.patch
```

Verification: temp tree `/tmp/brigto_b10_rename_verify_2zf9gj9v/repo`; stacked apply-check and py_compile passed. True B10 still needs a separate ledger-lag harness. See `CODEX-rename-false-b10-ledger-drops-gate-patch-verified-20260917.md`.

## 1.13. 2026-09-17 02:05 +07 — add baseline regression gate after B3/worst-run

`BENCHMARK.md` requires every performance number to stay within 10% of the previous-release baseline, but current `scripts/bench_real.py` has no `bench/baseline.json` enforcement. Apply this stacked harness patch after B3/worst-run:

```bash
git apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch
git apply audits/CODEX-apply-benchmark-baseline-regression-gate-20260917.patch
```

Then create the baseline only from a reviewed green artifact:

```bash
BASELINE_BOOTSTRAP=1 make gate
# review bench/results/<timestamp>/baseline_candidate.json
# commit reviewed candidate as bench/baseline.json in a separate baseline PR
make gate
```

Verification: temp tree `/tmp/brigto_baseline_verify_v2b_yab2szj7/repo`; stacked apply-check, py_compile, and synthetic regression/missing/bootstrap checks passed. See `CODEX-benchmark-baseline-regression-gate-patch-verified-20260917.md`.

## 1.12. 2026-09-17 01:45 +07 — full gate green is not full BENCHMARK.md coverage

Do not turn the green `/mnt/data02/BrigTO_Router/bench/results/20260917-000839` artifact into a broader claim than it proves. It proves the current local mock harness, and strict replay proves the pending B3/worst-run logic would also pass. It does not prove every named Tier A integration test, B5/B7/B8/B9/true-B10/B11, Tier C/D/E, or public "fastest in the world" versus named routers on identical hardware.

Read `CODEX-benchmark-contract-coverage-gaps-after-full-gate-20260917.md`. Required immediate sequence remains:

```bash
git apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch
git apply audits/CODEX-apply-make-gate-postgres-test-wrapper-20260917.patch
make gate
```

After that, add the missing coverage in slices: baseline regression, HTTP-level Tier A auth/key/body/ledger privacy tests, true B10 ledger lag or rename current ledger-drop check, B5/B8/B11 harnesses, then external C/D/E artifacts when environments exist. Do not add Redis/Postgres/cache/queue to the inference hot path to paper over missing tests.

## 1.11. 2026-09-17 01:26 +07 — make gate needs its own Postgres test wrapper

`make gate` currently calls `make test`, but `make test` runs `cargo test --all-targets` without `DATABASE_URL`. In a clean shell, `sqlx::test` integration tests fail before the benchmark starts. Apply:

```bash
git apply audits/CODEX-apply-make-gate-postgres-test-wrapper-20260917.patch
```

Verification: in `/tmp/brigto_make_gate_pg_light_eR9uun/repo`, `env -u DATABASE_URL CARGO_INCREMENTAL=0 ./scripts/test_postgres.sh` started isolated `pgvector/pgvector:pg16` and passed 52/52 tests. See `CODEX-make-gate-postgres-test-wrapper-patch-verified-20260917.md`.

Final release sequence now becomes:

```bash
git apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch
git apply audits/CODEX-apply-make-gate-postgres-test-wrapper-20260917.patch
make gate
```

## 2. Fallback only: individual patch sequence

```bash
git apply audits/CODEX-apply-benchmark-spec-align-canonical-harness-20260916.patch
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-gate-sh-canonical-wrapper-20260916.patch
python3 -m py_compile scripts/bench_real.py
bash -n benchmarks/gate.sh
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
```

Patch spec apply-check trực tiếp trên current worktree. Patch benchmark-truth đã được rebase against current worktree. Wrapper patch đã được apply-check sau benchmark-truth; nó bắt `benchmarks/gate.sh` gọi cùng một canonical harness (`scripts/bench_real.py`).

Nó sửa các lỗi root-cause của harness:

- `oha` JSON latency là seconds; script/gate hiện đang so với threshold ms, sai 1000x. Evidence từ raw run cũ: `router-50k-c50-r1.json` p50 `0.001302838` = 1.302838 ms, không phải 0.0013 ms.
- B6 hiện là open-loop saturation không target rate; dễ flood ledger/drop và tạo pass/fail giả.
- Ports `9000/8090` hard-code; benchmark song song/stale process sẽ contaminate nhau.
- `router_ledger_dropped_total` được emit trong ledger nhưng chưa nằm trong locked metric list/gate.
- `benchmarks/gate.sh` still carries a second B-gate implementation. After benchmark-truth, `CODEX-apply-gate-sh-canonical-wrapper-20260916.patch` makes it a thin wrapper to `scripts/bench_real.py`; do not maintain two benchmark truths.
- `benchmarks/BENCHMARK.md` still contains stale benchmark contract text (`bench/thresholds.toml`, `bench/make_payloads.py`, open-loop-for-everything, B6 saturation). Apply `CODEX-apply-benchmark-spec-align-canonical-harness-20260916.patch` so spec and harness match.
- `benchmarks/BENCHMARK.md` referenced `make gate`/`gate-local`/`gate-cloud`, but Makefile only has `bench-gate`. Apply `CODEX-apply-make-gate-release-entrypoint-20260916.patch` after spec/hot-path guard patches so release proof has one executable entrypoint.

## 3. Fallback continued: production hot-path patches, rồi đo focused

```bash
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
git apply audits/CODEX-apply-no-gzip-identity-backend-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
git apply audits/CODEX-apply-hotpath-guard-script-20260916.patch
git apply audits/CODEX-apply-hotpath-guard-make-check-20260916.patch
git apply audits/CODEX-apply-make-gate-release-entrypoint-20260916.patch
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo test -q route --lib
make -n gate
make -n gate-smoke
```

These production patches were verified sequentially after the rebased benchmark patch in `/tmp/brigto_sequence_verify_eVjK6pf3`. The full sequence including spec, gate wrapper, hot-path fixes, guard, and release entrypoint was smoke-verified in `/tmp/brigto_all_patches_recheck_ACY6ng`.

Root causes fixed by these production patches:

- `Cargo.toml` currently enables reqwest `gzip`; backend requests should force `Accept-Encoding: identity` for fastest pass-through and predictable usage parsing.
- `src/proxy/mod.rs` non-stream path currently uses `response.bytes().await`, so client response waits for full upstream EOF. That is directly hostile to 200k p50/p99.
- `src/route/mod.rs` currently clones `HashSet`, builds `Vec<Candidate>`, and builds fallback `ModelRoute { backend_ids: vec![fallback] }` on routing. That is needless hot-path allocation.

## 4. If 200k still misses, apply temporary probe and fix only measured top span

```bash
git apply audits/CODEX-apply-hotpath-probe-temporary-20260916.patch
BRIGTO_HOTPATH_PROBE=1 \
DUR=5s WARM=1s RUNS=1 CONCS=50 \
BENCH_PAYLOADS=200k BENCH_STREAM_PAYLOADS= BENCH_B6=0 \
REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Read `router.log` lines starting with `BRIGTO_HOTPATH_PROBE `. Sort by `total_us`.

Decision table:

| Dominant span | Fix |
|---|---|
| `handler.body_read` | Optimize request ingestion; preserve exact `Content-Length`; do not use naive `reqwest::Body::wrap_stream` unless exact size is proven. |
| `handler.parse_head` | Replace broad serde parsing with model/stream scanner or `simd-json` only if measured win is clear. |
| `proxy.execute_to_headers` | Bottleneck is upstream/mock/client timing, not router architecture. Do not add infra. |
| `finish.metrics_emit` | Remove per-request label string allocation or precompute/categorize labels. |
| `finish.ledger_event_enqueue` | Increase/adjust in-process channel or batcher; do not put Postgres on hot path. |

## 5. Production architecture boundary

Keep production hot path in-process:

- Request auth/config/route/budget: memory snapshot only.
- Forwarding: reqwest client pool only.
- Usage ledger: async enqueue only, Postgres writer behind it.
- Redis: no, unless multi-node global rate/concurrency becomes a measured production requirement.
- Postgres: yes for config/ledger/admin persistence, never synchronous per token/request on the latency path.

Final acceptance for this pass:

```bash
python3 -m py_compile scripts/bench_real.py
bash -n benchmarks/gate.sh
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo test --lib
DUR=60s WARM=15s RUNS=3 CONCS=1,50,200 REQUIRE_PASS=1 python3 scripts/bench_real.py
```

Do not claim SOTA until `gate.json` passes with raw direct/router JSON, `mock.log`, `router.log`, `router-metrics.txt`, and `router_ledger_dropped_total == 0`.

## Update 2026-09-16 22:27 +07 — stale benchmark is active, not idle

Process tree check showed the stale benchmark is still actively running `oha`:

```text
1983639 python3 scripts/bench_real.py
1986928 llm-router-mock      CPU ~1336%
1986929 brigto-router        CPU ~946%
2143654 oha ... -z 60s -c 200 ... direct-50k-c200-r3.json ... http://127.0.0.1:9000/v1/chat/completions
```

The run has been active for about 47 minutes and is writing `bench/results/20260916-212030`. Do not use this artifact for release/SOTA proof. Do not run focused 200k probe until these processes are gone or intentionally terminated by the coding agent that owns them.

## Update 2026-09-16 22:14 +07 — RequestHead parse measured cost

Release microbench on current payloads shows `serde_json::from_slice::<RequestHead>` costs ~141.5µs for `200k.json` (800,184 bytes). Treat parser rewrite as conditional: only do it if `BRIGTO_HOTPATH_PROBE` shows `handler.parse_head` dominates `total_us` after benchmark-truth/no-gzip/route/no-stream-response fixes. Prior scanner attempts regressed correctness/perf; see `CODEX-request-head-parse-measured-not-first-fix-20260916.md`.

## Update 2026-09-16 22:19 +07 — add hot-path guard to acceptance

I added `CODEX-apply-hotpath-guard-script-20260916.patch`. It intentionally fails current source because `.bytes().await` is still present, and passes after benchmark-truth/no-gzip/route-noalloc/nonstream-response-streaming. Use it as a regression guard after applying production hot-path fixes.

## Update 2026-09-16 22:22 +07 — guard in make check

I added `CODEX-apply-hotpath-guard-make-check-20260916.patch`; after hot-path fixes, `make check` now runs `python3 scripts/hotpath_guard.py` before fmt/clippy. Verified in `/tmp/brigto_make_guard_sequence_mpA1rR6F`.

## Update 2026-09-16 22:24 +07 — old artifact proves false pass

I parsed completed pairs in `bench/results/20260916-212030`. The old harness reports raw `oha` seconds as ms. Example medians: `50k c=50` true p50 overhead is `+0.778ms`, but old script value is `+0.000778`; `1k c=50` true p50 is `+0.326ms`, above the 0.3ms B1 threshold. See `CODEX-stale-benchmark-artifact-proves-false-pass-20260916.md`.

## Update 2026-09-16 22:22 +07 — locked build verified after Cargo.lock prune

`CODEX-apply-no-gzip-identity-backend-20260916.patch` was regenerated to include `Cargo.lock` changes. Without the lockfile change, `cargo check --locked` fails. With the regenerated patch, the full sequence passed in `/tmp/brigto_locked_sequence_fixed_eyQkRp8U`:

```bash
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
CARGO_INCREMENTAL=0 cargo build --release --locked --bins
```

Result: `HOTPATH_GUARD_PASS`, check locked PASS, release locked bins PASS.

## Update 2026-09-16 22:25 +07 — target failure quantified

`bench/results/20260916-212030` now has 2 completed `200k c=50` pairs. True converted overhead median is p50 `+2.529ms`, p99 `+3.459ms`, above B1/B2 thresholds of `1.0ms`/`2.0ms`. The old harness would display `+0.002529`/`+0.003459`, a false pass.

## Update 2026-09-16 22:27 +07 — one benchmark truth only

I added `CODEX-apply-gate-sh-canonical-wrapper-20260916.patch`. Apply it right after the benchmark-truth patch. It replaces `benchmarks/gate.sh` with a thin compatibility wrapper around `scripts/bench_real.py`, preserving old `CONC` env compatibility by mapping it to `CONCS` only when needed. Verified with `git apply --check`, `bash -n benchmarks/gate.sh`, `python3 -m py_compile scripts/bench_real.py`, and the full patch-order smoke in `/tmp/brigto_gate_wrapper_fullseq_6O3hZ6` (`HOTPATH_GUARD_PASS`).

## Update 2026-09-16 22:28 +07 — stale artifact now has 3 valid 200k c=50 pairs

`bench/results/20260916-212030` now has 25 valid direct/router pairs plus an incomplete zero-byte `router-200k-c200-r2.json`. Valid 200k c=50 median overhead is p50 `+2.455ms`, p99 `+3.391ms`, above thresholds `1.0ms`/`2.0ms`. Current source misses real 200k gate; the old harness hides it by reporting seconds as if they were ms.

## Update 2026-09-16 22:30 +07 — benchmark spec aligned with canonical harness

I added `CODEX-apply-benchmark-spec-align-canonical-harness-20260916.patch`. It updates `benchmarks/BENCHMARK.md` to match the canonical harness contract: `benchmarks/thresholds.toml`, `benchmarks/make_payloads.py`, explicit load shape (`conc=1` closed-loop, `conc=50/200` target-rate), B6 target-rate, canonical artifacts, and `gate.sh` wrapper-only. Full patch-order smoke passed in `/tmp/brigto_spec_fullseq_FaC40i` with `HOTPATH_GUARD_PASS`.

## Update 2026-09-16 22:32 +07 — executable release gate entrypoint

I added `CODEX-apply-make-gate-release-entrypoint-20260916.patch`. It adds `make gate` as ordered `check -> test -> bench-gate`, adds `make gate-smoke` as `check -> bench-gate-smoke`, and updates `BENCHMARK.md` to stop advertising nonexistent `gate-local`/`gate-cloud` targets. Full patch-order smoke passed in `/tmp/brigto_all_patches_recheck_ACY6ng`.

## Update 2026-09-16 22:33 +07 — terminal artifact is false pass

`bench/results/20260916-212030/gate.json` now exists and says `pass: true`. That result is invalid. Raw direct/router JSON gives `200k c=50` median p50 `+2.455ms` and p99 `+3.391ms`; old `gate.json` reports those as about `0.002`/`0.003` and passes them. It also lacks `mock.log`, `router.log`, and `router-metrics.txt`, so ledger-drop acceptance cannot be proven.

## Update 2026-09-16 22:37 +07 — combined apply-ready patch

I added `CODEX-apply-one-shot-sota-root-cause-20260916.patch` and verified it against the current worktree. It applies directly without the 9-patch ordering dance. Static/Rust checks passed (`py_compile`, `hotpath_guard`, `bash -n`, `fmt`, `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets`, `cargo test -q route --lib`). Postgres-backed `cargo test --locked --test streaming_integration` also passed 3/3. Use this combined patch first; keep the smaller patches only as fallback/debug.

## Update 2026-09-16 22:45 +07 — post-one-shot probe result

Focused measurement after the combined patch still misses `200k c=50`: p50 `+1.830ms`, p99 `+2.868ms`; with temporary probe: p50 `+1.607ms`, p99 `+2.674ms`. Final probe snapshot: `proxy.execute_to_headers` avg `1533µs`, `handler.body_read` avg `677µs`, `handler.parse_head` avg `373µs`; metrics/ledger/route are not root cause. Next fix should be exact-length streaming upload for non-mutated pass-through bodies, and the contract must move from full-body provider JSON validation to head-validation semantics. See `CODEX-post-one-shot-200k-probe-root-cause-20260916.md`.
