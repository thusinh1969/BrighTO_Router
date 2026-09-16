# CODEX SOTA architecture audit — current blockers after Postgres-only conversion

Time: 2026-09-16 Asia/Ho_Chi_Minh.
Scope: current source tree. This is architecture guidance for DeepSeek after the current P0 correctness fixes.
Rule from `audits/README.md`: Codex does not edit `src/`; exact repair guidance only.

## Verdict

The main architecture direction is now correct for the fastest production profile:

- Hot request path does not call Postgres/Redis/filesystem/env.
- Config is loaded into `ArcSwap<ConfigSnapshot>` and read lock-free by handlers.
- `reqwest::Client` is built once in `main.rs`, reused in proxy and health checks.
- Streaming uses `Body::from_stream` and bounded channel size 1, so client backpressure does not create unbounded memory growth.
- Ledger is async channel -> background Postgres batch/file fallback, so DB write latency is off the response path.
- Postgres-only is the right DB choice for production control plane + ledger. Redis/Valkey should stay out unless measured strict multi-instance quota requires it.

Still, the repo cannot honestly claim SOTA/fastest yet. There are two P0 correctness bugs in `CODEX-current-p0-admin-patch-and-team-disable-20260916.md`, plus P1 benchmark/hot-path issues below.

## P0 must be fixed first

Do not start performance tuning until these are green:

1. `src/admin/mod.rs::update_team`: `Separated` inserts comma before `WHERE`, so admin PATCH fails on real Postgres.
2. `src/auth.rs::authorize_key`: enabled key under disabled/missing team is still accepted and proxied.

These are production correctness issues, not benchmark issues.

## P1 performance issue — full-body serde parse for only `model` and `stream`

Current hot path:

```rust
// src/handlers.rs
let body_bytes = to_bytes(body, state.max_body_bytes).await?;
let head: RequestHead<'_> = serde_json::from_slice(&body_bytes)?;
```

This avoids materializing the full OpenAI payload as `Value`, which is good. But serde still scans/skips the full JSON body to deserialize two fields. On 200K-token prompts, that CPU work scales with payload size.

Micro-measure on this machine, release build, same `RequestHead` shape as current code:

```text
serde 1k-model-first:      1.303 us/iter
serde 50k-model-first:    48.017 us/iter
serde 200k-model-first:  190.356 us/iter

fast byte scan 1k-model-first:     0.167 us/iter
fast byte scan 50k-model-first:    0.182 us/iter
fast byte scan 200k-model-first:   0.223 us/iter
```

This does not mean the router is too slow yet; 190us can still fit under a 2ms p99 target. It does mean the current implementation has payload-size-coupled CPU before the backend request starts, so SOTA/fastest must be proven with real 1K/50K/200K overhead gates before claiming victory.

Exact low-risk improvement if benchmark shows payload scaling:

- Replace `RequestHead` serde parse with a top-level-only extractor: `extract_request_head(body: &[u8]) -> Result<(&str, bool), BadRequest>`.
- The extractor should scan only the root object keys, skip nested arrays/objects/strings without allocation, collect root `model` string and root `stream` bool, and stop once both are found.
- If it sees escapes or malformed edge cases it cannot safely parse, fall back to serde for correctness.
- Do not parse or allocate `messages`, `tools`, `response_format`, images, or other payload fields.

This keeps current correctness while removing the easy CPU tax for normal OpenAI payloads where `model` and `stream` are near the top.

## P1 correctness/perf issue — `contains_stream_options` is global byte search

Current proxy splice gate:

```rust
backend.format == BackendFormat::OpenAi
    && stream_request
    && !contains_stream_options(&body)
```

`contains_stream_options` is raw `memmem` over the whole request body. It does not prove `stream_options` is a root field. If nested JSON or tool/message content contains a key named `stream_options`, router can skip injecting root `stream_options.include_usage=true`; then stream usage may be estimated even when the upstream would provide exact usage.

Exact fix: reuse the same top-level extractor/token scanner proposed above and expose `has_root_stream_options`. Then splice only when the root object lacks `stream_options`.

Do not replace this with full `serde_json::Value` parse/re-encode. That would make router overhead more payload-dependent and can reorder/drop unknown request fields.

## P1 benchmark/proof issue — current benchmark artifacts are not enough

Current repo has benchmark docs and scripts, but no current official run artifact proving fastest/SOTA:

- `bench/run.sh` root jq summary is fixed.
- `install/router-setup/bench/run.sh` and `benchmarks/router-setup/bench/run.sh` still contain broken jq: `|.*100`.
- `scripts/bench_smoke.py` is stale SQLite-era code; production is now Postgres-only.
- `benchmarks/BENCHMARK.md` defines the right gates, but the repo needs generated results from current binary.

Exact requirement before any “fastest” claim:

```text
B1/B2: router-direct overhead p50/p99 for 1k/50k/200k, concurrency 50, 3 runs
B3: overhead flatness, p50(200k)-p50(1k) <= threshold
B4/B5: streaming TTFB and per-chunk added gap
B6: saturation throughput, non-200 = 0
B11: Postgres outage does not raise p99 beyond threshold because ledger fallback stays off hot response path
```

Acceptance must be router minus direct against same upstream, same payloads, same load generator. Single mock smoke is not a SOTA proof.

## P2 hot-path allocation cleanup — only after P0/P1 proof

Current route/proxy selection allocates on every request:

```rust
// src/proxy.rs
let mut tried: HashSet<i64> = HashSet::new();

// src/route.rs
self.acquire_excluding(route, &HashSet::new())
let mut excluded = initial_excluded.clone();
let mut candidates: Vec<Candidate> = Vec::new();
let mut tied: Vec<Candidate> = ...collect();
```

This is not the current production blocker, but it is unnecessary work on the hot path.

Exact minimal cleanup if profiling shows route selection in p99:

- Replace per-request `HashSet<i64>` with a tiny `Vec<i64>` or fixed small local list of tried backend ids; route backend lists are normally small and linear `contains` is cheaper than hashing/allocating.
- Change `acquire_excluding` to accept `&[i64]` or a tiny local helper instead of cloning a `HashSet`.
- Rewrite `choose_candidate` to track the best candidate in one pass instead of allocating `Vec<Candidate>` and a second `tied` vector. For ties, either choose first deterministically or use reservoir sampling without allocation.
- Avoid adding a dependency for this unless benchmarks prove it matters. No `smallvec` needed for the first pass.

## P2 budget allocation cleanup — only if budget-heavy workloads show it

`RamBudgetStore::reserve_scope` builds `UsageKey { model: model.to_string(), ... }` for budgeted requests. That allocates per scoped model budget. It is acceptable while budget checks are optional and not yet proven as p99 bottleneck.

If budget-heavy benchmark shows this in profiles, fix by using interned model ids or `Arc<str>` already held by the snapshot. Do not add this complexity before measurement.

## Packaging cleanup required for no-overengineering production run

Root production compose is now Postgres-only, which matches the user requirement. Stale install artifacts still advertise or package Redis/Valkey:

```text
install/docker-compose.yml
install/router-setup/docker-compose.yml
install/README.md
install/README-setup.md
install/router-setup/README-setup.md
install/Cargo.toml
install/router-setup/Cargo.toml
install/router-setup/Makefile
```

Exact direction:

- Remove Redis crate and Valkey service from install copies unless a separate optional feature is added with benchmark evidence.
- If keeping future docs, phrase it as optional strict multi-instance quota only, not default production dependency.
- Keep default production run: router + external Postgres, no Redis.

## Correct order for DeepSeek

1. Fix P0 admin PATCH and disabled-team auth bypass.
2. Re-run compile/fmt/clippy/Postgres tests/release smoke with team-disable check.
3. Replace stale SQLite/Redis install and bench scripts so proof path matches current Postgres-only architecture.
4. Run benchmark matrix from `benchmarks/BENCHMARK.md` on current binary.
5. Only if benchmark/profile shows p99 tax, implement top-level request-head scanner and route allocation cleanup.

Do not add Redis, new services, or broad traits to solve any issue in this file. The current architecture needs small correctness fixes and measured hot-path pruning, not another subsystem.
