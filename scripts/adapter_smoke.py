#!/usr/bin/env python3
"""Live adapter provider smoke tests.

This script calls live providers directly with tiny requests to prove API key + endpoint + model.
It never prints provider keys and skips providers whose key is missing.

Examples:
  python3 scripts/adapter_smoke.py --provider jina --task embedding
  python3 scripts/adapter_smoke.py --provider jina --task rerank
  python3 scripts/adapter_smoke.py --provider all --task all
  python3 scripts/adapter_smoke.py --provider openai --task asr --file ./sample.wav

Keep stress tests on mock providers. This file is only for low-cost live smoke tests.
"""
from __future__ import annotations

import argparse
import json
import mimetypes
import os
import ssl
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parent.parent
DEFAULT_ENV_FILE = REPO / ".env"


class SmokeError(Exception):
    pass


PROVIDERS: dict[str, dict[str, Any]] = {
    "openai": {
        "key_env": "OPENAI_API_KEY",
        "embedding_url": "https://api.openai.com/v1/embeddings",
        "embedding_model": "text-embedding-3-small",
        "asr_url": "https://api.openai.com/v1/audio/transcriptions",
        "asr_model": "whisper-1",
    },
    "qwen": {
        "key_env": "QWEN_API_KEY",
        "key_env_alt": "DASHSCOPE_API_KEY",
        "embedding_url": "https://dashscope-intl.aliyuncs.com/compatible-mode/v1/embeddings",
        "embedding_model": "qwen3.7-text-embedding",
        "rerank_base_env": "QWEN_RERANK_BASE_URL",
        "rerank_base_url": "https://dashscope-intl.aliyuncs.com",
        "rerank_path": "/compatible-api/v1/reranks",
        "rerank_model": "qwen3-rerank",
    },
    "jina": {
        "key_env": "JINA_API_KEY",
        "embedding_url": "https://api.jina.ai/v1/embeddings",
        "embedding_model": "jina-embeddings-v5-text-small",
        "rerank_url": "https://api.jina.ai/v1/rerank",
        "rerank_model": "jina-reranker-v3.5",
    },
    "voyage": {
        "key_env": "VOYAGE_API_KEY",
        "embedding_url": "https://api.voyageai.com/v1/embeddings",
        "embedding_model": "voyage-4-large",
        "rerank_url": "https://api.voyageai.com/v1/rerank",
        "rerank_model": "rerank-2.5-lite",
    },
    "cohere": {
        "key_env": "COHERE_API_KEY",
        "rerank_url": "https://api.cohere.com/v2/rerank",
        "rerank_model": "rerank-v3.5",
    },
}


def parse_env_file(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    if not path.exists():
        return values
    for raw in path.read_text(errors="ignore").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "'\"":
            value = value[1:-1]
        values[key] = value
    return values


def env_value(name: str, env_file_values: dict[str, str]) -> str:
    return os.environ.get(name) or env_file_values.get(name, "")


def request_json(url: str, key: str, body: dict[str, Any], timeout: int) -> tuple[int, dict[str, str], bytes, float]:
    data = json.dumps(body, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=data,
        method="POST",
        headers={
            "authorization": f"Bearer {key}",
            "content-type": "application/json",
            "accept": "application/json",
        },
    )
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            elapsed_ms = (time.perf_counter() - started) * 1000
            return resp.status, {k.lower(): v for k, v in resp.headers.items()}, resp.read(), elapsed_ms
    except urllib.error.HTTPError as exc:
        elapsed_ms = (time.perf_counter() - started) * 1000
        return exc.code, {k.lower(): v for k, v in exc.headers.items()}, exc.read(), elapsed_ms
    except urllib.error.URLError as exc:
        raise SmokeError(f"cannot reach provider: {exc}") from exc


def request_multipart(url: str, key: str, model: str, file_path: str, timeout: int) -> tuple[int, dict[str, str], bytes, float]:
    path = Path(file_path)
    if not path.exists() or not path.is_file():
        raise SmokeError(f"file not found: {file_path}")
    boundary = "----brighto-adapter-smoke-boundary"
    mime = mimetypes.guess_type(str(path))[0] or "application/octet-stream"
    file_bytes = path.read_bytes()
    data = b"".join(
        [
            f"--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{model}\r\n".encode("utf-8"),
            f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{path.name}\"\r\nContent-Type: {mime}\r\n\r\n".encode("utf-8"),
            file_bytes,
            f"\r\n--{boundary}--\r\n".encode("utf-8"),
        ]
    )
    req = urllib.request.Request(
        url,
        data=data,
        method="POST",
        headers={
            "authorization": f"Bearer {key}",
            "content-type": f"multipart/form-data; boundary={boundary}",
            "accept": "application/json",
        },
    )
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(req, context=ssl.create_default_context(), timeout=timeout) as resp:
            elapsed_ms = (time.perf_counter() - started) * 1000
            return resp.status, {k.lower(): v for k, v in resp.headers.items()}, resp.read(), elapsed_ms
    except urllib.error.HTTPError as exc:
        elapsed_ms = (time.perf_counter() - started) * 1000
        return exc.code, {k.lower(): v for k, v in exc.headers.items()}, exc.read(), elapsed_ms
    except urllib.error.URLError as exc:
        raise SmokeError(f"cannot reach provider: {exc}") from exc


def parse_json(raw: bytes) -> Any:
    try:
        return json.loads(raw.decode("utf-8"))
    except Exception as exc:
        raise SmokeError(f"provider returned non-JSON body: {raw[:200]!r}") from exc


def provider_url(cfg: dict[str, Any], task: str, env_file_values: dict[str, str]) -> str | None:
    direct = cfg.get(f"{task}_url")
    if direct:
        return str(direct)
    base_env = cfg.get(f"{task}_base_env")
    base = env_value(str(base_env), env_file_values).strip() if base_env else ""
    base = base or str(cfg.get(f"{task}_base_url") or "")
    path = str(cfg.get(f"{task}_path") or "")
    if not base:
        return None
    return base.rstrip("/") + "/" + path.lstrip("/")


def smoke_embedding(provider: str, cfg: dict[str, Any], key: str, model: str | None, timeout: int, env_file_values: dict[str, str]) -> bool:
    url = provider_url(cfg, "embedding", env_file_values)
    if not url:
        print(f"SKIP {provider} embedding: provider has no embedding endpoint in this smoke")
        return True
    model = model or cfg["embedding_model"]
    body: dict[str, Any] = {"model": model, "input": "BrighTO-Router adapter smoke test"}
    if provider == "voyage":
        body["input_type"] = "document"
    if provider == "jina":
        body["normalized"] = True
        body["embedding_type"] = "float"
    status, _headers, raw, elapsed_ms = request_json(url, key, body, timeout)
    data = parse_json(raw)
    ok = 200 <= status < 300 and isinstance(data.get("data"), list) and data["data"]
    emb = data.get("data", [{}])[0].get("embedding") if isinstance(data, dict) else None
    dim = len(emb) if isinstance(emb, list) else 0
    ok = ok and dim > 0
    usage = data.get("usage") if isinstance(data, dict) else None
    print(f"{'PASS' if ok else 'FAIL'} {provider} embedding model={model} status={status} elapsed_ms={elapsed_ms:.1f} dim={dim} usage={safe_json(usage)}")
    if not ok:
        print_error_body(data)
    return ok


def smoke_rerank(provider: str, cfg: dict[str, Any], key: str, model: str | None, timeout: int, env_file_values: dict[str, str]) -> bool:
    url = provider_url(cfg, "rerank", env_file_values)
    if not url:
        print(f"SKIP {provider} rerank: provider has no rerank endpoint in this smoke")
        return True
    model = model or cfg["rerank_model"]
    body: dict[str, Any] = {
        "model": model,
        "query": "fast Rust AI router",
        "documents": [
            "BrighTO-Router is an ultra-fast Rust gateway for model routing.",
            "Bananas are yellow fruit and unrelated to API gateways.",
            "Rerankers improve retrieval quality by scoring candidate documents.",
        ],
    }
    if provider in {"jina", "cohere"}:
        body["top_n"] = 2
    elif provider == "voyage":
        body["top_k"] = 2
    status, _headers, raw, elapsed_ms = request_json(url, key, body, timeout)
    data = parse_json(raw)
    results = None
    if isinstance(data, dict):
        results = data.get("results") or data.get("data")
    ok = 200 <= status < 300 and isinstance(results, list) and bool(results)
    first = results[0] if ok and isinstance(results[0], dict) else {}
    score = first.get("relevance_score", first.get("score"))
    ok = ok and first.get("index") is not None and score is not None
    usage = data.get("usage") or data.get("meta") if isinstance(data, dict) else None
    print(f"{'PASS' if ok else 'FAIL'} {provider} rerank model={model} status={status} elapsed_ms={elapsed_ms:.1f} results={len(results) if isinstance(results, list) else 0} first_index={first.get('index')} score={score} usage={safe_json(usage)}")
    if not ok:
        print_error_body(data)
    return ok


def smoke_asr(provider: str, cfg: dict[str, Any], key: str, model: str | None, file_path: str | None, timeout: int, env_file_values: dict[str, str]) -> bool:
    url = provider_url(cfg, "asr", env_file_values)
    if not url:
        print(f"SKIP {provider} asr: provider has no ASR endpoint in this smoke")
        return True
    if not file_path:
        print(f"SKIP {provider} asr: pass --file ./sample.wav to run ASR smoke")
        return True
    model = model or cfg["asr_model"]
    status, _headers, raw, elapsed_ms = request_multipart(url, key, model, file_path, timeout)
    data = parse_json(raw)
    text = data.get("text") if isinstance(data, dict) else None
    ok = 200 <= status < 300 and isinstance(text, str) and bool(text.strip())
    usage = data.get("usage") if isinstance(data, dict) else None
    print(f"{'PASS' if ok else 'FAIL'} {provider} asr model={model} status={status} elapsed_ms={elapsed_ms:.1f} text_chars={len(text or '')} usage={safe_json(usage)}")
    if not ok:
        print_error_body(data)
    return ok


def safe_json(value: Any) -> str:
    if value is None:
        return "-"
    text = json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    return text if len(text) <= 180 else text[:177] + "..."


def print_error_body(data: Any) -> None:
    if isinstance(data, dict):
        err = data.get("error") or data.get("message") or data
        print("  provider_error:", safe_json(err))
    else:
        print("  provider_error:", repr(data)[:200])


def selected(items: str, all_items: list[str]) -> list[str]:
    if items == "all":
        return all_items
    return [x.strip() for x in items.split(",") if x.strip()]


def main() -> int:
    parser = argparse.ArgumentParser(description="Run tiny live-provider smoke tests for preview-2 adapters.")
    parser.add_argument("--provider", default="all", help="Provider: openai, qwen, jina, voyage, cohere, or all. Comma-separated is allowed.")
    parser.add_argument("--task", default="all", help="Task: embedding, rerank, asr, or all. Comma-separated is allowed.")
    parser.add_argument("--model", help="Override provider model for a single provider/task run")
    parser.add_argument("--file", help="Audio file for --task asr")
    parser.add_argument("--env-file", default=str(DEFAULT_ENV_FILE), help="Env file to read provider keys from")
    parser.add_argument("--timeout", type=int, default=60, help="HTTP timeout seconds")
    args = parser.parse_args()

    providers = selected(args.provider, list(PROVIDERS))
    tasks = selected(args.task, ["embedding", "rerank", "asr"])
    bad_providers = [p for p in providers if p not in PROVIDERS]
    bad_tasks = [t for t in tasks if t not in {"embedding", "rerank", "asr"}]
    if bad_providers:
        raise SmokeError(f"unknown provider(s): {', '.join(bad_providers)}")
    if bad_tasks:
        raise SmokeError(f"unknown task(s): {', '.join(bad_tasks)}")
    if args.model and (len(providers) != 1 or len(tasks) != 1):
        raise SmokeError("--model override requires exactly one provider and one task")

    env_file_values = parse_env_file(Path(args.env_file))
    checks: list[bool] = []
    for provider in providers:
        cfg = PROVIDERS[provider]
        key_env = cfg["key_env"]
        key_env_alt = cfg.get("key_env_alt")
        key = env_value(key_env, env_file_values).strip()
        if not key and key_env_alt:
            key = env_value(str(key_env_alt), env_file_values).strip()
        if not key:
            suffix = f" or {key_env_alt}" if key_env_alt else ""
            print(f"SKIP {provider}: missing {key_env}{suffix}")
            continue
        for task in tasks:
            if task == "embedding":
                checks.append(smoke_embedding(provider, cfg, key, args.model, args.timeout, env_file_values))
            elif task == "rerank":
                checks.append(smoke_rerank(provider, cfg, key, args.model, args.timeout, env_file_values))
            elif task == "asr":
                checks.append(smoke_asr(provider, cfg, key, args.model, args.file, args.timeout, env_file_values))
    if not checks:
        print("RESULT SKIP: no runnable checks; set provider keys in environment or .env")
        return 2
    ok = all(checks)
    print("RESULT " + ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except SmokeError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(2)
    except KeyboardInterrupt:
        print("Interrupted", file=sys.stderr)
        raise SystemExit(130)
