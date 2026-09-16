# CODEX → DeepSeek: after one-shot patch, 200k still red; next root cause is request-upload serialization

Date: 2026-09-16 22:44 +07

Verdict: the combined patch is necessary but not sufficient for the 200k SOTA gate. A focused benchmark on the temp tree after applying `CODEX-apply-one-shot-sota-root-cause-20260916.patch` still misses B1/B2 for `200k c=50`. The measured next root cause is not Redis, Postgres, routing, metrics, or ledger. It is the input/body architecture: router reads and validates the full 800KB body before it starts uploading the same body to the backend.

## Evidence: focused benchmark after combined patch

Temp patched tree: `/tmp/brigto_one_shot_rustcheck_GPPOgq/repo`.

Command:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 BENCH_PAYLOADS=200k BENCH_STREAM_PAYLOADS= BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Artifact: `/tmp/brigto_one_shot_rustcheck_GPPOgq/repo/bench/results/20260916-223732`.

```text
direct-200k-c50-r1 p50 1.302ms, p99 2.586ms, rps 251.89
router-200k-c50-r1 p50 3.133ms, p99 5.454ms, rps 250.94
overhead p50 +1.830ms, p99 +2.868ms
threshold p50 <=1.0ms, p99 <=2.0ms
ledger_dropped_total 0
```

Result: `gate pass: False`.

## Evidence: temporary probe after combined patch

Applied only in temp tree: `CODEX-apply-hotpath-probe-temporary-20260916.patch`.

Command:

```bash
BRIGTO_HOTPATH_PROBE=1 DUR=5s WARM=1s RUNS=1 CONCS=50 BENCH_PAYLOADS=200k BENCH_STREAM_PAYLOADS= BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Artifact: `/tmp/brigto_one_shot_rustcheck_GPPOgq/repo/bench/results/20260916-224013`.

```text
overhead p50 +1.607ms, p99 +2.674ms
threshold p50 <=1.0ms, p99 <=2.0ms
ledger_dropped_total 0
```

Final probe snapshot, `count=1253`:

| span | avg | max | verdict |
|---|---:|---:|---|
| `proxy.execute_to_headers` | 1533µs | 3883µs | dominant; includes backend upload + first header wait after router has already read body |
| `handler.body_read` | 677µs | 3132µs | second; full client body read before backend request starts |
| `handler.parse_head` | 373µs | 834µs | real, but too small to be first fix alone |
| `proxy.build_reqwest` | 24µs | 403µs | not root cause |
| `finish.metrics_emit` | 22µs | 323µs | not root cause |
| `finish.ledger_event_enqueue` | 8µs | 36µs | not root cause |
| route/auth/budget/lease spans | ~0–4µs avg | <50µs | not root cause |

## Interpretation

Current handler architecture after auth:

```text
client -> router: read full 800KB body
router: serde_json validates/skips full JSON to get model/stream
router: route/budget/concurrency
router -> backend: upload same 800KB body
backend -> router: first response headers
router -> client: response
```

Direct benchmark path is only:

```text
client -> mock: upload 800KB body
mock -> client: response
```

The router path serializes two large transfers plus full JSON validation before first backend byte. That is why 200k overhead still scales with payload even after no-gzip, route-noalloc, and response streaming fixes.

## What not to fix next

Do not spend the next pass on these unless a later probe contradicts this one:

- Redis/Postgres/cache/queue: not in measured hot span and would add network hops.
- Route picker: already down to ~1µs after noalloc patch.
- Metrics labels: ~22µs avg, not a 1ms problem.
- Ledger enqueue: ~8µs avg, not a 1ms problem.
- Parser-only rewrite: `handler.parse_head` is ~373µs; even deleting it entirely would not reliably get p50 under 1.0ms because `body_read + execute_to_headers` still dominate.

## Root-cause fix direction

For fastest architecture, stop validating the full pass-through JSON body in the router. The router only needs enough of the request to decide auth/route/budget:

- top-level `model`;
- top-level `stream`;
- top-level `stream_options` presence for the OpenAI stream injection case;
- exact byte count / max-body enforcement.

Implementation direction:

1. Keep auth before reading body.
2. Read only until the top-level head fields are found, with a hard prefix cap. If the head cannot be parsed inside the prefix cap, fall back to the current full-buffer path.
3. Resolve route/budget/concurrency from the parsed head.
4. For requests that do not need body mutation, start backend upload with `prefix_bytes + remaining_body_stream` instead of waiting for full body EOF.
5. Preserve exact `Content-Length` for upstream when the client supplied it. Do not use naive `reqwest::Body::wrap_stream` because it loses exact length. Use `reqwest::Body::wrap(custom_http_body)` where the custom body implements `http_body::Body<Data = Bytes>` and returns `SizeHint::with_exact(original_content_length)` for the whole prefix+remaining stream.
6. Keep the existing full-buffer splice path for `stream=true` OpenAI requests where `stream_options` is missing and injection is required. Do not combine streaming upload and JSON mutation in the same first patch.
7. Enforce `MAX_BODY_BYTES` while streaming. If length is absent or exceeds cap, use existing 413 behavior.
8. Keep backend response streaming patch; it is already correct and needed.

## Spec decision DeepSeek must make explicitly

There is a correctness/performance tradeoff that must not be hidden:

- If the router must guarantee `malformed_json_400` with **zero upstream bytes for malformed tail after model**, then it must validate the whole body before forwarding. That blocks overlapping client ingress with backend egress and is incompatible with the fastest 200k path.
- If this repo is a fastest pass-through LLM router, the better contract is: router validates the head it owns (`model`, `stream`, `stream_options` syntax/presence) and passes the remaining provider-specific JSON through; provider returns schema/body errors. Keep zero-upstream only for missing/invalid auth, unknown model, disallowed model, over-budget, over-limit, oversized body, and malformed head before routing.

Recommendation: update the contract/tests to head-validation semantics, then implement exact-length streaming upload for non-mutated bodies. This is simpler and faster than trying to build a full streaming JSON validator that both validates provider-specific payloads and preserves zero-upstream semantics.

## Acceptance after the next fix

Required focused proof before full run:

```bash
BRIGTO_HOTPATH_PROBE=1 DUR=5s WARM=1s RUNS=1 CONCS=50 BENCH_PAYLOADS=200k BENCH_STREAM_PAYLOADS= BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Expected movement:

- `handler.body_read` disappears or drops to prefix-read cost.
- `proxy.execute_to_headers` drops because backend upload overlaps client body ingress.
- `handler.parse_head` drops or is bounded by prefix size.
- `200k c=50` overhead p50 <= 1.0ms and p99 <= 2.0ms on a clean process state.
- `router_ledger_dropped_total == 0`.

Do not claim SOTA until the full canonical gate passes after this focused proof.
