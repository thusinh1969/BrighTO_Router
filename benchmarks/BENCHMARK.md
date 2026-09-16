# BENCHMARK.md — Bộ cổng (gate) BrighTO-Router bắt buộc pass trước mọi release

Mọi thứ trong file này chạy bằng một lệnh, kết quả là JSON, và **exit code khác 0 = không được merge/release**. Không có "gần đạt". Số ngưỡng ghi trong `benchmarks/thresholds.toml`; muốn đổi ngưỡng phải sửa file đó qua PR có lý do.

Phạm vi v1 (team / user / key / budget / logging). Tính năng thêm sau này thêm gate của nó, không được làm gate cũ đỏ.

---

## 0. Bốn nguyên tắc đo

1. **Đo hiệu số, không đo số tuyệt đối.** Overhead = router − direct, cùng payload, cùng upstream, cùng lúc. Số tuyệt đối phụ thuộc máy; hiệu số thì không.
2. **Cô lập trước, thực tế sau.** Tầng A–B dùng **mock upstream** (`brighto-router-mock`, Rust, trả lời trong ~0 ms) để không có gì che overhead. Tầng C–D dùng vLLM/llama-server thật và cloud thật để chứng minh delta không biến mất ngoài đời.
3. **Load shape phải explicit trong artifact.** `conc=1` đo latency floor bằng closed-loop một kết nối. `conc=50/200` dùng target offered-rate (`oha -q`) để tránh coordinated omission nhưng không flood máy đo. B6 dùng target-rate trên `conc=200`, không dùng saturation run không giới hạn để claim pass/fail.
4. **Một cấu hình duy nhất cho mọi gate**: auth bật, budget bật, ledger ghi thật vào Postgres, log JSON bật, Prometheus bật. Không được tắt tính năng để đo cho đẹp.

---

## 1. Ma trận payload

Sinh bằng `benchmarks/make_payloads.py` (đã có). Kích thước tính theo ~4 ký tự/token; số token thật lấy từ `usage` upstream trả về.

| Tên | Token vào | Body | Stream | Dùng cho |
|---|---|---|---|---|
| `1k` | ~1.000 | ~4 KB | on/off | throughput, baseline |
| `50k` | ~50.000 | ~200 KB | on/off | RAG/agent điển hình |
| `200k` | ~200.000 | ~800 KB | on/off | deep research, trường hợp xấu nhất |

Harness canonical đo latency ở concurrency 1 / 50 / 200 để có đủ raw evidence. Release gates B1/B2/B3 trong bảng dưới áp ngưỡng ở `conc=50`; `conc=1` và `conc=200` vẫn phải nằm trong `summary.json`/raw JSON để audit regression và phát hiện saturation artifact. Thời lượng mỗi điểm đo: warm-up 15 s bỏ, đo 60 s, lặp 3 lần, lấy **median**. Kết quả xấu nhất trong 3 lần không được vượt ngưỡng quá 25% (chống flaky).

---

## 2. Tầng A — Đúng đắn (mock upstream, deterministic, chạy trong `cargo test`)

Mỗi dòng là một integration test có tên; pass/fail nhị phân.

### Xác thực & phân quyền
| Test | Kỳ vọng |
|---|---|
| `auth_missing_key` | 401, body chưa được đọc (mock không nhận request) |
| `auth_unknown_key` / `auth_disabled_key` / `auth_expired_key` | 401 |
| `auth_model_not_allowed` | 403, upstream không nhận request |
| `auth_both_header_styles` | `Authorization: Bearer` và `x-api-key` đều được |
| `auth_client_key_not_forwarded` | upstream nhận key của backend, không thấy key client trong bất kỳ header nào |

### Budget / rate limit / concurrency
| Test | Kỳ vọng |
|---|---|
| `budget_exact_block` | budget 10.000 token, bắn request 3.000 token liên tiếp → request thứ 4 bị 429, ledger sau đó ghi đúng 3 request |
| `budget_team_over_key` | key có budget riêng lớn hơn team → team thắng |
| `budget_per_model` | `claude-backup` ≤ 2M trong khi tổng team 40M → chặn đúng model, model khác vẫn chạy |
| `budget_429_headers` | có `retry-after`, `x-ratelimit-remaining-tokens` |
| `rpm_limit` | rpm=10, bắn 15 trong 1 s → đúng 5 cái 429 (±0) |
| `concurrency_limit` | limit=2, giữ 2 stream mở → request thứ 3 bị 429 ngay, không xếp hàng |
| `budget_shared_two_instances` | 2 router cùng Redis, budget 10.000, bắn xen kẽ → tổng token ghi ≤ 10.000 + 1 request |

### Đếm token & ledger
| Test | Kỳ vọng |
|---|---|
| `usage_stream_openai` | mock trả `usage` ở chunk cuối → ledger = đúng số đó, `estimated=false` |
| `usage_nonstream_openai` | như trên, non-stream |
| `usage_stream_anthropic` | `input_tokens` từ `message_start`, `output_tokens` từ `message_delta` |
| `usage_missing_fallback` | mock không trả usage → ledger có số ước lượng, `estimated=true`, không bao giờ 0 |
| `ledger_sum_equals_upstream` | 1.000 request ngẫu nhiên → Σ ledger == Σ mock đã trả, sai số **0** |
| `ledger_never_blocks` | Postgres tắt giữa chừng 60 s → p99 latency không đổi, record vào file fallback, bật lại → replay đủ, không trùng (`request_id` unique) |
| `ledger_no_payload` | grep toàn bộ DB + log + fallback file: không có một chuỗi nào từ `messages` |

### Passthrough & stream
| Test | Kỳ vọng |
|---|---|
| `body_bytes_identical` | upstream nhận đúng bytes client gửi (sha256 bằng nhau), trừ trường hợp inject bên dưới |
| `stream_options_injected_only_when_missing` | stream=true không có `stream_options` → được chèn; đã có → nguyên vẹn; stream=false → không chèn |
| `response_bytes_identical` | client nhận đúng bytes upstream trả (sha256 chunk-by-chunk), header content-type giữ nguyên |
| `chunk_forwarded_immediately` | mock nhả 1 chunk / 200 ms → client thấy mỗi chunk ≤ 205 ms sau khi mock nhả (không gom) |
| `request_id_present` | `x-router-request-id` ở response, cùng id trong ledger và log |
| `overhead_header_present` | `x-router-overhead-ms` có và < ngưỡng tầng B |

### Retry / failover / circuit
| Test | Kỳ vọng |
|---|---|
| `retry_on_connect_refused` | backend 1 chết → sang backend 2, client không thấy lỗi |
| `retry_on_5xx_before_first_byte` | như trên với 503 |
| `no_retry_after_first_byte` | mock chết sau 3 chunk → client nhận lỗi/stream đóng, **không có request thứ 2** tới backend nào |
| `no_retry_same_backend` | chỉ 1 backend, chết → lỗi ngay, không retry |
| `circuit_opens_after_3` | 3 lỗi liên tiếp → 30 s không nhận request, sau đó 1 request thử |
| `least_load_routing` | 2 backend weight bằng nhau, giữ 10 stream mở trên A → 10 request tiếp theo đi B ≥ 9 lần |
| `fallback_only_when_declared` | route không khai fallback → không bao giờ chạm Claude |

### Timeout & abort
| Test | Kỳ vọng |
|---|---|
| `timeout_connect_2s` / `timeout_first_byte` / `timeout_idle_between_chunks` / `timeout_total` | mỗi cái đúng ngưỡng ±10%, ledger ghi `error_class` đúng |
| `client_abort_cancels_upstream` | client ngắt giữa stream → mock thấy kết nối đóng trong **< 1 s**, ledger `client_aborted=true` với token đã nhận |
| `body_too_large_413` | > MAX_BODY_BYTES → 413 trước khi đọc hết |
| `malformed_json_400` | body không parse được `model` → 400, không tới upstream |

### Vận hành
| Test | Kỳ vọng |
|---|---|
| `config_reload_5s` | đổi budget trong Postgres → có hiệu lực ≤ 5 s, 0 request lỗi trong lúc đổi |
| `graceful_shutdown_drains` | SIGTERM khi 20 stream đang chạy → listener đóng ngay, 20 stream kết thúc bình thường, process thoát sau stream cuối |
| `healthz_reflects_db` | Postgres chết → `/healthz` vẫn 200 (router vẫn phục vụ được), `/readyz` 503 |
| `metrics_cardinality` | `/metrics` không có label chứa request_id hay nội dung |

---

## 3. Tầng B — Hiệu năng (mock upstream, gate số)

Máy tham chiếu: gateway pin 4 core (`taskset`), load generator 4 core khác, mock 2 core khác, cùng máy, loopback. Governor `performance`. Ghi rõ CPU model và benchmark knobs trong kết quả. Ngưỡng dưới là **tuyệt đối**; ngoài ra mỗi số không được xấu hơn baseline của release trước quá **10%** (regression gate, baseline = `bench/baseline.json`).

| Gate | Payload | Conc. | Ngưỡng |
|---|---|---|---|
| `B1 overhead_p50` | 1k / 50k / 200k, non-stream | 50 | ≤ 0,3 / 0,6 / **1,0 ms** |
| `B2 overhead_p99` | 1k / 50k / 200k, non-stream | 50 | ≤ 0,8 / 1,5 / **2,0 ms** |
| `B3 overhead_flat` | 200k vs 1k | 50 | p50(200k) − p50(1k) ≤ 0,8 ms — overhead **không được tỷ lệ với payload** |
| `B4 ttfb_delta` | 1k / 50k / 200k, stream | 50 | ≤ 1 / 2 / **3 ms** |
| `B5 chunk_gap_added` | 200k, stream, mock 64 chunk | 50 | p99 gap thêm ≤ 0,5 ms/chunk |
| `B6 throughput` | 1k non-stream | target-rate, conc 200 | đạt target ≥ **8.000 rps trên 4 core**, non-200 = 0 |
| `B7 stream_fanout` | 50k stream | **1.000** stream mở đồng thời | không lỗi, RSS ≤ 64 MB + 1,5 × Σ body đang chạy |
| `B8 memory_stable` | 50k stream, 10 phút, conc 200 | — | RSS phút 10 ≤ RSS phút 2 + 5% |
| `B9 cpu_per_request` | 50k non-stream | 50 | ≤ 150 µs CPU / request (pidstat) |
| `B10 ledger_lag` | 1k, 2.000 rps, 60 s | — | 99% record vào Postgres ≤ 2 s sau khi request xong |
| `B11 p99_under_pg_outage` | 1k, conc 50, Postgres tắt 60 s giữa chừng | — | p99 không tăng > 10% so với B2 |

Overhead đo từ 2 nguồn phải khớp: (a) hiệu số router − direct từ load generator, (b) `x-router-overhead-ms` do router tự báo. Lệch > 30% = gate fail (router tự báo sai).

---

## 4. Tầng C — LLM local thật (vLLM / llama-server)

Backend cố định 1 model, `max_tokens=64`, `temperature=0`, `seed=1`. Chạy trên GX10 (vLLM) **và** A6000 (llama-server) — hai stack khác nhau, cả hai phải pass.

| Gate | Ngưỡng |
|---|---|
| `C1 ttfb_delta_real` | 1k / 50k / 200k stream, conc 8: TTFB(router) − TTFB(direct) ≤ 3 ms, median 3 lần |
| `C2 total_delta_real` | total(router) − total(direct) ≤ 1% |
| `C3 tokens_equal_real` | 200 request: Σ ledger == Σ `usage` vLLM/llama báo, sai số 0 |
| `C4 prefill_200k_no_timeout` | 200k trên B300/GX10: không dính first-byte timeout ở ngưỡng cấu hình của model |
| `C5 long_stream` | `max_tokens=4096` stream 5 phút: không đứt, idle-timeout không nổ giữa chừng |
| `C6 abort_real` | ngắt client giữa stream → vLLM log thấy request cancelled ≤ 2 s (GPU không chạy tiếp) |
| `C7 parallel_slots` | llama-server `-np 4`, bắn 8 stream: router giữ 4 ở backend, 4 chờ hoặc 429 theo `max_inflight`, không lỗi lạ |
| `C8 models_endpoint` | `/v1/models` qua router trả đúng alias, client discover được |

---

## 5. Tầng D — LLM cloud thật (Anthropic `/v1/messages` passthrough; OpenAI-compatible cloud nếu có)

Model rẻ nhất (haiku), 50 request/gate, chi phí < $1/lần chạy. Ở đây **không đo overhead** (mạng WAN nhiễu hơn 100× overhead); đo đúng đắn và bền.

| Gate | Ngưỡng |
|---|---|
| `D1 anthropic_stream_intact` | bytes SSE client nhận == bytes provider trả (event order, `message_stop` cuối) |
| `D2 anthropic_usage` | ledger `input_tokens`/`output_tokens` == `usage` trong `message_start` + `message_delta` |
| `D3 anthropic_headers` | `anthropic-version`, `anthropic-beta` client gửi được forward nguyên; key client không lộ |
| `D4 provider_error_passthrough` | 400/401/429/529 từ provider trả đúng status + body cho client, `error_class` đúng, **không retry sau byte đầu**, 429/529 trước byte đầu → sang fallback nếu khai |
| `D5 tls_and_proxy` | qua HTTPS, có/không HTTP proxy env, CA hệ thống |
| `D6 budget_cloud` | budget per-model `claude-backup` chặn đúng như tầng A |
| `D7 openai_cloud_stream` (nếu có key) | như D1/D2 với `stream_options.include_usage` tự chèn |

---

## 6. Tầng E — Chaos (mock, tự động)

| Gate | Ngưỡng |
|---|---|
| `E1 backend_hang_no_bytes` | mock nhận rồi im → first-byte timeout đúng ngưỡng, sang backend 2 nếu có |
| `E2 backend_drip` | 1 chunk / 70 s → idle timeout 60 s nổ, ledger ghi token đã nhận |
| `E3 backend_flap` | backend chết/sống mỗi 5 s trong 2 phút → tỷ lệ lỗi client ≤ tỷ lệ request rơi đúng vào lúc chết, circuit hoạt động |
| `E4 redis_down` | *(chỉ cho feature Redis optional tương lai — KHÔNG trong default fastest profile; default dùng counter RAM)* Redis tắt → BUDGET_STORE=redis: router chuyển "fail-open có cảnh báo" hay "fail-closed" **theo config**, và làm đúng cái đã config; metric `router_budget_store_degraded=1` |
| `E5 slowloris` | 500 kết nối gửi header nhỏ giọt → router vẫn phục vụ client bình thường (header timeout) |
| `E6 config_poll_fails` | Postgres chết → snapshot cũ tiếp tục dùng, log warn mỗi 30 s, không spam |

---

## 7. Cách chạy & kết quả

```bash
make gate            # check + cargo test + B trên mock, exit≠0 nếu fail
make gate-smoke      # check + smoke B ngắn, KHÔNG phải release proof
make bench-gate      # chỉ tầng B canonical khi A/test đã chạy riêng
# Tầng C/D hiện chạy thủ công bằng backend thật; chỉ thêm gate-local/gate-cloud khi đã tự động hoá đủ env + artifact.
```

Kết quả canonical: `bench/results/<timestamp>/gate.json` + `summary.json` + raw direct/router `oha` JSON + `mock.log` + `router.log` + `router-metrics.txt`. `benchmarks/gate.sh` chỉ là wrapper gọi `scripts/bench_real.py`; không duy trì benchmark logic thứ hai. Schema `gate.json`:

```json
{ "sha": "...", "host": {"cpu": "...", "cores_router": 4, "kernel": "..."},
  "gates": [ {"id": "B2", "payload": "200k", "value_ms": 1.42, "threshold_ms": 2.0, "pass": true, "runs": [1.40, 1.42, 1.51]} ],
  "pass": true }
```

CI/perf runner chạy `make gate` trước release hoặc PR có thay đổi hot path. Tầng C/D chạy trước tag release bằng backend thật và phải đính kèm artifact tương đương trước khi thêm target `gate-local`/`gate-cloud`. Release note đính kèm `gate.json`. Baseline mới (`bench/baseline.json`) chỉ được cập nhật bởi một PR riêng, sau khi mọi gate xanh.
