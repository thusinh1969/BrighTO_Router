# BrighTO-Router Kubernetes starter

Kubernetes support is optional. Most teams should start with Docker Compose from the root README. Use Kubernetes when you already have a cluster or need multiple router pods, rolling upgrades, or load-balancer integration.

## One-command install

Local single-node starter with in-cluster development PostgreSQL:

```bash
./start.sh install --k8s --replicas 2
```

Production-style single-node or multi-node install with an existing PostgreSQL database:

```bash
./start.sh install --database-url 'postgres://user:pass@db-host:5432/brighto_router' --k8s --replicas 3
```

This assumes `kubectl` already points to the target cluster. The script does not install Kubernetes itself.

## What the installer does

1. Creates or updates the namespace. Default namespace: `brighto-router`.
2. Uses `k8s/postgres.dev.yaml` only when `DATABASE_URL` is the default local value.
3. Runs SQL migrations and `scripts/seed_defaults.sql`.
4. Creates `brighto-router-secret` from `.env`.
5. Applies the config map, router deployment, and ClusterIP service.
6. Rewrites the deployment replica count from `--replicas`.

## Production rule

Use external or managed PostgreSQL for real multi-node deployments. `k8s/postgres.dev.yaml` uses temporary pod storage and is only for local testing.

The router pods are stateless. Each pod loads configuration from PostgreSQL, keeps a fast in-memory routing snapshot, forwards requests to provider backends, and writes usage asynchronously to PostgreSQL. That makes horizontal scale-out straightforward: add pods behind the Kubernetes Service, then expose the Service with your Ingress or load balancer.

## Why scale out if the router is already fast?

Scale-out is mostly for operations, not because the Rust router is heavy. Multiple pods help with:

- availability during pod or node restarts;
- rolling upgrades without planned downtime;
- many long-running streams from developers or coding agents;
- separating traffic across nodes when one machine's network, CPU, or connection limits become the bottleneck;
- fitting into an existing platform team's Kubernetes standard.

The starter manifests are intentionally small. Enterprise packaging can add Helm-style values, autoscaling, pod disruption budgets, network policy, secrets-manager integration, and production ingress templates.
