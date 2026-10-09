# Developer guide

## Contributor toolchain

The pinned Rust toolchain is **1.94.0** and includes `rustfmt`, `clippy`, and
`rust-analyzer`. The 1.94.0 pin was set when the `pg-embed-setup-unpriv` 0.5.2
test dependency required that release through its transitive crates. With 0.6.3
locked, the highest `rust-version` any locked package declares is 1.92
(`postgresql_embedded`, `postgresql_commands`, `postgresql_archive` and
`pg-embed-setup-unpriv` itself), read from `cargo metadata --locked`, so the
dependency tree no longer requires 1.94.0. The pin stays at 1.94.0 because
lowering the repository's toolchain is a separate decision that this dependency
bump does not make; the declared floors were not verified by a build on 1.92.
Run `rustup toolchain install` from the repository root after changing
`rust-toolchain.toml` so local language-server, formatting, and linting
behaviour stays aligned with Continuous Integration (CI). Run `make typecheck`
to type-check every target with all features enabled.

## Spelling policy

Run the spelling gate with:

```bash
make spelling
```

`TYPOS_CONFIG_BUILDER_VERSION` in the `Makefile` pins the
`typos-config-builder` release the gate runs (currently `v0.1.3`). Raise it
together with the regenerated `typos.toml`, never on its own.

The gate enforces en-GB-oxendict spelling in tracked Markdown prose.
`make markdownlint` depends on it, so linting Markdown also checks spelling.

The tracked `typos.toml` is regenerated on every run from the live shared
dictionary and the repository-specific `typos.local.toml` overlay. Because the
dictionary is live, `typos.toml` must never be drift checked in Continuous
Integration (CI); a regenerated file differing from the committed copy is
expected.

Do not edit generated entries in `typos.toml`. Put only narrow
repository-specific proper nouns, quoted upstream titles, fixtures, stems or
exclusions in `typos.local.toml`.

The shared dictionary is maintained in `leynos/agent-helper-scripts`. Its
repository-local cache and freshness metadata are untracked. The gate replaces
the cache only when the authoritative copy is newer and reuses a valid cached
copy while offline. The gate also enforces exact phrase corrections that Typos
cannot match, because it splits hyphenated phrases into separate words.

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

## Coverage publication

Pull-request continuous integration (CI) generates LCOV coverage and ratchets
it against the baseline written by `coverage-main.yml`. The pull-request lane
publishes no coverage artefact, never contacts CodeScene, and never receives
`CS_ACCESS_TOKEN`, so a change in CodeScene's application programming interface
(API) cannot hold a pull request.

`coverage-main.yml` is the only publisher. On each push to `main` it refreshes
the ratchet baseline and uploads the report to CodeScene. It also runs on
demand through `workflow_dispatch`, for merges that fire no push event: a
dispatch on `main` uploads a fresh report, but the shared action advances the
baseline only on a push, so the ratchet catches up at the next push to `main`.
No `env` binds `CS_ACCESS_TOKEN`: a check step writes whether the secret is
set, from an expression evaluated before its shell runs, and the upload step
receives the token only as its `access-token` input, because the uploader is a
composite action that would pass its step's `env` to its nested steps. The
upload runs only when the token is present and the ref is `refs/heads/main`, so
a dispatch from a branch cannot publish that branch's coverage as the trunk's.

The publisher's concurrency group is keyed on the ref alone and never cancels a
run in progress, so runs on `main` never overlap, and a newer trigger replaces
an older pending run rather than queueing behind it. GitHub does not promise to
start runs in trigger order, so this does not guarantee commit order: an older
run that starts late can publish its commit's coverage after a newer one, and
the next push supersedes it. A manual re-run of an older run keeps its SHA and
its run id: it republishes that commit's coverage to CodeScene, but replaces no
ratchet baseline while the original run's cache entry survives, because the
shared action saves each baseline under a key that includes the run id. If that
entry is gone, never saved or since evicted, a re-run of a push saves the older
commit's baseline again, the shared action restores the newest entry under the
key prefix, and later ratchets read the older baseline until the next push
saves a newer one. That stale-order risk is accepted. A re-run of a dispatch
saves no baseline, since the shared action saves one only on a push.

Two gaps are known and accepted, and both are tracked in
[shared-actions issue 518](https://github.com/leynos/shared-actions/issues/518):

- Merges made by the Dependabot automerge workflow use `GITHUB_TOKEN` and fire
  no push event, so they are measured only at the next push to `main` or a
  manual dispatch.
- A dispatch that replaces a pending push writes no baseline, since the shared
  action saves one only on a push, so the ratchet baseline stays behind until
  the next push. A dispatch made before a push can also reach the concurrency
  group after it, replace it and upload the older commit; that is part of the
  stale-order risk accepted above.

No other workflow a push starts, directly or through a local call, may generate
coverage outside the pull-request guard, and none, the publisher included, may
run a local action, whose `action.yml` the contract does not read, so the
publisher is the only baseline writer. Both coverage steps select the same
inputs at the same `shared-actions` pin because the pull-request ratchet is
only meaningful against a baseline measured the same way.

`make test-workflow-contracts` holds this shape by running
`cv005-contracts check`, the shared contract library in `leynos/shared-actions`
(`packages/cv005-contracts`), from a full commit named by `CV005_CONTRACTS_REF`
in the Makefile, and CI runs it as its own step. A fix to the rules is
therefore a pin bump. The target needs `uv`, which fetches the Python 3.13 the
library runs under. The repository's parameters are in `.github/cv005.toml`: its
`repository` name and the `[selection]` inputs the baseline measures, which
the publisher's generator must carry and every pull-request lane must match.
The library's own suite proves each rule refuses the shape it exists to refuse,
so this repository keeps no copy of the readers or the refusal cases. Its rules
read every workflow a pull request can start, from its own events, reviews and
comments, a merge queue, or a push not confined to `main` or tags, following
local reusable-workflow calls, `workflow_run` chains and local composite
actions, and refuse any mention of the CodeScene host, uploader, client, or
token there. They also refuse `continue-on-error` wherever it would turn a
failed ratchet or upload green, and they read workflows strictly, so a
duplicate key is refused rather than silently resolved.

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

## The build standard

Development, test, lint, and typecheck builds use the `mold` linker on Linux;
the parallel frontend flag is nightly-only, and the pinned `1.94.0` is stable,
so it is not used. These are defaults in `.cargo/config.toml`, which Cargo
discovers on its own, so a bare `cargo build` gets them. `mold` ships for Linux
only, so the linker flag lives in a Linux-only table and macOS and Windows keep
their platform linker. Cargo selects one `rustflags` source rather than merging
them, so every source repeats the same flags apart from the linker.

An assigned `RUSTFLAGS` replaces the configuration's flags, so the Makefile
recipes that set it compose the standard's flags onto any inherited value (CI's
`setup-rust` exports one). Two builds are deliberately excluded: coverage
assigns `RUSTFLAGS` without the fast flags, because a measurement should not
depend on them, and the release recipe and workflow keep the platform linker,
because they assign `RUSTFLAGS` (even an empty value displaces the
configuration). Cargo has no per-profile `rustflags`, so a direct
`cargo build --release` takes the configuration's flags unless `RUSTFLAGS` is
assigned too.

On Linux, install `mold` before building: the configuration names it, so a
build without it fails at link time. CI installs it through `setup-rust`'s
`install-mold` input. `tests/build_standard_contract.rs` holds the standard. It
reads the configuration sources, the commands `make -n` prints for each
development target on a Linux host and a macOS host (each keeping the caller's
own `RUSTFLAGS`) and for each coverage and release target on a Linux host, and
the `setup-rust` steps of the CI workflows (each must pass `install-mold`), so
a flag lost through a recipe or workflow edit fails there. The decision is
recorded in [ADR 001](adr-001-rust-build-standard.md).

### Cranelift

Exception: Cranelift is not the development-profile backend, because Cranelift
requires a nightly toolchain and this repository pins the stable `1.94.0`
(recorded 2026-09-29). Revisit if the repository moves to a nightly pin.
