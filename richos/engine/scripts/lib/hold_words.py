"""hold_words.py: the words that hold work, in one place.

Read by two places that must agree (richos-hq docs/plans/2026-10-22-no-silent-
holds.md): the deferral analyzer (scripts/hooks/guard-unasked-deferral.py),
where they open a hold from a reply such as "Stream B is on hold, deliberately."
(2026-09-27), and the SendMessage parser (mega-lander/workspaces.py lifecycle),
where a message telling a teammate to "wait for my go" (2026-10-21) records a
hold on it. Neither blocks anything: each records a hold the engine keeps in
sight until the work runs, lands or the CEO decides.
"""
import re

HOLD_WORDS_RE = re.compile(
    r"\b(?:on\s+hold|hold(?:ing)?\s+(?:it\s+|this\s+|that\s+|them\s+)?off|parked|"
    r"until\s+I\s+say(?:\s+so)?|"
    r"wait\s+for\s+my\s+(?:go|word|signal|ok|okay|green\s+light))\b", re.I)


# The words that end such a hold: the go itself. A later message without them (a status
# question, a note) leaves the teammate held, and the hold reported (second review
# rv-20261022T002958Z-2ac46e09-fa4e, finding 1).
GO_WORDS_RE = re.compile(
    r"\b(?:go(?:\s+ahead)?|proceed|carry\s+on|continue|resume|start\s+(?:it|now|the)|you\s+can\s+start|"
    r"unblocked|green\s+light)\b", re.I)


def find(text):
    """The first hold-word match in text, or None."""
    return HOLD_WORDS_RE.search(text or "")


def is_go(text):
    """True when a message gives the go that ends a teammate's hold."""
    return bool(GO_WORDS_RE.search(text or ""))
