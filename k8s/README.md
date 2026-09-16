# BrighTO-Router Kubernetes starter

Use the root installer unless you are editing manifests directly.

Local starter with in-cluster development PostgreSQL:

```bash
./start.sh install --k8s --replicas 2
```

Production-style install with an existing PostgreSQL database:

```bash
./start.sh install --database-url 'postgres://user:pass@db-host:5432/brighto_router' --k8s --replicas 2
```

What the installer does:

1. Creates or updates the namespace. Default namespace: `brighto-router`.
2. Uses `k8s/postgres.dev.yaml` only when `DATABASE_URL` is the default local value.
3. Runs migrations and `scripts/seed_defaults.sql`.
4. Creates `brighto-router-secret` from `.env`.
5. Applies config map, deployment, and service.

The starter manifests are intentionally small. For production, use a managed PostgreSQL service, set a strong `ADMIN_MASTER_KEY`, store provider keys through your secret-management system, and tune CPU/memory requests for your traffic.
