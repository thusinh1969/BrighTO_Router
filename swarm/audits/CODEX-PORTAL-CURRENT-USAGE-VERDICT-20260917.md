# Portal current usage verdict - 2026-09-17

## Verified state

Local stack is running and healthy on `http://127.0.0.1:18080/`.

Evidence from 2026-09-17 local check:

```text
/healthz -> 200 ok
/readyz -> 200 ready
/admin/backends with x-admin-key -> 10 providers
providers=openai,anthropic,gemini,deepseek,kimi,qwen,zai,openrouter,meta-muse,custom-openai
enabled=0
key_ready=0
node --check static portal script -> pass
```

The default admin key in `.env` is currently:

```text
brightoIsGreat@2026
```

## User-facing current flow

1. Open `http://127.0.0.1:18080/`.
2. Paste admin key into **Master key**.
3. Click **Load providers**.
4. Set provider key from shell, for example `./start.sh set-key openai sk-...`.
5. In Portal: edit provider, enable it, save it.
6. Fetch models, or type a model manually if the provider model-list response is incompatible.
7. Create route.
8. Create team API key and use that key against `/v1/...` routes.

## Frontend work DeepSeek should do next

P0: make this flow impossible to misuse.

- Show the above flow as a guided wizard inside the portal.
- Validate missing admin key before API calls.
- Validate backend ID, model name, backend IDs, and budget JSON before submit.
- After provider save, reload provider list and keep selected backend visible.
- Show a clear message that provider API keys are set by `./start.sh set-key`, not pasted into the current portal.
- If route listing is needed, add a small `GET /admin/routes` endpoint with tests; current portal can create/update routes but cannot display existing routes.

Boundaries:

- Do not return provider plaintext keys from admin API.
- Do not add React/Vite/Tailwind just for this current setup flow.
- Do not add Redis or any new production dependency for portal polish.
