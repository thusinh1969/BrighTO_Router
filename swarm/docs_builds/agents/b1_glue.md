# B1 — glue: main.rs + handlers.rs (wire contract thanh pipeline ①→⑦)

**Owned:** `src/main.rs`, `src/handlers.rs` (MOI). Doc `src/contract.rs` (khong sua). Doc them:
`docs/llm-router-rust-plan.md` muc 3 (duong di request) + muc 6 (observability).

## Nhiem vu
- `main.rs`: dotenv, tracing JSON subscriber, khoi dong config loader (await snapshot dau TRUOC khi listen),
  spawn task nen (config poll 5s — goi A1 `run()`; ledger writer — goi B2 `run()`), axum server tren `LISTEN_ADDR`
  (env, mac dinh `0.0.0.0:8090` — may nay 8080/3000 da dung), graceful shutdown SIGTERM (danh listener, doi stream xong,
  timeout `stop_grace_period` 5 phut), healthcheck subcommand (`brigto-router healthcheck` GET /healthz -> 200).
- `handlers.rs`: `POST /v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, `GET /v1/models`, `POST /v1/messages` (Anthropic).
  Pipeline dung contract + module A1-A4:
  1. Lay key tu `Authorization: Bearer` hoac `x-api-key` -> A2 `hash_key` -> authorize TU TRUOC KHI doc body (401 som).
  2. Doc body thanh `Bytes` (gioi han `MAX_BODY_BYTES` env, mac dinh 64MB -> 413). Chi lay 2 truong `model` + `stream`
     (serde_json struct nhe, `#[serde(default)]`, bo qua phan con lai). SAI MODEL -> 403.
  3. A2 `try_reserve` (uoc luong `body_len / chars_per_token` tu route) -> 429 kem `x-ratelimit-remaining-tokens`/`retry-after`.
     Concurrency limit per key: tang/giam gauge quanh request.
  4. A3 `pick` -> 503 kem ly do neu None. Tang `inflight`, `note_result` khi xong.
  5. A4 forward (body Bytes nguyen ven, doi Authorization tu `Backend.api_key_ref` da resolve, loc header client theo
     allowlist [Authorization, Content-Type, Accept, X-Request-Id], them `x-request-id` uuid v7).
  6. Stream SSE ve client tung chunk flush ngay; A4 tap usage; ket thuc khi [DONE]/dong ket noi.
  7. Ket request: A2 `commit` (token that tu backend usage; khong co usage -> uoc luong + flag estimated),
     push `UsageEvent` vao ledger channel (try_send, day -> ghi file fallback, KHONG cho), emit metrics B4.
- Headers response moi request: `x-router-request-id`, `x-router-backend`, `x-router-overhead-ms` (thoi gian router TU,
  khong tinh cho backend — do tu luc nhan request den luc gui byte dau, tru thoi gian cho backend response).
- Test: `tests/handlers_test.rs` — dung `tower::ServiceExt::oneshot` + mock BackendPool/BudgetStore (impl trait mock
  trong test, KHONG mock proxy); 1 test end-to-end qua llama-server that 127.0.0.1:8088 (model qwen3.8-flash-next,
  nhan moi key) danh dau `#[ignore]` ( chay co dieu kien).
