#!/usr/bin/env python3
"""Emit the single-model UAT seed: local Qwen 3.8, dual OpenAI + Anthropic."""

from __future__ import annotations

import hashlib
import os


def sql(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def main() -> None:
    client_key = os.environ.get("UAT_CLIENT_KEY", "").strip()
    if not client_key:
        raise SystemExit("UAT_CLIENT_KEY is required")
    qwen_base = (
        os.environ.get("UAT_QWEN_BASE_URL", "http://host.docker.internal:8088/v1").strip()
        or "http://host.docker.internal:8088/v1"
    )
    qwen_model = (
        os.environ.get("UAT_QWEN_MODEL", "qwen3.8-flash-next").strip()
        or "qwen3.8-flash-next"
    )
    key_hash = hashlib.sha256(client_key.encode()).hexdigest()
    key_prefix = client_key[:8]

    print("\\set ON_ERROR_STOP on")
    print(
        "INSERT INTO teams (id, name, budget, enabled) VALUES "
        "(1, 'UAT Team', '{\"period\":\"day\",\"max_tokens\":100000000,\"per_model\":{}}', TRUE) "
        "ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, budget = EXCLUDED.budget, enabled = TRUE;"
    )
    print(
        "INSERT INTO api_keys "
        "(id, key_hash, key_prefix, team_id, owner, allowed_models, budget, rpm_limit, concurrency_limit, expires_at, enabled) "
        f"VALUES (1, {sql(key_hash)}, {sql(key_prefix)}, 1, 'uat-qwen', '[]', NULL, NULL, NULL, NULL, TRUE) "
        "ON CONFLICT (id) DO UPDATE SET key_hash = EXCLUDED.key_hash, key_prefix = EXCLUDED.key_prefix, "
        "team_id = EXCLUDED.team_id, owner = EXCLUDED.owner, allowed_models = EXCLUDED.allowed_models, enabled = TRUE;"
    )
    # Reuse backend id 1 for the only UAT model. Old UAT mock/group routes are
    # removed below, so no model other than Qwen 3.8 can remain public.
    print(
        "INSERT INTO backends "
        "(id, name, base_url, api_key_ref, weight, max_inflight, format, enabled) VALUES "
        f"(1, 'Custom LLM', {sql(qwen_base)}, 'env:NONE', 1, 100, 'openai', TRUE) "
        "ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, base_url = EXCLUDED.base_url, "
        "api_key_ref = EXCLUDED.api_key_ref, weight = EXCLUDED.weight, max_inflight = EXCLUDED.max_inflight, "
        "format = EXCLUDED.format, enabled = TRUE;"
    )
    print(
        "INSERT INTO model_routes "
        "(model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, "
        "provider_model_name, enabled, auth_mode, protocol, routing_policy, accept_anthropic_messages) "
        f"VALUES ({sql(qwen_model)}, '[1]', NULL, 4.0, 180, {sql(qwen_model)}, TRUE, 'none', "
        "'local_openai_chat', 'least_loaded_weighted', TRUE) "
        "ON CONFLICT (model_name) DO UPDATE SET backend_ids = EXCLUDED.backend_ids, "
        "fallback_backend_id = EXCLUDED.fallback_backend_id, chars_per_token = EXCLUDED.chars_per_token, "
        "first_byte_timeout = EXCLUDED.first_byte_timeout, provider_model_name = EXCLUDED.provider_model_name, "
        "enabled = TRUE, auth_mode = EXCLUDED.auth_mode, protocol = EXCLUDED.protocol, "
        "routing_policy = EXCLUDED.routing_policy, accept_anthropic_messages = TRUE;"
    )
    # UAT is intentionally single-model. Remove every other route, endpoint,
    # counter, and backend left by older seeds or Portal tests.
    print(f"DELETE FROM model_routes WHERE model_name <> {sql(qwen_model)};")
    print(f"DELETE FROM model_route_endpoints WHERE model_name <> {sql(qwen_model)};")
    print(f"DELETE FROM model_route_counters WHERE model_name <> {sql(qwen_model)};")
    print("DELETE FROM backends WHERE id <> 1;")


if __name__ == "__main__":
    main()
