# PATCH READY — benchmark payload filter for fast root-cause profiling

Time: 2026-09-16 21:59 +07.  
Scope: applies after benchmark-truth patch.  
Codex rule in this repo: patch/audit only; do not edit `src/` directly from Codex.

## Verdict

After applying the benchmark-truth patch, apply this small harness patch:

```bash
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
```

It keeps full benchmark defaults unchanged, but adds knobs to run only the payload/gate needed for root-cause profiling. This prevents another hour-long run when the immediate question is `200k c=50`.

## Why this is needed

The current old full benchmark run started at 21:20 and was still running after 30+ minutes because it runs many payload/concurrency/run combinations with long `DUR/WARM`. That is fine for final release evidence, but bad for iterative 200k profiling.

DeepSeek needs a fast truthful loop after `CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch` lands:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 BENCH_PAYLOADS=200k BENCH_STREAM_PAYLOADS= BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

This still writes raw direct/router oha JSON, `summary.json`, `gate.json`, logs, and metrics. It just skips unrelated payloads, streaming TTFB, and B6 while profiling a specific miss.

## Patch behavior

Patch file:

```text
audits/CODEX-apply-bench-payload-filter-20260916.patch
```

Adds env knobs:

```text
BENCH_PAYLOADS         default: 1k,50k,200k
BENCH_STREAM_PAYLOADS  default: 1k-stream,50k-stream,200k-stream; empty string disables B4 loop
BENCH_B6               default: 1; set 0 to skip B6 during focused profiling
```

Validation behavior:

```text
invalid BENCH_PAYLOADS values fail fast
invalid BENCH_STREAM_PAYLOADS values fail fast
full benchmark default behavior remains unchanged
summary.json/gate.json record PAYLOADS, STREAM_PAYLOADS, RUN_B6
B6 summary becomes null only when BENCH_B6=0
```

No product code change. No Redis. No DB/hot-path change.

## Verified checks

Patch is based on the benchmark-truth script, so apply it after:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
```

Sequential validation:

```text
python3 -m py_compile scripts/bench_real.py                    PASS
```

Full sequence validated in temp:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
```

Checks after that sequence:

```text
python3 -m py_compile scripts/bench_real.py                    PASS
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
```

## Correct usage

Focused 200k profiling:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 BENCH_PAYLOADS=200k BENCH_STREAM_PAYLOADS= BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Focused 1k regression check:

```bash
DUR=5s WARM=1s RUNS=3 CONCS=50 BENCH_PAYLOADS=1k BENCH_STREAM_PAYLOADS= BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Final release/SOTA evidence:

```bash
python3 scripts/bench_real.py
```

Do not use filtered runs for final SOTA claims. Use filtered runs only to shorten the optimize-measure loop.
