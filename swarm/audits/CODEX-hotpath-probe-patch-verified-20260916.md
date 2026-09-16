# CODEX hot-path probe patch — temporary, verified

Date: 2026-09-16 22:05 +07

Verdict: use this only after the benchmark-truth + payload-filter + route-noalloc + nonstream-response-streaming patches. This is a temporary profiling patch, not production architecture.

## Why this exists

Current 200k non-stream latency failures need measured root cause. Guessing between body read, JSON parse, route picker, reqwest build, upstream first byte, response setup, finish ledger, and metrics will waste cycles. This patch adds low-noise aggregate spans and emits one JSON line per second when enabled.

It is disabled by default. Production behavior is unchanged unless `BRIGTO_HOTPATH_PROBE=1` is set.

## Apply order

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
git apply audits/CODEX-apply-hotpath-probe-temporary-20260916.patch
```

## Focused run

Do not run this while the old long benchmark is still using CPU.

```bash
BRIGTO_HOTPATH_PROBE=1 \
DUR=5s WARM=1s RUNS=1 CONCS=50 \
BENCH_PAYLOADS=200k BENCH_STREAM_PAYLOADS= BENCH_B6=0 \
REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Read `router.log` for lines starting with `BRIGTO_HOTPATH_PROBE `. Sort by `total_us`, not by `max_us`.

## Spans emitted

- `handler.auth_snapshot`
- `handler.body_read`
- `handler.parse_head`
- `handler.authz_route_budget`
- `handler.build_proxy_ctx`
- `proxy.acquire_initial_lease`
- `proxy.backend_lookup_clone`
- `proxy.request_body_select`
- `proxy.build_reqwest`
- `proxy.execute_to_headers`
- `proxy.forward_response_setup`
- `finish.budget_concurrency`
- `finish.ledger_event_enqueue`
- `finish.metrics_emit`

## Validation

Temp tree: `/tmp/brigto_sequence_verify_eVjK6pf3`

Sequence applied cleanly:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
git apply audits/CODEX-apply-hotpath-probe-temporary-20260916.patch
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo test -q route --lib
```

Result:

- `py_compile`: PASS
- `cargo fmt --all -- --check`: PASS
- `cargo check --all-targets`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test -q route --lib`: PASS, 8 passed

## Root-cause rule for DeepSeek

After applying this probe, fix only the span that dominates `total_us` on the 200k c=50 run. Do not add Redis, Postgres paths, side queues, caches, or architecture components for a hot-path latency issue unless the probe shows the current Rust process is no longer the bottleneck.

## Update 2026-09-16 22:12 +07

The complete current sequence was re-verified by applying patches sequentially after rebasing the benchmark-truth patch to the latest DeepSeek worktree. `py_compile`, `fmt --check`, `cargo check`, `clippy -D warnings`, and `cargo test -q route --lib` all passed in `/tmp/brigto_sequence_verify_eVjK6pf3`.
## Update 2026-09-16 22:33 +07 — full patched integration pass with fresh target

After applying benchmark-truth, payload-filter, route-noalloc, nonstream-response-streaming, and this temporary probe patch in `/tmp/brigto_sequence_verify_eVjK6pf3`, I reran integration with a fresh `CARGO_TARGET_DIR`. Result: `cargo test --test streaming_integration` passed 3/3, including `nonstream_response_starts_before_upstream_eof`.

