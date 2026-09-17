# CODEX audit — Portal friendly provider labels

Date: 2026-09-18
Verdict: ACCEPTED for this polish slice.

## Problem

The Portal could show generated/test-prefixed provider connection names such as `pw-fullvisual-...-Custom LLM` as the primary provider label. That is technically traceable but not professional for Admin UX. Provider rows should show a human provider label while preserving the raw backend name for traceability.

## Fix

- `static/index.html`
  - Added `providerDisplayName(raw)` to collapse generated names ending in a known catalog/provider label into the clean label, for example `Custom LLM`.
  - Added `displayNameNode(...)` so the visible label is clean while the raw backend name remains in `title`.
  - Applied the clean provider label in:
    - Providers list primary label.
    - Models & Routes provider column.
    - Fallback provider selector in the route modal.
- `swarm/scripts/portal_full_page_audit.mjs`
  - Added a regression gate: after seeding `${prefix}-Custom LLM`, Providers and Models must not expose `${prefix}-Custom LLM` in primary visible content.

## Verification

Sequential live HTTPS checks against `https://127.0.0.1:18443`:

- `portal_polish_audit.sh` — PASS
- `portal_full_page_audit.sh` — PASS
- `portal_modal_surface_audit.sh` — PASS
- `portal_logic_acceptance.sh` — PASS
- `portal_static_gate.py` — PASS
- Re-ran `portal_full_page_audit.sh` after adding the new regression gate — PASS

Visual evidence:

- `swarm/out/playwright/20260918-043019-portal-full-page-audit/desktop-1440-providers.png` shows provider label `Custom LLM`, not the generated test prefix.
- `swarm/out/playwright/20260918-043019-portal-full-page-audit/mobile-390-models.png` shows provider label `Custom LLM` in the model card.
