# GLM architecture verdict — SOTA latency target, 2026-09-16

Mục tiêu audit này: chốt kiến trúc production để repo có cơ hội đạt router overhead thấp nhất có thể, không over-engineering, và chỉ dùng PostgreSQL/Redis khi thật sự cần cho production run.

Verdict ngắn: kiến trúc gốc trong `docs/llm-router-rust-plan.md` đúng hướng cho router rất nhanh, nhưng phải biến thành contract cứng: request path không được chạm PostgreSQL, không được chạm Redis ở profile fastest, không đọc disk/env, không parse JSON full body, không buffer streaming response, không tạo HTTP client mỗi request. Code hiện tại chưa đạt contract đó, như đã ghi chi tiết trong `audits/GLM-pass1.md`.

Không ai có thể tuyên bố “fastest in the world” chỉ bằng kiến trúc. Claim đó chỉ được phép sau benchmark direct-vs-router với payload thật 1K/50K/200K, stream/non-stream, concurrency 1/50/200. Nhưng kiến trúc dưới đây là đường ngắn nhất để đạt overhead sát giới hạn thực tế của Linux userspace proxy.

## Production Dependency Verdict

### PostgreSQL: YES, nhưng chỉ background

PostgreSQL cần cho production vì có 3 việc không nên nhét vào file/env:

1. Config lâu dài: teams, api_keys hash, backends, model_routes, budgets.
2. Ledger append-only: record usage để audit/billing/debug.
3. Admin API: tạo key, disable key, đổi budget, xem usage.

PostgreSQL bị cấm trong request hot path. Request handler không được `.await` DB, không query usage, không update budget trực tiếp. Luồng đúng:

```text
PostgreSQL -> config poll task -> ArcSwap<ConfigSnapshot> -> request reads snapshot in RAM
request -> LedgerSink.try_record() -> background writer -> PostgreSQL batch insert
```

Nếu PostgreSQL chết, router vẫn trả response. Ledger đi vào queue/fallback nền. Config giữ snapshot cuối. Admin API có thể lỗi 5xx, nhưng inference traffic không bị ảnh hưởng.

### Redis: NO cho fastest profile, YES chỉ cho strict global quota

Redis không cần cho production fastest nếu chấp nhận budget mềm theo plan v1: mỗi router instance giữ counter RAM, có thể vượt một lượng nhỏ trong vài giây khi chạy nhiều instance. Đây là lựa chọn đúng nếu router phục vụ nội bộ và mục tiêu số 1 là latency.

Redis chỉ đáng dùng khi có ít nhất một yêu cầu production sau:

1. Budget prepaid hoặc billing khách ngoài phải cứng tuyệt đối trên nhiều router instance.
2. RPM/concurrency phải global, không phải per-instance.
3. Có nhiều router active-active và overspend vài request là không chấp nhận được.

Nếu bật Redis, chỉ dùng 1 round-trip Lua `check_and_reserve` trước khi forward, rồi `commit_or_refund` sau response. Không dùng Redis cho route picking, health, ledger, model list, hoặc config. Redis profile phải là feature/implementation riêng, không làm chậm fastest profile.

Khuyến nghị hiện tại: giữ Redis là optional feature hoặc bỏ khỏi default dependency cho đến khi có yêu cầu strict quota. `Cargo.toml` đang có `redis` nhưng code chưa dùng; điều này không làm hot path chậm khi runtime, nhưng làm kiến trúc mơ hồ.

### Local fallback file: YES, nhưng không viết trực tiếp trong handler

Ledger fallback file cần cho production khi DB chết, vì usage không được mất. Nhưng request path không nên mở file hoặc fsync. Thiết kế đúng là:

```text
handler/proxy -> LedgerSink.try_record(event)
              -> in-memory bounded queue
              -> if primary full: overflow queue / fallback writer task
              -> file append JSONL outside request task
```

Nếu queue overflow cả primary lẫn fallback thì phải trả metric/error rõ, không silently drop. Với billing nghiêm túc, “drop usage silently” là P0.

## Hot Path Contract

Mỗi request sinh model phải đi đúng luồng sau. DeepSeek nên coi đây là acceptance contract, không phải gợi ý:

```text
1. read auth header
2. sha256 key
3. snapshot = cfg.load_full()
4. lookup key/team/route/backend metadata in RAM
5. read body Bytes with max limit
6. extract only top-level model + stream
7. check model permission
8. budget.reserve(est_tokens) in RAM atomic
9. concurrency guard acquire in RAM atomic
10. backend lease acquire: pick + inc inflight in one guard
11. optional byte splice stream_options.include_usage
12. forward Bytes through shared reqwest Client
13. stream backend chunks to client immediately; tap only chunks containing "usage"
14. on completion/drop: commit/refund budget, release guards, try_record ledger, emit metrics
```

Forbidden on this path:

- PostgreSQL query/insert/update.
- Redis call in fastest profile.
- `std::fs` or env lookup.
- `reqwest::Client::new()` or `Client::builder()` per request.
- `serde_json::Value` for full request body.
- `resp.bytes().await` for streaming.
- Holding DashMap/RwLock guard across `.await`.
- Returning before release of concurrency/backend inflight; use RAII guards.

## State Shape Verdict

Current `AppState` hides too much behind traits that do not expose required operations. That directly caused budget team/concurrency/backend inflight not being wired.

For v1 fastest, use concrete runtime state. This is simpler and safer than thin traits:

```rust
pub struct AppState {
    pub cfg: Arc<ArcSwap<ConfigSnapshot>>,
    pub budget: Arc<RamBudgetStore>,
    pub backends: Arc<RamBackendPool>,
    pub client: reqwest::Client,
    pub ledger: LedgerSink,
    pub metrics: MetricsHandle,
    pub max_body_bytes: usize,
}
```

If DeepSeek wants trait tests, traits must include the full behavior:

- `BudgetStore::reserve(...) -> Result<BudgetReservation, Decision>`
- `BudgetReservation::commit(actual_tokens)` or `BudgetStore::commit(reservation, actual_tokens)`
- `BudgetStore::acquire_concurrency(key) -> ConcurrencyGuard`
- `BackendPool::acquire(route) -> Result<BackendLease, NoBackend>`
- `BackendLease` releases inflight on Drop and records circuit result explicitly.

Do not keep current half-trait shape. It compiles but makes correct wiring easy to miss.

## Router/Proxy Boundary Verdict

There must be exactly one forwarding implementation. Right now `src/proxy/mod.rs` has the better streaming/splice/tap work, but `src/handlers.rs` bypasses it.

Correct ownership:

- `handlers.rs`: auth, body limit, model/stream extraction, permission, budget reservation, build `ProxyContext`, call proxy, attach response headers.
- `proxy/mod.rs`: backend request build, header filter, backend auth injection, byte splice, retry before first byte, streaming pump, usage tap, response body.
- `ledger/mod.rs`: background persistence only.
- `route/mod.rs`: pick/health/circuit/inflight only.
- `budget/mod.rs`: budget/rpm/concurrency only.

DeepSeek should delete local `forward_to_backend` and local `extract_usage` from `handlers.rs`. Duplicated proxy code is an architecture smell here; it already broke streaming.

## Request Body Parsing Verdict

For v1, serde into a small struct is acceptable only if it does not allocate skipped fields:

```rust
#[derive(Deserialize)]
struct RequestHead<'a> {
    #[serde(borrow)]
    model: &'a str,
    #[serde(default)]
    stream: bool,
}
```

If this does not compile cleanly with borrowed lifetimes, use `String` for model as a temporary v1 compromise. Do not use `serde_json::Value` in handler. At 200K context, full `Value` parse is needless allocator pressure.

The fastest final version should use a top-level scanner for `"model"` and `"stream"` after correctness tests pass. Do not implement a complex scanner until the serde struct path is benchmarked.

## Streaming Verdict

Streaming is the product. If router buffers stream, it is disqualified from the latency goal.

Required behavior:

- Return response headers as soon as upstream status/headers arrive.
- Body is `Body::from_stream(...)`.
- Each upstream chunk is sent to client immediately.
- Parse only chunks where `memmem::find(chunk, b"\"usage\"")` hits.
- OpenAI stream without `stream_options.include_usage` must be byte-spliced before forward.
- If client drops, backend request body/stream must drop too; ledger marks `client_aborted=true`.

Do not compute final ledger before returning streaming response. Ledger should be finalized by the stream task/drop reporter.

## Routing Verdict

Least-load by `inflight / weight` is the correct no-overengineering choice for v1. Do not add live vLLM metrics into the picker yet. It creates another background integration and another failure mode before the basic proxy is proven.

The missing piece is a backend lease:

```text
pick(route) -> backend_id
inc_inflight(backend_id)
return BackendLease { backend_id, pool }
Drop -> dec_inflight
```

`note_result` should be called once with transport result:

- connect error, first byte timeout, pre-body 5xx/429 -> failure
- successful response status from backend -> success for circuit, even if model output is 4xx client error
- after first byte sent -> no retry

Fallback to `fallback_backend_id` only when route explicitly declares it. Do not invent Claude fallback from model name or backend format.

## Budget Verdict

Fastest profile should use RAM atomic reservation. Current check-then-commit is not enough.

Minimal correct design:

```text
reserve(est_tokens):
  for each scope counter: CAS current -> current + est_tokens if <= limit
  if any scope fails: rollback previous reservations, return BudgetExceeded
  return BudgetReservation(scopes, est_tokens)

commit(actual_tokens):
  if actual > est: atomic add delta, allow bounded overshoot or reject next request
  if actual < est: atomic subtract refund
```

Scopes required:

- key total if key budget exists.
- key+model if key budget has per_model cap.
- team total if no key budget and team budget exists.
- team+model if team budget has per_model cap.

For multi-instance fastest, this is still soft globally but hard inside one process. That matches “no Redis unless truly needed”.

## Ledger Verdict

Ledger must be at-least-once, not best-effort silent.

Required design:

- Handler/proxy never sees `mpsc::Sender` directly.
- Expose `LedgerSink::try_record(event)`.
- Writer batches 100 or 1s into PostgreSQL.
- DB fail -> writer appends JSONL fallback.
- Channel full -> event enters fallback writer path, not dropped.
- Replay handles both `ledger_fallback.jsonl` and `ledger_fallback.replay`.
- `request_id` has UNIQUE index so replay idempotency is cheap.

PostgreSQL insert can be async and batched; it must not be part of client response latency.

## Metrics Verdict

Use the `metrics` crate already in `Cargo.toml`; do not hand-render custom names in handlers.

Required metrics for production signoff:

- `router_requests_total{team,key,model,backend,status}`
- `router_tokens_total{team,key,model,backend,direction,estimated}`
- `router_ttfb_seconds{model,backend,stream}`
- `router_overhead_seconds{model,backend,stream}`
- `router_backend_inflight{backend}`
- `router_budget_remaining{team,model}`
- `router_circuit_open{backend}`

`x-router-overhead-ms` must not equal total backend time. It should measure router work before first byte plus stream tap overhead if measurable. If exact per-stream overhead is hard, report pre-forward overhead in header and use histograms for measured internal segments. Do not publish misleading overhead.

## Admin/API Verdict

Admin is not part of fast path. Keep it boring:

- PostgreSQL access allowed.
- No Redis.
- No React/build step.
- Must be nested explicitly under `/admin`.
- IP allowlist must use socket peer addr. X-Forwarded-For is trusted only when peer is a configured trusted proxy.
- Admin changes should trigger immediate config reload by `Notify` or epoch flag; polling 5s remains fallback.

Do not let admin state create a separate DB dialect/schema story. It must use the same migration as config loader.

## DeepSeek One-Shot Fix Contract

Fix in this order, one pass:

1. Make migration valid SQL and align names/types: `format=openai|anthropic`, `first_byte_timeout_secs` or loader rename, `key_hash` representation consistent, correct demo hash or no demo key.
2. Replace `AppState` with concrete runtime state or extend traits fully. Wire config snapshot into `RamBackendPool::upsert_backend` and `RamBudgetStore::load_teams` after bootstrap and each reload.
3. Add shared `reqwest::Client` to state. No client construction per request.
4. Rewrite `handlers.rs` as thin pipeline. It must call A4 proxy, not implement forwarding.
5. Implement RAM reservation with rollback/refund. Add concurrency guard and backend lease RAII.
6. Replace raw ledger sender with `LedgerSink`; no silent drop on `try_send` failure.
7. Fix streaming path to return `Body::from_stream`, splice OpenAI usage, and finalize ledger from stream reporter.
8. Wire `/admin/*`, static portal, and peer-address admin allowlist.
9. Replace local metrics renderer with `metrics` exporter using locked names.
10. Run Docker gate, then run direct-vs-router bench against llama-server/vLLM. The repo can claim SOTA only when benchmark deltas prove it.

## Non-Goals To Protect Speed

Do not add these before the benchmark proves the simple router is insufficient:

- vLLM `/metrics` based scheduling.
- Redis default profile.
- DB reads in handler for latest budget.
- Provider format translation.
- Semantic cache.
- Prompt logging.
- Multi-region coordination.
- Complex admin frontend.
- Middleware stack that clones/buffers bodies.

The fastest architecture is deliberately small: RAM snapshot, atomic guards, one shared client pool, streaming proxy, background ledger.
