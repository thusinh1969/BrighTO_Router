# NEGATIVE RESULT — naive upload streaming is not the 200k fix

Time: 2026-09-16 23:48 ICT.  
Scope: current real worktree + temp prototype.  
Codex rule in this repo: I do not edit `src/`; this is an audit result, not a source patch.

## Verdict

Do **not** blindly replace the current full-buffer request path with `reqwest::Body::wrap_stream` as the next optimization. I prototyped that in temp. It compiled and passed targeted tests, but benchmark got worse.

The immediate next action remains:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
```

Then run the truthful benchmark. Only after that optimize the measured bottleneck.

## Current real tree status

Current real tree after DeepSeek's partial fixes:

```text
python3 -m py_compile scripts/bench_real.py                    PASS
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
```

But real benchmark harness is still not trustworthy until the remaining patch is applied:

```text
scripts/bench_real.py: oha latency seconds are still treated as ms
scripts/bench_real.py: B1/B2 still unbounded closed-loop
scripts/bench_real.py: B6 still unbounded saturation
scripts/bench_real.py: ports still hard-coded 9000/8090
benchmarks/gate.sh:   still tomllib-only
src/metrics.rs:       router_ledger_dropped_total not in locked METRICS list
```

## Prototype attempted

Temp path:

```text
/tmp/brigto_fastpath_patch_1789568272
```

Prototype shape:

- applied `audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch` first;
- kept strict full-buffer path for small/ambiguous requests;
- fast-upload path only activated when:
  - `Content-Length` exists;
  - body length > 64 KiB;
  - prefix contains top-level `model`;
  - prefix contains explicit top-level `"stream": false`;
- prefix scanner tracks JSON depth and string escaping, so nested/string `model`, `stream`, `stream_options` do not count;
- reconstructs upload body as `prefix_chunks.chain(rest)` and sends via `reqwest::Body::wrap_stream`;
- no Redis/Valkey; no DB in hot path.

The prototype compiled cleanly:

```text
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
cargo test -q request_head --lib                               PASS
cargo test -q prefix_head --lib                                PASS
cargo build --release --locked --bins                          PASS
```

## Benchmark result: worse, not better

Command:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Result with benchmark-truth patch + naive fast upload prototype:

```text
1k c=50 overhead p50 +0.375ms p99 +0.535ms
50k c=50 overhead p50 +1.452ms p99 +2.458ms
200k c=50 overhead p50 +3.458ms p99 +4.824ms
B6 sat rps 8494.55 non200 0
ledger_dropped_total 0.0
router_log_drop_lines 0
gate pass: False
```

Compare to benchmark-truth patch without naive upload streaming:

```text
1k c=50 overhead p50 +0.336ms p99 +0.598ms
50k c=50 overhead p50 +0.619ms p99 +1.048ms
200k c=50 overhead p50 +1.651ms p99 +2.312ms
B6 sat rps 8497.05 non200 0
ledger_dropped_total 0.0
router_log_drop_lines 0
gate pass: False
```

The naive streaming prototype made 50k/200k overhead much worse.

## Likely reason

`reqwest::Body::wrap_stream` does not preserve an exact byte content length for the chained stream in this prototype. The current full-buffer path sends a reusable `Bytes` body where reqwest can set exact body length. Losing exact length can turn the upstream upload into chunked/streaming transfer behavior and add overhead that dominates any benefit from pipelining client upload to upstream.

Local source evidence from reqwest 0.13.5:

```text
reqwest Body::content_length():
  Inner::Reusable(bytes)  -> Some(bytes.len())
  Inner::Streaming(body)  -> body.size_hint().exact()

Body::wrap_stream(stream) wraps a TryStream; it does not let this prototype set exact byte length.
```

## Advisor instruction for DeepSeek

Do the next steps in this order.

### Step 1 — fix benchmark truth first

Apply:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
```

Run:

```bash
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo build --release --locked --bins
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Expected: harness runs truthfully and may fail B1/B2. That is acceptable; false pass is not acceptable.

### Step 2 — profile before changing request-body architecture

Do not guess. Capture one truthful 200k run and inspect where time goes:

```bash
perf record -F 999 -g -- target/release/brigto-router
# or cargo flamegraph if available
```

If perf is not practical, add temporary counters/timers around exactly these spans and remove/guard them after measuring:

```text
handler: auth+snapshot
handler: body read to Bytes
handler: RequestHead parse
handler: budget/concurrency
proxy: build reqwest request
proxy: execute until headers
proxy: response body read
proxy: usage parse + finalize
```

### Step 3 — if doing upload streaming, preserve length or prove chunked is faster

A production-worthy upload fast path must meet all of these:

```text
no DB/Redis/fs/env in request path
no full body parse/re-encode
strict fallback path remains for small/ambiguous/stream-mutating cases
fast path only when top-level model + explicit stream=false are confidently parsed from prefix
budget reserve from trusted Content-Length or full-buffer fallback
no retry after upload stream ownership is moved; document this tradeoff
upstream request preserves exact Content-Length or benchmark proves chunked upload is faster
```

Do not ship a fast path based on `wrap_stream` unless the artifact proves 50k/200k overhead improves with truthful ms units.

### Step 4 — acceptance for any 200k optimization

After any optimization patch:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Required evidence in `bench/results/<ts>/`:

```text
raw direct/router oha JSON retained
latency p50/p99 stored/compared in real ms
B6 target-rate, not unbounded saturation
non200 == 0
ledger_dropped_total == 0 or no ledger-drop log lines
200k p50/p99 improved against previous truthful baseline
```

Do not raise thresholds to hide the miss. Do not add Redis. Do not increase ledger channels as a benchmark band-aid.
