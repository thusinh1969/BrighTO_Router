# CODEX audit — updated Portal smoke verdict — 2026-09-17

Current uncommitted DeepSeek worktree inspected:

```text
M  scripts/portal_smoke.py
M  src/admin/mod.rs
M  static/index.html
?? migrations/0003_admin_key_reveal.sql
```

## Checks run by Codex

```bash
node --check /tmp/brighto-current-portal.js
python3 scripts/portal_smoke.py
```

Result:

```text
PASS admin /teams lists 1 team
PASS admin /keys starts empty
PASS admin /stats starts empty
PASS POST /admin/keys returns lc- key
PASS GET /admin/keys/{id}/reveal returns plaintext
PASS GET /admin/settings returns runtime info
PASS POST /admin/backends creates provider
PASS GET /portal/me returns own key + team
PASS GET /portal/me/usage returns own row
PASS GET /portal/me/stats aggregates own usage
PASS bad key -> 401
PASS admin /keys never returns plaintext
RESULT PASS
```

## Verdict

Backend/API progress is good enough for this slice:

- create client key works;
- Admin reveal of BrighTO client API key works;
- `/admin/settings` works;
- `POST /admin/backends` works;
- `/admin/keys` list still does not leak plaintext keys by default.

## Still required before Portal can be called done

These are not optional polish items. They are user-facing product requirements.

1. Add negative reveal coverage to `scripts/portal_smoke.py`:
   - bad admin key cannot reveal;
   - client/user API key cannot reveal through admin endpoint;
   - `key_secret IS NULL` returns 410 with recreate message.

2. Replace stale copy in comments/UI:
   - `src/admin/mod.rs` top comment still says plaintext key is returned once;
   - `scripts/portal_smoke.py` comment still says plaintext returned once;
   - `static/index.html` still says "only plaintext copy unless...".

3. Finish product fields:
   - Model Route: provider model, context/max token, price per 1M input, price per 1M output;
   - API Key: expiry, budget, request-per-minute limit, concurrency limit;
   - Team: user-friendly budget controls, raw JSON only as advanced option;
   - Usage: visible filters by provider/backend, model, team, key, date, status, and totals.

4. After those are done, hand back with this exact sentence:

```text
Portal product flows ready for Codex Playwright audit.
```

Codex will then run Playwright against the real running Portal and click through all required admin/user flows.
