# CODEX AUDIT — Visual round 4: provider cell still overlaps next column

Date: 2026-09-17 17:42 +07
Live target: `https://127.0.0.1:18443`
Docker image rebuilt/recreated before test: `thusinh1969/brighto_airouter:v1`
Product commit under test: `dadc265 fix: close mobile sidebar after nav (responsive)`

## Verdict: RED — do not mark done

The previous blockers are fixed:

- action column now fits `Disable / Edit / Delete`,
- mobile sidebar closes after selecting `Models & Routes`,
- logic acceptance still passes.

New gate now checks adjacent text cells. The desktop screenshot still shows Provider text touching/overlapping the Provider Model column.

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

Latest visual artifact:

- `swarm/out/playwright/20260917-174207-portal-visual-audit/summary.json`
- `swarm/out/playwright/20260917-174207-portal-visual-audit/desktop-1440-models.png`
- `swarm/out/playwright/20260917-174207-portal-visual-audit/laptop-1024-models.png`
- `swarm/out/playwright/20260917-174207-portal-visual-audit/mobile-390-models.png`

## Confirmed failure

Provider cell overflows into the next column:

- desktop 1440: `clientWidth=140`, `scrollWidth=155`, text `Visual Audit Custom LLM`
- laptop 1024: `clientWidth=140`, `scrollWidth=155`, text `Visual Audit Custom LLM`
- mobile 390: `clientWidth=140`, `scrollWidth=155`, text `Visual Audit Custom LLM`

This is visible in the desktop screenshot as `Visual Audit Custom LLMmock-model` with no clean separation.

## Required root-cause fix

Apply the same controlled text pattern to Provider and Provider Model cells as Public Model:

- text must be inside a truncating element or the cell itself must have `overflow:hidden; text-overflow:ellipsis; white-space:nowrap`,
- full value should remain accessible via `title`,
- optional copy button is useful but not required for Provider,
- do not solve by endlessly widening the table; the root problem is uncontrolled nowrap text in fixed columns.

Concrete UI target:

- Provider column may stay 140px or be slightly wider, but text must never bleed into Provider Model.
- Provider Model column must also handle long real names such as `deepseek-chat-v4.0-pro-2026-extended-context`.
- Public Model, Provider, Provider Model, Status, and Actions must all have clean visual boundaries.

## Gate update

Codex updated `swarm/scripts/portal_visual_audit.mjs` to fail if Provider or Provider Model cell text overflows into the next column.

Acceptance condition remains:

```bash
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
```

Both must pass on the rebuilt Docker container.
