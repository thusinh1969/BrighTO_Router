#!/usr/bin/env python3
"""Full local stress benchmark for BrighTO-Router.

This benchmark is mock-only. It starts temporary PostgreSQL, two local
brighto-router-mock upstreams, and a temporary router. It measures direct mock
latency, then router-minus-direct overhead for:

  - HTTP and HTTPS router modes
  - single route, Model Group round-robin, Model Group weighted
  - payloads 1k, 50k, 200k, 500k, 1m
  - concurrency 50 and 100 by default

No paid provider is called. Existing local Docker Compose state is not touched.
"""

from __future__ import annotations

import hashlib
import json
import os
import pathlib
import socket
import subprocess
import time

REPO = pathlib.Path(__file__).resolve().parents[1]
OUT_ROOT = REPO / "bench" / "results"
KEY = "bench-key"
ADMIN_KEY = os.environ.get("ADMIN_MASTER_KEY", "br-admin-bench-local-only")
DUR = os.environ.get("DUR", "60s")
CONCS = [int(x) for x in os.environ.get("CONCS", "50,100").split(",") if x]
PAYLOADS = [x for x in os.environ.get("PAYLOADS", "1k,50k,200k,500k,1m").split(",") if x]
POLICIES = ["single", "rr", "weighted"]
TOKENS = {"1k": 1_000, "50k": 50_000, "200k": 200_000, "500k": 500_000, "1m": 1_000_000}
RATES_50 = {"1k": 1_000, "50k": 500, "200k": 150, "500k": 60, "1m": 30}
RATES_100 = {"1k": 2_000, "50k": 800, "200k": 200, "500k": 80, "1m": 40}


def rate_for(payload: str, conc: int) -> int:
    return RATES_50[payload] if conc <= 50 else RATES_100[payload]


def sh(*args, env=None, cwd=None, capture_output=False, check=True):
    print("+", " ".join(map(str, args)), flush=True)
    return subprocess.run(
        [str(a) for a in args],
        env=env,
        cwd=cwd,
        text=True,
        capture_output=capture_output,
        check=check,
    )


def free_port() -> int:
    sock = socket.socket()
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
    sock.close()
    return port


def wait_url(url: str, insecure: bool = False, timeout: int = 90) -> None:
    import ssl
    import urllib.request

    ctx = ssl._create_unverified_context() if insecure else None
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=2, context=ctx) as response:
                if response.status < 500:
                    return
        except Exception as exc:  # noqa: BLE001 - best-effort readiness wait
            last = exc
        time.sleep(0.25)
    raise RuntimeError(f"timeout waiting {url}: {last}")


def psql(db: str, sql: str) -> None:
    subprocess.run(
        ["psql", db, "-v", "ON_ERROR_STOP=1", "-q", "-c", sql],
        env=dict(os.environ, PGPASSWORD="brighto_router_dev"),
        check=True,
    )


def make_payloads(outdir: pathlib.Path) -> pathlib.Path:
    payload_dir = outdir / "payloads"
    payload_dir.mkdir(parents=True, exist_ok=True)
    chunk = (
        "path: crates/router/src/proxy.rs\n"
        "fn hot_path() { /* million-token brighto router */ }\n" * 64
    )
    for name in PAYLOADS:
        chars = TOKENS[name] * 4
        content = (chunk * ((chars // len(chunk)) + 1))[:chars]
        for label, model in {
            "direct": "mock-model",
            "single": "mock-model",
            "rr": "lb-rr",
            "weighted": "lb-weighted",
        }.items():
            body = {
                "model": model,
                "messages": [{"role": "user", "content": content}],
                "stream": False,
                "temperature": 0.1,
            }
            (payload_dir / f"{label}-{name}.json").write_text(json.dumps(body, separators=(",", ":")))
    return payload_dir


def parse_oha(path: pathlib.Path) -> dict:
    data = json.loads(path.read_text())
    summary = data.get("summary") or {}
    latency = data.get("latencyPercentiles") or {}
    status = data.get("statusCodeDistribution") or {}
    total = sum(int(v) for v in status.values()) if isinstance(status, dict) else 0
    ok = int(status.get("200", 0) or status.get(200, 0) or 0) if isinstance(status, dict) else 0
    return {
        "p50": (latency.get("p50") or latency.get("50") or 0) * 1000,
        "p99": (latency.get("p99") or latency.get("99") or 0) * 1000,
        "rps": summary.get("requestsPerSec") or 0,
        "non200": max(0, total - ok),
        "total": total,
    }


def run_oha(url: str, body: pathlib.Path, out: pathlib.Path, conc: int, rate: int, *, auth=False, insecure=False) -> dict:
    cmd = [
        "oha",
        "-z",
        DUR,
        "-c",
        str(conc),
        "-m",
        "POST",
        "--no-tui",
        "--latency-correction",
        "--wait-ongoing-requests-after-deadline",
        "-q",
        str(rate),
        "-T",
        "application/json",
        "-D",
        str(body),
        "-o",
        str(out),
        "--output-format",
        "json",
    ]
    if auth:
        cmd += ["-H", f"authorization: Bearer {KEY}"]
    if insecure:
        cmd += ["--insecure"]
    cmd += [url]
    subprocess.run(cmd, check=True)
    return parse_oha(out)


def create_self_signed_cert(outdir: pathlib.Path) -> tuple[pathlib.Path, pathlib.Path]:
    cert = outdir / "tls-cert.pem"
    key = outdir / "tls-key.pem"
    sh(
        "openssl",
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-keyout",
        key,
        "-out",
        cert,
        "-days",
        "1",
        "-nodes",
        "-subj",
        "/CN=127.0.0.1",
        capture_output=True,
    )
    return cert, key


def main() -> None:
    timestamp = time.strftime("%Y%m%d-%H%M%S")
    outdir = OUT_ROOT / f"{timestamp}-full-1m-http-https-lb-local"
    outdir.mkdir(parents=True, exist_ok=True)
    print("outdir", outdir, flush=True)
    payload_dir = make_payloads(outdir)
    sh("cargo", "build", "--release", "--locked", cwd=REPO)

    pg_name = f"brighto_full_pg_{os.getpid()}"
    procs = []
    pg_started = False
    try:
        sh(
            "docker",
            "run",
            "--rm",
            "-d",
            "--name",
            pg_name,
            "-e",
            "POSTGRES_DB=brighto_router",
            "-e",
            "POSTGRES_USER=brighto_router",
            "-e",
            "POSTGRES_PASSWORD=brighto_router_dev",
            "-p",
            "127.0.0.1::5432",
            "postgres:16-alpine",
        )
        pg_started = True
        pg_port = subprocess.check_output(["docker", "port", pg_name, "5432/tcp"], text=True).strip().split(":")[-1]
        db = f"postgres://brighto_router:brighto_router_dev@127.0.0.1:{pg_port}/brighto_router"
        for _ in range(80):
            ready = subprocess.run(
                ["docker", "exec", pg_name, "pg_isready", "-U", "brighto_router", "-d", "brighto_router"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            ).returncode == 0
            if ready:
                break
            time.sleep(0.5)
        sh("sqlx", "migrate", "run", "--source", REPO / "migrations", env=dict(os.environ, DATABASE_URL=db))

        mock_a_port = free_port()
        mock_b_port = free_port()
        for label, port in [("a", mock_a_port), ("b", mock_b_port)]:
            log = (outdir / f"mock-{label}.log").open("wb")
            proc = subprocess.Popen(
                [str(REPO / "target/release/brighto-router-mock")],
                env=dict(os.environ, MOCK_ADDR=f"127.0.0.1:{port}", MOCK_MAX_BODY_BYTES=str(24 * 1024 * 1024)),
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            procs.append((proc, log))
        wait_url(f"http://127.0.0.1:{mock_a_port}/health")
        wait_url(f"http://127.0.0.1:{mock_b_port}/health")

        key_hash = hashlib.sha256(KEY.encode()).hexdigest()
        psql(
            db,
            f"""
INSERT INTO backends (id,name,base_url,api_key_ref,weight,max_inflight,format,enabled)
VALUES (1,'mock-a','http://127.0.0.1:{mock_a_port}','',1,0,'openai',TRUE),
       (2,'mock-b','http://127.0.0.1:{mock_b_port}','',1,0,'openai',TRUE);
INSERT INTO model_routes (model_name,backend_ids,fallback_backend_id,chars_per_token,first_byte_timeout,provider_model_name,enabled,auth_mode,protocol,routing_policy)
VALUES ('mock-model','[1]',NULL,4.0,180,'mock-model',TRUE,'none','openai_chat','least_loaded_weighted'),
       ('lb-rr','[1,2]',NULL,4.0,180,'lb-rr',TRUE,'none','openai_chat','round_robin'),
       ('lb-weighted','[1,2]',NULL,4.0,180,'lb-weighted',TRUE,'none','openai_chat','weighted_round_robin');
INSERT INTO model_route_endpoints (model_name,backend_id,provider_model_name,provider_key_ref,auth_mode,protocol,weight,max_inflight,enabled)
VALUES ('lb-rr',1,'mock-model-a',NULL,'none','openai_chat',1,0,TRUE),
       ('lb-rr',2,'mock-model-b',NULL,'none','openai_chat',1,0,TRUE),
       ('lb-weighted',1,'mock-model-a',NULL,'none','openai_chat',3,0,TRUE),
       ('lb-weighted',2,'mock-model-b',NULL,'none','openai_chat',1,0,TRUE);
INSERT INTO teams (id,name,budget,enabled)
VALUES (1,'bench','{{"period":"month","max_tokens":1000000000000,"per_model":{{}}}}',TRUE);
INSERT INTO api_keys (id,key_hash,key_prefix,team_id,owner,allowed_models,budget,rpm_limit,concurrency_limit,expires_at,enabled)
VALUES (1,'{key_hash}','bench-key',1,'bench','[]',NULL,NULL,NULL,NULL,TRUE);
""",
        )

        direct = {}
        rows = []
        for payload in PAYLOADS:
            for conc in CONCS:
                rate = rate_for(payload, conc)
                result = run_oha(
                    f"http://127.0.0.1:{mock_a_port}/v1/chat/completions",
                    payload_dir / f"direct-{payload}.json",
                    outdir / f"direct-{payload}-c{conc}.json",
                    conc,
                    rate,
                )
                direct[(payload, conc)] = result
                print(
                    f"DIRECT {payload} c={conc} p50={result['p50']:.3f} p99={result['p99']:.3f} rps={result['rps']:.1f}",
                    flush=True,
                )

        cert, key = create_self_signed_cert(outdir)
        for scheme in ["http", "https"]:
            router_port = free_port()
            insecure = scheme == "https"
            base = f"{scheme}://127.0.0.1:{router_port}"
            router_log = (outdir / f"router-{scheme}.log").open("wb")
            router_env = dict(
                os.environ,
                DATABASE_URL=db,
                LISTEN_ADDR=f"127.0.0.1:{router_port}",
                ADMIN_MASTER_KEY=ADMIN_KEY,
                RUST_LOG="error",
                MAX_BODY_BYTES=str(24 * 1024 * 1024),
            )
            if scheme == "https":
                router_env.update(TLS_CERT_PATH=str(cert), TLS_KEY_PATH=str(key))
            else:
                router_env.update(TLS_CERT_PATH="", TLS_KEY_PATH="")
            router = subprocess.Popen(
                [str(REPO / "target/release/brighto-router")],
                env=router_env,
                stdout=router_log,
                stderr=subprocess.STDOUT,
            )
            procs.append((router, router_log))
            wait_url(f"{base}/readyz", insecure=insecure)

            for payload in PAYLOADS:
                for conc in CONCS:
                    rate = rate_for(payload, conc)
                    baseline = direct[(payload, conc)]
                    for policy in POLICIES:
                        result = run_oha(
                            f"{base}/v1/chat/completions",
                            payload_dir / f"{policy}-{payload}.json",
                            outdir / f"{scheme}-{policy}-{payload}-c{conc}.json",
                            conc,
                            rate,
                            auth=True,
                            insecure=insecure,
                        )
                        row = {
                            "scheme": scheme,
                            "policy": policy,
                            "payload": payload,
                            "concurrency": conc,
                            "direct_p50_ms": baseline["p50"],
                            "router_p50_ms": result["p50"],
                            "overhead_p50_ms": result["p50"] - baseline["p50"],
                            "direct_p99_ms": baseline["p99"],
                            "router_p99_ms": result["p99"],
                            "overhead_p99_ms": result["p99"] - baseline["p99"],
                            "router_rps": result["rps"],
                            "non200": result["non200"],
                        }
                        rows.append(row)
                        print(
                            f"{scheme.upper():5} {policy:<8} {payload:>4} c={conc:<3} "
                            f"p50_overhead={row['overhead_p50_ms']:+.3f}ms "
                            f"p99_overhead={row['overhead_p99_ms']:+.3f}ms "
                            f"rps={row['router_rps']:.1f} non200={row['non200']}",
                            flush=True,
                        )
            router.terminate()
            router.wait(timeout=10)
            proc, log = procs.pop()
            assert proc is router
            log.close()

        summary = {
            "name": "BrighTO local full 1M HTTP/HTTPS single-route and Model Group stress",
            "commit": subprocess.check_output(["git", "rev-parse", "--short", "HEAD"], cwd=REPO, text=True).strip(),
            "knobs": {"DUR": DUR, "CONCS": CONCS, "PAYLOADS": PAYLOADS, "rates_50": RATES_50, "rates_100": RATES_100},
            "note": "HTTPS rows compare HTTPS router latency against the same direct HTTP mock baseline, so they include direct router TLS cost.",
            "rows": rows,
        }
        (outdir / "summary.json").write_text(json.dumps(summary, indent=2))
        print("SUMMARY", outdir / "summary.json", flush=True)
    finally:
        for proc, log in reversed(procs):
            try:
                proc.terminate()
                proc.wait(timeout=5)
            except Exception:  # noqa: BLE001 - cleanup path
                try:
                    proc.kill()
                except Exception:
                    pass
            try:
                log.close()
            except Exception:
                pass
        if pg_started:
            subprocess.run(["docker", "stop", pg_name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


if __name__ == "__main__":
    main()
