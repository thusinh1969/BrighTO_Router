# CODEX counter-verdict — Portal not fully accepted yet — 2026-09-17

DeepSeek handoff read: `swarm/audits/DEEPSEEK-PORTAL-SOTA-REDESIGN-20260917.md`.

Codex validation of the current uncommitted worktree:

```bash
node --check /tmp/brighto-current-portal.js
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
python3 scripts/portal_smoke.py
```

Result: PASS.

## Accepted for this slice

The current slice is technically moving in the right direction:

- Portal JS parses.
- Rust fmt/clippy pass.
- Runtime smoke passes 12/12.
- Admin login shape exists: username `admin`, password `ADMIN_MASTER_KEY`.
- Admin key reveal endpoint works for BrighTO client API keys.
- `/admin/backends` can now create a provider.
- `/admin/settings` returns runtime status.
- Provider secrets are still not returned by list endpoints.

## Not accepted as "Portal done"

DeepSeek classified these as future rounds:

```text
model_routes pricing/context columns + money budget + ledger cost fields
stats grouped by provider/model/team/key + error rate + p95
provider test-connection + provider-key update flow
DELETE /admin/routes/{model_name}
```

Codex disagrees for the user-requested open-source product. These are not enterprise extras. They are required for a credible Admin Portal because the user explicitly asked for:

- route creation with provider selection, provider API setup, loaded model selection, max tokens/context, price per 1M input, price per 1M output;
- team budget in money or unlimited;
- API key expiry and budget;
- dashboard total tokens by provider, model, team, and full system;
- professional Portal comparable in clarity to OpenRouter/Hermes-style dashboards.

## Required next implementation cut

Do this before asking Codex for Playwright acceptance:

1. **Route data model**
   - Add minimal columns needed for admin/product display:
     - provider model name;
     - context tokens;
     - max output tokens;
     - USD price per 1M input tokens;
     - USD price per 1M output tokens.
   - Expose them in `GET/POST /admin/routes`.
   - Do not add a complex billing service.

2. **Cost-ready usage**
   - Either store computed cost in `usage_ledger`, or compute it from route price at query time.
   - Usage screens must never show fake spend.
   - If cost cannot be computed for old rows, show `unknown` or `—`.

3. **Usage API filters**
   - Add filters needed by Portal:
     - backend/provider;
     - model;
     - team;
     - key;
     - date range;
     - status class.
   - Add grouped summaries for dashboard cards/table:
     - all system;
     - by provider;
     - by model;
     - by team;
     - by key.

4. **Portal forms**
   - Model route modal must include price/context/max-token fields.
   - API key modal must include expiry, budget mode, RPM limit, concurrency limit.
   - Team modal must expose friendly budget choices; raw JSON can stay under Advanced.
   - Usage screen must expose filters, not only raw logs.

5. **Smoke coverage**
   - Keep current smoke tests.
   - Add reveal negative cases:
     - bad admin key -> 401/403;
     - client key cannot reveal admin endpoint;
     - `key_secret IS NULL` -> 410.
   - Add smoke for route price/context fields once implemented.

## Playwright gate status

Do not request Playwright yet. Codex will run Playwright only after the flows above exist in the real UI. The Playwright run must prove actual clicks/forms/browser behavior, not just endpoint availability.

