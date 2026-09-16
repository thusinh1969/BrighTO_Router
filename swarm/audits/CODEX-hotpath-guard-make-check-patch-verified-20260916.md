# CODEX hot-path guard in make check — verified

Date: 2026-09-16 22:22 +07

Verdict: after applying the hot-path fixes and `scripts/hotpath_guard.py`, wire the guard into `make check` so regressions fail before benchmark time is wasted.

Patch:

```text
audits/CODEX-apply-hotpath-guard-make-check-20260916.patch
```

## Patch behavior

`Makefile` changes:

```make
check:            ## fmt + hot-path guard + clippy chặt — chạy trước khi commit
	python3 scripts/hotpath_guard.py
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
```

## Validation

Temp tree:

```text
/tmp/brigto_make_guard_sequence_mpA1rR6F
```

Sequential order tested:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
git apply audits/CODEX-apply-no-gzip-identity-backend-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
git apply audits/CODEX-apply-hotpath-guard-script-20260916.patch
git apply audits/CODEX-apply-hotpath-guard-make-check-20260916.patch
make check
```

Result:

```text
python3 scripts/hotpath_guard.py
HOTPATH_GUARD_PASS
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
Finished `dev` profile ...
```

## Action for DeepSeek

Apply this only after the guard script and hot-path fixes. If applied before fixes, `make check` should fail, which is correct.
## Update 2026-09-16 22:22 +07 — locked build verified after Cargo.lock prune

`CODEX-apply-no-gzip-identity-backend-20260916.patch` was regenerated to include `Cargo.lock` changes. Without the lockfile change, `cargo check --locked` fails. With the regenerated patch, the full sequence passed in `/tmp/brigto_locked_sequence_fixed_eyQkRp8U`:

```bash
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
CARGO_INCREMENTAL=0 cargo build --release --locked --bins
```

Result: `HOTPATH_GUARD_PASS`, check locked PASS, release locked bins PASS.

