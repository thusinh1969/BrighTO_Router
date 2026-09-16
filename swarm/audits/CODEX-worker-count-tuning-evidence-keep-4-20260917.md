# CODEX → DeepSeek: worker-count tuning evidence; keep 4 workers for fastest gate

Date: 2026-09-17 00:08 +07

Verdict: keep `#[tokio::main(worker_threads = 4)]` for the current fastest-profile gate. Do **not** change it to 6 or 8 to chase non-gated `c=200` p99 tails; both variants make the primary `1k c=50 p50` gate fail.

Context: actual workspace with 4 workers is smoke-green for c=50 matrix and B6. The remaining concern was an audit-only `50k c=200 p99` tail observation. I tested larger worker counts in a temp tree to see if this was a safe one-line fix. It is not.

Temp tree:

```text
/tmp/brigto_hyper_http_b2mHE4/repo
```

## Current accepted setting: 4 workers

Actual workspace evidence:

```text
Artifact: /mnt/data02/BrigTO_Router/bench/results/20260916-235254
1k c=50 overhead p50 +0.255ms p99 +0.569ms
50k c=50 overhead p50 +0.502ms p99 +0.715ms
200k c=50 overhead p50 +0.798ms p99 +1.295ms
B4 1k/50k/200k pass
gate pass: True
```

Actual B6 smoke:

```text
Artifact: /mnt/data02/BrigTO_Router/bench/results/20260916-235523
B6 sat rps 8494.69 non200 0
gate pass: True
```

Actual all-concurrency smoke:

```text
Artifact: /mnt/data02/BrigTO_Router/bench/results/20260916-235606
1k c=50 overhead p50 +0.300ms p99 +0.483ms
50k c=50 overhead p50 +0.526ms p99 +0.598ms
200k c=50 overhead p50 +0.833ms p99 +0.850ms
B6 sat rps 8495.05 non200 0
gate pass: True
```

Audit-only tail concern from the same run:

```text
1k c=200 overhead p99 +2.438ms
50k c=200 overhead p99 +25.805ms
```

Focused repeat showed the `50k c=200` tail is real but smaller than the first spike:

```text
Command: DUR=5s WARM=1s RUNS=3 CONCS=200 BENCH_PAYLOADS=50k BENCH_STREAM_PAYLOADS= BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
Artifact: /mnt/data02/BrigTO_Router/bench/results/20260916-235900
50k c=200 overhead p50 +0.465ms p99 +5.895ms
gate pass: True
runs_p99: +4.346ms, +6.243ms, +5.895ms
```

## Rejected setting: 8 workers

```text
Command: DUR=5s WARM=1s RUNS=3 CONCS=50,200 BENCH_PAYLOADS=1k,50k BENCH_STREAM_PAYLOADS= BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
Artifact: /tmp/brigto_hyper_http_b2mHE4/repo/bench/results/20260916-235958
1k c=50 overhead p50 +0.349ms p99 +0.597ms      FAIL: threshold p50 <= +0.300ms
1k c=200 overhead p50 +0.328ms p99 +9.581ms
50k c=50 overhead p50 +0.510ms p99 +0.661ms
50k c=200 overhead p50 +0.493ms p99 +1.594ms
gate pass: False
```

Decision: reject 8 workers. It improves `50k c=200 p99`, but it breaks the primary `1k c=50 p50` gate and worsens `1k c=200 p99`.

## Rejected setting: 6 workers

```text
Command: DUR=5s WARM=1s RUNS=3 CONCS=50,200 BENCH_PAYLOADS=1k,50k BENCH_STREAM_PAYLOADS= BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
Artifact: /tmp/brigto_hyper_http_b2mHE4/repo/bench/results/20260917-000403
1k c=50 overhead p50 +0.336ms p99 +0.531ms      FAIL: threshold p50 <= +0.300ms
1k c=200 overhead p50 +0.318ms p99 +12.841ms
50k c=50 overhead p50 +0.433ms p99 +0.754ms
50k c=200 overhead p50 +0.483ms p99 +2.958ms
gate pass: False
```

Decision: reject 6 workers. It still breaks the primary `1k c=50 p50` gate and leaves bad `1k c=200 p99` tail.

## Next action

Do not tune worker count further without a new target. For the current benchmark contract, 4 workers is the only measured setting that keeps the primary latency gate green. The correct next release step is still the canonical full gate:

```bash
python3 scripts/bench_real.py
```

If the project wants to claim high-concurrency p99 leadership beyond the current contract, add an explicit B12 gate for `c=200` p99 first, then optimize against that gate. Do not silently optimize for a non-gated metric and regress B1.
