#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo test --locked --test model_group_smoke model_group_weighted_round_robin_three_endpoint_smoke -- --nocapture
