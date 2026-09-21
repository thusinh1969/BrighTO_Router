#!/usr/bin/env python3
"""Deterministic BrighTO-Router API matrix smoke.

Covers the public API families without paid provider keys:
  - OpenAI-compatible chat:        /v1/chat/completions
  - Anthropic Messages:            /v1/messages
  - Embeddings:                    /v1/embeddings
  - Rerank:                        /v1/rerank
  - ASR/transcription multipart:   /v1/audio/transcriptions

It starts temporary PostgreSQL, brighto-router, and brighto-router-mock on free
localhost ports. It never touches the user's dev database or live Docker runtime.
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
ROUTER_BIN = REPO / "target/release/brighto-router"
MOCK_BIN = REPO / "target/release/brighto-router-mock"
ADMIN_KEY = "api-matrix-admin"
ASR_FIXTURE = REPO / "tests/fixtures/asr_smoke.wav"


def free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def sh(*args: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=REPO, check=True, capture_output=True, text=True, env=env)


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


def ensure_binaries(env: dict[str, str]) -> None:
    if os.environ.get("BRIGHTO_SKIP_RELEASE_BUILD") == "1" and ROUTER_BIN.exists() and MOCK_BIN.exists():
        return
    stale = []
    if source_newer_than_binary(ROUTER_BIN):
        stale.append("brighto-router")
    if source_newer_than_binary(MOCK_BIN):
        stale.append("brighto-router-mock")
    if stale:
        print("Building release binaries for API matrix smoke: " + ", ".join(stale))
        subprocess.run(["cargo", "build", "--release", "--locked"], cwd=REPO, check=True, env=env)


def migrate(db: str, env: dict[str, str]) -> None:
    migrate_env = dict(env, DATABASE_URL=db, PGPASSWORD="brighto_router_dev")
    if subprocess.run(["sqlx", "--version"], capture_output=True).returncode == 0:
        sh("sqlx", "migrate", "run", "--source", str(REPO / "migrations"), env=migrate_env)
    else:
        for migration in sorted((REPO / "migrations").glob("*.sql")):
            sh("psql", db, "-v", "ON_ERROR_STOP=1", "-f", str(migration), env=migrate_env)


def admin_headers() -> dict[str, str]:
    return {"x-admin-key": ADMIN_KEY, "content-type": "application/json"}


def check(checks: list[bool], name: str, ok: bool, detail: Any = "") -> None:
    checks.append(ok)
    print(("PASS " if ok else "FAIL ") + name + (" " + str(detail) if detail else ""))


def create_backend(base: str, name: str, base_url: str, fmt: str) -> int:
    r = requests.post(
        base + "/admin/backends",
        headers=admin_headers(),
        json={"name": name, "base_url": base_url, "api_key_ref": "env:NONE", "format": fmt, "enabled": True},
        timeout=30,
    )
    r.raise_for_status()
    return int(r.json()["id"])


def create_route(base: str, model_name: str, backend_id: int, provider_model: str, protocol: str, auth_mode: str) -> None:
    r = requests.post(
        base + "/admin/routes",
        headers=admin_headers(),
        json={
            "model_name": model_name,
            "backend_ids": [backend_id],
            "provider_model_name": provider_model,
            "protocol": protocol,
            "auth_mode": auth_mode,
            "enabled": True,
            "first_byte_timeout": 30,
        },
        timeout=30,
    )
    r.raise_for_status()


def test_connection(base: str, base_url: str, dialect: str, model: str, protocol: str, auth_mode: str) -> requests.Response:
    return requests.post(
        base + "/admin/test-connection",
        headers=admin_headers(),
        json={
            "base_url": base_url,
            "dialect": dialect,
            "auth_mode": auth_mode,
            "provider_model_name": model,
            "protocol": protocol,
        },
        timeout=30,
    )


def client_key(base: str) -> str:
    r = requests.post(
        base + "/admin/keys",
        headers=admin_headers(),
        json={"team_id": 1, "owner": "api-matrix-smoke", "allowed_models": []},
        timeout=30,
    )
    r.raise_for_status()
    return r.json()["key"]


def validate(kind: str, response: requests.Response) -> bool:
    if response.status_code >= 400:
        return False
    data = response.json()
    if kind == "chat":
        return bool(data.get("choices"))
    if kind == "messages":
        return bool(data.get("content"))
    if kind == "embeddings":
        embedding = (((data.get("data") or [{}])[0]).get("embedding"))
        return isinstance(embedding, list) and len(embedding) > 0
    if kind == "rerank":
        results = data.get("results") or data.get("data") or []
        first = results[0] if results and isinstance(results[0], dict) else {}
        return first.get("index") is not None and ("relevance_score" in first or "score" in first)
    if kind == "asr":
        return bool((data.get("text") or "").strip())
    return False


def main() -> int:
    env = dict(os.environ)
    ensure_binaries(env)
    if not ASR_FIXTURE.exists():
        print(f"Missing ASR fixture: {ASR_FIXTURE}", file=sys.stderr)
        return 2

    pg = f"brighto_api_matrix_{os.getpid()}"
    pg_port = free_port()
    router_port = free_port()
    mock_port = free_port()
    data_dir = tempfile.mkdtemp(prefix="brighto-api-matrix-data-")
    router_log = os.path.join(tempfile.gettempdir(), f"brighto-api-matrix-router-{os.getpid()}.log")
    mock_log = os.path.join(tempfile.gettempdir(), f"brighto-api-matrix-mock-{os.getpid()}.log")

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
    mock = None
    checks: list[bool] = []
    base = f"http://127.0.0.1:{router_port}"
    mock_base = f"http://127.0.0.1:{mock_port}"

    try:
        for _ in range(60):
            if subprocess.run(["docker", "exec", pg, "pg_isready", "-U", "brighto_router", "-d", "brighto_router"], capture_output=True).returncode == 0:
                break
            time.sleep(1)
        migrate(db, env)
        sh(
            "psql", db, "-q", "-c",
            "INSERT INTO teams (id,name,budget,enabled) VALUES (1,'API Matrix Smoke',NULL,TRUE);",
            env=dict(env, PGPASSWORD="brighto_router_dev"),
        )

        with open(mock_log, "w") as logf:
            mock = subprocess.Popen(
                [str(MOCK_BIN)],
                cwd=REPO,
                env=dict(env, MOCK_ADDR=f"127.0.0.1:{mock_port}"),
                stdout=logf,
                stderr=subprocess.STDOUT,
                text=True,
            )
        for _ in range(120):
            try:
                if requests.get(mock_base + "/health", timeout=1).status_code == 200:
                    break
            except Exception:
                pass
            time.sleep(0.25)

        with open(router_log, "w") as logf:
            router = subprocess.Popen(
                [str(ROUTER_BIN)],
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
            time.sleep(0.25)

        openai_backend = create_backend(base, "Mock OpenAI", mock_base, "openai")
        anthropic_backend = create_backend(base, "Mock Anthropic", mock_base, "anthropic")
        key = client_key(base)
        user_headers = {"authorization": "Bearer " + key, "content-type": "application/json"}

        cases = [
            {
                "name": "openai chat",
                "kind": "chat",
                "model": "matrix-openai-chat",
                "provider_model": "mock-model",
                "protocol": "openai_chat",
                "backend_id": openai_backend,
                "dialect": "openai",
                "auth_mode": "none",
                "endpoint": "/v1/chat/completions",
                "json": {"model": "matrix-openai-chat", "messages": [{"role": "user", "content": "Say OK."}], "max_tokens": 8, "stream": False},
            },
            {
                "name": "anthropic messages",
                "kind": "messages",
                "model": "matrix-anthropic",
                "provider_model": "mock-model",
                "protocol": "anthropic_messages",
                "backend_id": anthropic_backend,
                "dialect": "anthropic",
                "auth_mode": "none",
                "endpoint": "/v1/messages",
                "json": {"model": "matrix-anthropic", "max_tokens": 8, "messages": [{"role": "user", "content": "Say OK."}]},
            },
            {
                "name": "embedding",
                "kind": "embeddings",
                "model": "matrix-embedding",
                "provider_model": "mock-embedding",
                "protocol": "openai_embeddings",
                "backend_id": openai_backend,
                "dialect": "openai",
                "auth_mode": "none",
                "endpoint": "/v1/embeddings",
                "json": {"model": "matrix-embedding", "input": "BrighTO router embedding matrix smoke"},
            },
            {
                "name": "rerank",
                "kind": "rerank",
                "model": "matrix-rerank",
                "provider_model": "mock-rerank",
                "protocol": "openai_rerank",
                "backend_id": openai_backend,
                "dialect": "openai",
                "auth_mode": "none",
                "endpoint": "/v1/rerank",
                "json": {"model": "matrix-rerank", "query": "fast Rust router", "documents": ["fast AI gateway", "slow unrelated text", "router benchmark"], "top_n": 2},
            },
            {
                "name": "asr",
                "kind": "asr",
                "model": "matrix-asr",
                "provider_model": "mock-asr",
                "protocol": "openai_audio_transcriptions",
                "backend_id": openai_backend,
                "dialect": "openai",
                "auth_mode": "none",
                "endpoint": "/v1/audio/transcriptions",
            },
        ]

        for case in cases:
            create_route(base, case["model"], case["backend_id"], case["provider_model"], case["protocol"], case["auth_mode"])
            tested = test_connection(base, mock_base, case["dialect"], case["provider_model"], case["protocol"], case["auth_mode"])
            ok_test = tested.status_code == 200 and tested.json().get("ok") is True
            check(checks, f"test-connection {case['name']}", ok_test, tested.text[:180] if not ok_test else tested.json().get("detail"))

            if case["kind"] == "asr":
                with ASR_FIXTURE.open("rb") as f:
                    public = requests.post(
                        base + case["endpoint"],
                        headers={"authorization": "Bearer " + key},
                        files={"file": ("asr_smoke.wav", f, "audio/wav")},
                        data={"model": case["model"]},
                        timeout=30,
                    )
            else:
                public = requests.post(base + case["endpoint"], headers=user_headers, json=case["json"], timeout=30)
            check(checks, f"public {case['name']}", validate(case["kind"], public), public.text[:180] if public.status_code >= 400 else public.status_code)

        # One cross-endpoint guard proves route protocol cannot silently forward a wrong request shape.
        guard = requests.post(base + "/v1/embeddings", headers=user_headers, json={"model": "matrix-openai-chat", "input": "wrong endpoint"}, timeout=30)
        check(checks, "endpoint guard chat rejects embeddings", guard.status_code == 400 and "OpenAI Chat" in guard.text, guard.status_code)

        ok = all(checks)
        print("RESULT " + ("PASS" if ok else "FAIL"))
        return 0 if ok else 1
    except Exception:
        for label, path in [("router", router_log), ("mock", mock_log)]:
            try:
                print(f"---- {label} log tail ----", file=sys.stderr)
                print("\n".join(pathlib.Path(path).read_text(errors="ignore").splitlines()[-80:]), file=sys.stderr)
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
        if mock is not None:
            mock.terminate()
            try:
                mock.wait(timeout=5)
            except subprocess.TimeoutExpired:
                mock.kill()
        subprocess.run(["docker", "rm", "-f", pg], capture_output=True)


if __name__ == "__main__":
    raise SystemExit(main())
