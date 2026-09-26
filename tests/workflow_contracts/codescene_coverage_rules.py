"""Hold the coverage lanes to main's ratchet baseline (CV-005).

The publisher's generator writes the ratchet baseline on a push to main, and
every pull-request generator compares against it. So each pull-request lane
must ratchet, publish no artefact and select exactly what the publisher
selects, at the same pin; and nothing else may write a baseline.
"""

from __future__ import annotations

import posixpath
import re
import typing as typ
from pathlib import PurePosixPath

from codescene_publisher_rules import READ_ONLY, upload_steps
from codescene_pull_request_rules import closure, pull_request_closure
from codescene_workflow_reader import (
    Document,
    Step,
    calls,
    continues_on_error,
    holding_job,
    steps,
    triggers,
)

COVERAGE_ACTION: typ.Final[str] = (
    "leynos/shared-actions/.github/actions/generate-coverage"
)
PULL_REQUEST_GUARD: typ.Final[str] = "github.event_name == 'pull_request'"
ARTEFACT_ACTION: typ.Final[str] = "actions/upload-artifact"
PINNED: typ.Final[re.Pattern[str]] = re.compile(r"@[0-9a-f]{40}")


def coverage_steps(name: str, document: Document) -> list[Step]:
    """Return one workflow's generate-coverage steps.

    Parameters
    ----------
    name : str
        The workflow's file name, for messages.
    document : Document
        The parsed workflow.

    Returns
    -------
    list of Step
        Every step calling the shared coverage action, in order.

    Examples
    --------
    >>> step = {"uses": f"{COVERAGE_ACTION}@v1"}
    >>> coverage_steps("ci.yml", {"jobs": {"t": {"steps": [{"run": "true"}, step]}}})
    [{'uses': 'leynos/shared-actions/.github/actions/generate-coverage@v1'}]

    """
    return [step for step in steps(name, document) if calls(step, COVERAGE_ACTION)]


def _selection(step: Step) -> dict[str, object]:
    """Return a coverage step's inputs, less the artefact switch."""
    inputs = step.get("with")
    inputs = dict(inputs) if isinstance(inputs, dict) else {}
    inputs.pop("publish-artefact", None)
    return inputs


def _pull_request_lane(
    name: str, document: Document, step: Step, trunk: Step
) -> list[str]:
    """Report a pull-request coverage step that cannot ratchet like main.

    The step's own guard is required, not merely permitted: the lane's
    workflow also answers a push to main, and an unguarded step would then
    write a second baseline there, outside the publisher's concurrency group.
    A guard on the holding job could only narrow it, down to `false`, while
    the step still read as guarded.
    """
    job = holding_job(name, document, step)
    return [
        f"{name} coverage {problem}"
        for problem, failed in (
            ("must not continue on error", any(map(continues_on_error, (step, job)))),
            ("job must run unconditionally", "if" in job),
            (
                f"may run only as `{PULL_REQUEST_GUARD}`",
                step.get("if") != PULL_REQUEST_GUARD,
            ),
            ("must set with-ratchet 'true'", _with(step, "with-ratchet") != "true"),
            (
                "must set publish-artefact 'false'",
                _with(step, "publish-artefact") != "false",
            ),
            (
                "selection differs from the publisher's",
                _selection(step) != _selection(trunk),
            ),
            ("pin differs from the publisher's", step.get("uses") != trunk.get("uses")),
            (
                f"job permissions must be exactly {READ_ONLY}",
                job.get("permissions") != READ_ONLY,
            ),
        )
        if failed
    ]


def _trunk_violations(publisher: str, trunk: Step, upload: Step) -> list[str]:
    """Report a baseline writer that could skip, fail green or drift its pin."""
    return [
        f"{publisher} {problem}"
        for problem, failed in (
            ("coverage must run unconditionally", "if" in trunk),
            ("coverage must not continue on error", continues_on_error(trunk)),
            (
                "coverage must set with-ratchet 'true'",
                _with(trunk, "with-ratchet") != "true",
            ),
            ("must pin shared actions by full SHA", not _pinned(trunk, upload)),
            ("upload pin differs from its coverage pin", _ref(trunk) != _ref(upload)),
            (
                "upload must read the coverage step's output-path",
                _with(upload, "path") != _with(trunk, "output-path"),
            ),
            (
                "upload must name the coverage step's format",
                _with(upload, "format") != _with(trunk, "format"),
            ),
        )
        if failed
    ]


def coverage_violations(documents: dict[str, Document]) -> list[str]:
    """Report coverage lanes that no longer ratchet against main's baseline.

    The publisher's generator writes the baseline and runs unconditionally;
    every pull-request generator reads it, so each must ratchet, publish no
    artefact and select exactly what the publisher selects, at the same pin.
    Nothing else a push starts may generate coverage, and no step may widen
    when the baseline is written.

    Parameters
    ----------
    documents : dict of str to Document
        Every workflow in the repository, keyed by file name.

    Returns
    -------
    list of str
        One message per violation; empty when the repository complies.

    Examples
    --------
    >>> from codescene_contract_support import fresh_documents
    >>> coverage_violations(fresh_documents())
    []

    """
    uploads = upload_steps(documents)
    if len(uploads) != 1:
        return ["coverage lanes need exactly one publisher to compare against"]
    publisher = uploads[0][0]
    trunk_steps = coverage_steps(publisher, documents[publisher])
    if len(trunk_steps) != 1:
        return [f"{publisher} must generate coverage exactly once"]
    trunk = trunk_steps[0]
    found = _trunk_violations(publisher, trunk, uploads[0][1])
    lanes = [
        (name, document, step)
        for name, document in pull_request_closure(documents).items()
        for step in coverage_steps(name, document)
    ]
    if not lanes:
        found.append("no pull-request lane generates coverage for the ratchet")
    for name, document, step in lanes:
        found += _pull_request_lane(name, document, step, trunk)
    found += _artefact_uploads(documents, _with(trunk, "output-path"))
    return found + _baseline_writers(documents) + _push_writers(documents, publisher)


def _artefact_uploads(documents: dict[str, Document], report: object) -> list[str]:
    """Report a pull-request step that uploads the coverage report as an artefact.

    `publish-artefact: 'false'` keeps the shared action from uploading it; a
    separate upload-artifact step would publish it anyway.
    """
    return [
        f"{name} must not upload the coverage report as an artefact"
        for name, document in pull_request_closure(documents).items()
        for step in steps(name, document)
        if calls(step, ARTEFACT_ACTION)
        and any(_may_hold(line, str(report)) for line in _upload_paths(step))
    ]


def _upload_paths(step: Step) -> list[str]:
    """Return the included paths of an upload-artifact step, one per line."""
    return [
        line.strip()
        for line in str(_with(step, "path") or "").splitlines()
        if line.strip() and not line.strip().startswith("!")
    ]


def _may_hold(path: str, report: str) -> bool:
    """Return whether an upload path could include the coverage report.

    Read as a matcher, not a substring: a glob, an expression, the workspace
    root or anything above it could hold the report under any name, and a
    directory holds whatever lies beneath it. Only a literal path that is
    neither the report nor one of its parent directories is cleared.

    Examples
    --------
    >>> [_may_hold(p, "lcov.info") for p in (".", "**/*", "./lcov.info", "dist")]
    [True, True, True, False]

    """
    if any(marker in path for marker in ("*", "?", "[", "$", "~")):
        return True
    literal = PurePosixPath(posixpath.normpath(path.replace("\\", "/")))
    if literal.is_absolute() or literal.parts[:1] in {(), ("..",)}:
        return True
    target = PurePosixPath(posixpath.normpath(report))
    return literal == target or literal in target.parents


def _push_writers(documents: dict[str, Document], publisher: str) -> list[str]:
    """Report coverage a push can run anywhere but the publisher.

    Such a step writes a second baseline on every push to main, outside the
    publisher's concurrency group. The push side is followed through local
    calls as the pull-request side is, since a called workflow runs on its
    caller's push; the publisher is a seed too, so its own callees are judged,
    and only its own document is exempt.
    """
    seeds = {
        name
        for name, document in documents.items()
        if "push" in triggers(name, document)
    }
    reached = closure(seeds, documents)
    return [
        f"{name} coverage can run on a push; guard it to pull requests"
        for name, document in reached.items()
        if name != publisher
        for step in coverage_steps(name, document)
        if step.get("if") != PULL_REQUEST_GUARD
    ] + _push_local_actions(reached)


def _push_local_actions(reached: dict[str, Document]) -> list[str]:
    """Report a local action a push runs, publisher included.

    Its `action.yml` is not read by these rules, so it could generate coverage
    and write a second baseline unseen; it is refused rather than followed.
    """
    return [
        f"{name} runs the local action {step['uses']} on a push, "
        "which these rules cannot read"
        for name, document in reached.items()
        for step in steps(name, document)
        if str(step.get("uses", "")).startswith(("./", "$/"))
    ]


def _baseline_writers(documents: dict[str, Document]) -> list[str]:
    """Report a coverage step that may write the baseline off main's push.

    The default, `auto`, saves the baseline only on a push to
    `refs/heads/main`. `always` hands that restriction to the calling
    workflow, so on a pull-request lane each push could lower the baseline its
    next push ratchets against, and on the publisher a dispatch from a branch
    would write one.
    """
    return [
        f"{name} coverage must leave publish-baseline at `auto`"
        for name, document in documents.items()
        for step in coverage_steps(name, document)
        if _with(step, "publish-baseline") not in {None, "auto"}
    ]


def _with(step: Step, key: str) -> object:
    """Return one input of a step, or None."""
    inputs = step.get("with")
    return inputs.get(key) if isinstance(inputs, dict) else None


def _ref(step: Step) -> str:
    """Return the ref a step's `uses:` names."""
    return str(step.get("uses", "")).partition("@")[2]


def _pinned(*called: Step) -> bool:
    """Return whether every step pins its action by a full commit SHA."""
    return all(PINNED.fullmatch(f"@{_ref(step)}") for step in called)
