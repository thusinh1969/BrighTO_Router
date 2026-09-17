# CODEX AUDIT — CUSTOM LLM BLANK KEY RUNTIME FAIL

Date: 2026-09-17  
Role: Codex auditor. Product code remains DeepSeek-owned.

## Verdict

**NOT ACCEPTED for Custom LLM local blank-key behavior.**

The product one-flow gate still passes for the mock because the mock ignores Authorization, but the actual saved route semantics are wrong for local llama.cpp.

## Runtime evidence

Focused Playwright gate artifact:

- `swarm/out/playwright/custom-llm-auth-20260917-173006/summary.json`
- Result: FAIL

Precondition verified:

- `custom-llm` catalog entry exists.
- `.env` has `CUSTOM_LLM_API_KEY` set, so catalog reports `key_set=true`.

Browser actions executed:

1. Login Admin over HTTPS.
2. Models → Add model.
3. Select `Custom LLM`.
4. Set Base URL: `http://127.0.0.1:9000/v1`.
5. Leave API key field blank.
6. Load models.
7. Pick `mock-model` from chooser.
8. Test connection.
9. Save enabled.
10. Read `/admin/routes`.

Observed saved route:

```json
{
  "auth_mode": "bearer",
  "protocol": "openai_chat"
}
```

Expected saved route:

```json
{
  "auth_mode": "none",
  "protocol": "local_openai_chat"
}
```

## Root cause

Current frontend logic in `static/index.html` makes local no-auth depend on `!(e.key_env && e.key_set)`:

```js
if(local && !pk.value.trim() && !(e.key_env && e.key_set)) {
  return { dialect:"openai", auth:"none", protocol:"local_openai_chat" };
}
return { dialect:"openai", auth:"bearer", protocol:"openai_chat" };
```

That silently applies `CUSTOM_LLM_API_KEY` to a local Custom LLM route even when Admin intentionally leaves the key blank.

## Required fix

For `custom-llm` and local/private base URLs:

1. Blank wizard key means `auth_mode=none`, `protocol=local_openai_chat`.
2. Bearer auth only when Admin explicitly pastes a key or explicitly chooses to use an env key.
3. Do not silently apply `CUSTOM_LLM_API_KEY` to local/private Custom LLM routes.
4. Add this exact case to the canonical Playwright gate:
   - `.env CUSTOM_LLM_API_KEY` set
   - Custom LLM local URL
   - wizard API key blank
   - saved route must be no-auth local.

## Why this matters

The user's real local upstream is llama.cpp on `/v1` with empty key. If BrighTO sends an unexpected Bearer header, some local servers will reject it or behave differently. Passing mock 200 is not enough; route semantics must be correct.
