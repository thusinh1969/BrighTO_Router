# CODEX audit — Disabled primary buttons no longer look active

Date: 2026-09-18
Verdict: ACCEPTED for this polish slice.

## Problem

The Add model modal showed disabled primary actions such as `Save enabled` with the same blue primary background, only reduced by opacity. On mobile this still looked like an active call-to-action and made the test gate less clear.

## Fix

- `static/index.html`
  - Added explicit disabled styling for `.btn.primary:disabled` and hover state.
  - Disabled primary buttons now use muted slate background, gray border, and muted text.
- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Added `disabledPrimaryButtons` computed-style metrics.
  - Modal audit now fails if disabled primary actions keep the active primary blue background/border.

## Verification

Live HTTPS checks against `https://127.0.0.1:18443`:

- `portal_modal_surface_audit.sh` — PASS with the new disabled-primary gate.
- `portal_full_page_audit.sh` — PASS.
- `portal_polish_audit.sh` — PASS.
- `portal_static_gate.py` — PASS.

Visual evidence:

- `swarm/out/playwright/20260918-045140-portal-modal-surface-audit/mobile-add-model.png` shows `Save enabled` as a muted disabled button instead of blue.
