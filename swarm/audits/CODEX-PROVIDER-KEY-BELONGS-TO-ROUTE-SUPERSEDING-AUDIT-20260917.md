# CODEX AUDIT — Provider key belongs to Model Route, not Provider

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

FAIL until the Provider/Route credential model is fixed.

The user is correct: provider API key must not live in the Provider definition. The current design still treats Provider/Backend as the place that owns `api_key_ref`, and DeepSeek added `PUT /admin/backends/{id}/key`. That is the wrong product model for this repo.

Supersede earlier guidance that suggested saving provider keys on provider/backends. The corrected rule is:

> Provider = reusable provider template/endpoint metadata. No secrets.  
> Model Route = concrete usable model mapping. It owns the provider credential used for that route.

## Why the current model is wrong

Current UI/API flow:

1. Admin opens Provider.
2. Provider has `api_key_ref` / provider key.
3. Route selects provider and model.

This breaks the user-requested workflow:

1. Admin creates or chooses a Provider template like OpenAI, Anthropic, DeepSeek, Custom OpenAI-compatible, local llama.cpp.
2. Admin creates a Model Route.
3. In that Model Route wizard, admin selects provider, enters provider API key or chooses no-auth local server, loads provider models, selects exactly one provider model, sets public model name/pricing/context/budget metadata, saves.

The second flow is the correct one because the same provider can be used by multiple routes with different keys, billing owners, model choices, local/no-auth behavior, timeout, prices, and route visibility.

## Correct product language

Use these terms consistently:

- Provider: OpenAI, Anthropic, Gemini, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, Meta Muse, Custom, Local llama.cpp/vLLM.
- Provider credential: the upstream cloud/local credential used to call that provider for one route. Empty only for no-auth local providers.
- BrighTO client API key: the key issued by BrighTO-Router to a team/user. This is separate from provider credentials.
- Model Route: public model offered by BrighTO-Router, mapped to one provider model and one route credential.

Do not label provider credential and BrighTO client API key both as “API key” without context.

## Required architecture change

Keep it simple. Do not add Redis or extra services.

### Preferred schema

Introduce provider templates without secrets:

```sql
providers (
  id BIGSERIAL PRIMARY KEY,
  slug TEXT UNIQUE NOT NULL,
  name TEXT NOT NULL,
  base_url TEXT NOT NULL,
  protocol TEXT NOT NULL,      -- openai | anthropic
  auth_mode_default TEXT NOT NULL, -- bearer | anthropic | none
  enabled BOOLEAN NOT NULL DEFAULT TRUE
)
```

Move credential ownership to model routes:

```sql
model_routes (
  id BIGSERIAL PRIMARY KEY,
  public_model_name TEXT UNIQUE NOT NULL,
  provider_id BIGINT NOT NULL REFERENCES providers(id),
  provider_model_name TEXT NOT NULL,
  provider_key_ref TEXT,       -- file:/... or env:...; NULL/empty only if auth_mode = none
  auth_mode TEXT NOT NULL,     -- bearer | anthropic | none
  context_tokens BIGINT,
  max_output_tokens BIGINT,
  price_input_per_mtok_usd DOUBLE PRECISION,
  price_output_per_mtok_usd DOUBLE PRECISION,
  first_byte_timeout BIGINT NOT NULL DEFAULT 180,
  enabled BOOLEAN NOT NULL DEFAULT TRUE
)
```

If DeepSeek wants to minimize migrations, it may keep the internal table name `backends`, but the semantics must change:

- Provider table/section has no `api_key_ref`.
- Route row stores the credential ref and auth mode.
- Runtime snapshot resolves route credential, not provider credential.

Do not leave `PUT /admin/backends/{id}/key` as the main product path. If retained temporarily, mark it legacy/internal and remove it from UI.

## Correct UI flow

### Providers screen

This screen manages provider templates only:

Fields:

- Provider name
- Provider type/protocol: OpenAI-compatible, Anthropic Messages
- Base URL
- Enabled

No provider key field. No `api_key_ref` field. No “set key” button here.

For predefined cloud providers, most users should not edit URL/type at all. They choose from a dropdown in the route wizard.

### Create/Edit Model Route wizard

Fields in this order:

1. Provider dropdown
   - OpenAI
   - Anthropic
   - Gemini
   - DeepSeek
   - Kimi
   - Qwen
   - Z.AI
   - OpenRouter
   - Meta Muse
   - Custom Provider
   - Local llama.cpp/vLLM

2. Provider URL/type
   - Autofilled for predefined providers.
   - Editable only for Custom/Local.

3. Provider credential
   - Cloud: paste provider API key here.
   - Local no-auth: choose “No API key required”.
   - On save, backend writes secret to server-side file/secret store and stores only `provider_key_ref` on the route.
   - Never return provider credential plaintext from API.

4. Load models button
   - Uses the selected provider + credential from this wizard.
   - Must work before final save.
   - For no-auth local providers, request must send no auth header.

5. Provider model dropdown
   - User must select exactly one provider model returned by `/models`.

6. Public model name
   - Default to provider model name.
   - Admin may rename it to a friendly public name.
   - Runtime must rewrite request body top-level `model` to provider model when public name differs.

7. Context/max output/prices
   - Context tokens: auto-fill when provider returns metadata; otherwise manual.
   - Max output tokens: auto-fill when known; otherwise manual.
   - Price per 1M input tokens.
   - Price per 1M output tokens.

8. Enabled

## Required API contract

Add route-level endpoints or equivalent:

- `POST /admin/routes/preview-models`
  - body includes provider/template id, base_url if custom/local, protocol, auth_mode, optional provider key plaintext.
  - returns model list.
  - does not persist the key unless explicitly saving.

- `POST /admin/routes`
  - creates route and route-level credential ref.

- `PATCH /admin/routes/{id}` or `PATCH /admin/routes/{old_public_model_name}`
  - updates existing route without creating a duplicate row.
  - can rotate provider credential when a new plaintext provider key is supplied.
  - if provider key field is blank during edit, keep existing route credential.

- `DELETE /admin/routes/{id}` or disable route endpoint.
  - UI label must match behavior: Delete if removed, Disable if disabled.

Do not use `POST /admin/routes` upsert by public model name as the edit path. It caused the live duplicate-row bug.

## Required runtime behavior

Runtime `ModelRoute` must contain:

- public model name
- provider model name
- provider base URL/protocol from provider template
- route-level auth mode
- route-level resolved provider key, optional only for no-auth
- context/max output/pricing/enabled

On request:

1. Authenticate BrighTO client key.
2. Authorize public model name.
3. Find enabled model route.
4. Forward to provider URL.
5. Use route-level provider credential.
6. If public model differs from provider model, rewrite only the top-level JSON `model` sent upstream.
7. Ledger/metrics record the public model name.

Large body rule:

- For buffered request bodies, rewriting top-level model is straightforward.
- For streaming-upload fast path, either implement safe prefix rewrite or reject public/provider model mismatch with a clear error until implemented.
- Do not parse/re-encode a 1M-token body just to rename a model.

## Acceptance tests

DeepSeek must run these before declaring done:

### A. Provider screen has no secret

1. Open Providers.
2. Create/edit provider template.
3. Assert no provider key or `api_key_ref` field is visible.

### B. Route wizard owns credential

1. Open Create Model Route.
2. Select DeepSeek provider.
3. Paste provider key in route wizard.
4. Click Load models.
5. Select exactly one provider model.
6. Save route.
7. `GET /admin/routes` returns route metadata but not plaintext provider key.

### C. Local llama.cpp no-auth

1. Select Local llama.cpp/vLLM.
2. Enter `http://127.0.0.1:8088/v1`.
3. Select “No API key required”.
4. Load models.
5. Select `qwen3.8-flash-next`.
6. Save route.
7. Call router successfully.
8. Assert upstream receives no `Authorization` and no `x-api-key`.

### D. Public-name rewrite

1. Provider model is `qwen3.8-flash-next`.
2. Public model name is `qwen-local`.
3. Client calls router with `model: "qwen-local"`.
4. Upstream receives `model: "qwen3.8-flash-next"`.
5. Usage dashboard records `qwen-local`.

### E. Edit does not duplicate

1. Create route.
2. Edit price/context only.
3. Row count unchanged.
4. Rename public model.
5. Old public name gone, new public name exists, row count unchanged.
6. Rename to an existing public name returns conflict.

### F. Delete/disable works visibly

1. Route delete/disable button changes backend state.
2. Table updates after action.
3. Refresh browser, state remains correct.
4. If action is disable, button text says Disable, not Delete.

## Current dirty-code warning

At the time of this audit, local working tree has DeepSeek product changes in `src/config/mod.rs` and `src/contract.rs`, while previous no-auth changes touched `src/admin/mod.rs` and `src/proxy/mod.rs`.

Do not ship partial no-auth support as final. Partial state where some endpoints allow missing keys but provider still owns `api_key_ref` is still product-wrong.
