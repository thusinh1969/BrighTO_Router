# CODEX AUDIT — Dashboard information architecture blocker

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

Current Portal dashboard is still not product-grade.

The UI is showing technical leftovers instead of answering the questions an Admin or User actually has. The user is right: raw hundreds-of-thousands of milliseconds on the dashboard is meaningless and makes the product look broken after long-prompt tests.

## Root cause

The dashboard mixes three different concepts into one undifferentiated view:

1. Business usage: requests, tokens, cost, budget.
2. Provider/model health: success/error, timeout, provider latency.
3. Router performance: router overhead, queueing, config/ledger health.

A single global `P95 total ms` across all requests is not useful because it mixes:

- 1k-token quick requests;
- 50k-token long prompts;
- 200k-token local llama.cpp prefill;
- streaming and non-streaming;
- different providers/models.

After a 200k prompt test, global P95 total can become hundreds of thousands of milliseconds. That number mostly means “the backend spent time processing a huge prompt”, not “router is slow”. Showing it as a top dashboard card is misleading.

## What matters to Admin

Admin dashboard must answer these questions first:

1. Is the gateway usable right now?
   - router ready status;
   - configured model routes count;
   - enabled routes count;
   - providers reachable / failing;
   - ledger healthy or using fallback file.

2. How much are teams using?
   - total requests today / 24h / 30d;
   - input tokens;
   - output tokens;
   - estimated cost if route prices are configured;
   - top teams by spend/tokens;
   - top public models by spend/tokens.

3. Are we close to limits?
   - teams over 80% budget;
   - keys near RPM/concurrency/budget limits;
   - expired/disabled keys count.

4. What is failing?
   - error rate by provider/model/team;
   - latest failed requests with short error class;
   - provider 429/5xx/timeout count.

5. Is router overhead still tiny?
   - p95 router overhead in ms;
   - p99 router overhead in ms;
   - only show this under “Router health” or diagnostics, not as the main business card.

## What matters to User / Team developer

User Portal must answer these questions:

1. What key am I using?
   - owner label;
   - key prefix or full key if it was supplied in this browser session;
   - expiry;
   - enabled status.

2. What can I call?
   - allowed public model names;
   - context/max output if known;
   - price per 1M input/output if admin configured prices.

3. How do I call it?
   - BrighTO base URL;
   - copyable curl example;
   - copyable OpenAI SDK example;
   - clear text: use BrighTO key `lc-...`, not provider key.

4. How much did I use?
   - own input/output tokens;
   - own cost estimate;
   - own remaining budget;
   - own errors/recent requests.

User does not need Providers, provider credentials, route management, all teams, all keys, or admin settings.

## Required dashboard redesign

### Admin top cards

Replace the current mixed cards with these, in this order:

1. Gateway status
   - `Ready` / `Config stale` / `Ledger fallback`
   - small status pill, not a huge number.

2. Requests today
   - count;
   - compare to previous day only if available.

3. Tokens today
   - input + output combined headline;
   - footnote: `in X / out Y`.

4. Estimated cost today
   - show `$0.00` if all prices are zero/missing;
   - footnote: “configure route prices for cost”.

5. Error rate today
   - percent + failed request count;
   - red only if above threshold.

6. Enabled routes
   - `N enabled / M total`.

Do not show global `P95 total ms` as a top card.

### Admin charts

Show these charts visibly:

1. Tokens by day, stacked by public model.
2. Spend by team or tokens by team.
3. Error count by provider/model.

If no usage exists, show a helpful empty state:

> No usage yet. Create a route, create a BrighTO API key, and send one request.

If usage exists but chart is empty, that is a bug. Add debug-safe fallback text: number of rows received and selected date range.

### Admin tables

Dashboard should include compact tables:

- Top models: model, requests, tokens, estimated cost, errors.
- Top teams: team, requests, tokens, estimated cost, budget used.
- Provider health: provider, enabled routes, last error, 429/5xx/timeout count.

### Performance area

Move timing metrics into a section named “Performance diagnostics”.

Show:

- Router overhead p95/p99 in ms.
- First byte latency by provider/model, grouped by prompt size bucket.
- Total latency by provider/model, grouped by prompt size bucket.

Prompt size buckets:

- `<2k tokens`
- `2k–32k`
- `32k–128k`
- `128k+`

Never show one global latency number across all prompt sizes as if it is health.

### Formatting rule for time

Do not print raw huge milliseconds like `228453ms` in a top-level dashboard card.

Use friendly formatting:

- `<1000ms`: `123 ms`
- `<60s`: `1.8 s`
- `>=60s`: `3m 48s`

Add label context:

- `Provider total latency`
- `Router overhead`
- `First byte latency`

## Visual style requirements

The current background/glow treatment still looks like a generic dark template. Make it more professional and calmer.

Concrete direction:

- Keep dark theme, but reduce oversized blurred blobs on login.
- Use a clean neutral background with subtle blue accent.
- Cards should have consistent spacing and typography.
- Use one primary action color.
- Avoid heavy gradients behind functional content.
- Use status colors only for actual state: green ready, amber warning, red failing.
- Make tables denser and easier to scan.
- Use chart colors that remain readable on dark background.

OpenRouter-style means clean information hierarchy and fast route creation, not decorative gradients.

## Current chart-specific audit

`static/index.html` has `chartBars(stats)` and dashboard calls it when `/admin/stats?days=30` returns rows.

If the user sees no chart after real usage exists, DeepSeek must check:

1. Does `/admin/stats?days=30` return non-empty rows?
2. Does `renderDashboard()` swallow errors silently?
3. Are SVG bars rendered with zero height because max calculation is wrong?
4. Is chart below the fold because cards/tables take too much space?
5. Is user in User mode but expecting Admin data?
6. Did static HTML update require rebuild/restart because it is embedded by Rust `include_str!`?

Do not claim dashboard chart works until verified by Playwright screenshot after seeded usage data.

## Required API improvements for dashboard

`/admin/summary` should return fields that match UI decisions:

- totals for today and 30 days;
- by public model;
- by provider/backend;
- by team;
- by key;
- estimated cost when route prices exist;
- error counts grouped by status/error_class;
- router overhead p95/p99;
- provider latency p95/p99 grouped by prompt-size bucket.

If this is too much for one endpoint today, keep `/admin/summary` simple and add `/admin/performance`. Do not block UI cleanup on perfect analytics.

## Acceptance tests

DeepSeek must verify with Playwright, not eyeballing:

1. Admin login.
2. Dashboard with empty DB shows useful setup empty state.
3. Seed usage rows including 1k/50k/200k latency values.
4. Dashboard shows charts.
5. Top cards do not show raw global huge ms.
6. Performance diagnostics formats long latency as `3m 48s`, not `228453ms`.
7. Admin can see top models/teams/errors.
8. User login shows only own usage, allowed models, key status, and copyable usage examples.
9. User dashboard does not show admin/provider settings.
10. F5 refresh keeps the current portal mode and dashboard after session restore.

## Priority for DeepSeek

1. Fix product contract first: route owns provider credential.
2. Fix CRUD identity/delete and F5 session.
3. Redesign dashboard information architecture as above.
4. Then polish visual style/backgrounds.

Do not spend time making the current wrong metrics prettier. Remove or move the wrong metrics first.
