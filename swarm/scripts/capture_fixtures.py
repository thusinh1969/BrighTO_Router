#!/usr/bin/env python3
"""Capture fixture THẬT từ llama-server 8088 — ground truth cho A4 (tap usage + byte-splice).
Chạy: python3 scripts/capture_fixtures.py   (cần llama-server 8088 sống)"""
import json, time, urllib.request, pathlib
OUT = pathlib.Path(__file__).resolve().parent.parent / "tests" / "fixtures"
BASE = "http://127.0.0.1:8088/v1/chat/completions"

def call(payload):
    req = urllib.request.Request(BASE, data=json.dumps(payload).encode(),
                                 headers={"Content-Type": "application/json"})
    t0 = time.time()
    r = urllib.request.urlopen(req, timeout=300)
    raw = r.read()
    return round(time.time()-t0, 2), raw

def save(name, raw):
    (OUT/name).write_bytes(raw)
    print(f"  saved {name}: {len(raw)} bytes")

big = "The quick brown fox jumps over the lazy dog. " * 4500  # ~50K chars payload
print("capturing from llama-server 8088 (model qwen3.8-flash-next):")
t, raw = call({"model":"x","messages":[{"role":"user","content":"Reply exactly: OK"}],"max_tokens":16,"temperature":0.0})
save("nonstream_small.json", raw)
t, raw = call({"model":"x","messages":[{"role":"user","content":"Count 1 to 10, one per line."}],"stream":True,"max_tokens":40,"temperature":0.0})
save("stream_no_usage_option.sse", raw)   # chứng minh: KHÔNG có stream_options → không có usage chunk
t, raw = call({"model":"x","messages":[{"role":"user","content":"Count 1 to 10, one per line."}],"stream":True,"max_tokens":40,"temperature":0.0,"stream_options":{"include_usage":True}})
save("stream_with_usage.sse", raw)        # usage chunk: choices=[] + timings — parser phải chịu được
t, raw = call({"model":"x","messages":[{"role":"user","content":big+"\nReply exactly: OK"}],"stream":True,"max_tokens":16,"temperature":0.0,"stream_options":{"include_usage":True}})
save("stream_big50k.sse", raw)
print(f"elapsed big: {t}s")
