# CODEX AUDIT — Portal mobile card layout + full-page gate pass

Date: 2026-09-17
Runtime tested: live Docker HTTPS at `https://127.0.0.1:18443`
Image tested: `thusinh1969/brighto_airouter:v1` @ `sha256:d9976378a3213405297fb1b7304348f0970fca1a161b1d6099538f987f5efb6a`

## Verdict

PASS for the specific bug class that made the Portal look broken on small screens:

- table rows no longer collapse into unreadable tiny columns on mobile;
- mobile rows now render as stacked cards with visible field labels;
- long public model names, provider names, provider model names, and API keys no longer cause whole-page horizontal overflow;
- dashboard and usage logs show meaningful token throughput (`Tok/s`) for real mock calls;
- logic CRUD gate still passes after the visual change.

This does not mean the Portal is final Hermes/OpenRouter-level product design. It means the current implementation is no longer obviously broken and now has an automated regression gate for the worst layout failures.

## Change made

`static/index.html`:

- Added mobile table-card CSS for `max-width: 820px`.
- Added `decorateTables()` to copy table headers into `td[data-label]` so mobile rows show labels such as `MODEL`, `STATUS`, `TOK/S`, `COST`.
- Added a `MutationObserver` so asynchronously loaded/revealed rows, including API keys, also get labels.

`swarm/scripts/portal_full_page_audit.*`:

- Added a full Portal screenshot gate across desktop and mobile.
- Seeds isolated `pw-fullvisual-*` data only.
- Sends one real mock chat request through the router so dashboard/usage/log panels have real data.
- Verifies mobile tables render as stacked cards (`table: block`, `tbody: grid`).
- Verifies no mobile table has horizontal scroll.
- Verifies all rows have readable column labels.
- Cleans up only `pw-fullvisual-*` records after the run.

## Commands run

```bash
cargo test --workspace
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
bash swarm/scripts/portal_polish_audit.sh
bash swarm/scripts/portal_full_page_audit.sh
```

## Results

- Rust tests: 64 passed.
- Logic acceptance: PASS.
- Visual responsive audit: PASS.
- Portal polish audit: PASS.
- Full-page audit: PASS.
- Browser console errors: none.
- Test cleanup: PASS; no `pw-logic-*`, `pw-visual-*`, `pw-polish-*`, or `pw-fullvisual-*` routes remained.

## Latest screenshot evidence

Generated under:

`swarm/out/playwright/20260917-180347-portal-full-page-audit/`

Important files:

- `desktop-1440-dashboard.png`
- `desktop-1440-providers.png`
- `desktop-1440-models.png`
- `desktop-1440-keys.png`
- `desktop-1440-usage.png`
- `mobile-390-dashboard.png`
- `mobile-390-providers.png`
- `mobile-390-models.png`
- `mobile-390-keys.png`
- `mobile-390-usage.png`

## Specific requirements now enforced

Do not mark Portal visual work as done unless all of these stay true:

1. `bash swarm/scripts/portal_logic_acceptance.sh` passes on live Docker HTTPS.
2. `bash swarm/scripts/portal_visual_audit.sh` passes on live Docker HTTPS.
3. `bash swarm/scripts/portal_polish_audit.sh` passes on live Docker HTTPS.
4. `bash swarm/scripts/portal_full_page_audit.sh` passes on live Docker HTTPS.
5. Mobile 390px screenshots show stacked records with labels, not compressed horizontal tables.
6. Usage and dashboard logs show `Tok/s` as compact values such as `89K`, not meaningless blank/giant millisecond-only output.
7. No real user/provider/model data is deleted by tests; cleanup may only target isolated test prefixes.

## Remaining product bar for true WOW

The current Portal is now usable and professional enough for V1 admin testing. It is not yet a rich Hermes/OpenRouter-style executive dashboard. The next design pass should focus on:

- clearer empty-state onboarding when DB has zero model routes;
- richer dashboard trend panels once real traffic exists;
- better visual hierarchy for provider/model cards, especially when there are many routes;
- route creation wizard copy that is shorter and more step-based;
- first-run seeded examples that are visible without requiring manual SQL.

These are design maturity items, not the same broken-layout bugs fixed in this round.
