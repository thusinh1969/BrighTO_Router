#!/usr/bin/env python3
"""SOTA benchmark orchestrator + measurement for BrighTO-Router (mock upstream + Postgres ledger).

Chứng minh router-minus-direct overhead trên payload 1K/50K/200K ở concurrency 1/50/200,
delta TTFB streaming, và throughput B6 target-rate (conc 200 thật). Ghi raw `oha` JSON cho MỌI
run (không ghi đè direct bằng router) + `gate.json` audit-friendly.

Cách chạy:
    # smoke ngắn: chứng minh orchestration + artifact, KHÔNG phải số SOTA (không fail ngưỡng)
    DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
    # full release gate
    python3 scripts/bench_real.py

Env knobs: DUR (60s), WARM (15s), RUNS (3), CONCS ("1,50,200"), REQUIRE_PASS ("1"),
            ADMIN_MASTER_KEY (bench-admin), MODEL (mock-model), BENCH_RPS_1K/50K/200K, B6_TARGET_RPS,
            BENCH_PAYLOADS ("1k,50k,200k"), BENCH_STREAM_PAYLOADS ("1k-stream,50k-stream,200k-stream"), BENCH_B6 ("1").
Cần: docker, sqlx-cli, psql, oha, cargo. Build target/release/brighto-router + brighto-router-mock.
"""
import hashlib
import json
import os
import pathlib
import socket
import statistics
import subprocess
import sys
import time

import requests

REPO = pathlib.Path(__file__).resolve().parent.parent
KEY = "bench-key"
MODEL = os.environ.get("MODEL", "mock-model")

CONCS = [c.strip() for c in os.environ.get("CONCS", "1,50,200").split(",") if c.strip()]
DUR = os.environ.get("DUR", "60s")
WARM = os.environ.get("WARM", "15s")
RUNS = int(os.environ.get("RUNS", "3"))
REQUIRE_PASS = os.environ.get("REQUIRE_PASS", "1") != "0"
ADMIN_KEY = os.environ.get("ADMIN_MASTER_KEY", "bench-admin")
# Ngưỡng tầng B — mirror benchmarks/thresholds.toml (đơn vị ms trừ B6).
B1_P50 = {"1k": 0.3, "50k": 0.6, "200k": 1.0}
B2_P99 = {"1k": 0.8, "50k": 1.5, "200k": 2.0}
B3_FLAT_P50_DELTA_MS = 0.8
B4_TTFB = {"1k": 1.0, "50k": 2.0, "200k": 3.0}
B6_MIN_RPS = 8000
WORST_RUN_TOLERANCE = 1.25
B6_MAX_NON200 = 0
B6_TARGET_RPS = int(os.environ.get("B6_TARGET_RPS", str(B6_MIN_RPS + 500)))
RUN_B6 = os.environ.get("BENCH_B6", "1") != "0"


def env_list(name, default, allowed):
    raw = os.environ.get(name)
    if raw is None:
        vals = list(default)
    elif raw.strip() == "":
        vals = []
    else:
        vals = [v.strip() for v in raw.split(",") if v.strip()]
    invalid = [v for v in vals if v not in allowed]
    if invalid:
        raise RuntimeError("%s contains invalid values %s; allowed=%s" % (name, invalid, sorted(allowed)))
    return vals


PAYLOADS = env_list("BENCH_PAYLOADS", ["1k", "50k", "200k"], B1_P50.keys())
STREAM_PAYLOADS = env_list(
    "BENCH_STREAM_PAYLOADS",
    ["1k-stream", "50k-stream", "200k-stream"],
    ["1k-stream", "50k-stream", "200k-stream"],
)
BENCH_TARGET_RPS = {
    "1k": int(os.environ.get("BENCH_RPS_1K", "4000")),
    "50k": int(os.environ.get("BENCH_RPS_50K", "1000")),
    "200k": int(os.environ.get("BENCH_RPS_200K", "250")),
}


def sh(*a, **kw):
    p = subprocess.run(a, capture_output=True, text=True, **kw)
    if p.returncode != 0:
        raise RuntimeError("cmd failed (%s):\n%s" % (" ".join(a), p.stderr[-2000:]))
    return p


def median(xs):
    return round(statistics.median(xs), 3)


def percentile_gate(gid, payload, conc, val, thr, runs):
    worst = max(runs) if runs else val
    worst_threshold = round(thr * WORST_RUN_TOLERANCE, 3)
    ok = val <= thr and worst <= worst_threshold
    return {
        "id": gid,
        "payload": payload,
        "conc": conc,
        "value": val,
        "threshold": thr,
        "pass": ok,
        "runs": [round(x, 3) for x in runs],
        "worst": round(worst, 3),
        "worst_threshold": worst_threshold,
    }


def latency_rate(payload, conc):
    # c=1 is a closed-loop single-request latency probe. A high open-loop -q with one
    # connection creates client-side backlog and bogus router-minus-direct deltas.
    try:
        if int(conc) <= 1:
            return None
    except Exception:
        return None
    return BENCH_TARGET_RPS[payload]


def free_tcp_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def host_info():
    cpu = ""
    try:
        cpu = sh("sh", "-c", "grep -m1 'model name' /proc/cpuinfo | cut -d: -f2").stdout.strip()
    except Exception:
        pass
    kernel = sh("uname", "-r").stdout.strip()
    try:
        sha = sh("git", "rev-parse", "--short", "HEAD", cwd=REPO).stdout.strip()
    except Exception:
        sha = "nogit"
    return {"cpu": cpu, "kernel": kernel, "sha": sha}


def parse_oha_json(out, raw_label, payload, conc, stderr):
    try:
        data = json.load(open(out))
    except Exception as e:
        raise RuntimeError(
            "invalid oha JSON %s (%s %s c=%s): %s; stderr=%s"
            % (out, raw_label, payload, conc, e, stderr[-2000:])
        )

    errors = data.get("errorDistribution") or {}
    statuses = data.get("statusCodeDistribution") or {}
    if not statuses:
        raise RuntimeError(
            "oha produced empty statusCodeDistribution in %s (%s %s c=%s); errors=%s; stderr=%s"
            % (out, raw_label, payload, conc, errors, stderr[-2000:])
        )
    deadline_aborts = int(errors.get("aborted due to deadline", 0))
    other_errors = {k: v for k, v in errors.items() if k != "aborted due to deadline" and v}
    try:
        max_deadline_aborts = int(conc)
    except Exception:
        max_deadline_aborts = 0
    if other_errors or deadline_aborts > max_deadline_aborts:
        raise RuntimeError(
            "oha transport errors in %s (%s %s c=%s): errors=%s max_deadline_aborts=%s; stderr=%s"
            % (out, raw_label, payload, conc, errors, max_deadline_aborts, stderr[-2000:])
        )

    lp = data.get("latencyPercentiles") or {}
    summary = data.get("summary") or {}
    for k in ("p50", "p99"):
        if not isinstance(lp.get(k), (int, float)):
            raise RuntimeError("oha missing numeric latencyPercentiles.%s in %s: %s" % (k, out, lp))
    if not isinstance(summary.get("requestsPerSec"), (int, float)):
        raise RuntimeError("oha missing numeric summary.requestsPerSec in %s: %s" % (out, summary))
    return data


def oha_run(url, payload, conc, out, raw_label, rate=None):
    """Warmup rồi đo 1 run; ghi raw JSON vào out; trả (p50_ms, p99_ms, rps, non200)."""
    auth = "Authorization: Bearer " + KEY
    body = str(REPO / "benchmarks/payloads" / (payload + ".json"))
    warm = [
        "oha", "-z", WARM, "-c", str(conc), "-m", "POST", "--no-tui",
        "-H", "Content-Type: application/json", "-H", auth,
        "-D", body, url + "/v1/chat/completions",
    ]
    if rate is not None:
        warm[1:1] = ["-q", str(rate)]
    subprocess.run(warm, capture_output=True, text=True)

    cmd = [
        "oha", "-z", DUR, "-c", str(conc), "-m", "POST", "--no-tui", "--latency-correction",
        "-H", "Content-Type: application/json", "-H", auth,
        "-D", body, "-o", out, "--output-format", "json",
        url + "/v1/chat/completions",
    ]
    if rate is not None:
        cmd[1:1] = ["-q", str(rate)]
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError("oha failed (%s %s c=%s): %s" % (raw_label, payload, conc, p.stderr[-2000:]))
    data = parse_oha_json(out, raw_label, payload, conc, p.stderr)
    lp = data["latencyPercentiles"]
    non200 = sum(v for k, v in data.get("statusCodeDistribution", {}).items() if k != "200")
    return lp["p50"] * 1000.0, lp["p99"] * 1000.0, data["summary"]["requestsPerSec"], non200


def ttfb(url, payload):
    """TTFB streaming tuần tự (curl 30 mẫu), median ms. oha không đo được TTFB byte đầu."""
    vals = []
    for _ in range(30):
        p = subprocess.run(
            ["curl", "-s", "-o", "/dev/null", "-N", "-w", "%{time_starttransfer}",
             "-H", "Content-Type: application/json",
             "-H", "Authorization: Bearer " + KEY,
             "--data-binary", "@" + str(REPO / "benchmarks/payloads" / (payload + ".json")),
             url + "/v1/chat/completions"],
            capture_output=True, text=True,
        )
        try:
            vals.append(float(p.stdout.strip()) * 1000.0)
        except ValueError:
            pass
    if not vals:
        raise RuntimeError("curl TTFB collected no samples for %s" % payload)
    return median(vals)


def wait_ready(url, path):
    for _ in range(200):
        try:
            if requests.get(url + path, timeout=1).status_code == 200:
                return
        except Exception:
            pass
        time.sleep(0.1)
    raise RuntimeError("%s%s never became ready" % (url, path))


def scrape_metrics(url, outdir):
    try:
        text = requests.get(url + "/metrics", timeout=2).text
    except Exception as e:
        text = "# scrape_failed: %s\n" % e
    (outdir / "router-metrics.txt").write_text(text)
    return text


def prometheus_counter_total(text, name):
    total = 0.0
    for line in text.splitlines():
        if line.startswith("#") or not line.startswith(name):
            continue
        parts = line.split()
        if len(parts) >= 2:
            try:
                total += float(parts[-1])
            except ValueError:
                pass
    return total


def main():
    outdir = REPO / "bench/results" / time.strftime("%Y%m%d-%H%M%S")
    outdir.mkdir(parents=True, exist_ok=True)
    host = host_info()
    mock_port = int(os.environ.get("MOCK_PORT") or free_tcp_port())
    router_port = int(os.environ.get("ROUTER_PORT") or free_tcp_port())
    mock_url = "http://127.0.0.1:%d" % mock_port
    router_url = "http://127.0.0.1:%d" % router_port
    print("host:", host)
    print("mock_url:", mock_url, "router_url:", router_url)

    # Build release: router + mock (mock giờ là bin trong root crate).
    sh("cargo", "build", "--release", "--locked", cwd=REPO)

    # Postgres + migrate + seed (backends/mock-model/teams/api_keys).
    pg = "brighto_bench_pg_%d" % os.getpid()
    sh("docker", "run", "--rm", "-d", "--name", pg,
       "-e", "POSTGRES_DB=brighto_router", "-e", "POSTGRES_USER=brighto_router",
       "-e", "POSTGRES_PASSWORD=brighto_router_dev", "-p", "127.0.0.1::5432",
       "postgres:16-alpine")
    try:
        port = sh("docker", "port", pg, "5432/tcp").stdout.strip().split(":")[-1]
        db = "postgres://brighto_router:brighto_router_dev@127.0.0.1:%s/brighto_router" % port
        for _ in range(60):
            r = subprocess.run(["docker", "exec", pg, "pg_isready", "-U", "brighto_router", "-d", "brighto_router"],
                               capture_output=True)
            if r.returncode == 0:
                break
            time.sleep(1)

        sh("sqlx", "migrate", "run", "--source", str(REPO / "migrations"),
           env=dict(os.environ, DATABASE_URL=db))
        kh = hashlib.sha256(KEY.encode()).hexdigest()
        sql = f"""
INSERT INTO backends (id,name,base_url,api_key_ref,weight,max_inflight,format,enabled)
VALUES (1,'mock','{mock_url}','MOCK_KEY',1,0,'openai',TRUE);
INSERT INTO model_routes (model_name,backend_ids,fallback_backend_id,chars_per_token,first_byte_timeout)
VALUES ('{MODEL}','[1]',NULL,4.0,180);
INSERT INTO teams (id,name,budget,enabled)
VALUES (1,'team','{{"period":"month","max_tokens":100000000000000,"per_model":{{}}}}',TRUE);
INSERT INTO api_keys (id,key_hash,key_prefix,team_id,owner,allowed_models,budget,rpm_limit,concurrency_limit,expires_at,enabled)
VALUES (1,'{kh}','bench-key',1,'bench','[]',NULL,NULL,NULL,NULL,TRUE);
"""
        sh("psql", "-h", "127.0.0.1", "-p", port, "-U", "brighto_router", "-d", "brighto_router", "-q", "-c", sql,
           env=dict(os.environ, PGPASSWORD="brighto_router_dev"))

        sh("python3", str(REPO / "benchmarks/make_payloads.py"), MODEL)

        mock_log = open(outdir / "mock.log", "wb")
        router_log = open(outdir / "router.log", "wb")
        mock = subprocess.Popen(
            [str(REPO / "target/release/brighto-router-mock")],
            env=dict(os.environ, MOCK_ADDR="127.0.0.1:%d" % mock_port),
            stdout=mock_log,
            stderr=subprocess.STDOUT,
        )
        router = subprocess.Popen(
            [str(REPO / "target/release/brighto-router")],
            env=dict(os.environ, DATABASE_URL=db, LISTEN_ADDR="127.0.0.1:%d" % router_port,
                     ADMIN_MASTER_KEY=ADMIN_KEY, MOCK_KEY="bench-mock-key", RUST_LOG="error"),
            stdout=router_log,
            stderr=subprocess.STDOUT,
        )
        try:
            wait_ready(mock_url, "/health")
            wait_ready(router_url, "/healthz")
            wait_ready(router_url, "/readyz")

            overhead = {}
            gates = []
            any_fail = False

            # B1/B2: overhead non-stream ở mọi conc; ngưỡng chính thức áp cho conc=50.
            for p in PAYLOADS:
                overhead[p] = {}
                for conc in CONCS:
                    d50s, d99s = [], []
                    for i in range(1, RUNS + 1):
                        d_out = str(outdir / ("direct-%s-c%s-r%d.json" % (p, conc, i)))
                        r_out = str(outdir / ("router-%s-c%s-r%d.json" % (p, conc, i)))
                        dp50, dp99, _, _ = oha_run(mock_url, p, conc, d_out, "direct", rate=latency_rate(p, conc))
                        rp50, rp99, _, rn = oha_run(router_url, p, conc, r_out, "router", rate=latency_rate(p, conc))
                        if rp50 is None or dp50 is None:
                            raise RuntimeError(
                                "oha got zero responses (no latency) for %s c=%s — router/direct unhealthy"
                                % (p, conc)
                            )
                        if rn:
                            print("WARN non-200 via router: %s c=%s non200=%s" % (p, conc, rn))
                        d50s.append(rp50 - dp50)
                        d99s.append(rp99 - dp99)
                    ov50, ov99 = median(d50s), median(d99s)
                    overhead[p][conc] = {"p50_median_ms": ov50, "p99_median_ms": ov99,
                                         "runs_p50": [round(x, 3) for x in d50s],
                                         "runs_p99": [round(x, 3) for x in d99s]}
                    print("%s c=%s overhead p50 %+.3fms p99 %+.3fms" % (p, conc, ov50, ov99))
                    if conc == "50":
                        for gid, val, thr, runs in (
                            ("B1", ov50, B1_P50[p], d50s),
                            ("B2", ov99, B2_P99[p], d99s),
                        ):
                            gate = percentile_gate(gid, p, 50, val, thr, runs)
                            any_fail |= not gate["pass"]
                            gates.append(gate)

            # B3: p50 overhead must stay flat from 1k to 200k at canonical conc=50.
            if all(payload in overhead and "50" in overhead[payload] for payload in ("1k", "200k")):
                one = overhead["1k"]["50"]
                large = overhead["200k"]["50"]
                flat_delta = round(large["p50_median_ms"] - one["p50_median_ms"], 3)
                flat_runs = [
                    round(b - a, 3)
                    for a, b in zip(one["runs_p50"], large["runs_p50"])
                ]
                worst = max(flat_runs) if flat_runs else flat_delta
                worst_threshold = round(B3_FLAT_P50_DELTA_MS * WORST_RUN_TOLERANCE, 3)
                ok = flat_delta <= B3_FLAT_P50_DELTA_MS and worst <= worst_threshold
                any_fail |= not ok
                gates.append({
                    "id": "B3",
                    "payload": "200k_vs_1k",
                    "conc": 50,
                    "value": flat_delta,
                    "threshold": B3_FLAT_P50_DELTA_MS,
                    "pass": ok,
                    "runs": flat_runs,
                    "worst": worst,
                    "worst_threshold": worst_threshold,
                })

            # B4: delta TTFB streaming (median 30 mẫu).
            ttfb_delta = {}
            for p in STREAM_PAYLOADS:
                d = ttfb(mock_url, p)
                r = ttfb(router_url, p)
                delta = round(r - d, 3)
                key = p.replace("-stream", "")
                ttfb_delta[key] = delta
                ok = delta <= B4_TTFB[key]
                any_fail |= not ok
                gates.append({"id": "B4", "payload": key, "conc": "curl30", "value": delta,
                              "threshold": B4_TTFB[key], "pass": ok, "runs": [delta]})
                print("B4 %s ttfb delta %+.3fms (direct %.3f / router %.3f)" % (key, delta, d, r))

            # B6: target-rate sustained throughput (conc 200 thật), 1k non-stream.
            b6_saturation = None
            if RUN_B6:
                sat_out = str(outdir / "router-sat-c200.json")
                _, _, rps, non200 = oha_run(router_url, "1k", "200", sat_out, "sat", rate=B6_TARGET_RPS)
                ok = (rps >= B6_MIN_RPS) and (non200 <= B6_MAX_NON200)
                any_fail |= not ok
                gates.append({"id": "B6", "payload": "1k", "conc": 200, "value": round(rps, 2),
                              "threshold": B6_MIN_RPS, "pass": ok, "runs": [round(rps, 2)]})
                gates.append({"id": "B6", "payload": "1k", "conc": 200, "value": non200,
                              "threshold": B6_MAX_NON200, "pass": non200 <= B6_MAX_NON200, "runs": [non200]})
                b6_saturation = {"target_rps": B6_TARGET_RPS, "rps": round(rps, 2), "non200": non200}
                print("B6 sat rps %.2f non200 %d" % (rps, non200))
            else:
                print("B6 skipped (BENCH_B6=0)")

            metrics_text = scrape_metrics(router_url, outdir)
            ledger_drops = prometheus_counter_total(metrics_text, "router_ledger_dropped_total")
            ok_ledger = ledger_drops == 0
            any_fail |= not ok_ledger
            gates.append({"id": "INTERNAL_LEDGER_DROPS", "payload": "all", "conc": "all", "value": ledger_drops,
                          "threshold": 0, "pass": ok_ledger, "runs": [ledger_drops],
                          "note": "internal safety check only; true B10 ledger-lag gate is tracked in swarm/audits"})

            summary = {
                "sha": host["sha"], "host": {"cpu": host["cpu"], "kernel": host["kernel"]},
                "knobs": {"DUR": DUR, "WARM": WARM, "RUNS": RUNS, "CONCS": CONCS,
                          "PAYLOADS": PAYLOADS, "STREAM_PAYLOADS": STREAM_PAYLOADS, "RUN_B6": RUN_B6,
                          "ADMIN_MASTER_KEY_set": bool(ADMIN_KEY), "B6_TARGET_RPS": B6_TARGET_RPS,
                          "BENCH_TARGET_RPS": BENCH_TARGET_RPS,
                          "MOCK_PORT": mock_port, "ROUTER_PORT": router_port},
                "overhead_ms": overhead,
                "streaming_ttfb_delta_ms": ttfb_delta,
                "b6_saturation": b6_saturation,
                "ledger_dropped_total": ledger_drops,
            }
            (outdir / "summary.json").write_text(json.dumps(summary, indent=2))

            gate = {"sha": host["sha"], "host": {"cpu": host["cpu"], "kernel": host["kernel"]},
                    "knobs": {"DUR": DUR, "WARM": WARM, "RUNS": RUNS, "CONCS": CONCS,
                              "PAYLOADS": PAYLOADS, "STREAM_PAYLOADS": STREAM_PAYLOADS, "RUN_B6": RUN_B6,
                              "B6_TARGET_RPS": B6_TARGET_RPS, "BENCH_TARGET_RPS": BENCH_TARGET_RPS,
                              "MOCK_PORT": mock_port, "ROUTER_PORT": router_port},
                    "gates": gates, "pass": not any_fail}
            (outdir / "gate.json").write_text(json.dumps(gate, indent=2))
            print("artifacts:", outdir)
            print("gate pass:", not any_fail)

            if REQUIRE_PASS and any_fail:
                sys.exit(1)
        finally:
            router.terminate()
            mock.terminate()
            try:
                router.wait(timeout=5)
            except subprocess.TimeoutExpired:
                router.kill()
            try:
                mock.wait(timeout=5)
            except subprocess.TimeoutExpired:
                mock.kill()
            mock_log.close()
            router_log.close()
    finally:
        subprocess.run(["docker", "rm", "-f", pg], capture_output=True)


if __name__ == "__main__":
    main()
