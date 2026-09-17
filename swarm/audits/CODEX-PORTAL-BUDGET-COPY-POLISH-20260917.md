# CODEX Portal budget copy polish — 2026-09-17

## Result

PASS. Team and API Key modals no longer expose `Advanced JSON` as a primary label.

## Change

- Replaced `Advanced JSON (money budget, per-model caps)` with `Advanced budget rules`.
- Added plain helper copy: `Optional: use this only for money caps or per-model caps.`
- Kept the underlying advanced JSON textarea behavior unchanged for power users.

## Verification

```bash
python3 swarm/scripts/portal_static_gate.py
node --check /tmp/brighto-portal.js
node --check swarm/scripts/portal_modal_surface_audit.mjs
git diff --check
bash swarm/scripts/portal_modal_surface_audit.sh
bash swarm/scripts/portal_logic_acceptance.sh
```

Audit gates now fail if Team/API Key modals regress to `Advanced JSON` copy.

Evidence:

```text
swarm/out/portal_logic_acceptance-budget-copy-225013.log
```
