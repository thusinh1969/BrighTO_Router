-- Default BrighTO-Router provider templates.
-- Safe to rerun. Inserts missing defaults only; never overwrites user-edited providers.

DO $$
DECLARE
  item jsonb;
  providers jsonb := '[
    {"name":"openai", "base_url":"https://api.openai.com", "api_key_ref":"env:OPENAI_API_KEY", "format":"openai"},
    {"name":"anthropic", "base_url":"https://api.anthropic.com", "api_key_ref":"env:ANTHROPIC_API_KEY", "format":"anthropic"},
    {"name":"gemini", "base_url":"https://generativelanguage.googleapis.com/v1beta/openai", "api_key_ref":"env:GEMINI_API_KEY", "format":"openai"},
    {"name":"deepseek", "base_url":"https://api.deepseek.com", "api_key_ref":"env:DEEPSEEK_API_KEY", "format":"openai"},
    {"name":"kimi", "base_url":"https://api.moonshot.ai/v1", "api_key_ref":"env:KIMI_API_KEY", "format":"openai"},
    {"name":"qwen", "base_url":"https://dashscope-intl.aliyuncs.com/compatible-mode/v1", "api_key_ref":"env:QWEN_API_KEY", "format":"openai"},
    {"name":"zai", "base_url":"https://api.z.ai/api/paas/v4", "api_key_ref":"env:ZAI_API_KEY", "format":"openai"},
    {"name":"openrouter", "base_url":"https://openrouter.ai/api/v1", "api_key_ref":"env:OPENROUTER_API_KEY", "format":"openai"},
    {"name":"jina", "base_url":"https://api.jina.ai", "api_key_ref":"env:JINA_API_KEY", "format":"openai"},
    {"name":"voyage", "base_url":"https://api.voyageai.com", "api_key_ref":"env:VOYAGE_API_KEY", "format":"openai"},
    {"name":"cohere", "base_url":"https://api.cohere.com/v2", "api_key_ref":"env:COHERE_API_KEY", "format":"openai"},
    {"name":"qwen-rerank", "base_url":"https://dashscope-intl.aliyuncs.com", "api_key_ref":"env:QWEN_API_KEY", "format":"openai"},
    {"name":"meta-muse", "base_url":"https://api.meta.ai/v1", "api_key_ref":"env:META_MUSE_API_KEY", "format":"openai"},
    {"name":"custom-llm", "base_url":"http://127.0.0.1:8088/v1", "api_key_ref":"env:CUSTOM_LLM_API_KEY", "format":"openai"}
  ]'::jsonb;
BEGIN
  INSERT INTO teams (name, budget, enabled)
  SELECT 'Default Team', '{"period":"month","max_tokens":1000000,"per_model":{}}', TRUE
  WHERE NOT EXISTS (SELECT 1 FROM teams WHERE name = 'Default Team');

  -- Provider endpoints are templates only. They stay disabled until a tested model route uses them.
  -- The Add Model wizard supports Chat, Embedding, Rerank, and ASR task types and creates/reuses
  -- these endpoints when the selected provider/model/key passes Test Connection.

  -- Local demo client key for first-run smoke tests and ./test_router.py.
  -- Plaintext: lc-0123456789abcdef0123456789abcdef
  -- Safe to rerun; disable or delete before shared/production use.
  INSERT INTO api_keys (key_hash, key_prefix, team_id, owner, allowed_models, budget, rpm_limit, concurrency_limit, expires_at, enabled, key_secret)
  SELECT
    '4aa892925e0be64d70cad871e803913056f1a9be0c9f06cb558870a8f2e347a4',
    'lc-01234',
    t.id,
    'Local demo key',
    '[]',
    NULL,
    NULL,
    NULL,
    NULL,
    TRUE,
    'lc-0123456789abcdef0123456789abcdef'
  FROM (SELECT id FROM teams WHERE name = 'Default Team' ORDER BY id LIMIT 1) t
  WHERE NOT EXISTS (
    SELECT 1 FROM api_keys WHERE key_hash = '4aa892925e0be64d70cad871e803913056f1a9be0c9f06cb558870a8f2e347a4'
  );

  FOR item IN SELECT * FROM jsonb_array_elements(providers) LOOP
    IF NOT EXISTS (SELECT 1 FROM backends WHERE name = item->>'name') THEN
      INSERT INTO backends (name, base_url, api_key_ref, weight, max_inflight, format, enabled)
      VALUES (item->>'name', item->>'base_url', item->>'api_key_ref', 1, 0, item->>'format', FALSE);
    END IF;
  END LOOP;
END $$;
