# CODEX Portal login copy polish — 2026-09-17

## Result

PASS. Login now states the product in plain language on both desktop and mobile.

## Change

- Replaced the hero headline `Route, budget, and observe every model call from one control plane.` with `One endpoint for every AI model your team uses.`
- Replaced `Rust LLM gateway` with `Fast Rust LLM gateway`.
- Replaced `simple portal` / `config reload` login metrics with `team endpoint` / `config refresh`.
- Added the product tagline inside the login card so mobile users see the product promise even though the desktop hero panel is hidden on small screens.

## Verification

```bash
python3 swarm/scripts/portal_static_gate.py
node --check /tmp/brighto-portal.js
node --check swarm/scripts/portal_login_audit.mjs
git diff --check
bash swarm/scripts/portal_login_audit.sh
```

Audit gates now fail if the login page regresses to `control plane` jargon or if the product statement disappears.

Evidence:

```text
swarm/out/playwright/20260917-230127-portal-login-audit/
```
