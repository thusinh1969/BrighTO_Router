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
- **Offered rate** means the request rate the load generator tries to start. It is a local test setting, not a global standard.
- **RSS** means resident set size: physical memory used by the router process on Linux.
- **Concurrency** means how many requests are in flight at the same time.

## The one table that matters

| Payload | Approx input size | Concurrent requests tested | What we measure | Current release rule | What it means in production |
|---|---:|---:|---|---|---|
| `1k` | 1,000 tokens | 1, 50, 200 | Median overhead, p99 overhead, streaming TTFB, high-rate throughput, ledger lag | Hard gate at concurrency 50; B6 throughput gate at concurrency 200; B10 ledger gate at 2,000 RPS | Normal app traffic must not pay visible router cost. A 100-person team can send many small prompts through one endpoint without the router becoming the bottleneck. |
| `50k` | 50,000 tokens | 1, 50, 200 | Median overhead, p99 overhead, streaming TTFB, memory samples | Hard gate at concurrency 50 | Common retrieval and agent prompts must remain pass-through. The router should route, enforce policy, and record usage without copying large bodies more than needed. |
| `200k` | 200,000 tokens | 1, 50, 200 | Median overhead, p99 overhead, streaming TTFB, flatness versus `1k`, memory samples | Hard gate at concurrency 50 | Large research prompts must not make router overhead grow in proportion to prompt size. If 200k is much worse than 1k, the router is buffering or parsing too much. |
| `500k` | 500,000 tokens | 1, 50, 200 | Overhead, streaming TTFB, correctness, RSS memory, ledger drops | Full large-context measurement artifact exists; no hard speed threshold yet | Extreme coding-context pass-through must stay stable without memory growth. |
| `1m` | 1,000,000 tokens. The name means 1M. | 1, 50, 200 | Overhead, streaming TTFB, correctness, RSS memory, ledger drops | Full large-context measurement artifact exists; no hard speed threshold yet | This covers large vibe-coding and repository-analysis contexts while keeping router memory visible. |

## How to read offered rate

An example such as `1k c=50 offered rate 4000 RPS` means:

- use the `1k` token-class payload;
- keep at most 50 requests active at the same time;
- ask the load generator to start up to 4,000 requests per second.

This is not a global standard. The globally defensible part is the comparison method: direct backend versus router on the same machine, same payload, same concurrency, same duration, same logging, and same network path. The exact offered-rate numbers must be calibrated per machine.

## How to read concurrency

| Concurrency | Meaning | Why it matters |
|---:|---|---|
| 1 | One request at a time | Shows the latency floor. If this is slow, the router has avoidable per-request work. |
| 50 | Fifty requests active at once | Represents busy internal team traffic and is the main release latency gate. |
| 200 | Two hundred requests active at once | Shows whether queues, connection pools, ledger writing, and memory stay controlled under heavy load. |

A practical example: a team of 100 people with 5 model backends may have many small requests, some long research prompts, and several long streams at the same time. The router must choose a healthy backend, enforce key/team budgets, keep client API keys private, and write usage without making model responses slower. Concurrency 50 is the everyday stress point. Concurrency 200 is the overload/capacity signal.

## Gate-name legend

The release contract uses short gate names so scripts can report failures compactly:

| Gate | Plain meaning |
|---|---|
| `B1` | Median latency overhead. |
| `B2` | 99th-percentile latency overhead. |
| `B3` | Whether 200k prompts add much more overhead than 1k prompts. |
| `B4` | Streaming time-to-first-byte overhead. |
| `B6` | High-rate throughput for small prompts. |
| `B10` | Usage-ledger lag from router completion to PostgreSQL insert. |

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

The preview-3 large-context benchmark has been run with coding-agent payloads from `1k` through `1m`, at concurrency 1, 50, and 200, over both HTTP and HTTPS. The benchmark uses a deterministic local Rust mock backend so model inference time and cloud network noise do not hide router overhead.

HTTP artifact: `benchmarks/artifacts/preview-3-http-1m-coding-context-summary.json`.

| Payload | c=1 p50 / p99 overhead | c=50 p50 / p99 overhead | c=200 p50 / p99 overhead |
|---|---:|---:|---:|
| `1k` | `+0.255 / +0.363 ms` | `+0.317 / +0.548 ms` | `+0.311 / +0.459 ms` |
| `50k` | `+0.976 / +0.977 ms` | `+0.458 / +0.646 ms` | `+0.457 / +53.865 ms` |
| `200k` | `+0.874 / +1.018 ms` | `+0.985 / +1.448 ms` | `+0.970 / +0.240 ms` |
| `500k` | `+1.336 / +1.378 ms` | `+1.091 / +0.935 ms` | `+1.325 / +1.174 ms` |
| `1m` | `+1.894 / +3.088 ms` | `+2.047 / +3.415 ms` | `+2.167 / +1.664 ms` |

HTTPS artifact: `benchmarks/artifacts/preview-3-https-1m-coding-context-summary.json`.

| Payload | c=1 p50 / p99 overhead | c=50 p50 / p99 overhead | c=200 p50 / p99 overhead |
|---|---:|---:|---:|
| `1k` | `+0.301 / +0.420 ms` | `+0.357 / +96.932 ms` | `+0.355 / +261.090 ms` |
| `50k` | `+1.578 / +1.755 ms` | `+0.830 / +101.449 ms` | `+0.919 / +341.180 ms` |
| `200k` | `+2.134 / +2.336 ms` | `+1.972 / +68.994 ms` | `+1.949 / +55.035 ms` |
| `500k` | `+3.720 / +5.122 ms` | `+3.555 / +31.512 ms` | `+3.696 / +33.680 ms` |
| `1m` | `+7.105 / +9.744 ms` | `+7.008 / +12.055 ms` | `+6.539 / +35.845 ms` |

Full-run health results:

| Check | HTTP result | HTTPS result |
|---|---:|---:|
| Router non-200 responses | `0` | `0` |
| Max router RSS | `60.04 MB` | `83.24 MB` |
| Ledger drops | `0` | `0` |
| 1k saturation check | `8,496.18 RPS`, `0` non-200 | `8,496.80 RPS`, `0` non-200 |
| Ledger check at 2,000 RPS | `15,964 / 15,999` rows observed | `15,922 / 16,000` rows observed |

Streaming first-byte delta was near zero on HTTP from `1k` to `1m`. HTTPS first-byte delta was about `19-22 ms` in this benchmark because the sequential `curl` probe opens new local TLS connections with a self-signed certificate. That is useful as a conservative new-connection number; production clients should reuse connections.

The `50k` HTTP run at concurrency 200 had one p99 tail spike in the artifact. It did not produce errors, ledger drops, or RSS growth, and the larger `200k`, `500k`, and `1m` concurrency-200 runs stayed low. Keep the raw artifact when comparing future runs so tail behavior remains visible.

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

Full large-context HTTP proof through 1M:

```bash
TLS_CERT_PATH= TLS_KEY_PATH= \
BENCH_PAYLOADS=1k,50k,200k,500k,1m \
BENCH_STREAM_PAYLOADS=1k-stream,50k-stream,200k-stream,500k-stream,1m-stream \
CONCS=1,50,200 RUNS=1 DUR=8s WARM=2s \
REQUIRE_PASS=0 \
python3 -u scripts/bench_real.py
```

Full large-context HTTPS proof through 1M, using `ssl/fullchain.pem` and `ssl/privkey.pem` by default:

```bash
BENCH_TLS=1 \
BENCH_PAYLOADS=1k,50k,200k,500k,1m \
BENCH_STREAM_PAYLOADS=1k-stream,50k-stream,200k-stream,500k-stream,1m-stream \
CONCS=1,50,200 RUNS=1 DUR=8s WARM=2s \
REQUIRE_PASS=0 \
python3 -u scripts/bench_real.py
```

## Model and media support

Current supported API paths are:

| API path | Status |
|---|---|
| `/v1/chat/completions` | Supported for OpenAI-compatible chat payloads. |
| `/v1/completions` | Supported for OpenAI-compatible completion payloads. |
| `/v1/embeddings` | Supported for OpenAI-compatible embedding payloads. |
| `/v1/rerank` | Preview-3 supported for Qwen/DashScope, Jina, Voyage, Cohere, and OpenAI-compatible/custom rerank adapters. |
| `/v1/audio/transcriptions` | Preview-3 supported for OpenAI-compatible multipart ASR/transcription providers. |
| `/v1/models` | Supported. Lists configured model aliases. |
| `/v1/messages` | Supported for Anthropic-compatible messages payloads. |
| `/v1/images/*` | Not implemented as a dedicated route. |
| `/v1/audio/generations`, `/v1/audio/speech` | Not implemented as dedicated TTS/audio-generation routes. |
| `/v1/video/*` | Not implemented as a dedicated route. |

Chat-style image/audio inputs can pass through `/v1/chat/completions` when the backend accepts the same JSON format and the body stays under `MAX_BODY_BYTES`. Preview-3 adds dedicated rerank and ASR routes. Image generation, TTS/audio generation, video, and realtime media remain future adapter work.
