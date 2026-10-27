//! Upgrade-path tests for migration 0008 (`accept_anthropic_messages`).
//!
//! The normal `sqlx::test(migrations = "./migrations")` tests prove a fresh
//! schema. These tests drop the new column to simulate a v1.2.0 database,
//! insert existing routes, then apply the new migration and prove the upgrade
//! is additive, backfills `false`, preserves data, is idempotent, and still
//! allows the old explicit-column query used by v1.2.0.

use sqlx::postgres::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn migration_0008_is_boolean_not_null_default_false_on_fresh_schema(pool: PgPool) {
    let (data_type, is_nullable, default): (String, String, Option<String>) = sqlx::query_as(
        "SELECT data_type, is_nullable, column_default \
         FROM information_schema.columns \
         WHERE table_name = 'model_routes' AND column_name = 'accept_anthropic_messages'",
    )
    .fetch_one(&pool)
    .await
    .expect("accept_anthropic_messages column must exist");
    assert_eq!(data_type, "boolean");
    assert_eq!(is_nullable, "NO");
    assert!(
        default
            .as_deref()
            .is_some_and(|value| value.contains("false")),
        "unexpected default: {default:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn migration_0008_upgrades_existing_v12_rows_and_is_idempotent(pool: PgPool) {
    // Simulate a v1.2.0 database that has 0001..0007 but not 0008.
    sqlx::query("ALTER TABLE model_routes DROP COLUMN accept_anthropic_messages")
        .execute(&pool)
        .await
        .expect("drop post-0007 column");
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'model_routes' AND column_name = 'accept_anthropic_messages'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(exists.is_none(), "test must start without the new column");

    sqlx::query(
        "INSERT INTO backends \
         (id, name, base_url, api_key_ref, weight, max_inflight, format, enabled) VALUES \
         (1, 'openai-upgrade', 'http://127.0.0.1:9001/v1', 'env:NONE', 1, 10, 'openai', TRUE), \
         (2, 'anthropic-upgrade', 'http://127.0.0.1:9002', 'env:NONE', 1, 10, 'anthropic', TRUE)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO teams (id, name, budget, enabled) VALUES \
         (1, 'upgrade-team', '{\"period\":\"day\",\"max_tokens\":1000,\"per_model\":{}}', TRUE)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_routes \
         (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, \
          provider_model_name, enabled, auth_mode, protocol, routing_policy) VALUES \
         ('existing-openai', '[1]', NULL, 4.0, 180, 'provider-openai', TRUE, 'none', 'openai_chat', 'least_loaded_weighted'), \
         ('existing-anthropic', '[2]', NULL, 4.0, 180, 'provider-anthropic', TRUE, 'none', 'anthropic_messages', 'least_loaded_weighted')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO api_keys \
         (id, key_hash, key_prefix, team_id, owner, allowed_models, budget, rpm_limit, concurrency_limit, expires_at, enabled) \
         VALUES (1, repeat('a', 64), 'sk-upgra', 1, 'upgrade', '[]', NULL, NULL, NULL, NULL, TRUE)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Apply the real migration file twice. ADD COLUMN IF NOT EXISTS must make
    // the second apply a no-op instead of rewriting/undoing existing values.
    let migration = include_str!("../migrations/0008_route_accept_anthropic_messages.sql");
    sqlx::raw_sql(migration)
        .execute(&pool)
        .await
        .expect("first 0008 apply");
    sqlx::raw_sql(migration)
        .execute(&pool)
        .await
        .expect("second 0008 apply must be idempotent");

    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT model_name, provider_model_name, protocol, auth_mode \
         FROM model_routes ORDER BY model_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "existing-anthropic".to_string(),
                "provider-anthropic".to_string(),
                "anthropic_messages".to_string(),
                "none".to_string()
            ),
            (
                "existing-openai".to_string(),
                "provider-openai".to_string(),
                "openai_chat".to_string(),
                "none".to_string()
            ),
        ],
        "existing route fields must survive the migration"
    );
    let flags: Vec<bool> = sqlx::query_scalar(
        "SELECT accept_anthropic_messages FROM model_routes ORDER BY model_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        flags,
        vec![false, false],
        "existing rows must backfill false"
    );

    // The old v1.2.0 explicit-column query must still work after the additive
    // migration; an old binary is not forced to know the new column.
    let old_query_rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT model_name, backend_ids, routing_policy \
         FROM model_routes ORDER BY model_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(old_query_rows.len(), 2);

    // A new route can opt in, and the already-migrated rows stay false.
    sqlx::query(
        "INSERT INTO model_routes \
         (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, \
          provider_model_name, enabled, auth_mode, protocol, routing_policy, accept_anthropic_messages) \
         VALUES ('new-bridge', '[1]', NULL, 4.0, 180, 'provider-openai', TRUE, 'none', 'openai_chat', 'least_loaded_weighted', TRUE)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let new_flag: bool = sqlx::query_scalar(
        "SELECT accept_anthropic_messages FROM model_routes WHERE model_name = 'new-bridge'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(new_flag);
    let existing_flags: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM model_routes \
         WHERE model_name IN ('existing-openai', 'existing-anthropic') \
           AND accept_anthropic_messages = FALSE",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(existing_flags, 2);

    // The new binary's snapshot loader must read the migrated column for both
    // backfilled and newly created routes.
    let loader = brighto_router::config::DbConfigLoader::new(pool.clone(), 5);
    let snapshot = loader
        .load_snapshot()
        .await
        .expect("load migrated snapshot");
    assert!(
        !snapshot.routes["existing-openai"].accept_anthropic_messages,
        "existing route must remain opt-out after migration"
    );
    assert!(
        !snapshot.routes["existing-anthropic"].accept_anthropic_messages,
        "native Anthropic route must remain opt-out"
    );
    assert!(
        snapshot.routes["new-bridge"].accept_anthropic_messages,
        "new bridge route must opt in"
    );
}
