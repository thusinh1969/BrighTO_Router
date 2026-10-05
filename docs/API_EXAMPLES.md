# Python SDK and API examples

[brighto.py](../brighto.py) is the single-file Python client for BrighTO-Router. Copy it beside your application. It requires Python 3.9+ and `requests`; the router itself does not need Python.

```bash
python3 -m pip install requests
export BRIGHTO_ROUTER_URL="http://127.0.0.1:18080"
export BRIGHTO_API_KEY="<client-api-key-from-the-portal>"
```

```python
from brighto import Router

with Router(model="<public-chat-route-or-group>") as client:
    print(client.ask("Reply OK in one short sentence."))
```

Use the **API model name** from the Portal. A Model Group works like a route: the server handles balancing and fallback. Choose the method matching the route's task. The SDK does not convert protocol shapes or turn chat models into embedding models.

The constructor accepts `base_url`, `api_key`, `model`, `timeout`, and `verify`. Environment defaults are `BRIGHTO_ROUTER_URL` (or `BRIGHTO_BASE_URL`), `BRIGHTO_API_KEY`, and `BRIGHTO_MODEL`. Without a URL, it uses `http://127.0.0.1:18080`; there is no default model. The SDK reads environment variables when constructed; only the command-line helper automatically reads `.env`.

The examples below use `from brighto import Router` and the environment settings above.

URLs can use the router root or end in `/v1`. A repeated trailing `/v1/v1` is normalized. Supply the router URL rather than a full endpoint such as `/v1/chat/completions`.

## Method reference

| Portal task | SDK method | Client endpoint | Return value |
|---|---|---|---|
| Chat Completions | `ask(text)` / `chat(messages)` | `/v1/chat/completions` | `Reply` |
| Chat Completions, streaming | `stream(text_or_messages)` | `/v1/chat/completions` | Iterator of text chunks |
| Completions | `complete(prompt)` | `/v1/completions` | `Reply` |
| Responses API | `respond(input)` | `/v1/responses` | `Reply` |
| Anthropic Messages | `messages(messages)` | `/v1/messages` | `Reply` |
| Embedding | `embed(text_or_list)` | `/v1/embeddings` | Float vector or list of vectors |
| Rerank | `rerank(query, documents)` | `/v1/rerank` | Full JSON dictionary |
| ASR / transcription | `transcribe(file_path)` | `/v1/audio/transcriptions` | Text |
| ASR / transcription, detailed result | `transcription(file_path)` | `/v1/audio/transcriptions` | Full JSON dictionary |
| System One / Decision | `systemone(state, questions)` | `/v1/systemone` | Full JSON dictionary |
| Decision alias | `decisions(state, questions)` | `/v1/decisions` | Full JSON dictionary |
| Model discovery | `models()` | `/v1/models` | List of public model names |

Every inference method accepts `model=` to override the default, and extra keyword options are forwarded to the backend. `Reply` exposes `.text` / `.content`, `.reasoning`, `.tool_calls`, `.usage`, `.finish_reason`, and `.raw`. Printing it prints the text; `.raw` retains complete JSON, including additional provider fields. Your application executes returned tool calls.

For full JSON, use `client.post("/embeddings", input="hello", model="<embedding-route>")`. For streamed JSON, use `events()`. Reuse a client to reuse connections; the `with` block closes it.

## Chat and conversation history

```python
with Router(model="<public-chat-route-or-group>") as client:
    reply = client.ask("What is 2 + 2?", max_tokens=32)
    print(reply.text, reply.usage)
    history = [
        {"role": "user", "content": "My project is called BrighTO."},
        {"role": "assistant", "content": "Understood."},
        {"role": "user", "content": "What is the project called?"},
    ]
    print(client.chat(history, system="Keep answers brief."))
```

Your application keeps history. The SDK forwards messages and preserves answer whitespace. Supply `tools=[...]` and `tool_choice="auto"` for backends supporting function calls, then inspect `reply.tool_calls`. Chat defaults do not inject Qwen settings. Explicit `thinking=True` / `False` forwards `chat_template_kwargs.enable_thinking` to compatible backends. `reasoning_effort="medium"` is a separate top-level option; supported values depend on the provider.

## Streaming

```python
with Router(model="<public-chat-route-or-group>") as client:
    for chunk in client.stream("Explain this project in one sentence."):
        print(chunk, end="", flush=True)
    print()
```

Chunks are text fragments, not necessarily single tokens. `stream()` also accepts a message list. For tools, reasoning, usage, or other streaming API shapes, read complete JSON events:

```python
with Router() as client:
    events = client.events(
        "/messages", model="<public-anthropic-route>",
        messages=[{"role": "user", "content": "Reply OK."}], max_tokens=64,
    )
    try:
        for event in events:
            print(event)
    finally:
        events.close()
```

`events()` reads Server-Sent Events (SSE), the HTTP format used for streamed responses. Consume an iterator completely or close it when stopping early. Malformed events and upstream errors raise `RouterError`. Streaming requires backend support.

## Completions, Responses, and Anthropic

```python
with Router() as client:
    print(client.complete("Reply OK.", model="<public-completions-route>", max_tokens=32))
    print(client.respond("Reply OK.", model="<public-responses-route>", max_tokens=32))
    print(client.messages(
        [{"role": "user", "content": "Reply OK."}],
        model="<public-anthropic-route>", max_tokens=32,
    ))
```

Each method calls its own API shape. `respond()` accepts structured input as well as text. Responses function calls and Anthropic tool-use blocks retain their provider format in `.tool_calls` and `.raw`.

## Embeddings

```python
with Router() as client:
    vector = client.embed("A fast Rust AI gateway.", model="<public-embedding-route>")
    print("dimensions:", len(vector))
    vectors = client.embed(["first document", "second document"], model="<public-embedding-route>")
    print("vectors:", len(vectors))
```

`embed()` returns float vectors in input order, using each item's `index` when present. For usage, base64 vectors, token-array inputs, or provider-specific multimodal embeddings, use `post()` with that provider's documented JSON shape.

## Rerank

```python
with Router() as client:
    result = client.rerank(
        "fast Rust LLM gateway",
        ["BrighTO is a Rust AI gateway.", "Bananas are yellow.", "Rerankers score documents."],
        model="<public-rerank-route>", top_n=2,
    )
    print(result)
```

The router handles supported provider request adapters. The SDK returns complete JSON. Result fields differ by provider; a common shape is `results`, with `index` and `relevance_score` for each document.

## Audio transcription

```python
with Router() as client:
    print(client.transcribe("sample.wav", model="<public-asr-route>", language="en"))
    details = client.transcription("sample.wav", model="<public-asr-route>", response_format="verbose_json")
    print(details)
```

Audio is uploaded as multipart. `transcribe()` returns text; `transcription()` retains metadata such as segments. For providers supporting `text`, `srt`, or `vtt`, `transcribe()` returns that text unchanged and `transcription()` wraps it under `text`. Supported formats depend on the provider.

## Multimodal chat

```python
import base64
from pathlib import Path

image = base64.b64encode(Path("photo.jpg").read_bytes()).decode("ascii")
with Router() as client:
    print(client.chat(
        [{"role": "user", "content": [
            {"type": "text", "text": "Describe this image."},
            {"type": "image_url", "image_url": {"url": f"data:image/jpeg;base64,{image}"}},
        ]}], model="<public-vision-chat-route>",
    ))
```

Audio blocks work the same way when the chat backend accepts `input_audio`. The SDK forwards message content; the router does not resize, transcode, or store media. The backend determines accepted inputs.

## System One decisions

```python
with Router() as client:
    result = client.systemone(
        state={"message": "Please close my card; I lost it yesterday."},
        questions={
            "intent": {
                "type": "choice", "instructions": "What does the customer want?",
                "criteria": {"cancel": "close the card", "limit": "change the limit", "other": None},
            },
            "urgent": {"type": "noul", "instructions": "The request is urgent."},
        },
        model="<public-systemone-route-or-group>",
    )
    print(result["answers"])
```

`choice` selects among named options; `noul` is the probability that a statement is true; `score` uses an ordered scale. `client.decisions(...)` accepts the same arguments and calls `/v1/decisions`. Responses retain probabilities, usage, and warnings.

## Quyet System One backend

[Quyết by Chinh Nguyen](https://github.com/ncchinh/quyet) is an independent open-source decision-model runtime. Its `predict(state, questions)` response follows the TypeSafe/System One contract. Upstream supplies Python inference; the optional [quyet_server.py](../smoke/systemone/quyet_server.py) example exposes HTTP for BrighTO. It is separate from the Rust router and is not bundled into the router Docker image.

Use a separate Python 3.10+ environment:

```bash
python3 -m venv .venv-quyet
. .venv-quyet/bin/activate
python3 -m pip install "git+https://github.com/ncchinh/quyet.git" fastapi uvicorn
python3 smoke/systemone/quyet_server.py --model chinhnc/Quyet-1.0-Small --alias quyet-small --device cuda:0 --host 127.0.0.1 --port 8090
```

The first load downloads weights. Use `--device cpu` for CPU inference. Set `QUYET_API_KEY` before starting the server for backend authentication; otherwise leave the provider key blank. The server warms the model before becoming healthy and provides `/v1/models` discovery.

In the Portal, choose **System One / Decision → Custom LLM**, enter Base URL `http://127.0.0.1:8090/v1`, provider model `quyet-small`, and a unique public API model name. If the backend is on another host, use its reachable URL. Enter its key if configured, click **Test connection**, then **Save enabled**. Call `client.systemone(..., model="<public-quyet-route>")`.

For four GPUs, serve one Small replica per GPU and group their tested routes in a System One Model Group. Small is an encoder; upstream `device_map` splitting applies to its LLM variants. Clients keep the same call and use the group's public name.

Small supports an 8,192-token encoder window, including the question. Oversized state can be truncated; inspect `warnings` or pass `strict=True` to reject truncation. This backend limit is independent of the router's million-token chat benchmarks.

## HTTPS and errors

```python
from brighto import Router, RouterError

try:
    with Router(
        base_url="https://<router-host>:18443/v1", model="<public-chat-route>",
        verify="ssl/cert.pem",  # your trusted CA or self-signed certificate
    ) as client:
        print(client.ask("Reply OK."))
except RouterError as error:
    print(error.status_code, error.request_id, str(error))
```

Certificate verification is enabled by default. Omit `verify` for publicly trusted certificates; `verify=False` is available for local self-signed testing. Invalid keys, Unicode characters in keys, and invalid URLs raise `ValueError` before a request. HTTP, network, and streaming failures raise `RouterError`; HTTP errors preserve status, diagnostic text, and request ID when available. Calls are never automatically retried by the SDK, avoiding duplicated paid work.

## Command-line testing with the same SDK

Keep `test_router.py` beside `brighto.py`. Existing options and `.env` defaults are retained:

```bash
python3 test_router.py --mode chat --model <public-chat-route> --text "Reply OK"
python3 test_router.py --mode chat --model <public-chat-group> --text "Reply OK" --stream
python3 test_router.py --mode completions --model <public-completions-route> --text "Reply OK"
python3 test_router.py --mode responses --model <public-responses-route> --text "Reply OK"
python3 test_router.py --mode messages --model <public-anthropic-route> --text "Reply OK"
python3 test_router.py --mode embeddings --model <public-embedding-route> --text "hello"
python3 test_router.py --mode rerank --model <public-rerank-route> --query "router speed" --document "fast gateway" --document "unrelated text" --top-n 1
python3 test_router.py --mode asr --model <public-asr-route> --file tests/fixtures/asr_smoke.wav
python3 test_router.py --mode systemone --model <public-quyet-route-or-group> --text "Please close my lost card." --question "The request is urgent."
python3 test_router.py --model <public-vision-chat-route> --text "Describe this image." --image photo.jpg
python3 test_router.py --model <public-audio-chat-route> --text "Summarize this audio." --audio sample.wav
```

Use `--router` and `--api-key` for remote deployments, `--ca ssl/cert.pem` for your own certificate, or `--insecure` for a local self-signed test. `--raw-response` prints full JSON; `--dry-run` shows the request without sending it. Provider presets are name shortcuts for routes already created; `--list-presets` lists them. Provider credentials are never substituted for a BrighTO client key.
