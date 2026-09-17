# CODEX AUDIT — WRAPPER FALSE GREEN: CUSTOM LLM AUTH STILL WRONG

Date: 2026-09-17  
Role: Codex auditor. Product code remains DeepSeek-owned.

## Verdict

**Do not accept the latest wrapper PASS as final.**

`bash swarm/scripts/portal_logic_acceptance.sh` now exits 0, but the gate is incomplete: its own evidence shows the Custom LLM blank-key route was saved with the wrong auth/protocol.

## Evidence

Command:

```bash
bash swarm/scripts/portal_logic_acceptance.sh
```

Result:

- Exit code: 0
- PASS count: 17
- Failure count: 0

But in the same printed summary, the saved Custom LLM route is:

```json
{
  "auth_mode": "bearer",
  "protocol": "openai_chat"
}
```

That route was created with:

- Provider: `Custom LLM`
- Base URL: `http://127.0.0.1:9000/v1`
- Wizard API key: blank
- `.env CUSTOM_LLM_API_KEY`: set

Expected result:

```json
{
  "auth_mode": "none",
  "protocol": "local_openai_chat"
}
```

## Required fix

Fix both product and test gate:

1. Product rule:
   - For `custom-llm` + local/private URL + blank wizard API key, save no-auth local route.
   - Do not silently inherit `CUSTOM_LLM_API_KEY` for local/private Custom LLM.
   - Only use Bearer when Admin explicitly pastes a key or explicitly opts into env key.

2. Gate rule:
   - `portal_logic_acceptance.mjs` must assert this exact route property:

```js
route.auth_mode === 'none' && route.protocol === 'local_openai_chat'
```

3. Selector rule:
   - Keep scoped model-picker selectors. Do not use global `text=mock-model` if stale table rows can exist.

4. Harness rule:
   - Wrapper PASS is only valid if it fails on wrong auth/protocol.

## Acceptance command

After product + gate fix:

```bash
cargo check --workspace
cargo test --workspace
bash swarm/scripts/portal_logic_acceptance.sh
```

Expected:

- All pass.
- Gate evidence route for Custom LLM blank-key shows `auth_mode=none`, `protocol=local_openai_chat`.

## Additional live-route evidence

Existing local route after current setup:

```json
{
  "model_name": "qwen3.8-flash-next-local",
  "provider_model_name": "qwen3.8-flash-next",
  "auth_mode": "bearer",
  "protocol": "openai_chat",
  "enabled": true,
  "effective_enabled": true,
  "backend_ids": [10]
}
```

Associated backend:

```json
{
  "name": "Custom LLM",
  "base_url": "http://127.0.0.1:8088/v1",
  "api_key_ref": "env:NONE",
  "key_resolved": false
}
```

This is internally inconsistent: the route says Bearer auth, while the backend has no key. The llama.cpp smoke still returned HTTP 200, but that is not proof the route is correct; it only proves this local server tolerates the request. The saved configuration should be corrected to no-auth local semantics.

Required data migration/repair after code fix:

```sql
UPDATE model_routes
SET auth_mode = 'none', protocol = 'local_openai_chat'
WHERE model_name = 'qwen3.8-flash-next-local'
  AND provider_model_name = 'qwen3.8-flash-next';
```

Or repair through the Admin UI after the UI logic is fixed. Do not leave V1.0 with an internally inconsistent route.
