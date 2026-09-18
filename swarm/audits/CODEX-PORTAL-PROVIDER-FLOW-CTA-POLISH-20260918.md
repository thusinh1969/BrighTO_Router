# Codex audit — Providers page must not pull admins back into the old setup flow

Date: 2026-09-18

## Finding

The Providers page still promoted `Prepare connection` as the primary action. That conflicted with the intended simple setup flow: admins should normally use `Models & Routes -> Add model`, where they choose the provider, enter the API key, load models, test, and save. Provider connection setup is only an advanced/manual path.

The page also used the phrase `upstream endpoint`, which is unnecessary technical wording for the Portal.

## Fix applied

- `static/index.html`
  - Providers page primary CTA is now `Add model`.
  - Provider manual setup is now secondary: `Advanced connection`.
  - CTA titles explain recommended vs optional flow.
  - Replaced `upstream endpoint` wording with `provider endpoint` in the visible Portal copy.

- `swarm/scripts/portal_full_page_audit.mjs`
  - Added desktop/mobile regression gates requiring Providers to lead with `Add model`.
  - Added gate requiring `Advanced connection` to be marked optional.
  - Added gate rejecting `Prepare connection` as the primary page action.
  - Added gate rejecting `upstream endpoint` jargon on the Providers page.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` — PASS
  - Desktop Providers showed `Add model` and `Advanced connection`.
  - Mobile Providers showed `Add model` and `Advanced connection`.
  - No `Prepare connection` page CTA.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh` — PASS
