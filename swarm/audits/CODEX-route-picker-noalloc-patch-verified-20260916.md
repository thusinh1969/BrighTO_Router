# PATCH READY — route picker no-alloc common path

Time: 2026-09-16 21:48 +07.  
Scope: current real worktree + temp prototype.  
Codex rule in this repo: patch/audit only; do not edit `src/` directly from Codex.

## Verdict

This is a safe follow-up patch **after** the benchmark-truth patch lands:

```bash
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
```

It removes avoidable heap allocation from the normal route-pick/retry exclusion path without changing backend lease RAII, circuit breaker semantics, fallback policy, or production dependencies.

Do not apply it before `audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch` if doing benchmark claims. Benchmark truth still comes first.

## Source problem fixed

Current real worktree allocates on the hot path:

```text
src/proxy/mod.rs:614   let mut tried: HashSet<i64> = HashSet::new();
src/route/mod.rs:131   acquire() calls self.acquire_excluding(route, &HashSet::new())
src/route/mod.rs:141   let mut excluded = initial_excluded.clone();
src/route/mod.rs:149   fallback creates ModelRoute { backend_ids: vec![fallback], ... }
src/route/mod.rs:204   let mut candidates: Vec<Candidate> = Vec::new();
src/route/mod.rs:275   let mut tied: Vec<Candidate> = ...collect();
```

That is unnecessary for the common case: one healthy primary backend, no retry, no fallback. At strict B1 p50 targets, per-request allocation in route selection should not survive if it is easy to remove.

## Patch behavior

Patch file:

```text
audits/CODEX-apply-route-picker-noalloc-20260916.patch
```

What it changes:

```text
src/route/mod.rs:
  - removes hot-path HashSet API from acquire_excluding
  - adds stack-backed BackendExclusions with inline capacity 8
  - falls back to Vec only after more than 8 excluded backend IDs
  - rewrites choose_candidate as one pass
  - keeps random tie-break via reservoir selection; no tied Vec
  - avoids constructing fallback ModelRoute/Vec for fallback_backend_id

src/proxy/mod.rs:
  - replaces HashSet tried set with BackendExclusions
  - keeps retry-before-byte behavior unchanged
```

No new crate. No Redis. No DB in request path.

## Verified checks in temp

Temp path:

```text
/tmp/brigto_route_noalloc_KwzJMVMI
```

Patch applies cleanly on current real worktree:

```text
$ git apply --check audits/CODEX-apply-route-picker-noalloc-20260916.patch
PASS
```

Patch also applies cleanly after the benchmark-truth patch:

```text
$ git apply --check \
    audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch \
    audits/CODEX-apply-route-picker-noalloc-20260916.patch
PASS
```

Validation in temp:

```text
cargo fmt --all -- --check                                     PASS
cargo test -q route --lib                                      PASS: 8 passed
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
```

Full lib tests with a real pgvector Postgres container:

```text
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:54551/llm_router cargo test --lib
PASS: 45 passed, 0 failed
```

A first `cargo test --lib` without `DATABASE_URL` failed four `#[sqlx::test]` tests because sqlx requires `DATABASE_URL`; that failure is environmental and disappeared with real Postgres.


## Combined patch validation

Applied together in temp path:

```text
/tmp/brigto_combined_truth_route_JuisevvH
```

Order:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
```

Checks:

```text
python3 -m py_compile scripts/bench_real.py                    PASS
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
cargo test -q route --lib                                      PASS: 8 passed
```

## Acceptance after DeepSeek applies it

Run:

```bash
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo test -q route --lib
```

If `DATABASE_URL` is available, also run:

```bash
cargo test --lib
```

Then benchmark only with the truthful harness:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Keep the patch only if 1k/50k/200k do not regress. Expected benefit, if visible, is mostly on 1k p50 and high-RPS routing overhead; it is not expected to fully solve the 200k body-path miss by itself.
