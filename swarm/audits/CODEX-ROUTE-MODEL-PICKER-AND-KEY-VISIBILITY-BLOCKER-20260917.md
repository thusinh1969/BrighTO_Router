# CODEX blocker — Create Model Route flow is wrong + key visibility incomplete — 2026-09-17

User feedback from live Portal refresh:

```text
Create model route SAI, phải cho refresh lấy list of model về và chọn 1, thiếu field này.
Và chưa cho hiển thị key gì cả !
```

This is accepted as a real product blocker.

## Root cause 1: Create Model Route is still a manual developer form

Current route modal has:

```text
Public model name
Provider model name (optional)
Provider backend(s)
Context window
Max output tokens
Price per 1M input/output
Fallback
Timeout
```

This is not the requested flow. It still assumes Admin already knows and types provider model names manually.

Current `Load models` exists only on the Provider table. That flow opens provider models and creates a route immediately from a clicked model, without the full route pricing/context form. That splits one user journey into two incomplete flows.

## Required root-cause fix

Make `Create model route` a single clear wizard/form:

1. Select exactly one Provider first.
   - Use a dropdown with configured providers.
   - Show provider status: configured/missing API key.

2. Add a button inside the route modal:

```text
Refresh models from provider
```

3. Button calls:

```text
GET /admin/backends/{id}/models
```

4. Show returned model names in a searchable dropdown/list.

5. Admin must select exactly one provider model.
   - No route can save until one provider model is selected.
   - Selected provider model becomes `provider_model_name`.
   - Public model name defaults to the selected provider model, but Admin may edit it as an alias.

6. Keep existing production fields in the same save flow:
   - context tokens;
   - max output tokens;
   - price per 1M input tokens, USD;
   - price per 1M output tokens, USD;
   - fallback optional;
   - first-byte timeout.

7. Save one route object only after all required fields are clear.

## Do not keep two competing route creation flows

Avoid this split:

- Provider table `Load models` creates a route directly;
- Models tab `Create model route` is manual.

Better minimal behavior:

- Provider table `Load models` may open the same Create Route modal preselected to that provider and loaded models.
- Models tab `Create model route` starts empty, then Admin selects provider and refreshes models.

This is simpler and matches user expectation.

## Root cause 2: Key visibility is not obvious enough

User says the Portal has not shown key clearly.

Required Admin behavior:

1. After `Create API key`, show the full BrighTO client API key in a large visible box.
2. Provide `Copy` button next to it.
3. Text must be plain:

```text
Admin can view and copy this client API key again later from the API Keys table.
```

4. After pressing `Done`, API Keys table must refresh immediately.
5. API Keys table must have a clear `View key` or `Show key` action on every row.
6. Clicking `View key` must call:

```text
GET /admin/keys/{id}/reveal
```

7. The reveal modal must show the full key visibly, not only prefix/toast.
8. For old keys with no stored plaintext, show the 410 message clearly:

```text
This key was created before key reveal was enabled. Disable and recreate it.
```

Scope: this applies only to BrighTO client API keys. Provider LLM API keys still must not be displayed in plaintext.

## Playwright acceptance to add

Codex will fail Playwright until these clicks pass:

1. Admin -> Models & Routes -> Create model route.
2. Select provider.
3. Click Refresh models from provider.
4. See model list.
5. Select exactly one model.
6. Public model auto-fills from selected model.
7. Fill context/max output/prices.
8. Save.
9. Route table shows public model + provider model + price fields.
10. Admin -> API Keys -> New key -> Create.
11. Full key appears immediately.
12. Done refreshes table.
13. View key shows full key again.

No final Portal acceptance until the above is true in the browser.
