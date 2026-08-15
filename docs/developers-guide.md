# Developer guide

## Contributor toolchain

The pinned Rust toolchain is **1.94.0** and includes `rustfmt`, `clippy`, and
`rust-analyzer`. The 1.94.0 floor comes from the `pg-embed-setup-unpriv` 0.5.2
test dependency: its transitive crates (`postgresql_embedded` and `sqlx`)
require that release, so building the `pg_worker` helper or running `make test`
on an older toolchain fails during dependency resolution. Run
`rustup toolchain install` from the repository root after changing
`rust-toolchain.toml` so local language-server, formatting, and linting
behaviour stays aligned with Continuous Integration (CI). Run `make typecheck`
to type-check every target with all features enabled.

## Spelling policy

Run `make markdownlint` to lint Markdown and enforce en-GB-oxendict spelling.
The target uses the `TYPOS_VERSION` pin in the `Makefile`, tests the policy
helper, refreshes the shared base dictionary, generates `typos.toml`, and scans
tracked Markdown files.

The shared dictionary is maintained in `leynos/agent-helper-scripts`. Its
repository-local cache and freshness metadata are untracked. The helper
replaces the cache only when the authoritative copy is newer and can reuse a
valid cached copy while offline. A clean checkout with an unavailable network
retains the reviewed, tracked `typos.toml` policy.

Do not edit generated entries in `typos.toml`. Put only repository-specific
proper nouns, quoted upstream titles, fixtures, stems or exclusions in
`typos.local.toml`, then regenerate with:

```bash
uv run scripts/generate_typos_config.py
```

Keep upstream API spellings in inline or fenced code where practical. The
spelling gate deliberately ignores code spans and fenced code blocks.

## Recursive search ordering

Recursive CTE search ordering is exposed through `SearchStyle` and
`WithRecursive::with_search`. `SearchStyle` is public because callers choose
between breadth-first and depth-first traversal. `SearchConfig` stays
crate-private because it is only the builder's stored rendering state; callers
should not construct or inspect it directly.

`with_search` accepts either one static column name or a static list of column
names. The static requirement matches the rest of the builder API, which stores
identifier names by reference and lets Diesel quote them during SQL rendering.
The renderer rejects empty and duplicate search-column lists before emitting
SQL.

PostgreSQL supports the SQL-standard `SEARCH ... BY ... SET ...` clause, so the
query fragment renders it only for `diesel::pg::Pg`. SQLite does not support
`SEARCH` or `CYCLE`, and other backends should not receive silently unsupported
syntax. The backend gate therefore returns a query-builder error when
`search_config` is present for any non-PostgreSQL backend.

## Mutation testing

The scheduled `cargo-mutants` workflow builds with `--all-features`. A mutant
inside a variant excluded by that feature set cannot be observed by the run,
even when another supported feature configuration exercises the behaviour.

The shared-workflow reference remains pinned to a commit SHA for supply-chain
integrity. Dependabot manages that SHA; do not duplicate it in tests or
documentation, because Dependabot updates only the workflow reference.

Use `#[cfg_attr(test, mutants::skip)]` only for such configuration-bound
mutants. Keep the attribute on the narrowest affected item and add a comment
that identifies the excluded configuration, the existing behavioural coverage,
and the tracking issue. The `mutants` dev-dependency exists solely so these
test-only attributes resolve. Do not skip mutants that can be exercised by the
workflow's feature set; strengthen the relevant tests instead.

### Workflow contract tests

`.github/workflows/mutation-testing.yml` is a thin caller of the shared
reusable workflow `leynos/shared-actions/.github/workflows/mutation-cargo.yml`.
The heavy lifting — running `cargo-mutants` and summarizing survivors — lives in
`shared-actions`; this repository carries only declarative configuration. The
run is informational only: it never gates a pull request, and survivors are
reported through the job summary and downloadable artefacts for triage into
tests rather than enforced as a blocking check.

The caller currently sets three inputs:

- `exclude-globs: "src/test_support.rs"` — keeps survivors from the unit-test
  scaffolding module out of the report, since they are noise rather than
  genuine test gaps.
- `extra-args: "--all-features"` — mirrors the repository's canonical test
  baseline (`make test` runs with all features enabled), so feature-gated code
  is compiled and exercised against mutants.
- `setup-commands` — pins `PG_PASSWORD` for the embedded PostgreSQL cluster.
  The `pg-embed-setup-unpriv` library otherwise generates a fresh random
  superuser password per run while cluster state persists between runs, so a
  second plain `cargo test` in the same job would fail to authenticate; a fixed
  password keeps every per-mutant run consistent with the baseline run.

Every other input keeps the shared workflow's default, including `paths`, which
this repository does not set.

`tests/workflow_contracts/mutation_testing_test.py` parses the caller with
PyYAML and pins the shape it must uphold, failing the pull request when the
caller drifts rather than letting the breakage surface only in a scheduled run.
Run it locally with `make test-workflow-contracts` (this wraps
`uv run --with 'pytest>=8' --with 'pyyaml>=6' pytest tests/workflow_contracts -q`).
The test validates:

- job permissions are exactly least-privilege (`contents: read`,
  `id-token: write`), and the workflow-level default token scope is empty;
- `concurrency` serializes runs per ref (`cancel-in-progress: false`);
- the daily 09:35 UTC schedule remains, and `workflow_dispatch` is present
  without the legacy `branch` input; and
- `uses:` names the shared `mutation-cargo.yml` workflow and ends with a
  40-character hexadecimal commit SHA; and
- the `with:` block carries exactly the three inputs above and no others.

The test validates only the SHA's shape, not its value. Dependabot continues to
manage the exact pin, so its updates do not require a matching test change; the
contract merely prevents the reusable workflow from being repointed at a
branch, tag, or abbreviated revision.

## Compile-fail UI tests

Compile-time contracts for macros and type-level behaviour are covered by the
`trybuild` harness in `tests/trybuild.rs`. Add CTE-focused fixtures under
`tests/ui/` with the `cte_` prefix. Keep each fixture self-documenting with a
`//!` module comment that states the guarantee being protected and `///`
comments on the fixture functions, including `main`, that name the invalid
invocation being exercised.

Use `compile_fail` fixtures for invalid macro invocations, invalid type-level
column combinations, and builder inputs that should fail Diesel trait bounds at
compile time. Keep runtime SQL rendering assertions focused on valid behaviour
in the existing unit and integration tests.

Refresh asserted diagnostics only after deliberately changing the expected
compiler output:

```bash
TRYBUILD=overwrite cargo test --test trybuild --all-features
cargo test --test trybuild --all-features
```

The 17-column `table_columns!` fixture needs Diesel's `32-column-tables`
feature enabled for dev builds. Without that feature, Diesel's own `table!`
macro rejects the table definition before this crate reaches its `ColumnNames`
boundary. Keep that feature in `[dev-dependencies]` only, so production feature
selection remains unchanged.

## PostgreSQL test support

This crate uses `pg-embed-setup-unpriv` for PostgreSQL-backed integration tests
because embedded PostgreSQL setup crosses process, filesystem, and privilege
boundaries. Keeping that behaviour in a dedicated helper avoids scattering
directory ownership, PostgreSQL binary caching, password file handling, and
cluster lifecycle policy through this crate's tests.

The dependency is especially important for sandboxed agentic development. These
workspaces often run automation as `root`, while PostgreSQL refuses to
initialize or run as `root`. `pg-embed-setup-unpriv` detects that case,
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

Use `make test` for the supported local test workflow. The target uses `jq` to
locate the locked `pg-embed-setup-unpriv` dependency manifest, builds its
`pg_worker` binary into `target/pg_worker`, exports `PG_EMBEDDED_WORKER`, and
then runs `cargo test --all-targets --all-features`. This keeps root-agent runs
aligned with CI and avoids hidden worker builds inside Rust test code.

`prepare-pg-worker` derives the worker install source from `PG_WORKER_PROFILE`
so the copied binary follows Cargo's built-in output directories:

- `dev` and `test` use `target/debug/pg_worker`;
- `release` and `bench` use `target/release/pg_worker`;
- custom profiles use `target/<profile>/pg_worker`.

The recipe validates that `cargo metadata --locked` returned a
`pg-embed-setup-unpriv` `manifest_path`, then chains manifest lookup, build,
and install with `&&`. This fail-fast flow prevents a missing manifest or
failed build from falling through to a misleading install step. Run
`make test-prepare-pg-worker` to exercise the profile mapping and empty
manifest error path without performing a real Cargo build.

For the focused PostgreSQL integration test as an unprivileged user, run:

```bash
cargo test --all-features --test postgres_recursive
```

Root invocations that bypass `make test` must prepare and export
`PG_EMBEDDED_WORKER` themselves:

```bash
make prepare-pg-worker
PG_EMBEDDED_WORKER="${PWD}/target/pg_worker" \
  cargo test --all-features --test postgres_recursive
```

This crate does not currently support external PostgreSQL test URLs. The test
suite relies on embedded PostgreSQL so local, CI and sandboxed agent runs use
the same lifecycle and cleanup behaviour.

For usage details, see
[`docs/pg-embed-setup-unpriv-users-guide.md`](pg-embed-setup-unpriv-users-guide.md).
