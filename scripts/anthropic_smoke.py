#!/usr/bin/env python3
"""Anthropic (Claude) route smoke: anthropic_messages protocol + x-api-key, through the router.

Run: python3 scripts/anthropic_smoke.py
Reads ANTHROPIC_API_KEY from env/.env first, then falls back to .model. Sanitized output only.
"""
import os, pathlib, re, socket, subprocess, sys, tempfile, time
import requests

REPO = pathlib.Path(__file__).resolve().parent.parent
BIN = REPO / "target/release/brighto-router"
ADMIN_KEY = "anthropic-smoke-admin"
ANTHROPIC_BASE = "https://api.anthropic.com"

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
    s = socket.socket(); s.bind(("127.0.0.1", 0)); p = s.getsockname()[1]; s.close(); return p

def sh(*a, **kw):
    return subprocess.run(a, check=True, capture_output=True, text=True, **kw)


def parse_env_file(path):
    vals = {}
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


def merged_env():
    env = dict(os.environ)
    for k, v in parse_env_file(REPO / ".env").items():
        env.setdefault(k, v)
    return env

def claude_credentials():
    """Return (base_url, api_key, model) without printing secrets."""
    env = merged_env()
    key = (env.get("ANTHROPIC_API_KEY") or env.get("CLAUDE_API_KEY") or "").strip()
    base = (env.get("ANTHROPIC_BASE_URL") or ANTHROPIC_BASE).strip()
    model = (env.get("ANTHROPIC_MODEL") or "").strip()

    model_file = REPO / ".model"
    if model_file.exists():
        text = model_file.read_text(errors="ignore")
        preferred = re.search(r'"model_name": "([^"]*claude[^"]*)".*?"api_base": ([^,]*),.*?"api_key": "([^"]+)"', text, re.S | re.I)
        if preferred:
            if not model:
                model = preferred.group(1).strip()
            if not key:
                key = preferred.group(3).strip()
            file_base = preferred.group(2).strip().strip('"')
            if (not base or base == ANTHROPIC_BASE) and file_base and file_base != "null":
                base = file_base

    if not key:
        raise SystemExit("ANTHROPIC_API_KEY not found in env/.env and no Claude key found in .model")
    if not model:
        model = "claude-opus-5"
    if not base or base == "null":
        base = ANTHROPIC_BASE
    return base, key, model

def main():
    ensure_binary()
    base_url, claude_key, claude_model = claude_credentials()
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
            "provider_model_name": claude_model,
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
