"""Hold where the publisher puts CS_ACCESS_TOKEN (CV-005).

The uploader is a composite action. A token bound in its step's `env` would
reach its nested upload-artifact and cache steps too, and the action binds the
token itself from `inputs.access-token`. So no `env` in the publisher binds the
token at all. A check step with no `env` and no `if:` runs one exact command,
whose expression Actions evaluates to `true` or `false` before the shell runs,
and the upload reads that output; the upload receives the token only as its
`access-token` input.

A guard on `env.CS_ACCESS_TOKEN != ''` is simply false when the binding is
deleted or moved, so the upload would skip forever with nothing failing. The
check step and its output are therefore asserted positively, not inferred from
the guard.
"""

from __future__ import annotations

import copy
import typing as typ

from codescene_workflow_reader import (
    Document,
    Step,
    continues_on_error,
    holding_job,
    jobs,
)
from codescene_workflow_text import folded, scalars

#: The check step's id, which the upload guard reads.
CHECK_ID: typ.Final[str] = "codescene-token"

#: The check step's whole command. The expression is evaluated before the shell
#: runs, so the step binds nothing and holds no shell conditional; a fork
#: without the secret writes `available=false` and skips the upload.
CHECK_COMMAND: typ.Final[str] = (
    "echo \"available=${{ secrets.CS_ACCESS_TOKEN != '' }}\" >> \"$GITHUB_OUTPUT\""
)

#: What the upload passes as `access-token`.
CREDENTIAL_INPUT: typ.Final[str] = "${{ secrets.CS_ACCESS_TOKEN }}"

#: Keys a check step may carry. Anything else, such as `if`, `env`, `uses`,
#: `shell` or `continue-on-error`, could skip it, bind the token, run other code
#: or turn its failure green.
CHECK_KEYS: typ.Final[frozenset[str]] = frozenset({"name", "id", "run"})

TOKEN: typ.Final[str] = "cs_access_token"


def expression(value: object) -> str:
    """Return an expression with its inner whitespace normalized.

    Parameters
    ----------
    value : object
        A scalar from a parsed workflow.

    Returns
    -------
    str
        The text with `${{` and `}}` spaced and runs of whitespace collapsed.

    Examples
    --------
    >>> expression("${{secrets.X}}")
    '${{ secrets.X }}'

    """
    return " ".join(str(value).replace("${{", "${{ ").replace("}}", " }}").split())


def _check_step(name: str, check: Step, upload_index: int, index: int) -> list[str]:
    """Report a check step that could skip, fail green or run other code."""
    return [
        f"{name} `{CHECK_ID}` step {problem}"
        for problem, failed in (
            ("must run before the upload", index > upload_index),
            (f"may carry only {sorted(CHECK_KEYS)}", not check.keys() <= CHECK_KEYS),
            ("must not continue on error", continues_on_error(check)),
            (
                f"must run exactly `{CHECK_COMMAND}`",
                expression(check.get("run", "")) != expression(CHECK_COMMAND),
            ),
        )
        if failed
    ]


def _env_bindings(name: str, document: Document) -> list[str]:
    """Report any `env` in the publisher that names the token.

    The workflow, every job and every step are read, since each scope's `env`
    reaches the steps beneath it, and the uploader passes its own step's `env`
    to its nested steps.
    """
    scopes: list[object] = [document.get("env")]
    for job in jobs(name, document).values():
        scopes.append(job.get("env"))
        scopes += [
            step.get("env")
            for step in typ.cast("list[Step]", job.get("steps", []))
            if isinstance(step, dict)
        ]
    if any(TOKEN in folded(text) for scope in scopes for text in scalars(scope)):
        return [f"{name} binds CS_ACCESS_TOKEN in an env"]
    return []


def _outside_steps(name: str, document: Document, check: Step, upload: Step) -> list[str]:
    """Report the token anywhere but the check command and the upload input."""
    # Removed by identity, not equality: a copy of the check step elsewhere
    # compares equal to it and must stay in the sweep.
    rest = dict(document)
    rest["jobs"] = {
        job_name: {
            **job,
            "steps": [
                step
                for step in typ.cast("list[Step]", job.get("steps", []))
                if step is not check and step is not upload
            ],
        }
        for job_name, job in jobs(name, document).items()
    }
    remaining = copy.deepcopy(upload)
    inputs = remaining.get("with")
    if isinstance(inputs, dict):
        inputs.pop("access-token", None)
    if any(TOKEN in folded(text) for text in [*scalars(rest), *scalars(remaining)]):
        return [f"{name} puts CS_ACCESS_TOKEN in reach outside its two uses"]
    return []


def token_violations(name: str, document: Document, upload: Step) -> list[str]:
    """Report a token bound in any env, or passed any other way.

    Parameters
    ----------
    name : str
        The publisher's file name, for messages.
    document : Document
        The parsed publisher.
    upload : Step
        The publisher's upload step.

    Returns
    -------
    list of str
        One message per violation; empty when the token is used as required.

    Examples
    --------
    >>> from codescene_contract_support import fresh_documents, find_publisher
    >>> publisher, upload = find_publisher(fresh_documents())
    >>> token_violations("coverage-main.yml", publisher, upload)
    []

    """
    held = typ.cast("list[Step]", holding_job(name, document, upload).get("steps", []))
    checks = [index for index, step in enumerate(held) if step.get("id") == CHECK_ID]
    if len(checks) != 1:
        return [f"{name} needs one `{CHECK_ID}` step in the upload job"]
    check = held[checks[0]]
    found = _check_step(name, check, held.index(upload), checks[0])
    inputs = upload.get("with")
    inputs = inputs if isinstance(inputs, dict) else {}
    if expression(inputs.get("access-token")) != CREDENTIAL_INPUT:
        found.append(f"{name} upload must pass access-token {CREDENTIAL_INPUT}")
    found += _env_bindings(name, document)
    return found + _outside_steps(name, document, check, upload)
