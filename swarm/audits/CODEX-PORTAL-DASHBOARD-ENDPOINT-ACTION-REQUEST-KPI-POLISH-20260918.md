# Codex audit — dashboard endpoint action and readable request KPI

Date: 2026-09-18
Scope: Admin Dashboard quick actions and mobile Recent requests KPI readability.

## Problems found

1. Admin Dashboard did not expose a direct `Copy endpoint` action. Admins had to know or find the OpenAI-compatible `/v1/chat/completions` URL elsewhere before sharing it with a team.
2. On mobile, the Recent requests `Tokens/sec` value could truncate important text. After the short-sample policy change, `Too short` rendered as `Too sh...` in a narrow KPI card.

## Fix applied

- Added `Copy endpoint` to Admin Dashboard quick actions.
- The button copies `location.origin + "/v1/chat/completions"`.
- Allowed the primary request KPI value to wrap, so `Too short` and other important values remain readable on mobile.
- Extended the full-page Playwright audit to verify:
  - Dashboard exposes `Copy endpoint` with the expected route URL.
  - Mobile request `Tokens/sec` KPI values are not ellipsized or clipped.

## Verification

Commands run against live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
python3 swarm/scripts/portal_static_gate.py
```

Results:

- `portal_full_page_audit`: PASS, 0 failures, 0 console errors.
- `portal_polish_audit`: PASS, 0 failures, 0 bugs, 0 console errors.
- `portal_static_gate`: PASS.

Key evidence from full-page audit:

```text
mobile dashboard bodyScrollWidth=390 clientWidth=390
mobile Tokens/sec value=Too short
mobile Tokens/sec textOverflow=clip
mobile Tokens/sec scrollWidth=clientWidth=72
Copy endpoint=https://127.0.0.1:18443/v1/chat/completions
```
