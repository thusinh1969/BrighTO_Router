# Model Group load-balancing smoke

Safe mock smoke for preview-3 Model Groups. It starts three local OpenAI-compatible mock chat endpoints, creates one public model group `coding-fast`, and verifies weighted round-robin routing plus endpoint-specific provider model rewrite.

Run:

```bash
./smoke/model_group/run_mock.sh
```

Expected sequence for weights `DeepSeek=3`, `local llama.cpp=1`, `OpenAI=2`:

```text
deepseek-v4-pro
deepseek-v4-pro
deepseek-v4-pro
qwen3.8-flash-next
gpt-5.6-mini
gpt-5.6-mini
```

This smoke measures router/load-balancer path latency against mock endpoints. It does not measure paid model inference speed.
