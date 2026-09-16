#!/usr/bin/env python3
"""Bench smoke: direct llama-server vs router (TTFB + total), streaming, Postgres thật.
Chạy: python3 scripts/bench_smoke.py  (cần llama-server 8088 sống, docker + sqlx-cli + psql)

Đây là bench CHỈ ĐỊNH HƯỚNG (smoke) — con số SOTA chính thức cần payload 1K/50K/200K +
concurrency 1/50/200 qua make bench (bench/run.sh).
"""
import hashlib
import os
import subprocess
import time
import pathlib

import requests

REPO = pathlib.Path(__file__).resolve().parent.parent
LLAMA_BASE = "http://127.0.0.1:8088"
DIRECT = LLAMA_BASE + "/v1/chat/completions"
ROUTER = "http://127.0.0.1:8090"
ROUTER_CHAT = ROUTER + "/v1/chat/completions"
KEY = "lc-dev0001"
N = 15
WARMUP = 3


def sh(*args, **kw):
    return subprocess.run(args, check=True, capture_output=True, text=True, **kw)


def main():
    name = "brigto_bench_pg_%d" % os.getpid()
    subprocess.run(
        [
            "docker", "run", "--rm", "-d", "--name", name,
            "-e", "POSTGRES_DB=llm_router",
            "-e", "POSTGRES_USER=llm_router",
            "-e", "POSTGRES_PASSWORD=llm_router_dev",
            "-p", "127.0.0.1::5432", "pgvector/pgvector:pg16",
        ],
        check=True, capture_output=True,
    )
    port = sh("docker", "port", name, "5432/tcp").stdout.strip().split(":")[-1]
    db = "postgres://llm_router:llm_router_dev@127.0.0.1:%s/llm_router" % port
    for _ in range(60):
        r = subprocess.run(
            ["docker", "exec", name, "pg_isready", "-U", "llm_router", "-d", "llm_router"],
            capture_output=True,
        )
        if r.returncode == 0:
            break
        time.sleep(1)

    proc = None
    try:
        env = dict(os.environ, DATABASE_URL=db)
        sh("sqlx", "migrate", "run", "--source", str(REPO / "migrations"), env=env)

        key_hash = hashlib.sha256(KEY.encode()).hexdigest()
        psql = [
            "psql", "-h", "127.0.0.1", "-p", port, "-U", "llm_router", "-d", "llm_router", "-q",
        ]
        pgenv = dict(os.environ, PGPASSWORD="llm_router_dev")
        sql = (
            "INSERT INTO backends (id,name,base_url,api_key_ref,weight,max_inflight,format,enabled) "
            "VALUES (1,'llama','%s','BACKEND_KEY_LLAMA',1,100,'openai',TRUE);" % LLAMA_BASE
            + "INSERT INTO model_routes (model_name,backend_ids,fallback_backend_id,chars_per_token,first_byte_timeout) "
            "VALUES ('x','[1]',NULL,4.0,180);"
            + "INSERT INTO teams (id,name,budget,enabled) VALUES (1,'team','{\"period\":\"month\",\"max_tokens\":100000000,\"per_model\":{}}',TRUE);"
            + "INSERT INTO api_keys (id,key_hash,key_prefix,team_id,owner,allowed_models,budget,rpm_limit,concurrency_limit,expires_at,enabled) "
            "VALUES (1,'%s','lc-dev000',1,'bench','[]',NULL,NULL,NULL,NULL,TRUE);" % key_hash
        )
        subprocess.run(psql + ["-c", sql], check=True, capture_output=True, text=True, env=pgenv)

        env2 = dict(
            os.environ,
            DATABASE_URL=db,
            LISTEN_ADDR="127.0.0.1:8090",
            BACKEND_KEY_LLAMA="dummy",
            RUST_LOG="error",
            CONFIG_POLL_SECS="3600",
        )
        proc = subprocess.Popen([str(REPO / "target/release/brigto-router")], env=env2)
        for _ in range(100):
            try:
                if requests.get(ROUTER + "/healthz", timeout=1).status_code == 200:
                    break
            except Exception:
                pass
            time.sleep(0.1)
        else:
            print("router not up in 10s")
            return

        d_ttf, d_total = bench(DIRECT, label="direct")
        r_ttf, r_total = bench(ROUTER_CHAT, key=KEY, label="router")
        print("-" * 50)
        print(
            "ROUTER OVERHEAD: TTFB +%.2fms | total +%.2fms (target TTFB < 3ms o 200K, total < 1%%)"
            % (r_ttf - d_ttf, r_total - d_total)
        )
    finally:
        if proc:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
        subprocess.run(["docker", "rm", "-f", name], capture_output=True)


def measure(url, key=None):
    headers = {"Content-Type": "application/json"}
    if key:
        headers["Authorization"] = "Bearer " + key
    payload = {
        "model": "x",
        "messages": [{"role": "user", "content": "Count 1 to 20, one per line."}],
        "stream": True,
        "max_tokens": 40,
        "temperature": 0.0,
    }
    t0 = time.perf_counter()
    ttf = None
    with requests.post(url, json=payload, headers=headers, stream=True, timeout=120) as r:
        r.raise_for_status()
        for raw in r.iter_lines():
            if raw and ttf is None:
                ttf = time.perf_counter() - t0
            if raw == b"data: [DONE]":
                break
    total = time.perf_counter() - t0
    return ttf, total


def bench(url, key=None, label=""):
    for _ in range(WARMUP):
        measure(url, key)
    ttfs, totals = [], []
    for _ in range(N):
        ttf, total = measure(url, key)
        ttfs.append(ttf * 1000)
        totals.append(total * 1000)
    ttfs.sort()
    totals.sort()
    med_ttf = ttfs[len(ttfs) // 2]
    med_total = totals[len(totals) // 2]
    print("%-8s: median TTFB %6.1fms | median total %7.1fms" % (label, med_ttf, med_total))
    return med_ttf, med_total


if __name__ == "__main__":
    main()
