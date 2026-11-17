#!/usr/bin/env python3
"""teammate-identity.py — WHAT A TEAMMATE IS CALLED. One answer, both halves.

===========================================================================
THE DEFECT THIS FILE EXISTS TO CLOSE — measured 2026-08-31
===========================================================================
Two in-flight notices were sent, witnessed, and written to
inflight-notices.jsonl at 22:34:43Z and 22:34:49Z with full 40-character
sha_tokens. guard-inflight-notify.sh still reported OWED-NO-NOTICE with
`notified-but-unacked: 0`, and the land was pushed through on two recorded
waivers — which is precisely how a blocking defense decays into a formality.

The cause was visible in the guard's own output. The DEBT side resolved a
teammate from `worker-events.jsonl`, whose every row carries `agent_type` —
the ROLE, `zach`. The WITNESS side recorded `SendMessage`'s `to`, which is
the teammate's MANDATORY UNIQUE NAME, `zach-opus-s1`, because that is the
only thing SendMessage can be addressed with and the very shape
guard-worktree-isolation.sh enforces at spawn time. One half of the engine
wrote `zach`; the other wrote `zach-opus-s1`; nothing could ever join them.

So identity is resolved HERE, once, and both halves import it. A predicate
kept in two copies is the defect class this engine keeps finding in itself,
and "what is this teammate called" had quietly become two of them.

===========================================================================
EXACT JOINS ONLY — AND WHY THERE IS NO ROLE FALLBACK
===========================================================================
Two independent sources, both exact, neither a heuristic:

  1. worker-events.jsonl `WorkerCreated.worker_name`, joined on `agent_id`.
     PostToolUse[Agent] records the spawn's own `name` input alongside the
     agent id extracted from the launch acknowledgement. Cheap, local,
     durable.

  2. The orchestrator's transcript, joined on tool_use_id: the `Agent`
     tool_use carries `input.name`, the matching tool_result carries
     `toolUseResult.agentId`. That join already exists in this engine as
     scripts/lib/agent-liveness.py:names_to_ids and is DELEGATED to, never
     re-implemented. It is the source that works when the PostToolUse[Agent]
     emitter is not registered — which is the live shape on this machine:
     session 374e6f14's worker-events.jsonl holds WorkerStarted rows only.

REFUSED, deliberately: matching a notice's recipient to a worktree by ROLE
PREFIX. `zach-opus-s1` and `zach-opus-n1` share a role, and on the day of
the defect three Zachs were running at once. A role-prefix credit would
report a teammate as told when a DIFFERENT teammate was told — a guard
lying in the direction that lets a land through. guard-agent-state-claims.py
refuses the same fallback for the same reason, and says so in its own
header. When neither exact source resolves, this module returns no name and
NAMES THE SOURCES IT TRIED, so the operator fixes the join instead of
waiving the guard.

===========================================================================
AND WHERE THE RECORD LIVES
===========================================================================
resolve_teams_dir() is here rather than in inflight.py for the same reason:
the witness writes the notice ledger and the guard reads it, and if the two
resolve the session team directory differently the ledger is written to one
path and read from another — the same defect one layer down.
"""

import glob
import json
import os
import re
import sys

# The spawn-name shape guard-worktree-isolation.sh enforces,
# <role>-<model>-<effort>-<id>, and the <role>-<model>-<id> it enforced before
# 2026-11-17: a name on a workspace that already exists has either.
TEAMMATE_NAME_RE = re.compile(
    r"^[a-z][a-z0-9]{1,15}-(?:fable|opus|sonnet|haiku)-(?:(?:low|medium|high|xhigh|max)-)?[a-z0-9]{1,12}$")

# Session team directories that hold one of these are teams that actually ran.
# spawned-names.log is deliberately NOT in this set: it is appended for spawns
# in sessions that never produced a team stream at all, and on this machine
# three dead session directories carry nothing else.
TEAM_STREAM_FILES = (
    "worker-events.jsonl",
    "inflight-notices.jsonl",
    "inflight-waivers.jsonl",
    "inflight-repos.txt",
    "idle-events.jsonl",
    "task-events.jsonl",
)


def looks_like_teammate_name(text):
    return bool(TEAMMATE_NAME_RE.match((text or "").strip()))


def role_of(name):
    """The role token of a well-formed teammate name, else ""."""
    name = (name or "").strip()
    if not looks_like_teammate_name(name):
        return ""
    return name.split("-", 1)[0]


# --------------------------------------------------------------------------
# the transcript
# --------------------------------------------------------------------------
def _agent_liveness():
    """The engine's existing exact name->id join, loaded by path.

    Imported lazily and defensively: this module is used by a log-only hook
    that must never fail, and a missing sibling must cost a source, not a run.
    """
    try:
        import importlib.util as ilu
        here = os.path.dirname(os.path.abspath(__file__))
        spec = ilu.spec_from_file_location(
            "agent_liveness_for_identity", os.path.join(here, "agent-liveness.py"))
        mod = ilu.module_from_spec(spec)
        spec.loader.exec_module(mod)
        return mod
    except Exception:
        return None


def transcript_for_session(session_id, projects_dir=""):
    """The orchestrator transcript for a session id, or "".

    ~/.claude/projects/<cwd-slug>/<session-id>.jsonl. The slug is not derivable
    from anything a hook is handed, so it is globbed. An 8-character prefix is
    accepted too, because a session TEAM directory is named `session-<first8>`
    and that is often the only form a by-hand caller has. Exactly one match or
    nothing: two matches means two sessions, and picking one would attribute
    one session's spawns to another.
    """
    sid = (session_id or "").strip()
    if not sid:
        return ""
    base = projects_dir or os.environ.get("CLAUDE_PROJECTS_DIR") or os.path.join(
        os.path.expanduser("~"), ".claude", "projects")
    for pattern in ("%s.jsonl" % sid, "%s*.jsonl" % sid[:8]):
        hits = sorted(glob.glob(os.path.join(base, "*", pattern)))
        hits = [h for h in hits if os.path.isfile(h)]
        if len(hits) == 1:
            return hits[0]
        if len(hits) > 1:
            return ""
    return ""


def session_id_from_teams_dir(teams_dir):
    """`.../session-374e6f14` -> `374e6f14`.

    A session TEAM directory is named for its session, so this is a read, not a
    guess — and it is the rung that makes the by-hand `status` work: a terminal
    has no CLAUDE_SESSION_ID, but the directory the sweep just resolved names
    the session whose transcript holds the name join."""
    base = os.path.basename((teams_dir or "").rstrip("/"))
    return base[len("session-"):] if base.startswith("session-") else ""


def resolve_transcript(transcript_path="", session_id="", teams_dir=""):
    """An explicit path wins; then the session id; then the team directory."""
    tp = (transcript_path or "").strip() or os.environ.get("INFLIGHT_TRANSCRIPT", "")
    if tp and os.path.isfile(tp):
        return tp
    found = transcript_for_session(session_id)
    if found:
        return found
    return transcript_for_session(session_id_from_teams_dir(teams_dir))


# --------------------------------------------------------------------------
# the index
# --------------------------------------------------------------------------
def _read_jsonl(path):
    rows = []
    if not path or not os.path.isfile(path):
        return rows
    try:
        with open(path, "r", encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    rows.append(json.loads(line))
                except Exception:
                    continue
    except Exception:
        pass
    return rows


def identity_index(teams_dir="", transcript_path="", session_id=""):
    """The whole answer, in one pass.

    Returns:
        {
          "names":   {agent_id: name},          # EXACT, unique spawn names
          "sources": {agent_id: source-label},
          "roles":   {agent_id: role},          # agent_type — a ROLE, never a name
          "tried":   [source-label, ...],       # every source consulted
          "found":   [source-label, ...],       # the ones that produced anything
          "spawned": [name, ...],               # spawned-names.log, if present
        }
    """
    out = {"names": {}, "sources": {}, "roles": {}, "tried": [], "found": [],
           "spawned": []}

    # --- source 1: WorkerCreated.worker_name, joined on agent_id -----------
    label = "worker-events.jsonl WorkerCreated.worker_name"
    out["tried"].append(label)
    events = _read_jsonl(os.path.join(teams_dir, "worker-events.jsonl")) if teams_dir else []
    for row in events:
        aid = row.get("agent_id") or ""
        if not aid:
            continue
        role = row.get("agent_type") or ""
        if role and aid not in out["roles"]:
            out["roles"][aid] = role
        name = row.get("worker_name") or ""
        if name and aid not in out["names"]:
            out["names"][aid] = name
            out["sources"][aid] = label
    if any(v == label for v in out["sources"].values()):
        out["found"].append(label)

    # --- source 2: the transcript's tool_use_id join -----------------------
    label = "orchestrator transcript (Agent tool_use -> toolUseResult.agentId)"
    tp = resolve_transcript(transcript_path, session_id, teams_dir)
    out["tried"].append("%s [%s]" % (label, tp or "not found"))
    if tp:
        al = _agent_liveness()
        if al is not None:
            try:
                for name, aid in (al.names_to_ids(tp) or {}).items():
                    if aid and name and aid not in out["names"]:
                        out["names"][aid] = name
                        out["sources"][aid] = label
            except Exception:
                pass
    if any(v == label for v in out["sources"].values()):
        out["found"].append(label)

    # --- context: the names this session actually spawned ------------------
    try:
        with open(os.path.join(teams_dir, "spawned-names.log"), encoding="utf-8") as fh:
            out["spawned"] = [ln.strip() for ln in fh if ln.strip()]
    except Exception:
        pass

    return out


def name_for_agent_id(agent_id, index):
    """(name, source) for an agent id — "" when no exact source resolved it."""
    if not agent_id:
        return "", ""
    return index["names"].get(agent_id, ""), index["sources"].get(agent_id, "")


def agent_id_for_name(name, index):
    """(agent_id, source) for a teammate name — the inverse, equally exact.

    Ambiguity resolves to nothing: if two agent ids carry the same name the
    join is not exact any more, and this module does not guess.
    """
    name = (name or "").strip()
    if not name:
        return "", ""
    hits = [aid for aid, nm in index["names"].items() if nm == name]
    if len(hits) != 1:
        return "", ""
    return hits[0], index["sources"].get(hits[0], "")


# --------------------------------------------------------------------------
# where the record lives
# --------------------------------------------------------------------------
def _teams_base():
    """(base, explicit) — the directory session team dirs live under."""
    for var in ("INFLIGHT_TEAMS_DIR", "WORKER_EVENTS_TEAMS_DIR"):
        val = os.environ.get(var, "").strip()
        if val:
            return val, var
    return os.path.join(os.path.expanduser("~"), ".claude", "teams"), ""


def _session_dirs(base):
    try:
        return sorted(
            os.path.join(base, n) for n in os.listdir(base)
            if n.startswith("session-") and os.path.isdir(os.path.join(base, n)))
    except Exception:
        return []


def _has_team_stream(d):
    return any(os.path.isfile(os.path.join(d, f)) for f in TEAM_STREAM_FILES)


def _newest_mtime(d):
    newest = 0.0
    try:
        for n in os.listdir(d):
            try:
                newest = max(newest, os.path.getmtime(os.path.join(d, n)))
            except OSError:
                continue
    except Exception:
        pass
    return newest


def resolve_teams_dir(session_id="", base=None):
    """(team-dir, how) — the session team directory the ledgers live in.

    THE LADDER, strictest first, and every rung is REPORTED rather than
    silently taken. The old resolution had two rungs — exact session id, then
    "only if there is exactly one session directory" — and on a machine with
    four of them a by-hand `inflight-notify.sh status` printed
    `notice ledger: <no team dir resolved>`: the operator's own diagnostic,
    blind at exactly the moment he needs it.

      1. INFLIGHT_TEAMS_DIR / WORKER_EVENTS_TEAMS_DIR pointing STRAIGHT AT a
         team directory (no session-* children). Documented in three headers
         and in `waive`'s own error message, and until now honored by none of
         them. The code follows the documentation, not the other way round.
      2. The session id, exactly.
      3. The only session directory under the base.
      4. The only session directory carrying a live team stream.
      5. The most recently active one, NAMED as such with the count it was
         chosen from — a guess the caller can see is a guess.

    Rungs 4 and 5 can only ever reduce false blocks: a wrong pick finds no
    notices, and no notices is a REFUSAL. There is no reading of this ladder
    that lets a land through that the old one would have stopped.
    """
    explicit = ""
    if base is None:
        base, explicit = _teams_base()
    if not base or not os.path.isdir(base):
        return "", "no team directory exists at %s" % (base or "<unset>")

    sessions = _session_dirs(base)

    # 1. an explicit pointer straight at a team directory
    if explicit and not sessions:
        return base, "%s points straight at the team directory" % explicit

    sid = (session_id or "").strip()
    if sid:
        cand = os.path.join(base, "session-%s" % sid[:8])
        if os.path.isdir(cand):
            return cand, "exact session id match (session-%s)" % sid[:8]

    if not sessions:
        return "", "no session team directories under %s" % base
    if len(sessions) == 1:
        return sessions[0], "the only session team directory under %s" % base

    active = [d for d in sessions if _has_team_stream(d)]
    if len(active) == 1:
        return active[0], (
            "the only one of %d session team directories carrying a team stream"
            % len(sessions))
    pool = active or sessions
    pool = sorted(pool, key=_newest_mtime, reverse=True)
    return pool[0], (
        "MOST RECENTLY ACTIVE of %d candidates (%s) — no session id was given, "
        "so this is a choice, not a match; pass --session to name one"
        % (len(pool), ", ".join(os.path.basename(p) for p in pool[:4])))


def main(argv):
    import argparse
    ap = argparse.ArgumentParser(prog="teammate-identity.py")
    ap.add_argument("--teams-dir", default="")
    ap.add_argument("--session", default="")
    ap.add_argument("--transcript", default="")
    ap.add_argument("--resolve-teams-dir", action="store_true")
    ap.add_argument("--how", action="store_true")
    ap.add_argument("--with-how", action="store_true")
    ap.add_argument("--name-for", default="")
    ap.add_argument("--id-for", default="")
    args = ap.parse_args(argv)

    if args.resolve_teams_dir:
        path, how = resolve_teams_dir(args.session)
        if args.with_how:
            sys.stdout.write("%s\t%s" % (path, how))
        else:
            sys.stdout.write(how if args.how else path)
        return 0

    index = identity_index(args.teams_dir, args.transcript, args.session)
    if args.name_for:
        name, source = name_for_agent_id(args.name_for, index)
        sys.stdout.write("%s\t%s" % (name, source))
        return 0
    if args.id_for:
        aid, source = agent_id_for_name(args.id_for, index)
        sys.stdout.write("%s\t%s" % (aid, source))
        return 0
    print(json.dumps(index, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
