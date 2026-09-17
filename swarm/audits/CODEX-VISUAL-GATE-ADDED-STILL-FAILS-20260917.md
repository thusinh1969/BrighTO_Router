# CODEX AUDIT — VISUAL RESPONSIVE GATE ADDED; PORTAL UI STILL FAILS

Date: 2026-09-17  
Role: Codex auditor.

## Verdict

**Portal logic is accepted, but visual polish remains red.**

I added a dedicated visual regression gate so the next coding agent can no longer rely on manual screenshots or a logic-only pass.

## New gate

Files added:

- `swarm/scripts/portal_visual_audit.sh`
- `swarm/scripts/portal_visual_audit.mjs`

How to run:

```bash
bash swarm/scripts/portal_visual_audit.sh
```

The wrapper runs Docker Playwright by default:

- image: `mcr.microsoft.com/playwright:v1.63.0-noble`
- network: `host`
- browser: Chromium headless shell revision 1243

## What it tests

The gate creates a deliberately long public model name:

```text
pw-visual-...-super-long-public-model-name-for-enterprise-qwen3-flash-next-1m-token-router-route-overflow-check
```

Then it opens the live Portal and captures/inspects:

- desktop `1440x1000`
- laptop `1024x800`
- mobile `390x844`

It checks:

1. Models navigation is reachable, including via hamburger on mobile.
2. Whole page does not horizontally overflow.
3. Long public model text does not visually overlap other columns.
4. Long text is either controlled by ellipsis/hidden overflow or rendered in a non-table card layout.
5. Action column is wide enough for Disable/Edit/Delete, or UI collapses actions into a menu.
6. Screenshots and JSON summary are written under `swarm/out/playwright/<timestamp>-portal-visual-audit/`.

## Current result

Command run:

```bash
bash swarm/scripts/portal_visual_audit.sh
```

Result: FAIL.

Artifact:

- `swarm/out/playwright/20260917-172957-portal-visual-audit/summary.json`
- `swarm/out/playwright/20260917-172957-portal-visual-audit/desktop-1440-models.png`
- `swarm/out/playwright/20260917-172957-portal-visual-audit/laptop-1024-models.png`
- `swarm/out/playwright/20260917-172957-portal-visual-audit/mobile-390-models.png`

Failures:

- Desktop: long public model overflows without ellipsis.
- Desktop: action column too narrow.
- Laptop: long public model overflows without ellipsis.
- Laptop: action column too narrow.
- Mobile: long public model overflows without ellipsis.
- Mobile: action column too narrow.

Representative metrics:

- Laptop public model cell: `clientWidth=75`, `scrollWidth=706`, `overflow=visible`, `textOverflow=clip`.
- Laptop actions cell: `clientWidth=75`, `scrollWidth=195`.
- Mobile public model cell: `clientWidth=35`, `scrollWidth=706`.
- Mobile actions cell: `clientWidth=35`, `scrollWidth=195`.

## Required UI fix

Do not keep equal-width table cells for Models.

Acceptable fixes:

### Desktop/laptop

- Use explicit column sizing or `colgroup`.
- Public model/provider model cells must contain a truncating text wrapper:
  - `overflow: hidden`
  - `text-overflow: ellipsis`
  - `white-space: nowrap`
  - `title` with full value
  - copy button/icon
- Reserve action width, minimum around `220px`, or collapse actions into a menu.

### Mobile

Use a card layout for model rows or a clean contained horizontal table. Current equal-width table is not acceptable; 35px public model/action columns are unusable.

## Acceptance

After UI changes and Docker rebuild:

```bash
cargo check --workspace
cargo test --workspace
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
```

All four must pass before UI polish can be accepted for V1.0.
