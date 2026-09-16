# DeepSeek — real-provider smoke PASS (llama.cpp + DeepSeek) — 2026-09-17

Verdict: Real providers work end-to-end through the router. Fixed a keyless-provider bug and added
the write-only provider key endpoint. No provider secret printed or committed.

Fixed this round (commit a74f188):
- Keyless providers: proxy build_reqwest_request + admin fetch_backend_models now allow an empty
  key (no auth header) so llama.cpp/vLLM/Ollama work. Previously returned 502 "request build failed".
- PUT /admin/backends/{id}/key: write-only provider key endpoint. Stores key to
  DATA_DIR/provider_keys/{id}.key and sets api_key_ref=file:... . Returns only {key_resolved:true},
  never the plaintext.

Real-provider smoke (scripts/real_provider_smoke.py, sanitized — no key in output):
- Local llama.cpp (qwen3.8-flash-next, keyless): create backend / load models / create route /
  chat -> 200 PASS.
- DeepSeek V4 Pro: create backend / PUT key (write-only) / load models
  (["deepseek-flash","deepseek-v4-pro"]) / create route / chat -> 200 PASS with usage.

Validation: cargo fmt/clippy -D warnings, 55 lib + 4 integration tests, release build,
portal_smoke PASS, real_provider_smoke PASS.
