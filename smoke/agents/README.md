# Native agent smoke

Test installed Claude Code, Codex, Hermes, or OpenClaw against BrighTO-Router without using `brighto.py`. Each test creates a random text file, requires a real file-reading tool call, and checks the model's final answer. Exit status is nonzero when the CLI is missing, a tool fails, or the answer is incomplete.

This optional test uses Python's standard library and your installed agent CLI. It does not install agents, start router services, create model routes, or change normal agent configuration. Temporary workspaces and client settings are removed after each run.

## Prerequisites

- A running router with enabled, tested routes and a valid BrighTO client API key.
- Chat route for Hermes/OpenClaw; Anthropic Messages route for Claude; native OpenAI Responses route for Codex.
- Upstream models that support the selected protocol and tools. Plain text-only inference is insufficient.
- The CLI you want to test on `PATH`, or its executable path supplied below.

Follow [client setup](../../docs/AGENT_CLIENTS.md) to select compatible routes.

## Run

```bash
export BRIGHTO_ROUTER_URL=http://llm-host.local:18080
export BRIGHTO_API_KEY='<brighto-client-api-key>'

python3 smoke/agents/run.py --harness hermes --chat-model '<public-chat-model>'
python3 smoke/agents/run.py --harness claude --messages-model '<public-messages-model>'
python3 smoke/agents/run.py --harness codex --responses-model '<public-responses-model>'
python3 smoke/agents/run.py --harness openclaw --chat-model '<public-chat-model>' --allow-private-network
```

Use `--allow-private-network` only for a trusted LAN/private router; it sets OpenClaw's explicit private-network permission. Other clients do not use that flag.

Run all four when installed:

```bash
python3 smoke/agents/run.py --harness all \
  --chat-model '<public-chat-model>' \
  --messages-model '<public-messages-model>' \
  --responses-model '<public-responses-model>' \
  --allow-private-network
```

For HTTPS with a local CA:

```bash
python3 smoke/agents/run.py --url https://llm-host.local:18443 \
  --ca-file /absolute/path/to/ca.pem \
  --harness codex --responses-model '<public-responses-model>'
```

Use the public CA certificate generated in `ssl/ca.pem`, not the server private key. The test combines this CA with normal system trust roots in its temporary workspace, so public HTTPS used by the harness remains trusted. Publicly trusted HTTPS does not need `--ca-file`.

`--claude-bin`, `--codex-bin`, `--hermes-bin`, and `--openclaw-bin` accept executable paths. `--context-window` and `--max-tokens` set OpenClaw's model metadata; choose limits supported by your upstream. `--timeout` defaults to 120 seconds per harness. Run `python3 smoke/agents/run.py --help` for all options.

Output is one JSON result per harness with elapsed time, tool-call count, and PASS/FAIL. Failure output is diagnostic and may include generated tool arguments; keep test logs private. The test never prints the client API key. Tests make small real inference calls through your routes, so normal provider billing applies when the upstream is a paid service.
