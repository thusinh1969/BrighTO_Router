#!/usr/bin/env python3
"""Generate deterministic chat/completions payloads for benchmark runs.

Token count is approximate: 4 ASCII characters ~= 1 token. Real token counts come
from upstream usage fields when the router records ledger entries.
"""
import json
import pathlib
import sys

model = sys.argv[1] if len(sys.argv) > 1 else "mock-model"
out = pathlib.Path(__file__).parent / "payloads"
out.mkdir(exist_ok=True)
# Coding-agent payloads simulate large repository context used by vibe-coding tools:
# file paths, source snippets, diffs, logs, test failures, and precise change requests.
# The router is content-agnostic, but this shape is closer to real large-context coding traffic
# than repeating natural-language paragraphs.
para = (
    "<repo-context>\n"
    "path: crates/router/src/gateway.rs\n"
    "fn route_chat(req: ChatRequest, state: Arc<AppState>) -> Result<Response, RouterError> {\n"
    "    let model = state.registry.resolve(&req.model)?;\n"
    "    let backend = state.policy.pick_backend(&model, &req.team_id)?;\n"
    "    let started = Instant::now();\n"
    "    let upstream = backend.client.forward(req).await?;\n"
    "    state.ledger.record_tokens(req.team_id, model.name, upstream.usage.clone()).await?;\n"
    "    Ok(upstream.into_response(started.elapsed()))\n"
    "}\n"
    "path: portal/src/admin/ModelRoutes.tsx\n"
    "export function ModelRouteEditor({ providers, route, onSave }) {\n"
    "  const [selectedProvider, setSelectedProvider] = useState(route?.providerId ?? '');\n"
    "  const [modelName, setModelName] = useState(route?.modelName ?? '');\n"
    "  async function loadModels() {\n"
    "    const res = await api.post('/admin/providers/' + selectedProvider + '/models');\n"
    "    setModelOptions(await res.json());\n"
    "  }\n"
    "  return <form onSubmit={onSave}>/* polished admin workflow */</form>;\n"
    "}\n"
    "diff --git a/src/config.rs b/src/config.rs\n"
    "+ reload_interval_ms = env('CONFIG_RELOAD_MS').unwrap_or(5000)\n"
    "- reload_interval_ms = 60000\n"
    "test failure: provider_delete_rejects_routes ... FAILED because active model route exists\n"
    "request: keep architecture simple, PostgreSQL only, no Redis unless production evidence requires it.\n"
    "request: preserve OpenAI-compatible chat/completions, embeddings, rerank, and streaming behavior.\n"
    "</repo-context>\n"
)

PAYLOADS = [
    ("1k", 1_000),
    ("50k", 50_000),
    ("200k", 200_000),
    ("500k", 500_000),
    ("1m", 1_000_000),
]

for name, tokens in PAYLOADS:
    text = (para * (tokens * 4 // len(para) + 1))[: tokens * 4]
    for stream in (False, True):
        body = {
            "model": model,
            "stream": stream,
            "max_tokens": 64,
            "temperature": 0,
            "messages": [
                {"role": "system", "content": "Summarize in one sentence."},
                {"role": "user", "content": text},
            ],
        }
        if stream:
            body["stream_options"] = {"include_usage": True}
        path = out / f"{name}{'-stream' if stream else ''}.json"
        path.write_text(json.dumps(body, ensure_ascii=True))
        print(f"{path.name:18} {path.stat().st_size / 1024:9.0f} KB")
