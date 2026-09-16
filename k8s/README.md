# Kubernetes

These manifests are intentionally small. They run BrighTO-Router with PostgreSQL-only production state and no Redis dependency.

Use `postgres.dev.yaml` for a quick in-cluster development database. For production, point `DATABASE_URL` in `secret.example.yaml` at a managed or separately operated PostgreSQL service and run migrations as part of your release process.

```bash
kubectl create namespace brighto-router
kubectl -n brighto-router apply -f k8s/postgres.dev.yaml
kubectl -n brighto-router apply -f k8s/secret.example.yaml
kubectl -n brighto-router apply -f k8s/configmap.example.yaml
kubectl -n brighto-router apply -f k8s/deployment.yaml
kubectl -n brighto-router apply -f k8s/service.yaml
```

Replace the example secret before production.
