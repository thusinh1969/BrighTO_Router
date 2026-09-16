-- Provider = template (no secret). Model Route = owns the provider credential.
-- Route-level credential: provider_key_ref (file:/... or env:...) + auth_mode (bearer|anthropic|none).
-- NULL/empty provider_key_ref chỉ hợp lệ khi auth_mode = 'none' (local llama.cpp/vLLM/Ollama).
ALTER TABLE model_routes
    ADD COLUMN IF NOT EXISTS provider_key_ref TEXT,
    ADD COLUMN IF NOT EXISTS auth_mode TEXT NOT NULL DEFAULT 'bearer';
