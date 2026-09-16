#!/usr/bin/env python3
"""Swarm driver — goi DeepSeek V4 Pro song song (toi da 4 call), moi agent nhan:
  system = luat 00_orchestrator + contract.rs + stub file cua no + intent doc cua no.
Output cua moi agent luu vao swarm/out/<agent>.response.md (nguyen van, de truy vet).
Chi dung python stdlib (khong them dependency)."""
import json, os, sys, time, urllib.request, concurrent.futures

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
KEY = open(os.path.join(BASE, ".deepseek_key")).read().strip()
API = "https://api.deepseek.com/v1/chat/completions"
MODEL = "deepseek-v4-pro"

def read(p):
    fp = os.path.join(BASE, p)
    if not os.path.exists(fp):
        return "(FILE MOI — chua ton tai, ban tao moi toan bo noi dung)"
    return open(fp).read()

def call_agent(name, doc, files, max_tokens=48000):
    system = (
        "Ban la agent " + name + " trong swarm build BrigTO Router (Rust).\n\n"
        "=== LUAT CHOI ===\n" + read("docs/agents/00_orchestrator.md") + "\n\n"
        "=== CONTRACT (src/contract.rs — CHUYEN DOI, chi doc) ===\n```rust\n" + read("src/contract.rs") + "\n```\n\n"
        + "".join("=== FILE CUA BAN: " + f + " (ban ghi lai TOAN BO file nay) ===\n```rust\n" + read(f) + "\n```\n\n" for f in files)
        + "=== NHIEM VU CUA BAN ===\n" + read(doc) + "\n\n"
        "=== FORMAT TRA LOI (TUYET DOI TUAN THU) ===\n"
        "Tra ve DUNG cac code block, moi block mo dau bang dong `// FILE: <duong-dan>` roi den toan bo noi dung file.\n"
        "Khong giai thich ngoai le, khong summary. Code phai tu bien dich (Rust 2024, deps nhu trong contract).\n"
        "Test unit dat trong `#[cfg(test)] mod tests` cung file hoac tests/<agent>_test.rs (neu duoc phep o muc Owned).\n"
    )
    body = {"model": MODEL, "messages": [{"role":"system","content":system},
            {"role":"user","content":"Thuc hiem. Tra DUNG code block theo format."}],
           "max_tokens": max_tokens, "temperature": 0.2}
    for attempt in range(3):
        try:
            req = urllib.request.Request(API, data=json.dumps(body).encode(),
                                         headers={"Content-Type":"application/json","Authorization":"Bearer "+KEY})
            t0 = time.time()
            r = json.loads(urllib.request.urlopen(req, timeout=900).read())
            out = r["choices"][0]["message"]["content"]
            os.makedirs(os.path.join(BASE,"swarm/out"), exist_ok=True)
            with open(os.path.join(BASE,"swarm/out",name+".response.md"),"w") as f:
                f.write(out)
            return name, f"OK {len(out)} chars, {time.time()-t0:.0f}s"
        except Exception as e:
            if attempt == 2: return name, f"FAIL {type(e).__name__} {str(e)[:200]}"
            time.sleep(10)

if __name__ == "__main__":
    agents = json.load(open(sys.argv[1]))  # [{"name":"a1_config","doc":"docs/agents/a1_config.md","files":["src/config/mod.rs"]}, ...]
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as ex:
        for name, res in ex.map(lambda a: call_agent(a["name"], a["doc"], a["files"], a.get("max_tokens", 48000)), agents):
            print(name, "->", res, flush=True)
