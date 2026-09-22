-- BrighTO-Router 1.0 Model Group load balancing.
-- model_routes remains the public Model Group table for migration compatibility.

ALTER TABLE model_routes
    ADD COLUMN IF NOT EXISTS routing_policy TEXT NOT NULL DEFAULT 'least_loaded_weighted';

CREATE TABLE IF NOT EXISTS model_route_endpoints (
    model_name          TEXT NOT NULL REFERENCES model_routes(model_name) ON DELETE CASCADE,
    backend_id          BIGINT NOT NULL REFERENCES backends(id) ON DELETE CASCADE,
    provider_model_name TEXT NOT NULL DEFAULT '',
    provider_key_ref    TEXT,
    auth_mode           TEXT NOT NULL DEFAULT 'bearer' CHECK (auth_mode IN ('bearer','anthropic','none')),
    protocol            TEXT NOT NULL DEFAULT 'openai_chat',
    weight              BIGINT NOT NULL DEFAULT 1 CHECK (weight >= 1),
    max_inflight        BIGINT NOT NULL DEFAULT 0 CHECK (max_inflight >= 0),
    enabled             BOOLEAN NOT NULL DEFAULT TRUE,
    PRIMARY KEY (model_name, backend_id)
);

CREATE TABLE IF NOT EXISTS model_route_counters (
    model_name    TEXT PRIMARY KEY REFERENCES model_routes(model_name) ON DELETE CASCADE,
    next_value    BIGINT NOT NULL DEFAULT 0 CHECK (next_value >= 0),
    updated_at_ms BIGINT NOT NULL DEFAULT 0
);
