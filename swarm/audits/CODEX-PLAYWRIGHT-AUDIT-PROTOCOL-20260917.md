# Playwright audit protocol for Portal - 2026-09-17

User requirement:

```text
Bạn khi audit lúc Deepseek xong dùng Playwright để check chạy thật nhé all steps/windows/click
```

Meaning:

When DeepSeek says Portal is done, Codex auditor must verify it with a real browser automation pass. Unit tests, curl, and backend smoke are not enough.

## Required Playwright audit scope

Run the Portal exactly like a user.

### Admin flow

1. Open `/`.
2. Confirm the first screen is login, not dashboard.
3. Try wrong admin password and verify clean error.
4. Login as:

```text
username: admin
password: ADMIN_MASTER_KEY
```

5. Verify dashboard loads.
6. Click each main navigation item:

```text
Dashboard
Providers
Models & Routes
Teams
API Keys
Usage
Settings
```

7. Providers:
   - list default providers;
   - create custom provider;
   - edit provider name and URL;
   - set or confirm API key flow;
   - test connection path if implemented;
   - load models path if a mock provider is available.
8. Models & Routes:
   - create route from one provider and one model;
   - set max tokens/context;
   - set price per 1M input tokens;
   - set price per 1M output tokens;
   - save;
   - verify route appears in route list.
9. Teams:
   - create team with unlimited budget;
   - create team with money budget;
   - create team with token budget;
   - verify table/cards update.
10. API Keys:
   - create key with expiry;
   - create key with inherited team budget;
   - create key with own money/token budget;
   - verify Admin can see/copy key again if DeepSeek implements user override;
   - verify disable/revoke works.
11. Usage:
   - seed or generate usage rows;
   - verify filters by provider, model, team, key, date;
   - verify charts/tables update.
12. Settings:
   - verify runtime status fields are visible and understandable.
13. Logout:
   - verify dashboard is hidden after logout;
   - refresh page and verify login is required again if session storage is cleared.

### User flow

1. Login/open user mode with a BrighTO client API key.
2. Verify user sees only own team/key/usage.
3. Verify bad API key shows 401/clean login error.
4. Verify user cannot access Admin screens without Admin login.

### Responsive/window checks

Run at minimum:

```text
Desktop: 1440x1000
Laptop: 1366x768
Mobile: 390x844
```

For each viewport:

- no horizontal overflow on main layout;
- primary actions visible;
- tables scroll cleanly when needed;
- login usable;
- forms usable.

## Required artifacts

Save artifacts under:

```text
swarm/out/playwright/<timestamp>/
```

Required files:

```text
summary.md
admin-dashboard.png
providers.png
model-route-create.png
teams.png
api-keys.png
usage.png
settings.png
user-portal.png
mobile-login.png
mobile-dashboard.png
trace.zip if Playwright trace is enabled
```

Do not commit `swarm/out/` artifacts unless the user asks. It is local evidence.

## Required command shape

Preferred:

```bash
npm/pnpm playwright test
```

If no Playwright project exists, create a small temporary or repo script such as:

```text
scripts/portal_playwright_audit.py or tests/playwright/portal.spec.ts
```

Use the real running app, not mocked DOM-only tests.

## Pass/fail rule

Fail the audit if any of these happen:

- login cannot be completed;
- any required screen is unreachable by click;
- forms cannot save data through the real backend;
- admin key and user API key modes are confused;
- plaintext provider keys leak unexpectedly;
- API key reveal behavior contradicts the latest user override;
- charts/tables show fake hard-coded data after real data exists;
- mobile layout blocks primary actions;
- browser console has uncaught runtime errors during the tested flows.

## Final auditor response requirement

When reporting to user, include:

```text
Playwright pass/fail
exact commit tested
Docker image digest if tested through Docker
URL tested
number of flows passed/failed
artifact folder path
any root-cause fixes required from DeepSeek
```

## User reaffirmation: real browser, all steps/windows/clicks

User instruction on 2026-09-17: when DeepSeek says Portal is done, Codex must audit with Playwright against the running app, not by reading HTML only. The audit must execute real browser interactions:

- Open the real Portal URL.
- Log in with the configured admin credential.
- Click every visible navigation item/window/panel.
- Exercise Provider creation/edit/test/load-model actions.
- Exercise Model Route creation path: provider selection, API key entry, model loading/selecting one model, max token/context fields, price per 1M input/output, save.
- Exercise Team creation/edit including finite and unlimited budget.
- Exercise API key creation/reveal/copy/disable including the user override that Admin may see client API keys again.
- Exercise Usage/Dashboard filters by provider, model, team, key, and full system where implemented.
- Exercise Settings and logout/login again.
- Repeat layout checks for desktop and mobile viewports.
- Capture screenshots, browser console errors, failed network requests, and a concise pass/fail summary under `swarm/out/playwright/<timestamp>/`.

Pass condition: every main user journey must work by clicking the UI. A green HTTP smoke test alone is not enough for Portal acceptance.

