#!/usr/bin/env python3
"""Real-provider smoke: local llama.cpp (keyless) + DeepSeek V4 Pro, through the router.

Self-contained: spins up its own temporary Postgres + router instance on free ports,
so it never touches the dev DB and never collides with a running router.

Run: DEEPSEEK_KEY=sk-... python3 scripts/real_provider_smoke.py
Never prints the DeepSeek key. Sanitized output only.
"""
import os
import pathlib
import socket
import subprocess
import sys
import tempfile
import time

import requests

REPO = pathlib.Path(__file__).resolve().parent.parent
BIN = REPO / "target/release/brighto-router"
ADMIN_KEY = "real-smoke-admin"
LLAMA_BASE = "http://127.0.0.1:8088/v1"
DEEPSEEK_BASE = "https://api.deepseek.com"


def source_newer_than_binary(binary):
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


def ensure_binary():
    if os.environ.get("BRIGHTO_SKIP_RELEASE_BUILD") == "1" and BIN.exists():
        return
    if source_newer_than_binary(BIN):
        print("Building release router binary for smoke test...")
        subprocess.run(["cargo", "build", "--release", "--locked"], cwd=REPO, check=True)


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def sh(*a, **kw):
    return subprocess.run(a, check=True, capture_output=True, text=True, **kw)


def main():
    ensure_binary()
    if "DEEPSEEK_KEY" not in os.environ:
        print("Set DEEPSEEK_KEY=sk-... to run this smoke.", file=sys.stderr)
        sys.exit(2)
    ds_key = os.environ["DEEPSEEK_KEY"]

    pg = "brighto_real_smoke_%d" % os.getpid()
    pg_port = free_port()
    router_port = free_port()
    data_dir = tempfile.mkdtemp(prefix="brigto-real-data-")
    log_path = os.path.join(tempfile.gettempdir(), "brigto-real-smoke-%d.log" % os.getpid())

    sh("docker", "run", "--rm", "-d", "--name", pg,
       "-e", "POSTGRES_DB=llm_router",
       "-e", "POSTGRES_USER=llm_router",
       "-e", "POSTGRES_PASSWORD=llm_router_dev",
       "-p", "127.0.0.1:%d:5432" % pg_port,
       "postgres:16-alpine")
    db = "postgres://llm_router:llm_router_dev@127.0.0.1:%d/llm_router" % pg_port
    router = None
    base = "http://127.0.0.1:%d" % router_port
    try:
        for _ in range(60):
            r = subprocess.run(
                ["docker", "exec", pg, "pg_isready", "-U", "llm_router", "-d", "llm_router"],
                capture_output=True,
            )
            if r.returncode == 0:
                break
            time.sleep(1)
        sh("sqlx", "migrate", "run", "--source", str(REPO / "migrations"),
           env=dict(os.environ, DATABASE_URL=db))
        sh("psql", "-h", "127.0.0.1", "-p", str(pg_port), "-U", "llm_router",
           "-d", "llm_router", "-q", "-c",
           "INSERT INTO teams (id,name,budget,enabled) VALUES (1,'Real Smoke',NULL,TRUE);",
           env=dict(os.environ, PGPASSWORD="llm_router_dev"))

        with open(log_path, "w") as logf:
            router = subprocess.Popen(
                [str(BIN)],
                env=dict(os.environ, DATABASE_URL=db,
                         LISTEN_ADDR="127.0.0.1:%d" % router_port,
                         TLS_CERT_PATH="", TLS_KEY_PATH="",  # disable TLS for the temp HTTP router
                         ADMIN_MASTER_KEY=ADMIN_KEY,
                         DATA_DIR=data_dir,
                         RUST_LOG="info"),
                stdout=logf, stderr=subprocess.STDOUT, text=True,
            )
        for _ in range(120):
            try:
                if requests.get(base + "/healthz", timeout=1).status_code == 200:
                    break
            except Exception:
                pass
            time.sleep(0.5)

        admin = {"x-admin-key": ADMIN_KEY, "Content-Type": "application/json"}
        checks = []

        def check(name, ok, detail=""):
            checks.append((name, ok))
            print(("PASS " if ok else "FAIL ") + name + (" " + str(detail) if detail else ""))

        # ---- Local llama.cpp (keyless, auth_mode=none) ----
        r = requests.post(base + "/admin/backends", headers=admin, json={
            "name": "Local Qwen", "base_url": LLAMA_BASE,
            "api_key_ref": "env:NONE", "format": "openai", "enabled": True}, timeout=60)
        check("create local backend", r.status_code == 200, r.status_code)
        lid = r.json()["id"]
        r = requests.post(base + "/admin/routes/preview-models", headers=admin, json={
            "base_url": LLAMA_BASE, "protocol": "openai",
            "auth_mode": "none", "provider_key": ""}, timeout=30)
        lm = r.json().get("models", [])
        check("local models (preview, no auth)",
              r.status_code == 200 and "qwen3.8-flash-next" in lm, lm)
        r = requests.post(base + "/admin/routes", headers=admin, json={
            "model_name": "qwen-local", "backend_ids": [lid],
            "provider_model_name": lm[0] if lm else "qwen3.8-flash-next",
            "auth_mode": "none", "first_byte_timeout": 300}, timeout=60)
        check("create local route", r.status_code == 200, r.status_code)
        r = requests.post(base + "/admin/keys", headers=admin, json={
            "team_id": 1, "owner": "real-test", "allowed_models": []}, timeout=60)
        ck = r.json()["key"]
        uh = {"Authorization": "Bearer " + ck, "Content-Type": "application/json"}
        r = requests.post(base + "/v1/chat/completions", headers=uh, json={
            "model": "qwen-local",
            "messages": [{"role": "user", "content": "Say hello in three words."}],
            "max_tokens": 16, "stream": False}, timeout=300)
        ok = r.status_code == 200 and "choices" in r.json()
        check("local llama.cpp chat", ok, r.status_code if not ok else "")

        # Protocol endpoint guard: chat route phải từ chối /v1/embeddings (không forward shape sai).
        r = requests.post(base + "/v1/embeddings", headers=uh, json={
            "model": "qwen-local", "input": "hello"}, timeout=30)
        ok = r.status_code == 400 and "OpenAI Chat Completions" in (r.text or "")
        check("endpoint guard: chat route rejects embeddings", ok, r.status_code)

        # ---- DeepSeek V4 Pro (route-level credential) ----
        r = requests.post(base + "/admin/backends", headers=admin, json={
            "name": "DeepSeek V4 Pro", "base_url": DEEPSEEK_BASE,
            "api_key_ref": "env:NONE", "format": "openai", "enabled": True}, timeout=60)
        did = r.json()["id"]
        r = requests.post(base + "/admin/routes/preview-models", headers=admin, json={
            "base_url": DEEPSEEK_BASE, "protocol": "openai",
            "auth_mode": "bearer", "provider_key": ds_key}, timeout=30)
        dm = r.json().get("models", [])
        check("deepseek models (preview)", r.status_code == 200 and "deepseek-v4-pro" in dm, dm)
        r = requests.post(base + "/admin/routes", headers=admin, json={
            "model_name": "deepseek-v4-pro", "backend_ids": [did],
            "provider_model_name": "deepseek-v4-pro",
            "provider_key": ds_key, "auth_mode": "bearer", "first_byte_timeout": 300}, timeout=60)
        check("create deepseek route (route credential)", r.status_code == 200, r.status_code)
        r = requests.post(base + "/v1/chat/completions", headers=uh, json={
            "model": "deepseek-v4-pro",
            "messages": [{"role": "user", "content": "Say hello in three words."}],
            "max_tokens": 16, "stream": False}, timeout=300)
        ok = r.status_code == 200 and "choices" in r.json()
        check("deepseek chat", ok, r.status_code if not ok else "")

        fails = [n for n, ok in checks if not ok]
        print("RESULT " + ("PASS" if not fails else "FAIL: " + ", ".join(fails)))
        sys.exit(0 if not fails else 1)
    except Exception:
        try:
            with open(log_path) as f:
                tail = f.read().splitlines()[-40:]
            print("---- router log tail ----", file=sys.stderr)
            print("\n".join(tail), file=sys.stderr)
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
    main()
