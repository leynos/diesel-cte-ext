---
name: pg-embed-setup-unpriv-v0-4-0-upgrade
description: Upgrade pg-embed-setup-unpriv to v0.4.0 with templated test DBs
---

# Plan

Upgrade the `pg-embed-setup-unpriv` dev-dependency to v0.4.0 and adjust the
Postgres integration tests to use the new database templating helpers for
faster, isolated test databases. Keep the change scoped to test setup and
supporting documentation while ensuring the standard quality gates pass.

## Requirements
- Bump `pg-embed-setup-unpriv` to v0.4.0 with the existing features.
- Adopt the database templating APIs (`ensure_template_exists`,
  `temporary_database_from_template`, or `create_database_from_template`) where
  tests currently use the default `postgres` database.
- Preserve test isolation and teardown behaviour.
- `make check-fmt`, `make lint`, and `make test` must succeed.

## Scope
- In: `Cargo.toml`, `Cargo.lock`, Postgres integration test fixtures, and any
  related documentation updates.
- Out: production library code, non-Postgres tests, and unrelated dependency
  upgrades.

## Files and entry points
- `Cargo.toml`
- `Cargo.lock`
- `tests/test_helpers.rs`
- `tests/postgres_recursive.rs`
- `docs/pg-embedded-setup-unpriv-users-guide.md` (if behaviour changes warrant
  an update)
- `README.md` (if the testing workflow description needs adjustment)

## Data model / API changes
- No library API changes. Test fixtures will switch from connecting to the
  default `postgres` database to per-test databases cloned from a template.

## Action items
[ ] Review v0.4.0 docs/release notes to confirm API changes and the intended
    templating flow for tests.
[ ] Update `pg-embed-setup-unpriv` to `0.4.0` in `Cargo.toml` and refresh
    `Cargo.lock` (use `cargo update -p pg-embed-setup-unpriv --precise 0.4.0`
    if a targeted update is preferred).
[ ] Extend the Postgres test fixture to:
    - Ensure a template database exists once per cluster using
      `ensure_template_exists` and any required setup/migrations.
    - Create a per-test `TemporaryDatabase` via
      `temporary_database_from_template`, returning its URL/connection handle to
      tests and allowing RAII cleanup.
[ ] Update `tests/postgres_recursive.rs` to use the templated database URL (or
    Diesel connection) instead of `database_url("postgres")`.
[ ] Update documentation that references the test harness if the connection or
    setup flow changes.
[ ] Run the quality gates with logged output and resolve any failures:
    - `set -o pipefail; make check-fmt | tee /tmp/make-check-fmt.log`
    - `set -o pipefail; make lint | tee /tmp/make-lint.log`
    - `set -o pipefail; make test | tee /tmp/make-test.log`
    - If docs change: `make markdownlint`, `make nixie`, and `make fmt`.

## Testing and validation
- `make check-fmt`
- `make lint`
- `make test`
- If docs change: `make markdownlint`, `make nixie`, `make fmt`.

## Risks and edge cases
- Template databases must have no active connections when cloning; ensure
  template setup closes all connections before cloning.
- Parallel tests may collide on database names; use a deterministic naming
  scheme (e.g., include the test name or a unique counter).
- If the fixture scope is widened to share a cluster, confirm that environment
  variable guards still prevent cross-test leakage.

## Open questions
- Do we need to seed the template with schema/migrations, or is an empty
  template sufficient for current tests?
- Should the cluster fixture scope be broadened to make templating pay off, or
  keep per-test clusters and use templating purely for isolation?
