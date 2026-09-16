#!/usr/bin/env python3
"""Sinh payload chat/completions ~1K, ~50K, ~200K token để bench.
Ước lượng 4 ký tự ASCII ≈ 1 token với Qwen/Gemma tokenizer; số thật lấy từ usage vLLM trả về."""
import json, sys, pathlib
model = sys.argv[1] if len(sys.argv) > 1 else "qwen3.8-27b"
out = pathlib.Path(__file__).parent / "payloads"; out.mkdir(exist_ok=True)
para = ("The pharmacy dispensing system records every prescription line with drug code, dose, "
        "frequency and duration, then checks interactions against the patient's active medication list. ")
for name, tokens in [("1k", 1_000), ("50k", 50_000), ("200k", 200_000)]:
    text = (para * (tokens * 4 // len(para) + 1))[: tokens * 4]
    for stream in (False, True):
        body = {"model": model, "stream": stream, "max_tokens": 64, "temperature": 0,
                "messages": [{"role": "system", "content": "Summarize in one sentence."},
                             {"role": "user", "content": text}]}
        if stream:
            body["stream_options"] = {"include_usage": True}
        p = out / f"{name}{'-stream' if stream else ''}.json"
        p.write_text(json.dumps(body, ensure_ascii=True))
        print(f"{p.name:16} {p.stat().st_size/1024:8.0f} KB")
