SHELL := /bin/bash
export PATH := $(HOME)/.cargo/bin:$(PATH)
-include .env
export

.PHONY: start stop status logs restart migrate migrate-new prepare dev build test check audit gate gate-smoke bench-gate bench-gate-smoke clean image up down help

help:
	@grep -E '^[a-zA-Z0-9_-]+:.*## ' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*## "}; {printf "%-20s %s\n", $$1, $$2}'

start:             ## start Postgres, migrations, and router via Docker Compose
	./start.sh start

stop:              ## stop the Docker Compose stack
	./start.sh stop

status:            ## show stack and endpoint status
	./start.sh status

logs:              ## follow router logs
	./start.sh logs

restart:           ## restart the stack via Docker Compose
	./start.sh restart

migrate:           ## run migrations against DATABASE_URL or the Compose Postgres
	./start.sh migrate

migrate-new:       ## make migrate-new NAME=add_teams
	sqlx migrate add -r $(NAME)

prepare:           ## generate .sqlx/ metadata for offline SQLx builds
	cargo sqlx prepare

dev:               ## run router locally with .env loaded by cargo runtime
	cargo run

build:             ## build production binary at target/release/brighto-router
	cargo build --release --locked

test:              ## run Rust tests with an available Postgres test database
	./scripts/test_postgres.sh

check:             ## fmt + hot-path guard + clippy
	python3 scripts/hotpath_guard.py
	cargo fmt --all -- --check
	cargo clippy --locked --all-targets -- -D warnings

audit:             ## run cargo-audit if installed
	cargo audit

gate:              ## release gate: check + tests + canonical mock benchmark
	$(MAKE) check
	$(MAKE) test
	$(MAKE) bench-gate

gate-smoke:        ## fast smoke gate; not release proof
	$(MAKE) check
	$(MAKE) bench-gate-smoke

bench-gate:        ## full SOTA matrix -> bench/results/<ts>/
	python3 scripts/bench_real.py

bench-gate-smoke:  ## short benchmark smoke; does not enforce thresholds
	DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py

clean:             ## remove Rust build artifacts
	cargo clean

image:             ## build Docker image
	DOCKER_BUILDKIT=1 docker build -t thusinh1969/brighto_airouter:v1 .

up:                ## alias for start
	./start.sh start

down:              ## alias for stop
	./start.sh stop
