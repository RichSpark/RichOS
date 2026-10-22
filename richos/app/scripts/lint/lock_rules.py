"""Non-blocking lock attempts must say what their caller does when the lock is busy.

Failure class: docs/development/failure-classes/busy-shared-lock-reported-as-a-failure.md.
Incident: on 2026-10-21 a new user on 1.2.1 saw "RichOS could not prepare this update." over
"Resource temporarily unavailable (os error 35)" on a plain update check. The update record's
readers took its lock with ONE `flock(LOCK_EX | LOCK_NB)` and no wait, so the app's own two
threads asking at the same moment made one of them fail, and the failure reached the screen as
the operating system's words.

A non-blocking attempt is right in exactly two shapes: it is one poll inside a bounded wait, or
"busy" is itself the answer the caller wants (one owner of a socket, a liveness probe). Both are
legitimate and a scanner cannot tell either from the defect, so the rule asks the author to say
which, where a reviewer sees it:

    // lock-busy: <what the caller does when the lock is busy>

on the line with `LOCK_NB` or on one of the three lines above it. A bare marker, or a reason
under ten characters, declares nothing. Comment lines are not sites, so prose that names
`LOCK_NB` is never counted. Rust only: every file lock in the app is taken from Rust.
"""
import re

RULES = {"lock-busy-undeclared": "blocking"}
SITE = re.compile(r"\bLOCK_NB\b")
DECLARED = re.compile(r"lock-busy:\s*(\S.{9,})")
LOOKBACK = 3


def scan(text):
    lines = text.splitlines()
    findings = []
    for number, line in enumerate(lines, 1):
        code = line.split("//", 1)[0]
        if not SITE.search(code):
            continue
        window = lines[max(0, number - 1 - LOOKBACK):number]
        if not any(DECLARED.search(candidate) for candidate in window):
            findings.append(("lock-busy-undeclared", number))
    return findings
