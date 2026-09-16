-- Measure true BENCHMARK.md B10 ledger lag without changing the request hot path.
-- completed_at_ms is captured when the request finishes; inserted_at_ms is assigned by PostgreSQL.

ALTER TABLE usage_ledger
    ADD COLUMN IF NOT EXISTS completed_at_ms BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS inserted_at_ms BIGINT NOT NULL DEFAULT (FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT);

CREATE INDEX IF NOT EXISTS idx_usage_completed_inserted_ms
    ON usage_ledger(completed_at_ms, inserted_at_ms);
