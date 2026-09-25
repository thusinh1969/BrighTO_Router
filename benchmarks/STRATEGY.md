# BrighTO-Router benchmark strategy

This document defines how BrighTO-Router is measured. For the short table, read [benchmarks/README.md](README.md) first. The goal is not to create numbers that look good. The goal is to prove that the router adds very little delay, does not buffer large prompts accidentally, keeps streaming responsive, records usage correctly, and stays stable under heavy load.

## Legend

- **LLM** means Large Language Model.
- **Router** means BrighTO-Router.
- **Backend** means the server that actually runs the model, for example vLLM, llama-server, Anthropic, or another OpenAI-compatible endpoint.
- **Direct run** means the benchmark client sends requests straight to the backend.
- **Router run** means the same benchmark client sends the same requests through BrighTO-Router.
- **Router overhead** means `router run latency - direct run latency` for the same payload and load shape.
- **Payload** means the JSON request body sent to `/v1/chat/completions`.
- **Pass-through** means the router forwards request and response bytes without decoding, rewriting, compressing, or storing the full prompt unless a specific feature requires a small bounded edit.
- **Streaming** means the backend sends response chunks over time and the router forwards each chunk promptly.
- **p50** means the median value: half the requests are faster and half are slower.
- **p99** means the 99th percentile value: 99% of requests are faster, 1% are slower.
- **TTFB** means time to first byte: time from sending a request until the first response byte arrives.
- **RPS** means requests per second.
- **Offered rate** means the request rate the load generator tries to start. For example, 4,000 RPS means it tries to start 4,000 requests per second. It is a local calibrated setting, not a global standard.
- **RSS** means resident set size: physical memory currently used by the router process, reported by Linux from `/proc/<pid>/status`.
- **Warm-up** means traffic sent before measurement starts, so connection pools and code paths are already active.
- **Release gate** means a benchmark command whose failure blocks release.
- **Stress proof** means a heavier run that records behavior under extreme payloads or concurrency. Stress proof can fail on correctness, crashes, memory explosion, or ledger loss. It should not fail on an arbitrary speed target before the hardware has been calibrated.

## Target-setting rule

Do not set targets just to fail ourselves.

Every hard performance target must come from one of these sources:

1. A real product requirement, such as “2,000 requests per second must keep usage ledger lag below 2 seconds.”
2. A previously reviewed green baseline from the same benchmark on the same hardware class.
3. A direct-backend calibration run that proves the load generator and backend can sustain the requested traffic without the router.

If none of those exists, the run is a measurement run, not a release-fail rule. Record the result, the hardware, the process memory, and the raw output. After we have stable evidence, promote the number into `benchmarks/thresholds.toml` through a separate change.

## Hardware profile

The reference local machine for current work is dual Intel Xeon Gold class hardware with enough RAM for large prompt pass-through tests. Every artifact must record:

- CPU model.
- Linux kernel.
- Git commit.
- Warm-up duration.
- Measurement duration.
- Concurrency.
- Offered request rate when one is used.
- Router RSS memory samples.
- Raw benchmark output for direct and router runs.

For clean numbers, keep the router, load generator, mock backend, and PostgreSQL on pinned CPU groups when doing final release proof. If CPU pinning is not used, the artifact must say so.

## Payload ranges

The default release gate measures 1k, 50k, and 200k token-class prompts. These are stable enough to run repeatedly and catch the main router overhead risks.

Stress proof also measures 500k and 1M token-class prompts. BrighTO-Router 1.0 has full HTTP and HTTPS artifacts for these sizes at concurrency 1, 50, and 200. These runs prove that the router still behaves like pass-through infrastructure when coding-agent contexts become very large. The first goal for 500k and 1M is not a made-up latency target. The first goal is evidence:

- No request body corruption.
- No response corruption.
- No router crash.
- No unbounded memory growth.
- No ledger drops.
- RSS memory recorded before, during, and after load.
- Router overhead reported against a direct run on the same payload.

`benchmarks/make_payloads.py` generates all five payload sizes: `1k`, `50k`, `200k`, `500k`, and `1m`, with streaming and non-streaming variants.

## Measurement layers

### Layer A: correctness

Layer A is the Rust test suite. It proves auth, routing, budget accounting, usage ledger behavior, streaming behavior, fallback behavior, and admin/config reload behavior. A performance number is not useful if correctness fails.

Command:

```bash
make test
```

### Layer B: local mock performance

Layer B uses `brighto-router-mock`, a deterministic Rust mock backend that returns immediately. This exposes router overhead because model inference time is not hiding it.

Default release command:

```bash
make gate
```

Short smoke command:

```bash
make gate-smoke
```

Smoke runs prove the scripts and binaries work. Smoke numbers are not release proof.

Layer B records:

- Direct run latency.
- Router run latency.
- Router overhead.
- Streaming TTFB delta.
- Target-rate throughput at concurrency 200.
- True ledger lag: p99 time from request completion to PostgreSQL insert.
- Internal ledger drops from Prometheus metrics.
- Router RSS memory samples.
- Baseline comparison when `bench/baseline.json` exists.

### Layer C: local real model

Layer C uses real local model servers such as vLLM and llama-server. This proves that the router works with real streaming behavior, real token usage fields, and real backend timeout behavior.

Layer C should compare router versus direct, but it should not hide router problems behind slow model inference. Use the same prompt, model, max output tokens, temperature, and seed for direct and router runs.

### Layer D: cloud provider correctness

Layer D uses provider APIs such as Anthropic and OpenAI-compatible cloud endpoints. Wide-area network noise is much larger than router overhead, so Layer D focuses on correctness:

- Streaming event order preserved.
- Usage fields recorded correctly.
- Provider errors passed through correctly.
- Client API key never forwarded to provider.
- Provider key loaded only from environment or file references.

### Layer E: failure behavior

Layer E proves behavior during failure:

- Backend connects but sends no bytes.
- Backend sends chunks too slowly.
- Backend dies before first byte.
- Backend dies after first byte.
- PostgreSQL is unavailable.
- Client disconnects during streaming.
- Configuration reload fails.

The router must not wait on PostgreSQL in the request path. Existing in-memory config should keep serving while `/readyz` reports control-plane trouble.

## Stress proof for 500k and 1M

BrighTO-Router 1.0 has a full large-context proof using coding-agent payloads. The payload generator creates repository-style context: file paths, source snippets, diffs, logs, failing tests, and change requests. That shape matches vibe-coding traffic better than repeated prose, while still keeping the router benchmark deterministic.

HTTP command:

```bash
TLS_CERT_PATH= TLS_KEY_PATH= \
BENCH_PAYLOADS=1k,50k,200k,500k,1m \
BENCH_STREAM_PAYLOADS=1k-stream,50k-stream,200k-stream,500k-stream,1m-stream \
CONCS=1,50,200 RUNS=1 DUR=8s WARM=2s \
REQUIRE_PASS=0 \
python3 -u scripts/bench_real.py
```

HTTPS command using the local files `ssl/fullchain.pem` and `ssl/privkey.pem` by default:

```bash
BENCH_TLS=1 \
BENCH_PAYLOADS=1k,50k,200k,500k,1m \
BENCH_STREAM_PAYLOADS=1k-stream,50k-stream,200k-stream,500k-stream,1m-stream \
CONCS=1,50,200 RUNS=1 DUR=8s WARM=2s \
REQUIRE_PASS=0 \
python3 -u scripts/bench_real.py
```

Reviewed large-context summary artifacts:

- `benchmarks/artifacts/v1-http-1m-coding-context-summary.json`
- `benchmarks/artifacts/v1-https-1m-coding-context-summary.json`

Review these fields in `bench/results/<timestamp>/summary.json` or the committed summary artifacts:

- `overhead_ms`: router overhead by payload and concurrency.
- `streaming_ttfb_delta_ms`: time-to-first-byte delta for streaming payloads.
- `router_rss_mb.max`: maximum router resident memory during the run.
- `router_rss_mb.samples`: labeled memory samples throughout the run.
- `ledger_dropped_total`: must remain zero.

A 500k/1M stress run becomes a hard release gate only after repeated public baselines on comparable hardware justify the threshold.

## Baseline policy

`bench/baseline.json` is the release baseline. `make gate` requires it when `REQUIRE_PASS=1`.

Create the first baseline only from a full, reviewed, green run:

```bash
BASELINE_BOOTSTRAP=1 make gate
cp bench/results/<timestamp>/baseline_candidate.json bench/baseline.json
```

A new baseline must be committed separately from code changes. This keeps performance regressions visible.

## What we can claim

We can claim a number only when the matching artifact exists.

Allowed claim example:

> On this dual-Xeon Gold machine, with this Git commit, this Docker image, this payload matrix, and this benchmark command, BrighTO-Router added X ms p50 and Y ms p99 overhead versus direct mock backend.

Do not claim “fastest in the world” until there is a public comparison against named routers on the same hardware, same payloads, same backend, same network path, same logging level, and same load shape.

## 1.0 Model Group benchmark

Model Groups are measured separately from the single-route fast path. Use deterministic local mock endpoints and compare direct mock latency against router Model Group latency. The current exact-length-body public artifact is `benchmarks/artifacts/v1-model-group-lb-current-summary.json`, covering round-robin and weighted round-robin for `1k`, `500k`, and `1m` at concurrency 200. The fair same-mock 60-second 1M delta gate artifact is `benchmarks/artifacts/v1-model-group-lb-1m-60s-gate-summary.json`; it compares round-robin and weighted groups against a one-endpoint Model Group baseline using the same mock backend and rejects large negative deltas. The older full-grid smoke artifact remains at `benchmarks/artifacts/v1-model-group-lb-smoke-summary.json` for historical comparison only.
