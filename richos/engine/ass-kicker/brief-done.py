#!/usr/bin/env python3
"""brief-done.py - A SPAWN WHOSE WORK IS ALREADY ON MAIN IS REFUSED BEFORE IT STARTS,
AND SO IS ONE WHOSE EVIDENCE SAYS "NOTHING" WHILE THE COMMAND IT QUOTES SAYS OTHERWISE.

===========================================================================
THE FAILURE, MEASURED (2026-09-28)
===========================================================================
The CEO: "WHY IS MY FUCKING TIME AND TOKENS ARE BEING WASTED LIKE THAT???"

The lead briefed andy-opus-dfix1 to fix Android defects D02, D03 and D04. All three had
been fixed and landed on 2026-09-24 (D02 at 84392f3d 2f3b790b c859f75e 8911a451, D03 at
b4e2835d af962783 9e207984, D04 at c91c6880 279bf32d 1f079f44), and
`git -C /Users/alex/ab/richos log --oneline main --grep=D02` finds them in a tenth of a
second. The spawn cost about 75 minutes of agent time and 389k tokens to find that out.

The brief carried its own evidence: "`git -C /Users/alex/ab/richos log --oneline main
--since=2026-09-24 -- richos/mobile/native-android` shows no fix". That command, re-run on
2026-09-28, prints 40 lines, and all ten fix commits are among them. The lead had run a
DIFFERENT command (`-- mobile/native-android`, from the repository root: nothing), and then
pasted the correct command beside the wrong outcome without running it.

So there are two checks, and each is a question with a factual answer:

  CHECK 1, ALREADY-DONE. The brief names items (D02, I05, O3, 3.41) or a teammate the work
    was "routed to". Does the main branch already carry commits that name them and change
    more than records? One `git log -F --grep` per target repository answers it.

  CHECK 2, CONTRADICTED. The brief quotes a read-only command as evidence and says it shows
    nothing ("shows no fix", "returns nothing", "is empty"). What does the command print
    NOW? It is run, read-only and time-limited, and if it prints lines the claim is refused
    with those lines in front of the lead. Separately, at spawn (`annotate`), every quoted
    evidence command is run once and its real line count and first lines are appended to
    the brief, so the teammate reads the output beside the claim instead of the claim alone.

===========================================================================
WHY THIS REFUSES, WHEN brief-provenance.py NEXT DOOR DELIBERATELY NEVER DOES
===========================================================================
brief-provenance.py judges prose - whether a statement carries a source - and its measured
false-positive rate would kill a blocking guard. This file reads prose only to find NAMES
(item IDs, a teammate, a quoted command) and then asks git or the command itself. What is
left to judgment is whether a found commit is the same item, and that judgment goes to the
lead WITH THE COMMITS PRINTED, which is the cheapest place it can be made.

===========================================================================
MENTION VERSUS FIX - decided by what the commit CHANGED, not by what it says
===========================================================================
An ID in a commit is not always a fix: "D02 found" in a QA report commit, a triage record,
a brief filed into the record. Telling those apart by the words of the message is a verb
list, and a verb list is one spelling of the thing (brief-provenance.py check 5). So the
discriminator is structural: a commit whose changes (a merge: against its first parent)
touch ONLY records - a docs/, wiki/, verification/, qa-audits/, ui-ux-signoffs/ or records/
directory, or a .md/.txt file - is a MENTION and is not counted. Anything that changed code,
tests or configuration is WORK.

What stays ambiguous - a failing-test commit, an ID reused by a later round or another
platform (iOS D03 is not Android D03) - is printed with its date and subject for the lead.
A brief that CITES one of an item's work commits by SHA has shown it read that history, and
that item is not refused: this is for the lead who did not know, not for one writing about it.

===========================================================================
MEASURED BEFORE IT WAS ALLOWED TO REFUSE ANYTHING
===========================================================================
Every Agent spawn in this project's transcripts from 2026-09-18 to 2026-09-28 - 313 briefs,
46 naming items - was judged against each repository's history AS OF its own spawn time
(`git log --until`), so work a brief caused was never counted against it. The first draft
refused 22 of them, and exactly one was right (andy-opus-dfix1). Short IDs are labels in
somebody's list and every list restarts at 1. Each rule below was added for a measured class
and is pinned by a case in tests/brief-done.test.sh:

  subject only        a body names every item it touched in passing   (22 -> 8 with the next two)
  verifiers exempt    QA_TOOLKIT_AGENTS types are sent to re-check fixes; ALREADY-DONE never
                      applies to them (CONTRADICTED still does)
  round date          a date on the item's line, paragraph or heading is the round it belongs
                      to; an older commit cannot be its fix ("Sage D2" of 2026-09-11 is not the
                      D2 of a 2026-09-19 round)
  namespace           "Frank G2", "(UX audit G6)", "(Urban G11)": a capitalized word right before
                      the ID (or before its list) names the list; such a commit counts only if the
                      brief mentions that word                          (8 -> 6)

What is left, 6 of 313: andy-opus-dfix1 (right), zach-opus-openg1 (spawned the morning after
commits on the same Frank G-findings; plausibly right), and four the lead acknowledges in
one line - a Sage review of plan points, Android parity for an item iOS had fixed, and two
where the list's name appears only in boilerplate. The refusal prints the commits, so the
acknowledgement is an informed one.

CONTRADICTED, on the same corpus: 27 briefs quote an evidence command, 4 make a negative
claim about one, and the only one refused is andy-opus-dfix1. The other three quote commands
that fail from the repository root (a relative path, a glob), and a failure is shown to the
teammate, never refused. A target ("After your change ... returns nothing", anything under a
Completion/Acceptance/Proof/Verify heading), a quotation of someone else's claim, and a
judgment ("nothing relevant") are not claims about now and are never refused.

===========================================================================
THE ESCAPE HATCH - one live line with a reason, logged
===========================================================================
    already-done-ack: <why this work is still needed although those commits exist>

on its own line, outside a code fence; it covers both checks, because both are the same
question - is this work already done? A bare marker exempts nothing: the reason is held to
guard-model-ceiling.sh's floor (30 characters, 5 words, 3 that carry content). An accepted
line on a LIVE spawn is appended to <project>/.claude/state/already-done-acks.log. spawn.sh's
dry evaluations (the envelope carries `richos_spawn_check` and no tool_use_id) do not log, so
one spawn is one row.

===========================================================================
WHAT IS RUN, AND WHAT NEVER IS
===========================================================================
Only these, with no shell, stdin closed, pager off, 5 s per command:
  git [-C <dir>] [--no-pager] log|grep|ls-files|rev-list|show   (never -c, --output, -O,
      --open-files-in-pager, --ext-diff, --textconv, --exec; --no-ext-diff --no-textconv
      are added)
  ls, grep/egrep/fgrep, rg (never --pre)
  `cd <dir> && ...` in front, and `| wc|head|tail|grep|sort|cut` behind (sort never -o).
Anything else - a redirect, `$`, a `<placeholder>`, `;`, `||`, a program not listed - is
not run and not judged.

===========================================================================
COST - it runs on every spawn, three times through spawn.sh
===========================================================================
A brief that names no item and quotes no negative evidence runs no subprocess at all.
Otherwise: one `git log -F --grep` per target repository and one run per negative-claim
command. Measured 2026-09-28, guard-brief-scope.sh before and after, median of 11, real
payloads against richos's 5,792 commits: a clean real brief +0.010 s; andy-opus-dfix1
(refused) +0.075 s; items named with their commits cited +0.059 s. `annotate` runs once per
spawn.sh call: 0.013 s on andy-opus-dfix1, 0.0001 s on a brief with no evidence command.

WHAT THIS CANNOT DO, stated here so it is never read as coverage:
  - It cannot see an item the brief describes without naming ("fix the microphone card").
  - It cannot see a fix whose commits never name the item on the main branch.
  - It cannot tell that a reused ID is a different defect; it prints and asks.
  - Check 2 reads only NEGATIVE claims. "returns 3 commits" beside a command that returns
    40 is not refused; the annotation puts the 40 in front of the teammate.
  - A command meant for the teammate's own workspace runs here in the repository's main
    checkout. If it fails here, the annotation says FAILED and nothing is refused.
"""
import json
import os
import re
import shlex
import subprocess
import sys
import time

# ---------------------------------------------------------------------------
# 1. WHAT THE BRIEF NAMES
# ---------------------------------------------------------------------------

# One or two capitals and one to three digits (D02, I05, O3), or a dotted number (3.41).
# Both are far too common to take bare - "T3 Code", "M1", "v2.2" - so an ID counts only
# where the brief FRAMES it as an item of work.
LETTER_ID = r"[A-Z]{1,2}\d{1,3}"
NUM_ID = r"\d{1,3}\.\d{1,3}"
ANY_ID = r"(?:%s|%s)" % (LETTER_ID, NUM_ID)
SEP = (r"\s*(?:,\s*(?:and\s+|or\s+)?|\band\b|\bor\b|&|/|\+|-|–|—|\bthrough\b|"
       r"\bto\b)\s*")
ID_LIST = r"(?:%s)(?:%s(?:%s))*" % (ANY_ID, SEP, ANY_ID)

ITEM_NOUN = (r"(?:defects?|items?|issues?|bugs?|findings?|rows?|TODOs?|tasks?|gaps?|"
             r"tickets?|regressions?)")
WORK_VERB = (r"(?:fix(?:es|ed|ing)?|clos(?:e|es|ing)|resolv(?:e|es|ing)|repair(?:s|ing)?|"
             r"address(?:es|ing)?|implement(?:s|ing)?|finish(?:es|ing)?|redo|re-?fix)")
# "fix the three open Android defects D02, D03 and D04", "items 3.41 and 3.42", "TODO 2.2"
FRAMED = re.compile(r"\b(?:%s|%s)\b[ \t]+(?:[\w'-]+[ \t]+){0,3}?(%s)(?!\w|\.\d)"
                    % (ITEM_NOUN, WORK_VERB, ID_LIST), re.I)
# "- D02: <path>", "**D03** - the card": a line that starts with the ID lists an item.
LEADING = re.compile(r"^\s*(?:[-*+]\s+|\d+\.\s+)?(?:\*\*)?(%s)(?:\*\*)?\s*(?::|—|–|-\s)"
                     % LETTER_ID)
# "(D03)", "(D03, D04)": the commit-subject convention, and briefs copy it.
PAREN = re.compile(r"\((%s)\)" % ID_LIST)
LETTER_ONLY = re.compile(r"^%s$" % LETTER_ID)
NUM_ONLY = re.compile(r"^%s$" % NUM_ID)

TEAMMATE = r"[a-z]+-(?:fable|opus|sonnet|haiku)-(?:(?:low|medium|high|xhigh|max)-)?[a-z0-9]+"
# A continuity record said "routed to andy-opus-d02". A name inside quotation marks is
# somebody's record being reported, not this brief's claim, and it is skipped.
ROUTED = re.compile(r"\b(?:routed|assigned|handed|dispatched|delegated)\s+(?:over\s+)?to\s+"
                    r"`?((?:cc/)?%s)`?" % TEAMMATE, re.I)

# A protocol line the spawn machinery reads. `ceo-todos-deferred: ... TODO 2.2 stays
# pending` is about work that is NOT being done.
MARKER = re.compile(r"^\s*[a-z][a-z0-9-]*:\s")
ACK = re.compile(r"^\s*already-done-ack:[ \t]*(\S.*?)\s*$")
SHA = re.compile(r"(?<![0-9a-zA-Z])[0-9a-f]{7,40}(?![0-9a-zA-Z])")

# Sections the spawn path APPENDS. They quote the brief, and this file's own section
# carries command output; neither is the lead's brief.
EVIDENCE_HEAD = "## Evidence commands in this brief, run at spawn"
GENERATED_HEADS = (EVIDENCE_HEAD,
                   "## Provenance of this brief — generated, not written by the lead")


def brief_only(text):
    cut = len(text)
    for head in GENERATED_HEADS:
        i = text.find(head)
        if i != -1:
            cut = min(cut, i)
    return text[:cut]


def _prose_lines(text):
    """Lines outside code fences, with protocol lines dropped."""
    fenced = False
    for raw in text.splitlines():
        s = raw.lstrip()
        if s.startswith("```") or s.startswith("~~~"):
            fenced = not fenced
            continue
        if fenced or MARKER.match(raw):
            continue
        yield raw


def _expand(listing):
    """'D02, D03 and D04' -> [D02, D03, D04]; 'D02-D04' -> [D02, D03, D04]."""
    ids, ranged = [], []
    for tok, _rng in re.findall(r"(%s)|(–|—|-|\bthrough\b|\bto\b)" % ANY_ID, listing):
        if tok:
            ids.append(tok)
            ranged.append(False)
        elif ids:
            ranged[-1] = True
    out = []
    for i, tok in enumerate(ids):
        out.append(tok)
        if i + 1 < len(ids) and ranged[i]:
            a = re.match(r"^([A-Z]{1,2})(\d{1,3})$", tok)
            b = re.match(r"^([A-Z]{1,2})(\d{1,3})$", ids[i + 1])
            if a and b and a.group(1) == b.group(1):
                lo, hi = int(a.group(2)), int(b.group(2))
                if 0 < hi - lo <= 20:
                    out += ["%s%0*d" % (a.group(1), len(a.group(2)), n)
                            for n in range(lo + 1, hi)]
    return out


ISO_DATE = re.compile(r"(?<!\d)(20\d\d-[01]\d-[0-3]\d)(?!\d)")


def item_floors(text):
    """{id: 'YYYY-MM-DD'} - the date of the round each item belongs to, where the brief
    says it: a date on the item's own line, else in its paragraph, else in the heading
    above it. An ID names an item of ONE round ("D02" of the 2026-09-24 acceptance round),
    and short IDs are reused round after round (G1-G14 meant different things on
    2026-09-10, 2026-09-19 and 2026-09-24); a commit made before the round existed cannot
    be that round's fix. The EARLIEST date found is used, so an ambiguous brief is judged
    against more history, never less."""
    floors = {}
    heading_date, para = "", []

    def dates(s):
        return sorted(ISO_DATE.findall(s))

    lines = list(_prose_lines(text))
    # paragraph index for each line
    paras, cur = [], []
    for ln in lines:
        if not ln.strip():
            if cur:
                paras.append(cur)
            cur = []
        else:
            cur.append(ln)
    if cur:
        paras.append(cur)
    for block in paras:
        if block[0].lstrip().startswith("#"):
            d = dates(block[0])
            heading_date = d[0] if d else ""
        block_dates = dates("\n".join(block))
        for ln in block:
            ids, _n = _items_on_line(ln)
            if not ids:
                continue
            d = dates(ln) or block_dates or ([heading_date] if heading_date else [])
            for i in ids:
                if d and (i not in floors or d[0] < floors[i]):
                    floors[i] = d[0]
                elif not d:
                    floors.setdefault(i, "")
    return floors


def _items_on_line(line):
    found = []
    m = LEADING.match(line)
    if m:
        found.append(m.group(1))
    for rx in (FRAMED, PAREN):
        for mm in rx.finditer(line):
            found += _expand(mm.group(1))
    return [t for t in found if LETTER_ONLY.match(t) or NUM_ONLY.match(t)], None


def named_items(text):
    """(ids, teammates) the brief frames as work, in first-seen order."""
    ids, names = [], []
    for line in _prose_lines(text):
        found = []
        m = LEADING.match(line)
        if m:
            found.append(m.group(1))
        for rx in (FRAMED, PAREN):
            for mm in rx.finditer(line):
                found += _expand(mm.group(1))
        for tok in found:
            if (LETTER_ONLY.match(tok) or NUM_ONLY.match(tok)) and tok not in ids:
                ids.append(tok)
        for mm in ROUTED.finditer(line):
            before = line[:mm.start()]
            if (before.count('"') + before.count("“") + before.count("”")) % 2 == 1:
                continue
            n = mm.group(1)
            n = n[3:] if n.startswith("cc/") else n
            if n not in names:
                names.append(n)
    return ids, names


def cited_shas(text):
    return set(m.group(0).lower() for m in SHA.finditer(text))


def ack_reason(text):
    fenced = False
    for raw in text.splitlines():
        s = raw.lstrip()
        if s.startswith("```") or s.startswith("~~~"):
            fenced = not fenced
            continue
        if fenced:
            continue
        m = ACK.match(raw)
        if m:
            return m.group(1).strip()
    return ""


STOP = {"the", "and", "for", "that", "this", "with", "have", "has", "had", "been", "from",
        "just", "only", "need", "needs", "needed", "want", "wants", "because", "none", "null",
        "reason", "tbd", "todo", "fine", "okay", "yes", "not", "but", "was", "were", "are",
        "its", "here", "there", "thing", "things", "stuff", "some", "any", "all", "does",
        "will", "would", "should", "could", "which", "them", "they", "their", "when", "what",
        "also", "into", "over", "than", "then", "very", "really", "sure", "done", "ack",
        "already", "work", "still"}


def reason_problem(reason):
    """'' when the acknowledgement carries a reason, the refusal sentence otherwise. The
    floors are guard-model-ceiling.sh's, deliberately: one engine, one idea of a reason."""
    r = (reason or "").strip()
    if not r:
        return "no 'already-done-ack: <reason>' line is present in the brief."
    words = re.findall(r"[A-Za-z][A-Za-z'-]*", r)
    content = {w.lower() for w in words if len(w) >= 4 and w.lower() not in STOP}
    if len(r) < 30:
        return ("the reason given is %d character(s) long; it needs at least 30. A bare or "
                "token marker exempts nothing." % len(r))
    if len(words) < 5:
        return "the reason given is %d word(s) long; it needs at least 5." % len(words)
    if len(content) < 3:
        return ("the reason given carries %d substantive word(s) (needs 3); it reads as "
                "filler, not a reason." % len(content))
    return ""


# ---------------------------------------------------------------------------
# 2. WHICH REPOSITORIES
# ---------------------------------------------------------------------------

_XREPO = re.compile(r"^\s*cross-repo-worktree:\s*(\S+)", re.M)


def _git(repo, args, timeout=20):
    try:
        p = subprocess.run(["git", "-C", repo] + args, capture_output=True, text=True,
                           timeout=timeout, stdin=subprocess.DEVNULL)
        return p.returncode, p.stdout
    except (OSError, subprocess.SubprocessError):
        return 127, ""


def main_checkout(path):
    """The MAIN checkout of the repository `path` is in, or ''. A workspace's history
    question is its repository's."""
    if not path or not os.path.isdir(path):
        return ""
    rc, out = _git(path, ["rev-parse", "--path-format=absolute", "--git-common-dir"])
    if rc != 0 or not out.strip():
        return ""
    common = out.strip()
    if os.path.basename(common) == ".git":
        return os.path.realpath(os.path.dirname(common))
    rc, out = _git(path, ["rev-parse", "--show-toplevel"])
    return os.path.realpath(out.strip()) if rc == 0 and out.strip() else ""


def work_repos(payload):
    """Where the WORK is: the cross-repo workspaces when the brief has any - and in
    spawn.sh's dry run they are only PLANNED, so the envelope's plan is read too -
    otherwise the session's own repository."""
    ti = payload.get("tool_input") or {}
    prompt = str(ti.get("prompt") or "")
    out = []
    for p in _XREPO.findall(prompt):
        r = main_checkout(p)
        if r and r not in out:
            out.append(r)
    plan = payload.get("richos_spawn_check")
    if isinstance(plan, dict):
        for item in plan.get("planned") or []:
            r = main_checkout(str((item or {}).get("repo") or ""))
            if r and r not in out:
                out.append(r)
    if not out:
        own = main_checkout(str(payload.get("cwd") or os.environ.get("CLAUDE_PROJECT_DIR")
                                or ""))
        if own:
            out.append(own)
    return out


def main_branch(repo):
    for b in ("main", "master"):
        rc, _ = _git(repo, ["rev-parse", "--verify", "-q", "refs/heads/%s^{commit}" % b])
        if rc == 0:
            return b
    return "HEAD"


# ---------------------------------------------------------------------------
# 3. CHECK 1 - IS THE NAMED WORK ALREADY ON MAIN?
# ---------------------------------------------------------------------------

RECORD_DIRS = {"docs", "wiki", "verification", "qa-audits", "ui-ux-signoffs", "records"}
RECORD_EXT = (".md", ".txt")


def is_record(path):
    return path.endswith(RECORD_EXT) or any(p in RECORD_DIRS for p in path.split("/")[:-1])


def _term_rx(term):
    if NUM_ONLY.match(term):
        # A bare dotted number is a version, a size, a time. In a commit it counts only
        # beside the same kind of word that made it an item in the brief.
        return re.compile(r"\b%s\s*#?\s*(?<![\d.])%s(?!\.?\d)" % (ITEM_NOUN, re.escape(term)),
                          re.I)
    if LETTER_ONLY.match(term):
        return re.compile(r"(?<![A-Za-z0-9])%s(?![A-Za-z0-9])" % re.escape(term))
    return re.compile(r"(?<![a-z0-9-])%s(?![a-z0-9-])" % re.escape(term))


def commits_naming(repo, terms, until=""):
    """{term: [commit]} for commits on the main branch whose message names the term, each
    with the files it changed (a merge: against its first parent). `until` exists for
    measuring old briefs against the history they were written against."""
    if not terms:
        return {}
    args = ["log", main_branch(repo), "-F", "--no-color", "--diff-merges=first-parent",
            "--name-only", "--format=%x1e%H%x1f%cs%x1f%s%x1f"]
    if until:
        args.append("--until=%s" % until)
    args += ["--grep=%s" % t for t in terms]
    rc, out = _git(repo, args, timeout=30)
    if rc != 0:
        return {}
    rxs = {t: _term_rx(t) for t in terms}
    found = {}
    for rec in out.split("\x1e"):
        f = rec.split("\x1f")
        if len(f) < 4:
            continue
        sha, date, subject = f[0].strip(), f[1], f[2]
        files = [x for x in f[3].splitlines() if x.strip()]
        # THE SUBJECT, NOT THE BODY. A fix names its item where it is read - "fix(android):
        # ... (D04)", "Merge cc/andy-opus-d02: ... (D02)". A body names every item it
        # touched in passing; over the 313-brief corpus body matches were most of the
        # rows and none of the one true refusal.
        for t, rx in rxs.items():
            m = rx.search(subject)
            if m:
                is_id = bool(LETTER_ONLY.match(t) or NUM_ONLY.match(t))
                found.setdefault(t, []).append(
                    {"sha": sha, "date": date, "subject": subject,
                     "qualifier": qualifier(subject[:m.start()]) if is_id else "",
                     "work": bool(files) and not all(is_record(x) for x in files)})
    return found


QUALIFIER = re.compile(r"^(?:[A-Z][A-Za-z]*|iOS|macOS)$")
# Capitalized because they start a subject, not because they name a list.
NOT_A_NAMESPACE = {"merge", "revert", "fix", "fixes", "test", "tests", "add", "adds", "the",
                   "a", "an", "and", "or", "for", "in", "on", "of", "to", "with", "wip"}
LIST_TOKEN = re.compile(r"^(?:%s|%s)[,;]?$|^(?:and|or|\+|/|&)$" % (LETTER_ID, NUM_ID))


def qualifier(before):
    """The NAMESPACE a commit subject puts on an ID: a capitalized word right before it or
    before the list it is in - "Frank G2", "Frank G1, G5", "Sage D2", "(UX audit G6)",
    "(Urban G11)", "iOS D03". Short IDs are labels in somebody's list, and the list is
    named there. '' when the ID stands alone ("(D04)", "reproduce D02")."""
    words = before.rstrip().split()
    i = len(words) - 1
    while i >= 0 and LIST_TOKEN.match(words[i]):
        i -= 1                            # walk back over the rest of the list
    for w in reversed(words[max(0, i - 1):i + 1]):
        clean = w.strip("'\"(")
        if QUALIFIER.match(clean) and clean.lower() not in NOT_A_NAMESPACE:
            return clean
        if w.startswith("(") or w.endswith((",", ":", ";", ")", "(")):
            break
    return ""


def verifier_types():
    """Teammate types whose job STARTS from a fix: they verify it. Read from the one list
    this engine already declares for them (QA_TOOLKIT_AGENTS in orchestration.config,
    through qa-toolkit.py), loaded only when a brief names an item at all."""
    try:
        import importlib.util
        spec = importlib.util.spec_from_file_location(
            "richos_qa_toolkit_bd", os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                                 "..", "scripts", "lib", "qa-toolkit.py"))
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        return set(mod.agents())
    except Exception:
        return {"ray", "urban", "kai", "quint"}


def already_done(text, repos, until="", agent_type=""):
    """[(repo, term, [work commits])] for every named item main already carries work for
    and the brief does not cite.

    A VERIFIER IS NOT SENT TO DO DONE WORK. A QA brief names the items it re-checks, and
    their fix commits are its premise; over the 313-brief corpus the QA briefs were seven
    of the twenty-one false refusals."""
    ids, names = named_items(text)
    terms = ids + names
    if not terms or not repos:
        return []
    if agent_type and agent_type in verifier_types():
        return []
    floors = item_floors(text)
    cited = cited_shas(text)
    words = set(re.findall(r"[a-z]+", text.lower()))
    out = []
    for repo in repos:
        found = commits_naming(repo, terms, until)
        for t in terms:
            floor = floors.get(t, "")
            # A commit about "Frank G2" is about Frank's G2; it is this brief's G2 only if
            # the brief is talking about Frank at all.
            work = [c for c in found.get(t, []) if c["work"] and c["date"] >= floor
                    and (not c["qualifier"] or c["qualifier"].lower() in words)]
            if not work:
                continue
            if any(c["sha"].startswith(s) for c in work for s in cited):
                continue
            out.append((repo, t, work))
    return out


# ---------------------------------------------------------------------------
# 4. CHECK 2 - WHAT DOES THE QUOTED EVIDENCE ACTUALLY PRINT?
# ---------------------------------------------------------------------------

INLINE = re.compile(r"`([^`\n]+)`")
TIMEOUT = 5.0
MAX_RUNS = 8

GIT_SUBS = {"log", "grep", "ls-files", "rev-list", "show"}
GIT_DENY = re.compile(r"^(?:--output|-O|--open-files-in-pager|--ext-diff|--textconv|--exec|"
                      r"--upload-pack|--config-env)")
SOURCES = {"git", "ls", "grep", "egrep", "fgrep", "rg"}
FILTERS = {"wc", "head", "tail", "grep", "egrep", "fgrep", "sort", "cut"}
BARE = re.compile(r"[<>$`;]|\|\||\bsudo\b")

# The claim that the evidence showed NOTHING - the only kind of claim a re-run can refute
# without judging what the output means.
NEGATIVE = re.compile(
    r"\b(?:shows?|showed|returns?|returned|prints?|printed|finds?|found|lists?|listed|"
    r"gives?|gave|outputs?|yields?|yielded|reports?|reported|has|had|contains?)\s+"
    r"(?:no|nothing|none|zero|0)\b|"
    r"\b(?:no|zero|0)\s+(?:\w+\s+)?(?:fix(?:es)?|commits?|results?|hits?|matches|lines|output|"
    r"entries|files|changes|occurrences)\b|"
    r"\b(?:is|was|comes?\s+back|came\s+back|returns?|returned)\s+empty\b|"
    r"\b(?:nothing|none)\s+(?:comes?|came)\s+back\b", re.I)
# "shows nothing RELEVANT" is the lead's judgment of lines that exist, not a claim that
# none do - echo-opus-dd1, 2026-09-28, said it of a `git log --grep=import` that prints
# plenty. A re-run cannot refute a judgment, so it is annotated and not refused.
JUDGED = re.compile(r"\b(?:nothing|no|none)\s+(?:\w+\s+)?(?:relevant|useful|new|related|else|"
                    r"more|interesting|important|of\s+note|that\s+matters)\b", re.I)
# A sentence that states the TARGET - "After your change, `git grep` ... returns nothing",
# "Completion criterion: `grep ...` -> no hit" - describes what must become true. Run at
# spawn it is contradicted by construction, because the work has not been done yet.
TARGET = re.compile(r"\b(?:after|once|until|when|if|must|should|will|shall|would|needs?\s+to)\b",
                    re.I)
# The heading's FIRST word decides: "The job: ... from the 2026-09-24 physical acceptance
# round" is a job, and read anywhere in the heading that word made it a criterion.
TARGET_SECTION = re.compile(r"^\W*(?:completion|acceptance|proof|verif|deliverable|done when|"
                            r"out of scope|definition of done)", re.I)
# A claim that the command SAYS something - what makes a quoted command evidence.
RESULT = re.compile(
    r"\b(?:shows?|showed|returns?|returned|prints?|printed|finds?|found|lists?|listed|"
    r"gives?|gave|outputs?|yields?|yielded|reports?|reported|counts?|matches|matched)\b|"
    r"→|->|=>", re.I)


def parse_pipeline(cmd):
    """([(argv)], cwd) for a command this file is allowed to run, or None."""
    if BARE.search(cmd):
        return None
    stages = [s.strip() for s in cmd.split("&&")]
    cwd = ""
    while len(stages) > 1:
        try:
            toks = shlex.split(stages[0])
        except ValueError:
            return None
        if len(toks) != 2 or toks[0] != "cd":
            return None
        cwd = os.path.expanduser(toks[1])
        stages.pop(0)
    if len(stages) != 1:
        return None
    parts = []
    for i, seg in enumerate(stages[0].split("|")):
        try:
            toks = [os.path.expanduser(t) if t.startswith("~") else t
                    for t in shlex.split(seg)]
        except ValueError:
            return None
        if not toks:
            return None
        prog = toks[0]
        if i == 0:
            if prog not in SOURCES:
                return None
            if prog == "git":
                j = 1
                while j < len(toks) and toks[j].startswith("-"):
                    if toks[j] == "-C" and j + 1 < len(toks):
                        j += 2
                    elif toks[j] == "--no-pager":
                        j += 1
                    else:
                        return None
                if j >= len(toks) or toks[j] not in GIT_SUBS:
                    return None
                if any(GIT_DENY.match(t) for t in toks[j + 1:]):
                    return None
                safe = {"log": ["--no-ext-diff", "--no-textconv"],
                        "show": ["--no-ext-diff", "--no-textconv"],
                        "grep": ["--no-textconv"]}.get(toks[j], [])
                toks = toks[:j + 1] + safe + toks[j + 1:]
                toks = ["git", "--no-pager"] + [t for t in toks[1:] if t != "--no-pager"]
            if prog == "rg" and any(t.startswith("--pre") for t in toks):
                return None
        else:
            if prog not in FILTERS:
                return None
            if prog == "sort" and any(t == "-o" or t.startswith("--output") for t in toks):
                return None
        parts.append(toks)
    return parts, cwd


def run_evidence(cmd, default_cwd):
    """{'lines': [...], 'count': n, 'rc': rc, 'error': str, 'cwd': dir} or None when the
    command is not one this file runs."""
    parsed = parse_pipeline(cmd)
    if not parsed:
        return None
    parts, cwd = parsed
    cwd = cwd or default_cwd or os.getcwd()
    if not os.path.isdir(cwd):
        return {"lines": [], "count": 0, "rc": 127, "cwd": cwd,
                "error": "the directory it runs in does not exist: %s" % cwd}
    env = dict(os.environ, GIT_PAGER="cat", PAGER="cat", GIT_TERMINAL_PROMPT="0",
               GIT_OPTIONAL_LOCKS="0", LC_ALL="C")
    env.pop("GIT_EXTERNAL_DIFF", None)
    procs, prev = [], subprocess.DEVNULL
    deadline = time.time() + TIMEOUT
    try:
        for k, argv in enumerate(parts):
            p = subprocess.Popen(argv, cwd=cwd, env=env, stdin=prev, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE)
            if k:
                procs[-1].stdout.close()
            procs.append(p)
            prev = p.stdout
        out, err = procs[-1].communicate(timeout=max(0.1, deadline - time.time()))
        rc = procs[-1].returncode
        first_rc = procs[0].wait(timeout=max(0.1, deadline - time.time()))
        first_err = procs[0].stderr.read() if len(procs) > 1 else err
    except subprocess.TimeoutExpired:
        for p in procs:
            p.kill()
        return {"lines": [], "count": 0, "rc": 124, "cwd": cwd,
                "error": "it did not finish inside %d s" % TIMEOUT}
    except OSError as exc:
        for p in procs:
            p.kill()
        return {"lines": [], "count": 0, "rc": 127, "cwd": cwd, "error": str(exc)}
    finally:
        for p in procs:
            for s in (p.stdout, p.stderr):
                if s:
                    s.close()
    text = out.decode("utf-8", "replace")
    lines = [l for l in text.splitlines() if l.strip()]
    error = ""
    # grep's 1 is "no match", not a failure.
    if first_rc not in (0, 1) or rc not in (0, 1):
        error = (first_err or err).decode("utf-8", "replace").strip().splitlines()[:1]
        error = error[0] if error else "exit %d" % (first_rc or rc)
    return {"lines": lines, "count": len(lines), "rc": rc, "cwd": cwd, "error": error}


def _paragraphs(text):
    """Blocks of prose (fences excluded) with the fenced blocks between them, in order,
    each with the heading it sits under: [(kind, body, section)], body a string for
    'prose' and a list of lines for 'code'."""
    out, buf, fence, fenced, section = [], [], [], False, ""

    def flush():
        if buf:
            out.append(("prose", "\n".join(buf), section))
            del buf[:]

    for raw in text.splitlines():
        s = raw.strip()
        if s.startswith("```") or s.startswith("~~~"):
            if fenced:
                out.append(("code", fence, section))
                fence = []
            else:
                flush()
            fenced = not fenced
            continue
        if fenced:
            fence.append(re.sub(r"^\s*[$#]\s+", "", s))
        elif not s:
            flush()
        elif s.startswith("#"):
            flush()
            section = s.lstrip("#").strip()
        elif not MARKER.match(raw):
            buf.append(raw)
    flush()
    return out


SENTENCE = re.compile(r"(?<=[.!?])\s+(?=[A-Z0-9\"*(“])")
LIST_ITEM = re.compile(r"^\s*(?:[-*+]|\d+\.)\s+")


def _units(body):
    """A paragraph as the units a claim lives in: each list item on its own, other
    lines joined, then split into sentences."""
    items, cur = [], []
    for ln in body.split("\n"):
        if LIST_ITEM.match(ln) and cur:
            items.append(" ".join(cur))
            cur = []
        cur.append(ln.strip())
    if cur:
        items.append(" ".join(cur))
    out = []
    for it in items:
        out += SENTENCE.split(it)
    return out


def _quoted_at(text, pos):
    """True when `pos` is inside a quotation: somebody else's words, reported."""
    before = text[:pos]
    return (before.count('"') + before.count("“") + before.count("”")) % 2 == 1


def _is_negative(sent):
    """A claim that the command printed NOTHING, made by this brief, about now."""
    bare = re.sub(r"CMD\d+", " ", sent)
    m = NEGATIVE.search(bare)
    if not m or _quoted_at(bare, m.start()):
        return False
    return not JUDGED.search(bare) and not TARGET.search(bare)


def evidence_claims(text):
    """[(command, sentence, negative)] for every quoted command the brief presents as
    showing something: an inline command in a sentence that says what it shows, or a
    fenced command beside such a paragraph. A sentence under a completion, proof or
    verification heading states the target, not the evidence, and is never negative."""
    found = {}

    def add(cmd, sentence, neg):
        # One row per command; a negative claim anywhere about it is the one kept.
        if cmd not in found or (neg and not found[cmd][1]):
            found[cmd] = (sentence, neg)

    blocks = _paragraphs(brief_only(text))
    for i, (kind, body, section) in enumerate(blocks):
        if kind != "prose":
            continue
        target = bool(TARGET_SECTION.search(section))
        cmds = INLINE.findall(body)
        # One sentence at a time, commands blanked, so `--grep=no` is not a claim.
        spans = list(INLINE.finditer(body))
        flat = body
        for k, m in reversed(list(enumerate(spans))):
            flat = flat[:m.start()] + ("CMD%d" % k) + flat[m.end():]
        for sent in _units(flat):
            idx = [int(x) for x in re.findall(r"CMD(\d+)", sent)]
            if not idx or not RESULT.search(re.sub(r"CMD\d+", " ", sent)):
                continue
            neg = not target and _is_negative(sent)
            shown = re.sub(r"CMD(\d+)", lambda m: "`%s`" % cmds[int(m.group(1))], sent)
            for k in idx:
                c = cmds[k].strip()
                if parse_pipeline(c):
                    add(c, shown.strip(), neg)
        for j in (i - 1, i + 1):
            if 0 <= j < len(blocks) and blocks[j][0] == "code":
                flatb = INLINE.sub(" ", body).replace("\n", " ")
                if not RESULT.search(flatb):
                    continue
                neg = not target and _is_negative(flatb)
                for line in blocks[j][1]:
                    if line and parse_pipeline(line):
                        add(line, body.replace("\n", " ").strip(), neg)
    return [(c, s, n) for c, (s, n) in found.items()]


def contradicted(text, repos):
    """[(command, sentence, result)] where the brief says the command showed nothing and,
    run now, it succeeds and prints lines.

    A command that FAILS here is not refused. Briefs quote commands meant for the
    teammate's own workspace, so a failure is as likely to be "not here" as "not true";
    the annotation shows the failure to the teammate instead."""
    out = []
    default = repos[0] if repos else ""
    for cmd, sent, neg in evidence_claims(text)[:MAX_RUNS]:
        if not neg:
            continue
        res = run_evidence(cmd, default)
        if res is None or res["error"]:
            continue
        if res["count"]:
            out.append((cmd, sent, res))
    return out


def _item_first(lines, ids):
    """The output lines that name one of the brief's items first; they are the ones
    that make the contradiction obvious."""
    if not ids:
        return lines
    rx = re.compile(r"(?<![A-Za-z0-9])(?:%s)(?![A-Za-z0-9])" % "|".join(map(re.escape, ids)))
    hit = [l for l in lines if rx.search(l)]
    return hit + [l for l in lines if l not in hit]


# ---------------------------------------------------------------------------
# 5. THE ANNOTATION - the output goes in front of the teammate, beside the claim
# ---------------------------------------------------------------------------

def annotate(prompt, repos):
    """(prompt, notes). Runs each evidence command once and appends what it printed. A
    brief with no evidence command is returned byte-identical."""
    claims = evidence_claims(prompt)[:MAX_RUNS]
    if not claims:
        return prompt, []
    default = repos[0] if repos else ""
    ids, _names = named_items(brief_only(prompt))
    rows = []
    for cmd, sent, _neg in claims:
        res = run_evidence(cmd, default)
        if res is None:
            continue
        rows.append((cmd, sent, res))
    if not rows:
        return prompt, []
    stamp = time.strftime("%Y-%m-%d %H:%M UTC", time.gmtime())
    out = [EVIDENCE_HEAD, "",
           "Each command below is quoted in this brief as evidence. The spawn ran it, read-only,",
           "at %s, and this is what it printed. **Read the output, not the sentence beside it**:" % stamp,
           "the sentence is the lead's account of the command, and the output is the command.", ""]
    for cmd, sent, res in rows:
        out.append("- `%s`" % cmd)
        out.append("  the brief says: “%s”" % sent[:300])
        if res["error"]:
            out.append("  ran in %s: FAILED - %s" % (res["cwd"], res["error"][:200]))
        else:
            out.append("  ran in %s: **%d line(s)**%s" % (res["cwd"], res["count"],
                                                      ", first %d:" % min(8, res["count"])
                                                      if res["count"] else "."))
        if res["count"]:
            out.append("")
            out.append("  ```")
            for l in _item_first(res["lines"], ids)[:8]:
                out.append("  %s" % l[:160])
            out.append("  ```")
        out.append("")
    return (prompt.rstrip("\n") + "\n\n" + "\n".join(out).rstrip() + "\n",
            ["evidence: %d quoted command(s) run and their output appended" % len(rows)])


# ---------------------------------------------------------------------------
# 6. THE DECISION
# ---------------------------------------------------------------------------

def is_spawn_check(payload):
    return isinstance(payload.get("richos_spawn_check"), dict) and not str(
        payload.get("tool_use_id") or "")


def _done_lines(done):
    lines = ["ALREADY-DONE: the main branch already carries commits that change more than",
             "records and name the work this brief asks for. The brief cites none of them.", ""]
    for repo, term, work in done:
        lines.append("  %s   %d commit(s) on main in %s" % (term, len(work), repo))
        for c in work[:6]:
            lines.append("    %s %s %s" % (c["sha"][:8], c["date"], c["subject"][:100]))
        if len(work) > 6:
            lines.append("    ... and %d more:  git -C %s log --oneline main -F --grep=%s"
                         % (len(work) - 6, repo, shlex.quote(term)))
        lines.append("")
    return lines


def _contradiction_lines(bad, ids):
    lines = ["CONTRADICTED: the brief says a command it quotes shows nothing. Run now, it does",
             "not:", ""]
    for cmd, sent, res in bad:
        lines.append("  the brief:  “%s”" % sent[:240])
        if res["error"]:
            lines.append("  run now in %s: it FAILS - %s" % (res["cwd"], res["error"][:200]))
            lines.append("  A negative claim resting on a command that cannot run is not evidence.")
        else:
            lines.append("  run now in %s: %d line(s)" % (res["cwd"], res["count"]))
            for l in _item_first(res["lines"], ids)[:8]:
                lines.append("    %s" % l[:140])
            if res["count"] > 8:
                lines.append("    ... and %d more" % (res["count"] - 8))
        lines.append("")
    return lines


def decide(payload, until=""):
    """(rc, lines, record). rc 2 refuses; record is what an accepted acknowledgement logs."""
    ti = payload.get("tool_input") or {}
    prompt = str(ti.get("prompt") or "")
    brief = brief_only(prompt)
    text = brief + "\n" + str(ti.get("description") or "")
    repos = work_repos(payload)
    ids, _names = named_items(text)
    done = already_done(text, repos, until, str(ti.get("subagent_type") or ""))
    bad = contradicted(brief, repos)
    if not done and not bad:
        return 0, [], None
    body = (_done_lines(done) if done else []) + (_contradiction_lines(bad, ids) if bad else [])
    reason = ack_reason(brief)
    problem = reason_problem(reason)
    if not problem:
        record = {"name": ti.get("name") or "", "reason": reason,
                  "items": ["%s:%s" % (os.path.basename(r), t) for r, t, _w in done],
                  "commits": sorted({c["sha"][:8] for _r, _t, w in done for c in w}),
                  "contradicted": [c for c, _s, _r in bad]}
        return 0, ["already-done-ack accepted: %s" % reason] + body, record
    codes = [x for x, y in (("ALREADY-DONE", done), ("CONTRADICTED", bad)) if y]
    lines = ["guard-brief-scope REFUSES this spawn  (%s)" % ", ".join(codes), ""] + body
    lines += ["Drop the work, or correct the brief (cite the commits and say what is still",
              "wrong; state what the command really prints), or, if the work is wanted anyway,",
              "add ONE line to the brief:",
              "",
              "    already-done-ack: <why this work is still needed although these commits exist>",
              "",
              "Commits that touch only records (docs/, wiki/, verification/, *.md, *.txt) are",
              "not counted. An accepted line is logged to .claude/state/already-done-acks.log."]
    if reason:
        lines += ["", "THE LINE YOU GAVE WAS NOT ACCEPTED: %s" % problem]
    return 2, lines, None


def log_ack(payload, record):
    """Best effort: the line in the prompt is itself the audit trail."""
    project = (os.environ.get("CLAUDE_PROJECT_DIR") or str(payload.get("cwd") or "")
               or os.getcwd())
    d = os.path.join(project, ".claude", "state")
    try:
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "already-done-acks.log"), "a", encoding="utf-8") as fh:
            fh.write("%s\tsession=%s\tname=%s\titems=%s\tcommits=%s\tcontradicted=%s\t"
                     "already-done-ack: %s\n"
                     % (time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                        payload.get("session_id") or "<unset>", record["name"] or "<unset>",
                        ",".join(record["items"]) or "-", ",".join(record["commits"]) or "-",
                        " | ".join(record["contradicted"]) or "-", record["reason"]))
    except OSError:
        pass


def check_payload(payload):
    """The hook's entry: (rc, lines). Logs an accepted acknowledgement on a live call only."""
    if (payload.get("tool_name") or "") != "Agent":
        return 0, []
    rc, lines, record = decide(payload)
    if record is not None and not is_spawn_check(payload):
        log_ack(payload, record)
    return rc, lines


def main(argv):
    if len(argv) >= 3 and argv[1] == "check":
        with open(argv[2], encoding="utf-8", errors="replace") as fh:
            payload = json.load(fh)
        rc, lines = check_payload(payload)
        for ln in lines:
            print(ln)
        return rc
    if len(argv) >= 3 and argv[1] == "items":
        with open(argv[2], encoding="utf-8", errors="replace") as fh:
            ids, names = named_items(brief_only(fh.read()))
        print(json.dumps({"ids": ids, "names": names}))
        return 0
    if len(argv) >= 3 and argv[1] == "annotate":
        with open(argv[2], encoding="utf-8", errors="replace") as fh:
            raw = fh.read()
        repos = [argv[argv.index("--repo") + 1]] if "--repo" in argv else []
        out, notes = annotate(raw, repos)
        sys.stdout.write(out)
        for n in notes:
            sys.stderr.write("  brief-done:  %s\n" % n)
        return 0
    sys.stderr.write("usage: brief-done.py check <payload.json>\n"
                     "       brief-done.py items <brief>\n"
                     "       brief-done.py annotate <brief> [--repo <repo>]\n")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
