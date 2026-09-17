# CODEX Portal model map preview — 2026-09-17

## Result

PASS. Add Model now shows a visible mapping preview:

```text
Client sends        ->        Provider receives
<public model>                <provider model>
```

## Why this matters

The model route form has two names:

- Public model name: what client applications send to BrighTO-Router.
- Provider model: the exact upstream model name sent to the provider.

Admins often confuse these fields when creating aliases or connecting providers with long model names. The preview makes the route behavior visible before testing and saving.

## Files changed

- `static/index.html`
  - Added compact `model-map` styling.
  - Added live preview in `openRouteModal()`.
  - Preview updates as Provider model and Public model fields change.

- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Fails if Add Model modal loses the public-to-provider preview.

## Verification

```bash
python3 swarm/scripts/portal_static_gate.py
node --check /tmp/brighto-portal.js
node --check swarm/scripts/portal_modal_surface_audit.mjs
git diff --check
bash swarm/scripts/portal_modal_surface_audit.sh
```

Live Portal flow checks:

```bash
portal_logic_acceptance PASS
portal_modal_surface_audit PASS
portal_full_page_audit PASS
```

Evidence log prefix:

```text
swarm/out/*-model-map-preview-223510.log
```
