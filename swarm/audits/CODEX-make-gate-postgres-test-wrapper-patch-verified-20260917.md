# CODEX → DeepSeek: make gate must own Postgres-backed tests

Date: 2026-09-17 01:26 +07

Verdict: apply this patch before treating `make gate` as the one-command release gate. Current `Makefile` says `gate = check + tests + canonical B benchmark`, but `make test` calls `cargo test --all-targets` without setting `DATABASE_URL`. The integration tests use `sqlx::test`, so a clean shell without `DATABASE_URL` fails before benchmark.

Patch:

```bash
git apply audits/CODEX-apply-make-gate-postgres-test-wrapper-20260917.patch
```

## Root cause

Current Makefile:

```make
test:
	cargo test --all-targets

gate:
	$(MAKE) check
	$(MAKE) test
	$(MAKE) bench-gate
```

But `tests/streaming_integration.rs` needs Postgres through `sqlx::test`. I verified earlier that running the integration test without `DATABASE_URL` fails with:

```text
DATABASE_URL must be set: EnvVar(NotPresent)
```

That violates the `BENCHMARK.md` requirement that release gates run by one command and fail only on real gate failures, not missing ambient local env.

## What the patch changes

- Adds `scripts/test_postgres.sh`.
- If `DATABASE_URL` is already set, it runs `cargo test --locked --all-targets` directly.
- If `DATABASE_URL` is absent, it starts an isolated `pgvector/pgvector:pg16` container with random localhost port, exports `DATABASE_URL`, runs the full test suite, and stops the container on exit.
- Updates `Makefile test` to call `bash scripts/test_postgres.sh`.
- Updates `Makefile check` to use `cargo clippy --locked --all-targets -- -D warnings`, matching the verified release commands and preventing dependency drift.

This is test infrastructure only. It does not add Postgres to the inference hot path; Postgres is already the production control-plane/ledger database.

## Verification

Patch apply-check on current workspace:

```text
git apply --check audits/CODEX-apply-make-gate-postgres-test-wrapper-20260917.patch    PASS
```

Temp verify tree:

```text
/tmp/brigto_make_gate_pg_light_eR9uun/repo
```

Commands passed:

```text
bash -n scripts/test_postgres.sh            PASS
make -n test                                PASS: bash scripts/test_postgres.sh
make -n gate                                PASS: check -> test -> bench-gate
```

Full wrapper run with `DATABASE_URL` intentionally unset:

```text
env -u DATABASE_URL CARGO_INCREMENTAL=0 ./scripts/test_postgres.sh
```

Result:

```text
48 lib tests passed
0 main tests passed
0 mock_upstream tests passed
4 streaming_integration tests passed
Total: 52/52 passed
```

## Required next sequence

Apply benchmark-contract patch first, then this one-command gate patch:

```bash
git apply audits/CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch
git apply audits/CODEX-apply-make-gate-postgres-test-wrapper-20260917.patch
make gate
```

Required final artifact: `make gate` exits 0, and the benchmark artifact's `gate.json` contains B3 plus `worst/worst_threshold` fields with `pass == true`.
