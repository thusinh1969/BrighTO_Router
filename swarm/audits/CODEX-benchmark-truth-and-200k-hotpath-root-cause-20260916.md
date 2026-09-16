# P0 BENCHMARK TRUTH + P1 200K HOT-PATH ROOT CAUSE

Time: 2026-09-16 23:28 ICT.  
Scope: current real worktree after DeepSeek fixed mock clippy.  
Codex rule in this repo: I do not edit `src/`; patch/audit only.

## Verdict

DeepSeek fixed the mock binary clippy issue. Good. But the repo is still **not ready for any SOTA/fastest claim** because the current benchmark harness can still pass while measuring the wrong thing.

Apply the remaining benchmark-truth patch first:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
```

Verified against the current real worktree:

```text
$ git apply --check audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
PASS
```

This patch now includes the critical unit fix: `oha` latency percentiles are seconds in JSON, so the harness must multiply p50/p99 by `1000.0` before comparing to ms thresholds. Without that, B1/B2 gates are loose by 1000x.

## Current real worktree evidence

Current compile-quality gates:

```text
$ python3 -m py_compile scripts/bench_real.py
PASS

$ cargo fmt --all -- --check
PASS

$ CARGO_INCREMENTAL=0 cargo check --all-targets
PASS

$ CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
PASS
```

DeepSeek did fix `src/bin/mock_upstream.rs`; no mock clippy blocker remains.

But current real benchmark smoke proves the harness is still unsafe as a claim source:

```bash
DUR=1s WARM=1s RUNS=1 CONCS=1 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Output from the real worktree:

```text
{"message":"ledger dropping usage events (rate-limited): primary + overflow full"}
{"message":"ledger dropping usage events (rate-limited): primary + overflow full"}
B6 sat rps 119500.71 non200 0
artifacts: /mnt/data02/BrigTO_Router/bench/results/20260916-210734
gate pass: True
```

This is a hard benchmark bug: `gate pass: True` while the router is dropping ledger events. A SOTA artifact must never pass while usage ledger events are being dropped.

Raw current artifact also proves the unit bug:

```text
bench/results/20260916-210734/router-sat-c200.json:
  latencyPercentiles.p50 = 0.00132146
  summary.average        = 0.001547507
  fastest                = 0.000233455
```

Those values are seconds. The current script labels/compares them as milliseconds.

Raw direct/router 200k c=1 from the same current artifact:

```text
direct-200k-c1-r1.json: p50 0.001040489, p99 0.002705068
router-200k-c1-r1.json: p50 0.004452207, p99 0.00605457
```

Correct p50 overhead is `(0.004452207 - 0.001040489) * 1000 = 3.412ms`, not `0.003ms`. The current harness prints `+0.003ms`, so it hides a real miss.

## What the remaining patch fixes

Patch file:

```text
audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
```

Changed files:

```text
scripts/bench_real.py
benchmarks/gate.sh
src/metrics.rs
```

Behavior after patch:

```text
B1/B2 p50/p99: convert oha seconds -> milliseconds before threshold compare
B1/B2 load: target-rate per payload instead of unbounded closed loop
B6: target-rate sustained throughput, default target 8500 RPS, pass threshold 8000 RPS
Ports: dynamic free ports by default; optional MOCK_PORT / ROUTER_PORT overrides
Artifacts: router.log, mock.log, router-metrics.txt, raw oha JSON retained
Ledger: gate records router_ledger_dropped_total when present and captures router log evidence
TOML: benchmarks/gate.sh works on Python 3.10 via tomli fallback
```

No Redis/Valkey added.

## Patch validation in temp copy

After applying the benchmark-truth patch in temp, these passed:

```text
python3 -m py_compile scripts/bench_real.py                    PASS
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
cargo build --release --locked --bins                          PASS
```

Smoke after fixing units and target rates:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Truthful result:

```text
1k c=50 overhead p50 +0.336ms p99 +0.598ms
50k c=50 overhead p50 +0.619ms p99 +1.048ms
200k c=50 overhead p50 +1.651ms p99 +2.312ms
B6 sat rps 8497.05 non200 0
ledger_dropped_total 0.0
router_log_drop_lines 0
gate pass: False
```

Failed gates after truthful measurement:

```text
B1 1k    value 0.336ms threshold 0.3ms
B1 50k   value 0.619ms threshold 0.6ms
B1 200k  value 1.651ms threshold 1.0ms
B2 200k  value 2.312ms threshold 2.0ms
```

This is the correct state: the harness is no longer lying. The next work is real hot-path optimization, not benchmark cosmetics.

## Root cause for 200k overhead: full request body buffering before forward

File: `src/handlers.rs:141-222`.

Current flow:

```rust
let body_bytes = to_bytes(body, state.max_body_bytes).await?;
let head: RequestHead<'_> = serde_json::from_slice(&body_bytes)?;
let est_tokens = estimate_tokens(&body_bytes, &route);
let req = Request::from_parts(parts, body_bytes);
proxy::proxy_forward(state, req, ctx).await
```

For an 800KB prompt payload, router currently does this serially:

```text
client -> router: upload entire body into Bytes
router: parse/validate head from full body
router -> upstream: upload entire body again
```

Direct benchmark does only:

```text
client -> upstream: upload entire body once
```

That extra serialized hop is the main 200k tax. Microbench shows the `serde_json::from_slice::<RequestHead>` part alone is only about 0.118ms for the 800KB payload, so optimizing response usage parsing or small JSON details will not buy back the 1.6-3.4ms 200k miss.

Microbench evidence from temp Rust binary using the same `RequestHead` shape:

```text
1k payload    4,184 bytes:   per_parse_ms 0.001365
50k payload   200,184 bytes: per_parse_ms 0.029271
200k payload  800,184 bytes: per_parse_ms 0.117576
```

## Required architecture decision for SOTA

There is a real tension between these two properties:

1. **Strict pre-forward invalid JSON rejection**: current behavior rejects invalid JSON before any upstream hit.
2. **Fastest large-prompt routing**: router starts forwarding body to upstream before it has buffered and validated the entire prompt.

You cannot fully guarantee both for a malformed body whose first bytes contain a valid `model` but whose later bytes are invalid. If the router must prove invalid JSON never reaches upstream, it must read/validate the whole body first, and 200k overhead will include a serialized extra body hop.

For a SOTA fastest router, choose the production fast path explicitly:

- Auth before reading body stays mandatory.
- Parse only the top-level routing prefix needed for `model`, `stream`, and top-level `stream_options`.
- Estimate budget from `Content-Length` when present; fall back to counted bytes while streaming.
- Start forwarding request body to upstream as soon as routing/budget/concurrency are decided.
- Let the upstream be the full JSON validator for malformed tail bytes.
- Keep the old full-buffer strict validator only for tests or an opt-in strict mode if you insist, but do not benchmark SOTA through that path.

If the team refuses to relax strict pre-forward validation, then do not keep B1 200k threshold at 1.0ms. That threshold is measuring an architecture the code intentionally forbids.

## Concrete next patch direction after benchmark-truth patch

Do **not** spend the next patch on:

```text
Redis/Valkey
Postgres in hot path
larger ledger channel as a benchmark band-aid
narrowing benchmark payloads
raising thresholds
response-usage scanner only
```

Do this instead:

1. Change handler/proxy contract so proxy can receive `Request<Body>` for the fast path, not only `Request<Bytes>`.
2. Read only a bounded prefix before routing. Suggested first limit: 64 KiB. If model/stream cannot be determined inside prefix, fall back to current full-buffer path and mark the request as `slow_path_reason=model_not_in_prefix` in debug metrics/logs.
3. For normal OpenAI/Anthropic requests where `model` is top-level and near the start, stream the original body to upstream using `reqwest::Body::wrap_stream` or equivalent without waiting for full body collection.
4. Preserve current full-buffer path only when the body must be modified, especially OpenAI stream requests missing top-level `stream_options` where `splice_include_usage` currently edits the body tail.
5. Keep budget reservation conservative: reserve from `Content-Length / chars_per_token` when available; if missing, either full-buffer fallback or conservative reject for production policy.
6. Add a benchmark artifact field that reports `fast_path_upload=true/false` and slow path count.

Acceptance after that patch:

```bash
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo build --release --locked --bins
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Then inspect:

```text
bench/results/<ts>/gate.json
bench/results/<ts>/summary.json
bench/results/<ts>/router.log
bench/results/<ts>/router-metrics.txt
raw oha JSON files
```

Expected direction, not guaranteed pass until measured:

```text
ledger_dropped_total == 0
router_log_drop_lines == 0
B6 non200 == 0
B1/B2 values are in real ms, not seconds mislabeled as ms
200k p50 overhead should drop because request upload is pipelined instead of serialized
```

Only after this should anyone run the full 60s x3 SOTA matrix and publish numbers.
