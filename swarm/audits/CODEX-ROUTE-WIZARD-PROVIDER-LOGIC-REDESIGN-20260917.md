# CODEX AUDIT — Provider / Model Route Logic Redesign

Date: 2026-09-17 14:40 ICT  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

The current Portal logic is still confusing because it exposes internal router concepts as if they were user workflow steps.

The user is correct: if Admin wants to create one custom model route, forcing them to first create a `Custom OpenAI` Provider with URL, then leave that screen and create a Model Route using that Provider is inefficient and feels stupid. It makes `Provider Type`, `Provider`, `Protocol`, and `Provider model` look like four separate things when the user only wants one outcome:

> expose model X through BrighTO-Router using endpoint Y and key Z.

Fix this once by making **Model Route creation the primary flow**. Provider/Connection rows should be created or reused behind the scenes.

## Product language to use

Use these terms in the Portal:

- **Provider template**: OpenAI, Anthropic, DeepSeek, Kimi, Qwen, Z.AI, Gemini, OpenRouter, Meta Muse. A preset that gives default base URL and API format.
- **Connection**: a real endpoint the router can call. Internally this is `backends`.
- **Model route**: the public model name clients call. Internally this is `model_routes`.
- **Provider model**: the real model name at the upstream provider.
- **Public model name**: the model name exposed to client teams.
- **API format**: how the router talks to the upstream: OpenAI Chat Completions, Anthropic Messages, OpenAI Embeddings, Local/OpenAI-compatible Chat, etc.

Avoid showing `Provider Type` as a primary concept. It is a preset/helper, not something the user should have to reason about.

## Correct one-flow UX

Primary CTA should be on Models page:

```text
Create model route
```

The route wizard should do everything in one modal/page.

### Step 1 — Choose source

Show cards or a clean dropdown:

```text
OpenAI
Anthropic
DeepSeek
Kimi / Moonshot
Qwen
Z.AI / GLM
Gemini
OpenRouter
Meta Muse
Custom OpenAI-compatible endpoint
Existing connection
```

Rules:

- For formal providers, prefill base URL and API format.
- For custom endpoint, show `Base URL` inline in this wizard.
- For existing connection, show a dropdown of existing backend connections.
- Do not require user to go to Providers page first.

### Step 2 — Credentials and API format

Fields:

```text
API key                 [password/text toggle]  optional when auth = none
Authentication          Bearer / Anthropic x-api-key / No API key
API format              OpenAI Chat / Anthropic Messages / Embeddings / Local OpenAI-compatible Chat / Custom OpenAI-compatible Chat
```

Rules:

- Filter API format by source. Do not show every protocol for every provider.
- Anthropic source defaults to `Anthropic Messages` + `Anthropic x-api-key`.
- OpenAI/DeepSeek/Kimi/Qwen/Z.AI/OpenRouter defaults to `OpenAI Chat` + `Bearer`.
- Custom local endpoint can choose `No API key`.
- Cloud provider with auth != none must require key before `Load models` or `Test endpoint`.

### Step 3 — Load/select provider model

Buttons/fields:

```text
Load models
Provider model          dropdown after load, manual input if provider does not support list
Public model name       auto-filled from provider model, editable
```

Rules:

- The user selects exactly one provider model.
- If provider model list returns unsupported/empty, show manual input, not an opaque error.
- For Qwen, do not use token-plan endpoint.
- Gemini can stay disabled/unimplemented until supported.

### Step 4 — Route limits and pricing

Fields:

```text
Context window tokens       optional
Max output tokens           optional
Price per 1M input tokens   optional
Price per 1M output tokens  optional
First-byte timeout          default 180 seconds
Enabled                     default off until endpoint test passes, or Save draft
```

### Step 5 — Test endpoint

Required before saving an enabled route:

```text
Test endpoint
```

Show:

```text
PASS  status 200, latency X ms, provider model returned parseable response
FAIL  exact reason, sanitized; no key shown
```

Rules:

- Save as disabled draft can be allowed without test.
- Save enabled must require a passing endpoint test in the current wizard session, or a persisted recent `last_test_ok` if implemented.
- Do not let Admin create enabled routes that immediately return `503 no healthy backend available`.

## What happens behind the scenes

Do not expose this as workflow to the user.

When the user saves a route:

1. If they selected an existing connection, use its backend id.
2. If they selected a formal provider template, reuse the existing formal backend row for that provider.
3. If they selected custom endpoint and no matching connection exists, create one automatically.
4. If they selected custom endpoint and a matching connection exists, reuse it.
5. Save the route with route-level provider key and selected provider model.
6. Reload config.

Matching rule for custom connection:

```text
same normalized base_url + same backend format
```

Do not create duplicate custom providers for the same URL every time the user creates a model.

If UI implements this with two existing API calls for v1:

- `POST /admin/backends` only when custom backend does not exist.
- `POST /admin/routes` after backend id exists.
- If route creation fails and backend was just created and has no usage/routes, delete that backend immediately.

Better but still simple server-side option:

- Extend `POST /admin/routes` to accept either `backend_ids` or an inline provider source:

```json
{
  "provider_source": {
    "kind": "custom_openai",
    "name": "local llama.cpp",
    "base_url": "http://127.0.0.1:8088/v1",
    "format": "openai"
  },
  "provider_key": "...",
  "auth_mode": "none",
  "protocol": "local_openai_chat",
  "provider_model_name": "qwen3.8-flash-next",
  "model_name": "qwen3.8-flash-next"
}
```

This gives one atomic backend operation, but it is optional for v1 if the UI cleanup is correct.

## Providers page role

The Providers page should be renamed or mentally treated as **Connections**.

It is not the main place to create a model.

Purpose:

- View endpoint connections the router can call.
- Disable/enable a connection.
- See which routes use it.
- See whether it has usage history.
- Delete only when lifecycle rules allow it.

Primary actions on Providers/Connections page:

```text
Edit connection
Enable / Disable
Create route from this connection
Delete   only if safe
```

Do not make `Add provider` the required first step for normal model creation.

For formal providers, avoid status like `Enabled · no key` if keys are route-level. That reads as broken. Better statuses:

```text
Template / No routes
Active / 3 routes
Disabled / 2 routes affected
Has usage / cannot delete
```

## Fix `Custom OpenAI` confusion

`Custom OpenAI-compatible endpoint` should be an option inside Create Model Route, not a confusing Provider Type the user must pre-create.

Wizard behavior for custom:

```text
Source: Custom OpenAI-compatible endpoint
Base URL: http://127.0.0.1:8088/v1
Authentication: No API key
API format: Local OpenAI-compatible Chat
Load models: qwen3.8-flash-next
Public model name: qwen3.8-flash-next
Test endpoint: PASS
Save route
```

After save, Providers/Connections page may show:

```text
custom-local-llama   http://127.0.0.1:8088/v1   Active / 1 route
```

The user created one model route in one flow. The backend connection was just an implementation detail.

## Minimal data model guidance

Do not add a heavy provider registry.

Current tables can work:

- `backends`: endpoint connection.
- `model_routes`: public route + provider model + protocol + route-level key.
- `usage_ledger`: transaction history.

Useful optional additions if needed, still simple:

- `backends.kind` or infer kind from name/base_url/format. If avoiding migration, infer for now.
- `model_routes.last_test_ok`, `last_test_at_ms`, `last_test_error` if you want persisted enable gating. If skipping this for v1, require a pass in the wizard session before Save enabled.

## Required acceptance tests

### Fresh install route creation without visiting Providers

1. Login Admin.
2. Go directly to Models.
3. Click `Create model route`.
4. Select `DeepSeek`.
5. Enter API key.
6. Load models.
7. Select exactly one model.
8. Test endpoint.
9. Save enabled route.
10. Call router `/v1/chat/completions` and get 200.

No step may require visiting Providers first.

### Custom local route in one flow

1. Go directly to Models.
2. Click `Create model route`.
3. Select `Custom OpenAI-compatible endpoint`.
4. Enter `http://127.0.0.1:8088/v1`.
5. Select `No API key`.
6. Select `Local OpenAI-compatible Chat`.
7. Load or type `qwen3.8-flash-next`.
8. Test endpoint returns 200.
9. Save route.
10. Providers/Connections now has one connection for that URL, not duplicates.

### Duplicate prevention

1. Create custom local route A using `http://127.0.0.1:8088/v1`.
2. Create custom local route B using the same URL.
3. Confirm only one backend/connection exists for that base URL.
4. Confirm both routes reference the same backend id.

### Existing connection route

1. From Providers/Connections row `custom-local-llama`, click `Create route from this connection`.
2. Wizard opens with source/base URL/API format prefilled.
3. User only fills provider model/public route/test/save.

### Lifecycle still applies

All rules from `CODEX-PROVIDER-MODEL-LIFECYCLE-CONTRACT-20260917.md` still apply:

- cannot delete provider with active route;
- cannot delete provider with usage history;
- disabling provider makes related routes effectively disabled;
- cannot delete model with usage history;
- Provider and Model pages need Enabled only / Disabled only / All filters.

## Current Portal-specific changes needed

1. Rename or reduce `Provider Type` in Provider modal. It should not be the main mental model.
2. Move custom endpoint creation into Route wizard.
3. Route wizard source picker must include formal templates and Custom endpoint even when no provider row exists.
4. Route wizard must create/reuse backend behind the scenes.
5. Providers page must add `Create route from this connection`.
6. Provider status must stop saying `Enabled · no key` for route-level-key architecture.
7. Add endpoint test before Save enabled.
8. Add lifecycle filters/actions requested by user.

Do not claim SOTA until a new user can create a cloud route or local custom route from the Models page alone without understanding backend internals.
