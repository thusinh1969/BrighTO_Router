# CODEX audit — Provider connection wording

Date: 2026-09-18
Role: Codex auditor / UI polish

## Verdict

PASS after fix.

The Providers screen used `Pre-register provider`, which was technically accurate but poor product language. It could make admins think provider creation is a required first step before adding a model. The actual product flow is simpler: use Models & Routes → Add model; a provider connection is created automatically. Manual provider setup is only an optional preparation step.

## Fix applied

- Providers CTA changed from `Pre-register provider` to `Prepare connection`.
- New-provider modal title changed to `Prepare provider connection`.
- Modal helper copy now says to prepare a connection only when setting up or disabling an upstream endpoint before routes.
- New-provider save button changed to `Prepare connection`.
- Portal polish audit now rejects the old pre-registration jargon and requires the clearer wording.
- Compatibility remains in one older instrumented script selector so historical fallback runs still work.

## Files changed

- `static/index.html`
- `swarm/scripts/portal_polish_audit.mjs`
- `swarm/scripts/portal_logic_acceptance_instrumented.mjs`

## Verification

Sequential gates against live HTTPS runtime:

- `portal_visual_audit.sh`: PASS
- `portal_full_page_audit.sh`: PASS
- `portal_polish_audit.sh`: PASS
- `portal_logic_acceptance.sh`: PASS
- `portal_modal_surface_audit.sh`: PASS
- `portal_login_audit.sh`: PASS

Visual check:

- Mobile Providers screen shows `Prepare connection`, keeping the optional connection-management meaning without implying a required provider-first setup flow.
