# CODEX → DeepSeek: benchmark contract coverage after the green full gate

Date: 2026-09-17 01:45 +07

Verdict: the current source is fast enough for the existing local mock release harness, but do not claim that the whole `benchmarks/BENCHMARK.md` contract is proven yet. The green artifact proves B1/B2/B4/B6 under the current harness, plus `ledger_drops == 0`. It does not prove every named Tier A integration test, B5/B7/B8/B9/B10 lag/B11, Tier C real local LLM, Tier D cloud, or public "fastest in the world" against named routers on the same hardware.

This is not a reason to add Redis, queues, caches, or another proxy stack. Keep the current architecture: hot path in Rust memory, exact-length request body passthrough, response streaming, async ledger off hot path. Postgres stays control-plane/ledger. Redis is only justified later if production needs cross-instance hard budget counters; it is not required for the default fastest profile.

## What is already proven on this workspace

Full release gate artifact:

```text
/mnt/data02/BrigTO_Router/bench/results/20260917-000839
```

Canonical run settings:

```text
DUR=60s WARM=15s RUNS=3 CONCS=1,50,200 BENCH_B6=1
```

Observed gate numbers:

```text
1k   c=50 overhead p50 +0.288ms p99 +0.512ms
50k  c=50 overhead p50 +0.503ms p99 +0.734ms
200k c=50 overhead p50 +0.785ms p99 +0.813ms
B4 1k   ttfb delta +0.004ms
B4 50k  ttfb delta +0.041ms
B4 200k ttfb delta -0.017ms
B6 target throughput rps 8499.48 non200 0
gate pass: True
```

Strict replay using the pending B3/worst-run patch also passed on that artifact:

```text
B1 1k   value=0.288 threshold=0.300 worst=0.291 worst_threshold=0.375 PASS
B2 1k   value=0.512 threshold=0.800 worst=0.526 worst_threshold=1.000 PASS
B1 50k  value=0.503 threshold=0.600 worst=0.534 worst_threshold=0.750 PASS
B2 50k  value=0.734 threshold=1.500 worst=0.771 worst_threshold=1.875 PASS
B1 200k value=0.785 threshold=1.000 worst=0.947 worst_threshold=1.250 PASS
B2 200k value=0.813 threshold=2.000 worst=1.639 worst_threshold=2.500 PASS
B3      value=0.497 threshold=0.800 worst=0.660 worst_threshold=1.000 PASS
```

Current test inventory is 52 tests:

```text
48 lib tests
4 streaming integration tests
```

The four integration tests currently prove:

```text
stream_request_taps_usage_and_forwards_sse
stream_without_usage_records_estimate_not_zero
large_nonstream_upload_preserves_exact_content_length
nonstream_response_starts_before_upstream_eof
```

Unit coverage already includes disabled/expired/wrong-model auth decisions, budget reserve/RPM/concurrency primitives, config polling, ledger fallback/replay, metric name lock, SSE usage parsers, no-gzip identity, route/circuit/fallback/least-load primitives, request id generation, and prefix request-head scanning.

## Contract gaps that still need code/tests

Treat these as named missing release gates, not optional polish.

### A1. HTTP-level auth/security integration tests

Add integration tests with an actual router service and mock upstream:

```text
auth_missing_key
- send a large/slow body without credentials
- expected: 401 before full body drain
- expected: mock upstream receives 0 requests

auth_model_not_allowed
- use valid key disallowed for requested model
- expected: 403
- expected: mock upstream receives 0 requests

auth_both_header_styles
- Authorization: Bearer and x-api-key both accepted
- expected: both reach upstream with same backend auth behavior

auth_client_key_not_forwarded
- upstream captures all request headers
- expected: backend Authorization key is present
- expected: client key value is absent from every forwarded header
```

Do not implement this by asserting unit decisions only. The contract is about body-read avoidance and header forwarding across the HTTP boundary.

### A2. Budget/rate-limit HTTP semantics

Add integration tests that hit the public endpoint and inspect status/headers/ledger:

```text
budget_exact_block
- configure 10,000 token team budget
- send three accepted ~3,000-token requests
- fourth request must be 429
- ledger must contain exactly the three accepted requests

budget_429_headers
- blocked response must include retry-after and x-ratelimit-remaining-tokens

rpm_limit
- configure rpm=10
- send 15 requests within one second
- exactly 5 responses must be 429

concurrency_limit
- open two streaming responses with limit=2
- third request must return 429 immediately
- it must not wait in a queue
```

Keep default budget store in RAM for fastest single-instance profile. Only add Redis when implementing `budget_shared_two_instances` as an explicit production multi-instance mode.

### A3. Ledger correctness and payload privacy

Add these tests before claiming Tier A complete:

```text
ledger_sum_equals_upstream
- 1,000 randomized mock responses with usage
- sum(input/output tokens) in ledger equals sum returned by mock exactly
- allowed error: 0

ledger_no_payload
- send request containing a unique sentinel string inside messages
- inspect Postgres usage_ledger, router logs, and fallback ledger file
- expected: sentinel string is absent everywhere

request_id_present
- response has x-router-request-id
- same id is in ledger event and JSON log

overhead_header_present
- x-router-overhead-ms exists
- compare with router-minus-direct measurement for same request class
- mismatch >30% must fail
```

### A4. Passthrough, retry, timeout, and abort behavior

Current code has primitives, but release proof needs end-to-end tests:

```text
body_bytes_identical
response_bytes_identical
chunk_forwarded_immediately
retry_on_connect_refused
retry_on_5xx_before_first_byte
no_retry_after_first_byte
no_retry_same_backend
fallback_only_when_declared
timeout_connect_2s
timeout_first_byte
timeout_idle_between_chunks
timeout_total
client_abort_cancels_upstream
body_too_large_413
malformed_json_400
graceful_shutdown_drains
healthz_reflects_db
metrics_cardinality
```

The critical ones for performance architecture are `chunk_forwarded_immediately`, `no_retry_after_first_byte`, and `client_abort_cancels_upstream`, because buffering or wrong retry behavior can make benchmarks look good while breaking streaming correctness.

### B. Performance gates not fully automated yet

Current `scripts/bench_real.py` emits B1/B2/B4/B6 and a `B10` entry that currently means `ledger_drops == 0`, not the BENCHMARK.md `ledger_lag p99 <= 2s` contract.

Required fixes:

```text
1. Apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch.
   Result: gate.json contains B3 plus worst/worst_threshold for B1/B2/B3.

2. Add baseline regression enforcement.
   Source: bench/baseline.json.
   Rule: each gate cannot regress by >10% versus previous-release baseline.
   Patch now available: CODEX-apply-benchmark-baseline-regression-gate-20260917.patch.
   Apply it after B3/worst-run; bootstrap a candidate from a reviewed green artifact only.

3. Replace current B10 name or implement real B10.
   Current behavior: ledger_drops == 0.
   Required BENCHMARK.md behavior: 99% of records inserted into Postgres <=2s after request completion at 2,000 rps for 60s.
   Patches now available: CODEX-apply-rename-false-b10-ledger-drops-gate-20260917.patch renames the current row to INTERNAL_LEDGER_DROPS, then CODEX-apply-true-b10-ledger-lag-gate-20260917.patch adds the real Postgres insert-lag p99 gate.

4. Add B5 chunk_gap_added.
   Mock emits exactly 64 chunks for 200k stream.
   Gate: p99 added gap <=0.5ms/chunk at conc=50.

5. Add B7 stream_fanout.
   1,000 concurrent 50k streams.
   Gate: no errors and RSS <= 64MB + 1.5 * sum(body in flight).

6. Add B8 memory_stable.
   50k stream, conc=200, 10 minutes.
   Gate: RSS minute 10 <= RSS minute 2 + 5%.

7. Add B9 cpu_per_request.
   50k non-stream, conc=50.
   Gate: pidstat-derived CPU/request <=150us.

8. Add B11 p99_under_pg_outage.
   1k conc=50, Postgres down for 60s mid-run.
   Gate: p99 increase <=10% versus B2.
```

Do not add B5/B7/B8/B9/B11 into the same patch as the hot-path fixes. They are harness/test work and should not perturb the already-green latency path.

### C/D/E. Real backend and chaos gates are not claimable yet

Do not claim Tier C, Tier D, or Tier E as passed until there are artifacts, not descriptions.

Required artifact rules:

```text
Tier C local real LLM
- one artifact per backend stack: vLLM/GX10 and llama-server/A6000
- include raw direct/router results and ledger sums
- include C1-C8 gate IDs in gate.json or a separate c_gate.json

Tier D cloud provider
- include exact provider/model/date
- include raw response status/body hashes, not provider secrets
- include D1-D7 gate IDs where provider is enabled

Tier E chaos
- automated mock chaos runner
- include E1-E6 gate IDs and pass/fail evidence
```

## Immediate patch order for DeepSeek

Run this order now:

```bash
git apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch
git apply audits/CODEX-apply-make-gate-postgres-test-wrapper-20260917.patch
make gate
```

Then do the next reviewable slices:

```text
Slice 1: apply CODEX-apply-benchmark-baseline-regression-gate-20260917.patch after B3/worst-run, then seed bench/baseline.json from the next reviewed green artifact only.
Slice 2: HTTP-level Tier A auth/key/body/ledger privacy tests.
Slice 3: apply CODEX-apply-rename-false-b10-ledger-drops-gate-20260917.patch, then CODEX-apply-true-b10-ledger-lag-gate-20260917.patch, and rerun the full gate.
Slice 4: B5 chunk gap + B8 memory stable harness.
Slice 5: B11 Postgres outage latency harness.
Slice 6: C/D/E artifacts only when the external backends/env are available.
```

If only one thing is done next, do `make gate` after the two pending patches. That creates the first honest one-command local release proof.
