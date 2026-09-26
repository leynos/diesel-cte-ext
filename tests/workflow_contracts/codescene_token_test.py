"""Prove where the publisher may use CS_ACCESS_TOKEN, and what the jobs may hold.

No `env` binds the token. A check step with no `env` runs one exact command
whose expression Actions evaluates before the shell runs, and the token reaches
the uploader only as its `access-token` input: the uploader is a composite
action, so a token in its step's `env` would reach the nested upload-artifact
and cache steps too. Every test mutates a copy of this repository's workflows
and asserts that the clause meant to catch the edit does.
"""

from __future__ import annotations

import typing as typ

import pytest
from codescene_contract_support import (
    CREDENTIAL_REFERENCE,
    LANE,
    Documents,
    assert_clean,
    assert_reports,
    coverage_step,
    find_publisher,
    first_job,
    job_steps,
)
from codescene_coverage_rules import coverage_violations
from codescene_publisher_rules import publisher_violations

if typ.TYPE_CHECKING:
    from codescene_workflow_reader import Step


def _check_step(documents: Documents) -> Step:
    """Return the publisher's token check step."""
    publisher, _ = find_publisher(documents)
    return next(s for s in job_steps(publisher) if s.get("id") == "codescene-token")


@pytest.mark.parametrize("scope", ["upload step", "check step", "job", "workflow"])
def test_token_is_bound_in_no_env(documents: Documents, scope: str) -> None:
    """No env in the publisher may bind the token.

    The composite uploader would pass its own step's env to its nested steps,
    and a job or workflow env reaches every step beneath it.
    """
    publisher, upload = find_publisher(documents)
    targets = {
        "upload step": upload,
        "check step": _check_step(documents),
        "job": first_job(publisher),
        "workflow": publisher,
    }
    targets[scope]["env"] = {"CS_ACCESS_TOKEN": CREDENTIAL_REFERENCE}
    assert_reports(publisher_violations, documents, "binds CS_ACCESS_TOKEN in an env")


def test_token_cannot_reach_another_upload_input(documents: Documents) -> None:
    """The upload carries the token only as its access-token input."""
    _, upload = find_publisher(documents)
    typ.cast("dict[str, object]", upload["with"])["cli-version"] = CREDENTIAL_REFERENCE
    assert_reports(publisher_violations, documents, "outside its two uses")


def test_token_cannot_reach_another_step(documents: Documents) -> None:
    """A later step naming the token in its script is refused."""
    publisher, _ = find_publisher(documents)
    job_steps(publisher).append({"run": f"echo {CREDENTIAL_REFERENCE}"})
    assert_reports(publisher_violations, documents, "outside its two uses")


def test_a_copied_check_step_is_still_swept(documents: Documents) -> None:
    """A copy of the check step in another job compares equal to the original.

    The sweep removes the approved steps by identity, so the copy still counts
    as the token outside its two uses.
    """
    publisher, _ = find_publisher(documents)
    other = {"runs-on": "ubuntu-latest", "steps": [dict(_check_step(documents))]}
    typ.cast("dict[str, object]", publisher["jobs"])["other"] = other
    assert_reports(publisher_violations, documents, "outside its two uses")


def test_check_step_must_exist(documents: Documents) -> None:
    """Without it the guard's output is never written and the upload never runs."""
    publisher, _ = find_publisher(documents)
    job_steps(publisher).remove(_check_step(documents))
    assert_reports(publisher_violations, documents, "needs one `codescene-token`")


def test_check_step_must_precede_the_upload(documents: Documents) -> None:
    """An output written after the upload cannot enable it."""
    publisher, upload = find_publisher(documents)
    held = job_steps(publisher)
    check = _check_step(documents)
    held.remove(check)
    held.insert(held.index(upload) + 1, check)
    assert_reports(publisher_violations, documents, "must run before the upload")


@pytest.mark.parametrize(
    ("change", "expected"),
    [
        ({"if": "false"}, "may carry only"),
        ({"shell": "python {0}"}, "may carry only"),
        ({"continue-on-error": True}, "must not continue on error"),
        # Always true, so a fork without the secret would fail the upload.
        ({"run": 'echo "available=true" >> "$GITHUB_OUTPUT"'}, "must run exactly"),
        (
            {"run": "echo \"available=${{ secrets.CS_ACCESS_TOKEN != '' }}\""},
            "must run exactly",
        ),
        (
            {
                "run": "false && echo \"available=${{ secrets.CS_ACCESS_TOKEN != '' }}\""
                ' >> "$GITHUB_OUTPUT"'
            },
            "must run exactly",
        ),
    ],
)
def test_check_step_runs_one_exact_command(
    documents: Documents, change: dict[str, object], expected: str
) -> None:
    """The check step can neither be skipped nor run anything else."""
    _check_step(documents).update(change)
    assert_reports(publisher_violations, documents, expected)


@pytest.mark.parametrize(
    ("permissions", "expected"),
    [
        ({"contents": "write"}, "upload job permissions"),
        (None, "upload job permissions"),
    ],
)
def test_upload_job_token_is_read_only(
    documents: Documents, permissions: object, expected: str
) -> None:
    """Nothing in the upload job writes to the repository."""
    publisher, _ = find_publisher(documents)
    first_job(publisher)["permissions"] = permissions
    assert_reports(publisher_violations, documents, expected)


def test_publisher_checkout_keeps_no_credentials(documents: Documents) -> None:
    """The coverage run executes repository and dependency code after checkout."""
    publisher, _ = find_publisher(documents)
    checkout = next(
        s for s in job_steps(publisher) if "actions/checkout" in str(s.get("uses"))
    )
    checkout.pop("with", None)
    assert_reports(publisher_violations, documents, "persist-credentials: false")


def test_pull_request_coverage_token_is_read_only(documents: Documents) -> None:
    """The pull-request lane runs contributed code with its job's token."""
    first_job(documents[LANE]).pop("permissions", None)
    assert_reports(coverage_violations, documents, "coverage job permissions")


#: A replacement for each upload input that differs from what coverage wrote.
OTHER_VALUE: typ.Final[dict[str, dict[str, str]]] = {
    "path": {"lcov.info": "coverage.xml", "coverage.xml": "lcov.info"},
    "format": {"lcov": "cobertura", "cobertura": "lcov"},
}


@pytest.mark.parametrize(
    ("key", "expected"),
    [("path", "coverage step's output-path"), ("format", "coverage step's format")],
)
def test_upload_reads_what_coverage_wrote(
    documents: Documents, key: str, expected: str
) -> None:
    """An upload of another file or format publishes nothing the run measured."""
    _, upload = find_publisher(documents)
    inputs = typ.cast("dict[str, str]", upload["with"])
    inputs[key] = OTHER_VALUE[key][inputs[key]]
    assert_reports(coverage_violations, documents, expected)


@pytest.mark.parametrize(
    "path",
    [
        "{report}",
        "./{report}",
        ".",
        "**/*",
        "${{{{ github.workspace }}}}",
        "../repo",
        "/home/runner/work",
        "dist/\n{report}",
    ],
)
def test_pull_request_lane_cannot_upload_the_report(
    documents: Documents, path: str
) -> None:
    """`publish-artefact: 'false'` is moot if another step uploads the report.

    The path is read as a matcher: the report itself, the workspace root, a
    glob or an expression could all include it.
    """
    report = typ.cast("dict[str, object]", coverage_step(documents[LANE])["with"])
    job_steps(documents[LANE]).append({
        "uses": "actions/upload-artifact@v4",
        "with": {"name": "coverage", "path": path.format(report=report["output-path"])},
    })
    assert_reports(coverage_violations, documents, "must not upload the coverage")


def test_a_directory_holding_the_report_is_refused(documents: Documents) -> None:
    """A directory upload includes every file beneath it, the report too."""
    publisher, upload = find_publisher(documents)
    report = "target/cov/lcov.info"
    for step in (coverage_step(documents[LANE]), coverage_step(publisher)):
        typ.cast("dict[str, object]", step["with"])["output-path"] = report
    typ.cast("dict[str, object]", upload["with"])["path"] = report
    job_steps(documents[LANE]).append({
        "uses": "actions/upload-artifact@v4",
        "with": {"name": "coverage", "path": "target/cov"},
    })
    assert_reports(coverage_violations, documents, "must not upload the coverage")


@pytest.mark.parametrize("path", ["dist", "target/nextest/junit.xml\n!**/lcov.info"])
def test_pull_request_lane_may_upload_other_files(
    documents: Documents, path: str
) -> None:
    """A literal path that is neither the report nor its directory is cleared."""
    job_steps(documents[LANE]).append({
        "uses": "actions/upload-artifact@v4",
        "with": {"name": "other", "path": path},
    })
    assert_clean(coverage_violations, documents)
