"""Reads from GitHub in the nightly's build and publish path, tried again over about a minute.

Run 20261014T135757Z-c7955224 passed every gate on 4f80e5322 and then refused at
`plan-recheck`, because `git ls-remote --tags origin` got `git@github.com: Permission
denied (publickey).` once; the same command worked from a shell a minute later. A one-off
refusal from GitHub never ends a build: a read that could not reach GitHub, or got no
answer in time, is tried again, and only a refusal that lasts is reported, as GitHub
access, with ssh's own words.

Only connection failures are tried again. GitHub ANSWERING (no such ref, an object it
will not serve) is an answer, and the caller gets it at once, exactly as before. Only
reads come here (`ls-remote`, `fetch`); pushes are not retried.
"""
import re
from time import monotonic, sleep

READS = ("ls-remote", "fetch")
# Seconds before attempts 2, 3 and 4: about a minute of waiting in all.
WAITS = (5, 15, 40)
# One attempt's limit. The same budget nightly-local.py has always given `git fetch`.
TIMEOUT = 120
# What ssh and git print when the connection itself failed, rather than GitHub answering.
CONNECTION_FAILURE = re.compile(
    r"Permission denied \(publickey|Could not read from remote repository|ssh: connect to host"
    r"|Connection (refused|timed out|reset|closed)|Operation timed out|kex_exchange_identification"
    r"|Could not resolve host|remote end hung up unexpectedly|early EOF|Failed to connect to",
    re.IGNORECASE)


class Unreachable(Exception):
    """One attempt did not reach GitHub; `str()` is ssh's or git's own error."""


def connection_failed(stderr):
    return bool(CONNECTION_FAILURE.search(stderr or ""))


def words(error):
    """ssh's and git's own error on one line."""
    return "; ".join(line.strip() for line in str(error).splitlines() if line.strip()) or "no error text"


def read(once, argv, refuse, notice):
    """`once()`'s value, tried again while it raises Unreachable.

    `refuse(message)` builds what is raised when every attempt failed; `notice(text)` says
    each retry where the run's log shows it.
    """
    started, attempts = monotonic(), len(WAITS) + 1
    command = " ".join(str(a) for a in argv)
    for attempt, wait in enumerate((0, *WAITS), 1):
        if wait:
            notice(f"GitHub access failed for `{command}` ({words(error)}); "
                   f"attempt {attempt} of {attempts} in {wait}s")
            sleep(wait)
        try:
            return once()
        except Unreachable as failure:
            error = failure
    raise refuse(f"GitHub access refused: `{command}` could not reach GitHub in {attempts} attempts "
                 f"over {monotonic() - started:.0f}s; ssh said: {words(error)}")
