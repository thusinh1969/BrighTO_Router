#!/usr/bin/env python3
"""Live OpenAI-compatible Model Group smoke: DeepSeek V4 Pro + local llama.cpp.

This is the 1.0 example the repo should show to the world:
one public OpenAI chat model name routes to two compatible backends:
  1. DeepSeek V4 Pro, authenticated with DEEPSEEK_API_KEY
  2. local llama.cpp qwen3.8-flash-next, keyless auth_mode=none

It starts temporary PostgreSQL and a temporary router on localhost. It never touches
the user's running Docker/router database and never stops local llama.cpp.
"""
from __future__ import annotations

import argparse
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

REPO = pathlib.Path(__file__).resolve().parents[2]
BIN = REPO / "target/release/brighto-router"
ADMIN_KEY = "model-group-live-admin"
DEFAULT_LOCAL_BASE = "http://127.0.0.1:8088/v1"
DEFAULT_DEEPSEEK_BASE = "https://api.deepseek.com"
DEFAULT_PUBLIC_MODEL = "coding-fast-live"
DEFAULT_DEEPSEEK_MODEL = "deepseek-v4-pro"
DEFAULT_LOCAL_MODEL = "qwen3.8-flash-next"


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
    print("Building release router binary for live model-group smoke...")
    subprocess.run(["cargo", "build", "--release", "--locked"], cwd=REPO, check=True, env=env)


def free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def sh(*args: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=REPO, check=True, capture_output=True, text=True, env=env)


def admin_headers() -> dict[str, str]:
    return {"x-admin-key": ADMIN_KEY, "content-type": "application/json"}


def check(checks: list[bool], name: str, ok: bool, detail: Any = "") -> None:
    checks.append(ok)
    print(("PASS " if ok else "FAIL ") + name + (" " + str(detail) if detail else ""))


def wait_http(url: str, timeout_s: float = 60.0) -> bool:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        try:
            if requests.get(url, timeout=1).status_code < 500:
                return True
        except Exception:
            pass
        time.sleep(0.5)
    return False


def migrate(db: str, env: dict[str, str]) -> None:
    migrate_env = dict(env, DATABASE_URL=db, PGPASSWORD="brighto_router_dev")
    if subprocess.run(["sqlx", "--version"], capture_output=True).returncode == 0:
        sh("sqlx", "migrate", "run", "--source", str(REPO / "migrations"), env=migrate_env)
    else:
        for migration in sorted((REPO / "migrations").glob("*.sql")):
            sh("psql", db, "-v", "ON_ERROR_STOP=1", "-f", str(migration), env=migrate_env)


def create_backend(base: str, name: str, base_url: str) -> int:
    r = requests.post(
        base + "/admin/backends",
        headers=admin_headers(),
        json={"name": name, "base_url": base_url, "api_key_ref": "env:NONE", "format": "openai", "enabled": True},
        timeout=30,
    )
    r.raise_for_status()
    return int(r.json()["id"])


def test_connection(base: str, base_url: str, model: str, auth_mode: str, key: str = "") -> requests.Response:
    payload: dict[str, Any] = {
        "base_url": base_url,
        "dialect": "openai",
        "auth_mode": auth_mode,
        "provider_model_name": model,
        "protocol": "openai_chat",
    }
    if key:
        payload["provider_key"] = key
    return requests.post(base + "/admin/test-connection", headers=admin_headers(), json=payload, timeout=180)


def create_client_key(base: str) -> str:
    r = requests.post(
        base + "/admin/keys",
        headers=admin_headers(),
        json={"team_id": 1, "owner": "model-group-live-smoke", "allowed_models": []},
        timeout=30,
    )
    r.raise_for_status()
    return r.json()["key"]


def create_model_group(
    base: str,
    public_model: str,
    deepseek_backend_id: int,
    local_backend_id: int,
    deepseek_model: str,
    local_model: str,
    deepseek_key: str,
    policy: str,
) -> requests.Response:
    return requests.post(
        base + "/admin/routes",
        headers=admin_headers(),
        json={
            "model_name": public_model,
            "backend_ids": [deepseek_backend_id, local_backend_id],
            "provider_model_name": public_model,
            "protocol": "openai_chat",
            "auth_mode": "bearer",
            "routing_policy": policy,
            "enabled": True,
            "first_byte_timeout": 300,
            "endpoints": [
                {
                    "backend_id": deepseek_backend_id,
                    "provider_model_name": deepseek_model,
                    "provider_key": deepseek_key,
                    "auth_mode": "bearer",
                    "protocol": "openai_chat",
                    "weight": 1,
                    "max_inflight": 32,
                    "enabled": True,
                },
                {
                    "backend_id": local_backend_id,
                    "provider_model_name": local_model,
                    "auth_mode": "none",
                    "protocol": "local_openai_chat",
                    "weight": 1,
                    "max_inflight": 4,
                    "enabled": True,
                },
            ],
        },
        timeout=30,
    )


def chat_once(base: str, key: str, model: str, timeout: int) -> requests.Response:
    return requests.post(
        base + "/v1/chat/completions",
        headers={"authorization": "Bearer " + key, "content-type": "application/json"},
        json={
            "model": model,
            "messages": [{"role": "user", "content": "Reply with OK only."}],
            "max_tokens": 6,
            "stream": False,
        },
        timeout=timeout,
    )


def main() -> int:
    env = merged_env()
    parser = argparse.ArgumentParser(description="Live DeepSeek + local llama.cpp OpenAI chat Model Group smoke")
    parser.add_argument("--local-base", default=env.get("LOCAL_OPENAI_BASE_URL") or env.get("LLAMA_BASE_URL") or DEFAULT_LOCAL_BASE)
    parser.add_argument("--deepseek-base", default=env.get("DEEPSEEK_BASE_URL") or DEFAULT_DEEPSEEK_BASE)
    parser.add_argument("--deepseek-model", default=env.get("DEEPSEEK_MODEL") or DEFAULT_DEEPSEEK_MODEL)
    parser.add_argument("--local-model", default=env.get("LOCAL_OPENAI_MODEL") or env.get("LLAMA_MODEL") or DEFAULT_LOCAL_MODEL)
    parser.add_argument("--public-model", default=env.get("BRIGHTO_MODEL_GROUP") or DEFAULT_PUBLIC_MODEL)
    parser.add_argument("--policy", choices=["round_robin", "weighted_round_robin"], default="round_robin")
    parser.add_argument("--timeout", type=int, default=360, help="Timeout seconds for each public chat call")
    args = parser.parse_args()

    deepseek_key = (env.get("DEEPSEEK_API_KEY") or env.get("DEEPSEEK_KEY") or "").strip()
    if not deepseek_key:
        print("SKIP missing DEEPSEEK_API_KEY or DEEPSEEK_KEY", file=sys.stderr)
        return 2

    ensure_binary(env)

    local_health = args.local_base.rstrip("/") + "/models"
    if not wait_http(local_health, 5):
        print(f"SKIP local llama.cpp is not reachable at {local_health}", file=sys.stderr)
        return 2

    pg = f"brighto_model_group_live_{os.getpid()}"
    pg_port = free_port()
    router_port = free_port()
    data_dir = tempfile.mkdtemp(prefix="brighto-model-group-live-data-")
    log_path = os.path.join(tempfile.gettempdir(), f"brighto-model-group-live-{os.getpid()}.log")
    base = f"http://127.0.0.1:{router_port}"
    router = None
    checks: list[bool] = []

    sh(
        "docker", "run", "--rm", "-d", "--name", pg,
        "-e", "POSTGRES_DB=brighto_router",
        "-e", "POSTGRES_USER=brighto_router",
        "-e", "POSTGRES_PASSWORD=brighto_router_dev",
        "-p", f"127.0.0.1:{pg_port}:5432",
        "postgres:16-alpine",
    )
    db = f"postgres://brighto_router:brighto_router_dev@127.0.0.1:{pg_port}/brighto_router"

    try:
        for _ in range(60):
            if subprocess.run(["docker", "exec", pg, "pg_isready", "-U", "brighto_router", "-d", "brighto_router"], capture_output=True).returncode == 0:
                break
            time.sleep(1)
        migrate(db, env)
        sh(
            "psql", db, "-q", "-c",
            "INSERT INTO teams (id,name,budget,enabled) VALUES (1,'Model Group Live Smoke',NULL,TRUE);",
            env=dict(env, PGPASSWORD="brighto_router_dev"),
        )

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
                    ROUTE_COUNTER_BLOCK_SIZE="1",
                ),
                stdout=logf,
                stderr=subprocess.STDOUT,
                text=True,
            )
        if not wait_http(base + "/healthz", 60):
            raise RuntimeError("router did not become healthy")

        did = create_backend(base, "DeepSeek V4 Pro", args.deepseek_base)
        lid = create_backend(base, "Local llama.cpp Qwen", args.local_base)
        check(checks, "create backends", did > 0 and lid > 0, f"deepseek={did} local={lid}")

        ds_test = test_connection(base, args.deepseek_base, args.deepseek_model, "bearer", deepseek_key)
        ds_ok = ds_test.status_code == 200 and ds_test.json().get("ok") is True
        check(checks, "test connection DeepSeek V4 Pro", ds_ok, ds_test.text[:180] if not ds_ok else ds_test.json().get("detail"))

        local_test = test_connection(base, args.local_base, args.local_model, "none")
        local_ok = local_test.status_code == 200 and local_test.json().get("ok") is True
        check(checks, "test connection local llama.cpp", local_ok, local_test.text[:180] if not local_ok else local_test.json().get("detail"))

        route = create_model_group(base, args.public_model, did, lid, args.deepseek_model, args.local_model, deepseek_key, args.policy)
        route_ok = route.status_code == 200 and route.json().get("routing_policy") == args.policy and len(route.json().get("endpoints") or []) == 2
        check(checks, f"create OpenAI chat model group {args.public_model}", route_ok, route.text[:220] if not route_ok else "2 endpoints")

        client_key = create_client_key(base)
        seen: list[str] = []
        for i in range(2):
            r = chat_once(base, client_key, args.public_model, args.timeout)
            backend = r.headers.get("x-router-backend", "")
            seen.append(backend)
            ok = r.status_code == 200 and bool((r.json().get("choices") or [])) and backend in {"DeepSeek V4 Pro", "Local llama.cpp Qwen"}
            check(checks, f"public chat call {i + 1} via {backend or '<missing backend header>'}", ok, r.text[:220] if not ok else f"overhead_ms={r.headers.get('x-router-overhead-ms')}")

        expected = ["DeepSeek V4 Pro", "Local llama.cpp Qwen"] if args.policy == "round_robin" else ["DeepSeek V4 Pro", "Local llama.cpp Qwen"]
        check(checks, "round-robin backend order", seen == expected, f"seen={seen} expected={expected}")

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
