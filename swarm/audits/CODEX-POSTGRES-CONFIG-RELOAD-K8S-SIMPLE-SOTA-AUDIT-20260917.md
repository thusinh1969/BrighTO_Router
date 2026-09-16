# CODEX AUDIT — PostgreSQL config reload for multi-session / Kubernetes without over-engineering

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

Yes: PostgreSQL polling every 5 seconds is the correct open-source default for config reload across multiple sessions and Kubernetes pods.

Do not add Redis, etcd, Kafka, NATS, or another control-plane dependency for config propagation. PostgreSQL is already required for production config and usage ledger, so it should remain the single source of truth.

The architecture is SOTA enough if this invariant is kept:

> Request hot path never calls PostgreSQL. It reads an immutable in-memory snapshot through `ArcSwap`. A background task reloads config from PostgreSQL and swaps the whole snapshot every few seconds.

## Correct runtime model

### Admin writes

Admin Portal/API writes config changes into PostgreSQL:

- providers/templates;
- model routes;
- route credentials;
- teams;
- BrighTO client API keys;
- budgets/limits.

The pod that receives the admin write may call local `reload_notify` for immediate reload.

### Other pods

Other Kubernetes pods do not receive that local notify. They pick up the change on their next PostgreSQL poll, normally within `CONFIG_POLL_SECS=5`.

This is acceptable and simple:

- one pod sees the change immediately;
- other pods converge within about 5 seconds;
- no extra service is needed;
- request latency stays independent from PostgreSQL latency.

### Browser sessions

Browser sessions do not need server-side session storage for the open-source version.

- Admin session: browser stores admin credential locally with TTL and verifies it after refresh.
- User session: browser stores BrighTO client API key locally with TTL and verifies `/portal/me` after refresh.
- Every API call still authenticates by header.

No Redis/session table is required for F5 refresh.

## What must stay out of the hot path

The request path must not do any of these per request:

- PostgreSQL query for route lookup;
- PostgreSQL query for API-key auth;
- PostgreSQL query for budget check;
- provider key file/env read;
- config JSON parse;
- distributed lock;
- Redis call.

Allowed per request:

- read current `ArcSwap<ConfigSnapshot>`;
- hash client API key and lookup in in-memory map;
- check in-memory budget/rate/concurrency;
- choose backend from in-memory backend pool;
- forward request;
- enqueue usage event to async ledger writer.

## Current code issue DeepSeek must fix

`DbConfigLoader::load_snapshot()` currently includes a `usage_ledger` boot counter query:

```sql
SELECT COUNT(*), SUM(input_tokens), SUM(output_tokens) FROM usage_ledger
```

The comment says this is a boot counter, but the function is called by the periodic config poller. That means every 5 seconds the router can scan/summarize the ledger table.

This is wrong for production.

### Required fix

Move this ledger counter out of `load_snapshot()`.

Acceptable options:

1. Remove it completely.
2. Run it only once at boot.
3. Replace it with cheap metrics from the ledger writer.

Do not query large usage tables during config reload.

Acceptance test:

- Insert a large number of `usage_ledger` rows.
- Confirm config reload still queries only config tables.
- Confirm `/readyz` does not turn unhealthy because ledger summary is slow.

## Is full snapshot reload every 5 seconds OK?

Yes for the current product scale.

A full snapshot of provider/templates/routes/teams/API keys is simple and fast because config size is small compared with request volume. Reload cost is paid in the background, not by requests.

Expected practical scale for this design:

- hundreds or thousands of model routes;
- thousands or tens of thousands of client API keys;
- many teams;
- multiple router pods.

If someone reaches hundreds of thousands of API keys/routes, add versioned incremental reload later. Do not build that now.

## Optional tiny improvement: config version table

This is optional, but still simple if reload cost becomes visible.

Add one table:

```sql
config_meta (
  id SMALLINT PRIMARY KEY DEFAULT 1,
  version BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL
)
```

Every admin config write increments `version` in the same transaction. Each pod polls only this single row every 5 seconds. If version changed, it reloads the full snapshot.

This reduces DB work, but it is not required for first production if config tables are small. Do not add it unless reload overhead is actually measurable or the implementation remains very small.

## Polling behavior for Kubernetes

Use these defaults:

- `CONFIG_POLL_SECS=5`
- `CONFIG_RELOAD_TIMEOUT_MS=2000`
- `READY_MAX_STALE_MS=max(3 * CONFIG_POLL_SECS, 5s)`
- config DB pool: 1–2 connections per pod

Add a tiny startup jitter to avoid all pods polling PostgreSQL at the exact same millisecond after rolling deploy:

- first poll delay: random 0–500ms or deterministic jitter from pod name/hostname;
- normal interval remains 5 seconds.

This is not a new dependency and avoids a small thundering herd effect.

## Multi-pod budget/rate-limit rule

Be explicit in docs and UI:

Open-source default budgets/rate/concurrency are fast in-memory controls per router instance. PostgreSQL ledger is the usage source of truth for reporting and restart seeding.

For multiple pods, that means:

- config is shared and converges through PostgreSQL;
- usage is centralized through PostgreSQL ledger;
- routing works across pods;
- limits are fast but not mathematically strict globally at the same millisecond across all pods.

This is the right performance tradeoff for this repo.

If a customer requires strict global quota across many pods, there are only three honest options:

1. Run one router replica for strict quotas.
2. Use sticky routing by team/key so a key usually hits the same pod.
3. Add optional Enterprise Redis/global quota backend later.

Do not use PostgreSQL row locks or transactions on every request to enforce exact quota. That would destroy the “fastest router” goal.

## Acceptance tests

DeepSeek must add or run these tests before declaring this area done:

1. Start two router instances against the same PostgreSQL.
2. Create a new route through instance A.
3. Instance A sees route immediately after admin write/reload.
4. Instance B sees route within 5 seconds without restart.
5. Disable the route through instance A.
6. Instance B rejects that model within 5 seconds.
7. Create a new BrighTO client API key through instance A.
8. Instance B accepts that key within 5 seconds.
9. Insert many usage ledger rows and confirm config reload still stays fast.
10. Confirm request latency does not change when PostgreSQL is briefly slow, as long as the last config snapshot is fresh.

## README wording

Document it plainly:

> BrighTO-Router stores configuration in PostgreSQL and reloads it into memory every 5 seconds. Requests do not query PostgreSQL for routing or key checks. In Kubernetes, every pod polls the same PostgreSQL database, so config changes normally reach all pods within 5 seconds. This keeps the data path fast and avoids Redis in the default deployment.

Also document:

> Multi-pod quotas are fast per-pod controls in the open-source version. For strict global quotas across many pods, use one replica, sticky routing, or the future Enterprise quota backend.

## Final recommendation

Keep PostgreSQL as the only required production dependency.

Fix the current reload bug by removing usage-ledger summary from the 5-second config load.

Do not add Redis unless strict global quota across many pods becomes a paid/enterprise requirement.
