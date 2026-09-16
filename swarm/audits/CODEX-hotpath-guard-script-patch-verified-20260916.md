# CODEX hot-path guard script patch — verified

Date: 2026-09-16 22:19 +07

Verdict: add a small static guard to CI/checks after the current hot-path patches are applied. It prevents regression into DB/Redis/fs/env/client-new in inference path, full backend response buffering, route-picker allocation, and reqwest gzip in fastest profile.

Patch:

```text
audits/CODEX-apply-hotpath-guard-script-20260916.patch
```

## What it checks

New script:

```bash
python3 scripts/hotpath_guard.py
```

Checks non-test source only:

- `src/handlers.rs` and `src/proxy/mod.rs` must not contain DB/Redis/filesystem/env reads or per-request reqwest client creation.
- `src/proxy/mod.rs` must not contain `.bytes().await` response buffering.
- `src/proxy/mod.rs` must not use `HashSet` for retry tracking.
- `src/route/mod.rs` must not allocate `Vec<Candidate>`/candidate vectors in picker path.
- `Cargo.toml` must not enable reqwest `gzip` feature.

## Current-source behavior

Against current source before patches, the guard fails as expected:

```text
HOTPATH_GUARD_FAIL src/proxy/mod.rs: found /\.bytes\(\)\.await/ — proxy must not buffer full backend response before returning client body
```

That is correct: current source still has `response.bytes().await` on the non-stream path.

## Patched-sequence validation

Temp tree:

```text
/tmp/brigto_hotpath_guard_sequence_FN81W1N3
```

Sequential order tested:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
git apply audits/CODEX-apply-no-gzip-identity-backend-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
git apply audits/CODEX-apply-hotpath-guard-script-20260916.patch
```

Checks:

```bash
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo test -q backend_request_forces_identity_encoding --lib
cargo test -q route --lib
```

Results:

- `hotpath_guard.py`: PASS (`HOTPATH_GUARD_PASS`)
- py_compile/fmt/check/clippy: PASS
- identity header unit test: PASS 1/1
- route unit tests: PASS 8/8

## Action for DeepSeek

Apply this after the production hot-path patches, then add it to the normal check target if desired. Do not apply it before fixing current source; it is expected to fail until `.bytes().await`, route allocations, and reqwest gzip are gone.
## Update 2026-09-16 22:22 +07 — locked build verified after Cargo.lock prune

`CODEX-apply-no-gzip-identity-backend-20260916.patch` was regenerated to include `Cargo.lock` changes. Without the lockfile change, `cargo check --locked` fails. With the regenerated patch, the full sequence passed in `/tmp/brigto_locked_sequence_fixed_eyQkRp8U`:

```bash
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
CARGO_INCREMENTAL=0 cargo build --release --locked --bins
```

Result: `HOTPATH_GUARD_PASS`, check locked PASS, release locked bins PASS.

