# CODEX POLL — Round 7 verification after DeepSeek fixes

Date: 2026-09-17 14:33 ICT  
Live target: `https://127.0.0.1:18443`  
HEAD verified: `b798b9d contain:layout on table-wrap for stable clicks`  
DeepSeek audit read: `swarm/audits/DEEPSEEK-ROUND6-CLOUD-BUDGET-CRUD-20260917.md`

## What is now fixed / verified

### Static + polish gates

Commands:

```bash
python3 swarm/scripts/portal_static_gate.py
bash swarm/scripts/portal_polish_audit.sh
```

Result:

```text
PORTAL_STATIC_GATE PASS
portal_polish_audit PASS
```

Live HTML contains `contain:layout`, no `flashrow`, Provider `Used by ... route(s)` title exists.

### Team budget null

Canonical gate now verifies:

```text
PASS teams: explicit budget:null clears team budget
```

This confirms `PATCH /admin/teams/{id}` now distinguishes omitted budget from explicit JSON null.

### Provider in-use delete guard

Canonical gate now verifies:

```text
PASS providers: in-use Provider Delete guard
```

After formal seed, `custom-local-llama` is referenced by `test-custom-local`; UI Delete is disabled.

### Cloud health/circuit fix

Updated `swarm/scripts/live_formal_provider_setup.py` to use correct protocol-aware smoke:

- OpenAI: `/v1/chat/completions`, `max_completion_tokens`, provider model `gpt-4o-mini`.
- Anthropic: `/v1/messages`, provider model `claude-opus-5`.
- DeepSeek: `/v1/chat/completions`, `deepseek-v4-pro`.
- Kimi: `/v1/chat/completions`, `kimi-k3`.
- Qwen: `/v1/chat/completions`, corrected provider model `qwen3.8-max` (not `qwen-3.8-max`).
- Z.AI: `/v1/chat/completions`, `glm-5.2`.
- Custom local: `/v1/chat/completions`, `qwen3.8-flash-next`, auth none.

The script waits 35 seconds before smoke so background health loop has time to poison cloud backends if the bug still exists.

Command:

```bash
python3 swarm/scripts/live_formal_provider_setup.py
```

Result:

```text
PASS test-openai status=200
PASS test-anthropic status=200
PASS test-deepseek status=200
PASS test-kimi status=200
PASS test-qwen status=200
PASS test-zai status=200
PASS test-custom-local status=200
```

Conclusion: cloud health fix is accepted for this round. No 503 after the 35s wait.

## Still failing / not accepted

### 1) Provider Delete after edit is still not actionably clickable by Playwright normal click

Canonical gate still fails:

```text
FAIL providers: unused Provider Delete button is not reliably clickable
FAIL providers: unused Provider delete through UI did not remove backend
```

Focused diagnostic on live HEAD:

```json
{
  "rect": { "left": 1106.5625, "top": 623.25, "width": 68, "height": 28 },
  "btnDisabled": false,
  "btnClass": "btn sm danger",
  "btnPointer": "auto",
  "btnVisibility": "visible",
  "btnDisplay": "flex",
  "topTag": "BUTTON",
  "topText": "Delete",
  "modalHidden": "hidden"
}
```

But:

```text
provider normal click FAIL locator.click: Timeout 5000ms exceeded.
provider_dialogs []
provider_exists_after true
provider force click deletes successfully
```

This is not a backend delete failure; it is a UI actionability/hit-test/stability problem. A normal Playwright click cannot complete even though `elementFromPoint()` is the button. Do not dismiss this as green until the canonical normal-click gate passes.

### 2) Model Route Delete after edit is still not accepted

Focused diagnostic:

```text
route normal click FAIL locator.click: Timeout 5000ms exceeded.
route_exists_after true
```

Route create/edit works. Route delete via normal click after edit is not accepted.

### 3) Lifecycle contract requested by user is not implemented yet

User contract:

- Cannot delete provider if it has at least one active model route.
- Cannot delete provider if it has any transaction in `usage_ledger.backend_id`.
- Can disable provider; all associated models become effectively disabled and cannot be used.
- Can manually disable model.
- Cannot delete model if it has any transaction in `usage_ledger.model`.
- Portal needs filter: Enabled only / Disabled only / All for Providers and Models.

Current code still lacks most of this:

- Provider table only guards delete using active route count from frontend `routes`; it does not show or enforce usage-history blockers in UI.
- Provider page has no Enabled/Disabled/All filter.
- Provider page has no explicit Disable/Enable action; only edit modal checkbox.
- Model page has no Enabled/Disabled/All filter.
- Model table has no Disable/Enable action.
- Model delete button is always rendered; no usage-history guard visible.
- API delete guards for transaction history need verification/implementation.
- Request admission should use effective enabled: `route.enabled && any referenced backend.enabled`. Disabled provider should produce a clear disabled/provider-disabled error, not generic 503.

## Required next DeepSeek actions

1. Fix normal-click actionability for Provider Delete and Route Delete. Acceptance must use normal Playwright click, not `force:true`.
2. Implement lifecycle contract in `CODEX-PROVIDER-MODEL-LIFECYCLE-CONTRACT-20260917.md`.
3. Add Provider and Model filters: Enabled only / Disabled only / All.
4. Add Provider and Model Disable/Enable actions outside edit modal.
5. Add usage-history delete blockers using existing PostgreSQL `usage_ledger`; no Redis.
6. Rebuild/recreate Docker and run:

```bash
python3 swarm/scripts/portal_static_gate.py
bash swarm/scripts/portal_polish_audit.sh
bash swarm/scripts/portal_logic_acceptance.sh
python3 swarm/scripts/live_formal_provider_setup.py
```

Do not say done until lifecycle delete/disable/filter tests are also added and passing.
