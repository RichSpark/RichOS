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


# The go that ends such a hold: an imperative that OPENS a sentence ("Go: run the walk
# now.", "Please continue.", "You can go ahead."). A word inside a question or a negation
# is not one: "What do you need before you can proceed?", "Do not resume yet" (second
# reviews rv-20261022T002958Z-2ac46e09-fa4e #1 and rv-20261022T023129Z-cefd17cf-7c77).
GO_WORDS_RE = re.compile(
    r"(?:^|(?<=[.!;:\n])\s*)(?:ok(?:ay)?[,:]?\s+|please\s+)?(?:go(?:\s+ahead)?|proceed|carry\s+on|continue|"
    r"resume|start(?:\s+(?:it|now|the\b))?|you\s+can\s+(?:go|start|proceed|continue|resume))\b", re.I)


def _sentences(text):
    return [s for s in re.split(r"(?<=[.!?;\n])", text or "") if s.strip()]


def find(text):
    """The first hold-word match in text, or None."""
    return HOLD_WORDS_RE.search(text or "")


def is_go(text):
    """True when a sentence of the message opens with the go and is not a question."""
    return any(GO_WORDS_RE.match(s.strip()) and not s.strip().endswith("?") for s in _sentences(text))
