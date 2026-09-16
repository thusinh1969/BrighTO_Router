#!/usr/bin/env python3
"""Runtime HTTP smoke cho portal + self-service endpoints.

Chung minh bang HTTP that (khong chi unit test):
  - Admin: GET /admin/teams, /admin/keys, /admin/stats, POST /admin/keys.
  - User: GET /portal/me, /portal/me/usage, /portal/me/stats (auth bang client API key).
  - Bao mat: bad key -> 401; /admin/keys khong tra plaintext key.

Run: python3 scripts/portal_smoke.py
Can: docker, sqlx-cli, psql, va target/release/brighto-router da build.
"""
import os
import pathlib
import socket
import subprocess
import sys
import time

import requests

REPO = pathlib.Path(__file__).resolve().parent.parent
BIN = REPO / "target/release/brighto-router"
ADMIN_KEY = "portal-smoke-admin"


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def sh(*a, **kw):
    return subprocess.run(a, check=True, capture_output=True, text=True, **kw)


def main():
    pg = "brighto_portal_smoke_%d" % os.getpid()
    pg_port = free_port()
    router_port = free_port()
    sh(
        "docker", "run", "--rm", "-d", "--name", pg,
        "-e", "POSTGRES_DB=llm_router",
        "-e", "POSTGRES_USER=llm_router",
        "-e", "POSTGRES_PASSWORD=llm_router_dev",
        "-p", "127.0.0.1:%d:5432" % pg_port,
        "postgres:16-alpine",
    )
    db = "postgres://llm_router:llm_router_dev@127.0.0.1:%d/llm_router" % pg_port
    router = None
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
           "INSERT INTO teams (id,name,budget,enabled) VALUES (1,'Smoke Team',NULL,TRUE);",
           env=dict(os.environ, PGPASSWORD="llm_router_dev"))

        router = subprocess.Popen(
            [str(BIN)],
            env=dict(
                os.environ,
                DATABASE_URL=db,
                LISTEN_ADDR="127.0.0.1:%d" % router_port,
                ADMIN_MASTER_KEY=ADMIN_KEY,
                RUST_LOG="error",
            ),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        base = "http://127.0.0.1:%d" % router_port
        for _ in range(100):
            try:
                if requests.get(base + "/healthz", timeout=1).status_code == 200:
                    break
            except Exception:
                pass
            time.sleep(0.1)

        admin = {"x-admin-key": ADMIN_KEY, "Content-Type": "application/json"}
        checks = []

        def check(name, ok):
            checks.append((name, ok))
            print(("PASS " if ok else "FAIL ") + name)

        # Admin list endpoints
        r = requests.get(base + "/admin/teams", headers=admin)
        check("admin /teams lists 1 team", r.status_code == 200 and len(r.json()) == 1)
        r = requests.get(base + "/admin/keys", headers=admin)
        check("admin /keys starts empty", r.status_code == 200 and r.json() == [])
        r = requests.get(base + "/admin/stats?days=30", headers=admin)
        check("admin /stats starts empty", r.status_code == 200 and r.json() == [])
        r = requests.get(base + "/admin/summary?days=30", headers=admin)
        ok = (r.status_code == 200 and "totals" in r.json()
              and "by_model" in r.json() and "by_team" in r.json() and "by_key" in r.json()
              and "by_bucket" in r.json()
              and "estimated_cost_usd" in r.json()["totals"]
              and "cost_known_requests" in r.json()["totals"]
              and "p95_router_overhead_ms" in r.json()["totals"])
        check("admin /summary returns grouped stats + cost + buckets", ok)

        # Create a client key (plaintext returned once)
        r = requests.post(base + "/admin/keys", headers=admin,
                          json={"team_id": 1, "owner": "smoke", "allowed_models": []})
        body = r.json()
        key = body.get("key", "")
        key_id = body.get("id")
        check("POST /admin/keys returns lc- key", r.status_code == 200 and key.startswith("lc-"))

        # New SOTA endpoints: reveal key, settings, add provider.
        r = requests.get(base + "/admin/keys/%d/reveal" % key_id, headers=admin)
        check("GET /admin/keys/{id}/reveal returns plaintext", r.status_code == 200 and r.json()["key"] == key)
        r = requests.get(base + "/admin/settings", headers=admin)
        ok = r.status_code == 200 and "version" in r.json() and "database_ok" in r.json()
        check("GET /admin/settings returns runtime info", ok)
        r = requests.post(base + "/admin/backends", headers=admin,
                          json={"name": "smoke-provider", "base_url": "https://example.com",
                                "api_key_ref": "env:SMOKE_KEY", "format": "openai", "enabled": False})
        check("POST /admin/backends creates provider", r.status_code == 200 and r.json().get("id"))
        bid = r.json()["id"]

        # Provider DELETE 409 path: route còn tham chiếu -> chặn xoá, message chứa tên route.
        r = requests.post(base + "/admin/routes", headers=admin,
                          json={"model_name": "smoke-route", "backend_ids": [bid],
                                "auth_mode": "none"}, timeout=60)
        check("create route referencing provider", r.status_code == 200)
        r2 = requests.delete(base + "/admin/backends/%d" % bid, headers=admin)
        ok = r2.status_code == 409 and "smoke-route" in (r2.text or "")
        check("DELETE backend in use -> 409 with route name", ok)

        # Cleanup route -> backend xoá được.
        requests.delete(base + "/admin/routes/smoke-route", headers=admin)
        r2 = requests.delete(base + "/admin/backends/%d" % bid, headers=admin)
        check("DELETE /admin/backends removes unused provider", r2.status_code == 204)

        # Insert a real usage row for this key so stats/usage are non-trivial.
        now = int(time.time())
        sh("psql", "-h", "127.0.0.1", "-p", str(pg_port), "-U", "llm_router",
           "-d", "llm_router", "-q", "-c",
           "INSERT INTO usage_ledger (ts, request_id, key_id, team_id, model, backend_id, status, "
           "input_tokens, output_tokens, estimated, ttfb_ms, total_ms, router_overhead_ms, stream, client_aborted) "
           "VALUES (%d, 'smoke-req-1', %d, 1, 'smoke-model', 1, 200, 100, 10, false, 50, 100, 3, false, false);"
           % (now, key_id),
           env=dict(os.environ, PGPASSWORD="llm_router_dev"))

        user = {"Authorization": "Bearer " + key, "Content-Type": "application/json"}
        r = requests.get(base + "/portal/me", headers=user)
        ok = (r.status_code == 200 and r.json()["key"]["owner"] == "smoke"
              and r.json()["team"]["name"] == "Smoke Team")
        check("GET /portal/me returns own key + team", ok)

        r = requests.get(base + "/portal/me/usage", headers=user)
        u0 = r.json()[0] if (r.status_code == 200 and len(r.json()) == 1) else {}
        ok = (r.status_code == 200 and len(r.json()) == 1
              and u0.get("total_tokens") == 110
              and u0.get("prompt_size_bucket") == "<2k"
              and u0.get("duration_display") == "100 ms"
              and u0.get("router_overhead_display") == "3 ms"
              and u0.get("total_tokens_per_second") == 1100.0
              and u0.get("cost_known") is False)
        check("GET /portal/me/usage returns enriched row (tok/s, bucket, duration, cost)", ok)

        r = requests.get(base + "/portal/me/stats?days=30", headers=user)
        js = r.json()
        ok = (r.status_code == 200 and len(js) == 1
              and js[0]["model"] == "smoke-model"
              and js[0]["input_tokens"] == 100 and js[0]["output_tokens"] == 10)
        check("GET /portal/me/stats aggregates own usage", ok)

        # Security: reveal is admin-only; list endpoint does not leak plaintext.
        r = requests.get(base + "/portal/me", headers={"Authorization": "Bearer bad-key"})
        check("bad key -> 401", r.status_code == 401)
        r = requests.get(base + "/admin/keys", headers=admin)
        check("admin /keys never returns plaintext", all("key" not in row for row in r.json()))

        # Negative reveal cases (user override: admin may reveal, nobody else may).
        r = requests.get(base + "/admin/keys/%d/reveal" % key_id, headers={"x-admin-key": "wrong"})
        check("bad admin key reveal -> 401", r.status_code == 401)
        r = requests.get(base + "/admin/keys/%d/reveal" % key_id,
                         headers={"Authorization": "Bearer " + key})
        check("client API key cannot reveal -> 401", r.status_code == 401)
        # Legacy key (key_secret IS NULL) -> 410 with clear message.
        legacy = sh("psql", "-h", "127.0.0.1", "-p", str(pg_port), "-U", "llm_router",
                    "-d", "llm_router", "-q", "-tA", "-c",
                    "INSERT INTO api_keys (key_hash, key_prefix, team_id, owner, allowed_models, budget, "
                    "rpm_limit, concurrency_limit, expires_at, enabled) "
                    "VALUES ('legacyhash', 'legacy-00', 1, 'legacy', '[]', NULL, NULL, NULL, NULL, TRUE) RETURNING id;",
                    env=dict(os.environ, PGPASSWORD="llm_router_dev"))
        legacy_id = int(legacy.stdout.strip())
        r = requests.get(base + "/admin/keys/%d/reveal" % legacy_id, headers=admin)
        check("legacy key reveal -> 410", r.status_code == 410)

        fails = [n for n, ok in checks if not ok]
        print("RESULT " + ("PASS" if not fails else "FAIL: " + ", ".join(fails)))
        sys.exit(0 if not fails else 1)
    finally:
        if router:
            router.terminate()
            try:
                router.wait(timeout=5)
            except subprocess.TimeoutExpired:
                router.kill()
        subprocess.run(["docker", "rm", "-f", pg], capture_output=True)


if __name__ == "__main__":
    main()
