# CODEX audit: benchmark strategy, B10 ledger lag, baseline enforcement integrated

Verdict: applied. This is no longer just an audit patch. The production harness now has true B10 ledger-lag measurement, baseline regression enforcement, 500k/1M payload support, and router RSS memory sampling.

Files changed:

- `scripts/bench_real.py`
  - Adds `BENCH_B10=1` default and `B10_TARGET_RPS=2000`.
  - Adds dedicated B10 model alias `MODEL-b10` and payload `1k-b10.json` so B10 ledger rows do not mix with previous phases.
  - Disables warm-up inside the measured B10 phase; previous smoke showed warm-up inflated rows from `317/200` and `367/200`. Fixed result is `200/200`.
  - Adds `completed_at_ms -> inserted_at_ms` B10 p99 query.
  - Adds `BASELINE_BOOTSTRAP=1` and `BENCH_BASELINE=bench/baseline.json`.
  - Adds metric keys for gate rows so B6 `rps` and `non200` cannot collide.
  - Adds router RSS memory samples to `summary.json`.
  - Allows `500k` and `1m` payloads for stress measurement without hard uncalibrated speed thresholds.
- `src/contract.rs`, `src/proxy/mod.rs`, `src/ledger/mod.rs`
  - Adds `UsageEvent.completed_at_ms` captured when the request finalizes.
  - Keeps ledger insert asynchronous; no database await was added to the request path.
- `migrations/0002_ledger_lag_timestamps.sql`
  - Adds `completed_at_ms` and PostgreSQL-assigned `inserted_at_ms`.
  - Adds index `(completed_at_ms, inserted_at_ms)` for the benchmark query.
- `benchmarks/STRATEGY.md`
  - Documents target-setting rules, term legend, release gate, stress proof, 500k/1M strategy, memory/RSS evidence, and claim rules.
- `benchmarks/BENCHMARK.md`
  - Rewritten as release contract, with clear terms and no arbitrary 500k/1M fail thresholds.
- `bench/README.md`
  - Documents reviewed baseline workflow.

Verification run on 2026-09-17:

```text
python3 -m py_compile scripts/bench_real.py benchmarks/make_payloads.py scripts/hotpath_guard.py   PASS
bash -n start.sh scripts/test_postgres.sh benchmarks/gate.sh                                      PASS
python3 benchmarks/make_payloads.py mock-model                                                     PASS; generated 500k/1m payloads
python3 scripts/hotpath_guard.py                                                                    HOTPATH_GUARD_PASS
cargo fmt --all -- --check                                                                          PASS
CARGO_INCREMENTAL=0 cargo check --locked --all-targets                                              PASS
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings                              PASS
DATABASE_URL=postgres://cognee:cognee@127.0.0.1:5433/cognee ./scripts/test_postgres.sh              PASS; 48 unit + 4 integration
```

B10 smoke verification command:

```bash
DUR=1s WARM=1s RUNS=1 CONCS=50 BENCH_B6=0 BENCH_B10=1 B10_TARGET_RPS=200 REQUIRE_PASS=0 BASELINE_BOOTSTRAP=1 python3 scripts/bench_real.py
```

Result:

```text
B10 ledger lag p99 0.486050s rows 200/200 rps 202.40 non200 0
baseline candidate: /mnt/data02/BrigTO_Router/bench/results/20260917-020222/baseline_candidate.json
gate pass: True
```

Release proof still required before claiming the whole BENCHMARK.md contract green for this exact commit:

```bash
BASELINE_BOOTSTRAP=1 make gate
# review bench/results/<timestamp>/baseline_candidate.json
# copy to bench/baseline.json in a separate baseline commit
make gate
```

Do not set hard fail thresholds for 500k/1M token-class stress until the first reviewed stress artifact exists. Record RSS and overhead first.
