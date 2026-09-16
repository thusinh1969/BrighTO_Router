# CODEX audit — benchmark docs made simple, 1M mock body limit fixed

Time: 2026-09-17 +07
Scope: current worktree after public cleanup and B10 gate integration.

## Verdict

Actionable fix applied. Benchmark docs now have one plain-language starting point: `benchmarks/README.md`. It explains what is measured from `1k` to `1m`, why concurrency `1/50/200` exists, what every output field means, and how this maps to a 100-person team with 5 LLM backends.

No fake 500k/1M release target was added. Large payloads stay measurement-first until a reviewed baseline exists.

## Root cause found

The first attempted 1M stress smoke failed before measuring router behavior because the mock upstream returned `413` on a 4 MB JSON body. That came from Axum's default body limit on the mock server, not from BrighTO-Router.

Evidence from failed artifact `bench/results/20260917-021108/direct-1m-c1-r1.json`:

```text
statusCodeDistribution: {"413": 479}
errorDistribution: {"aborted due to deadline": 1, "error writing a body to connection": 583, "operation was canceled": 4}
```

## Fix applied

`src/bin/mock_upstream.rs` now installs `DefaultBodyLimit::max(max_body_bytes)` with default `64 * 1024 * 1024` and env override `MOCK_MAX_BODY_BYTES`. This aligns the mock backend with router `MAX_BODY_BYTES=67108864`, so 1M token-class JSON payloads can be measured instead of being rejected by the benchmark fixture.

`Makefile` smoke now runs a real short smoke instead of a heavy release-like run:

```bash
DUR=1s WARM=1s RUNS=1 CONCS=50 BENCH_B6=0 BENCH_B10=1 B10_TARGET_RPS=200 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

`scripts/bench_real.py` now separates:

- `release_pass`: whether release thresholds pass.
- `command_pass`: whether the current command should succeed. Smoke can succeed while release thresholds are not enforced.

B10 gate row now labels its actual target rate, for example `200rps` in smoke instead of hard-coded `2000rps`.

## Verified commands

```bash
python3 -m py_compile scripts/bench_real.py
bash -n start.sh scripts/test_postgres.sh benchmarks/gate.sh
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
./start.sh smoke
BENCH_PAYLOADS=500k,1m BENCH_STREAM_PAYLOADS=500k-stream,1m-stream CONCS=1 RUNS=1 DUR=3s WARM=1s BENCH_B6=0 BENCH_B10=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

## Smoke evidence

`./start.sh smoke` artifact: `bench/results/20260917-021039`

```text
B10 ledger lag p99 0.993050s rows 200/200 rps 200.44 non200 0
smoke command pass: True
release thresholds pass: False (not enforced in smoke)
B10 gate conc label: 200rps
```

Large-payload stress smoke artifact: `bench/results/20260917-021146`

```text
500k c=1 overhead p50 +1.037ms p99 +0.771ms
1m   c=1 overhead p50 +1.667ms p99 +3.709ms
B4 500k ttfb delta -0.015ms
B4 1m   ttfb delta -0.009ms
router_rss_mb.max 29.77
ledger_dropped_total 0
smoke command pass: True
```

## Remaining release work

Run the real long baseline flow before public performance claims:

```bash
BASELINE_BOOTSTRAP=1 ./start.sh gate
cp bench/results/<timestamp>/baseline_candidate.json bench/baseline.json
```

Then run the 500k/1M stress proof on the target dual-Xeon server:

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

## Docker image verification

After this commit, the Docker runtime image was rebuilt and pushed for tag parity:

```bash
DOCKER_BUILDKIT=1 docker build -t thusinh1969/brighto_airouter:v1 . && docker push thusinh1969/brighto_airouter:v1
```

Result:

```text
image sha256:797139d67babced41a5a43c7748f8f357d0e02168da35d2351cf867359a39cde
v1 digest sha256:08904ab54e1bc42c7008e62f5e10310499201557f461692672b7ce60ebbe1a61
```

The digest stayed the same because the runtime image copies only `brighto-router`; this change affected docs, benchmark scripts, and the benchmark mock server.
