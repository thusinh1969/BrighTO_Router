#!/usr/bin/env python3
"""Anthropic (Claude) route smoke: anthropic_messages protocol + x-api-key, through the router.

Run: python3 scripts/anthropic_smoke.py
Reads the Claude key from .model (never printed). Sanitized output only.
"""
import os, pathlib, re, socket, subprocess, sys, tempfile, time
import requests

REPO = pathlib.Path(__file__).resolve().parent.parent
BIN = REPO / "target/release/brighto-router"
ADMIN_KEY = "anthropic-smoke-admin"
ANTHROPIC_BASE = "https://api.anthropic.com"

def free_port():
    s = socket.socket(); s.bind(("127.0.0.1", 0)); p = s.getsockname()[1]; s.close(); return p

def sh(*a, **kw):
    return subprocess.run(a, check=True, capture_output=True, text=True, **kw)

def claude_credentials():
    """Extract claude-opus-5 (provider anthropic) api_base + api_key from .model without printing."""
    text = (REPO / ".model").read_text()
    m = re.search(r'"model_name": "claude-opus-5".*?"api_base": ([^,]*),.*?"api_key": "([^"]+)"', text, re.S)
    if not m:
        raise SystemExit("claude-opus-5 entry not found in .model")
    base = m.group(1).strip().strip('"')
    key = m.group(2)
    if base == "null" or not base:
        base = ANTHROPIC_BASE
    return base, key

def main():
    base_url, claude_key = claude_credentials()
    pg = "brighto_anthropic_smoke_%d" % os.getpid()
    pg_port = free_port(); router_port = free_port()
    data_dir = tempfile.mkdtemp(prefix="brigto-anth-data-")
    log_path = os.path.join(tempfile.gettempdir(), "brigto-anth-smoke-%d.log" % os.getpid())

    sh("docker", "run", "--rm", "-d", "--name", pg,
       "-e", "POSTGRES_DB=llm_router", "-e", "POSTGRES_USER=llm_router",
       "-e", "POSTGRES_PASSWORD=llm_router_dev", "-p", "127.0.0.1:%d:5432" % pg_port, "postgres:16-alpine")
    db = "postgres://llm_router:llm_router_dev@127.0.0.1:%d/llm_router" % pg_port
    router = None
    base = "http://127.0.0.1:%d" % router_port
    try:
        for _ in range(60):
            r = subprocess.run(["docker", "exec", pg, "pg_isready", "-U", "llm_router", "-d", "llm_router"], capture_output=True)
            if r.returncode == 0: break
            time.sleep(1)
        sh("sqlx", "migrate", "run", "--source", str(REPO / "migrations"), env=dict(os.environ, DATABASE_URL=db))
        sh("psql", "-h", "127.0.0.1", "-p", str(pg_port), "-U", "llm_router", "-d", "llm_router", "-q", "-c",
           "INSERT INTO teams (id,name,budget,enabled) VALUES (1,'Smoke',NULL,TRUE);",
           env=dict(os.environ, PGPASSWORD="llm_router_dev"))

        with open(log_path, "w") as logf:
            router = subprocess.Popen([str(BIN)],
                env=dict(os.environ, DATABASE_URL=db, LISTEN_ADDR="127.0.0.1:%d" % router_port,
                         ADMIN_MASTER_KEY=ADMIN_KEY, DATA_DIR=data_dir, RUST_LOG="info",
                         TLS_CERT_PATH="", TLS_KEY_PATH=""),
                stdout=logf, stderr=subprocess.STDOUT, text=True)
        for _ in range(120):
            try:
                if requests.get(base + "/healthz", timeout=1).status_code == 200: break
            except Exception: pass
            time.sleep(0.5)

        admin = {"x-admin-key": ADMIN_KEY, "Content-Type": "application/json"}
        checks = []
        def check(name, ok, detail=""):
            checks.append((name, ok))
            print(("PASS " if ok else "FAIL ") + name + (" " + str(detail) if detail else ""))

        # Anthropic backend (format anthropic)
        r = requests.post(base + "/admin/backends", headers=admin, json={
            "name": "Anthropic", "base_url": base_url, "api_key_ref": "env:NONE",
            "format": "anthropic", "enabled": True}, timeout=60)
        check("create anthropic backend", r.status_code == 200, r.status_code)
        bid = r.json()["id"]

        # Anthropic messages route (route-level credential)
        r = requests.post(base + "/admin/routes", headers=admin, json={
            "model_name": "claude-route", "backend_ids": [bid],
            "provider_model_name": "claude-opus-5",
            "protocol": "anthropic_messages", "auth_mode": "anthropic",
            "provider_key": claude_key, "first_byte_timeout": 300}, timeout=60)
        check("create anthropic_messages route", r.status_code == 200, r.status_code)

        r = requests.post(base + "/admin/keys", headers=admin, json={
            "team_id": 1, "owner": "anthropic-smoke", "allowed_models": []}, timeout=60)
        ck = r.json()["key"]
        uh = {"Authorization": "Bearer " + ck, "Content-Type": "application/json"}

        # Anthropic Messages call
        r = requests.post(base + "/v1/messages", headers=uh, json={
            "model": "claude-route", "max_tokens": 8,
            "messages": [{"role": "user", "content": "Say hi in 3 words"}]}, timeout=120)
        ok = r.status_code == 200 and "content" in r.json()
        check("anthropic /v1/messages chat", ok, r.status_code if not ok else "")

        # endpoint guard: anthropic route must reject /v1/chat/completions
        r = requests.post(base + "/v1/chat/completions", headers=uh, json={
            "model": "claude-route", "messages": [{"role": "user", "content": "hi"}]}, timeout=30)
        ok = r.status_code == 400 and "Anthropic Messages" in (r.text or "")
        check("endpoint guard: anthropic route rejects chat/completions", ok, r.status_code)

        fails = [n for n, ok in checks if not ok]
        print("RESULT " + ("PASS" if not fails else "FAIL: " + ", ".join(fails)))
        sys.exit(0 if not fails else 1)
    except Exception:
        try:
            with open(log_path) as f: tail = f.read().splitlines()[-40:]
            print("---- router log tail ----", file=sys.stderr)
            print("\n".join(tail), file=sys.stderr)
        except OSError: pass
        raise
    finally:
        if router is not None:
            router.terminate()
            try: router.wait(timeout=5)
            except subprocess.TimeoutExpired: router.kill()
        subprocess.run(["docker", "rm", "-f", pg], capture_output=True)

if __name__ == "__main__":
    main()
