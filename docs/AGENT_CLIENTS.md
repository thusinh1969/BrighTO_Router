# Agent clients

Use your existing agent CLI with a BrighTO client API key and a public model route or Model Group name. The router forwards the selected native API protocol, including streaming, tool calls, and tool results. It can also translate Anthropic Messages to OpenAI Chat for a route that explicitly enables the Claude CLI bridge. A BrighTO SDK is optional. Agent software is not bundled into the router image.

## Choose the matching task

| Client configuration | Portal task | Request endpoint | Client base URL |
|---|---|---|---|
| Claude Code with an OpenAI-compatible model | Chat + **Also accept Anthropic Messages (Claude CLI)** | `/v1/messages` | `http://llm-host.local:18080` |
| Claude Code with a native Anthropic backend | Anthropic Messages | `/v1/messages` | `http://llm-host.local:18080` |
| Codex | OpenAI Responses | `/v1/responses` | `http://llm-host.local:18080/v1` |
| Hermes, custom provider in Chat Completions mode | Chat | `/v1/chat/completions` | `http://llm-host.local:18080/v1` |
| OpenClaw, `openai-completions` provider mode | Chat | `/v1/chat/completions` | `http://llm-host.local:18080/v1` |

OpenClaw's `openai-completions` mode here means Chat Completions, not the legacy `/v1/completions` API.

In **Models & Routes → Add model route**, select the task, provider, URL, provider model, and provider key if required. Custom endpoints can use a blank provider key. Test connection and save enabled. For a Model Group, select existing tested routes of the same compatible protocol.

For an OpenAI-compatible model that should also serve Claude Code, select the **Chat** task and enable **Also accept Anthropic Messages (Claude CLI)**. The same public model name then serves `/v1/chat/completions` normally and `/v1/messages` through translation. This is the only protocol bridge in BrighTO; other routes remain protocol pass-through.

The **API model name** is what the client sends. It may differ from the provider's model name. Use a client API key from **API Keys**, never the admin key or a provider key. Base URLs may include `/v1`; the router avoids duplicating that suffix when contacting upstreams.

The upstream must implement the selected protocol and support the tools your agent uses. Chat support does not imply Responses support. BrighTO does not automatically convert protocols except through the explicit Chat route toggle above, and it does not repair malformed upstream streams. A connection test proves one basic inference request; the tool smoke below checks a real multi-turn agent workflow.

## Claude Code

Claude Code speaks Anthropic Messages. You can serve it in either of two ways:

1. **Native Anthropic Messages route** — select the **Anthropic Messages** task and an Anthropic-compatible backend. The request is pass-through.
2. **OpenAI-compatible Chat route + bridge** — select the **Chat** task, use an OpenAI-compatible backend, and enable **Also accept Anthropic Messages (Claude CLI)**. BrighTO translates the request, streaming response, tools, images, usage, and errors. The same public model still serves `/v1/chat/completions`.

```bash
export ANTHROPIC_BASE_URL=http://llm-host.local:18080
export ANTHROPIC_API_KEY='<brighto-client-api-key>'
export ANTHROPIC_MODEL='<public-model>'
export ANTHROPIC_DEFAULT_SONNET_MODEL='<public-model>'
export ANTHROPIC_DEFAULT_HAIKU_MODEL='<public-model>'
export ANTHROPIC_DEFAULT_OPUS_MODEL='<public-model>'
export ANTHROPIC_DEFAULT_FABLE_MODEL='<public-model>'
# Optional: register the custom name for the /model picker.
export ANTHROPIC_CUSTOM_MODEL_OPTION='<public-model>'
claude --model '<public-model>'
```

Claude Code sends the configured model string unchanged. On a custom `ANTHROPIC_BASE_URL`, an unknown name prints `[claude-code:unrecognized_model]` on stderr but the request still reaches BrighTO; that diagnostic is not a bridge failure. Setting `ANTHROPIC_CUSTOM_MODEL_OPTION` is optional and registers the name for Claude Code's model picker. A recognized Claude model ID such as `claude-sonnet-4-5` also works; the provider model behind the route can still be any OpenAI-compatible model. BrighTO handles the standard Claude Code tool schemas. If a Claude Code build emits optional tool-search fields that a gateway does not support, set `ENABLE_TOOL_SEARCH=false`.

The bridge strips `anthropic-*` headers and Anthropic-only query flags before calling the OpenAI upstream. The upstream must support function calling for Claude Code tools; image input, thinking output, and provider-specific extensions are best-effort. Anthropic `thinking` request configuration is not forwarded. Prompt caching / `cache_control` is ignored. Thinking signatures are opaque placeholders and thinking history is not sent back upstream.

`/v1/messages/count_tokens` is available for both native Anthropic routes and bridged Chat routes. BrighTO answers it locally with an estimate; it does not call the provider or consume budget.

Enable the bridge on a Chat **Model Group** only when every selected route is an OpenAI Chat route. The group then accepts `/v1/messages` and rewrites each endpoint's provider model. Native Messages streaming and tool results still require a compatible backend. See [Claude gateway documentation](https://code.claude.com/docs/en/llm-gateway-connect).

### Local Qwen example

A local llama.cpp/vLLM Qwen server can be exposed the same way:

- Portal task **Chat**;
- Base URL, for example, `http://llm-host.local:8088/v1`;
- Provider model `qwen3.8-flash-next`;
- API model name `qwen3.8-flash-next`;
- enable **Also accept Anthropic Messages (Claude CLI)**;
- Test connection and save enabled.

Then point Claude Code at the same route:

```bash
export ANTHROPIC_BASE_URL=http://llm-host.local:18080
export ANTHROPIC_API_KEY='<brighto-client-api-key>'
export ANTHROPIC_MODEL='qwen3.8-flash-next'
export ANTHROPIC_DEFAULT_HAIKU_MODEL='qwen3.8-flash-next'
export ANTHROPIC_DEFAULT_SONNET_MODEL='qwen3.8-flash-next'
export ANTHROPIC_DEFAULT_OPUS_MODEL='qwen3.8-flash-next'
claude --model 'qwen3.8-flash-next'
```

The same public `qwen3.8-flash-next` name continues to serve `/v1/chat/completions`. Tool use depends on the local model/build; the local Qwen 3.8 llama.cpp server used for the 2026-10-27 check completed real `Read` tool round trips through both Claude Code and direct Anthropic requests. Strict chat templates that require system text first are supported; BrighTO collects Anthropic system messages into one leading system message.

## Isolated Qwen 3.8 UAT

The repository includes a separate `uat/` stack with exactly one public model:
local **Qwen 3.8 Flash Next**, served through both OpenAI Chat Completions and
Anthropic Messages. It uses project `brighto-uat`; the router listens on
`0.0.0.0:18081` with the repository's self-signed certificate, while
PostgreSQL (`55433`) stays on loopback. It has its own volumes and local image
tag and never references production ports, database, or image. Use
`https://<host>:18081` and trust `uat/tls/fullchain.pem` (or accept the local
certificate warning).

```bash
./uat/start.sh up
./uat/start.sh test      # Qwen Anthropic text/stream/tools/count_tokens + OpenAI chat
./uat/start.sh portal    # browser: one model, toggle, PATCH payload, badge, live calls
./uat/start.sh claude    # real Claude CLI text + Read tool round trip
./uat/start.sh qwen      # check_qwen + Claude CLI together
```

The seed removes every other route/backend, so the UAT cannot accidentally
test `claude-uat`, a mock model, a Claude alias, or a Model Group. The only
public model is `qwen3.8-flash-next`. See
[uat/README.md](https://github.com/thusinh1969/BrighTO_Router/blob/main/uat/README.md)
for details and limitations.

## Codex

Create an OpenAI Responses route. Add a custom provider to `~/.codex/config.toml`, keeping unrelated settings intact:

```toml
model = "<public-responses-model>"
model_provider = "brighto"

[model_providers.brighto]
name = "BrighTO-Router"
base_url = "http://llm-host.local:18080/v1"
env_key = "BRIGHTO_API_KEY"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
```

```bash
export BRIGHTO_API_KEY='<brighto-client-api-key>'
codex
```

This uses HTTP Responses streaming. BrighTO does not expose a Responses WebSocket transport. Use a backend with a complete native Responses contract, including function calls and tool-result input. A local model with only reliable Chat Completions support is not sufficient. See [Codex gateway configuration](https://learn.chatgpt.com/docs/enterprise/connect-to-a-gateway).

## Hermes

Set the custom model section in your Hermes configuration:

```yaml
model:
  provider: custom
  default: '<public-chat-model>'
  base_url: http://llm-host.local:18080/v1
  api_key: '<brighto-client-api-key>'
  api_mode: chat_completions
```

Run `hermes chat`. Keep the configuration private because it contains a client key. The chosen Chat route must support tool calling. See [Hermes model configuration](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuring-models.md).

## OpenClaw

Merge this provider and model selection into your existing OpenClaw configuration. Set context and output limits to the upstream model's actual supported limits; the values below are examples.

```json
{
  "models": {
    "mode": "merge",
    "providers": {
      "brighto": {
        "baseUrl": "http://llm-host.local:18080/v1",
        "apiKey": "${BRIGHTO_API_KEY}",
        "api": "openai-completions",
        "request": {"allowPrivateNetwork": true},
        "models": [{
          "id": "<public-chat-model>",
          "name": "Team Chat",
          "reasoning": false,
          "input": ["text"],
          "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
          "contextWindow": 32768,
          "maxTokens": 1024
        }]
      }
    }
  },
  "agents": {"defaults": {"model": {"primary": "brighto/<public-chat-model>"}}}
}
```

```bash
export BRIGHTO_API_KEY='<brighto-client-api-key>'
openclaw agent --local --agent main --message 'Reply OK in one sentence.'
```

`allowPrivateNetwork: true` opts in to a **trusted private/LAN router**; without it, the tested OpenClaw client blocks private addresses before sending a request. Omit it for public addresses. Configure `reasoning`, `input`, and model limits to match your upstream. The zero cost fields are client-side example estimates, not a claim that inference is free. See [OpenClaw custom providers](https://docs.openclaw.ai/gateway/config-tools/custom-providers).

## HTTPS

For a publicly trusted certificate, use the HTTPS base URL with normal certificate verification. For a local certificate generated by `start.sh`, copy the public `ssl/ca.pem` to the client and create the combined trust bundle described in [HTTPS setup](https://github.com/thusinh1969/BrighTO_Router/blob/main/HTTPS.md). This preserves normal public HTTPS trust alongside your local CA. Set the applicable variables:

```bash
export NODE_EXTRA_CA_CERTS=/absolute/path/to/client-ca-bundle.pem
export CODEX_CA_CERTIFICATE=/absolute/path/to/client-ca-bundle.pem
export SSL_CERT_FILE=/absolute/path/to/client-ca-bundle.pem
export REQUESTS_CA_BUNDLE=/absolute/path/to/client-ca-bundle.pem
```

Use the hostname covered by the certificate. Do not disable TLS verification. [HTTPS setup](https://github.com/thusinh1969/BrighTO_Router/blob/main/HTTPS.md) explains certificate creation and renewal.

## Repeat a real tool test

[Agent smoke instructions](https://github.com/thusinh1969/BrighTO_Router/blob/main/smoke/agents/README.md) exercise installed CLIs with a random file: the model requests a tool, the CLI reads the file, and the model returns its exact contents. Temporary configuration keeps the test separate from your normal settings. Missing CLIs, failed tools, and incomplete answers fail the test.

Verified client versions on 2026-10-05: Claude Code `2.1.289`, Codex `0.157.1`, Hermes revision `93c9360a8da592cee43e8895403e91c7a920b0ce`, and OpenClaw `2026.9.8`. Each completed the file-reading workflow over HTTP and HTTPS with certificate verification. Chat and Messages used a local llama.cpp model; Codex used native OpenAI Responses. On 2026-10-27, Claude Code `2.1.311` also completed text and Read tool_use/tool_result workflows through the explicit OpenAI Chat bridge against local Qwen 3.8 in the isolated UAT. This verifies those workflows, not every client version, model, optional agent feature, or upstream extension.

Only inference POST endpoints are exposed for Responses. `/v1/messages/count_tokens` is a local estimate. Response retrieval, deletion, compaction, and cross-provider session affinity are not implemented. Stateful requests referencing upstream response IDs require the same upstream; use a single route when that matters. Model Groups balance compatible protocols, not provider-held session state.
