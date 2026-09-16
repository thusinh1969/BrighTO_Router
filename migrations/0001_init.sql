-- BrighTO-Router production schema (PostgreSQL 16). Hot path không bao giờ đụng DB này.
-- JSON-shaped fields giữ TEXT (backend_ids, allowed_models, budget) để sqlx::Any không cần
-- branching array/JSONB — loader/admin parse JSON text như nhau trên cả hai dialect.

CREATE TABLE IF NOT EXISTS backends (
    id            BIGSERIAL PRIMARY KEY,
    name          TEXT NOT NULL,
    base_url      TEXT NOT NULL,
    api_key_ref   TEXT NOT NULL,                              -- env:NAME | file:/path
    weight        BIGINT NOT NULL DEFAULT 1 CHECK (weight >= 1),
    max_inflight  BIGINT NOT NULL DEFAULT 0,                  -- 0 = không giới hạn
    format        TEXT NOT NULL CHECK (format IN ('openai','anthropic')),
    enabled       BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS model_routes (
    model_name          TEXT PRIMARY KEY,
    backend_ids         TEXT NOT NULL DEFAULT '[]',           -- JSON array text
    fallback_backend_id BIGINT,
    chars_per_token     DOUBLE PRECISION NOT NULL DEFAULT 4.0,
    first_byte_timeout  BIGINT NOT NULL DEFAULT 180           -- giây
);

CREATE TABLE IF NOT EXISTS teams (
    id       BIGSERIAL PRIMARY KEY,
    name     TEXT NOT NULL,
    budget   TEXT,                                            -- JSON text
    enabled  BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS api_keys (
    id                 BIGSERIAL PRIMARY KEY,
    key_hash           TEXT NOT NULL,                         -- SHA-256 hex 64 ký tự
    key_prefix         TEXT NOT NULL,                         -- 8 ký tự đầu
    team_id            BIGINT NOT NULL REFERENCES teams(id),
    owner              TEXT NOT NULL,
    allowed_models     TEXT NOT NULL DEFAULT '[]',            -- JSON array text
    budget             TEXT,                                  -- JSON text
    rpm_limit          BIGINT,
    concurrency_limit  BIGINT,
    expires_at         BIGINT,                                -- unix epoch seconds
    enabled            BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS usage_ledger (
    ts                  BIGINT NOT NULL,                      -- epoch seconds
    request_id          TEXT NOT NULL,                        -- UNIQUE -> replay idempotent
    key_id              BIGINT NOT NULL,
    team_id             BIGINT NOT NULL,
    model               TEXT NOT NULL,
    backend_id          BIGINT NOT NULL,
    status              BIGINT NOT NULL,
    input_tokens        BIGINT NOT NULL,
    output_tokens       BIGINT NOT NULL,
    estimated           BOOLEAN NOT NULL,
    ttfb_ms             BIGINT NOT NULL,
    total_ms            BIGINT NOT NULL,
    router_overhead_ms  BIGINT NOT NULL,
    stream              BOOLEAN NOT NULL,
    client_aborted      BOOLEAN NOT NULL,
    error_class         TEXT
);

CREATE INDEX IF NOT EXISTS idx_usage_team_ts ON usage_ledger(team_id, ts);
CREATE INDEX IF NOT EXISTS idx_usage_key_ts ON usage_ledger(key_id, ts);
CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_request_id ON usage_ledger(request_id);

-- KHÔNG seed key mặc định vào production (tránh default credential). Admin API tạo key lúc chạy.
