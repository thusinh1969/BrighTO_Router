#!/usr/bin/env python3
"""End-to-end preview-2 adapter smoke through BrighTO-Router.

Starts temporary Postgres + router, creates adapter routes through Admin API, runs
/admin/test-connection, saves enabled routes, and calls the public BrighTO endpoints.
Skips providers whose API key is missing. Never prints provider keys.
"""
from __future__ import annotations

import json
import os
import pathlib
import socket
import subprocess
import sys
import tempfile
import time
from typing import Any

import requests

REPO = pathlib.Path(__file__).resolve().parent.parent
BIN = REPO / "target/release/brighto-router"
ADMIN_KEY = "adapter-smoke-admin"
ASR_FIXTURE = REPO / "tests/fixtures/asr_smoke.wav"


def parse_env_file(path: pathlib.Path) -> dict[str, str]:
    vals: dict[str, str] = {}
    if not path.exists():
        return vals
    for raw in path.read_text(errors="ignore").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, v = line.split("=", 1)
        v = v.strip()
        if len(v) >= 2 and v[0] == v[-1] and v[0] in "'\"":
            v = v[1:-1]
        vals[k.strip()] = v
    return vals


def merged_env() -> dict[str, str]:
    env = dict(os.environ)
    for k, v in parse_env_file(REPO / ".env").items():
        env.setdefault(k, v)
    return env


def free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def sh(*args: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, check=True, capture_output=True, text=True, env=env)


def source_newer_than_binary(binary: pathlib.Path) -> bool:
    if not binary.exists():
        return True
    cutoff = binary.stat().st_mtime
    roots = [REPO / "src", REPO / "migrations", REPO / "static", REPO / "Cargo.toml", REPO / "Cargo.lock"]
    for root in roots:
        if root.is_file():
            if root.stat().st_mtime > cutoff:
                return True
            continue
        if root.exists():
            for item in root.rglob("*"):
                if item.is_file() and item.stat().st_mtime > cutoff:
                    return True
    return False


def ensure_binary(env: dict[str, str]) -> None:
    if os.environ.get("BRIGHTO_SKIP_RELEASE_BUILD") == "1" and BIN.exists():
        return
    if not source_newer_than_binary(BIN):
        return
    print("Building release router binary for smoke test...")
    subprocess.run(["cargo", "build", "--release", "--locked"], cwd=REPO, check=True, env=env)


def admin_headers() -> dict[str, str]:
    return {"x-admin-key": ADMIN_KEY, "content-type": "application/json"}


def check(name: str, ok: bool, detail: Any = "") -> bool:
    print(("PASS " if ok else "FAIL ") + name + (" " + str(detail) if detail else ""))
    return ok


RATE_LIMITED_KEY_ENVS = {"VOYAGE_API_KEY": 21.0}
_last_provider_call: dict[str, float] = {}


def throttle_provider(key_env: str) -> None:
    delay = RATE_LIMITED_KEY_ENVS.get(key_env)
    if not delay:
        return
    now = time.monotonic()
    previous = _last_provider_call.get(key_env)
    if previous is not None:
        wait = delay - (now - previous)
        if wait > 0:
            print(f"WAIT {key_env} {wait:.1f}s to respect free-trial rate limit")
            time.sleep(wait)
    _last_provider_call[key_env] = time.monotonic()


def is_provider_rate_limited(resp: requests.Response, key_env: str) -> bool:
    if key_env not in RATE_LIMITED_KEY_ENVS or resp.status_code != 429:
        return False
    text = resp.text.lower()
    return "rate" in text or "reduced rate limits" in text or "too many" in text


PROBES = [
    {
        "name": "openai-chat",
        "key_env": "OPENAI_API_KEY",
        "base_url": "https://api.openai.com",
        "protocol": "openai_chat",
        "model_env": "OPENAI_CHAT_MODEL",
        "model": "gpt-4o-mini",
        "endpoint": "/v1/chat/completions",
        "body": {"model": "openai-chat", "messages": [{"role": "user", "content": "Reply OK."}], "max_tokens": 8, "stream": False},
        "kind": "chat",
    },
    {
        "name": "openai-embedding",
        "key_env": "OPENAI_API_KEY",
        "base_url": "https://api.openai.com",
        "protocol": "openai_embeddings",
        "model": "text-embedding-3-small",
        "endpoint": "/v1/embeddings",
        "body": {"model": "openai-embedding", "input": "BrighTO router smoke"},
        "kind": "embedding",
    },
    {
        "name": "openai-asr",
        "key_env": "OPENAI_API_KEY",
        "base_url": "https://api.openai.com",
        "protocol": "openai_audio_transcriptions",
        "model": "whisper-1",
        "endpoint": "/v1/audio/transcriptions",
        "kind": "asr",
    },
    {
        "name": "qwen-embedding",
        "key_env": "QWEN_API_KEY",
        "key_env_alt": "DASHSCOPE_API_KEY",
        "base_url": "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        "protocol": "openai_embeddings",
        "model": "qwen3.7-text-embedding",
        "endpoint": "/v1/embeddings",
        "body": {"model": "qwen-embedding", "input": "BrighTO router smoke"},
        "kind": "embedding",
        "optional": True,
    },
    {
        "name": "qwen-rerank",
        "key_env": "QWEN_API_KEY",
        "key_env_alt": "DASHSCOPE_API_KEY",
        "base_url_env": "QWEN_RERANK_BASE_URL",
        "protocol": "qwen_rerank",
        "model": "qwen3-rerank",
        "endpoint": "/v1/rerank",
        "kind": "rerank",
        "optional": True,
    },
    {
        "name": "jina-embedding",
        "key_env": "JINA_API_KEY",
        "base_url": "https://api.jina.ai",
        "protocol": "openai_embeddings",
        "model": "jina-embeddings-v5-text-small",
        "endpoint": "/v1/embeddings",
        "body": {"model": "jina-embedding", "input": "BrighTO router smoke", "normalized": True, "embedding_type": "float"},
        "kind": "embedding",
    },
    {
        "name": "jina-rerank",
        "key_env": "JINA_API_KEY",
        "base_url": "https://api.jina.ai",
        "protocol": "jina_rerank",
        "model": "jina-reranker-v3.5",
        "endpoint": "/v1/rerank",
        "kind": "rerank",
    },
    {
        "name": "voyage-embedding",
        "key_env": "VOYAGE_API_KEY",
        "base_url": "https://api.voyageai.com",
        "protocol": "openai_embeddings",
        "model": "voyage-4-large",
        "endpoint": "/v1/embeddings",
        "body": {"model": "voyage-embedding", "input": "BrighTO router smoke", "input_type": "document"},
        "kind": "embedding",
    },
    {
        "name": "voyage-rerank",
        "key_env": "VOYAGE_API_KEY",
        "base_url": "https://api.voyageai.com",
        "protocol": "voyage_rerank",
        "model": "rerank-2.5-lite",
        "endpoint": "/v1/rerank",
        "kind": "rerank",
    },
    {
        "name": "cohere-rerank",
        "key_env": "COHERE_API_KEY",
        "base_url": "https://api.cohere.com/v2",
        "protocol": "cohere_rerank",
        "model": "rerank-v3.5",
        "endpoint": "/v1/rerank",
        "kind": "rerank",
    },
]


def rerank_body(model_name: str) -> dict[str, Any]:
    return {
        "model": model_name,
        "query": "fast Rust AI router",
        "documents": [
            "BrighTO-Router is a fast Rust gateway for model routing.",
            "Bananas are yellow and unrelated to routers.",
            "Rerankers score documents against a query.",
        ],
        "top_n": 2,
    }


def validate_public_response(kind: str, resp: requests.Response) -> bool:
    if resp.status_code >= 400:
        return False
    data = resp.json()
    if kind == "chat":
        return bool(data.get("choices"))
    if kind == "embedding":
        emb = (((data.get("data") or [{}])[0]).get("embedding"))
        return isinstance(emb, list) and len(emb) > 0
    if kind == "rerank":
        results = data.get("results") or data.get("data") or ((data.get("output") or {}).get("results")) or []
        first = results[0] if results and isinstance(results[0], dict) else {}
        return first.get("index") is not None and ("relevance_score" in first or "score" in first)
    if kind == "asr":
        return bool((data.get("text") or "").strip())
    return False


def main() -> int:
    env = merged_env()
    ensure_binary(env)
    pg = f"brighto_adapter_smoke_{os.getpid()}"
    pg_port = free_port()
    router_port = free_port()
    data_dir = tempfile.mkdtemp(prefix="brighto-adapter-data-")
    log_path = os.path.join(tempfile.gettempdir(), f"brighto-adapter-smoke-{os.getpid()}.log")
    sh(
        "docker", "run", "--rm", "-d", "--name", pg,
        "-e", "POSTGRES_DB=brighto_router",
        "-e", "POSTGRES_USER=brighto_router",
        "-e", "POSTGRES_PASSWORD=brighto_router_dev",
        "-p", f"127.0.0.1:{pg_port}:5432",
        "postgres:16-alpine",
    )
    db = f"postgres://brighto_router:brighto_router_dev@127.0.0.1:{pg_port}/brighto_router"
    router = None
    base = f"http://127.0.0.1:{router_port}"
    checks: list[bool] = []
    try:
        for _ in range(60):
            r = subprocess.run(["docker", "exec", pg, "pg_isready", "-U", "brighto_router", "-d", "brighto_router"], capture_output=True)
            if r.returncode == 0:
                break
            time.sleep(1)
        migrate_env = dict(env, DATABASE_URL=db, PGPASSWORD="brighto_router_dev")
        if subprocess.run(["sqlx", "--version"], capture_output=True).returncode == 0:
            sh("sqlx", "migrate", "run", "--source", str(REPO / "migrations"), env=migrate_env)
        else:
            for migration in sorted((REPO / "migrations").glob("*.sql")):
                sh("psql", db, "-v", "ON_ERROR_STOP=1", "-f", str(migration), env=migrate_env)
        sh("psql", db, "-q", "-c", "INSERT INTO teams (id,name,budget,enabled) VALUES (1,'Adapter Smoke',NULL,TRUE);", env=migrate_env)
        with open(log_path, "w") as logf:
            router = subprocess.Popen(
                [str(BIN)],
                cwd=REPO,
                env=dict(
                    env,
                    DATABASE_URL=db,
                    LISTEN_ADDR=f"127.0.0.1:{router_port}",
                    TLS_CERT_PATH="",
                    TLS_KEY_PATH="",
                    ADMIN_MASTER_KEY=ADMIN_KEY,
                    DATA_DIR=data_dir,
                    RUST_LOG="warn",
                ),
                stdout=logf,
                stderr=subprocess.STDOUT,
                text=True,
            )
        for _ in range(120):
            try:
                if requests.get(base + "/healthz", timeout=1).status_code == 200:
                    break
            except Exception:
                pass
            time.sleep(0.5)
        ah = admin_headers()
        r = requests.post(base + "/admin/keys", headers=ah, json={"team_id": 1, "owner": "adapter-smoke", "allowed_models": []}, timeout=30)
        r.raise_for_status()
        client_key = r.json()["key"]
        ch = {"authorization": "Bearer " + client_key}
        ran = 0
        for probe in PROBES:
            key_env = probe["key_env"]
            key_env_alt = probe.get("key_env_alt")
            chosen_key_env = key_env if env.get(key_env, "").strip() else (key_env_alt if key_env_alt and env.get(key_env_alt, "").strip() else "")
            if not chosen_key_env:
                print(f"SKIP {probe['name']}: missing {key_env}" + (f" or {key_env_alt}" if key_env_alt else ""))
                continue
            probe_base_url = env.get(probe.get("base_url_env", ""), "").strip() if probe.get("base_url_env") else probe.get("base_url", "")
            if not probe_base_url:
                print(f"SKIP {probe['name']}: missing {probe['base_url_env']}")
                continue
            ran += 1
            backend = requests.post(base + "/admin/backends", headers=ah, json={
                "name": probe["name"],
                "base_url": probe_base_url,
                "api_key_ref": "env:NONE",
                "format": "openai",
                "enabled": True,
            }, timeout=30)
            checks.append(check(f"create backend {probe['name']}", backend.status_code == 200, backend.status_code))
            backend_id = backend.json()["id"]
            provider_model = env.get(probe.get("model_env", ""), "").strip() or probe["model"]
            test_payload = {
                "base_url": probe_base_url,
                "dialect": "openai",
                "auth_mode": "bearer",
                "provider_key_ref": "env:" + chosen_key_env,
                "provider_model_name": provider_model,
                "protocol": probe["protocol"],
            }
            throttle_provider(chosen_key_env)
            tested = requests.post(base + "/admin/test-connection", headers=ah, json=test_payload, timeout=90)
            ok_test = tested.status_code == 200 and tested.json().get("ok") is True
            rate_limited_test = is_provider_rate_limited(tested, chosen_key_env)
            checks.append(check(
                f"test connection {probe['name']}",
                ok_test or rate_limited_test,
                "RATE-LIMIT provider free-trial" if rate_limited_test else (tested.text[:180] if not ok_test else tested.json().get("detail")),
            ))
            route = requests.post(base + "/admin/routes", headers=ah, json={
                "model_name": probe["name"],
                "backend_ids": [backend_id],
                "provider_model_name": provider_model,
                "provider_key_ref": "env:" + chosen_key_env,
                "auth_mode": "bearer",
                "protocol": probe["protocol"],
                "enabled": True,
                "first_byte_timeout": 180,
            }, timeout=30)
            checks.append(check(f"save route {probe['name']}", route.status_code == 200, route.status_code))
            throttle_provider(chosen_key_env)
            if probe["kind"] == "asr":
                with ASR_FIXTURE.open("rb") as f:
                    public = requests.post(base + probe["endpoint"], headers=ch, files={"file": ("asr_smoke.wav", f, "audio/wav")}, data={"model": probe["name"]}, timeout=120)
            else:
                body = rerank_body(probe["name"]) if probe["kind"] == "rerank" else dict(probe["body"], model=probe["name"])
                public = requests.post(base + probe["endpoint"], headers={**ch, "content-type": "application/json"}, json=body, timeout=120)
            public_ok = validate_public_response(probe["kind"], public)
            rate_limited_public = is_provider_rate_limited(public, chosen_key_env)
            checks.append(check(
                f"public call {probe['name']}",
                public_ok or rate_limited_public,
                "RATE-LIMIT provider free-trial" if rate_limited_public else (public.text[:180] if public.status_code >= 400 else public.status_code),
            ))
        if ran == 0:
            print("RESULT SKIP: no provider keys available")
            return 2
        ok = all(checks)
        print("RESULT " + ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    except Exception:
        try:
            print("---- router log tail ----", file=sys.stderr)
            print("\n".join(pathlib.Path(log_path).read_text(errors="ignore").splitlines()[-80:]), file=sys.stderr)
        except OSError:
            pass
        raise
    finally:
        if router is not None:
            router.terminate()
            try:
                router.wait(timeout=5)
            except subprocess.TimeoutExpired:
                router.kill()
        subprocess.run(["docker", "rm", "-f", pg], capture_output=True)


if __name__ == "__main__":
    raise SystemExit(main())
