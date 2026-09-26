"""Hold every CodeScene contact off the pull-request lanes (CV-005).

CV-005 moves every CodeScene call off the pull-request lanes. A pull request
generates coverage for its own ratchet and nothing else. The reason is the call
rather than the artefact: the shared uploader pins the cs-coverage archive by
digest, but the client talks to CodeScene's API and refuses to run when that
answer changes shape, which has happened twice. Keeping the call on the trunk
keeps such a change off every pull request's critical path.

Each rule is a pure function from parsed workflow documents to a list of
violations.
"""

from __future__ import annotations

import typing as typ

from codescene_workflow_reader import (
    Document,
    WorkflowError,
    jobs,
    steps,
    triggers,
)
from codescene_workflow_text import folded, scalars

#: This repository, for refusing a qualified call to one of its own workflows.
REPOSITORY: typ.Final[str] = "leynos/diesel-cte-ext"
WORKFLOW_PREFIX: typ.Final[str] = ".github/workflows/"
#: Events that start a workflow for a pull request: its head, its queued
#: merge, a review of it, or a comment on it. The review and comment events and
#: `merge_group` run with the repository's secrets for a same-repository pull
#: request.
PULL_REQUEST_EVENTS: typ.Final[frozenset[str]] = frozenset({
    "issue_comment",
    "merge_group",
    "pull_request",
    "pull_request_review",
    "pull_request_review_comment",
    "pull_request_target",
})

#: What no pull-request-reachable scalar may contain, case-folded and with
#: whitespace removed. The token's name catches an `env` key, a reference in a
#: script, an input or an env value under any key, a named `secrets:`
#: forwarding and a `workflow_call` secret declaration alike; indexed,
#: filtered and serialized `secrets` reach it without spelling the name.
PULL_REQUEST_FORBIDDEN: typ.Final[tuple[tuple[str, str], ...]] = (
    ("codescene.io", "names the CodeScene host"),
    ("upload-codescene-coverage", "calls the CodeScene uploader"),
    ("cs-coverage", "names the cs-coverage client"),
    ("cs_access_token", "puts CS_ACCESS_TOKEN in reach"),
    ("secrets[", "indexes the secrets context"),
    ("secrets.*", "filters the secrets context"),
    ("tojson(secrets", "serializes the secrets context"),
)


def _refuse_a_call_at_a_ref(reference: str) -> None:
    """Refuse a call to this repository's own workflow at some ref.

    The closure can read only the checked-out file, so a call that runs the
    file as it stands at another ref cannot be judged.
    """
    if reference.casefold().startswith(f"{REPOSITORY}/".casefold()):
        message = f"{reference} runs this repository's workflow at a ref"
        raise WorkflowError(message)
    if reference.startswith("$/") and "@" in reference:
        message = f"{reference}: a `$/` call cannot name a ref"
        raise WorkflowError(message)


def local_callee(reference: str, documents: dict[str, Document]) -> str | None:
    """Return the workflow file a job-level `uses:` names in this tree.

    Matched by shape: strip a leading `./` or `$/` and ask whether the rest is
    a file under the workflow directory. A `$/` call carries no ref, and a
    qualified call to this repository runs the file at that ref rather than
    the one checked out, so both are refused rather than followed.

    Parameters
    ----------
    reference : str
        A job's `uses:` value.
    documents : dict of str to Document
        Every workflow in the repository, keyed by file name.

    Returns
    -------
    str or None
        The called workflow's file name, or None for a call to another
        repository.

    Raises
    ------
    WorkflowError
        If the call is a qualified self-call, a `$/` call with a ref, or names
        a local workflow that does not exist.

    Examples
    --------
    >>> documents = {"lint.yml": {}}
    >>> local_callee("./.github/workflows/lint.yml", documents)
    'lint.yml'
    >>> local_callee("other/repo/.github/workflows/lint.yml@v1", documents) is None
    True

    """
    _refuse_a_call_at_a_ref(reference)
    path = reference.removeprefix("./").removeprefix("$/")
    if not path.startswith(WORKFLOW_PREFIX):
        return None
    callee = path.removeprefix(WORKFLOW_PREFIX)
    if callee not in documents:
        message = f"{reference} names no workflow in this repository"
        raise WorkflowError(message)
    return callee


def _callees(name: str, documents: dict[str, Document]) -> set[str]:
    """Return the local workflows one workflow's jobs call."""
    references = (job.get("uses") for job in jobs(name, documents[name]).values())
    return {
        callee
        for reference in references
        if isinstance(reference, str)
        and (callee := local_callee(reference, documents)) is not None
    }


def _chained(found: set[str], documents: dict[str, Document]) -> set[str]:
    """Return workflows a `workflow_run` trigger chains onto any found one."""
    # GitHub matches `workflows:` on a workflow's `name:`, or on its path from
    # the repository root when it declares none.
    watched_names = {
        str(documents[name].get("name", f"{WORKFLOW_PREFIX}{name}")) for name in found
    }
    return {
        name
        for name, document in documents.items()
        if watched_names & _watched_workflows(name, document)
    }


def _watched_workflows(name: str, document: Document) -> set[str]:
    """Return the workflows one workflow's `workflow_run` trigger watches.

    Only a list of names is read. A bare string would split into characters
    and match nothing, dropping the chained workflow from the closure, so
    any other shape is refused.
    """
    run = triggers(name, document).get("workflow_run")
    watched = run.get("workflows", []) if isinstance(run, dict) else []
    match watched:
        case list() if all(isinstance(entry, str) for entry in watched):
            return set(typ.cast("list[str]", watched))
        case _:
            message = f"{name}: cannot read `workflow_run.workflows` {watched!r}"
            raise WorkflowError(message)


def _reached_from(found: set[str], documents: dict[str, Document]) -> set[str]:
    """Return the found workflows and everything one call or chain away."""
    grown = found | _chained(found, documents)
    return grown | {callee for name in grown for callee in _callees(name, documents)}


def closure(seeds: set[str], documents: dict[str, Document]) -> dict[str, Document]:
    """Return the seeds and every workflow they start, transitively.

    A workflow declaring only `workflow_call` still runs when a seed's job
    calls it, and a `workflow_run` chained onto a seed runs after it, so both
    are followed until nothing new is reached.

    Parameters
    ----------
    seeds : set of str
        File names of the workflows to start from.
    documents : dict of str to Document
        Every workflow in the repository, keyed by file name.

    Returns
    -------
    dict of str to Document
        The seeds and everything they reach, keyed by file name.

    Raises
    ------
    WorkflowError
        If a local call cannot be followed; see `local_callee`.

    Examples
    --------
    >>> documents = {
    ...     "ci.yml": {
    ...         True: "pull_request",
    ...         "jobs": {"call": {"uses": "./.github/workflows/lint.yml"}},
    ...     },
    ...     "lint.yml": {True: {"workflow_call": None}, "jobs": {}},
    ... }
    >>> list(closure({"ci.yml"}, documents))
    ['ci.yml', 'lint.yml']

    """
    found = set(seeds)
    while (grown := _reached_from(found, documents)) != found:
        found = grown
    return {name: documents[name] for name in sorted(found)}


def _push_is_trunk_or_tags(push: object) -> bool:
    """Return whether a push filter admits only main, or only tags.

    Any other push runs for a same-repository pull request's head branch, with
    the repository's secrets, so it belongs to the pull-request surface.
    """
    if not isinstance(push, dict):
        return False
    if push == {"branches": ["main"]}:
        return True
    return "tags" in push and not {"branches", "branches-ignore"} & push.keys()


def serves_pull_requests(name: str, document: Document) -> bool:
    """Return whether a pull request can start a workflow by its own trigger.

    Parameters
    ----------
    name : str
        The workflow's file name, for messages.
    document : Document
        The parsed workflow.

    Returns
    -------
    bool
        True for a pull-request event, or a push not confined to main or tags.

    Examples
    --------
    >>> serves_pull_requests("ci.yml", {True: {"push": {"branches": ["main"]}}})
    False
    >>> serves_pull_requests("ci.yml", {True: "pull_request"})
    True

    """
    events = triggers(name, document)
    if PULL_REQUEST_EVENTS & events.keys():
        return True
    return "push" in events and not _push_is_trunk_or_tags(events["push"])


def pull_request_closure(documents: dict[str, Document]) -> dict[str, Document]:
    """Return every workflow a pull request can start, directly or not.

    A called workflow receives the token through `secrets: inherit`, so the
    rules below read the transitive closure, not a trigger list.

    Parameters
    ----------
    documents : dict of str to Document
        Every workflow in the repository, keyed by file name.

    Returns
    -------
    dict of str to Document
        Every workflow a pull-request event starts, and everything those reach.

    Raises
    ------
    WorkflowError
        If no workflow answers a pull-request event, which means the reader is
        broken rather than the repository compliant.

    Examples
    --------
    >>> documents = {
    ...     "ci.yml": {True: "pull_request", "jobs": {}},
    ...     "release.yml": {True: {"push": {"tags": ["v*"]}}, "jobs": {}},
    ... }
    >>> list(pull_request_closure(documents))
    ['ci.yml']

    """
    seeds = {
        name
        for name, document in documents.items()
        if serves_pull_requests(name, document)
    }
    if not seeds:
        message = "no workflow serves a pull request; the reader is broken"
        raise WorkflowError(message)
    return closure(seeds, documents)


def pull_request_contacts(documents: dict[str, Document]) -> list[str]:
    """Report every way a pull request could reach CodeScene or its token.

    Every scalar is read, keys included, at every scope, so a workflow-level
    `defaults.run.shell`, an env value under an unrelated key or a callee's
    secret declaration is seen as readily as a step's script. The parser
    discards comments, so prose explaining the policy is not a violation.

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
    >>> step = {"run": "curl https://api.codescene.io"}
    >>> document = {True: "pull_request", "jobs": {"t": {"steps": [step]}}}
    >>> pull_request_contacts({"ci.yml": document})
    ['ci.yml names the CodeScene host']

    """
    return [
        problem
        for name, document in pull_request_closure(documents).items()
        for problem in (
            *_forbidden_contacts(name, document),
            *_inherited_secrets(name, document),
            *_local_actions(name, document),
        )
    ]


def _forbidden_contacts(name: str, document: Document) -> list[str]:
    """Report each forbidden marker found in any scalar of one workflow."""
    texts = {folded(text) for text in scalars(document)}
    return [
        f"{name} {reason}"
        for marker, reason in PULL_REQUEST_FORBIDDEN
        if any(marker in text for text in texts)
    ]


def _inherited_secrets(name: str, document: Document) -> list[str]:
    """Report each job in one workflow that forwards every secret."""
    return [
        f"{name} job {job_name} forwards every secret with `secrets: inherit`"
        for job_name, job in jobs(name, document).items()
        if job.get("secrets") == "inherit"
    ]


def _local_actions(name: str, document: Document) -> list[str]:
    """Report each step in one workflow that runs this repository's action.

    A local action's `action.yml` is not read by these rules, so it could
    reach CodeScene unseen; it is refused rather than followed. An action
    named as this repository at a ref is refused alike, since it runs that
    file from whichever branch the ref names.
    """
    return [
        f"{name} runs the local action {step['uses']}, which these rules cannot read"
        for step in steps(name, document)
        if _is_own_action(str(step.get("uses", "")))
    ]


def _is_own_action(uses: str) -> bool:
    """Return whether a step's `uses:` runs an action from this repository."""
    folded_uses = uses.casefold()
    own = REPOSITORY.casefold()
    return uses.startswith(("./", "$/")) or folded_uses.startswith(
        (f"{own}/", f"{own}@")
    )
