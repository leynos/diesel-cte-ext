.PHONY: help all clean test build release lint fmt check-fmt markdownlint nixie prepare-pg-worker


TARGET ?= libdiesel-cte-ext.rlib

CARGO ?= cargo
BUILD_JOBS ?=
RUST_FLAGS ?= -D warnings
CARGO_FLAGS ?= --all-targets --all-features
CLIPPY_FLAGS ?= $(CARGO_FLAGS) -- $(RUST_FLAGS)
TEST_FLAGS ?= $(CARGO_FLAGS)
MDLINT ?= markdownlint-cli2
NIXIE ?= nixie
PG_WORKER_PATH ?= $(CURDIR)/target/pg_worker
PG_WORKER_PROFILE ?= dev
PG_WORKER_BUILD_DIR = $(if $(filter dev test,$(PG_WORKER_PROFILE)),debug,$(if $(filter bench,$(PG_WORKER_PROFILE)),release,$(PG_WORKER_PROFILE)))
ifndef PG_EMBED_RUN_ID
PG_EMBED_RUN_ID := $(shell printf '%s-%s' "$$(date +%s)" $$$$)
endif
PG_EMBED_BASE ?= $(CURDIR)/target/pg-embed-runs/$(PG_EMBED_RUN_ID)
PG_RUNTIME_DIR ?= $(PG_EMBED_BASE)/runtime
PG_DATA_DIR ?= $(PG_EMBED_BASE)/data

build: target/debug/$(TARGET) ## Build debug binary
release: target/release/$(TARGET) ## Build release binary

all: check-fmt lint test ## Perform a comprehensive check of code

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

target/%/$(TARGET): ## Build binary in debug or release mode
	$(CARGO) build $(BUILD_JOBS) $(if $(findstring release,$(@)),--release)

lint: ## Run Clippy with warnings denied
	RUSTDOCFLAGS="$(RUSTDOC_FLAGS)" $(CARGO) doc --no-deps
	$(CARGO) clippy $(CLIPPY_FLAGS)

fmt: ## Format Rust and Markdown sources
	$(CARGO) fmt --all
	mdformat-all

check-fmt: ## Verify formatting
	$(CARGO) fmt --all -- --check

markdownlint: ## Lint Markdown files
	$(MDLINT) '**/*.md'

nixie: ## Validate Mermaid diagrams
	$(NIXIE) --no-sandbox

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | \
	awk 'BEGIN {FS=":"; printf "Available targets:\n"} {printf "  %-20s %s\n", $$1, $$2}'
