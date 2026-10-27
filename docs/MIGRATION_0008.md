# Migration 0008: Claude CLI bridge (v1.3.0)

Migration `0008_route_accept_anthropic_messages.sql` is required for the Claude
CLI bridge released in v1.3.0. It adds one nullable-safe column with a non-null default:

```sql
ALTER TABLE model_routes
    ADD COLUMN IF NOT EXISTS accept_anthropic_messages BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN model_routes.accept_anthropic_messages IS
    'OpenAI-chat route also accepts Anthropic Messages at /v1/messages via translation';
```

The migration is additive. On PostgreSQL 16, adding a column with a constant
default is metadata-only; it does not rewrite the table. Existing routes receive
`FALSE`, so **no route starts accepting `/v1/messages` automatically**. The
bridge remains an explicit per-route or per-group opt-in.

## Compatibility

| Component | Before 0008 | After 0008 |
|---|---|---|
| v1.2.0 binary | Works | Works; it ignores the extra column |
| v1.3.0 bridge binary | **Fails** when loading routes/config or listing routes: `column "accept_anthropic_messages" does not exist` | Works |
| Existing routes | Unchanged | `accept_anthropic_messages = FALSE` |
| New routes | N/A | Default `FALSE`; may be saved with `TRUE` on an OpenAI Chat route |

Deployment order for a rolling or single-node upgrade:

1. Back up PostgreSQL.
2. Apply migration 0008.
3. Start the new binary/image.
4. Verify readiness and the route list.
5. Roll traffic/restart the remaining replicas.

Old binaries may keep serving during step 3 because the migration is additive.

## Backup before migrating

`./start.sh upgrade` creates a private backup under `backups/` before changing
the container. For a manual migration, make an explicit dump first:

```bash
mkdir -p backups
docker compose exec -T postgres \
  pg_dump -U "${DB_USER:-brighto_router}" -d "${DB_NAME:-brighto_router}" -Fc \
  > "backups/pre-migration-0008-$(date +%Y%m%d-%H%M%S).dump"
```

Store the dump outside the PostgreSQL volume. Do not commit dumps or `.env`
files to Git.

## Apply

Recommended, using the repository's migration runner:

```bash
./start.sh migrate
```

`./start.sh upgrade` also runs migrations after its backup. For Kubernetes or an
external PostgreSQL database, run migrations against that database before
rolling the new router pods.

If a new image was already started against an unmigrated database, stop it,
apply the migration, and start it again. Do not repeatedly restart it and assume
it will recover.

## Verify

Check the column and its default:

```sql
SELECT column_name, data_type, is_nullable, column_default
FROM information_schema.columns
WHERE table_name = 'model_routes'
  AND column_name = 'accept_anthropic_messages';

SELECT model_name, protocol, accept_anthropic_messages
FROM model_routes
ORDER BY model_name;
```

Expected:

- one row with `boolean`, `NO`, default containing `false`;
- every pre-existing route has `accept_anthropic_messages = false`;
- a route saved through the Portal with the toggle shows `true`.

Run the application checks:

```bash
./start.sh status
curl -fsS http://127.0.0.1:18080/readyz
curl -fsS http://127.0.0.1:18080/v1/models
```

Then open the Portal, edit a Chat route, enable **Also accept Anthropic Messages
(Claude CLI)**, test, save, and call `/v1/messages`. Test Connection alone is not
proof of the bridge; make a real inference request.

## Test coverage

The migration has dedicated upgrade tests:

```bash
cargo test --locked --test migration_0008
```

They prove:

- a fresh schema has `BOOLEAN NOT NULL DEFAULT FALSE` on PostgreSQL;
- applying 0008 to a simulated v1.2.0 schema backfills existing routes to
  `false`;
- route fields (provider model, protocol, auth mode) survive unchanged;
- applying 0008 twice is idempotent;
- the old v1.2.0 explicit-column query still works after the additive migration;
- a new route can store `true` while existing routes stay `false`.

The full SQLx/PostgreSQL suite also runs on every migration:

```bash
./scripts/test_postgres.sh
```

## Rollback

There is intentionally no down migration for an additive boolean column.

- Rolling back to the v1.2.0 binary is safe; it ignores the column.
- Do **not** drop the column on a live database merely to "roll back" the
  feature. Dropping it loses the saved toggles and requires a second schema
  change.
- Restore the pre-migration dump only if the database is actually damaged or a
  bad migration must be abandoned. Restoring discards writes made after the
  dump.

## UAT migration path

The isolated UAT stack (`uat/`) applies every migration into its own
PostgreSQL volume and records applied files in `uat_schema_migrations`. It never
touches the production database:

```bash
./uat/start.sh up
./uat/start.sh migrate
```

If UAT is reset with `./uat/start.sh reset`, the next `up` reapplies all
migrations from a clean database.
