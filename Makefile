SHELL := /usr/bin/env bash
.DEFAULT_GOAL := ci

# ─── Rust ────────────────────────────────────────────────────────────────────

RUST_PACKAGES := bugswarm-sandbox bugswarm-evidence bugswarm-cpg bugswarm-symbolic
CARGO_FLAGS ?=

.PHONY: cargo-check
cargo-check:
	@echo "── cargo check ──"
	cargo check --workspace --all-features $(CARGO_FLAGS)

.PHONY: cargo-clippy
cargo-clippy:
	@echo "── cargo clippy ──"
	cargo clippy --all -- -D warnings $(CARGO_FLAGS)

.PHONY: cargo-test
cargo-test:
	@echo "── cargo test ──"
	cargo test --workspace $(CARGO_FLAGS)

.PHONY: cargo-audit
cargo-audit:
	@echo "── cargo audit ──"
	cargo audit $(CARGO_FLAGS)

.PHONY: cargo-fmt
cargo-fmt:
	@echo "── cargo fmt ──"
	cargo fmt --check $(CARGO_FLAGS)

.PHONY: safety-comments
safety-comments:
	@echo "── SAFETY comments check ──"
	bash bugswarm-sandbox/ci/check_safety_comments.sh
	@echo "SAFETY check passed"

# ─── Python ──────────────────────────────────────────────────────────────────

.PHONY: ruff-check
ruff-check:
	@echo "── ruff check ──"
	ruff check --output-format=concise .
	@echo "ruff check passed"

.PHONY: ruff-format
ruff-format:
	@echo "── ruff format ──"
	ruff format --check .
	@echo "ruff format passed"

.PHONY: mypy
mypy:
	@echo "── mypy ──"
	mypy --strict --ignore-missing-imports bugswarm_mini/ bugswarm-gateway/src/gateway/ bugswarm-agent/src/agent/ bugswarm-swarm/src/swarm/
	@echo "mypy passed"

.PHONY: pytest
pytest:
	@echo "── pytest ──"
	python3 -m pytest tests/ -q --tb=short
	@echo "pytest passed"

.PHONY: secrets-test
secrets-test:
	@echo "── secrets unit tests ──"
	python3 -m pytest tests/unit/test_secrets.py -q
	@echo "secrets tests passed"

# ─── Pre-commit ──────────────────────────────────────────────────────────────

.PHONY: pre-commit-install
pre-commit-install:
	@echo "── installing pre-commit hooks ──"
	pre-commit install
	pre-commit install --hook-type pre-push
	@echo "pre-commit hooks installed"

.PHONY: pre-commit-run
pre-commit-run:
	@echo "── running pre-commit on all files ──"
	pre-commit run --all-files
	@echo "pre-commit passed"

# ─── Composite targets ───────────────────────────────────────────────────────

.PHONY: lint
lint: cargo-clippy ruff-check ruff-format safety-comments

.PHONY: typecheck
typecheck: mypy

.PHONY: test
test: cargo-test pytest secrets-test

.PHONY: audit
audit: cargo-audit cargo-fmt

.PHONY: ci
ci: cargo-check lint typecheck test audit
	@echo ""
	@echo "═══════════════════════════════════════════════════════"
	@echo "  ALL CHECKS PASSED"
	@echo "═══════════════════════════════════════════════════════"

.PHONY: all
all: ci

.PHONY: fix
fix:
	ruff check --fix .
	ruff format .
	cargo fmt
