# CODEX audit — Mobile request status pill

Date: 2026-09-18
Verdict: ACCEPTED for this polish slice.

## Problem

On mobile Recent requests, the HTTP status pill could stretch into a full-width green bar because a later mobile CSS rule changed `.request-card-head` to a single grid column. The model name became readable, but the status indicator looked like a progress bar instead of a compact status badge.

## Fix

- `static/index.html`
  - Mobile request card header now uses `minmax(0, 1fr) auto` for model/time plus compact status.
  - The direct status pill is constrained with `width:auto`, `max-width:max-content`, `justify-self:start`, and `white-space:nowrap`.
- `swarm/scripts/portal_full_page_audit.mjs`
  - Added `requestStatusPills` metrics.
  - Mobile Dashboard and Usage now fail if request status pills are stretched or not self-aligned as compact pills.

## Verification

Live HTTPS checks against `https://127.0.0.1:18443`:

- `portal_full_page_audit.sh` — PASS with the new status-pill gate.
- `portal_polish_audit.sh` — PASS.
- `portal_static_gate.py` — PASS.

Visual evidence:

- `swarm/out/playwright/20260918-044114-portal-full-page-audit/mobile-390-dashboard.png` shows the status `200` as a compact pill at the top-right of Recent requests, while the model name still wraps.
