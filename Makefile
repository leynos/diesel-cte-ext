.PHONY: help all clean test build release lint typecheck fmt check-fmt markdownlint \
	nixie prepare-pg-worker spelling \
	test-prepare-pg-worker test-workflow-contracts interrogate


TARGET ?= libdiesel-cte-ext.rlib

CARGO ?= cargo
BUILD_JOBS ?=
RUST_FLAGS ?= -D warnings
CARGO_FLAGS ?= --all-targets --all-features
CLIPPY_FLAGS ?= $(CARGO_FLAGS) -- $(RUST_FLAGS)
TEST_FLAGS ?= $(CARGO_FLAGS)
MDLINT ?= markdownlint-cli2
# `make fmt` and `make check-fmt` call mdtablefix directly. `--git` selects the
# Markdown files Git tracks and `--include-untracked` adds the untracked files
# Git does not ignore, so a new document is formatted before it is staged.
# Both modes need mdtablefix 0.6.0 or later; CI pins the version at the
# install-mdtablefix step.
MDTABLEFIX ?= mdtablefix
MDTABLEFIX_SELECT = --git --include-untracked
MDTABLEFIX_RULES = --wrap --renumber --breaks --ellipsis --fences
NIXIE ?= nixie
WHITAKER ?= whitaker
UV ?= uv
UV_ENV = UV_CACHE_DIR=.uv-cache UV_TOOL_DIR=.uv-tools

# The CV-005 CodeScene contracts live in shared-actions and run from a full
# commit, so a fix is a pin bump. `.github/cv005.toml` holds this repository's
# only parameters.
CV005_CONTRACTS_REF ?= 88977798a5c3bae1549afb99642529488c665276
CV005_CONTRACTS = $(UV_ENV) $(UV) tool run --python 3.13 \
	--from 'git+https://github.com/leynos/shared-actions@$(CV005_CONTRACTS_REF)\#subdirectory=packages/cv005-contracts' \
	cv005-contracts

TYPOS_CONFIG_BUILDER_VERSION ?= v0.1.3
TYPOS_CONFIG_BUILDER = $(UV_ENV) $(UV) tool run --python 3.14 --from \
	"git+https://github.com/leynos/typos-config-builder.git@$(TYPOS_CONFIG_BUILDER_VERSION)" \
	typos-config-builder
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

# The development build standard (concordat rule `rust-build-defaults`):
# the mold linker on Linux. The pin is stable, so the nightly-only parallel
# frontend flag is not used. An assigned RUSTFLAGS replaces every
# `rustflags` table in .cargo/config.toml, so each recipe that sets it
# composes this onto any inherited value (CI's setup-rust exports one),
# except coverage, which stays on the platform linker.
BUILD_HOST_OS := $(shell uname -s)
STANDARD_RUSTFLAGS := $(if $(filter Linux,$(BUILD_HOST_OS)),-Clink-arg=-fuse-ld=mold)

build: target/debug/$(TARGET) ## Build debug binary
release: target/release/$(TARGET) ## Build release binary

all: check-fmt lint test test-prepare-pg-worker test-workflow-contracts ## Perform a comprehensive check of code

clean: ## Remove build artifacts
	$(CARGO) clean

test: prepare-pg-worker ## Run tests with warnings treated as errors
	mkdir -p "$(PG_EMBED_BASE)"
	chmod 1777 "$(PG_EMBED_BASE)"
	PG_EMBEDDED_WORKER="$(PG_WORKER_PATH)" PG_RUNTIME_DIR="$(PG_RUNTIME_DIR)" PG_DATA_DIR="$(PG_DATA_DIR)" RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(RUST_FLAGS) $(STANDARD_RUSTFLAGS)" $(CARGO) test $(TEST_FLAGS) $(BUILD_JOBS)

prepare-pg-worker: ## Build the locked pg_worker helper used by PostgreSQL tests
	mkdir -p "$(dir $(PG_WORKER_PATH))"
	set -e; \
	manifest_path="$$( \
		$(CARGO) metadata --format-version 1 --locked | \
		jq -r 'first(.packages[] | select(.name == "pg-embed-setup-unpriv") | .manifest_path)' \
	)" && \
	test -n "$$manifest_path" && \
	RUSTFLAGS="$(if $(PG_WORKER_IS_RELEASE_PROFILE),$${RUSTFLAGS-},$${RUSTFLAGS:+$$RUSTFLAGS }$(STANDARD_RUSTFLAGS))" $(CARGO) build --locked --manifest-path "$$manifest_path" --bin pg_worker --profile "$(PG_WORKER_PROFILE)" --target-dir "$(CURDIR)/target" $(BUILD_JOBS) && \
	install -m 0755 "$(CURDIR)/target/$(PG_WORKER_BUILD_DIR)/pg_worker" "$(PG_WORKER_PATH)"

test-prepare-pg-worker: ## Test pg_worker profile mapping and fail-fast setup
	bash tests/prepare_pg_worker_makefile.sh

test-workflow-contracts: ## Validate the mutation-testing and CodeScene coverage workflow contracts
	$(CV005_CONTRACTS) check --repository .
	uv run --with 'pytest>=8' --with 'pyyaml>=6' pytest tests/workflow_contracts -q

target/%/$(TARGET): ## Build binary in debug or release mode
	$(if $(findstring release,$(@)),RUSTFLAGS="$${RUSTFLAGS-}",RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(STANDARD_RUSTFLAGS)") $(CARGO) build $(BUILD_JOBS) $(if $(findstring release,$(@)),--release)

# Interrogate measures docstring presence over every Python file the repository
# owns. Add a new Python directory to INTERROGATE_TARGETS. No `--python` pin: a
# managed interpreter would be downloaded, and a PyO3 build could link it.
INTERROGATE_VERSION ?= 1.7.0
INTERROGATE_TARGETS ?= tests
# Level 2 prints a MISSED row per undocumented definition, which the
# `interrogate` target fails on: the rounded percentage alone passes a lone gap.
INTERROGATE_FLAGS ?= -vv --fail-under 100
INTERROGATE = $(UV_ENV) $(UV) tool run \
	--from 'interrogate==$(INTERROGATE_VERSION)' interrogate

# Interrogate rounds its percentage to one decimal, so `--fail-under 100` alone
# passes a lone gap in a large repository; fail on any MISSED row as well.
interrogate: ## Enforce complete Python docstring coverage
	@report="$$($(INTERROGATE) $(INTERROGATE_FLAGS) $(INTERROGATE_TARGETS))"; \
	  status=$$?; \
	  printf '%s\n' "$$report"; \
	  test "$$status" -eq 0 || exit "$$status"; \
	  if printf '%s\n' "$$report" | grep -q '| *MISSED *|'; then \
	    printf '%s\n' 'interrogate: undocumented definitions remain; see the MISSED rows above' >&2; \
	    exit 1; \
	  fi

# Whitaker runs under the toolchain its installer manages, so it takes none of
# the development standard: its literal `RUSTFLAGS` displaces the `rustflags`
# tables (no `mold`) and inherits nothing from the caller.
lint: interrogate ## Run Clippy and the Whitaker Dylint suite with warnings denied
	RUSTDOCFLAGS="$(RUSTDOC_FLAGS)" RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(STANDARD_RUSTFLAGS)" $(CARGO) doc --no-deps
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(STANDARD_RUSTFLAGS)" $(CARGO) clippy $(CLIPPY_FLAGS)
	RUSTFLAGS="-D warnings" $(WHITAKER) --all -- $(CARGO_FLAGS)

typecheck: ## Type-check every target with all features enabled
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(STANDARD_RUSTFLAGS)" $(CARGO) check $(CARGO_FLAGS)

fmt: ## Format Rust and Markdown sources
	$(CARGO) fmt --all
	$(MDTABLEFIX) --in-place $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)
	$(MDLINT) --fix "**/*.md"

check-fmt: ## Verify formatting
	$(CARGO) fmt --all -- --check
	$(MDTABLEFIX) --check $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)

markdownlint: spelling ## Lint Markdown files and enforce repository spelling
	$(MDLINT) '**/*.md'

spelling: ## Enforce en-GB-oxendict spelling
	$(TYPOS_CONFIG_BUILDER) gate --repository .

nixie: ## Validate Mermaid diagrams
	$(NIXIE) --no-sandbox

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | \
	awk 'BEGIN {FS=":"; printf "Available targets:\n"} {printf "  %-20s %s\n", $$1, $$2}'
