#!/usr/bin/env python3
"""Runtime smoke: stream_options false-positive. Xac nhan router chen top-level stream_options.include_usage
kể cả khi prompt chứa chuỗi "stream_options" (nested/string). Echo backend capture body thật."""
import hashlib
import http.server
import json
import os
import subprocess
import threading
import time
import pathlib
import urllib.request

import requests

REPO = pathlib.Path(__file__).resolve().parent.parent
ROUTER = "http://127.0.0.1:8090"
KEY = "smoke-key"

bodies = []


class Echo(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        bodies.append(self.rfile.read(n))
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"usage":{"prompt_tokens":1,"completion_tokens":1}}')

    def log_message(self, *a):
        pass


def main():
    srv = http.server.HTTPServer(("127.0.0.1", 9001), Echo)
    threading.Thread(target=srv.serve_forever, daemon=True).start()

    pg = "brigto_so_pg_%d" % os.getpid()
    subprocess.run(
        ["docker", "run", "--rm", "-d", "--name", pg,
         "-e", "POSTGRES_DB=llm_router", "-e", "POSTGRES_USER=llm_router",
         "-e", "POSTGRES_PASSWORD=llm_router_dev", "-p", "127.0.0.1::5432",
         "pgvector/pgvector:pg16"],
        check=True, capture_output=True,
    )
    port = subprocess.run(["docker", "port", pg, "5432/tcp"], check=True, capture_output=True, text=True).stdout.strip().split(":")[-1]
    db = "postgres://llm_router:llm_router_dev@127.0.0.1:%s/llm_router" % port
    for _ in range(60):
        r = subprocess.run(["docker", "exec", pg, "pg_isready", "-U", "llm_router", "-d", "llm_router"], capture_output=True)
        if r.returncode == 0:
            break
        time.sleep(1)
    env = dict(os.environ, DATABASE_URL=db)
    subprocess.run(["sqlx", "migrate", "run", "--source", str(REPO / "migrations")], check=True, capture_output=True, env=env)
    kh = hashlib.sha256(KEY.encode()).hexdigest()
    sql = (
        "INSERT INTO backends (id,name,base_url,api_key_ref,weight,max_inflight,format,enabled) VALUES (1,'echo','http://127.0.0.1:9001','ECHO_KEY',1,100,'openai',TRUE);"
        + "INSERT INTO model_routes (model_name,backend_ids,fallback_backend_id,chars_per_token,first_byte_timeout) VALUES ('x','[1]',NULL,4.0,180);"
        + "INSERT INTO teams (id,name,budget,enabled) VALUES (1,'t','{\"period\":\"month\",\"max_tokens\":100000000,\"per_model\":{}}',TRUE);"
        + "INSERT INTO api_keys (id,key_hash,key_prefix,team_id,owner,allowed_models,budget,rpm_limit,concurrency_limit,expires_at,enabled) VALUES (1,'%s','smoke',1,'s','[]',NULL,NULL,NULL,NULL,TRUE);" % kh
    )
    subprocess.run(
        ["psql", "-h", "127.0.0.1", "-p", port, "-U", "llm_router", "-d", "llm_router", "-q", "-c", sql],
        check=True, capture_output=True, env=dict(os.environ, PGPASSWORD="llm_router_dev"),
    )

    router = subprocess.Popen(
        [str(REPO / "target/release/brigto-router")],
        env=dict(os.environ, DATABASE_URL=db, LISTEN_ADDR="127.0.0.1:8090", ECHO_KEY="x", RUST_LOG="error"),
    )
    try:
        for _ in range(100):
            try:
                if requests.get(ROUTER + "/healthz", timeout=1).status_code == 200:
                    break
            except Exception:
                pass
            time.sleep(0.1)

        cases = [
            ("normal_missing_root", '{"model":"x","stream":true,"messages":[{"role":"user","content":"hello"}]}'),
            ("string_false_positive", '{"model":"x","stream":true,"messages":[{"role":"user","content":"stream_options"}]}'),
            ("nested_false_positive", '{"model":"x","stream":true,"messages":[{"role":"user","content":"hello","stream_options":{"include_usage":false}}]}'),
        ]
        ok = True
        for name, body in cases:
            r = requests.post(
                ROUTER + "/v1/chat/completions",
                headers={"Authorization": "Bearer " + KEY, "Content-Type": "application/json"},
                data=body, stream=True,
            )
            r.raise_for_status()
            list(r.iter_content(chunk_size=None))
            fwd = bodies.pop(0).decode()
            top = '"stream_options":{"include_usage":true}' in fwd
            print("CASE %-22s status=%d top_injected=%s" % (name, r.status_code, top))
            ok = ok and top
        print("RESULT", "PASS" if ok else "FAIL")
    finally:
        router.terminate()
        subprocess.run(["docker", "rm", "-f", pg], capture_output=True)
        srv.shutdown()


if __name__ == "__main__":
    main()
