# CODEX audit — Desktop chart legend wraps long model names

Date: 2026-09-18
Verdict: ACCEPTED for this polish slice.

## Problem

Desktop Dashboard and Usage chart legends could still truncate long public model names with CSS ellipsis. This made the chart less useful for real provider/model names, especially when public route names include provider, model family, and deployment suffixes.

## Fix

- `static/index.html`
  - `.legend-label` now wraps on all viewport sizes.
  - Removed the desktop `nowrap + ellipsis` behavior from chart legend labels.
- `swarm/scripts/portal_full_page_audit.mjs`
  - Added a Dashboard/Usage regression gate for legend labels.
  - The gate fails if a legend label uses `nowrap`, `ellipsis`, or clipped dimensions.

## Verification

Live HTTPS checks against `https://127.0.0.1:18443`:

- `portal_full_page_audit.sh` — PASS with the new desktop/mobile legend gate.
- `portal_polish_audit.sh` — PASS.
- `portal_static_gate.py` — PASS.

Visual evidence:

- `swarm/out/playwright/20260918-044604-portal-full-page-audit/desktop-1440-usage.png` shows the long model name wrapping inside the chart legend chip instead of truncating.
