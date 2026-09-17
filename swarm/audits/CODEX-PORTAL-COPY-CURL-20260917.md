# CODEX audit — User Portal copy-cURL quickstart

Date: 2026-09-17
Role: auditor/implementer for frontend polish

## Verdict

PASS after local implementation and Playwright verification.

## Change

User Portal dashboard now has two explicit actions in the `Call endpoint` panel:

- `Copy endpoint` copies the OpenAI-compatible `/v1/chat/completions` endpoint URL.
- `Copy cURL` copies a complete terminal-ready smoke request with:
  - current Portal origin,
  - `Authorization: Bearer <your API key>` placeholder,
  - `Content-Type: application/json`,
  - first allowed model from the logged-in key,
  - a tiny `Reply OK` payload.

The copy payload is also stored in `data-copy` so Playwright can assert the actual generated command, not only the visible button text.

## UI polish

The call endpoint grid now uses wider cards on desktop so `Authorization: Bearer <your API key>` is readable without ugly wrapping. Mobile keeps the one-column layout.

## Verification

Static/parser gates:

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal JS `node --check` — PASS
- `node --check swarm/scripts/portal_user_journey_audit.mjs` — PASS
- `git diff --check` — PASS

Full Portal suite:

- `portal_login_audit` — PASS
- `portal_empty_state_audit` — PASS
- `portal_logic_acceptance` — PASS
- `portal_visual_audit` — PASS
- `portal_polish_audit` — PASS
- `portal_full_page_audit` — PASS
- `portal_modal_surface_audit` — PASS
- `portal_user_journey_audit` — PASS

Primary evidence log prefix: `swarm/out/*-copy-curl-215252.log`.
