# CODEX AUDIT — Provider model hot-path + local no-auth blocker

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

FAIL for the SOTA provider/model route workflow until two backend-contract gaps are fixed:

1. The portal can save `provider_model_name`, but the router hot path does not load or use it.
2. Custom local providers such as llama.cpp with no API key require a fake key today.

Both issues are product blockers for the user-requested OpenRouter-style setup flow.

## Evidence

### 1. `provider_model_name` is admin-only today

Admin route CRUD writes these columns:

- `provider_model_name`
- `context_tokens`
- `max_output_tokens`
- `price_input_per_mtok_usd`
- `price_output_per_mtok_usd`
- `enabled`

But runtime config loader still loads only the old route fields:

```sql
SELECT model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout
FROM model_routes
```

`ModelRoute` in `src/contract.rs` also has no `provider_model_name`, context, pricing, or enabled fields. `proxy_forward` receives the original request body and forwards it without rewriting the JSON `model` field.

### Practical impact

If admin creates:

- public model shown to clients: `fast-qwen`
- provider model selected from provider list: `qwen3.8-flash-next`

then client calls router with `model: "fast-qwen"`. Current router routes by `fast-qwen`, but forwards the body unchanged. The provider receives `model: "fast-qwen"`, which is wrong unless provider also has that exact model name.

This means the new model picker UI can look correct while runtime calls the wrong provider model.

## Required one-pass fix for provider model mapping

DeepSeek should extend the runtime contract, not only the admin response.

1. Add fields to `ModelRoute`:
   - `provider_model_name: String`
   - `context_tokens: Option<i64>`
   - `max_output_tokens: Option<i64>`
   - `price_input_per_mtok_usd: Option<f64>`
   - `price_output_per_mtok_usd: Option<f64>`
   - `enabled: bool`

2. Update config loader query to load these fields.

3. When matching a requested public model, reject disabled routes before forwarding.

4. Before forwarding to backend, rewrite top-level JSON `model` from public model to `provider_model_name` if different.
   - Keep this safe: only parse/rewrite for buffered request bodies.
   - For the large-prompt streaming-upload path, either require public model == provider model or implement a bounded prefix rewrite before stream passthrough.
   - Do not parse full 1M-token body on the hot path just to rename a model. If streaming rewrite is not implemented yet, enforce and document the limitation.

5. Ledger/metrics should continue recording the public model name, because that is what teams see and budget against.

## Acceptance tests for provider model mapping

1. Mock provider exposes only `provider-real-model`.
2. Create route public `team-friendly-model` -> provider `provider-real-model`.
3. Call router with JSON body `model: "team-friendly-model"`.
4. Mock provider must receive `model: "provider-real-model"`.
5. Router `/admin/usage` and dashboard must record `team-friendly-model`.
6. Disabled route returns a clear non-200 error and does not forward to provider.
7. Large body behavior is explicit and tested:
   - either safe rewrite works with large payloads, or
   - router rejects public/provider mismatch for streaming-upload mode with a clear error.

## 2. Local no-auth providers currently need a fake key

The user provided a local llama.cpp endpoint:

- base URL: `http://0.0.0.0:8088/v1`
- key: empty
- model: `qwen3.8-flash-next`

`0.0.0.0` is a bind address; router clients should normally call `127.0.0.1` or the host LAN IP. Direct llama.cpp model listing works at `http://127.0.0.1:8088/v1/models`.

Current backend creation requires non-empty `api_key_ref`, model fetch rejects missing/empty key, and proxy fails if `backend.api_key` is `None`. To test llama.cpp today, Codex had to configure a dummy file key. llama.cpp ignored the dummy Bearer header, so the route worked, but this is not a professional product flow.

## Required one-pass fix for local/custom auth

Add explicit provider authentication mode. Keep it minimal:

- `auth_mode = "bearer"` for OpenAI-compatible cloud providers.
- `auth_mode = "anthropic"` for Anthropic-style `x-api-key` providers.
- `auth_mode = "none"` for local llama.cpp/vLLM/Ollama-compatible servers that do not require a key.

Then:

1. `api_key_ref` may be empty only when `auth_mode = "none"`.
2. `/admin/backends/{id}/models` sends no auth header when `auth_mode = "none"`.
3. `proxy_forward` sends no provider auth header when `auth_mode = "none"`.
4. Portal route wizard must expose this in plain language:
   - “Cloud provider with API key”
   - “Local server, no API key”
5. Provider list should show `No auth` as healthy for local providers, not `Enabled · no key`.

## Acceptance tests for no-auth custom provider

1. Start llama.cpp/vLLM-compatible mock with no auth requirement.
2. Create provider with `auth_mode: "none"`, empty key.
3. Click/load `/models` from portal or call admin API.
4. Create route by selecting exactly one provider model.
5. Call router with a BrighTO client key.
6. Provider receives no `Authorization` and no `x-api-key` header.
7. Request succeeds and usage is recorded.

