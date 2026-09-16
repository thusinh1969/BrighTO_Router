# DeepSeek — Playwright acceptance PASS + usage filters — 2026-09-17

Verdict: Ran the CODEX Playwright browser acceptance against the current SOTA portal and it
PASSES (result PASS, 0 failures). Usage screen gained labeled provider/key/date filters.

## Playwright acceptance (headless Chromium, real browser)
Run: swarm/out/playwright/20260917-035020/run_playwright_audit.sh (temp Postgres + seed + router).
Result: PASS, failures = [].

Covered:
1. Admin login (wrong password -> clear error, correct -> dashboard).
2. All 6 nav views render: Providers, Models & Routes, Teams, API Keys, Usage, Settings.
3. Provider create (secret-free: name/base_url/format).
4. Route wizard: protocol dropdown present, auth mode present, provider model manual entry,
   context/max-output/price-per-1M fields present, save works.
5. Team create.
6. API key create -> Key created modal shows lc- key -> Done -> key re-visible in admin table
   (user override: admin may re-view keys).
7. Usage filters: model/status/provider/team/key/from/to/range.
8. User mode login with client key -> API key verified -> mobile 390px viewport renders.

The only console error is the intentional wrong-password 401 (filtered).

## This round's change
- Usage screen filter bar: labeled filters (Model/Status/Provider/Team/Key/From/To/Range) wired
  to the existing /admin/usage backend filters (backend/key/from/to were already supported).

## Overall portal state (rounds 5-17)
All CODEX blockers closed and live on 18080: runtime hang, duplicate routes, protocol taxonomy +
endpoint guard, call-log tok/s + cost + friendly duration, dashboard IA + spend, perf diagnostics
(overhead + prompt buckets), provider health, DELETE CRUD, F5 session restore, HTTPS/TLS, route
wizard protocol filtering + manual model entry, provider protocols column.
