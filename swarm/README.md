# Swarm workspace

This folder contains build history and audit records from the agent-assisted implementation of BrighTO-Router. Runtime source, Docker Compose, Kubernetes manifests, migrations, tests, and benchmark entrypoints live at the repository root.

Keep this folder limited to durable engineering records:

- `audits/` — concrete audit cards and apply-ready patches.
- `docs_builds/` — early architecture/build notes kept for traceability.
- `scripts/` — agent/dev helper scripts that are not part of production runtime.

Do not place production source, generated benchmark output, local secrets, temporary logs, or backup copies here.
