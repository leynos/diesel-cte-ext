"""Contract tests for the mutation-testing caller workflow.

The executable logic lives in the ``leynos/shared-actions`` reusable
workflow, which carries its own unit and integration tests;
diesel-cte-ext's caller is declarative configuration. These tests parse
the caller with PyYAML and pin the contract it must uphold, so drift
(repointing the pin at a branch, widening permissions, or losing the
scaffolding exclusion and feature arguments) fails CI on the pull
request rather than surfacing in a scheduled or manual run.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import re
from pathlib import Path

import yaml

WORKFLOW_PATH = (
    Path(__file__).resolve().parents[2] / ".github" / "workflows" / "mutation-testing.yml"
)

#: The exact caller configuration: exclude the unit-test scaffolding
#: module (survivors there are noise), mirror the repository's
#: canonical test baseline (`make test` runs --all-features), and pin
#: PG_PASSWORD so repeated plain `cargo test` runs against the
#: persistent embedded PostgreSQL cluster keep authenticating (the
#: library otherwise generates a fresh random password per run, so the
#: second and subsequent per-mutant runs fail to connect). Every other
#: input keeps the reusable workflow's default.
EXPECTED_WITH = {
    "exclude-globs": "src/test_support.rs",
    "extra-args": "--all-features",
    "setup-commands": (
        'echo "PG_PASSWORD=cargo-mutants-embedded-pg" >> "$GITHUB_ENV"\n'
    ),
}


USES_RE = re.compile(
    r"^leynos/shared-actions/.github/workflows/mutation-cargo\.yml@[0-9A-Fa-f]{40}$"
)


def _load() -> dict[str, object]:
    """Parse the workflow file."""
    return yaml.safe_load(WORKFLOW_PATH.read_text(encoding="utf-8"))


def _triggers(workflow: dict[str, object]) -> dict[str, object]:
    """Return the ``on:`` mapping (PyYAML parses the bare key as True)."""
    triggers = workflow.get("on", workflow.get(True))
    assert isinstance(triggers, dict), "the workflow must declare an on: mapping"
    return triggers


def _mutation_job(workflow: dict[str, object]) -> dict[str, object]:
    """Return the single calling job."""
    jobs = workflow.get("jobs")
    assert isinstance(jobs, dict), "the workflow must declare a jobs mapping"
    assert jobs, "the workflow must declare at least one job"
    assert list(jobs) == ["mutation"], (
        f"expected a single job named 'mutation', found {sorted(jobs)}"
    )
    return jobs["mutation"]


def test_job_permissions_are_exactly_least_privilege() -> None:
    """The job grants contents: read and id-token: write, nothing broader."""
    permissions = _mutation_job(_load()).get("permissions")
    assert permissions == {"contents": "read", "id-token": "write"}, (
        "jobs.mutation.permissions must be exactly "
        f"{{'contents': 'read', 'id-token': 'write'}}, got {permissions!r}"
    )


def test_workflow_default_permissions_are_empty() -> None:
    """The workflow-level default token scope is empty."""
    workflow = _load()
    assert workflow.get("permissions") == {}, (
        f"top-level permissions must be an empty mapping, got "
        f"{workflow.get('permissions')!r}"
    )


def test_concurrency_serializes_per_ref_without_cancelling() -> None:
    """Runs queue per ref instead of cancelling one another."""
    concurrency = _load().get("concurrency")
    assert isinstance(concurrency, dict), "the workflow must declare concurrency"
    assert concurrency.get("group") == "mutation-testing-${{ github.ref }}", (
        f"concurrency.group must key on the triggering ref, got "
        f"{concurrency.get('group')!r}"
    )
    assert concurrency.get("cancel-in-progress") is False, (
        f"concurrency.cancel-in-progress must be false, got "
        f"{concurrency.get('cancel-in-progress')!r}"
    )


def test_triggers_keep_schedule_and_plain_dispatch() -> None:
    """The daily schedule stays; dispatch has no legacy branch input."""
    triggers = _triggers(_load())
    schedule = triggers.get("schedule")
    assert schedule == [{"cron": "35 9 * * *"}], (
        f"on.schedule must be the daily 09:35 UTC cron, got {schedule!r}"
    )
    assert "workflow_dispatch" in triggers, "on.workflow_dispatch is missing"
    dispatch = triggers.get("workflow_dispatch") or {}
    inputs = dispatch.get("inputs") or {}
    assert "branch" not in inputs, (
        "on.workflow_dispatch must not declare a branch input; the Actions "
        "run-workflow control selects the ref"
    )


def test_mutation_job_uses_a_pinned_shared_workflow() -> None:
    """The reusable workflow reference keeps a full commit-SHA pin."""
    uses = _mutation_job(_load()).get("uses")
    assert isinstance(uses, str), "jobs.mutation.uses must be a string"
    assert USES_RE.fullmatch(uses), (
        "jobs.mutation.uses must reference mutation-cargo.yml at a "
        f"40-character commit SHA, got {uses!r}"
    )


def test_with_block_carries_the_caller_configuration() -> None:
    """The caller passes exactly the scaffolding exclusion and feature args."""
    with_block = _mutation_job(_load()).get("with")
    assert isinstance(with_block, dict), "jobs.mutation.with is missing"
    assert with_block == EXPECTED_WITH, (
        f"jobs.mutation.with must be exactly {EXPECTED_WITH!r} (scaffolding "
        f"exclusion plus the --all-features baseline; all other inputs keep "
        f"the shared workflow's defaults), got {with_block!r}"
    )
