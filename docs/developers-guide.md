# Developer guide

## PostgreSQL test support

This crate uses `pg-embed-setup-unpriv` for PostgreSQL-backed integration tests
because embedded PostgreSQL setup crosses process, filesystem, and privilege
boundaries. Keeping that behaviour in a dedicated helper avoids scattering
directory ownership, PostgreSQL binary caching, password file handling, and
cluster lifecycle policy through this crate's tests.

The dependency is especially important for sandboxed agentic development. These
workspaces often run automation as `root`, while PostgreSQL refuses to
initialise or run as `root`. `pg-embed-setup-unpriv` detects that case,
prepares the runtime and data directories with the permissions PostgreSQL
expects, and delegates lifecycle commands to a worker helper that drops to the
`nobody` user. The test harness can therefore keep its original process
identity without mutating the effective user ID mid-test.

Local PostgreSQL tests should prefer the existing `pg-embed-setup-unpriv`
helpers instead of starting PostgreSQL directly. This keeps root and
unprivileged execution paths aligned, preserves deterministic environment
variables such as `PGPASSFILE` and `TZDIR`, and makes failures easier to
diagnose in Continuous Integration (CI) and agent sandboxes.

PostgreSQL integration tests use one shared embedded cluster per test process
and create one template-cloned temporary database for each test. The shared
cluster avoids repeated PostgreSQL bootstrap work, while each
`TemporaryDatabase` keeps mutable database state isolated between tests.

The fixture architecture is recorded in
[`docs/adr/0001-adopt-shared-pg-embed-test-cluster.md`](adr/0001-adopt-shared-pg-embed-test-cluster.md).

Use `make test` for the supported local test workflow. The target builds the
locked `pg_worker` binary from the `pg-embed-setup-unpriv` dependency into
`target/pg_worker`, exports `PG_EMBEDDED_WORKER`, and then runs
`cargo test --all-targets --all-features`. This keeps root-agent runs aligned
with CI and avoids hidden worker builds inside Rust test code.

For the focused PostgreSQL integration test, run:

```bash
set -o pipefail; PG_EMBEDDED_WORKER="${PWD}/target/pg_worker" \
  cargo test --all-features --test postgres_recursive
```

Run `make prepare-pg-worker` first when executing that focused command as
`root`. Unprivileged `cargo test` invocations can use the upstream defaults,
but root invocations that bypass `make test` must provide `PG_EMBEDDED_WORKER`.

This crate does not currently support external PostgreSQL test URLs. The test
suite relies on embedded PostgreSQL so local, CI and sandboxed agent runs use
the same lifecycle and cleanup behaviour.

For usage details, see
[`docs/pg-embed-setup-unpriv-users-guide.md`](pg-embed-setup-unpriv-users-guide.md).
