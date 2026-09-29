---
title: BrighTO-Router — Rust LLM Gateway, Model Load Balancer, and SystemOne Router
description: Free open-source, self-hosted LLM gateway and router in Rust for OpenAI-compatible, Anthropic-compatible, Ollaya/Laya, JEV/DJEV-style SystemOne decisions, team keys, token budgets, fallback routing, and model load balancing.
---

# BrighTO-Router — Rust LLM Gateway, Model Load Balancer, and SystemOne Router

**Million-token AI traffic, simple Rust fast path, one Docker install.**

BrighTO-Router, also searchable as **Brighto LLM Router**, is a free open-source, self-hosted LLM gateway, AI router, and SystemOne decision router written in Rust. It gives teams one stable API endpoint for OpenAI-compatible, Anthropic-compatible, cloud, local, Ollaya/Laya, and JEV/DJEV-style System One backends with model load balancing, fallback routing, team API keys, token budgets, usage analytics, and privacy-first logging.

![BrighTO-Router architecture: open-source Rust LLM gateway with model load balancing, chat completions, embeddings, rerank, ASR transcription, PostgreSQL usage metadata, and privacy-first no prompt storage](assets/brighto-router-architecture.png)


## Why this exists

Most teams do not need a large hosted AI platform to start. They need a fast, understandable LLM API proxy that they can run, audit, and maintain. BrighTO-Router focuses on the traffic path: authenticate, enforce budget, choose model route or Model Group, forward the request, stream the response, and record usage metadata.

## Core features

- OpenAI-compatible `/v1/chat/completions`, `/v1/completions`, `/v1/responses`, `/v1/embeddings`
- Anthropic-compatible `/v1/messages`
- System One decisions through `/v1/systemone` and `/v1/decisions`
- Rerank and ASR adapter routes
- Round-robin and weighted Model Groups for load balancing
- Team API keys, expiry, token budgets, RPM limits, and concurrency limits
- PostgreSQL usage ledger and Portal dashboard
- No prompt or response content stored by default
- One-line Docker install

## Quick start

```bash
curl -fsSL https://raw.githubusercontent.com/thusinh1969/BrighTO_Router/main/install.sh | bash
```

Then open:

```text
http://127.0.0.1:18080/
```

## Architecture

- [v1.1.0 architecture](architecture/brighto-router-workflow.html) — Portal, Admin API, PostgreSQL snapshot, Rust hot path, adapters, Model Groups, SystemOne, ledger, and release gates.
- [PostgreSQL data flow](postgresql-data-flow.html) — durable control plane, Model Group counters, usage ledger, and JSONL fallback.
- [API examples](API_EXAMPLES.md) — Python calls for Chat Completions, Completions, Responses, embeddings, rerank, ASR, System One, and Anthropic Messages.
- [Load-balancing test notes](REAL_LOAD_BALANCING_TESTS.md) — round-robin and weighted Model Group validation.

## Repository and Docker image

- GitHub: [thusinh1969/BrighTO_Router](https://github.com/thusinh1969/BrighTO_Router)
- Docker: `thusinh1969/brighto_airouter:v1.1.0`
- Release: `v1.1.0`

## Benchmarks

BrighTO-Router publishes deterministic mock-backend benchmarks to measure router overhead, not model inference speed. The 1M-token and Model Group artifacts are in the repository under `benchmarks/artifacts/`.

## Keywords

LLM gateway, LLM router, AI gateway, AI router, OpenAI API proxy, Anthropic API router, self-hosted LLM proxy, model router, model load balancer, SystemOne router, System One decisions, JEV router, DJEV router, Ollaya router, Laya model, fallback routing, token budget, cost reduction, LiteLLM alternative, Bifrost alternative, Rust API gateway.
