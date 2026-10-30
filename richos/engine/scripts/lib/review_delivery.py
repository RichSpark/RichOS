#!/usr/bin/env python3
"""review_delivery.py: A SECOND-REVIEW VERDICT REACHES THE RUNNING TEAMMATE, ONCE.

Ruling §113 (2026-10-08): "A regular RichOS user can never be expected anything
even remotely close to that. So, this all must be completely automated." Slice 3
of richos-hq docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md
(§2.5, "Changes requested, mid-job": "Delivered to the running teammate once, at
its next tool call, by a hook (no mailbox). The lead is told; nothing is
stopped.") review-watch starts a mid-job review of a long or quiet job and tells
the lead; this tells the teammate whose work it is.

It is the body of scripts/hooks/deliver-review-verdict.sh, a matcherless
PreToolUse hook. For a call made inside a teammate (the payload carries
agent_id), every verdict in second-review's ledger (<state>/reviews.jsonl) on
that teammate's work (its own record, or one it continues) that finished after
that work was registered (the continued record's registration for a verdict on
continued work), and was not yet delivered to this teammate, is printed as
additionalContext, which reaches the teammate's model with that call. "Once" is
a marker file per teammate and review, created exclusively, so two calls made
at the same moment cannot both deliver it.

It never blocks, never stops, edits or messages anything: every outcome is exit 0.
A verdict waits for the teammate's next call however long that takes: which
verdicts are still owed is decided by registration and the delivery markers,
never by age. The lead's own calls cost no interpreter, and a teammate's call
while the ledger holds no verdict on any teammate's work costs one read of it;
the workspace registry is loaded only when one could be this teammate's.

Test seams (second-review-land.test.sh, the class
SecondReview_AMidJobVerdictReachesTheRunningTeammate of
mega-lander/tests/workspaces.test.py): SECOND_REVIEW_STATE_DIR (the ledger),
REVIEW_WATCH_STATE_DIR (the markers), RICHOS_WORKSPACES_DIR (the registry).
"""

import calendar
import hashlib
import importlib.util
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
MAX_FINDINGS = 8


def epoch(text):
    try:
        return float(calendar.timegm(time.strptime(str(text), "%Y-%m-%dT%H:%M:%SZ")))
    except (TypeError, ValueError):
        return None


def config_base():
    return (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude")


def claude_home():
    """machine_settings.claude_home(), loaded from beside this file: the machine's one Claude
    home, where second-review writes its ledger whichever account the writer spent."""
    mod = sys.modules.get("machine_settings")
    if mod is None or not hasattr(mod, "claude_home"):
        spec = importlib.util.spec_from_file_location("machine_settings", os.path.join(HERE, "machine_settings.py"))
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
    return mod.claude_home()


def ledger_path():
    d = (os.environ.get("SECOND_REVIEW_STATE_DIR") or "").strip() or os.path.join(claude_home(), "state")
    return os.path.join(d, "reviews.jsonl")


def marker_dir():
    d = (os.environ.get("REVIEW_WATCH_STATE_DIR") or "").strip() or os.path.join(config_base(), "state", "review-watch")
    return os.path.join(d, "delivered")


def teammate_verdicts():
    """Every finished verdict on a teammate's work in the ledger, however old.
    Which of them a teammate still gets is decided by its registration and the
    delivery markers alone (main), never by age: a teammate whose next tool call
    comes a day later still gets it then (review rv-20261009T023055Z-b51ebb97-bc65,
    finding 3)."""
    rows = []
    try:
        with open(ledger_path(), encoding="utf-8") as f:
            for line in f:
                try:
                    r = json.loads(line)
                except ValueError:
                    continue
                if not isinstance(r, dict) or not r.get("verdict") or not r.get("id"):
                    continue
                if not str(r.get("work") or "").startswith("teammate:"):
                    continue
                done = epoch(r.get("finished_at"))
                if done is not None:
                    r["_done"] = done
                    rows.append(r)
    except OSError:
        pass
    return rows


def load_workspaces():
    path = os.path.join(HERE, "..", "..", "mega-lander", "workspaces.py")
    spec = importlib.util.spec_from_file_location("review_delivery_workspaces", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def teammate(agent_id):
    """(record, {work key: that work's registration epoch}) for every record of
    its chain (its own and each one it continues), or None. A verdict is judged
    against the registration of the work it is ON, never the continuation's
    own: a verdict on the predecessor's work that finished between the two
    registrations is still owed to the continuation (review
    rv-20261009T031207Z-ba444a8a-607a, finding 2)."""
    ws = load_workspaces()
    key = ws.key_for_id(agent_id)
    rec = ws.load_agent(key) if key else None
    if not rec:
        return None
    works = dict(("teammate:%s" % r["key"], epoch(r.get("registered_at")) or 0.0) for r in ws._chain(rec))
    return rec, works


def claim(key, rid):
    d = marker_dir()
    os.makedirs(d, exist_ok=True)
    name = "%s-%s" % (hashlib.sha1(key.encode("utf-8")).hexdigest()[:16], rid)
    try:
        os.close(os.open(os.path.join(d, name), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600))
        return True
    except FileExistsError:
        return False


def findings(row):
    try:
        with open(os.path.join(row.get("record") or "", "verdict.json"), encoding="utf-8") as f:
            answer = (json.load(f) or {}).get("answer") or {}
    except (OSError, ValueError, AttributeError):
        return [], []
    return answer.get("findings") or [], answer.get("not_yet_claimed") or []


def render(row):
    mid = row.get("trigger") in ("long-job", "quiet")
    verdict = str(row.get("verdict"))
    out = ["SECOND REVIEW OF YOUR WORK (%s review, nobody asked for it: it starts by itself): %s@%s, by %s (%s): "
           "%s, %s finding(s), %s P1." % (
               "a mid-job" if mid else "a " + str(row.get("trigger")), os.path.basename(row.get("repo") or ""),
               str(row.get("tip"))[:12], row.get("reviewer"), row.get("reviewer_model"), verdict.upper(),
               row.get("findings"), row.get("p1"))]
    found, pending = findings(row)
    for f in found[:MAX_FINDINGS]:
        if isinstance(f, dict):
            out.append("  P%s %s (%s): %s" % (f.get("priority"), " ".join(str(f.get("title") or "").split()),
                                             ", ".join(str(x) for x in f.get("files") or []),
                                             " ".join(str(f.get("evidence") or "").split())[:400]))
    if len(found) > MAX_FINDINGS:
        out.append("  ... and %d more" % (len(found) - MAX_FINDINGS))
    if pending:
        out.append("  Not yet claimed at that commit: %s" % "; ".join(str(p) for p in pending[:6])[:400])
    out.append("  The full verdict and its fixtures: %s" % os.path.join(row.get("record") or "?", "verdict.json"))
    if verdict == "changes-requested":
        out.append("  Nothing was stopped. Fix what it found that falls inside your task, in your next commits; "
                   "your handover is reviewed again, and your work lands only when that review passes. "
                   "The lead has been told as well.")
    else:
        out.append("  Nothing to change for what it reviewed; your handover is still reviewed again.")
    return "\n".join(out)


def main():
    try:
        payload = json.loads(sys.stdin.read())
    except ValueError:
        return 0
    if not isinstance(payload, dict):
        return 0
    aid = str(payload.get("agent_id") or "")
    if not aid:
        return 0                                # the lead's own call: review-watch tells the lead
    rows = teammate_verdicts()
    if not rows:
        return 0
    who = teammate(aid)
    if not who:
        return 0
    rec, works = who
    # "Once" is per teammate: the marker is keyed by THIS record, so a verdict
    # its predecessor already received still reaches the continuation, once.
    texts = [render(r) for r in rows if r.get("work") in works and r["_done"] >= works[r["work"]] - 1
             and claim(rec["key"], str(r["id"]))]
    if texts:
        print(json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse",
                                                 "additionalContext": "\n\n".join(texts)}}))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # noqa: BLE001: a delivery hook never fails a tool call
        sys.stderr.write("review_delivery: %s: %s\n" % (exc.__class__.__name__, exc))
        sys.exit(0)
