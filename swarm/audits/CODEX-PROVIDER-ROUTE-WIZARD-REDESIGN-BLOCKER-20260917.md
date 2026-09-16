# CODEX blocker — Redesign Provider + Route wizard professionally — 2026-09-17

User feedback from live Portal:

```text
List of Provider pre-defined hiện ra, chọn, điền API là điền ở đây chứ ai điền bên provider?
Hay là tôi sai logic? Chọn provider là dropdown list hoặc chọn Custom Provider,
kiểu OpenAI hay Anthropic cũng chưa có, nói chung là plan kém quá.
Friendly, professional. Kiểu checkbox như nhà quê.
```

Codex verdict: user logic is correct. Current Portal flow is product-wrong, not just cosmetically rough.

## Root cause

The current UI separates one real user job into scattered developer forms:

- Provider tab manages provider rows and key references.
- Provider table has `Load models` shortcut.
- Model route modal asks for manual provider model name and backend checkbox(es).
- API key setup is treated as shell/env work or provider-row edit.

This is not how a team admin expects an LLM router dashboard to work. The product flow should be:

```text
Create route -> choose provider -> enter provider API key if needed -> refresh provider models -> choose one model -> price/context -> save
```

## Required route wizard UX

Replace the current checkbox backend picker with a professional single-provider route wizard.

### Step 1 — Provider

Inside `Models & Routes -> Create model route`, first field must be:

```text
Provider
[ dropdown ]
```

Dropdown content:

- predefined providers seeded from DB:
  - OpenAI
  - Anthropic
  - Gemini
  - DeepSeek
  - Kimi
  - Qwen
  - Z.AI
  - OpenRouter
  - Meta Muse
  - Custom OpenAI-compatible
- plus explicit option:
  - Custom Provider

Each option should show status:

```text
OpenAI          Missing API key
Anthropic      Configured
OpenRouter     Missing API key
Custom         Add new provider
```

No checkbox list for primary provider selection. This route creates one primary provider-model mapping. Fallback can stay as an optional advanced dropdown after the primary provider is selected.

### Step 2 — Provider connection details

If selected provider is predefined:

Show editable but prefilled fields:

```text
Provider name
Base URL
Provider type: OpenAI-compatible | Anthropic
API key
```

If selected provider is Custom Provider:

Show blank fields:

```text
Provider name
Base URL
Provider type: OpenAI-compatible | Anthropic
API key
```

Provider type must be a dropdown, not free text. Current backend format values can stay:

```text
openai
anthropic
```

UI label should be human:

```text
OpenAI-compatible
Anthropic Messages API
```

### Step 3 — Save/test provider key from the wizard

User expectation: API key is entered here, not through shell and not through a separate provider page.

Minimal production-safe implementation without over-engineering:

- Add admin endpoint that accepts provider key plaintext only on write.
- Store provider key as a file under router data volume, for example:

```text
/var/lib/brighto-router/provider-keys/<backend-id-or-slug>.key
```

- Update `backends.api_key_ref` to:

```text
file:/var/lib/brighto-router/provider-keys/<backend-id-or-slug>.key
```

This fits current architecture because `resolve_backend_key` already supports `file:/path`.

Security rule:

- Portal may send provider key to backend when Admin saves it.
- Backend must never return provider key plaintext.
- List/detail endpoint returns only:

```text
key_resolved: true/false
api_key_ref: file:... or env:...
```

No Redis. No external secret manager. No encrypted vault for community version unless user explicitly asks. File in Docker volume is enough and simple for self-hosted production.

### Step 4 — Refresh models from provider

In the same route wizard, after provider/key is configured, show button:

```text
Refresh models from provider
```

Button behavior:

```text
GET /admin/backends/{id}/models
```

If key/base URL is missing, show direct error:

```text
Provider API key is missing. Paste the key above, save, then refresh models.
```

If provider returns an error, show provider status code and short message.

### Step 5 — Choose exactly one provider model

After refresh, show searchable list/select:

```text
Search models...
○ gpt-4.1-mini
○ gpt-4o
○ o3
```

Requirements:

- exactly one model can be selected;
- save is disabled until one model is selected;
- selecting a provider model sets `provider_model_name`;
- `Public route name` auto-fills from selected provider model;
- Admin may edit public route name as alias.

### Step 6 — Route production fields

Keep in the same form:

```text
Context window tokens
Max output tokens
Price per 1M input tokens (USD)
Price per 1M output tokens (USD)
Fallback provider/model (Advanced, optional)
First-byte timeout seconds (Advanced)
Enabled
```

Avoid raw JSON and avoid checkbox grids in the main flow.

## Provider page role after redesign

Provider page still exists, but it is a management table, not the required first setup path.

Provider page should support:

- view predefined providers;
- configured/missing key status;
- edit base URL/type;
- update API key via write-only field;
- test/refresh models;
- add custom provider.

But a first-time Admin must be able to complete route setup entirely from `Create model route`.

## Route table must show useful columns

Current route table is too thin. Show:

```text
Public model
Provider
Provider model
Context
Max output
Input $/1M
Output $/1M
Fallback
Enabled
Actions
```

This lets Admin verify immediately that the selected model and pricing were saved.

## Backend/API changes needed

Use minimal endpoints; do not create a large framework.

Suggested endpoints:

```text
POST /admin/backends                 # create custom provider or predefined provider row
PATCH /admin/backends/{id}           # edit name/base_url/format/enabled/key_ref
PUT /admin/backends/{id}/key         # write provider API key; returns key_resolved only
GET /admin/backends/{id}/models      # refresh model list live
POST /admin/routes                   # save route with provider_model_name/context/prices
```

`PUT /admin/backends/{id}/key` request:

```json
{ "api_key": "sk-..." }
```

response:

```json
{ "id": 1, "key_resolved": true, "api_key_ref": "file:/var/lib/brighto-router/provider-keys/openai.key" }
```

No endpoint should return provider key plaintext.

## Playwright acceptance for this blocker

Codex will fail until this browser flow works:

1. Admin login.
2. Models & Routes.
3. Create model route.
4. Provider dropdown visible with predefined providers.
5. Select `OpenAI` or a mock/custom provider.
6. Provider type dropdown visible: OpenAI-compatible / Anthropic.
7. API key field visible in this route wizard.
8. Save/update provider key from this wizard.
9. Click `Refresh models from provider`.
10. Model list appears.
11. Select exactly one model.
12. Public route name auto-fills.
13. Fill context/max output/prices.
14. Save route.
15. Route table shows provider + provider model + prices.

Do not call Portal done until this passes in real Playwright.
