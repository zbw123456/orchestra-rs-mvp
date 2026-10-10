SHELL := /bin/bash
.PHONY: setup dev build test integration bench fmt

setup:
	source scripts/env.sh && cargo build --locked
	cd frontend && npm ci --no-audit --no-fund --cache ../.local/npm-cache

dev:
	bash scripts/dev.sh

build:
	source scripts/env.sh && cargo build --locked
	cd frontend && npm run build

test:
	source scripts/env.sh && cargo test --locked
	PYTHONPATH=scheduler python3 -m unittest discover -s scheduler/tests -v
	python3 -m unittest discover -s benchmarks -v
	cd frontend && npm run build

integration:
	source scripts/env.sh && cargo build --locked
	python3 scripts/integration.py

bench:
	python3 benchmarks/compare.py

fmt:
	source scripts/env.sh && cargo fmt --all
