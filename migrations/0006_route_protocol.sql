-- Route-level provider protocol (CODEX provider-protocol taxonomy).
-- protocol = openai_chat | openai_completions | openai_embeddings | openai_rerank
--          | cohere_rerank | voyage_rerank | jina_rerank | openai_audio_transcriptions
--          | anthropic_messages | local_openai_chat | custom_openai_chat
-- Xác định endpoint client gọi + shape upstream. Tách khỏi auth_mode.
-- Backward-compat: route cũ mặc định 'openai_chat' (OpenAI Chat Completions).
ALTER TABLE model_routes
    ADD COLUMN IF NOT EXISTS protocol TEXT NOT NULL DEFAULT 'openai_chat';

-- Backward-compat: route có backend chính format 'anthropic' -> anthropic_messages
-- (trước đây format anthropic ở backend + auth_mode=anthropic ngụ ý Anthropic Messages).
UPDATE model_routes r
SET protocol = 'anthropic_messages'
FROM backends b
WHERE (r.backend_ids::jsonb ->> 0)::bigint = b.id
  AND b.format = 'anthropic'
  AND r.protocol = 'openai_chat';
