# CODEX AUDIT — Product docs must be PostgreSQL-only

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

User rejected Redis language in README/product docs. Product messaging must be dead simple:

> BrighTO-Router production runtime = Rust router + PostgreSQL.

Do not mention Redis in README, INSTALL, Docker Compose comments, Portal UI, or normal operator docs.

If future enterprise strict distributed quota is discussed internally, keep it out of the open-source quick-start path. The public product should not sound like it is missing a required dependency.

## Changes made by Codex

Removed Redis references from:

- `README.md`
- `docker-compose.yml`
- `Cargo.toml` comments

## Required from DeepSeek

When updating docs/UI, keep this language:

- PostgreSQL stores config and usage.
- Router reloads config into memory every 5 seconds.
- Request path does not query PostgreSQL.
- No extra dependency is required for production default.

Do not add Redis prompts, Redis env vars, Redis UI cards, or Redis roadmap bullets.
