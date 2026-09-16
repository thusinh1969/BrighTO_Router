# Current verification — `stream_options` root detection P1 is fixed

Time: 2026-09-16 22:xx ICT  
Scope: current worktree. Codex did not edit `src/`.

## Verdict

The `stream_options` false-positive bug is fixed in current source and runtime verified.

Do not revert to body-wide `memmem` detection. The correct architecture now carries a top-level bool from the existing borrowed serde parse in handler into proxy.

## Source evidence

Producer side in `src/handlers.rs`:

```text
src/handlers.rs:15  use serde_json::value::RawValue;
src/handlers.rs:126 let stream_options_present = head.stream_options.is_some();
src/handlers.rs:182 stream_options_present,
```

Consumer side in `src/proxy/mod.rs`:

```text
src/proxy/mod.rs:255 pub stream_options_present: bool,
src/proxy/mod.rs:644 && !ctx.stream_options_present
```

Old root cause removed from production path:

```text
rg contains_stream_options src/proxy/mod.rs   # no function/call remains
```

## Runtime proof

Command:

```bash
cargo build --release --locked
python3 /tmp/brigto_stream_options_false_positive.py
```

Result:

```text
CASE normal_missing_root status 200 top_has True include_true True
CASE string_false_positive status 200 top_has True include_true True
CASE nested_false_positive status 200 top_has True include_true True
RESULT PASS
STREAM_OPTIONS_SMOKE_RC=0
```

This proves these three cases now inject top-level `stream_options.include_usage=true` correctly:

1. no `stream_options` anywhere;
2. prompt content contains literal `stream_options`;
3. nested message object contains `stream_options`.

## Regression gates after fix

```text
cargo fmt --all -- --check                         PASS
cargo clippy --all-targets -- -D warnings          PASS
cargo build --release --locked                     PASS
invalid JSON runtime smoke                         PASS: status 400, upstream hits 0
admin immediate-disable smoke                      PASS: FAILURES 0, no extra upstream hit
fresh pgvector/pgvector:pg16 + cargo test all      PASS: lib 40/40, streaming integration 2/2
cargo audit                                        PASS: 0 vulnerabilities, 0 warnings
```

Note: `/tmp/brigto_invalid_json_smoke.py` still returns rc=1 when the old bug is absent; use printed `status 400 hits 0` as the assertion.

## Remaining open items

1. Benchmark B6 shell bug remains unless source changes after this audit:
   - `benchmarks/gate.sh:58 CONC=200 read ... < <(run_oha ...)`
   - `benchmarks/router-setup/bench/gate.sh:58 same bug`
2. Full SOTA benchmark matrix remains unproven. The existing `bench/results/20260916-200856/summary.json` is a useful non-stream conc=50 smoke, not a release-grade fastest-in-world artifact.
3. Dependency policy remains a decision: `sqlx` umbrella keeps SQLite/MySQL packages in metadata/lock even though root runtime path is Postgres and Redis is absent.
