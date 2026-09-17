# CODEX audit — Portal Add Model save action wording — 2026-09-18

## Root cause

The Add Model wizard used the button label `Save disabled`. The behavior was correct, but the wording was technical and easy to misread. The product intent is:

- untested routes may be saved only as disabled drafts;
- enabled routes require a passing `Test connection` result;
- clients can call only enabled routes.

## Fix

Changed `static/index.html`:

- renamed `Save disabled` to `Save draft`;
- added a clear title: `Saves a disabled draft; it cannot receive client traffic until tested and enabled`;
- kept `Save enabled` disabled until connection test passes;
- kept backend behavior unchanged: draft save still persists `enabled:false`.

Updated Playwright contracts:

- `swarm/scripts/portal_modal_surface_audit.mjs`
- `swarm/scripts/portal_logic_acceptance.mjs`
- `swarm/scripts/selfcheck_newflow.mjs`
- `swarm/scripts/portal_logic_acceptance_instrumented.mjs`

## Verification

Live HTTPS container, base URL `https://127.0.0.1:18443`:

- `./swarm/scripts/portal_modal_surface_audit.sh` — PASS
- `./swarm/scripts/portal_logic_acceptance.sh` — PASS
- `./swarm/scripts/portal_full_page_audit.sh` — PASS
- `./swarm/scripts/portal_user_journey_audit.sh` — PASS
- `./swarm/scripts/portal_polish_audit.sh` — PASS

Representative screenshot checked manually:

- `swarm/out/playwright/20260918-024514-portal-modal-surface-audit/desktop-add-model.png`

## Verdict

This is a UX contract cleanup. It reduces confusion without adding new state, services, or backend complexity. The safety rule remains intact: a route cannot be saved enabled until the endpoint test passes.
