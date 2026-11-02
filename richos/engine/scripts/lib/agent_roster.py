"""agent_roster.py — the folder a process reads teammate definitions from.

An app lease carries its own roster in RICHOS_AGENT_ROSTER (the agents folder of its plugin,
the bytes its session boots teammates from). The coordination folder's `.claude/agents` is shared
by every lease of the app, so a lease started for another company may have rendered its own
definitions there; a reader that takes the shared folder reads the wrong company's teammate.
Where the variable is unset (the terminal) the reader's own folder stands, as it always did.
The shell twin is `agent_roster_dir` in resolve-roots.sh.
"""
import os
from pathlib import Path

ENV = "RICHOS_AGENT_ROSTER"


def roster_dir(root):
    """The lease's roster when it has one, else `<root>/.claude/agents`."""
    return Path(os.environ.get(ENV) or Path(root) / ".claude/agents")
