#!/usr/bin/env python3
"""escalations.py — AN ESCALATION A TEAMMATE WRITES CANNOT QUIETLY FAIL TO ARRIVE.

Read scripts/lib/escalations.sh for the wiring. This file is the mechanism, and
the mechanism has to be exact and testable without a session.

===========================================================================
THE FAILURE THIS EXISTS TO REMOVE
===========================================================================
On 2026-09-02 two teammates finished their work and each wrote a `BLOCKED.md`
recording that a premise in its brief was contradicted by evidence. NEITHER WAS
A STALL. Both were right to write one. They were found on 2026-09-04 by a
worktree cleanup, because somebody was counting directories.

The mailbox is measured at roughly 50% loss and doctrine forbids it carrying
load-bearing signal, so a FILE was chosen as the durable substrate instead. It
is durable in exactly the wrong direction: it survives perfectly and is read by
nobody. An escalation written as a file on a teammate's own branch is visible
only to whoever merges that branch — and a teammate whose branch is never
landed takes its escalation with it.

Three facts shaped everything below:

  1. THE ROOT IS CLOSED. Both wrote to the repository root, which is nine
     entries by permanent CEO ruling (ceo-decisions.md 27). The same escalation
     today would be refused at the write with nowhere obvious to go. That
     ruling names `docs/verification/` as where a block record belongs, so the
     record file goes there and never at a root.

  2. A TEAMMATE CANNOT SEE WHETHER ITS BRANCH WAS LANDED. It finishes; the
     merge happens later or never. So DELIVERY MUST NOT DEPEND ON ANYTHING THE
     TEAMMATE DOES AFTER WRITING, and it must not depend on a merge at all.

  3. THE DURABLE LEDGERS ALREADY REACH THE LEAD WITHOUT A MERGE. That is the
     property to copy, and it is the one this ledger has.

===========================================================================
WHY A LEDGER AND NOT A SECOND MAILBOX — the alternatives, and why each loses
===========================================================================
  THE EXISTING EVENT LOGS (idle-events.jsonl / task-events.jsonl) — REJECTED
    as the carrier, though they were the first place to look and the row said
    so. They are written BY HOOKS from harness payloads; there is no field a
    teammate can put its own words in, and nothing in the payload is authored
    by the teammate at all. Piggybacking would mean a hook lifting a file out
    of the worktree at TeammateIdle — which re-introduces both defects at once:
    it fires only if the teammate idles cleanly, and it reads a path that the
    reaper may already have removed.

  THE MAILBOX (SendMessage) — REJECTED as the carrier and KEPT as the doorbell.
    Half of what crosses it is lost. It stays in the protocol because a doorbell
    that sometimes rings is strictly better than none, and it now costs nothing:
    the ledger row is written first and the message is advisory.

  A FILE IN THE WORKTREE — KEPT as the RECORD and demoted from the delivery
    path. It is genuinely useful (it lands with the branch, it is readable by a
    human opening the worktree, and 27 says a block record belongs under
    `docs/verification/`) and it is genuinely NOT a delivery mechanism.

  A NEW MAILBOX — REFUSED outright. This is a LEDGER: append-only, written by
    the raiser, read by a CHECK rather than delivered to a reader, and closed by
    an acknowledgement row rather than by being consumed. It has one file, one
    writer path, and two readers that both call this module. Two ledgers written
    by one writer and read by nobody is how the waiver count reached 251 on
    2026-09-02; there is exactly one here.

===========================================================================
WHERE IT LIVES, AND WHY NOT IN THE SESSION TEAM DIRECTORY
===========================================================================
    ~/.claude/state/escalations.jsonl

OUTSIDE EVERY REPOSITORY, EVERY WORKTREE AND EVERY SESSION DIRECTORY — the same
substrate the worktree ownership ledger already uses, for the same reason.

The session team directory (<teams>/session-<first8>/) was the obvious home and
it is wrong here, for a reason the original failure demonstrates: THE
ESCALATIONS SAT FOR TWO DAYS, ACROSS SESSIONS. A ledger scoped to a session
dies with the session that raised it, and the sessions that follow — the ones
that would have found it — would each start with an empty world. A per-session
ledger would have hidden this defect rather than fixed it.

Repository scoping is refused for the same class of reason. A femcboost seat
landing work in richos worktrees is the NORMAL shape of this operation, not an
edge; a notice that only reported escalations raised in the seat's own
repository would hide the common case. Every outstanding escalation is
reported, wherever it was raised. The volume that justifies this: two in a week.

===========================================================================
WHAT AN ESCALATION IS, AND THE FIELD THAT SEPARATES IT FROM A STALL
===========================================================================
`state` is REQUIRED, and it is the field that stops this mechanism teaching
teammates not to raise escalations:

  work-complete  The work is DONE. This is a record of something the lead must
                 know, not a request to be unblocked. BOTH of the originals
                 were this, and both said so explicitly. A mechanism that read
                 every escalation as a failure would have punished them for
                 being right.
  proceeding     Raised, and still working on everything that does not depend
                 on the answer. The doctrine's default.
  stopped        The whole task depends on the answer and work has stopped.
                 The only state that is a stall, and the only one the notices
                 frame as blocking.

`for` says who the answer belongs to — `lead` (default) or `ceo`. The two
originals were addressed to the CEO, which is exactly why nobody in between
felt they were theirs to act on. Naming the audience makes the lead's job
explicit: route it or answer it, but not neither.

`question` is REQUIRED and is the smallest question that would unblock. A
record with no question is a note; there is nothing to acknowledge and nothing
would ever close it.

===========================================================================
IDENTITY, AND WHY IT IS DERIVED
===========================================================================
    esc-<UTC timestamp, compact>-<first 8 hex of sha256(teammate|title|worktree)>

Time-ordered, greppable, unique per (teammate, title, worktree) per second, and
typed by nobody. Raising the same escalation twice produces two ids and two
rows on purpose — a duplicate is visible, and de-duplicating would mean
deciding that two teammates' words are the same thing, which is a judgment.

===========================================================================
AGE BUCKETS — why an unacknowledged escalation gets LOUDER
===========================================================================
The turn-end notice is state-change de-duplicated, because a line repeated
under every turn is a line the eye is trained to skip. That de-duplication is
correct and it has one dangerous consequence: a condition that never changes is
announced once and then goes quiet forever. THE ORIGINALS SAT FOR TWO DAYS. A
mechanism that announced them once on day one and then fell silent would have
reproduced the defect with a better audit trail.

So the notice's state key carries an AGE BUCKET, and crossing a boundary is a
state change:

    0 -> 60 minutes   ONE RUN SEGMENT. The measured median teammate run segment
                      on this machine is 40 minutes (22 completed segments,
                      2026-08-30, the same measurement inflight.sh's ack
                      timeout is derived from). An escalation older than one
                      median segment has outlived the teammate that raised it,
                      so nobody is going to bring it up again.
    -> 24 hours       A calendar day. At this point "nobody has looked" has
                      stopped being a timing accident.
    -> 72 hours       Past the observed failure. The originals were found at
                      about 48 hours; a bucket at 72 fires on anything worse
                      than the incident that caused this file to exist.

Plus: the notice ledger is keyed per SESSION, so every new session re-announces
every outstanding escalation from scratch. Silence about an escalation is only
ever "still what I told you this session".

===========================================================================
TWO TEAMMATES RAISING AT ONCE
===========================================================================
Every write is a single `O_APPEND` write of one line, which the kernel serializes
against other appends to the same file — so the normal case needs no lock, and a
lock is refused deliberately: a lock file in `~/.claude/state/` left behind by a
killed agent would block the next teammate's escalation, and a mechanism whose
failure mode is "your escalation was not raised" is the defect being fixed.

The pathological case is a line longer than the pipe buffer interleaving with
another. It is not prevented; it is SURVIVED. A line that will not parse is
counted as malformed and REPORTED by every reader — never silently dropped —
because a half-corrupt ledger reporting as a clean empty world is how this
whole class of defect works.

===========================================================================
CLOSING ONE — an acknowledgement, never a deletion
===========================================================================
`ack` APPENDS a row; nothing is ever removed or rewritten. It requires a
DISPOSITION of at least 30 characters saying what was decided or done, because
an ack with no disposition is a dismissal wearing a ledger row — the exact
shape of the 251 waivers.

Anyone can write an ack. Nothing here checks that the disposition is TRUE; a
string match is not comprehension and this engine does not pretend otherwise.
What is engineered out is an escalation that is never seen AT ALL, which is the
failure that happened.
"""

import argparse
import getpass
import hashlib
import json
import os
import re
import sys
from datetime import datetime, timezone

# --- the closed vocabularies -----------------------------------------------
STATES = ("work-complete", "proceeding", "stopped")
AUDIENCES = ("lead", "ceo")
# `needs` (2026-10-01): WHOSE HANDS the answer takes, which `for` cannot say.
# On 2026-10-01 three escalations that needed the CEO physically at the test
# iPhone were all `for: lead`, rightly: the lead fetches him. One value, closed:
# `ceo-hands` means the answer needs the CEO AT A DEVICE (an approval, Touch ID,
# a passcode, a login, a cable). The stall watcher tells such an escalation
# first and every 10 minutes (stall_watch.py, ESCALATION). Optional: absent is
# the ordinary case, and a row without it is exactly as before.
NEEDS = ("ceo-hands",)
NEEDS_GLOSS = {"ceo-hands": "NEEDS THE CEO AT A DEVICE (an approval, Touch ID, a passcode, a login, a cable)"}

# Minutes. See the header: one median run segment, one day, past the incident.
AGE_BUCKETS = ((72 * 60, "72h"), (24 * 60, "24h"), (60, "1h"), (0, "new"))

MIN_QUESTION = 20
MIN_DISPOSITION = 30

# THE TOLERATED AGE, in hours. An escalation younger than this is news; one
# older than this is a BACKLOG, and the owned-state gate refuses dispatches
# while any backlog item is outstanding and nothing has made it smaller this
# session (owned-systems.declaration, row `escalations`). 24 h because it is
# the loudness bucket this channel already shouts at, and one working day is
# the longest a teammate's question should wait on a lead who is working.
# It is printed in the summary line below, and the declaration's `backlog:`
# pattern reads the number back from there, so the threshold lives here once.
BACKLOG_AGE_HOURS = 24

STATE_GLOSS = {
    "work-complete": "work COMPLETE — a record, NOT a stall",
    "proceeding": "still working on everything that does not depend on this",
    "stopped": "STOPPED — the task depends on the answer",
}


def utcnow():
    return datetime.now(timezone.utc)


def iso(dt):
    return dt.replace(microsecond=0).isoformat().replace("+00:00", "Z")


def parse_iso(s):
    """Parse an ISO timestamp written by this module, or return None.

    Tolerant on purpose: a row whose timestamp cannot be read must still be
    REPORTED (as unknown age), never dropped. A ledger reader that silently
    discards rows it does not understand is the defect one level down.
    """
    if not s:
        return None
    t = str(s).strip()
    if t.endswith("Z"):
        t = t[:-1] + "+00:00"
    try:
        d = datetime.fromisoformat(t)
    except Exception:
        return None
    if d.tzinfo is None:
        d = d.replace(tzinfo=timezone.utc)
    return d


def ledger_path():
    """The ONE ledger. Overridable for tests and for a non-standard home."""
    p = os.environ.get("RICHOS_ESCALATION_LEDGER")
    if p:
        return os.path.abspath(os.path.expanduser(p))
    return os.path.join(os.path.expanduser("~"), ".claude", "state", "escalations.jsonl")


def read_rows(path=None):
    """Every row, in file order. A malformed line is SKIPPED and COUNTED.

    Returns (rows, malformed_count), or (None, 0) when the ledger exists and
    cannot be read. UNREADABLE IS NOT EMPTY: a reader that returned an empty
    list for an unreadable file would report a clean world over a broken one.
    """
    path = path or ledger_path()
    rows, bad = [], 0
    try:
        with open(path, "r", encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    d = json.loads(line)
                except Exception:
                    bad += 1
                    continue
                if isinstance(d, dict):
                    rows.append(d)
                else:
                    bad += 1
    except FileNotFoundError:
        return [], 0
    except Exception:
        return None, 0
    return rows, bad


def append_row(row, path=None):
    """Append one row. RAISES on failure — a raise that silently no-ops is the
    whole defect, so the teammate has to be told its escalation did not land."""
    path = path or ledger_path()
    parent = os.path.dirname(path)
    if parent:
        os.makedirs(parent, exist_ok=True)
    with open(path, "a", encoding="utf-8") as fh:
        fh.write(json.dumps(row, ensure_ascii=False) + "\n")
    return path


def make_id(when, teammate, title, worktree):
    stamp = when.strftime("%Y%m%dT%H%M%SZ")
    h = hashlib.sha256(
        ("%s|%s|%s" % (teammate or "", title or "", worktree or "")).encode("utf-8")
    ).hexdigest()[:8]
    return "esc-%s-%s" % (stamp, h)


def age_bucket(age_min):
    if age_min is None:
        return "unknown"
    for floor, name in AGE_BUCKETS:
        if age_min >= floor:
            return name
    return "new"


def outstanding(rows, now=None):
    """Every Escalation row with no LIVE EscalationAck, oldest first.

    Each carries `age_min` and `bucket`. An ack for an id that was never raised
    is IGNORED rather than treated as closing something — it closes nothing, and
    inventing a subject for it would be worse than leaving it in the file.

    AN ACKNOWLEDGEMENT CAN EXPIRE (2026-09-10, failure type J — "a claim baked
    into a record with no expiry"). The worked example in the record is a lock
    reason ending "the agent is live": when that agent finished the sentence
    became false, nothing could void it, and it blocked the fix until a person
    removed it by hand. A disposition has exactly the same shape — "waiting on
    a decision that is the CEO's" closes the demand permanently, INCLUDING on
    the day after he decides — and land-disposition.py says so about itself in
    its own header, which is more honest than hiding it and is still an
    instance of the defect.

    So `ack --until <date>` writes `until_epoch` on the ack, and an ack whose
    date has passed no longer closes anything: the escalation reappears here
    carrying `reopened_by_expiry` and the disposition that expired, so the
    reader sees WHY it is back. An ack with no `--until` is permanent, exactly
    as before — nothing existing changes behavior.
    """
    now = now or utcnow()
    now_epoch = now.timestamp()
    acked = set()
    expired = {}
    for r in rows:
        if r.get("event") != "EscalationAck" or not r.get("id"):
            continue
        rid = str(r["id"])
        until = r.get("until_epoch")
        if isinstance(until, (int, float)) and until <= now_epoch:
            # Held on a reason with a shelf life, and the shelf life is over.
            # Recorded rather than discarded: the ack still happened.
            expired[rid] = r
            continue
        acked.add(rid)
        expired.pop(rid, None)
    out = []
    for r in rows:
        if r.get("event") != "Escalation":
            continue
        rid = str(r.get("id") or "")
        if not rid or rid in acked:
            continue
        raised = parse_iso(r.get("raised"))
        age = None
        if raised is not None:
            age = int((now - raised).total_seconds() // 60)
            if age < 0:
                age = 0
        e = dict(r)
        e["age_min"] = age
        e["bucket"] = age_bucket(age)
        gone = expired.get(rid)
        if gone:
            e["reopened_by_expiry"] = {
                "acked": gone.get("acked"),
                "until": gone.get("until"),
                "disposition": gone.get("disposition"),
            }
        out.append(e)
    out.sort(key=lambda e: str(e.get("raised") or ""))
    return out


def counts(live):
    """The three numbers every reader leads with, computed in ONE place.

    2026-09-25: the backlog stood at 143 outstanding, the oldest 19 days, 13
    addressed to the CEO, and it was printed at every session start for a week
    — with the CEO's share at the END of a block that listed every escalation
    in full. The number was there; nobody read that far. So every surface now
    opens with these numbers, and they come from here so no two can disagree.

    An escalation whose `raised` cannot be parsed counts as backlog: an age
    nobody can read is never allowed to pass as young.
    """
    ceo = len([e for e in live if e.get("for") == "ceo"])
    floor = BACKLOG_AGE_HOURS * 60
    backlog = len([e for e in live
                   if e.get("age_min") is None or e.get("age_min") >= floor])
    return {"outstanding": len(live), "ceo": ceo, "backlog": backlog}


def summary_line(live):
    """The one line, under the header, that the owned-state gate reads.

    The `measure:`, `backlog:` and `since:` patterns of the `escalations` row in
    owned-systems.declaration are written against THIS line and the header
    above it, so rewording it is a change to that row too. owned-state.test.sh
    runs the SHIPPED row against the SHIPPED list output, which is what catches
    that drift instead of a gate that silently stops measuring.
    """
    c = counts(live)
    oldest = live[0] if live else None
    return ("  for the CEO: %d; older than %dh: %d; oldest raised %s (%s)."
            % (c["ceo"], BACKLOG_AGE_HOURS, c["backlog"],
               (oldest.get("raised") or "unknown") if oldest else "none",
               age_phrase(oldest.get("age_min")) if oldest else "none"))


def acks_for(rows, rid):
    return [r for r in rows
            if r.get("event") == "EscalationAck" and str(r.get("id")) == str(rid)]


def age_phrase(age_min):
    if age_min is None:
        return "age unknown"
    if age_min < 60:
        return "%dm old" % age_min
    if age_min < 24 * 60:
        return "%dh old" % (age_min // 60)
    return "%dd old" % (age_min // (24 * 60))


def _actor():
    try:
        return getpass.getuser()
    except Exception:
        return ""


# ---------------------------------------------------------------------------
# RAISE
# ---------------------------------------------------------------------------
def build_row(args, when=None):
    when = when or utcnow()
    teammate = (args.teammate or "").strip()
    title = (args.title or "").strip()
    worktree = os.path.abspath(os.path.expanduser(args.worktree)) if args.worktree else ""
    row = {
        "event": "Escalation",
        "id": make_id(when, teammate, title, worktree),
        "raised": iso(when),
        "teammate": teammate,
        "worktree": worktree,
        "branch": (args.branch or "").strip(),
        "repo": (args.repo or "").strip(),
        "head": (args.head or "").strip(),
        "state": args.state,
        "for": args.audience,
        "title": title,
        "question": (args.question or "").strip(),
        "tried": (args.tried or "").strip(),
        "meanwhile": (args.meanwhile or "").strip(),
        "record": (args.record or "").strip(),
        "session_id": (args.session or os.environ.get("CLAUDE_SESSION_ID", "") or "").strip(),
        "actor": _actor(),
    }
    needs = (getattr(args, "needs", "") or "").strip()
    if needs:
        # Only when given: a row without it is exactly the row it always was.
        row["needs"] = needs
    return row


def validate_raise(args):
    problems = []
    if not (args.title or "").strip():
        problems.append("--title is required: one line naming what this is about.")
    if "\n" in (args.title or ""):
        problems.append("--title must be ONE line.")
    if args.state not in STATES:
        problems.append(
            "--state must be one of: %s. It is REQUIRED because it is what separates an "
            "escalation from a stall, and a channel that reads every escalation as a failure "
            "teaches teammates not to raise them." % " ".join(STATES))
    if args.audience not in AUDIENCES:
        problems.append("--for must be one of: %s" % " ".join(AUDIENCES))
    needs = (getattr(args, "needs", "") or "").strip()
    if needs and needs not in NEEDS:
        problems.append(
            "--needs must be one of: %s, or left out. It says the answer needs the CEO "
            "physically at a device; any other word would be stored and read by nothing."
            % " ".join(NEEDS))
    q = (args.question or "").strip()
    if len(q) < MIN_QUESTION:
        problems.append(
            "--question is %d characters; it needs at least %d. State the SMALLEST question "
            "that would unblock this. A record with no question can never be closed, because "
            "there is nothing to answer." % (len(q), MIN_QUESTION))
    return problems


# ---------------------------------------------------------------------------
# RENDERING — three audiences, one predicate
# ---------------------------------------------------------------------------
def render_text(rows, bad, now=None):
    """Full detail, for a human at a terminal and for `escalate.sh list`."""
    now = now or utcnow()
    out = ["escalation ledger: %s" % ledger_path()]
    if bad:
        out.append("  ** %d malformed line(s) in the ledger were skipped — read them by hand." % bad)
    live = outstanding(rows, now)
    total = len([r for r in rows if r.get("event") == "Escalation"])
    out.append("  %d raised in all, %d OUTSTANDING (raised and never acknowledged)."
               % (total, len(live)))
    if not live:
        out.append("")
        out.append("  Nothing outstanding.")
        return "\n".join(out)
    out.append(summary_line(live))
    for e in live:
        out.append("")
        out.append("=== %s ===" % e["id"])
        out.append("  raised   : %s (%s)" % (e.get("raised", ""), age_phrase(e.get("age_min"))))
        out.append("  from     : %s" % (e.get("teammate") or "<unnamed>"))
        out.append("  state    : %s — %s"
                   % (e.get("state", ""), STATE_GLOSS.get(e.get("state", ""), "")))
        out.append("  for      : %s" % e.get("for", "lead"))
        if e.get("needs"):
            out.append("  needs    : %s — %s" % (e["needs"], NEEDS_GLOSS.get(e["needs"], "")))
        out.append("  title    : %s" % e.get("title", ""))
        out.append("  question : %s" % e.get("question", ""))
        if e.get("tried"):
            out.append("  tried    : %s" % e["tried"])
        if e.get("meanwhile"):
            out.append("  meanwhile: %s" % e["meanwhile"])
        if e.get("worktree"):
            out.append("  worktree : %s (%s)" % (e["worktree"], e.get("branch") or "no branch"))
        if e.get("record"):
            out.append("  record   : %s" % e["record"])
            out.append("             ^^ IN THE TEAMMATE'S WORKTREE. It may never be merged;")
            out.append("                nothing here depends on it, and this row is the escalation.")
        out.append("  close it : escalate.sh ack %s --disposition \"<what you decided or did>\""
                   % e["id"])
    out.append("")
    out.append("  An outstanding escalation is announced at EVERY session start and gets LOUDER")
    out.append("  as it ages (1h / 24h / 72h). It goes quiet only when it is acknowledged.")
    return "\n".join(out)


def render_hook_summary(rows, bad, now=None):
    """Three lines for the Stop hook: state key, count, the one sentence.

    The wrapper owns none of these. A wrapper that composed its own sentence
    could tell the operator a different number from the one `escalate.sh list`
    prints, and two answers to one question is the failure this engine keeps
    finding in itself.
    """
    now = now or utcnow()
    live = outstanding(rows, now)
    if not live and not bad:
        return "clear\n0\n"
    key_parts = ["%s:%s" % (e["id"], e["bucket"]) for e in live]
    if bad:
        key_parts.append("malformed:%d" % bad)
    key = "outstanding:" + "|".join(key_parts)

    ceo = [e for e in live if e.get("for") == "ceo"]
    stopped = [e for e in live if e.get("state") == "stopped"]
    oldest = live[0] if live else None
    bits = []
    if len(live) == 1 and oldest is not None:
        bits.append("%s raised an escalation %s and NOTHING HAS ACKNOWLEDGED IT: \"%s\""
                    % (oldest.get("teammate") or "a teammate",
                       age_phrase(oldest.get("age_min")), oldest.get("title", "")))
    elif live:
        bits.append("%d ESCALATIONS ARE OUTSTANDING, the oldest %s from %s: \"%s\""
                    % (len(live), age_phrase(oldest.get("age_min")),
                       oldest.get("teammate") or "a teammate", oldest.get("title", "")))
    if ceo:
        bits.append("%d is for the CEO" % len(ceo) if len(ceo) == 1
                    else "%d are for the CEO" % len(ceo))
    if stopped:
        bits.append("%d has STOPPED work" % len(stopped) if len(stopped) == 1
                    else "%d have STOPPED work" % len(stopped))
    if live and not stopped:
        # Said EVERY time, because the opposite reading is what kills the channel.
        bits.append("none is a stall — the work is done or continuing")
    if bad:
        bits.append("%d ledger line(s) are unreadable" % bad)
    sentence = ("ESCALATION OUTSTANDING — " + "; ".join(bits)
                + ". Read it: escalate.sh list. Close it: escalate.sh ack <id> "
                  "--disposition \"...\".")
    return key + "\n" + str(len(live)) + "\n" + sentence + "\n"


def model_entry(e):
    """One escalation as the lead's MODEL reads it: the teammate's own words.

    Shared by the SessionStart block and the turn-end delivery, so the lead
    reads an escalation the same way whichever of the two brought it.
    """
    needs = e.get("needs") or ""
    # The head is the DELIVERY KEY, not the bare id (2026-10-01): an escalation
    # reopened by an expired ack reads `[esc-…@reopened:<until>]`, so every reader
    # of the transcript (reached_scan) can tell the reopened item from the one
    # the lead was told about before. For every other escalation key == id.
    return ("  [%s] %s (%s), from %s, state=%s (%s), for=%s%s. QUESTION: %s%s%s"
            % (delivery_key(e), e.get("title", ""), age_phrase(e.get("age_min")),
               e.get("teammate") or "<unnamed>", e.get("state", ""),
               STATE_GLOSS.get(e.get("state", ""), ""), e.get("for", "lead"),
               (", needs=%s: %s" % (needs, NEEDS_GLOSS.get(needs, needs))) if needs else "",
               e.get("question", ""),
               (" TRIED: %s" % e["tried"]) if e.get("tried") else "",
               (" MEANWHILE: %s" % e["meanwhile"]) if e.get("meanwhile") else ""))


def render_session_context(rows, bad, now=None):
    """Two blocks for SessionStart: the model's paragraph, then the operator's.

    Separated by a form feed — one character that cannot occur in either half,
    so the shell can split without a parser.
    """
    now = now or utcnow()
    live = outstanding(rows, now)
    if not live and not bad:
        return ""
    # THE FIRST LINE IS THE COUNT AND THE CEO'S SHARE OF IT (2026-09-25). The
    # old first line gave the total and then listed every escalation in full;
    # the number addressed to the CEO came after all 143 entries, and a week of
    # session starts read past it.
    c = counts(live)
    model = ["%d ESCALATION(S) OUTSTANDING, %d FOR THE CEO (for=ceo), %d older than %dh — "
             "raised by teammates, never acknowledged, and waiting for you. This is the whole "
             "content; nothing has to be merged, and no worktree has to still exist, to read it."
             % (c["outstanding"], c["ceo"], c["backlog"], BACKLOG_AGE_HOURS)]
    for e in live:
        model.append(model_entry(e))
    if any(e.get("for") == "ceo" for e in live):
        model.append("  At least one is addressed to the CEO. Routing it to him or deciding it "
                     "yourself are both answers; leaving it is not — that is the failure this "
                     "channel was built from.")
    if not any(e.get("state") == "stopped" for e in live):
        model.append("  NONE of these is a stall. Each teammate either finished its work or is "
                     "still working; they are records of something you must know, not requests "
                     "to be rescued.")
    if bad:
        model.append("  %d ledger line(s) could not be parsed and are NOT counted above." % bad)
    model.append("  Acknowledge each one you have dealt with: escalate.sh ack <id> "
                 "--disposition \"<what you decided or did>\". Until then it is announced at "
                 "every session start and gets louder at 1h, 24h and 72h.")
    if live:
        oldest = live[0]
        op = ("%d ESCALATION(S) OUTSTANDING, %d FOR THE CEO — oldest %s from %s: \"%s\". "
              "escalate.sh list"
              % (c["outstanding"], c["ceo"], age_phrase(oldest.get("age_min")),
                 oldest.get("teammate") or "a teammate", oldest.get("title", "")))
    else:
        op = "%d unreadable line(s) in the escalation ledger — escalate.sh list" % bad
    return "\n".join(model) + "\f" + op


# ---------------------------------------------------------------------------
# DELIVERY — the NEW escalation reaches the lead's MODEL at turn end
# ---------------------------------------------------------------------------
# THE DEFECT (measured 2026-09-28, Rich's session 356e9552). A teammate raised
# esc-20260928T134933Z-a0e676a9 at 13:49Z. The Stop notice fired four times
# after it, and every firing is in the transcript as a `hook_success`
# attachment with the JSON in `stdout` and `content: ""`. Its only field was
# `systemMessage`, and a Stop hook's systemMessage is shown to the PERSON and
# never to the model — the engine measured that itself on 2.1.270 (the table
# in every rooted hook's bootstrap comment), and the hooks reference says the
# same ("systemMessage: Warning message shown to the user"). So the lead
# learned of an escalation only at the next session start, mixed in with every
# older one. The header of notice-escalations.sh promised "the ARRIVAL ... in
# the turn after it is raised". That arrival never happened for the model.
#
# THE CHANNEL: the Stop hook's `hookSpecificOutput.additionalContext`. The
# hooks reference: "Non-error feedback for Claude. The conversation continues so
# Claude can act on it, but unlike decision: block it is shown in the
# transcript as hook feedback rather than a hook error." It is the one Stop
# field that reaches the model, and the engine's 2.1.270 table records it as
# reaching the model on Stop. It is NOT a refusal: nothing has to be answered or
# acknowledged for the turn to end, and the next turn end has nothing new, so
# the lead's turn is extended by exactly one model call per new escalation.
#
# WHAT COUNTS AS ALREADY DELIVERED, per session:
#   1. whatever session-start-escalations.sh put in the model's context. Read
#      off the host's OWN record of it — the SessionStart `hook_success`
#      attachment in the transcript, whose stdout is that hook's JSON — rather
#      than assumed. The host caps additionalContext at 10,000 characters and
#      replaces a longer value with a file path and a 2,000-character preview,
#      so only the ids inside what the model actually got count. On 2026-09-25
#      the session-start block was 143 escalations long; everything past the
#      preview was never in front of the model, and is delivered here.
#   2. whatever this function delivered earlier in the same session.
# Everything else outstanding is NEW and is delivered once. An escalation
# reopened by an expired ack is new again, under a key that names the expiry.
#
# THE MEMORY is a small JSON file per session beside the Stop notice ledger
# (<entity>/.claude/state/stop-hook-notices/), NOT a row in the escalation
# ledger: that ledger is the teammates' record and every suite that drives this
# hook would otherwise write delivery rows into the operator's real one. A
# payload with no session id shares one "no-session" bucket, so a host that
# stopped sending session ids is told once, not at every turn end.
#
# RECORDED ONLY WHAT WAS ACTUALLY SENT. The delivery stays under the host cap;
# an escalation that did not fit is not marked delivered and goes out at the
# next turn end.
HOST_CONTEXT_CAP = 10000
HOST_PREVIEW_CHARS = 2000
DELIVERY_BUDGET = 9000
# Room kept for the paragraph's closing lines when the first entry is cut.
FOOTER_RESERVE = 1000
ID_RX = re.compile(r"esc-\d{8}T\d{6}Z-[0-9a-f]{8}")
SESSION_START_HOOK = "session-start-escalations.sh"


def delivery_key(e):
    reo = e.get("reopened_by_expiry")
    if reo:
        return "%s@reopened:%s" % (e["id"], reo.get("until") or reo.get("acked") or "")
    return e["id"]


def _state_file(state_dir, session_id):
    safe = re.sub(r"[^A-Za-z0-9-]", "", session_id or "")[:64] or "no-session"
    return os.path.join(state_dir, "%s.escalations-delivered.json" % safe)


def _load_state(path):
    try:
        with open(path, "r", encoding="utf-8") as fh:
            d = json.load(fh)
        if isinstance(d, dict):
            return d
    except Exception:
        pass
    return {}


def _save_state(path, state):
    """Atomic replace. Returns False when it cannot be written."""
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        tmp = "%s.tmp.%d" % (path, os.getpid())
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(state, fh)
        os.replace(tmp, path)
        return True
    except Exception:
        return False


def visible_context(text):
    """What the model actually received of one additionalContext string."""
    text = text or ""
    if len(text) > HOST_CONTEXT_CAP:
        return text[:HOST_PREVIEW_CHARS]
    return text


# ---------------------------------------------------------------------------
# WHAT HAS ALREADY REACHED THE LEAD'S MODEL — one answer, two readers
# ---------------------------------------------------------------------------
# 2026-10-01: a teammate's question that needed the CEO at the test iPhone sat
# 82 minutes because nothing woke the idle lead (richos-hq docs/operations/
# 2026-10-01-escalation-wakes-the-lead.md). The stall watcher now wakes it
# (stall_watch.py, ESCALATION), so TWO readers decide "has the model been told
# about this escalation?": that monitor, and the Stop-hook delivery below. They
# must answer it the same way, or the lead is told twice or not at all. This is
# that one answer. It is READ-ONLY and keeps no state of its own: each reader
# keeps its own byte offset, and only the Stop hook writes its memory file (two
# writers of one unlocked JSON file would lose each other's updates; Sage's
# review of the design, item 6c, richos-hq 8ba32b71).
#
# THE HOST'S TRANSCRIPT IS THE WITNESS. A key counts as reached when it appears
# in one of three host rows, and in nothing else:
#
#   1. the SessionStart escalation block: the `hook_success` attachment of
#      session-start-escalations.sh, only the part the host showed the model
#      (visible_context: the 10,000-character cap).
#   2. a Stop-hook delivery: a `hook_additional_context` attachment of the Stop
#      event whose item starts "TEAMMATE ESCALATION".
#   3. a monitor event from the stall watcher, in EXACTLY the host's own
#      envelope: `<task-notification>…<summary>Monitor event: …</summary>
#      <event>ESCALATION-WATCH …</event>`, as a `user` row (an idle lead woken)
#      or a `queued_command` attachment (delivered mid-turn). Measured: all 170
#      monitor events in 10 lead transcripts arrived in one of those two shapes.
#      NEVER a `queue-operation` row: the host writes the enqueue before the
#      model sees anything, and a lead blocked in AskUserQuestion has an enqueue
#      it has not read. NEVER a teammate's completion row that quotes a block:
#      its summary is "Agent … finished", not "Monitor event:" (Sage, items 1-2).
#      Only keys inside the ESCALATION-WATCH section count, read from entry heads.
#
# KEYS, NOT IDS. Everything is keyed on delivery_key(e): an escalation reopened
# by an expired ack is `<id>@reopened:<until>`, so being told about the first
# opening never counts as being told about the reopening (Sage, item 3).
WATCH_MARKER = "ESCALATION-WATCH"
MONITOR_SUMMARY = "Monitor event:"
STOP_DELIVERY_MARKER = "TEAMMATE ESCALATION"
KEY_RX = re.compile(r"\[(esc-\d{8}T\d{6}Z-[0-9a-f]{8}(?:@reopened:[^\]\s]+)?)\]")
_SUMMARY_RX = re.compile(r"<summary>(.*?)</summary>", re.S)
_EVENT_RX = re.compile(r"<event>(.*?)</event>", re.S)


def _keys_in(text):
    """Delivery keys at entry heads (`[esc-…]`), plus every bare id the text
    names. A bare id counts only for an escalation that was not reopened (see
    has_reached), which is exactly what the old id-only reading counted."""
    text = text or ""
    return set(KEY_RX.findall(text)) | set(ID_RX.findall(text))


def monitor_event_keys(text):
    """The delivery keys one host row's text carried as an ESCALATION-WATCH
    monitor event, or an empty set when it is anything else."""
    if not text or WATCH_MARKER not in text:
        return set()
    body = text.lstrip()
    if not body.startswith("<task-notification>"):
        return set()
    summary = _SUMMARY_RX.search(body)
    if not summary or not summary.group(1).strip().startswith(MONITOR_SUMMARY):
        return set()
    keys = set()
    for event in _EVENT_RX.findall(body):
        event = event.strip()
        if not event.startswith(WATCH_MARKER):
            continue
        for line in event.splitlines():
            line = line.strip()
            if line.startswith("STALL-WATCH") or line.startswith("STALL-CLEARED"):
                break                    # the stall half of the same look
            head = KEY_RX.match(line)
            if head:
                keys.add(head.group(1))
    return keys


def reached_scan(transcript, offset=0):
    """Scan the host transcript from byte `offset` for the three rows above.

    Returns ({"session_start": keys, "stop": keys, "monitor": keys}, new_offset,
    readable). Only COMPLETE lines are consumed, so a line the host is still
    writing is read on the next call. A missing transcript is readable and
    empty (no turn has been recorded yet); a transcript that exists and cannot
    be read is NOT empty, and says so through `readable` (Sage, item 6b).
    """
    found = {"session_start": set(), "stop": set(), "monitor": set()}
    if not transcript:
        return found, offset, True
    try:
        size = os.path.getsize(transcript)
    except FileNotFoundError:
        return found, offset, True
    except Exception:
        return found, offset, False
    if offset > size:
        offset = 0
    markers = (SESSION_START_HOOK.encode("utf-8"), STOP_DELIVERY_MARKER.encode("utf-8"),
               WATCH_MARKER.encode("utf-8"))
    pos = offset
    try:
        with open(transcript, "rb") as fh:
            fh.seek(offset)
            for raw in fh:
                if not raw.endswith(b"\n"):
                    break
                pos += len(raw)
                if not any(m in raw for m in markers):
                    continue
                try:
                    d = json.loads(raw.decode("utf-8", "replace"))
                except Exception:
                    continue
                if not isinstance(d, dict):
                    continue
                kind = d.get("type")
                att = d.get("attachment")
                if kind == "user":
                    msg = d.get("message")
                    content = msg.get("content") if isinstance(msg, dict) else None
                    if isinstance(content, str):
                        found["monitor"] |= monitor_event_keys(content)
                    continue
                if kind != "attachment" or not isinstance(att, dict):
                    continue            # queue-operation and everything else: never
                atype = att.get("type")
                if atype == "queued_command":
                    found["monitor"] |= monitor_event_keys(str(att.get("prompt") or ""))
                elif (atype == "hook_success" and att.get("hookEvent") == "SessionStart"
                      and SESSION_START_HOOK in str(att.get("command") or "")):
                    try:
                        out = json.loads(att.get("stdout") or "")
                        ctx = (out.get("hookSpecificOutput") or {}).get("additionalContext") or ""
                    except Exception:
                        continue
                    found["session_start"] |= _keys_in(visible_context(str(ctx)))
                elif atype == "hook_additional_context" and att.get("hookEvent") == "Stop":
                    items = att.get("content")
                    for item in (items if isinstance(items, list) else [items]):
                        if isinstance(item, str) and item.startswith(STOP_DELIVERY_MARKER):
                            found["stop"] |= _keys_in(item)
    except Exception:
        return found, offset, False
    return found, pos, True


def has_reached(e, baseline, delivered):
    """Has this outstanding escalation already reached the lead's model?

    `baseline` holds what the SessionStart block showed, `delivered` what a
    Stop delivery or a monitor event carried (sets of keys and ids). An
    escalation reopened by an expired ack has reached it only under its own
    reopened key: the first opening told the lead about a different thing."""
    key = delivery_key(e)
    if key in delivered or key in baseline:
        return True
    return e["id"] in baseline and not e.get("reopened_by_expiry")


def session_start_ids(transcript, offset=0):
    """Ids the SessionStart escalation block put in front of the model, and the
    new offset. Kept for its callers; the reading is reached_scan's."""
    found, pos, _readable = reached_scan(transcript, offset)
    return found["session_start"], pos


def render_delivery(new, already):
    """The lead's paragraph. Returns (text, keys actually included)."""
    head = ("TEAMMATE ESCALATION — %d NEW since you were last told, raised by a teammate and "
            "not acknowledged. It reaches you as Stop hook feedback: your turn continues so you "
            "can act on it. Nothing is blocked, and nothing has to be answered before you stop."
            % len(new))
    lines = [head]
    sent = []
    used = len(head)
    skipped = 0
    for e in new:
        line = model_entry(e)
        reo = e.get("reopened_by_expiry")
        if reo:
            line += (" REOPENED: the ack of %s held only until %s — \"%s\""
                     % (reo.get("acked") or "?", reo.get("until") or "?",
                        reo.get("disposition") or ""))
        if e.get("worktree"):
            line += " WORKTREE: %s (%s)" % (e["worktree"], e.get("branch") or "no branch")
        if sent and used + len(line) + 1 > DELIVERY_BUDGET:
            skipped += 1
            continue
        if not sent and used + len(line) + 1 > DELIVERY_BUDGET - FOOTER_RESERVE:
            # The first entry is always sent so delivery makes progress, but
            # past the host cap the model would see a 2,000-character preview
            # and this id would still be recorded as delivered (P5-37). Cut it
            # HERE, under the cap, and say so and where the rest is.
            marker = (" ... [TRUNCATED: the rest of this escalation did not fit; read it in "
                      "full with escalate.sh show %s]" % e["id"])
            room = DELIVERY_BUDGET - FOOTER_RESERVE - used - 1 - len(marker)
            line = line[:max(room, 0)] + marker
        lines.append(line)
        used += len(line) + 1
        sent.append(e)
    if any(e.get("for") == "ceo" for e in sent):
        lines.append("  At least one is addressed to the CEO. Routing it to him or deciding it "
                     "yourself are both answers; leaving it is not.")
    if not any(e.get("state") == "stopped" for e in sent):
        lines.append("  None of these is a stall: each teammate finished or is still working.")
    tail = []
    if skipped:
        tail.append("%d more NEW escalation(s) did not fit and arrive at your next turn end"
                    % skipped)
    if already:
        tail.append("%d other outstanding escalation(s) were already delivered this session and "
                    "are not repeated" % already)
    if tail:
        lines.append("  " + "; ".join(tail) + ". escalate.sh list has all of them.")
    lines.append("  Close each one you have dealt with: escalate.sh ack <id> --disposition "
                 "\"<what you decided or did>\".")
    return "\n".join(lines), [delivery_key(e) for e in sent]


def deliver(payload, state_dir, rows, now=None):
    """(additionalContext text or "", note for the operator or "").

    Never raises on a bad payload: a payload it cannot read has no session id
    and no transcript, which is the one-bucket, deliver-everything-once path.
    """
    now = now or utcnow()
    if not isinstance(payload, dict):
        payload = {}
    if payload.get("agent_id"):
        # A worker's own turn. Escalations are for the lead; a teammate told
        # about every other teammate's escalation is noise it cannot act on.
        return "", ""
    sid = str(payload.get("session_id") or "")
    transcript = str(payload.get("transcript_path") or "")
    path = _state_file(state_dir, sid)
    state = _load_state(path)
    if state.get("transcript") != transcript:
        state = {"transcript": transcript, "offset": 0, "baseline": [], "delivered": []}
    found, offset, _readable = reached_scan(transcript, int(state.get("offset") or 0))
    baseline = set(state.get("baseline") or []) | found["session_start"]
    # What a monitor event (stall_watch.py's ESCALATION-WATCH) or an earlier
    # Stop delivery already put in front of the model is delivered: a wake the
    # monitor caused is not repeated at the end of the turn it started. An
    # unreadable transcript adds nothing here, which is the noisy direction:
    # the escalation is delivered (again), never lost.
    delivered = set(state.get("delivered") or []) | found["monitor"] | found["stop"]

    live = outstanding(rows, now)
    new = [e for e in live if not has_reached(e, baseline, delivered)]
    already = len(live) - len(new)
    # NEWEST FIRST. Two kinds of "new" share this list: an escalation raised
    # during this session, and an older one the SessionStart block raised but
    # the host cut from the model's preview. Measured on the lead's own session
    # 356e9552: its block was 12,547 characters, so the model got a 2,000-
    # character preview naming 2 of the 8. When both kinds exist and the budget
    # cannot hold them all, the one a teammate raised minutes ago goes first; the
    # older overflow follows at the next turn end.
    new.sort(key=lambda e: str(e.get("raised") or ""), reverse=True)

    text, sent = ("", [])
    if new:
        text, sent = render_delivery(new, already)
    state.update({
        "session_id": sid,
        "transcript": transcript,
        "offset": offset,
        "baseline": sorted(baseline),
        "delivered": sorted(delivered | set(sent)),
        "updated": iso(now),
    })
    note = ""
    if not _save_state(path, state) and sent:
        note = ("the delivery memory at %s could not be written, so these escalations will be "
                "delivered to the lead again at the next turn end" % path)
    return text, note


def merge_stop_output(operator_json, context, note):
    """One JSON object for the Stop hook: the operator's line plus the model's."""
    out = {}
    if operator_json:
        try:
            d = json.loads(operator_json)
            if isinstance(d, dict):
                out = d
        except Exception:
            out = {}
    if note:
        msg = out.get("systemMessage") or ""
        out["systemMessage"] = (msg + " | " if msg else "") + "ESCALATION DELIVERY: " + note + "."
        out.setdefault("suppressOutput", True)
    if context:
        out["hookSpecificOutput"] = {"hookEventName": "Stop", "additionalContext": context}
    if not out:
        return ""
    return json.dumps(out, ensure_ascii=False)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------
def cmd_raise(args):
    problems = validate_raise(args)
    if problems:
        for p in problems:
            sys.stderr.write("escalations: %s\n" % p)
        return 2
    row = build_row(args)
    try:
        path = append_row(row)
    except Exception as exc:
        sys.stderr.write(
            "escalations: COULD NOT WRITE THE LEDGER (%s: %s).\n"
            "  YOUR ESCALATION HAS NOT BEEN DELIVERED. Do not treat this as raised.\n"
            "  Put it verbatim in your final report and in your commit message, and say that\n"
            "  the ledger write failed — a raise that silently does nothing is the exact\n"
            "  defect this channel was built to remove.\n" % (type(exc).__name__, exc))
        return 2
    print(json.dumps({"id": row["id"], "ledger": path}))
    return 0


def cmd_ack(args):
    d = (args.disposition or "").strip()
    if len(d) < MIN_DISPOSITION:
        sys.stderr.write(
            "escalations: --disposition is %d characters; it needs at least %d. Say what you "
            "DECIDED or DID. An acknowledgement with no disposition is a dismissal wearing a "
            "ledger row.\n" % (len(d), MIN_DISPOSITION))
        return 2
    rows, bad = read_rows()
    if rows is None:
        sys.stderr.write("escalations: the ledger at %s could not be read.\n" % ledger_path())
        return 2
    live = dict((e["id"], e) for e in outstanding(rows))
    known = set(str(r.get("id")) for r in rows if r.get("event") == "Escalation")
    if args.id not in known:
        sys.stderr.write(
            "escalations: no escalation with id %r was ever raised. Run `escalate.sh list` — "
            "acknowledging an id that does not exist would record a decision about nothing.\n"
            % args.id)
        return 2
    if args.id not in live:
        # A TIMED HOLD MAY BE CLOSED FOR GOOD. A hold says "look again by this
        # date"; when the answer is already in, a permanent acknowledgement
        # replaces it, so the hold cannot expire and tell somebody about a thing
        # that is over. Only that: an id closed permanently stays closed, and a
        # hold is never renewed or replaced by another hold this way.
        now_e = utcnow().timestamp()
        acks = acks_for(rows, args.id)
        permanent = any(not isinstance(a.get("until_epoch"), (int, float)) for a in acks)
        held = any(isinstance(a.get("until_epoch"), (int, float)) and a["until_epoch"] > now_e
                   for a in acks)
        if permanent or not held or (getattr(args, "until", "") or "").strip():
            sys.stderr.write("escalations: %s is already acknowledged. Nothing appended.\n"
                             % args.id)
            return 1
    row = {
        "event": "EscalationAck",
        "id": args.id,
        "acked": iso(utcnow()),
        "disposition": d,
        "actor": _actor(),
        "session_id": (args.session or os.environ.get("CLAUDE_SESSION_ID", "") or "").strip(),
    }
    until = (getattr(args, "until", "") or "").strip()
    if until:
        # A DATE, OR NOTHING. A free-text condition ("until the CEO decides")
        # reads like an expiry and cannot be evaluated by anything, so it would
        # be the very defect this flag exists to fix wearing the fix's costume —
        # type J with a field name on it. Refusing it forces the one thing that
        # actually reopens the item: a date by which somebody looks again. The
        # condition still belongs in --disposition, where a reader sees it.
        when = parse_iso(until)
        if when is None:
            sys.stderr.write(
                "escalations: --until %r is not a date. It must be an ISO date or datetime "
                "(2026-09-17, or 2026-09-17T09:00:00Z) — a condition nothing can evaluate "
                "cannot reopen anything, and an expiry that never fires is the type-J defect "
                "this flag exists to end. Put the CONDITION in --disposition and give a date "
                "by which somebody looks again.\n" % until)
            return 2
        if when.tzinfo is None:
            when = when.replace(tzinfo=timezone.utc)
        row["until"] = iso(when)
        row["until_epoch"] = when.timestamp()
    try:
        append_row(row)
    except Exception as exc:
        sys.stderr.write("escalations: could not append the ack (%s: %s).\n"
                         % (type(exc).__name__, exc))
        return 2
    print(json.dumps({"id": args.id, "acked": row["acked"]}))
    return 0


def cmd_list(args):
    rows, bad = read_rows()
    if rows is None:
        sys.stderr.write("escalations: the ledger at %s could not be read.\n" % ledger_path())
        return 2
    if args.format == "json":
        print(json.dumps({"ledger": ledger_path(), "malformed": bad,
                          "outstanding": outstanding(rows)}, indent=1))
    elif args.format == "hook-summary":
        sys.stdout.write(render_hook_summary(rows, bad))
    elif args.format == "session-context":
        sys.stdout.write(render_session_context(rows, bad))
    else:
        print(render_text(rows, bad))
    return 1 if outstanding(rows) else 0


def cmd_deliver(args):
    """Stop-hook delivery. Payload on stdin; the operator's JSON (possibly empty)
    in ESCALATIONS_OPERATOR_JSON; the final hook JSON on stdout, or nothing.

    Exit 0 whenever it printed what the hook must print; 2 when the ledger
    could not be read, and then it prints nothing so the caller keeps its own
    operator line and says that delivery failed.
    """
    raw = sys.stdin.read()
    try:
        payload = json.loads(raw) if raw.strip() else {}
    except Exception:
        payload = {}
    rows, _bad = read_rows()
    if rows is None:
        sys.stderr.write("escalations: the ledger at %s could not be read.\n" % ledger_path())
        return 2
    context, note = deliver(payload, args.state_dir, rows)
    out = merge_stop_output(os.environ.get("ESCALATIONS_OPERATOR_JSON", ""), context, note)
    if out:
        print(out)
    return 0


def cmd_show(args):
    rows, bad = read_rows()
    if rows is None:
        sys.stderr.write("escalations: the ledger at %s could not be read.\n" % ledger_path())
        return 2
    hits = [r for r in rows if r.get("event") == "Escalation" and str(r.get("id")) == args.id]
    if not hits:
        sys.stderr.write("escalations: no escalation with id %r.\n" % args.id)
        return 2
    print(json.dumps(hits[-1], indent=1, ensure_ascii=False))
    for a in acks_for(rows, args.id):
        print("ACK %s by %s: %s"
              % (a.get("acked", ""), a.get("actor", ""), a.get("disposition", "")))
    return 0


# ---------------------------------------------------------------------------
# THE FIELDS FILE: free text that never crosses the command line
# ---------------------------------------------------------------------------
# Measured 2026-09-28 on Claude Code 2.1.283. For a worktree-isolated agent,
# Claude Code refuses a Bash call before it runs when a program it does not know
# (escalate.sh is one) receives a literal argument that (a) has several words and
# starts with the word `git` or a shell name, or (b) contains a quote, `$`,
# backtick or backslash anywhere and names `git` or a shell as any word. Ordinary
# escalation prose does both: "git status, git add, git commit in the workspace",
# "we can't run git here". The refusal comes from the host, not from this engine,
# and it arrives before escalate.sh runs, so nothing here can intercept it.
#
# So the text travels in a file the agent writes with its Write tool, which that
# check never reads, and the command line carries only a path. This reads it and
# refuses anything it would have to guess about. `fields` prints the names that
# are present; `fields --get NAME` prints one value followed by a single "." so
# the caller keeps trailing newlines intact. escalate.sh assigns each value
# through a fixed case statement: nothing from the file is ever evaluated.
FIELD_NAMES = ("title", "state", "for", "question", "tried", "meanwhile",
               "teammate", "worktree", "disposition", "until", "needs")
FIELDS_MAX_BYTES = 65536


def _load_fields(path, given):
    """(dict, None) for a usable fields file, or (None, [problem, ...])."""
    try:
        with open(path, "rb") as f:
            raw = f.read(FIELDS_MAX_BYTES + 1)
    except OSError as exc:
        return None, ["cannot read the fields file %s (%s)." % (path, exc.strerror or exc)]
    if len(raw) > FIELDS_MAX_BYTES:
        return None, ["the fields file %s is over %d bytes. An escalation is a question, not a "
                      "log; put long evidence in a file and name it in 'tried'." % (path, FIELDS_MAX_BYTES)]
    try:
        data = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, ValueError) as exc:
        return None, ["the fields file %s is not a JSON object (%s).\n"
                      "  Write it with your Write tool, for example:\n"
                      '  {"title": "...", "state": "proceeding", "question": "...", "tried": "...", '
                      '"meanwhile": "..."}' % (path, exc)]
    if not isinstance(data, dict):
        return None, ["the fields file %s must hold one JSON object, not a %s."
                      % (path, type(data).__name__)]
    problems = []
    for key in sorted(data):
        if key not in FIELD_NAMES:
            problems.append("unknown field %r in %s. Allowed: %s."
                            % (key, path, ", ".join(FIELD_NAMES)))
        elif not isinstance(data[key], str):
            problems.append("field %r in %s must be a string, not %s."
                            % (key, path, type(data[key]).__name__))
        elif "\x00" in data[key]:
            problems.append("field %r in %s contains a NUL character, which a shell "
                            "variable cannot hold." % (key, path))
        elif key in given:
            problems.append("field %r is given both in %s and as --%s; give it once."
                            % (key, path, key))
    return (None, problems) if problems else (data, None)


def cmd_fields(args):
    data, problems = _load_fields(args.file, set((args.given or "").split()))
    if problems:
        for p in problems:
            sys.stderr.write("escalations: %s\n" % p)
        return 2
    if args.get:
        if args.get not in data:
            sys.stderr.write("escalations: field %r is not in %s.\n" % (args.get, args.file))
            return 2
        sys.stdout.write(data[args.get] + ".")
        return 0
    print(" ".join(k for k in FIELD_NAMES if k in data))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(prog="escalations.py")
    sub = ap.add_subparsers(dest="cmd")

    fl = sub.add_parser("fields")
    fl.add_argument("--file", required=True)
    fl.add_argument("--given", default="",
                    help="space-separated field names already given as options")
    fl.add_argument("--get", default="", help="print this one field's value, then a '.'")
    fl.set_defaults(func=cmd_fields)

    r = sub.add_parser("raise")
    r.add_argument("--title", default="")
    r.add_argument("--state", default="")
    r.add_argument("--for", dest="audience", default="lead")
    r.add_argument("--question", default="")
    r.add_argument("--tried", default="")
    r.add_argument("--meanwhile", default="")
    r.add_argument("--teammate", default="")
    r.add_argument("--worktree", default="")
    r.add_argument("--branch", default="")
    r.add_argument("--repo", default="")
    r.add_argument("--head", default="")
    r.add_argument("--record", default="")
    r.add_argument("--session", default="")
    r.add_argument("--needs", default="",
                   help="ceo-hands when the answer needs the CEO physically at a device")
    r.set_defaults(func=cmd_raise)

    a = sub.add_parser("ack")
    a.add_argument("--id", required=True)
    a.add_argument("--disposition", default="")
    a.add_argument("--session", default="")
    a.add_argument("--until", default="",
                   help="ISO date or datetime after which this acknowledgement "
                        "EXPIRES and the escalation reopens. For a hold whose "
                        "reason has a shelf life.")
    a.set_defaults(func=cmd_ack)

    l = sub.add_parser("list")
    l.add_argument("--format", default="text",
                   choices=("text", "json", "hook-summary", "session-context"))
    l.set_defaults(func=cmd_list)

    dv = sub.add_parser("deliver")
    dv.add_argument("--state-dir", required=True,
                    help="where the per-session delivery memory lives")
    dv.set_defaults(func=cmd_deliver)

    s = sub.add_parser("show")
    s.add_argument("--id", required=True)
    s.set_defaults(func=cmd_show)

    args = ap.parse_args(argv)
    if not getattr(args, "func", None):
        ap.print_help()
        return 2
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
