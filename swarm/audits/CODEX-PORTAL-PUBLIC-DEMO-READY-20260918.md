# CODEX — Portal Public Demo Ready — 2026-09-18

Scope:

- `static/index.html`
- `PROVIDERS.md`
- `swarm/scripts/portal_logic_acceptance.mjs`
- `swarm/scripts/portal_logic_acceptance_instrumented.mjs`

## Verdict

PASS. The Portal is ready for a public V1 demo after the final Docker release and post-restart gates pass.

## Final polish in this round

1. Models desktop route actions now stay on one compact row instead of stacking vertically.

2. Provider terminology is consistent:
   - Provider setup docs and Portal copy use `provider endpoint`.
   - `Test connection` remains unchanged because it is the user action that tests the network/model call.

3. Models page hint now says the endpoint is created or reused for the route.

4. API key layout from the previous round remains intact:
   - Scope is model access only.
   - Expiry belongs in Limits.
   - Limits includes expiry, requests/min, simultaneous calls, budget, and state.

5. Generated logic acceptance script was regenerated from the current source so stale Provider wording does not remain in the audit surface.

## Public-demo gate suite before Docker release

Commands run against live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
python3 swarm/scripts/portal_static_gate.py
node --check swarm/scripts/portal_login_audit.mjs
node --check swarm/scripts/portal_empty_state_audit.mjs
node --check swarm/scripts/portal_logic_acceptance.mjs
node --check swarm/scripts/portal_logic_acceptance_instrumented.mjs
node --check swarm/scripts/portal_visual_audit.mjs
node --check swarm/scripts/portal_full_page_audit.mjs
node --check swarm/scripts/portal_polish_audit.mjs
node --check swarm/scripts/portal_user_journey_audit.mjs
node --check swarm/scripts/portal_modal_surface_audit.mjs
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_login_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_empty_state_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_logic_acceptance.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_visual_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
```

Results:

- `portal_static_gate.py`: PASS
- `portal_login_audit.sh`: PASS, 0 failures, 0 console errors
- `portal_empty_state_audit.sh`: PASS, 0 failures, 0 console errors
- `portal_logic_acceptance.sh`: PASS, 0 failures, 0 console errors
- `portal_visual_audit.sh`: PASS, 0 failures, 0 console errors
- `portal_full_page_audit.sh`: PASS, 0 failures, 0 console errors
- `portal_polish_audit.sh`: PASS, 0 failures, 0 bugs, 0 console errors
- `portal_user_journey_audit.sh`: PASS, 0 failures, 0 console errors
- `portal_modal_surface_audit.sh`: PASS, 0 failures, 0 console errors

Visual evidence:

- `swarm/out/playwright/20260918-102554-portal-visual-audit/desktop-1440-models.png`
