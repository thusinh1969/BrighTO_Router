# CODEX audit — Mobile primary data labels wrap

Date: 2026-09-18
Verdict: ACCEPTED for this polish slice.

## Problem

Mobile Dashboard and Usage still allowed primary data labels to truncate visually with CSS ellipsis. This affected chart legends, Top consumers / Top models rows, and Recent requests model labels. For an admin portal, the primary model/team labels must be readable on mobile without relying on hover tooltips.

## Fix

- `static/index.html`
  - Mobile CSS now wraps `.legend-label`, `.focus-title`, and `.request-model` instead of forcing `nowrap + ellipsis`.
  - Mobile focus rows stack value below the label so long names have room.
  - Mobile request cards keep status aligned while letting model names wrap.
- `swarm/scripts/portal_full_page_audit.mjs`
  - Added mobile regression inspection for primary data label CSS.
  - Dashboard and Usage now fail if primary data labels use `nowrap`, `ellipsis`, or clipped scroll dimensions on mobile.

## Verification

Live HTTPS checks against `https://127.0.0.1:18443`:

- `portal_full_page_audit.sh` — PASS with the new mobile label gate.
- `portal_polish_audit.sh` — PASS.
- `portal_static_gate.py` — PASS.

Visual evidence:

- `swarm/out/playwright/20260918-043628-portal-full-page-audit/mobile-390-dashboard.png` shows chart legend, Top models, and Recent requests model labels wrapping instead of truncating.
