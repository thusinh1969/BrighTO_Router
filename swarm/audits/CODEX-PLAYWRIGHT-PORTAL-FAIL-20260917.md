# CODEX Playwright audit — Portal current slice FAIL — 2026-09-17

Artifact folder:

```text
swarm/out/playwright/20260917-035020/
```

Runner:

```text
Docker mcr.microsoft.com/playwright:v1.63.0-noble
Chromium
Temporary PostgreSQL
Current release binary
Real browser clicks, not DOM-only inspection
```

## Result

FAIL. The Portal is clickable and much improved, but it is not yet product-complete.

## Browser flows that worked

- Admin login page opens.
- Wrong admin password shows clean error.
- Correct admin password reaches Dashboard.
- Navigation clicks work for Providers, Models & Routes, Teams, API Keys, Usage, Settings.
- Provider creation through UI works.
- Model route creation through UI works with:
  - provider model name;
  - context tokens;
  - max output tokens;
  - price per 1M input tokens;
  - price per 1M output tokens.
- Team creation through the friendly default budget flow works.
- API key creation works.
- Admin can reveal/copy a created BrighTO client API key after refreshing API Keys tab.
- User login with the created client API key works.
- Mobile viewport screenshot was captured.

## Blocking failures to fix next

1. **API key budget is still raw JSON.**
   - Current API key modal exposes Budget as JSON.
   - Required user-facing choices: inherit team, unlimited, token budget, money budget, daily/monthly.
   - Raw JSON can stay under Advanced only.

2. **API Keys table does not refresh after key creation.**
   - Flow: API Keys -> New key -> Create -> Done.
   - Expected: new key row appears immediately with `View key` and `Disable`.
   - Actual: row is not visible until the API Keys tab is reopened.
   - Root cause: after create, `keys=await api('/admin/keys','GET')` runs but `renderKeys(content)` is not called after closing the success modal.

3. **Usage screen has no visible filters.**
   Required filters are still missing:
   - provider/backend;
   - model;
   - team;
   - key;
   - date range;
   - status class.

4. **Usage totals need product meaning.**
   - After filters exist, show totals for requests, input tokens, output tokens, and cost when route price is available.
   - Never show fake spend. Use `—` or `unknown` for rows without pricing.

## Notes

The previous route pricing/context gap is mostly fixed in the latest worktree. Keep it covered by smoke tests so it does not regress.

The browser console recorded a 401 from the intentional wrong-password login check. Codex ignored it as expected test behavior; there were no unexpected browser console/request failures.

Do not request final Playwright acceptance again until the four blocking items above are fixed.
