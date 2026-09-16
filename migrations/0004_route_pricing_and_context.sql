-- Route pricing + context cho admin clarity + cost display (CODEX-PORTAL-PRODUCT-GAP-AUDIT).
-- provider_model_name: tên model thật ở provider (vd "gpt-4o-mini"); context/max_output: token;
-- price_*_per_mtok_usd: USD cho 1 triệu token. enabled: route on/off.
ALTER TABLE model_routes
    ADD COLUMN IF NOT EXISTS provider_model_name TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS context_tokens BIGINT,
    ADD COLUMN IF NOT EXISTS max_output_tokens BIGINT,
    ADD COLUMN IF NOT EXISTS price_input_per_mtok_usd DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS price_output_per_mtok_usd DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT TRUE;
