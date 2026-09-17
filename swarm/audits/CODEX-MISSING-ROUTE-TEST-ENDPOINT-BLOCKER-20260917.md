# CODEX BLOCKER — Model Route wizard still has no Test Endpoint

Date: 2026-09-17 14:43 ICT  
Live target checked: `https://127.0.0.1:18443`  
HEAD checked: `f1cdfc7`

## Verdict

The Model Route wizard still does **not** implement the required `Test endpoint` / `Test connection` step.

This is a direct miss against the plan and the user's requirement. `Load models` is not endpoint testing. A provider can list models and still fail chat/messages calls. A route can save and still immediately return 400/401/404/429/503.

Live/source check:

```text
Test endpoint    false
Test connection  false
Save enabled     false
last_test        false
preview-models   true
Create route     true
```

Source evidence:

```text
static/index.html only has:
- Create model route
- Load models via /admin/routes/preview-models
- Save route
```

There is no route smoke/test endpoint API or UI action.

## Required behavior

In `Create model route` / `Edit route`, after provider/source + key + provider model are chosen, show:

```text
Test endpoint
```

When clicked, it must perform a real tiny route-level call using the exact values in the wizard:

- selected provider/base URL
- selected API format/protocol
- selected auth mode/key
- selected provider model
- max output <= 8 tokens
- non-streaming

It must not require creating/enabling a production route first.

Expected UI result:

```text
PASS  HTTP 200, latency N ms, response parseable
FAIL  HTTP status + sanitized provider/router error
```

Never display the provider API key in result/log/audit.

## Save rule

- `Save draft disabled` may be allowed without endpoint test.
- `Save enabled` must be disabled until the current wizard values have a passing endpoint test.
- If user changes Provider / Base URL / API format / Auth / API key / Provider model after a pass, clear the pass state and require another test.
- Edit route: if route remains enabled and any connection-critical field changes, require test again.

## Minimal implementation options

Option A — server-side preview smoke endpoint, preferred:

```text
POST /admin/routes/test-endpoint
```

Payload:

```json
{
  "base_url": "https://api.deepseek.com",
  "protocol": "openai_chat",
  "auth_mode": "bearer",
  "provider_key": "...",
  "provider_model_name": "deepseek-v4-pro"
}
```

Response:

```json
{
  "ok": true,
  "status": 200,
  "latency_ms": 1234,
  "message": "Endpoint works"
}
```

For Anthropic, call provider messages format. For OpenAI-compatible, call chat completions. For local no-auth, call local chat completions.

Option B — temporary disabled route smoke, acceptable v1 if cleaned:

1. Create a temporary disabled route name like `_probe_<uuid>`.
2. Call router with that model.
3. Delete temp route immediately.
4. Return sanitized result to wizard.

This is less clean but reuses the real router path. It must never leave temp routes behind.

## Acceptance test

Playwright must verify this exact flow:

1. Admin opens Models.
2. Clicks `Create model route`.
3. Chooses DeepSeek.
4. Enters API key.
5. Loads models.
6. Selects exactly one provider model.
7. `Save enabled` is disabled before test.
8. Clicks `Test endpoint`.
9. Test returns PASS.
10. `Save enabled` becomes enabled.
11. Saves route.
12. Client call through `/v1/chat/completions` returns 200.

Second test:

1. Same wizard with blank key for cloud provider.
2. `Test endpoint` fails locally with `Enter API key first`.
3. It must not call upstream and leak 401/502.

Third test:

1. Custom local endpoint `http://127.0.0.1:8088/v1`.
2. Auth `No API key`.
3. Provider model `qwen3.8-flash-next`.
4. `Test endpoint` returns PASS.
5. Save enabled route works.

Do not claim done until this is implemented and tested on live Docker.
