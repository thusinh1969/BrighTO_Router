# Benchmarks in plain language

This is the benchmark guide to read first. It answers four questions:

1. What do we measure?
2. What result do we get?
3. Why do we measure it this way?
4. What does it mean for a real team using many model backends?

## Term legend

- **LLM** means Large Language Model.
- **Backend** means the server that runs the model. Examples: vLLM, llama-server, Anthropic, or another OpenAI-compatible server.
- **Router overhead** means `router latency - direct backend latency` using the same payload and same load.
- **Payload** means the JSON request body sent to the model API.
- **Pass-through** means the router forwards bytes without storing prompts or doing provider translation.
- **p50** means median latency. Half the requests are faster, half are slower.
- **p99** means 99th percentile latency. 99% of requests are faster, 1% are slower.
- **TTFB** means time to first byte: how long the client waits before the first response byte arrives.
- **RPS** means requests per second.
- **RSS** means resident set size: physical memory used by the router process on Linux.
- **Concurrency** means how many requests are in flight at the same time.

## The one table that matters

| Payload | Approx input size | Concurrent requests tested | What we measure | Current release rule | What it means in production |
|---|---:|---:|---|---|---|
| `1k` | 1,000 tokens | 1, 50, 200 | Median overhead, p99 overhead, streaming TTFB, high-rate throughput, ledger lag | Hard gate at concurrency 50; B6 throughput gate at concurrency 200; B10 ledger gate at 2,000 RPS | Normal app traffic must not pay visible router cost. A 100-person team can send many small prompts through one endpoint without the router becoming the bottleneck. |
| `50k` | 50,000 tokens | 1, 50, 200 | Median overhead, p99 overhead, streaming TTFB, memory samples | Hard gate at concurrency 50 | Common retrieval and agent prompts must remain pass-through. The router should route, enforce policy, and record usage without copying large bodies more than needed. |
| `200k` | 200,000 tokens | 1, 50, 200 | Median overhead, p99 overhead, streaming TTFB, flatness versus `1k`, memory samples | Hard gate at concurrency 50 | Large research prompts must not make router overhead grow in proportion to prompt size. If 200k is much worse than 1k, the router is buffering or parsing too much. |
| `500k` | 500,000 tokens | 1, 50, then 200 after review | Overhead, streaming TTFB, correctness, RSS memory, ledger drops | Measurement-first stress proof until a reviewed baseline exists | Extreme prompt pass-through must stay stable. We record memory and correctness first; we do not invent a speed target before the machine is calibrated. |
| `1m` | 1,000,000 tokens. The name means 1M. | 1, 50, then 200 after review | Overhead, streaming TTFB, correctness, RSS memory, ledger drops | Measurement-first stress proof until a reviewed baseline exists | This proves the router can survive very large prompts without runaway memory. It is for capacity planning on high-memory servers, not a fake everyday target. |

## How to read concurrency

| Concurrency | Meaning | Why it matters |
|---:|---|---|
| 1 | One request at a time | Shows the latency floor. If this is slow, the router has avoidable per-request work. |
| 50 | Fifty requests active at once | Represents busy internal team traffic and is the main release latency gate. |
| 200 | Two hundred requests active at once | Shows whether queues, connection pools, ledger writing, and memory stay controlled under heavy load. |

A practical example: a team of 100 people with 5 model backends may have many small requests, some long research prompts, and several long streams at the same time. The router must choose a healthy backend, enforce key/team budgets, keep client API keys private, and write usage without making model responses slower. Concurrency 50 is the everyday stress point. Concurrency 200 is the overload/capacity signal.

## What the output files tell you

After a run, read `bench/results/<timestamp>/summary.json` first.

| Field | Meaning | Healthy result |
|---|---|---|
| `overhead_ms` | Router latency minus direct backend latency for each payload/concurrency | Near zero; for `1k`, `50k`, `200k` it must pass `BENCHMARK.md` thresholds. |
| `streaming_ttfb_delta_ms` | Extra time before the first streaming byte reaches the client | Near zero; large prompts should not delay first byte because of router buffering. |
| `b6_saturation` | Whether `1k` traffic can sustain the target request rate | At least 8,000 RPS on the reference 4-core router allocation, with zero non-200 responses. |
| `b10_ledger_lag` | Time from request completion to PostgreSQL usage insert | p99 must be at most 2 seconds at the B10 target rate. |
| `router_rss_mb` | Router physical memory samples | Stable memory. Large prompt tests must not show runaway growth. |
| `ledger_dropped_total` | Usage records dropped by the ledger path | Must be zero. |
| `baseline` | Comparison with `bench/baseline.json` | Release runs must not regress more than the allowed baseline rule. |

## Current measured status

The current repo has verified harness support for 1k, 50k, 200k, 500k, and 1M token-class payloads. The long local mock artifact before B10/baseline additions measured the main 1k to 200k gates on an Intel Xeon Gold 6148 machine:

| Payload | Concurrency | Median overhead | p99 overhead | Streaming TTFB delta |
|---|---:|---:|---:|---:|
| `1k` | 50 | +0.288 ms | +0.512 ms | +0.004 ms |
| `50k` | 50 | +0.503 ms | +0.734 ms | +0.041 ms |
| `200k` | 50 | +0.785 ms | +0.813 ms | -0.017 ms |

The same artifact sustained 8,499.48 RPS for the `1k` target-rate gate with zero non-200 responses.

After the B10 harness change, a short current smoke run measured B10 ledger lag at reduced load:

| Check | Result |
|---|---:|
| B10 target load | 200 RPS |
| Observed rows in PostgreSQL | 200 / 200 |
| p99 ledger lag | 0.991040 seconds |
| non-200 responses | 0 |

A short local stress smoke has also verified that the harness can run `500k` and `1m` at concurrency 1 after raising the mock backend body limit. That smoke is engineering evidence, not public release proof.

The next official release proof is a full run with `BASELINE_BOOTSTRAP=1`, followed by review and commit of `bench/baseline.json`. The 500k and 1M stress proof must be run on the dual-Xeon server and reviewed before any public claim about those payload sizes.

## Commands

Short smoke. This checks that the benchmark system works; it is not release proof:

```bash
./start.sh smoke
```

Full release baseline candidate:

```bash
BASELINE_BOOTSTRAP=1 ./start.sh gate
cp bench/results/<timestamp>/baseline_candidate.json bench/baseline.json
```

Large prompt stress proof:

```bash
BENCH_PAYLOADS=500k,1m \
BENCH_STREAM_PAYLOADS=500k-stream,1m-stream \
CONCS=1,50,200 \
RUNS=3 \
DUR=60s \
WARM=15s \
BENCH_B6=0 \
BENCH_B10=0 \
REQUIRE_PASS=0 \
python3 scripts/bench_real.py
```

## Model and media support

Current supported API paths are:

| API path | Status |
|---|---|
| `/v1/chat/completions` | Supported for OpenAI-compatible chat payloads. |
| `/v1/completions` | Supported for OpenAI-compatible completion payloads. |
| `/v1/embeddings` | Supported for OpenAI-compatible embedding payloads. |
| `/v1/models` | Supported. Lists configured model aliases. |
| `/v1/messages` | Supported for Anthropic-compatible messages payloads. |
| `/v1/images/*` | Not implemented as a dedicated route. |
| `/v1/audio/*` | Not implemented as a dedicated route. |
| `/v1/video/*` | Not implemented as a dedicated route. |

Chat-style image inputs can pass through `/v1/chat/completions` when the backend accepts the same JSON format and the body stays under `MAX_BODY_BYTES`. This has not yet been given a separate benchmark gate. Audio and video usually need different routes or multipart handling, so they are future work, not current functionality.
