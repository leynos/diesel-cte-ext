.PHONY: help all clean test build release lint typecheck fmt check-fmt markdownlint \
	nixie prepare-pg-worker spelling spelling-helper-test \
	test-prepare-pg-worker test-workflow-contracts


TARGET ?= libdiesel-cte-ext.rlib

CARGO ?= cargo
BUILD_JOBS ?=
RUST_FLAGS ?= -D warnings
CARGO_FLAGS ?= --all-targets --all-features
CLIPPY_FLAGS ?= $(CARGO_FLAGS) -- $(RUST_FLAGS)
TEST_FLAGS ?= $(CARGO_FLAGS)
MDLINT ?= markdownlint-cli2
NIXIE ?= nixie
WHITAKER ?= whitaker
UV ?= uv
UV_ENV = UV_CACHE_DIR=.uv-cache UV_TOOL_DIR=.uv-tools
TYPOS_VERSION ?= 1.48.0
TYPOS = $(UV) tool run typos@$(TYPOS_VERSION)
PG_WORKER_PATH ?= $(CURDIR)/target/pg_worker
PG_WORKER_PROFILE ?= dev
PG_WORKER_DEBUG_PROFILES := dev test
PG_WORKER_RELEASE_PROFILES := release bench
PG_WORKER_IS_DEBUG_PROFILE = $(filter $(PG_WORKER_DEBUG_PROFILES),$(PG_WORKER_PROFILE))
PG_WORKER_IS_RELEASE_PROFILE = $(filter $(PG_WORKER_RELEASE_PROFILES),$(PG_WORKER_PROFILE))
PG_WORKER_DEFAULT_BUILD_DIR = $(if $(PG_WORKER_IS_RELEASE_PROFILE),release,$(PG_WORKER_PROFILE))
PG_WORKER_BUILD_DIR = $(if $(PG_WORKER_IS_DEBUG_PROFILE),debug,$(PG_WORKER_DEFAULT_BUILD_DIR))
ifndef PG_EMBED_RUN_ID
PG_EMBED_RUN_ID := $(shell printf '%s-%s' "$$(date +%s)" $$$$)
endif
PG_EMBED_BASE ?= $(CURDIR)/target/pg-embed-runs/$(PG_EMBED_RUN_ID)
PG_RUNTIME_DIR ?= $(PG_EMBED_BASE)/runtime
PG_DATA_DIR ?= $(PG_EMBED_BASE)/data

build: target/debug/$(TARGET) ## Build debug binary
release: target/release/$(TARGET) ## Build release binary

all: check-fmt lint test test-prepare-pg-worker ## Perform a comprehensive check of code

clean: ## Remove build artifacts
	$(CARGO) clean

test: prepare-pg-worker ## Run tests with warnings treated as errors
	mkdir -p "$(PG_EMBED_BASE)"
	chmod 1777 "$(PG_EMBED_BASE)"
	PG_EMBEDDED_WORKER="$(PG_WORKER_PATH)" PG_RUNTIME_DIR="$(PG_RUNTIME_DIR)" PG_DATA_DIR="$(PG_DATA_DIR)" RUSTFLAGS="$(RUST_FLAGS)" $(CARGO) test $(TEST_FLAGS) $(BUILD_JOBS)

prepare-pg-worker: ## Build the locked pg_worker helper used by PostgreSQL tests
	mkdir -p "$(dir $(PG_WORKER_PATH))"
	set -e; \
	manifest_path="$$( \
		$(CARGO) metadata --format-version 1 --locked | \
		jq -r 'first(.packages[] | select(.name == "pg-embed-setup-unpriv") | .manifest_path)' \
	)" && \
	test -n "$$manifest_path" && \
	$(CARGO) build --locked --manifest-path "$$manifest_path" --bin pg_worker --profile "$(PG_WORKER_PROFILE)" --target-dir "$(CURDIR)/target" $(BUILD_JOBS) && \
	install -m 0755 "$(CURDIR)/target/$(PG_WORKER_BUILD_DIR)/pg_worker" "$(PG_WORKER_PATH)"

test-prepare-pg-worker: ## Test pg_worker profile mapping and fail-fast setup
	bash tests/prepare_pg_worker_makefile.sh

test-workflow-contracts: ## Validate the mutation-testing caller contract
	uv run --with 'pytest>=8' --with 'pyyaml>=6' pytest tests/workflow_contracts -q

target/%/$(TARGET): ## Build binary in debug or release mode
	$(CARGO) build $(BUILD_JOBS) $(if $(findstring release,$(@)),--release)

lint: ## Run Clippy and the Whitaker Dylint suite with warnings denied
	RUSTDOCFLAGS="$(RUSTDOC_FLAGS)" $(CARGO) doc --no-deps
	$(CARGO) clippy $(CLIPPY_FLAGS)
	RUSTFLAGS="$(RUST_FLAGS)" $(WHITAKER) --all -- $(CARGO_FLAGS)

typecheck: ## Type-check every target with all features enabled
	$(CARGO) check $(CARGO_FLAGS)

fmt: ## Format Rust and Markdown sources
	$(CARGO) fmt --all
	mdformat-all

check-fmt: ## Verify formatting
	$(CARGO) fmt --all -- --check

markdownlint: spelling ## Lint Markdown files and enforce repository spelling
	$(MDLINT) '**/*.md'

spelling: spelling-helper-test ## Enforce en-GB-oxendict spelling in Markdown prose
	@$(UV_ENV) $(UV) run scripts/generate_typos_config.py
	@git ls-files -z '*.md' | \
		xargs -0 -r env $(UV_ENV) $(TYPOS) --config typos.toml --force-exclude

spelling-helper-test: ## Validate the shared spelling-policy integration
	@PYTHONPATH=scripts $(UV_ENV) $(UV) run --python 3.13 \
		--with pytest==9.0.2 --with pytest-cov==7.0.0 \
		python -m pytest scripts/tests/test_typos_rollout.py \
		--cov=generate_typos_config --cov=typos_rollout \
		--cov=typos_rollout_cache --cov-fail-under=90

nixie: ## Validate Mermaid diagrams
	$(NIXIE) --no-sandbox

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | \
	awk 'BEGIN {FS=":"; printf "Available targets:\n"} {printf "  %-20s %s\n", $$1, $$2}'
