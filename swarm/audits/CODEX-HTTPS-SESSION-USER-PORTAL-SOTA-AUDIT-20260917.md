# CODEX AUDIT — HTTPS PEM, refresh session, and User Portal purpose

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

Current Portal/runtime is not SOTA yet for production UX.

Three fixes are required in one coherent pass:

1. Optional HTTPS must work when the installer is given custom PEM certificate/key files.
2. Browser refresh must not force a new login.
3. User Portal must have a clear purpose and must not look like a broken Admin Portal.

## 1. HTTPS with custom PEM files

### Product rule

Default install remains simple HTTP on `LISTEN_ADDR`, because many production users terminate TLS at a load balancer, ingress, Cloudflare, Nginx, or Caddy.

But if the user provides PEM files during setup, BrighTO-Router must serve HTTPS directly from the Rust binary. Do not add a default Caddy/nginx service just for this; that is extra operational surface for the default path.

### Required installer behavior

Support this flow:

```bash
./start.sh install --https --tls-cert ./fullchain.pem --tls-key ./privkey.pem
./start.sh restart
```

or equivalent flags:

```bash
./start.sh tls --cert ./fullchain.pem --key ./privkey.pem
```

The script must:

1. Validate both files exist.
2. Validate cert/key are readable.
3. Copy them into a gitignored runtime secret directory, for example:
   - `.secrets/tls/fullchain.pem`
   - `.secrets/tls/privkey.pem`
4. `chmod 600` key file where host permissions allow it.
5. Set `.env`:
   - `TLS_CERT_PATH=/certs/fullchain.pem`
   - `TLS_KEY_PATH=/certs/privkey.pem`
   - `BASE_URL=https://<host>:<port>` if host is known, otherwise show the exact URL the admin should open.
6. Restart/recreate router.

`docker-compose.yml` should mount certs read-only:

```yaml
volumes:
  - router-data:/var/lib/brighto-router
  - ./.secrets/tls:/certs:ro
```

This mount can exist even when TLS files are absent; the binary only enables HTTPS if both env vars are present.

### Required runtime behavior

At boot:

- If neither `TLS_CERT_PATH` nor `TLS_KEY_PATH` is set: serve HTTP exactly as today.
- If both are set: load PEM cert/key and serve HTTPS on `LISTEN_ADDR`.
- If only one is set: fail fast with a clear error.
- If files are invalid: fail fast with a clear error.
- Log protocol safely:
  - `brighto-router listening protocol=http addr=...`
  - `brighto-router listening protocol=https addr=...`

Implementation direction:

- Keep one port/listener.
- Use Rustls in the binary. The repo already avoids OpenSSL and uses Rustls/AWS-LC for outbound HTTP. Do not introduce native-tls/OpenSSL.
- If using `axum-server`, enable its Rustls support if compatible with the current dependency set. Otherwise add the smallest `tokio-rustls`/`rustls-pemfile` path.
- Healthcheck must understand HTTP vs HTTPS. `healthcheck` currently builds `http://{LISTEN_ADDR}/healthz`; it must use HTTPS when TLS is enabled.

### Acceptance tests

1. No TLS env: router starts HTTP and `/healthz` succeeds over HTTP.
2. Only cert env set: boot fails with clear TLS config error.
3. Only key env set: boot fails with clear TLS config error.
4. Bad PEM: boot fails with clear parse error.
5. Valid self-signed/custom PEM: router starts HTTPS and `/healthz` succeeds with `curl -k`.
6. HTTP request to HTTPS listener fails as expected; README explains URL must be `https://`.
7. Docker Compose install mounts certs read-only and restart keeps HTTPS.

## 2. F5 refresh must keep login

### Root cause

Current `static/index.html` stores auth only in JavaScript memory:

```js
var mode = "admin";
var authKey = "";
```

After F5/browser refresh, the whole page reloads and `authKey` becomes empty. The app correctly shows the login screen again because it has no persisted session.

### Product rule

F5 must keep the user inside the Portal when the saved credential is still valid.

Use browser `localStorage` with a simple expiry. Do not add server sessions, cookies, Redis, or a session table for the open-source default. This portal is a single-page admin/team utility, and every API request is already authenticated by admin key or BrighTO client key.

### Required frontend behavior

On successful login, save:

```json
{
  "mode": "admin" | "user",
  "key": "...",
  "saved_at": 1234567890,
  "expires_at": 1234567890
}
```

Suggested TTL:

- Admin: 12 hours default.
- User: 7 days default.
- Optional “Remember this browser” checkbox can extend user session to 30 days, but do not add this unless UI stays clean.

On page load:

1. Read saved session.
2. If missing/expired: show login.
3. If admin: call `GET /admin/settings` or `/admin/backends` to verify.
4. If user: call `GET /portal/me` to verify.
5. If verify passes: restore app view and refresh data.
6. If verify fails: clear saved session and show login with a clear message.

On logout:

- Clear memory `authKey`.
- Remove saved session from `localStorage`.
- Return to login screen.

### Security note

This stores a bearer credential in the browser, which is normal for a simple self-hosted admin portal but must be explicit in `SECURITY.md`.

If a stricter enterprise deployment wants central browser sessions, that belongs to Enterprise: SSO/OIDC/SAML, server sessions, audit logs, and configurable idle timeout.

Open-source default should stay simple.

### Acceptance tests

Playwright must verify:

1. Admin login succeeds.
2. Browser reload keeps Admin Portal open.
3. Admin API call after reload succeeds.
4. Logout returns to login.
5. Browser reload after logout stays logged out.
6. User login with BrighTO client key succeeds.
7. Browser reload keeps User Portal open.
8. Expired saved session is cleared and login is shown.

## 3. What is User Portal for?

### Current confusion

The UI has Admin/User mode on the same login screen. User login asks for a BrighTO client API key and then hides admin menus. This is technically workable, but the product purpose is under-explained.

The user does not log in to register an account or paste provider API keys. That would be wrong.

### Correct product definition

Admin Portal is for operators:

- configure provider templates;
- create model routes;
- enter provider credentials inside route setup;
- create teams;
- issue BrighTO client API keys;
- set budgets/limits/expiry;
- view all usage and health.

User Portal is for a team/app owner who already has a BrighTO client API key:

- see their team name and key identity;
- see allowed public models;
- copy example curl/OpenAI SDK config using BrighTO-Router endpoint;
- view their own usage, cost estimate, rate limits, budget, and expiry;
- confirm whether a key is still enabled;
- never configure provider credentials;
- never create providers/routes/teams;
- never see other teams' usage.

### Should users self-register?

No for open-source default.

Do not build account registration now. It adds password reset, email verification, spam/abuse handling, user tables, invitation flows, and more support burden. That is over-engineering for this repo today.

Enterprise can later add:

- SSO/OIDC/SAML login;
- SCIM/team provisioning;
- invite flow;
- per-user audit trail;
- role-based access control;
- approval workflow for route/provider changes.

### Recommended login UI

Keep one URL, but make entry clear:

- Tab 1: Admin
  - Username: `admin`
  - Password: admin password from `.env`
- Tab 2: Team / Developer
  - BrighTO API key: `lc-...`
  - Helper text: “Use the API key your admin created for your team. This is not your OpenAI/DeepSeek provider key.”

Do not ask User Portal for provider keys. Users only use BrighTO client keys.

### Required User Dashboard sections

After user login, show these sections:

1. My key
   - prefix/full key if user entered it in this browser session;
   - owner label;
   - expiry;
   - enabled/disabled status.

2. My team
   - team name;
   - budget remaining;
   - RPM/concurrency limits if set.

3. Allowed models
   - list public model names user can call;
   - provider names hidden unless admin chooses to expose them;
   - context/max output/pricing if available.

4. How to use
   - Base URL.
   - Copyable curl command.
   - Copyable OpenAI-compatible SDK snippet.
   - One short note: use the BrighTO key as `Authorization: Bearer lc-...`.

5. My usage
   - tokens and estimated cost by day/model;
   - last requests table;
   - errors only for this key/team.

### Acceptance tests

1. User login with a BrighTO client key shows only user sections.
2. User cannot see Providers, Routes, Teams, all API Keys, Settings.
3. User cannot call `/admin/*` endpoints.
4. User usage endpoints only show own team/key rows.
5. User dashboard has a copyable curl example using BrighTO base URL and BrighTO key.
6. User dashboard text explicitly says provider keys are managed by Admin in routes.

## Priority order for DeepSeek

Fix in this order:

1. Route-level credential model. This is still the biggest product-logic blocker.
2. Route edit/delete identity bugs.
3. F5 session restore.
4. User Portal purpose and dashboard copy/examples.
5. Optional HTTPS PEM setup/runtime.
6. Playwright full pass over Admin + User + reload + CRUD + local llama no-auth.

Do not spend time on visual polish until these product contracts are correct.
