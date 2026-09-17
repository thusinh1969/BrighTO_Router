# CODEX Portal provider pre-register copy — 2026-09-17

## Result

PASS. The Providers page no longer implies admins must create a provider before creating a model route.

## Change

- `static/index.html`
  - Renamed Providers page create CTA from `Add provider` to `Pre-register provider`.
  - Renamed create modal title to `Pre-register provider`.
  - Renamed create action to `Pre-register`.
  - Updated helper copy to explain that most teams should use `Models & Routes -> Add model`, and manual provider creation is only for preparing or disabling an endpoint before adding routes.

- `swarm/scripts/portal_polish_audit.mjs`
  - Fails if Providers page regresses to `Add provider` as the primary CTA.
  - Verifies the provider modal explains pre-registration and does not expose stale `Provider Type` jargon.

- `swarm/scripts/portal_logic_acceptance_instrumented.mjs`
  - Updated provider CRUD selectors for the new CTA/action label.

## Verification

```bash
python3 swarm/scripts/portal_static_gate.py
node --check /tmp/brighto-portal.js
node --check swarm/scripts/portal_polish_audit.mjs
node --check swarm/scripts/portal_logic_acceptance_instrumented.mjs
git diff --check
bash swarm/scripts/portal_polish_audit.sh
bash swarm/scripts/portal_logic_acceptance.sh
```

Responsive surface checks:

```bash
portal_full_page_audit PASS
portal_modal_surface_audit PASS
```

Evidence log prefix:

```text
swarm/out/*-provider-preregister-copy-230623.log
swarm/out/portal_logic_acceptance-provider-preregister-230601.log
```
