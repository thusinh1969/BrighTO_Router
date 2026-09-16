# A4 — proxy forward + SSE pump + tap usage (TRAI TIM hot path)

**Owned:** `src/proxy/mod.rs` (tach file con `splice.rs` neu can — van thuoc A4). Doc `src/contract.rs` (khong sua).

## Nhiem vu
- Forward: cung path/method, body la `Bytes` di thang **khong parse/re-encode**. Doi `Authorization` sang key backend
  (tu `Backend.api_key_ref` da resolve), xoa header client khong nen lo (`x-api-key`...), them `x-request-id`.
- **Byte-splice** (da verify tren llama-server that): chi khi `stream=true` VA body chua chua chuoi `"stream_options"`,
  tim `}` cuoi cung, chen `,"stream_options":{"include_usage":true}` truoc no. Khong parse JSON.
- SSE pump: moi chunk tu backend -> viet ngay cho client, flush ngay, khong gom.
  Truoc khi parse chunk nao: `memchr::memmem::find(chunk, b'\"usage\"')` -> khong co thi bo qua, chi forward.
  Co -> parse rieng chunk do (vai tram byte) lay `usage.prompt_tokens`/`completion_tokens`.
  **Parser phai tolerant:** llama-server tra chunk usage voi `"choices":[]` + field thua `timings` (fixture that trong
  `tests/fixtures/stream_with_usage.sse`). Ket thuc khi `data: [DONE]` hoac backend dong.
- Anthropic path: `message_start` -> `input_tokens`; `message_delta` cuoi -> `output_tokens` (khong can splice).
- Client ngat giua chung -> huy request backend (drop connection), van ghi UsageEvent nhung gi nhan duoc, `client_aborted=true`.
- Timeout (config per model, mac dinh): connect 2s; TTFB 180s; im lang giua 2 chunk 60s; tong 30 phut.
- Retry: CHI KHI khong ket noi duoc hoac backend tra 5xx/429 **truoc khi gui byte dau tien cho client**; toi da 2 backend
  khac nhau, khong retry lai cung backend. Da gui byte dau -> khong retry, tra loi/dong stream, ghi so.

## Test (unit + fixture, KHONG mock backend)
- `splice_inserts_before_last_brace_only_when_needed` (fixture `stream_no_usage_option.sse` + body nho).
- `tap_usage_from_fixture` — parse fixture `stream_with_usage.sse`, dung `prompt_tokens`/`completion_tokens`.
- `tap_survives_extra_fields_and_empty_choices` (chinh shape llama-server).
- Test tich hop nho ban thang llama-server `http://127.0.0.1:8088` model `qwen3.8-flash-next` (backend that, nhan moi key):
  stream + non-stream, so usage router dem vs backend tra — sai so 0. Ghi marker `#[ignore]` neu can chay co dieu kien.
- Test do overhead: parse chunk 400KB bang memchr-path < 0.5ms (in so ra, khong assert chat).

## Rang buoc
- Hot path: memchr filter truoc khi parse; khong cap phat ngoai Bytes + chunk usage; khong await DB.
