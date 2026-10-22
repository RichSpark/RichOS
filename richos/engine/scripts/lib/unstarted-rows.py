#!/usr/bin/env python3
"""unstarted-rows.py — THE PREDICATE: is anything unblocked with nothing running for it?

Read scripts/lib/unstarted-rows.sh first; it carries the argument. This file
carries only the mechanism, because the mechanism has to be exact.

===========================================================================
THE ONE SENTENCE
===========================================================================
    A ROW THAT NAMES NOBODY AS BLOCKING IT, AND HAS NO LIVE WORKTREE
    WORKING ON IT, IS WORK THAT WAS WRITTEN DOWN INSTEAD OF STARTED. THE
    TURN DOES NOT END QUIETLY OVER IT.

Everything here is that sentence plus the precision needed to make it fire on
the right rows and on no others.

===========================================================================
THE QUEUE IS TWO FILES, AND THAT IS THE WHOLE POINT
===========================================================================
The record this governs says so in its own header, in the lander's own words,
after the failure happened a second time:

    "THE QUEUE IS TWO FILES, NOT THIS ONE. ... An empty table here is not an
     empty queue."

A paragraph is a promise. So the two files are read as ONE corpus here, and
a half-corpus — one file present, the other missing — is BROKEN rather than a
clean sweep over whichever half survived. That is the single most important
line in this file: the defect it exists to catch is precisely "I looked at one
of them and reported an empty queue".

===========================================================================
THE TWO ROW SHAPES, AND HOW EACH ONE DECLARES A BLOCKER
===========================================================================
THE QUEUE (`RICH-TODOs.md`). A three-column table: an id, the item, and a
`Blocked by` cell. THAT CELL IS THE DECLARATION and it already exists — on the
day this was commissioned, four rows were legitimately the CEO's (a dead
Railway login, an uninstalled dictation journal, a GitHub Actions billing
block, an absent Windows toolchain) and all four already named what they were
waiting on. Nothing was invented for them; they are silent because the file
was already carrying the fact.

  nothing named   ""  "-"  "—"  "–"  "n/a"  "none"  "nobody"  "nothing"
  closed          the id cell is struck through, and the cell says "done"
  anything else   a named blocker: SILENT

THE SECTION (`wiki/open-items.md` section 3, "Buildable now — nobody blocked").
Every row there carries a warrant — a status token plus pinned object ids — and
that token is the state. But a warrant cannot say WHO a row waits on, and the
page's own contract is that an item waiting on the CEO moves to section 1 or 2.
Rows do not always move the moment they should, so there is one in-place
declaration, in a shape both files accept:

    **Blocked:** the CEO — his Railway credentials

Put it in the row's prose, BEFORE the `**State:**` warrant. It is deliberately
the same construct as the queue's `Blocked by` cell wearing markdown: one way
to say one thing. It does not disturb the row-currency contract, which reads
`**State:**` and nothing else.

WHY NOT A NEW WARRANT TOKEN. Because `.row-currency` declares the vocabulary
and rejects an unknown key or an unknown token by design, so a new token means
editing an enforced contract to add a word that means "not enforced here" —
and because the state of the WORK ("BOUNDED") and who it WAITS ON are two
different facts that must not be crammed into one slot.

===========================================================================
GREEN OVER AN EMPTY SET IS THE FAILURE THIS ENGINE KEEPS COMMITTING
===========================================================================
A scanner reporting CLEAN over an empty corpus; a runner reporting all-passed
over a suite it was not running. So every one of these is BROKEN, loudly, and
never a clean sweep:

  * either file absent, unreadable or empty
  * the queue's governed table not found by its header shape, or found twice
  * the queue table parsed to ZERO rows
  * a queue row whose id cell is empty after markdown is stripped
  * a queue row that is NOT struck through and whose cell says it is done
  * the governed section absent from the record, or holding ZERO rows
  * a section row with no warrant, or with a status token outside the
    declared vocabulary
  * one id appearing in both files, so a claim could mean either
  * scripts/lib/row-currency.py absent — the section parser lives there and
    this file will not grow a second copy of that grammar

INPUT — one JSON job, path given as argv[1] (or "-" for stdin)
    {
      "queue_label":   "RICH-TODOs.md",
      "queue_text":    "<the file>",
      "record_label":  "wiki/open-items.md",
      "record_text":   "<the file>",
      "row_sections":  ["3"],
      "status_tokens": ["OPEN","BUILT","BOUNDED","BLOCKED-ON-RICH","CLOSED"],
      "terminal_tokens":  ["CLOSED"],
      "actionable_tokens":["OPEN","BUILT","BOUNDED"],
      "claims": {"worktrees": [{"where": "...", "hay": "branch + dirname",
                                "ids": ["11", "3.1"]}]}
    }

OUTPUT — tab-separated lines on stdout. First line is the verdict:

    SWEPT       <swept> <unstarted> <claimed> <declared> <closed> <other>
    UNSTARTED   <count> <swept> <claimed> <declared> <closed> <other>
    BROKEN      <reason>

then one line per row, in document order:

    ROW  <state>  <id>  <where>  <detail>

where <state> is UNSTARTED | CLAIMED | DECLARED | CLOSED | OTHER.

EXIT  0 always, unless the job itself is unreadable (2). The VERDICT is the
      product: a non-zero exit would make "there is unstarted work" and "the
      checker is broken" the same signal to every caller.
"""

import importlib.util
import json
import os
import re
import sys

# --- Grammar ---------------------------------------------------------------
TABLE_RULE_RE = re.compile(r"^\|[\s:|-]*\|\s*$")
H2_RE = re.compile(r"^##\s+\S")
STRIKE_RE = re.compile(r"^~~(?P<inner>.*)~~$")
BLOCKED_RE = re.compile(r"\*\*Blocked:\*\*\s*(?P<who>[^|]*)")
WARRANT_MARK = "**State:**"
STATUS_RE = re.compile(r"^\s*`(?P<tok>[A-Z][A-Z0-9-]*)`")

# The queue's governed table is found by the SHAPE OF ITS HEADER, never by a
# heading name or a position in the file. A heading is prose and gets rewritten;
# the column that says what a row is waiting on is the contract.
BLOCKED_HEADER_RE = re.compile(r"^blocked", re.I)

# What a `Blocked by` cell says when it names nobody. An em dash is what the
# record actually uses; the rest are the spellings a human reaches for.
NOTHING_NAMED = {"", "-", "--", "—", "–", "n/a", "na", "none", "nobody", "nothing", "no one"}

# THE STRIKE-THROUGH IS THE CLOSURE SIGNAL, AND IT IS THE ONLY ONE.
#
# The first draft treated the cell saying "done" as a second, corroborating
# signal and refused any row where the two disagreed. Running it against the
# live record killed that in under an hour: row 11 landed struck through with
# `**CEO — a product decision**` in its blocked-by cell, and it was RIGHT.
# The work was measured and finished; what the cell names is who owns the
# RESIDUAL. Strike-through and blocked-by are not two ways of saying one thing.
#
# The reverse direction is still a genuine ambiguity and is still refused: a
# row NOT struck through whose cell says it is done claims to be finished in
# one place while being open in the other, and that is the dangerous way round.
#
# Only the first word is read, because the record qualifies its own closures —
# one live row reads "done, but unrun", which is a true and useful sentence.
DONE_WORDS = {"done", "closed", "landed", "finished", "shipped"}


def names_nobody(who):
    """True when a `**Blocked:**` declaration names NOBODY — "nothing",
    "nobody", "none", a dash — judged by its FIRST word, the way the queue's
    `Blocked by` cell already is.

    Found 2026-09-02: rows 3.19 and 3.20 of the real record each read
    `**Blocked:** nothing — buildable now, nobody blocked.` and were SILENT,
    because the construct's presence was taken as a declaration without
    reading what it declared. A row that says in so many words that nothing
    blocks it is the most unstarted row on the page, and it was the one the
    sweep could not see. The first word decides, not the whole string,
    because the record's own style is `nothing — <why it is buildable>`."""
    norm = strip_markdown(who or "").strip().lower()
    if not norm:
        return True
    first = re.split(r"[\s,;.:]+", norm, 1)[0]
    return first in NOTHING_NAMED or norm in NOTHING_NAMED


# ONLY THE CEO QUIETS A ROW (2026-10-22, richos-hq docs/plans/2026-10-22-no-
# silent-holds.md). A blocker that names Rich, or work that must come first,
# is a hold on his work like any other, and 36 of 82 rows sat quiet behind one
# while he asked "WHEN WILL [THIS] STOP ONCE AND FOR ALL???". Such a row is
# swept as unstarted. A blocker naming the CEO (his decision, on CEO-TODOs)
# still declares it.
# OWNED BY HIM, NOT MENTIONING HIM (second review rv-20261022T023129Z-cefd17cf-7c77, finding
# 2): the blocker opens with the CEO as its owner ("CEO — his Railway credentials", "the CEO —
# he has to decide"). "Rich needs to prepare the CEO brief" waits on Rich, and stays loud.
CEO_RE = re.compile(r"(?:the\s+)?CEO\b", re.I)


def names_ceo(who):
    return bool(CEO_RE.match(strip_markdown(who or "").strip()))

_HERE = os.path.dirname(os.path.abspath(__file__))


def fail(reason):
    sys.stdout.write("BROKEN\t%s\n" % reason)
    sys.exit(0)


def load_row_currency():
    """The section parser, borrowed rather than re-implemented.

    TWO PARSERS OF ONE GRAMMAR IS THE DEFECT THIS ENGINE KEEPS FINDING IN
    ITSELF — it is written in row-currency.py's own header. Section 3's row
    shape and its warrant are already parsed there, by the code the landing
    guard runs, so they are parsed there for this too. Its absence is BROKEN,
    never a fallback: a private copy that drifts is worse than a refusal.
    """
    p = os.path.join(_HERE, "row-currency.py")
    if not os.path.isfile(p):
        fail("scripts/lib/row-currency.py is missing at %s. The record's row "
             "grammar is defined there and this predicate refuses to keep a "
             "second copy of it — a divergent parser reports a clean sweep "
             "over rows it has misread." % p)
    try:
        spec = importlib.util.spec_from_file_location("row_currency", p)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
    except Exception as exc:
        fail("scripts/lib/row-currency.py could not be loaded (%s), so the "
             "record's rows cannot be parsed at all." % exc)
    return mod


def strip_markdown(s):
    s = re.sub(r"<!--.*?-->", "", s, flags=re.S)
    s = s.replace("~~", "").replace("**", "").replace("`", "")
    s = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", s)
    return s.strip()


def split_cells(line):
    """The cells of a markdown table row.

    Leading and trailing pipes are the fence, not separators. The FIRST and
    LAST cells are what this predicate reads, and the middle is re-joined, so a
    literal pipe inside the prose cell cannot shift the id or the blocker into
    the wrong column.
    """
    s = line.strip()
    if s.startswith("|"):
        s = s[1:]
    if s.endswith("|"):
        s = s[:-1]
    return [c.strip() for c in s.split("|")]


def title_of(text, limit=96):
    t = strip_markdown(text)
    t = re.sub(r"\s+", " ", t)
    return t[:limit] + ("…" if len(t) > limit else "")


# ===========================================================================
# THE QUEUE
# ===========================================================================
def parse_queue(text, label):
    """-> [row]  or fail() loudly. Never returns an empty list."""
    lines = text.split("\n")
    headers = []
    for n, line in enumerate(lines):
        if not line.lstrip().startswith("|"):
            continue
        cells = split_cells(line)
        if len(cells) >= 3 and BLOCKED_HEADER_RE.match(strip_markdown(cells[-1]).lower()):
            headers.append(n)
    if not headers:
        fail("%s holds no table whose last column is a `Blocked by` column. "
             "That column IS the contract — it is how a row says what it is "
             "waiting on — so a record without it cannot be swept, and "
             "reporting a clean sweep over it would be a lie about a file "
             "nobody read." % label)
    if len(headers) > 1:
        fail("%s holds %d tables with a `Blocked by` column (lines %s). Which "
             "one is the queue is a guess, and guessing wrong means sweeping "
             "the wrong rows while reporting the right ones."
             % (label, len(headers), ", ".join(str(h + 1) for h in headers)))

    start = headers[0]
    rows = []
    for n in range(start + 1, len(lines)):
        line = lines[n]
        if H2_RE.match(line):
            break
        if not line.lstrip().startswith("|"):
            continue
        stripped = line.strip()
        if TABLE_RULE_RE.match(stripped):
            continue
        cells = split_cells(stripped)
        if len(cells) < 3:
            fail("%s line %d is a table row with %d cell(s) where the queue's "
                 "shape is three (id, item, blocked-by). A row this parser "
                 "cannot read is a row it must not report on: %s"
                 % (label, n + 1, len(cells), stripped[:110]))
        raw_id, blocker = cells[0].strip(), cells[-1].strip()
        item = " | ".join(cells[1:-1])

        struck = bool(STRIKE_RE.match(raw_id))
        ident = strip_markdown(raw_id)
        if not ident:
            fail("%s line %d carries no id in its first cell, so nothing can "
                 "be said about it and nothing can claim it: %s"
                 % (label, n + 1, stripped[:110]))

        norm = strip_markdown(blocker).lower()
        first = re.split(r"[\s,;.]+", norm.strip(), 1)[0] if norm.strip() else ""
        is_done_cell = first in DONE_WORDS
        if is_done_cell and not struck:
            fail("%s row %s disagrees with itself: its blocked-by cell says "
                 "'%s' and its id is NOT struck through. The row claims to be "
                 "finished in one place while being open in the other, and "
                 "picking one silently is how a row gets swept as closed while "
                 "it is open."
                 % (label, ident, strip_markdown(blocker)))

        if struck:
            state = "CLOSED"
            detail = "struck through" if is_done_cell or not norm \
                else "struck through; residual: %s" % strip_markdown(blocker)
        elif names_nobody(blocker):
            state, detail = "OPEN", ""
        elif names_ceo(blocker):
            state, detail = "DECLARED", strip_markdown(blocker)
        else:
            state, detail = "OPEN", "held behind: %s" % strip_markdown(blocker)

        bm = BLOCKED_RE.search(item)
        if state == "OPEN" and bm and names_ceo(bm.group("who")):
            state, detail = "DECLARED", strip_markdown(bm.group("who"))

        rows.append({"id": ident, "where": label, "state": state,
                     "detail": detail, "title": title_of(item)})

    if not rows:
        fail("%s has a `Blocked by` header and NOT ONE ROW under it. An empty "
             "table is either a table that was emptied or a table shape that "
             "moved; both mean this sweep is about to report a clean queue "
             "having read nothing." % label)
    return rows


# ===========================================================================
# THE SECTION
# ===========================================================================
def parse_section(rc, text, label, sections, status_tokens, terminal, actionable):
    items, problems, seen = rc.parse_record(text, sections)
    if problems:
        fail("%s section %s holds %d row(s) this parser cannot identify — the "
             "first is: %s" % (label, "/".join(sections), len(problems),
                               problems[0][2][:160]))
    for s in sections:
        if s not in seen:
            fail("%s has no section %s. The governed section is where the "
                 "buildable-now rows live; if it moved or was renamed, every "
                 "row in it just became invisible and this sweep would report "
                 "a clean corpus over none of them." % (label, s))

    rows = []
    for it in items:
        if not it["governed"]:
            continue
        warrant = rc.warrant_of(it)
        if warrant is None:
            fail("%s row %s (line %d) carries no `%s` warrant, so it has no "
                 "state a machine can read. A row with no state is a row this "
                 "sweep would silently pass over."
                 % (label, it["id"], it["line0"], WARRANT_MARK))
        m = STATUS_RE.match(warrant)
        if not m:
            fail("%s row %s (line %d) has a warrant that opens with no status "
                 "token: %s" % (label, it["id"], it["line0"], warrant[:110]))
        tok = m.group("tok")
        if tok not in status_tokens:
            fail("%s row %s carries the status token `%s`, which is not in the "
                 "declared vocabulary (%s). An unrecognized token is a row "
                 "nobody can classify, and classifying it as 'fine' by default "
                 "is the whole defect."
                 % (label, it["id"], tok, " ".join(status_tokens)))

        body = rc.span_text(it)
        bm = BLOCKED_RE.search(body.split(WARRANT_MARK)[0])
        if tok in terminal:
            state, detail = "CLOSED", tok
        elif bm and names_ceo(bm.group("who")):
            state, detail = "DECLARED", strip_markdown(bm.group("who"))
        elif tok in actionable:
            state, detail = "OPEN", tok + ("; held behind: %s" % strip_markdown(bm.group("who"))
                                           if bm and not names_nobody(bm.group("who")) else "")
        else:
            state, detail = "OTHER", tok

        rows.append({"id": it["id"], "where": "%s §%s" % (label, it["section"]),
                     "state": state, "detail": detail,
                     "title": title_of(body)})

    if not rows:
        fail("%s section %s parsed to ZERO rows. That is not an empty queue, "
             "it is an unread one." % (label, "/".join(sections)))
    return rows


# ===========================================================================
# CLAIMS — is anything actually running for this row?
# ===========================================================================
def claim_pattern(rid):
    """The regex a live worktree's branch or directory must match to claim <rid>.

    BOUNDED ON BOTH SIDES, and that is the whole of it. Two near-misses this
    has to refuse, and both would buy silence with a bug:

        row-1   must NOT claim row 11        (a prefix)
        row-3.1 must NOT claim row 3         (a prefix on the other side of
                                              the dot)

    So the id is anchored: nothing alphanumeric before `row`, and nothing
    alphanumeric OR a dot after the id. A fuzzy match here fails toward
    SILENCE, which is the one direction this hook must never fail in.
    """
    base = re.escape(rid.lower())
    alts = [base]
    if "." in rid:
        alts.append(re.escape(rid.lower().replace(".", "-")))
    # A dash or underscore followed by a digit continues the id as a dashed child
    # (`row-3-1` is row 3.1), so it must not claim the parent `3` either.
    return re.compile(r"(?<![a-z0-9])row[-_]?(?:%s)(?![a-z0-9.]|[-_][0-9])" % "|".join(alts))


def claimed_by(rid, worktrees):
    """-> where the claim came from, or None.

    Two ways, and the second exists because the first cannot be retrofitted: a
    worktree whose branch was named before anybody knew which row it was for
    writes the id into <worktree>/.claude/row-claims.txt instead.
    """
    pat = claim_pattern(rid)
    for wt in worktrees:
        if rid in (wt.get("ids") or []):
            return "%s (row-claims.txt)" % wt.get("where", "a worktree")
        if pat.search(wt.get("hay") or ""):
            return wt.get("where", "a worktree")
    return None


def main():
    if len(sys.argv) < 2:
        sys.stderr.write("usage: unstarted-rows.py <job.json>|-\n")
        return 2
    try:
        raw = sys.stdin.read() if sys.argv[1] == "-" else open(sys.argv[1], encoding="utf-8").read()
        job = json.loads(raw)
    except Exception as exc:
        sys.stderr.write("unstarted-rows.py: unreadable job: %s\n" % exc)
        return 2

    qlabel = job.get("queue_label") or "the queue record"
    rlabel = job.get("record_label") or "the working record"
    qtext = job.get("queue_text")
    rtext = job.get("record_text")

    # THE HALF-CORPUS CHECK. This is the defect, in one branch.
    if qtext is None and rtext is None:
        fail("neither %s nor %s could be read. The queue is two files and "
             "this sweep found neither." % (qlabel, rlabel))
    if qtext is None:
        fail("%s could not be read, while %s could. THE QUEUE IS TWO FILES: "
             "sweeping the half that survives and reporting it clean is "
             "exactly the failure this check exists to catch." % (qlabel, rlabel))
    if rtext is None:
        fail("%s could not be read, while %s could. THE QUEUE IS TWO FILES: "
             "sweeping the half that survives and reporting it clean is "
             "exactly the failure this check exists to catch." % (rlabel, qlabel))
    if not qtext.strip():
        fail("%s is empty." % qlabel)
    if not rtext.strip():
        fail("%s is empty." % rlabel)

    rc = load_row_currency()
    sections = [str(s) for s in (job.get("row_sections") or ["3"])]
    status_tokens = list(job.get("status_tokens") or [])
    terminal = set(job.get("terminal_tokens") or ["CLOSED"])
    actionable = set(job.get("actionable_tokens") or ["OPEN", "BUILT", "BOUNDED"])
    if not status_tokens:
        fail("the job declares no status vocabulary, so no row's state can be "
             "checked against anything.")
    unknown = (actionable | terminal) - set(status_tokens)
    if unknown:
        fail("the job declares %s as actionable or terminal, and those are not "
             "in the record's own vocabulary (%s). A contract that names a "
             "token the record cannot carry governs nothing."
             % (", ".join(sorted(unknown)), " ".join(status_tokens)))

    rows = parse_queue(qtext, qlabel)
    rows += parse_section(rc, rtext, rlabel, sections, status_tokens, terminal, actionable)

    seen_ids = {}
    for r in rows:
        if r["id"] in seen_ids:
            fail("the id '%s' appears in both %s and %s. Ids are how a "
                 "worktree claims a row, so a duplicate makes every claim "
                 "ambiguous — and an ambiguous claim resolved in the quiet "
                 "direction is a row nobody is working on, reported as "
                 "covered." % (r["id"], seen_ids[r["id"]], r["where"]))
        seen_ids[r["id"]] = r["where"]

    worktrees = (job.get("claims") or {}).get("worktrees") or []
    counts = {"UNSTARTED": 0, "CLAIMED": 0, "DECLARED": 0, "CLOSED": 0, "OTHER": 0}
    out = []
    unstarted = []
    for r in rows:
        if r["state"] == "OPEN":
            src = claimed_by(r["id"], worktrees)
            if src:
                state, detail = "CLAIMED", src
            else:
                state, detail = "UNSTARTED", r["detail"] or r["title"]
                unstarted.append(r["id"])
        else:
            state, detail = r["state"], r["detail"]
        counts[state] += 1
        out.append("ROW\t%s\t%s\t%s\t%s" % (state, r["id"], r["where"], detail))

    head = "%d\t%d\t%d\t%d\t%d" % (counts["CLAIMED"], counts["DECLARED"],
                                   counts["CLOSED"], counts["OTHER"], len(rows))
    if unstarted:
        sys.stdout.write("UNSTARTED\t%d\t%s\n" % (counts["UNSTARTED"], head))
    else:
        sys.stdout.write("SWEPT\t0\t%s\n" % head)
    sys.stdout.write("\n".join(out) + ("\n" if out else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
