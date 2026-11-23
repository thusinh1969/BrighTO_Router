-- Admin read paths aggregate over the whole usage_ledger.
--
-- load_lifecycle_data() runs
--     SELECT backend_id, model, COUNT(*) FROM usage_ledger GROUP BY backend_id, model
-- on EVERY admin request (backends, routes, teams, keys all call it), so the whole
-- admin API was paying a sequential scan over millions of rows (~1.2 s each).
-- /admin/summary additionally paid three percentile_cont() sorts (~30-40 s).
--
-- These are plain CREATE INDEX statements (transactional), so they apply cleanly
-- through the normal migration runner and sqlx::test.
--
-- PRODUCTION NOTE: on a large existing table build them OUTSIDE a transaction with
-- CREATE INDEX CONCURRENTLY so the router is never write-blocked; that is how the
-- live 23.4M-row table was migrated. Afterwards run
--     VACUUM (ANALYZE) usage_ledger;
-- or the visibility map is missing and Postgres keeps fetching heap pages, leaving
-- the index-only scan slower than the seq scan it replaced.

-- Covers the per-(backend, model) GROUP BY as an index-only scan.
CREATE INDEX IF NOT EXISTS usage_ledger_backend_model_idx
    ON usage_ledger (backend_id, model);

-- /admin/summary filters every aggregate by a time window.
CREATE INDEX IF NOT EXISTS usage_ledger_ts_idx
    ON usage_ledger (ts);

-- Dashboard default window is the last 30 days; the composite keeps the per-model
-- grouping index-only while still bounding by ts.
CREATE INDEX IF NOT EXISTS usage_ledger_ts_backend_model_idx
    ON usage_ledger (ts, backend_id, model);
