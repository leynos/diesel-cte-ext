"""Read a parsed workflow as text, for the CV-005 marker rules.

The marker rules ask whether any key or value a workflow can reach names the
CodeScene host, uploader, client or token. These helpers flatten a parsed
document into its scalars and normalize each one, so that a marker cannot hide
behind case, whitespace or a key position.
"""

from __future__ import annotations

import re
import typing as typ

if typ.TYPE_CHECKING:
    import collections.abc as cabc


def scalars(value: object) -> cabc.Iterator[str]:
    """Yield every key and value in a parsed document as text.

    Keys are yielded as well as values: an `env` key or a `workflow_call`
    secret declaration names the token with no value that refers to it.

    Parameters
    ----------
    value : object
        A parsed document or any part of one.

    Yields
    ------
    str
        Each key and each non-null leaf value, as text.

    Examples
    --------
    >>> list(scalars({"env": {"TOKEN": None}, "run": ["a", 1]}))
    ['env', 'TOKEN', 'run', 'a', '1']

    """
    match value:
        case dict():
            for key, child in value.items():
                yield str(key)
                yield from scalars(child)
        case list():
            for child in value:
                yield from scalars(child)
        case None:
            return
        case _:
            yield str(value)



def folded(text: str) -> str:
    """Return text case-folded with all whitespace removed.

    Parameters
    ----------
    text : str
        The text to normalize.

    Returns
    -------
    str
        The text, so that `toJSON( secrets )` and `API.CODESCENE.IO` match
        their plain spellings.

    Examples
    --------
    >>> folded("toJSON( Secrets )")
    'tojson(secrets)'

    """
    return re.sub(r"\s+", "", text).casefold()
