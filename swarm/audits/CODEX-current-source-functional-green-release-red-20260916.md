# CODEX current source verification — functional green, release/SOTA red

Date: 2026-09-16 22:23 +07

Verdict: current source is compile/test green when run with the correct Postgres test environment, but it is not release/SOTA green. Do not use this as a performance claim. The benchmark harness and hot-path patches from `CODEX-DEEPSEEK-ROOT-CAUSE-ONE-SHOT-20260916.md` are still required.

## Current verification run

Without `DATABASE_URL`, `cargo test --lib` fails only because `sqlx::test` needs Postgres:

- `config::tests::load_survives_empty_db`
- `config::tests::snapshot_picks_up_budget_change_within_poll_interval`
- `ledger::tests::batch_flush_by_size_va_by_time`
- `ledger::tests::db_down_writes_file_replay_on_reconnect`

With a fresh `pgvector/pgvector:pg16` container and dynamic host port:

```bash
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo test -q route --lib
DATABASE_URL=postgres://... CARGO_INCREMENTAL=0 cargo test --lib
DATABASE_URL=postgres://... CARGO_INCREMENTAL=0 cargo test --test streaming_integration
```

Results observed:

- `py_compile`: PASS
- `cargo fmt --all -- --check`: PASS
- `cargo check --all-targets`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test -q route --lib`: PASS, 8/8
- `cargo test --lib` with Postgres: PASS, 45/45
- `cargo test --test streaming_integration` with Postgres: PASS, 2/2

## Release blockers still present in current source

Evidence command:

```bash
rg -n 'response\.bytes\(\)\.await|HashSet|Vec<Candidate>|router_ledger_dropped_total|B6_TARGET_RPS|BENCH_PAYLOADS|BRIGTO_HOTPATH_PROBE' \
  src/proxy/mod.rs src/route/mod.rs src/metrics.rs scripts/bench_real.py tests/streaming_integration.rs
```

Observed current markers:

- `src/proxy/mod.rs:535` still has `response.bytes().await` on the non-stream path.
- `src/proxy/mod.rs:614` still creates `HashSet<i64>` for tried backends.
- `src/route/mod.rs:202` still takes `&HashSet<i64>` in candidate selection.
- `src/route/mod.rs:204` still builds `Vec<Candidate>` per pick.
- `src/route/mod.rs:275` still builds tied candidate `Vec`.
- No current-source `B6_TARGET_RPS` / `BENCH_PAYLOADS` marker in `scripts/bench_real.py`, so benchmark-truth and focused payload-filter patches are not applied.
- No current-source `BRIGTO_HOTPATH_PROBE` marker, so temporary probe is not applied.

## Stale benchmark is still contaminating this host

Still active at this audit:

- `python3 scripts/bench_real.py` PID 1983639
- `llm-router-mock` PID 1986928
- `brigto-router` PID 1986929
- latest artifact under `bench/results/20260916-212030`

The artifact proves current harness unit bug: `router-50k-c50-r1.json` has `latencyPercentiles.p50 = 0.001302838`, which is 1.302838 ms. The current script treats that value as ms unless the benchmark-truth patch is applied.

## Action for DeepSeek

Do not chase new architecture while this state is true. Apply/fix in this order:

1. `CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch`
2. `CODEX-apply-bench-payload-filter-20260916.patch`
3. `CODEX-apply-route-picker-noalloc-20260916.patch`
4. `CODEX-apply-nonstream-response-streaming-20260916.patch`
5. Only if 200k still misses: `CODEX-apply-hotpath-probe-temporary-20260916.patch`

Then run the focused 200k command from `CODEX-DEEPSEEK-ROOT-CAUSE-ONE-SHOT-20260916.md` after the stale benchmark PIDs are gone.
## Update 2026-09-16 22:27 +07 — stale benchmark is active, not idle

Process tree check showed the stale benchmark is still actively running `oha`:

```text
1983639 python3 scripts/bench_real.py
1986928 llm-router-mock      CPU ~1336%
1986929 brigto-router        CPU ~946%
2143654 oha ... -z 60s -c 200 ... direct-50k-c200-r3.json ... http://127.0.0.1:9000/v1/chat/completions
```

The run has been active for about 47 minutes and is writing `bench/results/20260916-212030`. Do not use this artifact for release/SOTA proof. Do not run focused 200k probe until these processes are gone or intentionally terminated by the coding agent that owns them.
## Update 2026-09-16 22:33 +07 — current-source vs patched-sequence tests

The `streaming_integration` 2/2 result above is for current source before hot-path patches. The patched sequence was rerun with a fresh Cargo target and passed 3/3, including the new delayed non-stream response test.

