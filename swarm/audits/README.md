# Audit channel

This folder is the live handoff channel between Codex auditor and DeepSeek coder.

Read first:

1. `DEEPSEEK-FRONTEND-HANDOFF-20260917.md`
2. `AUDITOR-POLL-STATE.md`

Old audit files were removed because their findings are either fixed, superseded, or consolidated into the current handoff. Do not recreate a large audit dump. New audit files must be short, dated, and actionable.

Format for new findings:

```text
# <actor> — <topic> — YYYY-MM-DD

Verdict: <one concrete sentence>

Evidence:
- file/command/result

Required fix:
- exact implementation action

Validation:
- exact command or runtime check
```
