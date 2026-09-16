# GLM pass 1 audit — 2026-09-16

Phạm vi: đọc `docs/llm-router-rust-plan.md`, `docs/agents/*`, `audits/*`, toàn bộ `src/`, `migrations/`, `static/`, `scripts/`, `swarm/out/*` chính. Không sửa code trong `src/`.

Lệnh đã chạy:

```bash
docker run --rm -v "$PWD":/app -v brigto-cargo-registry:/usr/local/cargo/registry \
  -v brigto-rustup:/usr/local/rustup -w /app rust:1.98.1-bookworm bash -c \
  "cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test"
```

Kết quả: fail ở clippy với 7 lỗi (`src/ledger/mod.rs:31`, `src/admin/mod.rs:204`, `src/handlers.rs:380`, `381`, `387`, `474`, `495`).

```bash
docker run --rm -v "$PWD":/app -v brigto-cargo-registry:/usr/local/cargo/registry \
  -v brigto-rustup:/usr/local/rustup -w /app rust:1.98.1-bookworm bash -c "cargo test --all-targets"
```

Kết quả: compile được nhưng `ledger::tests::db_down_writes_file_replay_on_reconnect` fail. Output chính: `ledger replay failed: error returned from database: (code: 1) no such table: usage_ledger` và `Error: No such file or directory (os error 2)`.

## P0 — migration không chạy, schema không khớp loader/admin

File: `migrations/0001_init.sql:1`, `migrations/0001_init.sql:12`, `migrations/0001_init.sql:21`, `migrations/0001_init.sql:33`, `migrations/0001_init.sql:77`, `src/config/mod.rs:91`, `src/config/mod.rs:112`, `src/config/mod.rs:171`, `src/admin/mod.rs:371`.

Bằng chứng:

```bash
docker run --rm -v "$PWD":/app -w /app rust:1.98.1-bookworm bash -c 'python3 - <<PY
import sqlite3, pathlib
sql = pathlib.Path("migrations/0001_init.sql").read_text()
con = sqlite3.connect(":memory:")
try:
    con.executescript(sql)
    print("migration ok")
except Exception as e:
    print(type(e).__name__ + ": " + str(e))
PY'
```

Output bắt đầu bằng `OperationalError: near "```sql ...": syntax error`. File migration đang chứa Markdown fence ` ```sql ` và ` ``` `, nên migration fail ngay.

Sau khi bỏ fence vẫn còn lệch schema:

- `migrations/0001_init.sql:12` cho phép format `open_ai`, nhưng `src/config/mod.rs:91` chỉ accept `openai` và `anthropic`.
- `migrations/0001_init.sql:21` tạo `first_byte_timeout_secs`, nhưng `src/config/mod.rs:112` query `first_byte_timeout`.
- `migrations/0001_init.sql:33` khai `key_hash BLOB`, `src/admin/mod.rs:371` insert bytes, nhưng `src/config/mod.rs:171` đọc `String` và parse hex.
- `migrations/0001_init.sql:77` seed all-zero hash, không phải hash của `lc-dev0001`. Bằng chứng: `printf lc-dev0001 | sha256sum` trả `6f48c439d85e6ec2cdf3cb47bfe0b0853e8278724b56140a9edb7d3e31a78819`.

Đề xuất fix:

1. Xóa Markdown fence khỏi migration.
2. Chốt một representation cho `BackendFormat`: dùng `openai` để khớp plan và loader, hoặc đổi loader sang `open_ai` nhưng phải thống nhất contract/admin/docs.
3. Đổi loader query sang `first_byte_timeout_secs` hoặc đổi migration sang `first_byte_timeout`.
4. Chốt `key_hash` là hex `TEXT` hoặc bytes `BLOB`. Tôi khuyến nghị `TEXT` hex cho dual-dialect `sqlx::Any` đơn giản, rồi admin insert `hex_encode(hash)`.
5. Seed đúng hash hoặc bỏ seed key nếu chưa muốn có key demo.

## P0 — router không có backend để pick

File: `src/main.rs:73`, `src/main.rs:74`, `src/route/mod.rs:67`, `src/handlers.rs:259`.

Bằng chứng: `main` tạo `RamBudgetStore::new()` và `RamBackendPool::new()` rồi cast ngay thành trait object:

```rust
let budget_store: Arc<dyn BudgetStore> = Arc::new(RamBudgetStore::new());
let pool: Arc<dyn BackendPool> = Arc::new(RamBackendPool::new());
```

Không có chỗ nào gọi `RamBackendPool::upsert_backend` từ snapshot. `rg -n "upsert_backend|RamBackendPool|pool.pick" src` chỉ thấy `upsert_backend` trong impl/test route, `RamBackendPool::new()` ở `main`, và `pool.pick()` ở handlers/proxy.

Hậu quả: `src/handlers.rs:259` gọi `state.inner.pool.pick(&route)`, nhưng `states` trong pool rỗng, nên request thật luôn 503 `no backend available` dù config có backends.

Đề xuất fix: không ẩn concrete pool trước khi nạp state. Tạo `Arc<RamBackendPool>`, sau bootstrap iterate `cfg.load_full().backends.values()` để `upsert_backend`, spawn task sync pool mỗi lần config poll thành công, rồi clone/cast sang `Arc<dyn BackendPool>` cho `AppState`.

## P0 — hot path không stream, không dùng A4 proxy, không tự chèn `stream_options.include_usage`

File: `src/handlers.rs:212`, `src/handlers.rs:282`, `src/handlers.rs:288`, `src/handlers.rs:346`, `src/handlers.rs:467`, `src/proxy/mod.rs:536`, `docs/agents/b1_glue.md:21`, `docs/agents/a4_proxy.md:8`.

Bằng chứng:

- B1 yêu cầu stream SSE từng chunk và dùng A4 tap usage (`docs/agents/b1_glue.md:21`).
- A4 có `proxy_forward` và splice usage (`src/proxy/mod.rs:536`, `src/proxy/mod.rs:593`).
- Handler không gọi `proxy_forward`. Nó gọi local `forward_to_backend` ở `src/handlers.rs:282`, sau đó `resp.bytes().await` ở `src/handlers.rs:288`, rồi trả `Body::from(upstream_body)` ở `src/handlers.rs:346`.
- Handler parse request thành `serde_json::Value` ở `src/handlers.rs:212`, tức allocate full JSON body thay vì struct hai field.
- Handler không splice `stream_options.include_usage`; nếu client stream không gửi option này, fixture thật cho thấy backend không trả usage (`tests/fixtures/stream_no_usage_option.sse`).

Hậu quả: streaming bị biến thành buffered response, TTFB của client bằng thời gian hoàn tất upstream; mục tiêu TTFB delta < 3 ms ở 200K không thể đạt. Ledger streaming cũng phải đợi toàn bộ response mới parse.

Đề xuất fix: B1 phải xóa local `forward_to_backend`/`extract_usage` hoặc chỉ giữ thin wrapper, build `Request<Bytes>` và `ProxyContext`, rồi gọi `crate::proxy::proxy_forward`. Request model/stream parse bằng struct nhỏ hoặc scanner top-level. A4 phải là nơi duy nhất forward/tap/splice stream.

## P0 — budget team, concurrency và backend inflight không được wire

File: `src/budget/mod.rs:100`, `src/budget/mod.rs:115`, `src/budget/mod.rs:143`, `src/route/mod.rs:89`, `src/route/mod.rs:96`, `src/main.rs:73`, `src/handlers.rs:242`, `src/handlers.rs:259`.

Bằng chứng: `RamBudgetStore` có `load_teams`, `try_acquire_concurrency`, `release_concurrency`; `RamBackendPool` có `inc_inflight`, `dec_inflight`. Nhưng `AppState` chỉ giữ trait `BudgetStore`/`BackendPool`, mà trait không có các method này. `rg` xác nhận các method đó chỉ được gọi trong unit test, không được gọi trong `main`/`handlers`.

Hậu quả:

- Team budgets trong snapshot không bao giờ được nạp vào `RamBudgetStore`, nên chỉ key-level budget có tác dụng.
- `concurrency_limit` của key không có tác dụng.
- Least-load không thấy request thật vì backend inflight không tăng/giảm; mọi backend có score 0 nên chọn gần như random.
- `max_inflight` backend cũng không bảo vệ được nếu inflight luôn 0.

Đề xuất fix: mở rộng contract hoặc tạo guard object trong concrete state. Cách sạch nhất: `BudgetStore::try_acquire_concurrency/release_concurrency` và `BackendPool::mark_inflight_start/end`, hoặc đổi `AppState` giữ concrete wrapper có các method đó. Handler phải dùng RAII guard để release khi mọi path return/drop.

## P0 — TOCTOU budget vẫn tồn tại

File: `src/contract.rs:134`, `src/budget/mod.rs:179`, `src/budget/mod.rs:244`, `src/budget/mod.rs:263`.

Bằng chứng: trait `BudgetStore` vẫn là `try_reserve(&self, ..., est_tokens) -> Decision` và `commit(&self, ..., tokens)`. Impl hiện tại đọc usage ở `check_budget` (`src/budget/mod.rs:179`) nhưng chỉ cộng ở `commit` sau khi upstream xong (`src/budget/mod.rs:263` trở đi).

Hai request đồng thời cùng key/team có thể cùng thấy remaining đủ, cùng được Allow, rồi cùng commit sau đó. Đây đúng P0 mà C1 đã phát hiện trong `swarm/out/c1_audit.response.md`.

Đề xuất fix: `try_reserve` phải atomically reserve `est_tokens` bằng CAS trên counter, trả `Reservation { scopes, est_tokens }`; `commit(reservation, actual_tokens)` điều chỉnh phần chênh. Nếu chấp nhận budget mềm do multi-instance thì vẫn cần atomic per-process để không vượt lớn trong một instance.

## P0 — ledger event có thể mất im lặng khi channel đầy

File: `src/handlers.rs:338`, `src/proxy/mod.rs:324`, `src/proxy/mod.rs:375`, `src/ledger/mod.rs:30`.

Bằng chứng: plan yêu cầu channel đầy thì ghi file fallback, không drop. Nhưng handler chỉ:

```rust
let _ = state.inner.ledger_tx.try_send(event);
```

A4 `UsageReporter` cũng chỉ ignore error từ `try_send`. `LedgerWriter` có fallback khi DB fail, nhưng không có API nào cho hot path ghi fallback khi channel full.

Hậu quả: dưới burst lớn hoặc ledger writer chết, usage mất vĩnh viễn và budget/ledger không còn đúng.

Đề xuất fix: tạo `LedgerSink` thay vì expose raw `mpsc::Sender`: `try_record(event)` thử channel, nếu full thì append JSONL bằng non-blocking dedicated fallback worker hoặc `spawn_blocking`/background queue riêng. Nếu nhất quyết hot path không đụng disk, phải có bounded loseless overflow khác; hiện tại không đạt yêu cầu “không bao giờ ghi 0 rồi im”.

## P1 — ledger replay không idempotent với file `.replay`, test hiện fail

File: `src/ledger/mod.rs:97`, `src/ledger/mod.rs:189`, `src/ledger/mod.rs:197`, `src/ledger/mod.rs:217`, `src/ledger/mod.rs:377`.

Bằng chứng: `cargo test --all-targets` fail ở `ledger::tests::db_down_writes_file_replay_on_reconnect`. Trong test, writer background ở `src/ledger/mod.rs:383` tự reconnect qua `connect_pool()` ở `src/ledger/mod.rs:97`, gặp DB khác không có schema, rename fallback thành `.replay`, làm test sau đó không thấy fallback nữa.

Thiết kế replay cũng nguy hiểm: `replay_fallback` chỉ check `fallback.exists()` (`src/ledger/mod.rs:192`), rename sang `.replay` (`src/ledger/mod.rs:197`), và chỉ xóa `.replay` khi xong (`src/ledger/mod.rs:217`). Nếu process crash giữa chừng, lần sau bỏ qua `.replay`, các dòng bị kẹt.

Đề xuất fix: xử lý cả `fallback` và `fallback.replay` khi khởi động; replay theo transaction/batch, nếu lỗi phải rename `.replay` về fallback hoặc giữ danh sách pending rõ ràng. Test nên tắt auto reconnect hoặc inject connect function để không đụng DB global.

## P1 — backend key ref resolution không thống nhất

File: `src/config/mod.rs:246`, `src/handlers.rs:277`, `src/proxy/mod.rs:56`.

Bằng chứng: config comment và `check_api_key_ref` hỗ trợ `env:NAME` và `file:/path`. Handler lại gọi `std::env::var(&backend.api_key_ref)` trực tiếp, nên `env:BACKEND_KEY` và `file:/path` đều fail. A4 proxy có resolver riêng nhưng chỉ hiểu env raw hoặc path heuristic, không hiểu prefix `env:`/`file:`.

Đề xuất fix: một hàm public duy nhất `resolve_backend_key(api_key_ref)` trong config/secrets module, dùng ở bootstrap/proxy/admin nếu cần. `Backend` trong snapshot nên chứa key đã resolve hoặc `SecretString` runtime-only nếu muốn hot path không đọc env/disk mỗi request.

## P1 — admin auth tin header spoofable và admin router chưa được serve

File: `src/admin/mod.rs:35`, `src/admin/mod.rs:118`, `src/admin/mod.rs:171`, `src/main.rs:88`, `src/handlers.rs:124`.

Bằng chứng: admin lấy IP từ `x-forwarded-for` hoặc `x-real-ip` do client gửi (`src/admin/mod.rs:118`) rồi allowlist (`src/admin/mod.rs:187`). Không đọc socket peer addr, không kiểm tra request đến từ trusted proxy. `main` chỉ gọi `handlers::router(router_state)`; `handlers::router` không nest `crate::admin::router()` và cũng không serve `static/index.html`.

Hậu quả: nếu admin router được nest sau này mà vẫn giữ auth hiện tại, client có thể tự set `X-Forwarded-For: 127.0.0.1` để qua default allowlist. Hiện tại admin API/portal lại chưa truy cập được qua binary chính.

Đề xuất fix: dùng `ConnectInfo<SocketAddr>` làm nguồn IP mặc định. Chỉ tin XFF khi remote peer thuộc CIDR reverse proxy tin cậy. Nest admin ở `/admin` trong router chính và serve portal tĩnh theo plan.

## P1 — metrics không đúng plan và header overhead đo sai

File: `src/metrics.rs:6`, `src/handlers.rs:78`, `src/handlers.rs:305`, `src/handlers.rs:329`, `src/handlers.rs:371`.

Bằng chứng: `src/metrics.rs` khóa tên `router_requests_total`, `router_tokens_total`, histogram TTFB/overhead. Handler tự render `brigto_router_*` counters, không dùng crate `metrics`, không có label key/team/model/backend/status, không có token counters/histograms. `ttfb_ms = total_ms` ở `src/handlers.rs:306`, nên `router_overhead_ms` trong ledger bằng 0 (`src/handlers.rs:329`), còn header `x-router-overhead-ms` lại set bằng `total_ms` (`src/handlers.rs:371`).

Đề xuất fix: Bỏ `Metrics` local trong handlers. Dùng `metrics` crate và exporter Prometheus. Đo TTFB tại thời điểm nhận header/byte đầu upstream; overhead header phải là thời gian router tự xử lý trước khi gửi byte đầu, không gồm thời gian backend generate.

## P1 — reqwest client tạo mới mỗi request, mất connection pool

File: `src/handlers.rs:447`, `src/proxy/mod.rs:542`.

Bằng chứng: handler gọi `reqwest::Client::new()` trong `forward_to_backend`; A4 `proxy_forward` cũng build client mới mỗi request. Plan yêu cầu connection pool keep-alive per backend.

Đề xuất fix: `AppState` giữ `reqwest::Client` dùng chung, cấu hình connect timeout/pool/idle timeout một lần. A4 nhận client từ state, không tạo trong hot path.

## P1 — auth module tốt nhưng bị bypass, expiry check lệch

File: `src/auth.rs:33`, `src/handlers.rs:184`, `src/handlers.rs:197`.

Bằng chứng: `auth::authorize_detailed` phân biệt `InvalidKey`, `Expired`, `Disabled`, `ModelNotAllowed`, nhưng handler tự lookup hash và tự check. Handler dùng `if now > expires_at`; auth module dùng `if exp <= now`. Tại đúng giây hết hạn, handler còn cho request qua.

Đề xuất fix: handler dùng `crate::auth::hash_key` và `authorize_detailed`. Giữ một nguồn sự thật cho semantics 401/403.

## P2 — các lỗi clippy cơ học đang chặn gate

File: `src/ledger/mod.rs:31`, `src/admin/mod.rs:204`, `src/handlers.rs:380`, `src/handlers.rs:381`, `src/handlers.rs:387`, `src/handlers.rs:474`, `src/handlers.rs:495`.

Bằng chứng: gate Docker fail trước khi chạy test vì `-D warnings`. Đây là sửa cơ học, nhưng phải làm trước khi mọi gate xanh.

Đề xuất fix: remove `mut` thừa, collapse nested `if let`. Sau đó chạy lại full gate.

## Thứ tự fix đề xuất cho DeepSeek

1. Sửa migration/schema/hash seed để DB boot được.
2. Wire concrete state: `RamBackendPool::upsert_backend`, health loop, budget `load_teams`, concurrency guard, backend inflight guard.
3. Thay handler forward path bằng A4 `proxy_forward`, giữ streaming thật, splice usage, client pool dùng chung.
4. Sửa BudgetStore reservation P0.
5. Bọc ledger sender bằng sink có fallback khi channel full; sửa replay `.replay`.
6. Nest admin + portal, sửa admin IP auth bằng peer addr/trusted proxy.
7. Sửa metrics đúng tên/label/histogram và overhead/TTFB.
8. Dọn clippy và chạy lại full Docker gate.
