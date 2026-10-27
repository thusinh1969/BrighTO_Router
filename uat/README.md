# Qwen 3.8 dual-protocol UAT

This directory is an **isolated** stack with exactly one public model: the
local **Qwen 3.8 Flash Next** OpenAI-compatible server, exposed through both
OpenAI Chat Completions and Anthropic Messages. It exists so the bridge can be
proven without touching the production router, database, `.env`, volumes, or
ports.

| Item | UAT value |
| --- | --- |
| Compose project | `brighto-uat` |
| Portal / router | `https://127.0.0.1:18081` (also `https://<host>:18081`) |
| PostgreSQL | `127.0.0.1:55433` |
| Public model | `qwen3.8-flash-next` |
| Upstream | `http://host.docker.internal:8088/v1` |
| Local image | `thusinh1969/brighto_airouter:uat-qwen-dual` (built from v1.3.0 source, never pushed) |
| Volumes | `brighto-uat-pg`, `brighto-uat-router-data` |

Production ports (`18443`, `18080`, `55432`), the production image tag, and the
production database are not referenced here. The UAT router listens on
`0.0.0.0:18081` for host/LAN access; PostgreSQL stays on loopback. If the host
has a public interface, restrict `18081` with the host firewall; this is a test
router, not production.

## Quick start

```bash
./uat/start.sh up
./uat/start.sh status
```

`up` builds the image from the current working tree (embedded Portal included),
applies migrations to the UAT database, seeds one team/client key and the
single `qwen3.8-flash-next` route, and starts the router. It creates
`uat/.env.uat` on first run. Open `https://<host>:18081/`, log in as `admin`
with `ADMIN_MASTER_KEY` from `uat/.env.uat`; `./uat/start.sh env` prints the
local test values.

The UAT seed removes every other route, endpoint, counter, and backend on each
run. It never creates `claude-uat`, a mock model, a Model Group, or a Claude
alias.

## Single public model, both protocols

`qwen3.8-flash-next` maps to the local provider model of the same name. The
route has:

- protocol `local_openai_chat`;
- `accept_anthropic_messages = true`;
- auth `none`;
- the same public name on `/v1/chat/completions` and `/v1/messages`.

The router reaches the host through `host.docker.internal:8088` (Docker
`host-gateway`). The llama.cpp/vLLM server must listen on `0.0.0.0`, not only
`127.0.0.1`. Override in `uat/.env.uat` if needed:

```bash
UAT_QWEN_BASE_URL=http://host.docker.internal:8088/v1
UAT_QWEN_MODEL=qwen3.8-flash-next
```

## Checks

```bash
./uat/start.sh test      # Qwen: Anthropic text/stream/tools/count_tokens + OpenAI chat
./uat/start.sh portal    # Playwright: one model, toggle, exact PATCH, badge, live calls
./uat/start.sh claude    # real Claude CLI text + Read tool round trip
./uat/start.sh qwen      # check_qwen + Claude CLI in one command
```

`test` asserts that `/v1/models` exposes exactly one model. `claude` sets
`ANTHROPIC_BASE_URL`, `ANTHROPIC_API_KEY`, and all model aliases to the UAT
router, runs in `uat/work`, and allows tools only inside that directory. It
reads `uat/work/uat.txt` through the real `Read` tool, then checks the final
answer. The `[claude-code:unrecognized_model]` stderr line is expected for the
custom model name; the request still reaches the router.

### HTTPS and trust

`start.sh up` copies `../ssl/fullchain.pem` and `../ssl/privkey.pem` into
`uat/tls/`, enables TLS, and sets `UAT_BASE_URL=https://127.0.0.1:18081`. The
certificate is self-signed and covers the host name used when it was generated
(`<router-host>`), `localhost`, `127.0.0.1`, and the host IP. Browsers show a warning until
`uat/tls/fullchain.pem` (or `ssl/fullchain.pem`) is trusted. CLI checks use
`--cacert`/`UAT_CA_CERT` automatically. Set `UAT_TLS=0` in `uat/.env.uat` and
rerun `./uat/start.sh up` to force plain HTTP when no certificate exists.

## Lifecycle

```bash
./uat/start.sh logs
./uat/start.sh down       # keep UAT volumes
./uat/start.sh reset      # remove only brighto-uat containers + named volumes
./uat/start.sh env        # print UAT URLs and local keys
```

`reset` followed by `up` is the clean one-shot path: the database is rebuilt
from the same migrations as production, then seeded with Qwen only.

## What this proves

- One public model, one provider endpoint, no hidden extra models.
- OpenAI Chat and Anthropic Messages both work on the same route.
- Anthropic streaming, tool use/result, usage, errors, and `count_tokens`.
- Claude Code `2.1.311` text and Read-tool workflows against local Qwen 3.8.
- Portal shows the bridge toggle/badge and PATCHes
  `accept_anthropic_messages: true`.
- Strict Qwen chat templates are respected: Anthropic system text is collected
  into one leading system message.

## Known limitations

- `message_start.usage.input_tokens` is an estimate; final `output_tokens` comes
  from upstream usage when available.
- Prompt caching / `cache_control` is ignored.
- Thinking signatures are opaque placeholders; thinking history is not sent
  back upstream.
- Tool quality and image support depend on the local Qwen build. Qwen 3.8 Flash
  Next completed the text and Read-tool workflows in this UAT.
- The UAT is not a performance benchmark.
