# CODEX AUDIT — VISUAL / RESPONSIVE UI FAIL

Date: 2026-09-17  
Role: Codex auditor. Product code remains DeepSeek-owned.

## Verdict

**Portal logic is close, but UI polish is NOT accepted.**

The current interface is functional, but it is not yet professional enough for a V1.0 public launch. A long public model name breaks the Models table visually on desktop and laptop widths. This matches the user's report: text overlaps status/action UI and looks unpolished.

## Evidence

Focused visual Playwright audit:

- Artifact summary: `swarm/out/playwright/visual-responsive-20260917-172138/summary.json`
- Desktop screenshot: `swarm/out/playwright/visual-responsive-20260917-172138/desktop-models.png`
- Laptop screenshot: `swarm/out/playwright/visual-responsive-20260917-172138/laptop-models.png`
- Result: FAIL

Test data:

```text
pw-visual-super-long-public-model-name-for-enterprise-qwen3-flash-next-1m-token-router-route-overflow-check
```

Observed CSS/runtime metrics:

### Desktop 1440px

- Public model cell width: `121px`
- Public model cell scroll width: `652px`
- `overflow: visible`
- `text-overflow: clip`
- `white-space: nowrap`
- Action cell width: `121px`
- Action cell scroll width: `195px`

### Laptop 1024px

- Public model cell width: `75px`
- Public model cell scroll width: `781px`
- `overflow: visible`
- `text-overflow: clip`
- `white-space: nowrap`
- Action cell width: `75px`
- Action cell scroll width: `195px`

Visual result: public model text runs across other columns, status pill is overprinted, and action buttons are squeezed/overlapping. This is not acceptable for OpenRouter/Hermes-level dashboard quality.

## Root cause

Current CSS applies table-wide equal fixed columns but does not control overflow inside cells:

```css
table { width:100%; table-layout:fixed }
th,td { white-space:nowrap }
td.mono { font-family:var(--mono); font-size:12px }
```

Only the provider base URL cell has manual ellipsis. Models table cells do not.

## Required fix — Models table layout

Do not keep equal-width columns for model rows. Use an explicit layout.

Preferred simple fix:

1. Give the Models table a minimum width around `1180px` so columns do not collapse into 75px.
2. Use `colgroup` or per-cell classes for widths:
   - Public model: flexible, minimum 260px
   - Status: 130px
   - Provider: 140px
   - Provider model: flexible, minimum 220px
   - Auth: 90px
   - Context: 90px
   - Max out: 90px
   - Price: 110px
   - Actions: 220px
3. For long model names:
   - Wrap the visible text in `.truncate` with `overflow:hidden; text-overflow:ellipsis; white-space:nowrap; display:block; max-width:100%`.
   - Add `title` attribute with the full model name.
   - Add a small copy button/icon beside the name.
4. Do not let status pills or action buttons share visual space with the model name.
5. Action column must have reserved width and no clipping. Current buttons require at least 195px; use 220px.

Acceptance checks:

- At 1440px and 1024px, long model name must not overlap any other column.
- `firstCell.scrollWidth > firstCell.clientWidth` is acceptable only if `text-overflow: ellipsis` and `overflow: hidden` are active.
- Action cell `clientWidth >= scrollWidth` for `Disable/Edit/Delete`.

## Required fix — responsive behavior

For viewport `<= 1100px`, choose one of these simple patterns:

### Option A: horizontal table, but contained and readable

- Table keeps min-width.
- `.table-wrap` scrolls horizontally.
- Whole page must not scroll horizontally.
- First column and action column may be sticky if clean.

### Option B: card rows for Models/Connections on mobile

- Each route becomes a compact card:
  - title: public model name, ellipsized with copy button
  - status badge
  - provider + provider model
  - auth/protocol small text
  - context/max/cost in one line
  - actions at bottom right

Option B looks more professional and avoids table squeeze on phones.

Acceptance checks:

- 390px mobile: hamburger navigation must let the user reach Models, Providers/Connections, Teams, API Keys, Usage, Settings.
- No whole-page horizontal overflow.
- Long model names must not cover buttons or badges.
- All buttons remain clickable by normal Playwright click, no `force:true` for final visual gate.

## Required fix — dashboard information hierarchy

Current dashboard still feels thin for an Admin. Keep it simple, but make it look like an operations console.

Admin dashboard should prioritize:

1. Spend today / 30 days.
2. Total tokens input/output.
3. Requests and error rate.
4. Tokens/second from recent calls.
5. Provider health: success, 429, 5xx, timeout.
6. Top models by tokens/cost.
7. Recent calls with model, team, provider, status, tokens, tok/s, cost.

Avoid showing low-value technical labels in primary cards. Put p95 router overhead in diagnostics, not as a headline unless there is traffic.

## Required fix — visual standard

Make the UI feel like a serious SaaS admin tool:

- Use consistent vertical rhythm: table rows 36–44px, modal fields compact but readable.
- Use muted text for metadata; reserve bright colors for status/primary action.
- Avoid large empty panels when there is no usage. Empty state should tell Admin exactly what to do next.
- Use `Connections` instead of `Providers` if the page is internal provider instances; keep `Provider` for the catalog choice in Add model.
- Add real chart area only when there is data; otherwise show a clean empty state.

## Gate requirement before accepting UI polish

Add a visual gate or extend Playwright to prove:

1. Long public model name at 1440px does not overlap status/provider/actions.
2. Long public model name at 1024px does not overlap status/provider/actions.
3. Action column has enough width for Disable/Edit/Delete.
4. Mobile navigation works via hamburger.
5. Mobile Models view is usable with the same long model name.
6. Screenshots saved under `swarm/out/playwright/<timestamp>/`.

Do not claim V1.0 UI polish until this gate passes.
