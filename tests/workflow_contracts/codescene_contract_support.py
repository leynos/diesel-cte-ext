"""Shared inputs for the CV-005 contract tests.

The contract tests mutate a copy of this repository's workflows in the way a
later edit could, and assert that the clause meant to catch that edit does.
These helpers hand each test its own copy and find the parts they mutate.
"""

from __future__ import annotations

import typing as typ
from pathlib import Path

from codescene_publisher_rules import upload_steps
from codescene_workflow_reader import Document, Step, read_workflows

if typ.TYPE_CHECKING:
    import collections.abc as cabc

type Documents = dict[str, Document]
type Rule = cabc.Callable[[Documents], list[str]]

WORKFLOWS: typ.Final[Path] = (
    Path(__file__).resolve().parents[2] / ".github" / "workflows"
)
#: The pull-request lane, which the mutation cases extend.
LANE: typ.Final[str] = "ci.yml"
PROBE: typ.Final[str] = "probe.yml"
CREDENTIAL_REFERENCE: typ.Final[str] = "${{ secrets.CS_ACCESS_TOKEN }}"
#: The publisher's coverage selection, pinned so that both lanes changing
#: together cannot pass the parity rule unseen.
EXPECTED_SELECTION: typ.Final[dict[str, object]] = {
    "output-path": "lcov.info",
    "format": "lcov",
    "use-cargo-nextest": "false",
    "with-ratchet": "true",
}


def fresh_documents(directory: Path = WORKFLOWS) -> Documents:
    """Read a private copy of the workflows for one test to mutate.

    Read afresh each time rather than cached for the process, so no test can
    see another's mutation and a read failure surfaces in the test that met
    it, as the reader's `WorkflowError`.

    Parameters
    ----------
    directory : Path
        The workflow directory; this repository's by default.

    Returns
    -------
    Documents
        Each workflow's parsed document, keyed by file name.

    Raises
    ------
    WorkflowError
        If the workflows cannot be read; see `read_workflows`.

    Examples
    --------
    >>> "coverage-main.yml" in fresh_documents()
    True

    """
    return read_workflows(directory)


def assert_clean(rule: Rule, documents: Documents) -> None:
    """Fail unless a rule reports nothing.

    Parameters
    ----------
    rule : Rule
        The rule to run.
    documents : Documents
        The workflows to judge.

    Raises
    ------
    AssertionError
        If the rule reports any violation, naming them all.

    Examples
    --------
    >>> assert_clean(lambda documents: [], {})

    """
    found = rule(documents)
    if found:
        message = f"expected no violations, got {found}"
        raise AssertionError(message)


def assert_reports(rule: Rule, documents: Documents, fragment: str) -> None:
    """Fail unless a rule reports a violation containing a fragment.

    Parameters
    ----------
    rule : Rule
        The rule to run.
    documents : Documents
        The workflows to judge.
    fragment : str
        Text that at least one reported violation must contain.

    Raises
    ------
    AssertionError
        If no reported violation contains the fragment.

    Examples
    --------
    >>> assert_reports(lambda documents: ["ci.yml names the host"], {}, "host")

    """
    found = rule(documents)
    if not any(fragment in problem for problem in found):
        message = f"expected a violation naming {fragment!r}, got {found}"
        raise AssertionError(message)


def find_publisher(documents: Documents) -> tuple[Document, Step]:
    """Return the publisher document and its upload step.

    Parameters
    ----------
    documents : Documents
        The workflows to search.

    Returns
    -------
    tuple of (Document, Step)
        The one workflow calling the CodeScene uploader, and that step.

    Raises
    ------
    ValueError
        Unless exactly one step in all the workflows calls the uploader.

    Examples
    --------
    >>> publisher, upload = find_publisher(fresh_documents())
    >>> upload["with"]["mode"]
    'upload'

    """
    [(name, step)] = upload_steps(documents)
    return documents[name], step


def first_job(document: Document) -> dict[str, object]:
    """Return a workflow's first job.

    Parameters
    ----------
    document : Document
        The parsed workflow.

    Returns
    -------
    dict of str to object
        The first job's mapping, in declaration order.

    Raises
    ------
    StopIteration
        If the workflow declares no job.

    Examples
    --------
    >>> first_job({"jobs": {"a": {"runs-on": "x"}, "b": {}}})
    {'runs-on': 'x'}

    """
    jobs = typ.cast("dict[str, dict[str, object]]", document["jobs"])
    return next(iter(jobs.values()))


def job_steps(document: Document) -> list[Step]:
    """Return a workflow's first job's steps.

    Parameters
    ----------
    document : Document
        The parsed workflow.

    Returns
    -------
    list of Step
        The first job's steps, the live list a test may extend.

    Raises
    ------
    StopIteration
        If the workflow declares no job.
    KeyError
        If the first job declares no steps.

    Examples
    --------
    >>> job_steps({"jobs": {"a": {"steps": [{"run": "true"}]}}})
    [{'run': 'true'}]

    """
    return typ.cast("list[Step]", first_job(document)["steps"])


def coverage_step(document: Document) -> Step:
    """Return a workflow's generate-coverage step.

    Parameters
    ----------
    document : Document
        The parsed workflow.

    Returns
    -------
    Step
        The first job's first step calling the shared coverage action.

    Raises
    ------
    StopIteration
        If the first job has no coverage step.

    Examples
    --------
    >>> coverage_step({"jobs": {"a": {"steps": [{"uses": "x/generate-coverage@v1"}]}}})
    {'uses': 'x/generate-coverage@v1'}

    """
    return next(
        step
        for step in job_steps(document)
        if "generate-coverage" in str(step.get("uses"))
    )


def lane_jobs(documents: Documents) -> dict[str, object]:
    """Return the pull-request lane's jobs, to add a calling job.

    Parameters
    ----------
    documents : Documents
        The workflows, which must include the lane.

    Returns
    -------
    dict of str to object
        The lane's live jobs mapping, which a test may extend.

    Raises
    ------
    KeyError
        If the lane's workflow is missing.

    Examples
    --------
    >>> "build-test" in lane_jobs(fresh_documents())
    True

    """
    return typ.cast("dict[str, object]", documents[LANE]["jobs"])
