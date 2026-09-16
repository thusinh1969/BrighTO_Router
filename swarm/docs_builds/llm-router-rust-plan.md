# LLM Router (Rust) — Kiến trúc & Kế hoạch v1

Mục tiêu một câu: **một binary Rust đứng trước toàn bộ LLM backend của GORIC, kiểm soát ai được gọi gì, đếm token đúng đến từng key/team/model, và không làm chậm request dù context 200K.**

---

## 0. Nguyên tắc thiết kế (đọc trước, mọi quyết định bên dưới đều suy từ đây)

1. **Router chỉ là ống nước có đồng hồ.** Nó không hiểu nội dung prompt, không dịch format, không cache, không guardrail. Việc đó của tầng gọi (Hermes) hoặc của model.
2. **Chi phí xử lý không được tỷ lệ với độ dài prompt.** Đây là lý do tồn tại của project. Mọi bước trên đường đi của request phải là O(1) hoặc O(bytes) một lần duy nhất (copy bytes), không bao giờ parse/re-encode toàn bộ JSON.
3. **Không tokenize trong router.** Số token lấy từ `usage` mà vLLM/llama-server/Claude trả về — đó là số thật. Ước lượng chỉ dùng để chặn trước, không dùng để tính tiền.
4. **Hot path không chạm DB, không chạm disk.** Config nằm sẵn trong RAM. Ghi log/usage đi qua hàng đợi nền, không bao giờ block response.
5. **Không lưu prompt/response.** Log chỉ metadata. Vừa nhanh, vừa khỏi lo PHI.
6. **Production ≠ demo.** Acceptance test chạy trên vLLM thật, payload thật, không mock.

---

## 1. Phạm vi v1

### Làm
- Endpoint OpenAI-compatible: `POST /v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, `GET /v1/models`
- Endpoint Anthropic-native: `POST /v1/messages` (cho Claude backup, body giữ nguyên format Anthropic)
- Streaming (SSE) và non-streaming
- API key per team/agent → xác thực, giới hạn model, budget token, rate limit
- Route model → nhiều backend, cân tải theo tải thực (ít request đang chạy nhất), failover khi backend chết
- Đếm token per key / team / model / backend, ghi sổ cái (ledger) không sửa được
- Prometheus metrics + response header cho biết router tốn bao nhiêu ms
- Admin API + 1 trang web tối giản để tạo key, set budget, xem usage

### Không làm (cố tình)
- Dịch format giữa provider (OpenAI ↔ Anthropic). Client gọi đúng endpoint của format mình muốn.
- Semantic cache, prompt cache injection, guardrails, PII, MCP, prompt repo
- Multi-region, gossip cluster, OIDC/SSO (admin dùng 1 master key + IP allowlist)
- Portal đẹp. Dashboard dùng Grafana trên Prometheus.

Thêm gì sau này thì thêm — nhưng chỉ khi đo được là cần.

---

## 2. Bức tranh tổng thể

```
Hermes agents / apps
        │  Authorization: Bearer lc-xxxx   (key của team/agent)
        ▼
┌──────────────────────────────────────────────────────┐
│  LLM Router (1 binary Rust, 2 instance sau VIP)      │
│                                                      │
│  ① nhận request, đọc header → tra key trong RAM      │
│  ② đọc body (bytes), chỉ lấy `model` + `stream`      │
│  ③ check budget/rate limit từ counter trong RAM      │
│  ④ chọn backend (khoẻ + ít việc nhất + trọng số)     │
│  ⑤ forward body nguyên bytes, đổi Authorization      │
│  ⑥ stream response về client từng chunk, chỉ "nhìn"  │
│     chunk nào có chữ "usage" để lấy số token         │
│  ⑦ kết thúc: cộng counter RAM, đẩy 1 record vào      │
│     queue nền → batch ghi Postgres                   │
└──────────────────────────────────────────────────────┘
        │ HTTP/1.1 keep-alive, connection pool
        ▼
vLLM (B300 ×8) · vLLM (GX10) · llama-server (A6000) · Anthropic API (backup)
```

Postgres: cấu hình (backend, model route, team, key, budget) + ledger usage.
Router load config từ Postgres lúc start, sau đó poll mỗi 5 giây; thay đổi được đổi nguyên khối trong RAM (atomic swap), hot path không bao giờ chờ lock.

---

## 3. Đường đi của một request — chi tiết từng bước và lý do

### ① Xác thực
- Lấy key từ `Authorization: Bearer ...` hoặc `x-api-key` (để SDK Anthropic dùng được).
- SHA-256 key → tra trong `HashMap` trong RAM (snapshot config). Không có → 401 ngay, chưa đọc body.
- Key inactive / hết hạn → 401.

### ② Đọc body
- Đọc toàn bộ body vào RAM (kiểu `Bytes`). Cần giữ nguyên để (a) retry sang backend khác nếu backend đầu không kết nối được, (b) forward không copy thêm.
- Giới hạn kích thước (mặc định 64 MB, cấu hình được). Quá → 413.
- **Chỉ lấy 2 trường `model` và `stream`.** Dùng serde_json deserialize vào struct 2 field (serde vẫn phải lướt qua toàn bộ body nhưng không cấp phát gì cho phần bỏ qua — ~0.3–0.5 ms cho 400 KB). Nếu đo thấy đáng kể ở 200K token, đổi sang scanner chỉ đọc key top-level rồi dừng. **Đo trước, tối ưu sau.**
- Kiểm tra `model` có trong danh sách key được phép không → không thì 403.

### ③ Chặn trước theo budget và rate limit
- Counter trong RAM: per key, per team, per (team, model), period hiện tại (ngày/tháng).
- Ước lượng input token = `body_len / chars_per_token` (mặc định 3.5, config per model). Chỉ dùng để từ chối sớm khi rõ ràng đã hết budget. Không ghi số này vào sổ.
- Rate limit request/phút per key: token bucket trong RAM.
- Vượt → 429, kèm header `x-ratelimit-remaining-tokens` / `retry-after`.
- Concurrency limit per key (số request đang chạy) → chặn 1 agent lỗi vòng lặp không ăn hết GPU.

### ④ Chọn backend
- `model` → danh sách backend từ config (ví dụ `qwen3.8-27b` → [gx10, b300-1, b300-2]).
- Lọc backend khoẻ (circuit chưa mở, chưa chạm `max_inflight`).
- Chọn theo **ít request đang chạy nhất, chia cho trọng số** (`inflight / weight`). Hoà → random. Đây là "least-load" — đơn giản, không cần đọc metrics vLLM, tự nhiên đẩy request sang box rảnh.
- Không có backend nào khoẻ → 503 kèm lý do.
- *Sau này nếu cần:* đọc `/metrics` vLLM (KV cache usage, running/waiting) mỗi 2 giây để chọn chính xác hơn. Không làm ở v1.

### ⑤ Forward
- Cùng path, cùng method, body là chính `Bytes` đã đọc — không re-encode.
- Đổi `Authorization` sang key của backend. Xoá header client không nên lộ (`x-api-key` của client). Thêm `x-request-id`.
- **Một trường hợp phải sửa body:** stream mà client không gửi `stream_options.include_usage: true` thì vLLM không trả `usage` → router không đếm được. Xử lý bằng **ghép bytes**: tìm dấu `}` cuối cùng, chèn `,"stream_options":{"include_usage":true}` trước nó. Không parse. Chỉ làm khi `stream=true` và body chưa có chuỗi `"stream_options"`. (llama-server và vLLM đều hỗ trợ trường này.) Với Anthropic không cần — `usage` luôn có trong `message_start` và `message_delta`.
- HTTP client: connection pool keep-alive per backend, HTTP/1.1 (uvicorn của vLLM). Không bật HTTP/2 tới backend cho đến khi đo có lợi.
- Timeout tách 3 loại: kết nối 2 s; đến byte đầu tiên 180 s (prefill 200K trên B300 có thể mất vài chục giây — config per model); im lặng giữa 2 chunk 60 s; tổng 30 phút.

### ⑥ Stream về client
- Mỗi chunk từ backend → viết ngay cho client, flush ngay. Không gom.
- Trước khi parse chunk nào, dùng `memchr` tìm chuỗi `"usage"`. Không có → bỏ qua, chỉ forward. Có → parse riêng chunk đó (vài trăm byte) lấy `prompt_tokens`, `completion_tokens`. Kết thúc khi gặp `data: [DONE]` hoặc backend đóng kết nối.
- Anthropic: `message_start` → `input_tokens`; `message_delta` cuối → `output_tokens`.
- Non-stream: response nhỏ, parse `usage` một lần từ body đầy đủ rồi forward nguyên bytes.
- Client ngắt giữa chừng → huỷ request tới backend (drop kết nối), vẫn ghi sổ những gì đã nhận được, đánh dấu `client_aborted`.

### ⑦ Ghi sổ
- Nếu backend không trả `usage` (lỗi, hoặc backend lạ): tính tạm `body_len/3.5` cho input, số ký tự content nhận được /3.5 cho output, ghi flag `estimated=true`. Không bao giờ ghi 0 rồi im.
- Cộng counter RAM (atomic add) → budget có hiệu lực ngay cho request kế tiếp.
- Đẩy 1 record vào `mpsc` channel → task nền gom 100 record hoặc 1 giây → `INSERT` batch vào Postgres. Channel đầy (Postgres chết) → ghi ra file local append-only, replay sau. **Không bao giờ vì log mà chậm response.**
- Record: `ts, request_id, key_id, team_id, model, backend_id, status, input_tokens, output_tokens, estimated, ttfb_ms, total_ms, router_overhead_ms, stream, client_aborted, error_class`.

### Retry & failover — quy tắc cứng
- Chỉ retry khi: không kết nối được, hoặc backend trả 5xx/429 **trước khi router đã gửi byte nào cho client**.
- Đã gửi byte đầu tiên → không retry, trả lỗi/đóng stream, ghi sổ.
- Tối đa 2 backend khác nhau. Không retry lại cùng backend.
- Fallback sang Claude **chỉ khi route của model đó có khai báo** — không tự động, vì khác format và khác tiền.

### Health
- Passive: 3 lỗi kết nối/5xx liên tiếp → mở circuit 30 s → cho 1 request thử → OK thì đóng.
- Active: `GET /health` (vLLM) hoặc `GET /v1/models` mỗi 5 s.

---

## 4. Dữ liệu

### Bảng cấu hình (Postgres, đọc vào RAM)
| Bảng | Trường chính |
|---|---|
| `backends` | id, name, base_url, api_key_ref (tên biến env / file, **không** lưu plaintext trong DB), weight, max_inflight, format (`openai`/`anthropic`), enabled |
| `model_routes` | model_name (client gọi), backend_ids[] theo thứ tự ưu tiên, fallback_backend_id (tuỳ chọn), chars_per_token, first_byte_timeout_s |
| `teams` | id, name, budgets (JSON: `{period, max_tokens, per_model: {...}}`), enabled |
| `api_keys` | id, key_hash, key_prefix (8 ký tự đầu để nhận diện), team_id, owner (FRT_E_ID), allowed_models[], budget riêng (tuỳ chọn), rpm_limit, concurrency_limit, expires_at, enabled |

### Sổ cái
- `usage_ledger`: append-only, các trường ở mục ⑦. Index theo `(team_id, ts)`, `(key_id, ts)`.
- Aggregate theo ngày/tháng tính từ ledger (materialized view refresh mỗi phút). Counter RAM lúc khởi động = SUM ledger của period hiện tại.

### Budget cứng hay mềm — quyết định cần chốt
Với 2 instance router, mỗi instance giữ counter riêng và đồng bộ qua ledger mỗi vài giây → team có thể vượt budget một lượng nhỏ (≈ vài giây spend × 2 instance). Với budget nội bộ theo team, tôi đề xuất chấp nhận cái này ở v1 và ghi rõ trong docs.
Nếu anh cần **cứng tuyệt đối** (bán cho khách ngoài, prepaid): thay counter RAM bằng Redis, dùng 1 Lua script `check-and-reserve` atomic. Thêm ~0.2 ms/request và thêm 1 thành phần phải chăm. Đường nâng cấp đã chừa sẵn: counter nằm sau 1 trait `BudgetStore`, đổi implementation là xong.

---

## 5. Admin API & portal

- `POST /admin/teams`, `/admin/keys` (trả plaintext key **đúng 1 lần**), `PATCH` budget/limit, `DELETE`/disable, `GET /admin/usage?team=&from=&to=` (từ ledger).
- Auth admin: 1 master key trong env + IP allowlist. Không OIDC ở v1.
- Portal: 1 trang HTML tĩnh do binary phục vụ, gọi admin API bằng fetch. Đủ để tạo key, set budget, xem bảng usage 30 ngày. Không React, không build step.
- Dashboard vận hành: Grafana trên Prometheus.

---

## 6. Observability

- `/metrics` Prometheus: `router_requests_total{key,team,model,backend,status}`, `router_tokens_total{...,direction}`, `router_ttfb_seconds` (histogram), `router_overhead_seconds` (histogram — **thời gian router tự tốn, không tính chờ backend**), `router_backend_inflight{backend}`, `router_budget_remaining{team,model}`, `router_circuit_open{backend}`.
- Response headers: `x-router-request-id`, `x-router-backend`, `x-router-overhead-ms`. Cái cuối là công cụ verify: client đo được router tốn bao nhiêu trên từng request thật.
- Log JSON có cấu trúc (crate `tracing`), một dòng/request, không payload.

---

## 7. Bảo mật

- Key client: chỉ lưu hash; prefix để tra cứu/thu hồi.
- Key backend: từ env hoặc file mount, không vào DB.
- TLS: đề xuất terminate tại nginx/haproxy phía trước (đã có sẵn trong hạ tầng), router nghe HTTP nội bộ. Nếu muốn 1 binary tự lo, bật `rustls` — hỗ trợ sẵn nhưng tắt mặc định.
- Header từ client được lọc theo allowlist trước khi forward.
- Giới hạn body size, số kết nối/IP, timeout mọi chiều — chống một agent lỗi kéo sập router.

---

## 8. Stack kỹ thuật (tối thiểu, đều là crate chuẩn)

| Việc | Crate | Ghi chú |
|---|---|---|
| Runtime | `tokio` | multi-thread |
| HTTP server | `axum` 0.8 (trên `hyper` 1) | mỏng, đủ; không dùng framework nặng |
| HTTP client | `reqwest` 0.12 (`rustls`, `stream`) | pool keep-alive, streaming body |
| JSON | `serde`, `serde_json` | chỉ parse struct nhỏ |
| Tìm chuỗi | `memchr` | lọc chunk trước khi parse |
| Bytes | `bytes` | body zero-copy |
| Config snapshot | `arc-swap` | đổi config không lock |
| Counter | `dashmap` + `AtomicU64` | budget/inflight trong RAM |
| DB | `sqlx` (postgres, sqlite cho dev) | async, compile-time check query |
| Metrics | `metrics` + `metrics-exporter-prometheus` | |
| Log | `tracing`, `tracing-subscriber` (json) | |
| Hash | `sha2` | key hash |

Không dùng: framework actor, ORM nặng, gRPC, Kafka, WASM plugin. Thêm khi có lý do đo được.

Cấu trúc code (~3–4K dòng):
```
src/
  main.rs          — khởi động, load config, spawn tasks nền
  config/          — struct + loader từ Postgres, snapshot ArcSwap
  auth.rs          — tra key, quyền model
  budget/          — trait BudgetStore + impl RAM (+ Redis sau)
  route/           — chọn backend, health, circuit
  proxy/           — forward, stream, tap usage (openai.rs, anthropic.rs)
  ledger/          — channel + batch writer + fallback file
  admin/           — REST + static portal
  metrics.rs
```

---

## 9. Kế hoạch kiểm chứng (đây là phần quyết định, không phải code)

### Bộ test hiệu năng — chạy trước khi tin bất kỳ con số nào
Payload: 1K / 50K / 200K token. Stream on/off. Concurrency 1 / 50 / 200. Backend: vLLM thật trên GX10 (và A6000 llama-server).
Đo **hiệu số** router vs gọi thẳng vLLM:
- TTFB delta (mục tiêu **< 3 ms** ở 200K)
- Tổng thời gian delta (mục tiêu < 1%)
- `router_overhead_ms` p99 (mục tiêu **< 2 ms** ở 200K, không phụ thuộc kích thước)
- RAM router ≈ (body size × request đang chạy) + hằng số. Không rò rỉ sau 1 giờ tải.

### Bộ test đúng đắn
- Tổng token trong ledger == tổng `usage` vLLM báo (so trên 1000 request, sai số 0).
- Budget: set 10K token → request thứ N vượt bị 429, không vượt quá ước lượng 1 request.
- Kill backend giữa stream → client nhận lỗi, không retry, circuit mở, ledger ghi `client_aborted=false, error_class=upstream_closed`.
- Postgres chết 5 phút → response không chậm, ledger ghi file, replay đủ khi Postgres lên.
- Client ngắt giữa stream → kết nối tới vLLM bị huỷ trong < 1 s (xem vLLM log).
- Config đổi budget → có hiệu lực trong ≤ 5 s, không có request nào lỗi trong lúc đổi.

Unit test được phép mock. Acceptance **không** mock.

---

## 10. Lộ trình

| Tuần | Giao | Điều kiện xong |
|---|---|---|
| 1 | Proxy + key + streaming passthrough + tap usage + Prometheus + header overhead | Chạy được với Hermes trỏ vào; bộ test hiệu năng đạt mục tiêu ở 200K |
| 2 | Budget/rate limit/concurrency, ledger + batch writer + fallback file, config hot reload, health/circuit/failover | Bộ test đúng đắn pass |
| 3 | Anthropic `/v1/messages`, admin API + portal 1 trang, 2 instance sau VIP, hardening, load test ký nhận | Chạy song song với đường cũ 1 tuần, so ledger |

---

## 11. Bốn quyết định cần anh chốt trước khi code

1. **Budget mềm (RAM, có thể vượt vài giây spend) hay cứng (Redis)?** Đề xuất: mềm cho v1.
2. **Router tự chèn `stream_options.include_usage` hay bắt client gửi?** Đề xuất: router tự chèn (ghép bytes), vì client ngoài Hermes sẽ quên.
3. **Fallback sang Claude tự động theo route hay client gọi model tên khác?** Đề xuất: khai báo trong route, mặc định tắt.
4. **TLS tại nginx phía trước hay trong binary?** Đề xuất: nginx.
