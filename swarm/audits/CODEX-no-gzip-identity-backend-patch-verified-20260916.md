# CODEX no-gzip / identity backend patch — verified

Date: 2026-09-16 22:12 +07

Verdict: for fastest proxy mode, do not let `reqwest` transparently decompress backend responses. The router needs pass-through semantics and predictable CPU. Backend requests should force `Accept-Encoding: identity`; client `Accept-Encoding` must not be forwarded upstream.

Patch:

```text
audits/CODEX-apply-no-gzip-identity-backend-20260916.patch
```

## Root cause

Current `Cargo.toml` enables `reqwest` feature `gzip`:

```toml
reqwest = { ..., features = ["rustls", "stream", "json", "gzip"] }
```

This is wrong for a fastest LLM proxy:

- If backend returns compressed JSON/SSE, router pays decompression CPU before usage parsing.
- Transparent decompression changes pass-through behavior and can make response headers/body semantics harder to reason about.
- If gzip is removed but client `Accept-Encoding: gzip` is forwarded, backend can still return compressed body and router usage parsing can fail. The right fix is both: remove auto-decode and ask backend for identity.

## Patch behavior

`Cargo.toml` / `Cargo.lock`:

- removes `gzip` from reqwest features.
- prunes gzip transitive packages from lockfile (`async-compression`, `flate2`, `compression-*`, `miniz_oxide`, `zlib-rs`, etc.).
- documents why proxy mode avoids gzip auto-decode.

`src/proxy/mod.rs`:

- drops incoming `accept-encoding` in `must_drop_header`.
- inserts `Accept-Encoding: identity` on backend requests.
- adds unit test `backend_request_forces_identity_encoding`.

No Redis. No DB. No new dependency. This is a smaller dependency graph and less hot-path CPU variance.

## Validation

Prototype temp tree:

```text
/tmp/brigto_no_gzip_identity_A5WNPFDS
```

Checks:

```bash
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo test -q backend_request_forces_identity_encoding --lib
cargo test -q anthropic_version --lib
```

Results:

- fmt/check/clippy: PASS
- identity header unit test: PASS 1/1
- existing Anthropic header tests: PASS 2/2

Dependency evidence from patched chain:

```text
cargo tree -e features -i async-compression
error: package ID specification `async-compression` did not match any packages

cargo tree -e features -i flate2
error: package ID specification `flate2` did not match any packages

cargo tree -e features -i reqwest
reqwest features active from brigto-router: rustls, json, stream
```

Full current patch-chain validation temp tree:

```text
/tmp/brigto_no_gzip_sequence_gxvMxLST
```

Sequential order tested:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
git apply audits/CODEX-apply-no-gzip-identity-backend-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
git apply audits/CODEX-apply-hotpath-probe-temporary-20260916.patch
```

Checks after full sequence:

- `python3 -m py_compile scripts/bench_real.py`: PASS
- `cargo fmt --all -- --check`: PASS
- `CARGO_INCREMENTAL=0 cargo check --all-targets`: PASS
- `CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test -q backend_request_forces_identity_encoding --lib`: PASS 1/1
- `cargo test -q route --lib`: PASS 8/8
- `cargo test --test streaming_integration` with fresh Postgres: PASS 3/3

## Apply guidance for DeepSeek

Place this after benchmark-truth/payload-filter and before the route/nonstream patches:

```bash
git apply audits/CODEX-apply-no-gzip-identity-backend-20260916.patch
```

If a real provider requires compressed responses, make it a measured compatibility mode, not the default fastest profile. The default benchmark/profile should be identity encoding.
## Update 2026-09-16 22:22 +07 — locked build verified after Cargo.lock prune

`CODEX-apply-no-gzip-identity-backend-20260916.patch` was regenerated to include `Cargo.lock` changes. Without the lockfile change, `cargo check --locked` fails. With the regenerated patch, the full sequence passed in `/tmp/brigto_locked_sequence_fixed_eyQkRp8U`:

```bash
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
CARGO_INCREMENTAL=0 cargo build --release --locked --bins
```

Result: `HOTPATH_GUARD_PASS`, check locked PASS, release locked bins PASS.

