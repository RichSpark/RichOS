#!/usr/bin/env python3
"""host_child.py: A WATCHER THE APP HOST RUNS AS ITS OWN CHILD (--host-json).

Claude Code starts plugin monitors only in interactive sessions, and his team's
lead in the RichOS app runs with --print, so none of monitors/monitors.json
starts in it. The app host therefore runs quota-watch, stall-watch and
codex-watch itself, from launch to quit (richos-core team_watch.rs), the way it
runs review-watch (review_watch.py run_host_loop), and sends each notice to his
team's lead as a message of its own: a monitor inside a print-mode lead would
start a turn the host never asked for and cannot attribute.

What the three share, and nothing else:

  start()    the host's own pid comes in HOST_ENV at spawn; the watcher runs
             while its parent is still that host, so a crashed or killed app
             never leaves it running (review_watch.py run_host_loop, the real
             second review of 16c154f5a, finding 2)
  session()  the session of his team's lead, read at every look from the file
             the host names in LEAD_ENV ({"session": "...", "started": <epoch s>},
             "" while no lead runs). It is also put in RICHOS_SESSION_ID, so the
             watcher's own session logic (workspaces.current_session) sees that
             lead, as it sees the terminal lead it runs inside
  started()  when the host started that lead, from the same file. A terminal
             monitor finds its lead's start through its own ancestry; here the
             lead is the watcher's sibling, so that finds nothing, and an
             escalation raised before the watcher's first look would count as
             backlog and never be told (review rv-20261020T132202Z-c5ab9358-2de8)
  line()     one notice as one JSON line, {"text": ...}: a notice has many
             lines, and the host sends each whole
"""

import json
import os

HOST_ENV = "RICHOS_WATCH_HOST"
LEAD_ENV = "RICHOS_WATCH_LEAD"
SESSION_ENV = "RICHOS_SESSION_ID"


def start():
    """A callable: is this process's parent still the host that started it? The host's pid is
    taken out of the environment here, so nothing this watcher starts inherits it."""
    host = os.environ.pop(HOST_ENV, "").strip()
    parent = int(host) if host.isdigit() else os.getppid()

    def alive():
        return os.getppid() == parent
    return alive


def _lead():
    """The host's lead file as a dict; {} when none is named or it cannot be read."""
    path = (os.environ.get(LEAD_ENV) or "").strip()
    if not path:
        return {}
    try:
        with open(path, "r", encoding="utf-8") as fh:
            data = json.load(fh)
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


def session():
    """The session of his team's lead, or "" while none runs (or the file cannot be read)."""
    sid = str(_lead().get("session") or "").strip()
    if sid and os.sep not in sid:
        os.environ[SESSION_ENV] = sid
        return sid
    os.environ.pop(SESSION_ENV, None)
    return ""


def started(sid):
    """Epoch seconds the host started the lead of `sid`, or None: no host file, another lead
    named in it now, or no start written."""
    data = _lead()
    if not sid or str(data.get("session") or "").strip() != sid:
        return None
    at = data.get("started")
    if isinstance(at, bool) or not isinstance(at, (int, float)) or at <= 0:
        return None
    return float(at)


def line(text):
    return json.dumps({"text": text.rstrip("\n")})
