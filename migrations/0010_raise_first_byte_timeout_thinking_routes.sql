-- Raise first_byte_timeout for the local vLLM thinking routes.
--
-- Audit: medlc_s1/audits/2026-11-01_1306_claude_router_first_byte_timeout_deepseek_route.md
-- Non-streaming high-effort thinking requests to the local vLLM exceed the old
-- 180 s and the router answered HTTP 504 first byte timeout (29 of 34 audit
-- errors against deepseek-v41-flash). 900 s matches the client timeout the
-- synth uses; streaming is unaffected because the first byte arrives early.
--
-- The router polls model_routes every 5 s, so no restart is needed.

UPDATE model_routes
SET first_byte_timeout = 900
WHERE model_name IN (
    'deepseek-v41-flash',
    'Qwen/Qwen3.8-Flash-Next-NVFP4',
    'Qwen3.8-27B-Uncensored-NVFP4'
);
