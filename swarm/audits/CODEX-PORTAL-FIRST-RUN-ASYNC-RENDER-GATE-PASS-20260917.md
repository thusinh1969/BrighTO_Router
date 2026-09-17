# CODEX AUDIT — Portal first-run empty state + async render race fixed

Date: 2026-09-17
Runtime tested: live Docker HTTPS at `https://127.0.0.1:18443`

## Verdict

PASS for first-run Portal usability and render stability.

A real Playwright empty-state audit exposed a root cause bug: Dashboard could render duplicate panels/buttons when the same async view was requested twice quickly. The page render function was firing async renderers without a sequence guard, so stale async work could append into the current `#content` after a newer render had started.

## Fix

`static/index.html` now has:

- a render sequence guard: `renderSerial`, `isCurrentRender()`, and `finishRender()`;
- guarded async Dashboard, Usage, and Settings rendering;
- a first-run `Launch checklist` panel when Admin has zero model routes;
- first-run CTAs: `Add first model` and `View API keys`;
- Usage empty group panels show explanatory empty states instead of naked empty tables.

## Gate added

New command:

```bash
bash swarm/scripts/portal_empty_state_audit.sh
```

It runs against live Docker HTTPS and requires:

1. DB starts with zero routes and zero providers for this audit.
2. Dashboard shows exactly one `Launch checklist` panel.
3. Dashboard shows exactly one `Add first model` CTA.
4. Dashboard has `View API keys` CTA.
5. `Add first model` opens the real Add model wizard.
6. The wizard exposes Provider, API key, and Test connection controls.
7. Usage empty state does not render empty tables with only headers.
8. Browser console has no errors.

## Commands run

```bash
cargo test --workspace
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
bash swarm/scripts/portal_empty_state_audit.sh
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
bash swarm/scripts/portal_polish_audit.sh
bash swarm/scripts/portal_full_page_audit.sh
```

## Results

- Rust tests: 64 passed.
- Empty-state audit: PASS.
- Logic acceptance: PASS.
- Visual responsive audit: PASS.
- Portal polish audit: PASS.
- Full-page audit: PASS.
- Test cleanup: PASS; no `pw-*` test routes remained.

Latest full-page screenshot run:

`swarm/out/playwright/20260917-181251-portal-full-page-audit/`

Latest empty-state screenshot run:

`swarm/out/playwright/20260917-181213-portal-empty-state-audit/`

## Remaining bar

This closes a concrete first-run/product-stability bug. It does not close the broader “Hermes/OpenRouter-level” design goal. The remaining work is design maturity, not broken functionality: richer onboarding visuals, stronger dashboard information hierarchy, and more polished route wizard microcopy.
