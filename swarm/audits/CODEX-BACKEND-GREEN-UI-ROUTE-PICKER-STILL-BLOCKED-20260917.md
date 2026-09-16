# CODEX audit — backend green, route picker UI still blocked — 2026-09-17

Current uncommitted files inspected:

```text
M src/admin/mod.rs
M static/index.html
```

## Backend checks now pass

Commands run by Codex:

```bash
cargo check --locked --all-targets
python3 scripts/hotpath_guard.py
python3 scripts/portal_smoke.py
```

Result:

```text
cargo check PASS
HOTPATH_GUARD_PASS
portal_smoke.py RESULT PASS
```

`portal_smoke.py` now covers:

- `/admin/summary` grouped stats;
- client key create;
- admin reveal returns plaintext;
- settings endpoint;
- provider create;
- user portal endpoints;
- bad user key 401;
- `/admin/keys` list does not leak plaintext;
- bad admin reveal 401;
- client API key cannot reveal admin endpoint;
- legacy key reveal 410.

## Accepted backend progress

- Usage row filter compile blocker is fixed.
- Summary call-sites are now wired with `backend`, `model`, and normalized `status`.
- Key table refresh after key create is present in `static/index.html`:

```javascript
keys=await api("/admin/keys","GET"); renderKeys($("content"));
```

Codex still needs Playwright to verify the key visibility flow in a real browser after final UI changes.

## Still blocking user-reported UI issue

The user specifically reported:

```text
Create model route SAI, phải cho refresh lấy list of model về và chọn 1, thiếu field này.
```

This is still true in current `static/index.html`.

Evidence:

- `Models & Routes -> Create model route` still opens a manual form.
- The modal still has `Provider model name (optional)` as a text input.
- There is no button inside the modal named `Refresh models from provider`.
- There is no searchable loaded model list in the modal.
- There is no forced exactly-one provider model selection before save.
- Existing `Load models` is still only on the Provider table and creates a route directly, bypassing the full route form.

## Required next UI change

Replace the manual provider-model text flow with a single wizard-like form:

1. Provider dropdown: select exactly one provider.
2. Button: `Refresh models from provider`.
3. Call: `GET /admin/backends/{id}/models`.
4. Render loaded models in a searchable select/list.
5. Require exactly one selected provider model before save.
6. Auto-fill public model name from selected provider model; allow editing alias.
7. Keep context/max output/price fields in same form.
8. Route table should show public model, provider model, provider/backend, context/max output, input price, output price.

## Runtime note

Current public runtime may not show source changes until rebuilt/restarted because Portal is embedded into Rust binary with `include_str!`. For developer speed, consider a tiny dev-only static-file mode later:

```text
STATIC_DIR=static -> serve from disk in dev
unset STATIC_DIR -> embed HTML in production
```

Do not add a frontend build system for this.
