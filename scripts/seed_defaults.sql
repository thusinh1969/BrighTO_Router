-- Default BrighTO-Router provider templates.
-- Safe to rerun. It updates URL/key-ref/format for known provider names and keeps enabled as-is.

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
    {"name":"zai", "base_url":"https://api.z.ai/api/coding/paas/v4", "api_key_ref":"env:ZAI_API_KEY", "format":"openai"},
    {"name":"openrouter", "base_url":"https://openrouter.ai/api/v1", "api_key_ref":"env:OPENROUTER_API_KEY", "format":"openai"},
    {"name":"meta-muse", "base_url":"https://api.meta.ai/v1", "api_key_ref":"env:META_MUSE_API_KEY", "format":"openai"},
    {"name":"custom-openai", "base_url":"http://127.0.0.1:8000/v1", "api_key_ref":"env:CUSTOM_LLM_API_KEY", "format":"openai"}
  ]'::jsonb;
BEGIN
  INSERT INTO teams (name, budget, enabled)
  SELECT 'Default Team', '{"period":"month","max_tokens":1000000,"per_model":{}}', TRUE
  WHERE NOT EXISTS (SELECT 1 FROM teams WHERE name = 'Default Team');

  FOR item IN SELECT * FROM jsonb_array_elements(providers) LOOP
    IF EXISTS (SELECT 1 FROM backends WHERE name = item->>'name') THEN
      UPDATE backends
      SET base_url = item->>'base_url',
          api_key_ref = item->>'api_key_ref',
          format = item->>'format'
      WHERE id = (SELECT id FROM backends WHERE name = item->>'name' ORDER BY id LIMIT 1);
    ELSE
      INSERT INTO backends (name, base_url, api_key_ref, weight, max_inflight, format, enabled)
      VALUES (item->>'name', item->>'base_url', item->>'api_key_ref', 1, 0, item->>'format', FALSE);
    END IF;
  END LOOP;
END $$;
