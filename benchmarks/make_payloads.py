#!/usr/bin/env python3
"""Generate deterministic chat/completions payloads for benchmark runs.

Token count is approximate: 4 ASCII characters ~= 1 token. Real token counts come
from upstream usage fields when the router records ledger entries.
"""
import json
import pathlib
import sys

model = sys.argv[1] if len(sys.argv) > 1 else "mock-model"
out = pathlib.Path(__file__).parent / "payloads"
out.mkdir(exist_ok=True)
para = (
    "The pharmacy dispensing system records every prescription line with drug code, dose, "
    "frequency and duration, then checks interactions against the patient's active medication list. "
)

PAYLOADS = [
    ("1k", 1_000),
    ("50k", 50_000),
    ("200k", 200_000),
    ("500k", 500_000),
    ("1m", 1_000_000),
]

for name, tokens in PAYLOADS:
    text = (para * (tokens * 4 // len(para) + 1))[: tokens * 4]
    for stream in (False, True):
        body = {
            "model": model,
            "stream": stream,
            "max_tokens": 64,
            "temperature": 0,
            "messages": [
                {"role": "system", "content": "Summarize in one sentence."},
                {"role": "user", "content": text},
            ],
        }
        if stream:
            body["stream_options"] = {"include_usage": True}
        path = out / f"{name}{'-stream' if stream else ''}.json"
        path.write_text(json.dumps(body, ensure_ascii=True))
        print(f"{path.name:18} {path.stat().st_size / 1024:9.0f} KB")
