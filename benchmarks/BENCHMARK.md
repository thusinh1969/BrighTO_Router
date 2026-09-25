# BrighTO-Router benchmark contract

This file is the release contract. Read [benchmarks/README.md](README.md) first for the simple table, then [benchmarks/STRATEGY.md](STRATEGY.md) for the measurement rationale.

Rule: a release gate with `REQUIRE_PASS=1` exits non-zero when a required check fails. A short smoke run proves the scripts work, but it is not release proof. In smoke artifacts, `command_pass` can be true while `release_pass` is false because release thresholds are not enforced.

## Required evidence for a release

A release artifact must include:

- `bench/results/<timestamp>/gate.json`
- `bench/results/<timestamp>/summary.json`
- raw direct-run JSON files from `oha`
- raw router-run JSON files from `oha`
- `router.log`
- `mock.log`
- `router-metrics.txt`
- CPU model, kernel, Git commit, warm-up duration, run duration, concurrency, and offered request rate
- router RSS memory samples, where RSS means resident set size from Linux `/proc/<pid>/status`

## Default release payloads

| Payload name | Approximate input tokens | Approximate JSON body size | Purpose |
|---|---:|---:|---|
| `1k` | 1,000 | 4 KB | latency floor and throughput |
| `50k` | 50,000 | 200 KB | common retrieval and agent workloads |
| `200k` | 200,000 | 800 KB | large-context release gate |

The release gate measures these payloads at concurrency 1, 50, and 200. Hard pass/fail latency thresholds apply at concurrency 50. The other concurrency values are still recorded to catch load-shape problems.

**Offered request rate** means the request rate the load generator tries to start. Example: `1k c=50 offered rate 4000 RPS` means the 1k-token payload, at most 50 active requests, and up to 4,000 request starts per second. This is a calibrated local load setting, not a global standard. The benchmark contract is the same-hardware direct-versus-router comparison.

## Large prompt stress payloads

| Payload name | Approximate input tokens | Approximate JSON body size | Purpose |
|---|---:|---:|---|
| `500k` | 500,000 | 2 MB | pass-through stress proof |
| `1m` | 1,000,000. The name means 1M. | 4 MB | extreme pass-through and memory proof |

BrighTO-Router 1.0 includes full HTTP and HTTPS measurement artifacts for 500k and 1M at concurrency 1, 50, and 200. They record correctness, router overhead, streaming time to first byte, ledger drops, and router memory. Do not add arbitrary pass/fail speed thresholds for these payloads until repeated public baselines justify them.

## Current Layer B release gates

Layer B uses `brighto-router-mock`, a deterministic local backend that returns immediately. This makes router overhead visible.

| Gate | What it measures | Payloads | Required result |
|---|---|---|---|
| `B1` | p50 router overhead. p50 means median latency. | `1k`, `50k`, `200k` non-stream | ≤ 0.3 ms, ≤ 0.6 ms, ≤ 1.0 ms |
| `B2` | p99 router overhead. p99 means 99% of requests are faster. | `1k`, `50k`, `200k` non-stream | ≤ 0.8 ms, ≤ 1.5 ms, ≤ 2.0 ms |
| `B3` | Payload flatness: p50 overhead for 200k minus p50 overhead for 1k. | `200k` versus `1k` | ≤ 0.8 ms |
| `B4` | Streaming TTFB delta. TTFB means time to first byte. | `1k`, `50k`, `200k` stream | ≤ 1 ms, ≤ 2 ms, ≤ 3 ms |
| `B6` | Target-rate throughput. RPS means requests per second. | `1k` non-stream, concurrency 200 | ≥ 8,000 RPS and non-200 responses = 0 |
| `B10` | Ledger lag: time from router request completion to PostgreSQL insert. | `1k`, 2,000 RPS | p99 ≤ 2 seconds, at least 99% of expected rows observed |
| `INTERNAL_LEDGER_DROPS` | Internal safety counter from metrics. | all measured traffic | `router_ledger_dropped_total = 0` |
| `baseline` | Regression against `bench/baseline.json`. | every gate row above | no metric worse than baseline by more than 10% |

The harness records raw data for 500k and 1M in full 1.0 runs. Model Group has one calibrated large-payload regression gate in fair mode: run `MODEL_GROUP_SAME_MOCK=1` so the single route and every Model Group member use the same mock backend. At HTTP 1M, Model Group p50 overhead minus single-route p50 overhead must be between `0` and `0.5 ms` in the same run. A negative delta is rejected because it is not a credible release claim.

## Commands

Release gate:

```bash
make gate
```

Smoke gate:

```bash
make gate-smoke
```

First baseline bootstrap:

```bash
BASELINE_BOOTSTRAP=1 make gate
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

Full large-context HTTPS proof through 1M:

```bash
BENCH_TLS=1 \
BENCH_PAYLOADS=1k,50k,200k,500k,1m \
BENCH_STREAM_PAYLOADS=1k-stream,50k-stream,200k-stream,500k-stream,1m-stream \
CONCS=1,50,200 RUNS=1 DUR=8s WARM=2s \
REQUIRE_PASS=0 \
python3 -u scripts/bench_real.py
```

## Target changes

Change thresholds only in `benchmarks/thresholds.toml`, and only with a benchmark artifact proving why the new target is correct. A target must come from product need, direct-backend calibration, or a reviewed previous-release baseline.

## Public claims

Do not claim “fastest in the world” from internal numbers alone. That claim needs a comparison against named routers on the same machine, same payloads, same backend, same network path, same logging level, and same load shape.
