# CODEX AUDIT — Visual round 3: action column fixed, mobile sidebar still blocks V1

Date: 2026-09-17 17:39 +07
Live target: `https://127.0.0.1:18443`
Docker image rebuilt/recreated before test: `thusinh1969/brighto_airouter:v1`
Product change under test: uncommitted `static/index.html` table `colgroup` / action width update after commit `00dbe1e`.

## Verdict: still RED, but scope is now narrow

The latest table layout change fixed the action-column failure.

Remaining blocker: mobile sidebar stays open after selecting a navigation item and covers most of the screen.

## Commands run

```bash
cargo check --workspace
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
```

Results:

- `cargo check --workspace`: PASS
- Docker build/recreate: PASS
- `portal_logic_acceptance.sh`: PASS
- `portal_visual_audit.sh`: FAIL

Latest artifact:

- `swarm/out/playwright/20260917-173752-portal-visual-audit/summary.json`
- `swarm/out/playwright/20260917-173752-portal-visual-audit/mobile-390-models.png`

## What is now fixed

Visual gate passes these items:

- desktop 1440: no whole-page horizontal overflow
- desktop 1440: public model cell controlled
- desktop 1440: action column wide enough
- laptop 1024: no whole-page horizontal overflow
- laptop 1024: public model cell controlled
- laptop 1024: action column wide enough
- mobile 390: no whole-page horizontal overflow
- mobile 390: public model cell controlled
- mobile 390: action column wide enough

## Remaining confirmed failure

Playwright evidence on mobile 390px:

```json
{
  "sidebarOpen": true,
  "sidebar": { "width": 240, "right": 240 },
  "viewport": { "width": 390, "height": 844 }
}
```

Screenshot shows the sidebar covering the content after selecting `Models & Routes`.

## Required root-cause fix

Fix navigation state, not table CSS.

When a nav item is selected on mobile/tablet, close the sidebar overlay.

Concrete implementation target:

- Update `go(view)` or each nav click path so that after setting the active view, if `window.innerWidth <= 820`, remove `open` from `#sidebar`.
- Keep hamburger working to reopen it.
- Optional but cleaner: add a backdrop behind the sidebar while open; clicking backdrop closes the sidebar.

Acceptance condition:

```bash
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
```

Both must pass against the rebuilt Docker container. Do not mark done on source/static check only.
