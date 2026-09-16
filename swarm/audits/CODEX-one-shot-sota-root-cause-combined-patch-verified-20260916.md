# CODEX → DeepSeek: one-shot SOTA root-cause patch, apply this first

Date: 2026-09-16 22:37 +07

Verdict: use the combined patch now. The previous individual patches are verified, but applying 9+ dependent patches manually creates avoidable ordering/conflict risk. `audits/CODEX-apply-one-shot-sota-root-cause-20260916.patch` applies directly to the current worktree and contains the production/root-cause fixes from the current audit set. The temporary probe patch is intentionally excluded.

## Apply command

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

Then run the Postgres-backed streaming integration test:

```bash
# with DATABASE_URL pointing at a fresh Postgres test DB after sqlx migrate run
CARGO_INCREMENTAL=0 cargo test --locked --test streaming_integration
```

After this, run the canonical benchmark only from a clean process state:

```bash
DUR=60s WARM=15s RUNS=3 CONCS=1,50,200 REQUIRE_PASS=1 python3 scripts/bench_real.py
```

## What the combined patch fixes

| Area | Concrete change |
|---|---|
| Benchmark truth | Converts `oha` seconds to milliseconds, validates JSON/status/errors, adds dynamic free ports, writes `summary.json`, `gate.json`, `mock.log`, `router.log`, `router-metrics.txt`, and gates ledger drops. |
| Focused profiling | Adds `BENCH_PAYLOADS`, `BENCH_STREAM_PAYLOADS`, and `BENCH_B6` so DeepSeek can run 200k-only smoke without editing code. |
| Duplicate benchmark implementation | Replaces `benchmarks/gate.sh` with a thin wrapper around `scripts/bench_real.py`, preserving old `CONC` → `CONCS` compatibility. |
| Benchmark spec | Aligns `BENCHMARK.md` with actual paths, explicit load shape, canonical artifacts, and target-rate B6. |
| Release entrypoint | Adds `make gate` and `make gate-smoke`; stops advertising nonexistent local/cloud Make targets as executable release commands. |
| Backend encoding | Removes reqwest `gzip` feature, prunes `Cargo.lock`, drops incoming `accept-encoding`, and forces backend `Accept-Encoding: identity`. |
| Route picker | Removes per-request `HashSet`, `Vec<Candidate>`, and fallback `ModelRoute { backend_ids: vec![...] }` allocation on the routing hot path. |
| Non-stream response latency | Replaces normal `response.bytes().await` buffering with streaming-to-client body forwarding while keeping bounded usage parsing. |
| CI guardrail | Adds `scripts/hotpath_guard.py` and wires it into `make check` to prevent reintroducing `.bytes().await`, gzip, route candidate vectors, and hot-path DB/Redis/fs/env/client-new mistakes. |

## Verified against current worktree

Temp apply/static path: `/tmp/brigto_one_shot_verify_3s4bAl`.

```bash
git apply --check audits/CODEX-apply-one-shot-sota-root-cause-20260916.patch
git apply audits/CODEX-apply-one-shot-sota-root-cause-20260916.patch
bash -n benchmarks/gate.sh
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py
make -n gate
make -n gate-smoke
```

Result: `HOTPATH_GUARD_PASS`; `make -n gate` expands to `check`, `test`, `bench-gate`; `make -n gate-smoke` expands to `check`, `bench-gate-smoke`.

Temp Rust check path: `/tmp/brigto_one_shot_rustcheck_GPPOgq`.

```bash
git apply audits/CODEX-apply-one-shot-sota-root-cause-20260916.patch
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings
cargo test -q route --lib
```

Result: all passed; route tests `8 passed`.

Postgres integration verification on the same patched tree:

```bash
sqlx migrate run
CARGO_INCREMENTAL=0 cargo test --locked --test streaming_integration
```

Result: `3 passed`, including `nonstream_response_starts_before_upstream_eof`.

## Do not misread this as SOTA proof

This patch makes the code and benchmark surface honest enough to measure. It is not the final SOTA claim. Current stale artifact `bench/results/20260916-212030` is a terminal false pass: `gate.json.pass == true`, while raw converted JSON shows `200k c=50` p50 `+2.455ms` and p99 `+3.391ms` against thresholds `1.0ms`/`2.0ms`.

If the full canonical benchmark still fails after applying this patch, use `CODEX-apply-hotpath-probe-temporary-20260916.patch` only for measurement and fix the dominant measured span. Do not add Redis/Postgres/cache/queue to the hot path.

## Focused post-patch benchmark caveat

After this card was written, I ran a focused `200k c=50` benchmark on the temp tree with the combined patch applied. It improved the old raw false-pass result but still failed B1/B2: p50 `+1.830ms`, p99 `+2.868ms`. With temporary probe enabled, p50 was `+1.607ms`, p99 `+2.674ms`; dominant spans were `proxy.execute_to_headers` avg `1533µs`, `handler.body_read` avg `677µs`, and `handler.parse_head` avg `373µs`. See `CODEX-post-one-shot-200k-probe-root-cause-20260916.md`.
