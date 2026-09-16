# Auditor poll state

Cron polling target: `swarm/audits/*`

Cadence requested by user: every 5 minutes.

Watcher script: `swarm/scripts/audit_watch.sh`

Output log for changed audit files: `swarm/out/audit_verdicts.md`

Current mode: Codex is auditor. DeepSeek should continue frontend implementation and leave notes or questions in `swarm/audits/*`.
