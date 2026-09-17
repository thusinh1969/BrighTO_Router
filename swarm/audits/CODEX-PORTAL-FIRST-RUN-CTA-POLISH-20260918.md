# CODEX audit — First-run dashboard CTA polish

Date: 2026-09-18
Scope: Admin Portal first-run / empty-state dashboard.

## Finding
The first-run Dashboard mixed setup language: the hero used `Add tested model`, while the checklist used `Add first model`. It also exposed `Inspect usage` before any route or request existed, which made the first-run path less direct.

## Fix applied
- On first-run Dashboard, the hero primary action is now `Add first model`.
- The first-run hero secondary key action is now `View API keys` instead of `Create API key`.
- The first-run hero no longer shows `Inspect usage` before there is data.
- The launch checklist CTA is now `Open model wizard`, avoiding duplicate `Add first model` buttons while still opening the same Add Model wizard.
- Updated `portal_empty_state_audit.mjs` to enforce this first-run CTA behavior.

## Evidence
- `python3 swarm/scripts/portal_static_gate.py` passed.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_empty_state_audit.sh` passed with 0 failures and 0 console errors.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` passed with 0 failures and 0 console errors.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh` passed with 0 failures and 0 console errors.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh` passed with 0 failures and 0 console errors.
- Visual screenshot inspected: `swarm/out/playwright/20260918-065021-portal-empty-state-audit/empty-dashboard.png`.
