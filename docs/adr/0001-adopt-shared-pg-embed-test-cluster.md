# 0001. Adopt shared pg-embed test cluster

Status: Accepted

Date: 2026-06-25

## Y-Statement

In the context of PostgreSQL-backed integration tests for `diesel-cte-ext`,
facing slow per-test embedded cluster startup, duplicated bootstrap code, and
root-agent worker setup that should be explicit, this project adopts one
`pg-embed-setup-unpriv` shared embedded PostgreSQL cluster per test process,
accessed through `test_support::shared_cluster_handle()`, plus per-test
`TemporaryDatabase` values cloned from a template database, and against per-test
`TestCluster::new()`, local ownership of `PG_RUNTIME_DIR`, `PG_DATA_DIR` and
`PG_PASSWORD`, Rust test-code Cargo metadata parsing for `pg_worker`, copying
downstream custom shutdown wrappers, and an external PostgreSQL URL
abstraction, to achieve faster tests with database-level isolation and fewer
local lifecycle responsibilities, accepting that root workflows must prepare
`PG_EMBEDDED_WORKER` through Makefile or Continuous Integration tooling before
running tests directly.

## Consequences

- PostgreSQL integration tests reuse one upstream shared cluster handle and
  must create a unique `TemporaryDatabase` for every test that mutates state.
- `make test` is the supported local and Continuous Integration path because it
  builds the locked `pg_worker` binary and exports `PG_EMBEDDED_WORKER`.
- Direct `cargo test` remains available for unprivileged workflows, but root
  workflows that bypass `make test` must export `PG_EMBEDDED_WORKER` themselves.
