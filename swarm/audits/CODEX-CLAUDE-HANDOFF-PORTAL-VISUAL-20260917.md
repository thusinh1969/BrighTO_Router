# CODEX HANDOFF — Portal visual polish status for next coder

Date: 2026-09-17 17:46 +07
Role: Codex auditor only. Product UI coding should be done by the next implementation agent.

## Current verdict

Core Portal logic is green. Visual polish is not green yet.

Do not call this DONE until the visual gate passes after Docker rebuild/recreate.

## Last known good areas

Verified on live Docker HTTPS Portal at `https://127.0.0.1:18443`:

```bash
cargo check --workspace
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
bash swarm/scripts/portal_logic_acceptance.sh
```

Logic gate passed:

- Admin login over HTTPS.
- Provider catalog from `.env`.
- Gemini disabled as coming soon.
- Add model wizard requires connection test before Save enabled.
- Custom LLM blank API key saves as `auth_mode=none` and `protocol=local_openai_chat`.
- Client smoke call returns HTTP 200 through the router.
- Model/provider lifecycle delete guards work.
- Team budget null clearing works.
- API key create/reveal works.
- User login hides admin menus.

Recent product fixes also verified:

- action column now reserves enough width,
- mobile sidebar closes after navigation,
- long public model name is controlled in the current gate.

## Remaining blocker

`bash swarm/scripts/portal_visual_audit.sh` fails because the Provider cell in Models & Routes overflows into Provider Model.

Last failing artifact:

- `swarm/out/playwright/20260917-174207-portal-visual-audit/summary.json`
- `swarm/out/playwright/20260917-174207-portal-visual-audit/desktop-1440-models.png`
- `swarm/out/playwright/20260917-174207-portal-visual-audit/mobile-390-models.png`

Evidence:

```json
{
  "summary": "desktop-1440: provider cell overflows into the next column",
  "clientWidth": 140,
  "scrollWidth": 155,
  "text": "Visual Audit Custom LLM"
}
```

Human-visible symptom: Provider and Provider Model look glued together, like `Visual Audit Custom LLMmock-model`.

## Exact code location

`static/index.html` currently handles Public Model and Provider Model differently:

- Public Model has a `.truncate` span plus copy button.
- Provider Model has a `.truncate` span.
- Provider uses raw `el("td", null, ...)`, so it can bleed into the next column.

Current problematic line area:

```js
tr.appendChild(el("td",null,r.backend_ids.map(function(id){var b=backends.find(function(x){return x.id===id});return b?b.name:"#"+id;}).join(", ")));
```

## Required implementation

Apply controlled text rendering to Provider exactly like Provider Model:

```js
var providerName = r.backend_ids.map(function(id){
  var b = backends.find(function(x){ return x.id === id; });
  return b ? b.name : "#" + id;
}).join(", ");
var pv = el("td");
var pvw = el("span", "truncate", providerName || "—");
pvw.title = providerName || "";
pv.appendChild(pvw);
tr.appendChild(pv);
```

Also check that any long Provider Model value remains safe, especially real provider names like long DeepSeek/Kimi/Qwen model IDs.

## Required verification after fix

Run this exact sequence because UI is embedded in the Rust binary via `include_str!`:

```bash
cargo check --workspace
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
```

Acceptance means both scripts return PASS against the rebuilt Docker container.

## Quality note

Passing the current visual gate means the known overlap/regression bugs are closed. It does not mean the Portal is visually WOW. For WOW/public launch, the next pass should still improve:

- mobile Models rows as cards or clearer horizontal-scroll affordance,
- tighter information hierarchy on Dashboard,
- consistent table/card spacing across Providers, Models, Teams, API Keys,
- no cramped text in real provider/model names.
