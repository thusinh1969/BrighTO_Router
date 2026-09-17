# CODEX AUDIT — Portal visual/responsive round 2 still fails

Date: 2026-09-17 17:35 +07
Live target: `https://127.0.0.1:18443`
Docker image under test: `thusinh1969/brighto_airouter:v1`
Product commit under test: `00dbe1e fix: Models/Connections table truncate long names + min-width + copy button (visual responsive)`

## Verdict: RED — partial fix only, not professional enough for V1

The latest UI change improved one symptom: a long public model name no longer overlaps the Status badge in the Playwright gate.

It did not fix the root UI/responsive problem. The Models & Routes table is still visually brittle and mobile is still not acceptable.

## Commands run by Codex

```bash
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
```

Result:

- Logic acceptance: PASS.
- Visual responsive audit: FAIL.

Latest visual artifact:

- `swarm/out/playwright/20260917-173424-portal-visual-audit/summary.json`
- `swarm/out/playwright/20260917-173424-portal-visual-audit/desktop-1440-models.png`
- `swarm/out/playwright/20260917-173424-portal-visual-audit/laptop-1024-models.png`
- `swarm/out/playwright/20260917-173424-portal-visual-audit/mobile-390-models.png`

## Confirmed failures

### 1. Action column is too narrow on desktop and laptop

Evidence from Playwright:

- desktop 1440: action cell `clientWidth=131`, `scrollWidth=195`
- laptop 1024: action cell `clientWidth=131`, `scrollWidth=195`
- mobile 390: action cell `clientWidth=131`, `scrollWidth=195`

This means `Disable / Edit / Delete` cannot fit. It is clipped/hidden inside a horizontally scrolling table.

Required fix:

- Give the action column an explicit reserved width large enough for all buttons, or
- Replace row action buttons with one compact `Actions` button/dropdown, especially for small screens.

Do not rely on the table auto-layout guessing widths.

### 2. Mobile sidebar remains open after selecting a nav item

Evidence from Playwright mobile 390px:

- `.sidebarOpen = true` after clicking hamburger and then `Models & Routes`.
- Sidebar box remains visible: `width=240`, `right=240`, viewport width is only `390`.

The screenshot shows the sidebar covering most of the screen after navigation. This is not a responsive admin UI.

Required fix:

- In `go(view)` or nav click handling, close the sidebar when viewport is mobile/tablet.
- Expected state after selecting `Models & Routes`: `.sidebar` must not have class `open`.
- Keep hamburger available to reopen it.

### 3. Table presentation is still not V1 quality

Current solution uses a wide table with horizontal scroll. That is acceptable as a fallback, but the first visible screen still looks cramped and action controls are partly outside the usable area.

Required product behavior:

- Desktop/laptop: fixed/reserved widths for Status, Auth, Context, Max Out, Price, Actions; long names truncate with tooltip/copy.
- Mobile: either card layout per route, or table with an obvious horizontal scroll affordance plus a sticky/accessible action menu. Prefer card layout for Models, Providers, Teams, API Keys.
- Long public model and provider model names must never overlap another cell.
- Row height must remain compact but readable.

## Gate update

Codex also hardened the visual gate in `swarm/scripts/portal_visual_audit.mjs` so this cannot be missed again:

- retries transient navigation errors,
- checks mobile sidebar closes after navigation,
- checks selected page content remains usable on mobile,
- keeps the long public model/action width checks.

DeepSeek must run this after every UI fix:

```bash
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
```

## Acceptance rule

Do not say DONE until both commands pass on the live Docker Portal:

```bash
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
```

A screenshot-only/manual check is not enough.
