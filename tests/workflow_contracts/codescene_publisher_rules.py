"""Hold the single push-to-main CodeScene publisher (CV-005).

One workflow, answering only a push to main (or a dispatch, which the upload's
ref guard confines to main), refreshes the ratchet baseline and uploads. Its
upload is bound, guarded and serialized here; the coverage lanes that ratchet
against its baseline are held in `codescene_coverage_rules`. The checksum
machinery the uploader has retired stays out of every workflow.
"""

from __future__ import annotations

import typing as typ

from codescene_token_rules import CHECK_ID, token_violations
from codescene_workflow_reader import (
    Document,
    Step,
    calls,
    continues_on_error,
    holding_job,
    jobs,
    steps,
    triggers,
)
from codescene_workflow_text import folded, scalars

UPLOAD_ACTION: typ.Final[str] = (
    "leynos/shared-actions/.github/actions/upload-codescene-coverage"
)
#: The upload step's whole condition, as a set of conjuncts. Exact rather than
#: a superset: an extra conjunct can only narrow the upload, and `&& false`
#: narrows it to never. Exactness also refuses every `||`, because an `||`
#: leaves some conjunct unequal to both required ones.
UPLOAD_GUARD: typ.Final[frozenset[str]] = frozenset({
    f"steps.{CHECK_ID}.outputs.available == 'true'",
    "github.ref == 'refs/heads/main'",
})

#: The only token scope the upload job needs.
READ_ONLY: typ.Final[dict[str, str]] = {"contents": "read"}
CHECKOUT_ACTION: typ.Final[str] = "actions/checkout"

#: The publisher answers these events and no others.
PUBLISHER_EVENTS: typ.Final[frozenset[str]] = frozenset({"push", "workflow_dispatch"})

#: Retired with CV-005 everywhere, not only on pull-request lanes: the
#: uploader rejects `installer-checksum` outright, and the variable and its
#: refresher workflow pinned an installer script the uploader no longer runs.
RETIRED: typ.Final[tuple[str, ...]] = (
    "installer-checksum",
    "codescene_cli_sha256",
    "get-codescene-sha",
)


def _conjuncts(condition: object) -> frozenset[str]:
    """Split a step condition on `&&`, normalizing whitespace."""
    text = str(condition).strip()
    if text.startswith("${{") and text.endswith("}}"):
        text = text[3:-2]
    return frozenset(" ".join(part.split()) for part in text.split("&&"))


def upload_steps(documents: dict[str, Document]) -> list[tuple[str, Step]]:
    """Return every step in any workflow that calls the CodeScene uploader.

    Parameters
    ----------
    documents : dict of str to Document
        Every workflow in the repository, keyed by file name.

    Returns
    -------
    list of tuple of (str, Step)
        Each uploading step with its workflow's file name.

    Examples
    --------
    >>> step = {"uses": f"{UPLOAD_ACTION}@v1"}
    >>> [(name, found is step) for name, found in upload_steps(
    ...     {"main.yml": {"jobs": {"u": {"steps": [step]}}}}
    ... )]
    [('main.yml', True)]

    """
    return [
        (name, step)
        for name, document in documents.items()
        for step in steps(name, document)
        if calls(step, UPLOAD_ACTION)
    ]


def _publisher_triggers(name: str, document: Document) -> list[str]:
    """Report a publisher answering anything but a push to main or a dispatch.

    The event set is pinned exactly: losing `workflow_dispatch` would leave
    automerged changes unmeasurable with nothing failing.
    """
    events = triggers(name, document)
    found = (
        [f"{name} must answer exactly {sorted(PUBLISHER_EVENTS)}, not {sorted(events)}"]
        if events.keys() != PUBLISHER_EVENTS
        else []
    )
    if events.get("push") != {"branches": ["main"]}:
        found.append(f"{name} must answer exactly `push: branches: [main]`")
    return found


def expected_concurrency(name: str) -> dict[str, object]:
    """Return the one concurrency block a publisher may declare.

    One group per ref, named after the workflow, never cancelling. Runs in one
    group never overlap, and a newer trigger replaces an older pending run
    rather than queueing behind it. GitHub does not promise to start runs in
    trigger order, so this does not guarantee commit order. Keying the group on
    the event as well would let a dispatch and a push to main run at once.

    Parameters
    ----------
    name : str
        The publisher's file name.

    Returns
    -------
    dict of str to object
        The workflow-level `concurrency` mapping required.

    Examples
    --------
    >>> expected_concurrency("coverage-main.yml")["group"]
    'coverage-main-${{ github.ref }}'

    """
    stem = name.rpartition(".")[0]
    return {"group": f"{stem}-${{{{ github.ref }}}}", "cancel-in-progress": False}


def _publisher_concurrency(name: str, document: Document) -> list[str]:
    """Report a publisher whose uploads could overlap, reorder or be cancelled.

    The workflow-level block is compared whole, so a group keyed on the event,
    a missing group and any `cancel-in-progress` other than false all fail. A
    job-level group would govern that job apart from the workflow's, so none
    is allowed.
    """
    expected = expected_concurrency(name)
    found = (
        []
        if document.get("concurrency") == expected
        else [f"{name} concurrency must be exactly {expected}"]
    )
    return found + [
        f"{name} job {job_name} must not declare its own concurrency"
        for job_name, job in jobs(name, document).items()
        if "concurrency" in job
    ]


def _upload_step(name: str, step: Step) -> list[str]:
    """Report an upload step not guarded and moded as required."""
    inputs = step.get("with")
    inputs = inputs if isinstance(inputs, dict) else {}
    found: list[str] = []
    if _conjuncts(step.get("if", "")) != UPLOAD_GUARD:
        found.append(f"{name} upload must be guarded on exactly {sorted(UPLOAD_GUARD)}")
    if inputs.get("mode") != "upload":
        found.append(f"{name} upload must name `mode: upload`")
    if continues_on_error(step):
        found.append(f"{name} upload must not continue on error")
    return found


def _publisher_jobs(name: str, document: Document) -> list[str]:
    """Report a publisher job that could be skipped or could fail green."""
    return [
        f"{name} job {job_name} {problem}"
        for job_name, job in jobs(name, document).items()
        for problem, failed in (
            ("must run unconditionally", "if" in job),
            ("must not continue on error", continues_on_error(job)),
        )
        if failed
    ]


def _upload_job(name: str, document: Document, upload: Step) -> list[str]:
    """Report an upload job with a wider token or persisted Git credentials.

    Nothing in the job writes to the repository, and the coverage run executes
    repository and dependency code after checkout, so the token is read-only
    and the checkout keeps no credentials.
    """
    job = holding_job(name, document, upload)
    found = (
        []
        if job.get("permissions") == READ_ONLY
        else [f"{name} upload job permissions must be exactly {READ_ONLY}"]
    )
    return found + [
        f"{name} checkout must set persist-credentials: false"
        for step in typ.cast("list[Step]", job.get("steps", []))
        if calls(step, CHECKOUT_ACTION) and not _keeps_no_credentials(step)
    ]


def _keeps_no_credentials(checkout: Step) -> bool:
    """Return whether a checkout step sets `persist-credentials: false`."""
    inputs = checkout.get("with")
    return isinstance(inputs, dict) and inputs.get("persist-credentials") is False


def publisher_violations(documents: dict[str, Document]) -> list[str]:
    """Report anything but one guarded push-to-main publisher.

    Where the token is bound is held by `codescene_token_rules`.

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
    >>> publisher_violations(fresh_documents())
    []

    """
    uploads = upload_steps(documents)
    if len(uploads) != 1:
        return [f"expected one CodeScene upload step, found {len(uploads)}"]
    name, upload = uploads[0]
    document = documents[name]
    return [
        *_publisher_triggers(name, document),
        *_publisher_concurrency(name, document),
        *_publisher_jobs(name, document),
        *_upload_step(name, upload),
        *_upload_job(name, document, upload),
        *token_violations(name, document, upload),
    ]


def retired_names(documents: dict[str, Document]) -> list[str]:
    """Report any retired checksum input, variable or refresher workflow.

    Parameters
    ----------
    documents : dict of str to Document
        Every workflow in the repository, keyed by file name.

    Returns
    -------
    list of str
        One message per workflow and retired name; empty when none remains.

    Examples
    --------
    >>> retired_names({"get-codescene-sha.yml": {"jobs": {}}})
    ['get-codescene-sha.yml still names get-codescene-sha']

    """
    found = [
        f"{name} still names {retired}"
        for name, document in documents.items()
        for retired in RETIRED
        if retired in name.casefold()
        or any(retired in folded(text) for text in scalars(document))
    ]
    return sorted(set(found))
