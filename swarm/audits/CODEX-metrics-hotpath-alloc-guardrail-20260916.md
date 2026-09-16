# CODEX metrics hot-path allocation guardrail

Date: 2026-09-16 22:16 +07

Verdict: current metrics emission allocates several `String`s per request. Do not add a metrics-handle cache yet unless `BRIGTO_HOTPATH_PROBE` shows `finish.metrics_emit` dominates. If it does dominate, fix it with retained metric handles / shared labels, not by removing observability or adding infrastructure.

## Evidence

Current `src/metrics.rs` emits dynamic labels like this:

```rust
metrics::counter!(
    "router_requests_total",
    "team" => team.to_string(),
    "key" => key.to_string(),
    "model" => model.to_string(),
    "backend" => backend.to_string(),
    "status" => status.to_string(),
)
.increment(1);
```

The same pattern exists in:

- `request_total`
- `tokens_total`
- `observe_ttfb`
- `observe_overhead`
- background gauges

In the current non-stream path, `CompletionReporter::finish()` runs before returning the buffered body to the client. That means metrics allocation can affect measured full-response latency. After `CODEX-apply-nonstream-response-streaming-20260916.patch`, response body starts earlier, but metrics still happens before the stream closes.

Local `metrics 0.24.6` source confirms the cost model:

- `metrics::Label::new` takes values `Into<SharedString>`.
- `SharedString` supports static, owned, and `Arc`-wrapped values.
- The macro docs allow labels as `String` or `&'static str`; current dynamic labels use `String` allocation.
- Retained `Key`/handles are cheap to clone after first construction.

## What not to do

Do not:

- remove labels blindly;
- drop request/tokens/latency metrics;
- add Redis/Postgres/exporter sidecar to solve this;
- introduce an unbounded DashMap cache keyed by arbitrary user-controlled labels without cardinality limits.

## If probe shows `finish.metrics_emit` dominates

Use one of these concrete fixes:

1. Precompute metric-safe shared labels during config load:
   - key id label string;
   - team id label string;
   - model label string;
   - backend label string.
2. Cache retained metric handles by bounded/cardinality-known tuples:
   - `(team_id, key_id, model, backend, status)` for request counter;
   - `(team_id, key_id, model, backend, direction, estimated)` for token counters;
   - `(model, backend, stream)` for histograms.
3. Prefer keeping cache inside `Metrics` / observability module, not scattered through proxy code.
4. Keep current metric names locked; if `router_ledger_dropped_total` is emitted, add it to `METRICS` and gate it.

Acceptance for a metrics optimization patch:

```bash
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
DATABASE_URL=postgres://... cargo test --lib
```

Then rerun focused probe. Keep the patch only if `finish.metrics_emit` drops materially without increasing p50/p99 elsewhere.
