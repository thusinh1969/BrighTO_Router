#!/usr/bin/env python3
"""Full UAT check for the single local Qwen 3.8 model.

The same public model must serve both OpenAI Chat Completions and Anthropic
Messages, including streaming, tool use/result, usage, and count_tokens.
"""

from __future__ import annotations

import json
import os
import ssl
import sys
import urllib.error
import urllib.request

BASE = os.environ.get("UAT_BASE_URL", "https://127.0.0.1:18081").rstrip("/")
KEY = os.environ.get("UAT_CLIENT_KEY", "")
MODEL = os.environ.get("UAT_QWEN_MODEL", "qwen3.8-flash-next")
CA_CERT = os.environ.get("UAT_CA_CERT", "")
SSL_CONTEXT = (
    ssl.create_default_context(cafile=CA_CERT)
    if CA_CERT and os.path.exists(CA_CERT)
    else None
)


def check(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)
    print("PASS " + message)


def post(path: str, body: dict, timeout: float = 180.0) -> tuple[int, dict, bytes]:
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        BASE + path,
        data=data,
        headers={
            "content-type": "application/json",
            "x-api-key": KEY,
            "anthropic-version": "2023-06-01",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout, context=SSL_CONTEXT) as resp:
            return resp.status, {k.lower(): v for k, v in resp.headers.items()}, resp.read()
    except urllib.error.HTTPError as error:
        return error.code, {k.lower(): v for k, v in error.headers.items()}, error.read()


def text_blocks(message: dict) -> str:
    return "".join(
        block.get("text", "")
        for block in message.get("content", [])
        if block.get("type") == "text"
    )


def main() -> None:
    if not KEY:
        raise SystemExit("UAT_CLIENT_KEY is required")

    status, headers, raw = post(
        "/v1/messages",
        {
            "model": MODEL,
            "max_tokens": 128,
            "system": "Answer briefly in plain text.",
            "messages": [{"role": "user", "content": "Reply with exactly QWEN_ANTHROPIC_OK."}],
        },
    )
    message = json.loads(raw) if raw else {}
    check(status == 200, f"Qwen /v1/messages returns 200 (got {status})")
    check(headers.get("content-type", "").startswith("application/json"), "Qwen response is JSON")
    check(message.get("type") == "message", "Qwen response is an Anthropic message")
    check(message.get("model") == MODEL, "Qwen keeps the public model name")
    check(bool(text_blocks(message).strip()), "Qwen returns non-empty text")
    check(message.get("usage", {}).get("input_tokens", 0) > 0, "Qwen usage input_tokens present")
    print("Qwen Anthropic text: " + text_blocks(message).strip()[:300])

    status, _, raw = post(
        "/v1/messages",
        {
            "model": MODEL,
            "max_tokens": 128,
            "stream": True,
            "messages": [{"role": "user", "content": "Reply with exactly QWEN_STREAM_OK."}],
        },
    )
    check(status == 200, f"Qwen streamed /v1/messages returns 200 (got {status})")
    stream = raw.decode(errors="replace")
    stream_text = []
    for line in stream.splitlines():
        if not line.startswith("data: "):
            continue
        try:
            event = json.loads(line[6:])
        except json.JSONDecodeError:
            continue
        delta = event.get("delta") if isinstance(event, dict) else None
        if isinstance(delta, dict) and delta.get("type") == "text_delta":
            stream_text.append(delta.get("text", ""))
    check("event: message_start" in stream, "Qwen stream has message_start")
    check("event: message_stop" in stream, "Qwen stream has message_stop")
    check("".join(stream_text).strip() != "", "Qwen stream emits text deltas")
    check("data: [DONE]" not in stream, "OpenAI [DONE] is not forwarded")

    tools = [
        {
            "name": "Read",
            "description": "Read a text file",
            "input_schema": {
                "type": "object",
                "properties": {"file_path": {"type": "string"}},
                "required": ["file_path"],
            },
        }
    ]
    status, _, raw = post(
        "/v1/messages",
        {
            "model": MODEL,
            "max_tokens": 256,
            "tools": tools,
            "tool_choice": {"type": "tool", "name": "Read"},
            "messages": [{"role": "user", "content": "Read the file uat.txt."}],
        },
    )
    check(status == 200, f"Qwen tool-selection request returns 200 (got {status})")
    first = json.loads(raw) if raw else {}
    tool = next((block for block in first.get("content", []) if block.get("type") == "tool_use"), None)
    check(tool is not None, "Qwen returns a tool_use block")
    check(tool.get("name") == "Read", "Qwen tool_use name is Read")
    check(isinstance(tool.get("input"), dict), "Qwen tool_use input is an object")
    check(first.get("stop_reason") == "tool_use", "Qwen tool_use stop_reason")

    status, _, raw = post(
        "/v1/messages",
        {
            "model": MODEL,
            "max_tokens": 256,
            "messages": [
                {"role": "user", "content": "Read the file uat.txt."},
                {"role": "assistant", "content": [tool]},
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": tool["id"],
                            "content": "claude-bridge-uat",
                        }
                    ],
                },
            ],
        },
    )
    check(status == 200, f"Qwen tool-result request returns 200 (got {status})")
    final = json.loads(raw) if raw else {}
    check(bool(text_blocks(final).strip()), "Qwen returns text after tool_result")

    status, _, raw = post(
        "/v1/chat/completions",
        {
            "model": MODEL,
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "Reply with exactly QWEN_CHAT_OK."}],
        },
    )
    chat = json.loads(raw) if raw else {}
    check(status == 200, f"Qwen /v1/chat/completions returns 200 (got {status})")
    check(bool(chat.get("choices")), "Qwen Chat Completions returns choices")

    status, _, raw = post(
        "/v1/messages/count_tokens",
        {
            "model": MODEL,
            "system": "Answer briefly in plain text.",
            "messages": [{"role": "user", "content": "Count these tokens please"}],
        },
    )
    count = json.loads(raw) if raw else {}
    check(status == 200, f"Qwen count_tokens returns 200 (got {status})")
    check(count.get("input_tokens", 0) > 0, "Qwen count_tokens returns an estimate")
    print("QWEN 3.8 DUAL-PROTOCOL UAT PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as error:
        print("FAIL " + str(error), file=sys.stderr)
        sys.exit(1)
    except urllib.error.URLError as error:
        print(f"FAIL Qwen endpoint unreachable through UAT router: {error}", file=sys.stderr)
        sys.exit(1)
