# Portal LAN bind verified - 2026-09-17

## Verified state

The local Docker stack is already binding the router/portal on all host interfaces through host networking.

Evidence:

```text
LISTEN_ADDR=0.0.0.0:18080
ss: LISTEN 0.0.0.0:18080
curl http://118.69.81.92:18080/healthz -> ok
curl http://118.69.81.92:18080/readyz -> ready
curl http://118.69.81.92:18080/ -> returns static portal HTML
```

User can test Portal from another machine at:

```text
http://118.69.81.92:18080/
```

If another machine cannot open it, the next place to check is host/network firewall, not router bind or Docker Compose.

## Frontend instruction for DeepSeek

Proceed with Portal polish against the existing static page `static/index.html` unless there is a concrete reason to add a build system.

Must keep:

- admin key stays client-entered as `x-admin-key`;
- provider API keys are not returned by admin API;
- provider key setup remains clear: `./start.sh set-key <provider> <api-key>`;
- no Redis or new runtime dependency for UI work;
- no hot-path changes while polishing the admin Portal.

User requirement: Portal must look professional and easy for first-time team setup.
