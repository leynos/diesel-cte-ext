# Adopt pg-embed-setup-unpriv v0.5.1 test architecture

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
`Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`,
and `Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: DRAFT

## Purpose / big picture

The crate already depends on `pg-embed-setup-unpriv = "0.5.1"` and uses the
Diesel support and template-database helpers. The test harness still contains
local bootstrap code that v0.5.x made unnecessary: it manually sets PostgreSQL
environment variables, deletes data directories, builds `pg_worker` from the
Cargo dependency graph, and starts a fresh `TestCluster` fixture for each
PostgreSQL test.

After this plan is implemented, PostgreSQL integration tests use the upstream
v0.5.1 testing architecture. A single shared embedded cluster is bootstrapped
per test process, each test still receives an isolated temporary database
cloned from a template, root-agent worker setup is handled by explicit tooling
rather than hidden test code, and the decision is recorded in an Architecture
Decision Record (ADR).

The observable result is that `make test` passes with the PostgreSQL tests using
`pg_embedded_setup_unpriv::test_support::shared_cluster_handle()` or an
equivalent upstream shared fixture, while bespoke code in
`tests/test_helpers.rs` and `tests/postgres_recursive.rs` is removed or reduced
to a small test-specific database helper.

## Constraints

- Read and follow `AGENTS.md` before editing files.
- Use the `rust-router` skill first for Rust test-fixture work, then load
  only the smallest relevant Rust follow-on skill. `rust-unit-testing` is the
  likely follow-on because this task changes fixtures and integration tests.
- Use the `arch-decision-records` skill before creating the ADR.
- Keep the production crate API unchanged. The intended scope is test
  harnesses, build or Continuous Integration (CI) tooling, and documentation.
- Preserve the current PostgreSQL test semantics. The synchronous and
  asynchronous recursive common table expression (CTE) coverage in
  `tests/postgres_recursive.rs` must still pass under the existing feature
  matrix.
- Preserve per-test database isolation. Sharing a cluster must not cause
  tests to share mutable database state.
- Do not copy `wildside`'s older custom `atexit` wrapper. In
  `pg-embed-setup-unpriv` v0.5.1, upstream `shared_cluster_handle()` registers
  a built-in shutdown hook.
- Do not adopt `mxd`'s external `POSTGRES_TEST_URL` abstraction unless a
  later human decision explicitly expands this crate's scope to support
  external PostgreSQL test servers.
- Do not enable the `async-api` feature unless the implementation converts
  the PostgreSQL async coverage to native `#[tokio::test]` tests and proves
  that this is simpler than the current synchronous fixture.
- Use Makefile targets for quality gates. Run test, lint, and formatting
  commands sequentially and pipe output through `tee` to a file in `/tmp`.
- Commit only after the relevant gates pass.

If satisfying the objective requires violating any constraint, stop, record the
conflict in `Decision Log`, and ask for direction.

## Tolerances (exception triggers)

- Scope: if implementation requires modifying more than eight repository
  files, stop and confirm the expanded scope before continuing.
- Public API: if a public library item in `src/` must change, stop and
  confirm the API impact.
- Dependencies: if a new external crate is required, stop and justify the
  dependency before adding it.
- CI workflow: if the GitHub Actions workflow requires more than a minimal
  worker-installation step or cache-warming step, stop and present options.
- Test retries: if the focused PostgreSQL test fails for the same reason
  after three implementation attempts, stop and document the failure.
- Runtime: if `make test` takes more than 20 minutes or times out, stop and
  inspect whether the shared cluster or worker setup is hanging.
- Ambiguity: if both upstream `shared_test_cluster` and
  `shared_cluster_handle()` appear equally good after prototyping, choose
  `shared_cluster_handle()` unless there is a concrete simplification from the
  rstest fixture form.

## Risks

- Risk: sharing one embedded PostgreSQL cluster may surface test-order or
  database-name collisions that per-test clusters hid. Severity: medium.
  Likelihood: medium. Mitigation: keep one `TemporaryDatabase` per test,
  generate unique test database names, and add a focused isolation regression
  test.

- Risk: removing deterministic target-local `PG_RUNTIME_DIR` and
  `PG_DATA_DIR` paths may make local debugging less predictable. Severity: low.
  Likelihood: medium. Mitigation: rely on upstream defaults for normal runs and
  document debugging alternatives in the ADR or developer guide only if needed.

- Risk: moving `pg_worker` preparation out of test code could break
  root-agent test runs if CI or local tooling forgets to export
  `PG_EMBEDDED_WORKER`. Severity: high. Likelihood: medium. Mitigation: add a
  Makefile target that installs or copies `pg_worker`, run tests through that
  target, and document direct command requirements for contributors who bypass
  `make`.

- Risk: the existing async test uses a manually created Tokio runtime inside
  a synchronous rstest. Enabling `async-api` could broaden the dependency
  feature surface without enough benefit. Severity: medium. Likelihood: low.
  Mitigation: keep async coverage on the shared synchronous cluster unless a
  prototype shows that native async cluster startup removes more code than it
  adds.

- Risk: `tests/test_helpers.rs` is also imported by `tests/env_var_guard.rs`.
  Deleting it without removing or replacing those tests will break the test
  suite. Severity: medium. Likelihood: high. Mitigation: remove the obsolete
  env-var guard tests in the same commit that removes the guard, or replace
  them with tests for the new helper behaviour.

## Progress

- [x] (2026-06-25) Drafted the adoption plan from the current harness audit
  and cross-repository findings.
- [ ] Load the required skills for implementation:
  `rust-router`, `rust-unit-testing`, `arch-decision-records`, and
  `commit-message`.
- [ ] Establish the baseline by running the focused PostgreSQL tests and the
  relevant quality gates.
- [ ] Create the ADR documenting the shared embedded PostgreSQL test-cluster
  decision.
- [ ] Add the smallest regression test that fails before the fixture
  migration and proves shared-cluster, per-test database isolation.
- [ ] Replace the local `embedded_cluster` fixture with upstream shared
  cluster access and a small per-test template database helper.
- [ ] Remove obsolete environment guard, manual data-directory cleanup, and
  manual worker-build code.
- [ ] Move `pg_worker` preparation into Makefile and CI/tooling, or document
  why the implementation deliberately keeps tests self-contained.
- [ ] Update developer documentation to describe the new test workflow.
- [ ] Run formatting, Markdown, lint, and test gates.
- [ ] Commit the completed adoption work with a clear commit message.

## Surprises & discoveries

- Observation: No implementation has started yet.
  Evidence: this plan is in `DRAFT` status. Impact: implementation agents must
  update this section with actual discoveries rather than treating it as
  historical record.

## Decision Log

- Decision: Target upstream shared-cluster access rather than per-test
  `TestCluster::new()`. Rationale: `pg-embed-setup-unpriv` v0.5.x added shared
  fixtures, `ClusterHandle`, split handle/guard APIs, template helpers, default
  cleanup, and root-worker support. The current repo already uses template
  databases, so sharing the cluster removes the largest remaining startup cost
  without weakening per-test isolation. Date/Author: 2026-06-25, planning agent.

- Decision: Prefer `shared_cluster_handle()` as the implementation target.
  Rationale: it gives direct access to `ClusterHandle`, works naturally with
  `ensure_template_exists()` and `temporary_database_from_template()`, avoids
  copying `wildside`'s custom shutdown wrapper, and aligns with `mxd`'s direct
  handle-based pattern. Date/Author: 2026-06-25, planning agent.

- Decision: Record the fixture architecture in an ADR.
  Rationale: choosing shared embedded PostgreSQL plus template databases is a
  test-runtime architecture decision with operational consequences for root
  agents, CI, and local debugging. Date/Author: 2026-06-25, planning agent.

## Outcomes & retrospective

No implementation outcome exists yet. At completion, summarize which local
helpers were deleted, which upstream APIs replaced them, what happened to
root-agent worker setup, and whether the focused PostgreSQL test runtime
improved.

## Context and orientation

This repository is the Rust crate `diesel-cte-ext`. PostgreSQL integration
tests live in `tests/postgres_recursive.rs`. That file currently imports
`tests/test_helpers.rs`, configures `PG_RUNTIME_DIR`, `PG_DATA_DIR`, and
`PG_PASSWORD`, builds `pg_worker` when running as `root`, starts
`TestCluster::new()` in the `embedded_cluster` rstest fixture, and creates a
per-test `TemporaryDatabase` from a fixed template named `cte_ext_template`.

The dependency is already:

```toml
pg-embed-setup-unpriv = { version = "0.5.1", features = ["diesel-support"] }
```

The current code has already adopted:

- `TestCluster`.
- `TemporaryDatabase`.
- `ensure_template_exists`.
- `temporary_database_from_template`.
- `diesel_connection()`.
- The `diesel-support` feature.

The current code has not fully adopted:

- upstream rstest fixtures such as `test_support::test_cluster` or
  `shared_test_cluster`;
- upstream process-shared cluster handles such as
  `test_support::shared_cluster_handle()`;
- v0.5.0 default `CleanupMode::DataOnly` and partial data-directory recovery
  as replacements for local data-directory deletion;
- v0.5.1 first-class `pg_worker` distribution through normal tooling.

Important local files:

- `Cargo.toml` declares the dev-dependency and features.
- `tests/postgres_recursive.rs` contains the PostgreSQL tests, the local
  `embedded_cluster` fixture, manual worker build helpers, and template
  database helper.
- `tests/test_helpers.rs` contains `EnvVarGuard`, env-var mutation helpers,
  data-directory cleanup, `.pgpass` cleanup, and related unit tests.
- `tests/env_var_guard.rs` tests behaviour that should disappear if
  `EnvVarGuard` is removed.
- `Makefile` defines `make check-fmt`, `make lint`, `make test`,
  `make fmt`, `make markdownlint`, and `make nixie`.
- `.github/workflows/ci.yml` currently runs formatting, Markdown lint, lint,
  and coverage-backed tests but does not explicitly prepare `pg_worker`.
- `docs/developers-guide.md` should describe the adopted testing workflow.
- `docs/pg-embed-setup-unpriv-users-guide.md` already records the upstream
  v0.5.1 guide and should be linked rather than duplicated.

Terms used in this plan:

- Embedded PostgreSQL means a PostgreSQL server downloaded and started by
  `pg-embed-setup-unpriv` for tests.
- A shared cluster means one PostgreSQL server per test process, reused by
  multiple tests.
- A temporary database means a per-test database guard that drops the database
  when the guard is dropped.
- A template database means a database prepared once and cloned for each test
  to keep database setup cheap while preserving isolation.
- `pg_worker` means the helper binary used by `pg-embed-setup-unpriv` to
  perform filesystem work safely when the calling process runs as `root`.

## External references

Use these references during implementation. If the external repositories have
moved, inspect the exact commit links first and only then compare with their
current `main` branches for newer refinements.

- Upstream v0.5.1 README:
  <https://github.com/leynos/pg-embedded-setup-unpriv/blob/0e289523c1d629d9177487ecfe90a60e8312a78f/README.md>
- Upstream v0.5.0 migration guide:
  <https://github.com/leynos/pg-embedded-setup-unpriv/blob/0e289523c1d629d9177487ecfe90a60e8312a78f/docs/v0-5-0-migration-guide.md>
- Upstream shared-cluster implementation:
  <https://github.com/leynos/pg-embedded-setup-unpriv/blob/0e289523c1d629d9177487ecfe90a60e8312a78f/src/test_support/shared_singleton.rs>
- `mxd` shared handle and template strategy:
  <https://github.com/leynos/mxd/blob/7f798acf3873256e333df5aa279b9b78c5ed9fc5/test-util/src/postgres/embedded.rs>
- `mxd` migration-hash template naming:
  <https://github.com/leynos/mxd/blob/7f798acf3873256e333df5aa279b9b78c5ed9fc5/test-util/src/postgres/common.rs>
- `mxd` developer-guide notes on v0.5.0 lifecycle APIs:
  <https://github.com/leynos/mxd/blob/7f798acf3873256e333df5aa279b9b78c5ed9fc5/docs/developers-guide.md>
- `wildside` shared cluster and template database helper:
  <https://github.com/leynos/wildside/blob/e288bc9f1c4be8271ef3135b894e472f32b8111b/backend/tests/support/embedded_postgres.rs>
- `wildside` worker preparation in Makefile:
  <https://github.com/leynos/wildside/blob/e288bc9f1c4be8271ef3135b894e472f32b8111b/Makefile>
- `wildside` developer-guide notes on cache warm-up and serialized
  PostgreSQL-backed tests:
  <https://github.com/leynos/wildside/blob/e288bc9f1c4be8271ef3135b894e472f32b8111b/docs/developers-guide.md>

## Plan of work

### Stage 1: baseline and design confirmation

Read `AGENTS.md`, this plan, `Cargo.toml`, `Makefile`,
`tests/postgres_recursive.rs`, `tests/test_helpers.rs`,
`tests/env_var_guard.rs`, `docs/developers-guide.md`, and
`docs/pg-embed-setup-unpriv-users-guide.md`.

Load `rust-router` and route the work to `rust-unit-testing` because the main
change is a Rust integration-test fixture refactor. Load
`arch-decision-records` before writing the ADR. Use `leta` for symbol-aware
navigation if available; use `rg` for literal strings and documentation.

Run the current focused tests and quality gates to establish baseline
behaviour. Record the command outputs in `Artifacts and notes` if failures are
unrelated to this work.

The focused command is:

```bash
set -o pipefail; cargo test --all-features --test postgres_recursive \
  | tee /tmp/postgres-recursive-baseline-diesel-cte-ext-update-pg-embedded-docs.out
```

If this command fails before any edits because embedded PostgreSQL cannot
download or start in the environment, inspect the failure. Do not start the
refactor until either the environment is repaired or the failure is recorded as
a known baseline with explicit human approval.

### Stage 2: create the ADR

Create `docs/adr/` if it does not exist. Number the ADR sequentially. If there
are still no ADR files, use:

```plaintext
docs/adr/0001-adopt-shared-pg-embed-test-cluster.md
```

The ADR must use the Y-Statement format from the `arch-decision-records` skill.
The decision should say, in plain language, that this crate uses one shared
`pg-embed-setup-unpriv` embedded PostgreSQL cluster per test process and
per-test template-cloned databases for isolation.

The ADR must explicitly decide for:

- `pg_embedded_setup_unpriv::test_support::shared_cluster_handle()` or the
  chosen upstream shared fixture;
- per-test `TemporaryDatabase` values created from a template;
- external Makefile or CI worker preparation when running tests as `root`.

The ADR must explicitly decide against:

- per-test `TestCluster::new()` for ordinary PostgreSQL integration tests;
- local `EnvVarGuard` ownership of `PG_RUNTIME_DIR`, `PG_DATA_DIR`, and
  `PG_PASSWORD`;
- test-code-driven Cargo metadata parsing and worker builds;
- copying `wildside`'s custom `atexit` wrapper;
- adopting `mxd`'s external PostgreSQL abstraction for this crate.

Update `docs/developers-guide.md` to link to the ADR and describe the supported
local testing path.

### Stage 3: write the red test

Add or update the smallest test that proves the desired fixture architecture.
The test must fail before the implementation for the expected reason and pass
after the implementation.

The preferred route is to extract a small helper in
`tests/postgres_recursive.rs`, for example:

```rust,no_run
fn templated_database(cluster: &ClusterHandle) -> BootstrapResult<TemporaryDatabase> {
    // Implementation comes later in the green stage.
}
```

Then add a focused test that obtains the shared cluster handle, creates two
temporary databases from the template, writes a marker table or value to the
first database, and proves the second database does not see that state. This
test should initially fail to compile because the helper still accepts
`&TestCluster` or because `ClusterHandle` has not been imported and wired in.

If a compile-fail red stage is too noisy, use a strict runtime red stage: first
add the test against the old per-test-cluster fixture and assert that two calls
use a shared handle. The assertion should fail before the fixture migration and
pass after it. Remove any temporary expected-failure marker before the green
commit.

Record the red command and the expected failure in `Artifacts and notes`.

### Stage 4: migrate the fixture to upstream shared cluster access

Replace the `embedded_cluster` fixture in `tests/postgres_recursive.rs`. Prefer
direct programmatic access through:

```rust,no_run
use pg_embedded_setup_unpriv::test_support::shared_cluster_handle;
use pg_embedded_setup_unpriv::{BootstrapResult, ClusterHandle, TemporaryDatabase};
```

The target shape is:

```rust,no_run
fn templated_database(cluster: &ClusterHandle) -> BootstrapResult<TemporaryDatabase> {
    let connection = cluster.connection();
    connection.ensure_template_exists(TEMPLATE_DB_NAME, |_db_name| Ok(()))?;
    connection.temporary_database_from_template(next_database_name(), TEMPLATE_DB_NAME)
}
```

Each PostgreSQL test should obtain the handle through
`shared_cluster_handle()?`, create its own `TemporaryDatabase`, and connect with
`cluster.connection().diesel_connection(temp_db.name())?` or `temp_db.url()`
as appropriate.

Keep the fixed `cte_ext_template` name unless template setup starts applying
schema or migrations. If schema or migrations are added, adopt the `mxd` and
`wildside` pattern of using
`pg_embedded_setup_unpriv::test_support::hash_directory()` to derive a template
name from migration contents.

Do not use `shared_test_cluster` if it forces awkward lifetime or fixture
plumbing. The implementation may choose it only if it clearly deletes more
local code than `shared_cluster_handle()` while keeping the helper readable.

### Stage 5: remove obsolete local cleanup and env-var code

Once `tests/postgres_recursive.rs` no longer imports `tests/test_helpers.rs`,
remove obsolete helpers and tests:

- delete `configure_pg_embed_env()`;
- delete `GuardedCluster`;
- delete `WorkerProfile`;
- delete `target_dir()`;
- delete `worker_profile()`;
- delete `pg_worker_path()`;
- delete `cargo_lock_path()`;
- delete `pg_embed_setup_unpriv_version()`;
- delete `parse_cargo_lock_version()`;
- delete `pg_embed_manifest_path()`;
- delete `fetch_cargo_metadata()`;
- delete `find_pg_embed_manifest()`;
- delete `ensure_worker_binary()`;
- delete `worker_env_guard()`;
- delete `tests/test_helpers.rs` if nothing else imports it;
- delete `tests/env_var_guard.rs` if it only tests removed behaviour;
- remove now-unused `serde_json` and `toml` dev-dependencies if nothing else
  imports them.

If any helper remains useful for non-PostgreSQL tests, split that useful piece
into a smaller file with a module-level `//!` comment and keep only the
necessary tests.

### Stage 6: move worker preparation into tooling

Add a Makefile target similar in spirit to `wildside` but smaller. The target
should install or copy `pg_worker` into `target/pg_worker`, then run tests with
`PG_EMBEDDED_WORKER` set. Use the existing Makefile variables and style.

The intended shape is:

```make
PG_WORKER_PATH ?= $(CURDIR)/target/pg_worker
PG_WORKER_INSTALL_ROOT ?= $(CURDIR)/target/pg-worker-root
PG_EMBED_SETUP_UNPRIV_VERSION ?= 0.5.1

prepare-pg-worker:
	mkdir -p "$$(dirname "$(PG_WORKER_PATH)")"
	if command -v pg_worker >/dev/null 2>&1; then \
	  install -m 0755 "$$(command -v pg_worker)" "$(PG_WORKER_PATH)"; \
	else \
	  cargo install --locked --root "$(PG_WORKER_INSTALL_ROOT)" \
	    --version "$(PG_EMBED_SETUP_UNPRIV_VERSION)" --bin pg_worker \
	    pg-embed-setup-unpriv; \
	  install -m 0755 "$(PG_WORKER_INSTALL_ROOT)/bin/pg_worker" \
	    "$(PG_WORKER_PATH)"; \
	fi
```

Update `make test` or add a PostgreSQL-aware prerequisite so the standard test
path prepares the worker before tests. Keep the implementation minimal: this
repository does not need `wildside`'s full binary cache warm-up script unless
CI download time or rate limits prove it necessary.

Update `.github/workflows/ci.yml` only if required for the Makefile change. The
preferred CI shape is to keep invoking `make test`, allowing the Makefile to
own worker preparation.

If the implementation decides tests must remain fully self-contained and cannot
rely on Makefile preparation, record that decision in the ADR and
`Decision Log`, and explain why the local worker-build code remains.

### Stage 7: consider async-api only if it deletes code

The current async PostgreSQL test is a synchronous rstest that creates a Tokio
runtime manually. Do not change this by default.

Only enable `pg-embed-setup-unpriv`'s `async-api` feature and use
`TestCluster::start_async()` or `start_async_split()` if a small prototype
shows that native `#[tokio::test]` coverage is simpler and does not require
duplicating the cluster fixture. If enabled, update `Cargo.toml`, `Cargo.lock`,
and documentation, then add a focused async validation command.

### Stage 8: documentation and cleanup

Update `docs/developers-guide.md` to describe:

- the shared embedded PostgreSQL cluster policy;
- per-test template-cloned databases;
- root-agent `pg_worker` preparation through Makefile or CI tooling;
- how to run the focused PostgreSQL test;
- when to consult `docs/pg-embed-setup-unpriv-users-guide.md`;
- why external PostgreSQL test URLs are not currently supported.

Update this ExecPlan as implementation proceeds. Keep `Progress`,
`Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective`
current.

Run `make fmt` after Markdown changes, then review the diff. Formatting may
touch unrelated Markdown tables. Keep only relevant formatting changes unless
the formatter has made required repository-wide Markdown normalization.

## Concrete steps

All commands run from the repository root:

```bash
pwd
git branch --show-current
git status --short --branch
```

Expected branch:

```plaintext
update-pg-embedded-docs
```

Baseline focused test:

```bash
set -o pipefail; cargo test --all-features --test postgres_recursive \
  | tee /tmp/postgres-recursive-baseline-diesel-cte-ext-update-pg-embedded-docs.out
```

Red-stage focused test:

```bash
set -o pipefail; cargo test --all-features --test postgres_recursive \
  | tee /tmp/postgres-recursive-red-diesel-cte-ext-update-pg-embedded-docs.out
```

Green-stage focused test:

```bash
set -o pipefail; cargo test --all-features --test postgres_recursive \
  | tee /tmp/postgres-recursive-green-diesel-cte-ext-update-pg-embedded-docs.out
```

Full quality gates:

```bash
set -o pipefail; make check-fmt \
  | tee /tmp/check-fmt-diesel-cte-ext-update-pg-embedded-docs.out
set -o pipefail; make markdownlint \
  | tee /tmp/markdownlint-diesel-cte-ext-update-pg-embedded-docs.out
set -o pipefail; make nixie \
  | tee /tmp/nixie-diesel-cte-ext-update-pg-embedded-docs.out
set -o pipefail; make lint \
  | tee /tmp/lint-diesel-cte-ext-update-pg-embedded-docs.out
set -o pipefail; make test \
  | tee /tmp/test-diesel-cte-ext-update-pg-embedded-docs.out
```

If Markdown files changed, run:

```bash
set -o pipefail; make fmt \
  | tee /tmp/fmt-diesel-cte-ext-update-pg-embedded-docs.out
git diff --check
```

Commit after passing gates:

```bash
git status --short
git add Cargo.toml Cargo.lock Makefile .github/workflows/ci.yml \
  tests docs
git commit
```

Use the `commit-message` skill for the final commit message. A suitable subject
is:

```plaintext
Adopt shared embedded Postgres test cluster
```

## Validation and acceptance

The implementation is accepted when all of the following are true:

- `tests/postgres_recursive.rs` no longer starts a fresh `TestCluster` per
  test through the local `embedded_cluster` fixture.
- PostgreSQL tests obtain a shared upstream cluster handle or shared upstream
  fixture and still create one `TemporaryDatabase` per test.
- `tests/test_helpers.rs` and `tests/env_var_guard.rs` are deleted, or reduced
  to only behaviour still required by other tests.
- Manual Cargo metadata parsing and dependency-manifest lookup for building
  `pg_worker` are removed from Rust test code.
- Root-agent worker setup is documented and handled through Makefile or CI
  tooling, unless the ADR records a deliberate exception.
- Local data-directory deletion before bootstrap is removed unless a focused
  test proves it is still required.
- `serde_json` and `toml` dev-dependencies are removed if they become unused.
- An ADR exists under `docs/adr/` and uses the Y-Statement format.
- `docs/developers-guide.md` accurately describes the new workflow.
- The red-stage test fails before implementation for the expected reason and
  passes after implementation.
- `make check-fmt`, `make markdownlint`, `make nixie`, `make lint`, and
  `make test` all pass.

Quality method:

- Use the focused PostgreSQL test during Red-Green-Refactor.
- Use the full Makefile gates before committing.
- Inspect `git diff --check` before commit.

## Idempotence and recovery

The fixture refactor should be idempotent. Re-running `shared_cluster_handle()`
returns the same process-local handle, and re-running
`temporary_database_from_template()` with a unique database name creates a new
per-test database.

If a PostgreSQL bootstrap fails because of a partially initialized data
directory, do not reintroduce manual `rm -rf` cleanup. v0.5.0 added partial
data-directory recovery. Inspect the failure first, then record evidence if the
upstream recovery path is insufficient.

If the new Makefile worker preparation target fails during `cargo install`,
retry the command once after verifying network access and Cargo cache health.
Do not add an isolated Cargo cache. Use the shared default Cargo cache.

If CI fails only because `pg_worker` is unavailable under `root`, prefer fixing
the Makefile or workflow export of `PG_EMBEDDED_WORKER` over restoring Rust
test-code worker builds.

To roll back an incomplete implementation, revert only the current task's
uncommitted changes. Do not reset or overwrite unrelated user changes.

## Artifacts and notes

Initial audit notes to preserve for the implementation agent:

- Current per-test cluster fixture:
  `tests/postgres_recursive.rs` defines `embedded_cluster()` and calls
  `TestCluster::new()`.
- Current template helper:
  `tests/postgres_recursive.rs` defines `templated_database()` and already calls
  `ensure_template_exists()` and `temporary_database_from_template()`.
- Current manual environment guard:
  `tests/test_helpers.rs` defines `EnvVarGuard`, sets `PG_RUNTIME_DIR`,
  `PG_DATA_DIR`, and `PG_PASSWORD`, deletes the data directory, and removes
  `.pgpass`.
- Current manual worker build:
  `tests/postgres_recursive.rs` parses `Cargo.lock`, runs `cargo metadata`,
  finds `pg-embed-setup-unpriv`, and builds `pg_worker`.
- Upstream v0.5.1 `shared_cluster_handle()` internally uses
  `ensure_worker_env()`, `TestCluster::new_split()`, leaks the guard for
  process lifetime, and registers a best-effort shutdown hook.
- `mxd` is the closest direct pattern for `ClusterHandle` plus template
  database creation.
- `wildside` is useful for Makefile worker preparation and CI cache policy,
  but its custom `atexit` wrapper predates the relevant v0.5.1 upstream
  shutdown hook and should not be copied.

## Interfaces and dependencies

Use these upstream APIs from `pg-embed-setup-unpriv` v0.5.1:

```rust,no_run
use pg_embedded_setup_unpriv::test_support::shared_cluster_handle;
use pg_embedded_setup_unpriv::{BootstrapResult, ClusterHandle, TemporaryDatabase};
```

The final helper in `tests/postgres_recursive.rs` should have a small,
test-local surface similar to:

```rust,no_run
fn templated_database(cluster: &ClusterHandle) -> BootstrapResult<TemporaryDatabase>;
```

The tests should use:

```rust,no_run
let cluster = shared_cluster_handle()?;
let temp_db = templated_database(cluster)?;
let mut conn = cluster.connection().diesel_connection(temp_db.name())?;
```

The async coverage may continue to use:

```rust,no_run
let db_url = temp_db.url().to_owned();
```

Only add the `async-api` feature if Stage 7 approves native async cluster
startup:

```toml
pg-embed-setup-unpriv = { version = "0.5.1", features = ["diesel-support", "async-api"] }
```

No production dependency changes are expected. `serde_json` and `toml` should
be removed from `[dev-dependencies]` if their only use was the manual
worker-build path.

## Revision note

Initial draft created on 2026-06-25. This plan captures the v0.5.1 adoption
gaps, the required ADR, cross-repository references from `mxd` and `wildside`,
and a milestone-by-milestone route for another agent to implement the work.
