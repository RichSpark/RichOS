#!/usr/bin/env python3
"""brief-provenance.py — separate what a brief CITES from what it merely ASSERTS, and
tell the agent which is which, before the agent builds on either.

===========================================================================================
WHAT THIS IS FOR — type W, richos-hq docs/verification/lifecycle-failure-record-2026-09-13.md
===========================================================================================
The CEO, 2026-09-14:

    "That's not a 'hand-off'. That's a BRIEF to a newly spawned agent. A BRIEF that
     contains shitloads of not just unnecessary garbage but factually wrong and harmful
     garbage."

The brief that spawned `zach-opus-n3` carried roughly fourteen load-bearing statements: six
verified quotations, four unverified narratives asserted as observed fact, two factually
wrong, one distorted quotation, and two prescriptions built on an architecture the author had
read one code comment of.

`CLAUDE.md` already carries the rule — *"A NUMBER IN A BRIEF CARRIES THE COMMAND THAT PRODUCED
IT, OR IT CARRIES THE WORD UNVERIFIED. There is no third setting."* It was broken by its own
author one turn after he wrote up its sibling failure. The same page records a DELIBERATE
decision not to enforce it with a hook, because a blocking guard over prose has a large
false-positive class and habitual waiving kills a guard. That reasoning is sound and this file
does not overturn it.

===========================================================================================
THE ONE IDEA
===========================================================================================
THIS DOES NOT JUDGE WHETHER A CLAIM IS TRUE, AND IT NEVER REFUSES A SPAWN. It separates the
statements that carry a source from the statements that do not, and it hands that separation
to the READER WHO IS HARMED BY IT — the agent.

`CLAUDE.md` supplies the evidence that this works, from the day the rule was written:

    "The marked form works and I know it works: three times that day I wrote 'confirm this
     yourself, I derived it with a two-condition grep', and all three times the agent
     re-derived it and was right."

Marking worked 3/3. It failed only because it was applied by hand, by the one person who
cannot see his own recollection as a recollection. So the mark is generated instead.

WHY NON-BLOCKING IS NOT A WEAKER VERSION OF BLOCKING. A blocking guard over prose dies the way
g11/g12/g13 died: false positive -> waive -> habitual waive -> dead guard. This has no waiver
to grant. A false positive costs the agent one re-derivation of a fact that turns out to be
true, which is the cheapest possible failure and leaves no habit behind. That asymmetry, not
optimism about precision, is what makes the non-blocking form viable where the blocking form
was rejected.

===========================================================================================
THE FIVE CHECKS, IN DESCENDING ORDER OF SOUNDNESS — and the order is the honest part
===========================================================================================
1. QUOTATION (exact, no judgment, no false positives). A blockquote presented as coming from
   a file is searched for in that file, byte for byte modulo whitespace. Either the string is
   there or it is not. This is the only check that can prove a defect rather than name a
   risk. It catches the distorted quotation.

2. RESTATEMENT (structural). A prose reference to a numbered point of a document the brief
   itself names — "Point 5 then guarantees...", "Point 7: both endings delete" — is a
   PARAPHRASE of a citable source. The record's own words: "every paraphrase is a fresh
   surface for distortion - one of them distorted. A brief that says *read points 4, 5, 7 and
   11 of <path>* carries the same authority and cannot drift from it."

3. PROVENANCE (heuristic, and noisy). A statement is listed when it carries a COUNT, an
   assertion of CERTAINTY, or a claim about the BEHAVIOR OF A NAMED CODE ENTITY, and no command,
   captured output, commit, file:line or re-derive instruction sits in scope with it.

4. CAPABILITY (heuristic, and the noisiest — added for type Y, §10i of the same record). The
   MIRROR IMAGE of check 3: it looks only at statements that DO carry a source, and asks whether
   the source is the KIND of thing that could establish the sentence's verb. A state measurement
   answers what IS; it never answers who did it, when, whether it was allowed, or which of
   several candidates it was. `git branch --no-merged` returning one branch was reported to the
   CEO as "all but one are fully merged into main", and merged names an act somebody performed.
   Re-running the command CONFIRMS the number, which is why checks 1-3 are blind to this by
   construction rather than by oversight.

   MEASURED, so the noise is not a matter of opinion: over this session's 23 spawn payloads it
   adds 1 row (against 140 from checks 1-3), and over 63 landed records it adds 37 (against
   1743). Of those 37, eight are real, six are arguable and 23 are false positives — a 62% false
   positive rate, which is survivable ONLY in the non-blocking shape and would be fatal in a
   guard. The eight include all three sentences of the one record in that corpus later PROVEN
   false by another agent, at the cost of an escalation and a published correction.

5. SELECTOR BLINDNESS (heuristic; the narrowest of the five and the one with the oldest rule
   behind it — added 2026-09-14 for the three instances of one night). A NEGATIVE-EXISTENCE
   claim resting on a search. `grep "git init"` returned one hit, THE COMMENT MAKING THE CLAIM,
   and the brief said the suite builds no repositories; it had built them all along, spelled
   `git -C "$root" init -q -b main`. The rule is this project's own, written 2026-05-08 for
   TESTS (`feedback_negative_tests_pass_for_wrong_reason.md`) and never pointed at claims until
   now: a negative result cannot tell "the thing is absent" from "the check could not see it."

   THE LINE, because a check that fires on every negative claim dies in a week. A CENSUS
   (`git status`, `ls`) enumerates its domain, so empty output IS absence. A SELECTOR SEARCH
   puts an author-written pattern in front of the domain, and only that can be blind. Then the
   discriminator: the claim is flagged only when the SENTENCE DOES NOT NAME THE PATTERN — a
   sentence about the string is grep's own question, and grep answers it exactly.

   TWO ENTRY BRANCHES, because the instance as retold and the instance as SHIPPED are not the
   same. (a) the search is cited and the sentence is about something wider. (b) no search is
   cited at all and a QUOTED LITERAL is asserted absent — which is the flagship instance: the
   brief relayed a conclusion, the search happened in the lead's head, and a claim that a
   string is absent is a search result whether or not anybody shows the search.

   MEASURED on the surface it is delivered on: 4 rows over 30 real briefs (3 real, 1 the known
   mention-versus-use false positive), 6 over 31 spawn payloads. Over the 65-document records
   corpus it would be 31 against checks 1-4's 1862, which is why it is not wired into the
   record-write hook; that hook is check 4 only by its own measured decision.

WHAT THIS CANNOT DO, STATED HERE SO IT IS NOT DISCOVERED LATER:
  - It cannot tell a true account from a false one. Nothing textual can.
  - Check 4 cannot tell MENTION from USE. A document about the word "merged" is flagged for
    containing it; test case Y11 asserts that failure out loud rather than hiding it.
  - Check 4 only speaks about commands in its own table. An unrecognized command yields nothing,
    which is deliberate: the table is an allowlist of capabilities somebody wrote down, not a
    guess about every program on the machine.
  - It cannot catch a bare prescription that names no code entity — "do not build a second
    reaper". That sentence is formally identical to a legitimate constraint ("do not weaken
    the guard"), which good briefs carry. It catches such a prescription only through the
    account it rests on, which is usually in the same sentence and usually is caught.
  - Check 5 cannot see a negative claim that names neither a search nor a literal. "The suite
    builds no repositories", relayed with nothing beside it, has no surface to recognize; test
    case S7b asserts that out loud.
  - NOTHING HERE TOUCHES MISREADING. A command that ran, whose output was pasted, and whose
    output was read wrong — indented log lines taken for top-level failures — produces a brief
    with a real command, a real output and a false sentence, and every check above is satisfied
    by it. That failure has a second reader as its only defense.
  - It cannot make the lead read its output. It does not need to: the annotation reaches the
    agent whether or not the lead reads a word of it.

===========================================================================================
SCOPE OF EVIDENCE — why "in scope" is the block, not the sentence
===========================================================================================
A brief that pastes a grep and then says "21 other sites" has sourced the 21. Evidence
therefore counts for a whole block (a paragraph, a list item, a table) and for a code block
directly adjacent to it. It does NOT count across a heading: a command in the problem
statement does not source a claim three sections later. That is the same scoping the
re-derive instruction gets, and it is what stops one "do not assume" at the bottom of a page
exempting everything above it.
"""
import argparse
import json
import os
import re
import shlex
import sys

# ---------------------------------------------------------------------------
# Sections that are pure instruction in every brief this project writes. They
# carry the stock "21-181 s, exit 3 is success" and "Report worktree path,
# branch and commit SHAs" lines, which are directions to the agent and not
# claims about the system. Reading them adds one identical finding to every
# brief ever written, which is how a report becomes wallpaper.
# ---------------------------------------------------------------------------
SKIP_SECTION = re.compile(
    r"(completion criterion|^\s*verify\b|out of scope|^\s*rules\b|^\s*deliverable)", re.I)

# ---------------------------------------------------------------------------
# EVIDENCE
# ---------------------------------------------------------------------------
EXEC_FIRST = (r"(?:git|grep|rg|ls|cat|sed|awk|find|head|tail|wc|diff|docker|npm|npx|node|"
              r"python3?|bash|sh|zsh|adb|xcrun|make|cargo|curl|jq|pgrep|ps|open|"
              r"\./[\w./-]+|[\w./-]+\.(?:sh|py|pl|rb))")
INLINE_CMD = re.compile(r"`([^`\n]+)`")
SHA = re.compile(r"(?<![0-9a-zA-Z/_-])[0-9a-f]{7,40}(?![0-9a-zA-Z/_-])")
FILE_LINE = re.compile(r"(?:line|lines|:)\s*\d{1,5}\b", re.I)
RUN_ID = re.compile(r"\b(?:run\s+)?\d{8,}\b", re.I)
URL = re.compile(r"https?://\S+")
UNVERIFIED = re.compile(r"\bunverified\b", re.I)
REDERIVE = re.compile(
    r"(re-?derive|derive (?:it|them|the list) yourself|confirm (?:this|it|or refute)|"
    r"prove it from|reproduce (?:it )?locally|do not assume|do not trust|rather than trusting|"
    r"check (?:this|it) yourself|verify (?:this|it) yourself|measure it|do not adopt|"
    r"read (?:them|it|that|this|those|the [\w' -]{2,30}) (?:first|yourself|in full)|"
    r"read (?:that|the) (?:commit|section|file|page|document|header|source)|"
    r"say so and stop|establish the real shape)", re.I)

# ---------------------------------------------------------------------------
# CLAIM TRIGGERS
# ---------------------------------------------------------------------------
COUNT_WORD = re.compile(
    r"\b(two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|twice|thrice|"
    r"both|neither|dozens|several|a handful|a couple)\b"
    # "three commits" is a count somebody had to derive. "three hours earlier" is a
    # duration, and reading it as a count put a row on a brief that had done nothing
    # wrong. Durations are excluded by the noun that follows them.
    r"(?!\s+(?:hours?|minutes?|seconds?|days?|weeks?|months?|years?|nights?|"
    r"o'clock|a\.?m\.?|p\.?m\.?)\b)", re.I)
# A digit is a count unless it is a pointer (point 11, line 229, exit 3), a date,
# a time, a version, or a duration in the stock instruction lines.
DIGIT = re.compile(r"(?<![\w./:-])(\d{1,3}(?:,\d{3})*|\d{4,})(?![\w./:-])")
POINTER_BEFORE = re.compile(
    r"(point|points|§|section|sections|clause|line|lines|row|rows|step|item|page|shard|"
    r"case|run|exit|port|v|version|figure|table|round|type|layer|chapter|no\.|#)\s*$", re.I)
UNIT_AFTER = re.compile(r"^\s*(?:-\s*\d+\s*)?(s|ms|sec|secs|seconds?|min|mins|minutes?|h|"
                        r"hours?|px|%)\b", re.I)
ISO_DATE = re.compile(r"\b\d{4}-\d{2}-\d{2}")
CERTAIN = re.compile(
    r"\b(demonstrably|provably|clearly|obviously|evidently|certainly|definitely|undeniably|"
    r"in fact|of course|it is clear|needless to say|self-evident)\b", re.I)
# Protocol lines the spawn machinery reads, not statements the agent acts on. They are
# addressed to a guard; flagging them puts an identical row on briefs that share nothing.
MARKER_LINE = re.compile(
    r"^\s*(cross-repo-worktree|ceo-todos-deferred|main-checkout-run|resume-ack|inflight-ack|"
    r"no-inflight-ack|data-contract-bypass|model-ceiling-ack|model-downgrade-ack|"
    r"dialect-exempt|hardware-fixed)\s*:", re.I)
# A number inside somebody else's quoted words is THEIR number. The same exemption the
# dialect guard grants quoted external material, for the same reason.
QUOTED_RUN = re.compile(r"[\"“‘']([^\"“”‘’]{12,})[\"”’']")
ENTITY = re.compile(r"`([\w./-]+\.(?:sh|py|ts|js|tsx|svelte|kt|swift|json|md|yml|yaml|toml))`")
DEFINITE_ARCH = re.compile(
    r"\bthe (existing|current|same|only|whole|entire) [\w -]{2,40}?\b(path|code|logic|check|"
    r"signal|mechanism|reaper|guard|hook|flow|handler|branch|marker|record|registry|table|"
    r"list|field|function|script|suite|gate|step)\b", re.I)
BEHAVIOR = re.compile(
    r"\b(refus\w+|reject\w+|accept\w+|ask\w*|asks|read\w*|writ\w+|handl\w+|mark\w+|block\w+|"
    r"see\w*|saw|take\w*|took|exist\w*|remain\w*|pass\w+|fail\w+|convert\w+|print\w+|"
    r"delet\w+|reap\w+|fire\w*|record\w+|consum\w+|treat\w+|keep\w*|kept|know\w*|"
    r"is keyed|does not|do not see|did not|never (?:sees|fires|marks|runs))\b", re.I)

MAX_QUOTE = 220


# ---------------------------------------------------------------------------
# SEGMENTATION
# ---------------------------------------------------------------------------
def segment(text):
    """Blocks of one kind each, carrying the heading they sit under."""
    lines = text.split("\n")
    blocks = []
    section = ""
    i = 0
    buf, kind = [], None

    def flush():
        nonlocal buf, kind
        if buf and kind:
            blocks.append({"kind": kind, "text": "\n".join(buf), "section": section})
        buf, kind = [], None

    while i < len(lines):
        ln = lines[i]
        if ln.strip().startswith("```"):
            flush()
            fence = [ln]
            i += 1
            while i < len(lines) and not lines[i].strip().startswith("```"):
                fence.append(lines[i])
                i += 1
            if i < len(lines):
                fence.append(lines[i])
            blocks.append({"kind": "code", "text": "\n".join(fence), "section": section})
            i += 1
            continue
        if not ln.strip():
            flush()
            i += 1
            continue
        if ln.startswith("#"):
            flush()
            section = ln.lstrip("#").strip()
            blocks.append({"kind": "heading", "text": ln, "section": section})
            i += 1
            continue
        this = ("code" if re.match(r"^(?: {4,}|\t)", ln) else
                "quote" if ln.lstrip().startswith(">") else
                "table" if ln.lstrip().startswith("|") else "prose")
        if kind and this != kind:
            flush()
        kind = this
        buf.append(ln)
        i += 1
    flush()
    return blocks


SENT_SPLIT = re.compile(r"(?<=[.!?])\s+(?=[\"'*`(A-Z0-9])")


def sentences(block_text):
    """Sentences, with list markers and table rows treated as their own units."""
    out = []
    for raw in block_text.split("\n"):
        s = raw.strip()
        if not s:
            continue
        s = re.sub(r"^(?:[-*+]|\d+\.)\s+", "", s)
        if s.startswith("|"):
            out.append(s.strip("|").strip())
            continue
        for part in SENT_SPLIT.split(s):
            part = part.strip()
            if len(part) > 2:
                out.append(part)
    return out


QUOTE_SPAN = re.compile(r"[\"“][^\"“”]{12,}?[\"”]", re.S)


def mask_quotes(block_text):
    """The same text with everything inside a quotation blanked out, LENGTH AND LINE
    BREAKS PRESERVED — so it splits into exactly the same sentences as the original and
    the two can be walked together.

    Why it has to happen before sentence splitting: a CEO quotation routinely wraps across
    two lines, and splitting first leaves each half holding one unbalanced quote mark. That
    is how 'it has been broken twice this week' — HIS words, HIS count — was read as the
    lead's unsourced claim."""
    out = list(block_text)
    for m in QUOTE_SPAN.finditer(block_text):
        for k in range(m.start(), m.end()):
            if out[k] not in "\n.!?":     # keep what the splitter keys on, so the masked
                out[k] = "x"              # text splits into the SAME sentences as the shown
    return "".join(out)


def plain(s):
    """Markdown emphasis removed; backticked spans KEPT, they carry the entities."""
    s = re.sub(r"\*\*|__|\*|_", "", s)
    return s.strip()


# ---------------------------------------------------------------------------
# EVIDENCE IN SCOPE
# ---------------------------------------------------------------------------
# A SPAN THAT WAS NAMED IS NOT A SPAN THAT WAS RUN, and the difference was worth
# finding. The brief that shipped the flagship search-blindness instance cited NO
# command for it — it relayed a conclusion — and check 3 was silent on the false
# sentence anyway, because the sentence contained “`git init`” in a code span and that
# span was read as a citation. THE STRING THE CLAIM WAS WRONG ABOUT WAS COUNTED AS THE
# EVIDENCE FOR IT.
#
# An invocation carries a flag, a path, a pipe, a redirect, an extension, or a third
# token. A bare `verb noun` pair carries none of those and is how prose NAMES a command
# it is talking about. Measured before adopting, because widening what counts as
# unsourced is the direction that can turn a report into wallpaper: +3 rows over 30
# briefs, +3 over 31 spawn payloads, +25 over 65 records — and the real sentence gets
# its row.
INVOKED = re.compile(r"(?:^|\s)--?[a-zA-Z]|[/|>]|\.\w|\s\S+\s\S+")


def is_command(span):
    span = span.strip()
    if " " not in span and not span.startswith("./"):
        return False
    if not INVOKED.search(span):
        return False
    if re.match(r"^" + EXEC_FIRST + r"\b", span):
        return True
    return bool(re.search(r"(?:^|\s)--?[a-zA-Z]", span))


def evidence_of(text):
    """Every kind of source a block can carry, as a set of labels."""
    got = set()
    for span in INLINE_CMD.findall(text):
        if is_command(span):
            got.add("command")
        elif SHA.fullmatch(span.strip()):
            got.add("commit")
    stripped = INLINE_CMD.sub(" ", text)
    if SHA.search(stripped):
        got.add("commit")
    if FILE_LINE.search(text):
        got.add("file:line")
    if RUN_ID.search(stripped) or URL.search(text):
        got.add("run")
    if UNVERIFIED.search(text):
        got.add("marked-unverified")
    if REDERIVE.search(text):
        got.add("re-derive instruction")
    return got


def scope_evidence(blocks, idx):
    """The block itself, plus a code block directly adjacent on either side — never
    across a heading."""
    got = evidence_of(blocks[idx]["text"])
    for j in (idx - 1, idx + 1):
        if 0 <= j < len(blocks) and blocks[j]["kind"] == "code":
            got.add("captured output")
            got |= evidence_of(blocks[j]["text"])
    return got


# ---------------------------------------------------------------------------
# CHECK 3 — PROVENANCE
# ---------------------------------------------------------------------------
def counts_in(sentence):
    bare = INLINE_CMD.sub(" ", sentence)
    bare = QUOTED_RUN.sub(" ", bare)
    hits = [m.group(0) for m in COUNT_WORD.finditer(bare)]
    bare = ISO_DATE.sub(" ", bare)
    bare = URL.sub(" ", bare)
    for m in DIGIT.finditer(bare):
        before, after = bare[:m.start()], bare[m.end():]
        if POINTER_BEFORE.search(before) or UNIT_AFTER.match(after):
            continue
        if re.match(r"^(?:19|20)\d\d$", m.group(0)):
            continue
        if len(m.group(0)) >= 4 and "," not in m.group(0):
            continue                      # ids, years, run numbers
        hits.append(m.group(0))
    return hits


def triggers(sentence):
    p = plain(sentence)
    out = []
    c = counts_in(p)
    if c:
        out.append("count(%s)" % ", ".join(sorted(set(c))[:3]))
    if CERTAIN.search(p):
        out.append("asserted as certain")
    ents = ENTITY.findall(p)
    arch = DEFINITE_ARCH.search(p)
    if (ents or arch) and BEHAVIOR.search(p):
        what = ents[0] if ents else arch.group(0)
        out.append(("what %s says" if what.endswith((".md", ".txt")) else "behavior of %s")
                   % what)
    return out


# ---------------------------------------------------------------------------
# CHECK 1 — QUOTATION, and CHECK 2 — RESTATEMENT
# ---------------------------------------------------------------------------
# A match starts only where a run of path characters starts. It finds exactly what it found
# without the lookbehind (a path found from inside a run is also found from the run's start,
# which comes first), but a search over a long run with no "/" in it is now linear: tried from
# every position of 65,000 "x" characters it took 10 s per search, three searches per brief,
# and pushed one hire in app.test.py past the app's 120 s bound on a loaded Mac.
PATH_IN = re.compile(r"`?(?<![\w.-])((?:[\w.-]+/)+[\w.-]+\.(?:md|py|sh|ts|js|json|txt))`?")
VERBATIM = re.compile(r"\b(verbatim|word for word|in (?:his|her|their|its) own words|quoted?)\b",
                      re.I)
POINT_REF = re.compile(r"\b(?:point|points|§|clause|section)\s*(\d+(?:\s*(?:,|and|-)\s*\d+)*)",
                       re.I)
# Referring to a numbered point is fine and is what a brief should do. RESTATING what it
# says is the laundering surface. The difference is an assertion verb or a colon after the
# reference, and it is the difference between "read point 11" and "point 7: both endings
# delete" — the second is a paraphrase the agent will believe instead of the source.
POINT_ASSERT = re.compile(
    r"\b(?:point|points|§|clause|section)\s*\d[\d,\s&and-]*\s*(?::|—|-\s|then\b|"
    r"(?:then\s+)?(?:guarantees?|says?|means?|requires?|states?|covers?|forbids?|makes?|"
    r"scopes?|applies|is\b|are\b|has\b|have\b))", re.I)
IMPERATIVE = re.compile(
    r"^(do|don't|make|find|report|read|run|keep|add|use|write|give|paste|prove|revert|never|"
    r"always|treat|start|state|update|fix|check|confirm|verify|work|commit|rewrite|take|"
    r"follow|put|push|send|ask|say|list|show|name|consider|escalate|deliver|include|avoid|"
    r"ensure|leave|change|remove|delete|stop|spawn|land|measure|enumerate|re-derive)\b", re.I)


def norm(s):
    """Whitespace, markdown emphasis and smart punctuation removed, so that a quotation
    the author re-bolded still matches the source it came from. A quotation check that
    fired on an added `**` would be noise pretending to be rigor."""
    s = plain(s)
    s = s.replace("’", "'").replace("‘", "'")
    s = s.replace("“", '"').replace("”", '"')
    s = s.replace("—", "-").replace("–", "-")
    return re.sub(r"\s+", " ", s).strip().lower()


INLINE_QUOTE = re.compile(r"[\"“]([^\"“”]{40,})[\"”]")


def _body_of(path, repo, cache):
    if path in cache:
        return cache[path]
    full = os.path.join(repo, path) if repo else path
    body = ""
    if os.path.isfile(full):
        try:
            with open(full, encoding="utf-8", errors="replace") as fh:
                body = norm(fh.read())
        except OSError:
            body = ""
    cache[path] = body
    return body


def check_quotations(blocks, repo):
    """A quotation attributed to a file is either in that file or it is not. This is the
    only check here that PROVES a defect: no judgment, no threshold, no false positive."""
    findings = []
    cache = {}
    doc = ""
    for i, b in enumerate(blocks):
        m = PATH_IN.search(b["text"])
        if m:
            doc = m.group(1)
        if b["kind"] == "heading":
            continue
        intro = blocks[i - 1]["text"] if i else ""
        near = PATH_IN.search(b["text"]) or PATH_IN.search(intro)
        # A BLOCKQUOTE is formally attributed, so the document named anywhere above it is
        # its source. An INLINE quotation is only attributable when a source is named right
        # there — otherwise a quoted line of guard output gets checked against a spec
        # mentioned three sections earlier, and the mechanism invents a defect.
        path = (near.group(1) if near else doc) if b["kind"] == "quote" else (
            near.group(1) if near else "")
        if not path:
            continue
        body = _body_of(path, repo, cache)
        if not body:
            continue                      # a file this machine does not have is not evidence
        if b["kind"] == "quote":
            quoted = ["\n".join(re.sub(r"^\s*>\s?", "", l) for l in b["text"].split("\n"))]
        else:
            quoted = INLINE_QUOTE.findall(b["text"])
        for q in quoted:
            # An elision is the author's own admission that this is not the whole thing;
            # each surviving run is checked on its own.
            for run in re.split(r"\s*(?:\.\.\.|…|\[\.\.\.\])\s*", q):
                n = norm(plain(run))
                if len(n) < 40:
                    continue
                if n not in body:
                    findings.append({
                        "check": "quotation",
                        "detail": "presented as from %s; this run is NOT in that file" % path,
                        "text": run.strip()[:MAX_QUOTE],
                        "verbatim_claimed": bool(VERBATIM.search(intro)),
                    })
    return findings


def check_restatements(blocks):
    """Prose that restates a numbered point instead of quoting or pointing at it."""
    findings = []
    doc = ""
    for i, b in enumerate(blocks):
        m = PATH_IN.search(b["text"])
        if m:
            doc = m.group(1)
        if b["kind"] in ("code", "quote", "heading"):
            continue
        if SKIP_SECTION.search(b["section"] or ""):
            continue
        next_quote = i + 1 < len(blocks) and blocks[i + 1]["kind"] == "quote"
        for s in sentences(b["text"]):
            p = plain(s)
            if not POINT_REF.search(p) or not POINT_ASSERT.search(p):
                continue
            if next_quote and p.rstrip().endswith(":"):
                continue                  # this is the introduction to a quotation, not one
            if p.rstrip().endswith("?") or IMPERATIVE.match(p):
                continue                  # asking about a point, or aiming at it, is not
                                          # restating what it says
            # A sentence that is MOSTLY quotation is a quotation, and check 1 owns it. A
            # short quoted fragment inside a paraphrase is not: that is the shape that
            # dropped "Landed means" from point 4 while keeping five of its words.
            quoted_chars = sum(len(q) for q in INLINE_QUOTE.findall(p)
                               + re.findall(r"[\"“]([^\"“”]+)[\"”]", p))
            if quoted_chars > 0.6 * len(p):
                continue
            findings.append({
                "check": "restatement",
                "detail": "restates %s in prose rather than quoting it or pointing at it"
                          % (doc or "a numbered source"),
                "text": p[:MAX_QUOTE],
            })
    return findings


def check_provenance(blocks):
    findings = []
    for i, b in enumerate(blocks):
        if b["kind"] in ("code", "heading"):
            continue
        if SKIP_SECTION.search(b["section"] or ""):
            continue
        if b["kind"] == "quote":
            continue                      # a quotation is check 1's business
        got = scope_evidence(blocks, i)
        sourced = got - {"marked-unverified"}
        shown = sentences(b["text"])
        masked = sentences(mask_quotes(b["text"]))
        if len(masked) != len(shown):
            masked = shown           # never analyze a sentence that is not the one shown
        for s, m in zip(shown, masked):
            if MARKER_LINE.match(s):
                continue
            t = triggers(m)
            # A table in these briefs is an ENUMERATION OF TARGETS with a pointer in every
            # row — line numbers, case names, counts of what is in each. Reading its cells
            # as claims puts a row on the report for every row of the table.
            if b["kind"] == "table":
                t = [x for x in t if not x.startswith("count(")]
            if not t:
                continue
            if "marked-unverified" in got:
                continue
            if sourced:
                # A COUNT is settled by something that can be counted again. A single
                # quoted instance evidences one occurrence and never a total, which is
                # exactly how "twice" survived a brief that quoted the guard once.
                countish = any(x.startswith("count(") for x in t)
                recountable = sourced & {"command", "re-derive instruction",
                                         "captured output", "run"}
                if not (countish and not recountable):
                    continue
            findings.append({
                "check": "provenance",
                "detail": "; ".join(t) + (" — in scope: %s" % ", ".join(sorted(sourced))
                                          if sourced else " — nothing in scope sources it"),
                "text": plain(s)[:MAX_QUOTE],
                "section": b["section"],
            })
    return findings


# ---------------------------------------------------------------------------
# CHECK 4 — CAPABILITY: the citation is real, and it does not establish the verb
#
# Type Y, richos-hq docs/verification/lifecycle-failure-record-2026-09-13.md §10i.
# Checks 1-3 ask whether a statement HAS a source. This one only ever looks at
# statements that DO, which is why it is not a stricter version of check 3 but its
# complement: check 3 fires when nothing in scope sources a sentence, check 4 fires
# only when something does. The two populations are disjoint by construction.
#
# WHAT IT DOES NOT DO. It does not decide whether the sentence is true, and it does
# not decide entailment. It states what the CITED COMMAND is capable of establishing
# and what the SENTENCE asserts, and leaves the comparison to the reader. That is
# deliberate: a capability statement about `git branch --no-merged` has no truth value
# to get wrong, so this check cannot produce a false ACCUSATION. It can only produce
# an UNNECESSARY row, and the rate of those is the thing measured on the real corpus.
#
# THREE RULES, and they are three because the corpus is not one shape. §10i proposes a
# single tell - "if the sentence has an actor in it, the command must have found the
# actor". Instance 2 of its own four has no actor: a count of rows ever addressed to
# the CEO was given the name outstanding. That is a SCOPE mismatch, not an actor one,
# and an actor detector is blind to it.
#
#   A1  AUTHORITY and EXPECTATION are universally unestablishable. No shell command
#       reports whether something was allowed, or whose action is awaited. This rule
#       needs no table and does not even need a command: ANY cited evidence plus such
#       a predicate is a mismatch.
#   A2  ACT, AGENT, TIME and IDENTITY are table-driven, because whether a command can
#       carry them depends on the command. `git reflog` DOES record operations;
#       `git branch --no-merged` does not. Firing on the word alone would flag the
#       correct usage, and a check that cannot tell the two apart teaches its reader
#       to skip it.
#   B   A COUNT'S NAME MUST BE DERIVABLE FROM ITS SELECTOR. Purely lexical: if the
#       sentence calls a count `outstanding` and the command that produced it contains
#       no word to that effect, the count may carry a filter that was never applied.
#       This is the only rule that catches instance 2.
# ---------------------------------------------------------------------------

# What a command establishes, and the classes of claim it cannot carry. FIRST MATCH
# WINS, so the specific forms precede the general ones. A command that is not in this
# table produces NO finding under A2 - the table is an allowlist of things we can state
# a capability for, not a denylist, and that is what keeps the rate low.
CAPABILITY_TABLE = [
    (re.compile(r"\bgit\b[^|;&]*\bbranch\b[^|;&]*--(?:no-)?merged\b"),
     "which branch tips are reachable from a ref at this instant",
     {"act", "agent", "time"}),
    (re.compile(r"\bgit\b[^|;&]*\bmerge-base\b[^|;&]*--is-ancestor\b"),
     "whether one commit is reachable from another at this instant - which is equally "
     "true of EVERY later merge, so it singles out none of them",
     {"act", "agent", "time", "identity"}),
    (re.compile(r"\bgit\b[^|;&]*\bstatus\b"),
     "the working tree's state at this instant",
     {"act", "agent", "time"}),
    (re.compile(r"\bgit\b[^|;&]*\b(?:rev-parse|show-ref|symbolic-ref|cat-file|ls-tree)\b"),
     "what a ref or object is at this instant",
     {"act", "agent", "time"}),
    (re.compile(r"\bgit\b[^|;&]*\b(?:branch|tag|worktree\s+list|stash\s+list)\b"
                r"(?![^|;&]*(?:-[dDmM]\b|--delete|--move|--force))"),
     "which refs or workspaces exist at this instant",
     {"act", "agent", "time"}),
    (re.compile(r"(?:\bwc\s+-l\b|\bgrep\s+-c\b|\b--count\b|\|\s*wc\b|\bjq\b[^|;&]*\blength\b)"),
     "a count of the items the selector you gave it matched",
     {"act", "agent", "status"}),
    (re.compile(r"(?:^|\s)(?:ls|test\s+-[a-z]|\[\s+-[a-z]|stat|find)\b"),
     "what is present at this instant",
     {"act", "agent", "time"}),
    (re.compile(r"(?:^|\s)(?:grep|rg)\b"),
     "whether that text is present in the files you searched",
     {"act", "agent", "time"}),
]

# A command that PERFORMS the act establishes the act. Running `git merge` and then
# saying it was merged is not this failure; it is a report.
#
# `merge(?!-base)` is not cosmetic. A hyphen is a word boundary, so `\bmerge\b` matches
# INSIDE `git merge-base` - which made this list read the corpus's own read-only
# ancestry probe as a merge operation and silently exonerated instance 4.
MUTATION = re.compile(
    r"\bgit\b[^|;&]*\b(?:merge(?!-base)|push|commit|rebase|cherry-pick|revert|reset|"
    r"checkout|switch|am|apply|worktree\s+(?:add|remove)|clean)\b|"
    r"\bgit\b[^|;&]*\b(?:branch|tag)\s+-[dDmM]\b|"
    r"(?:^|\s)(?:rm|mv|cp|mkdir|touch|kill|pkill)\b", re.I)
# A command that READS THE HISTORY genuinely carries who and when. The codex-merge
# record's whole method rests on the reflog, and a check that flagged its every row
# would be wallpaper on the one document that got this right.
HISTORY_CAPABLE = re.compile(
    r"\bgit\b[^|;&]*\b(?:reflog|blame|whatchanged)\b|"
    r"\bgit\b[^|;&]*\blog\b[^|;&]*(?:--merges|--author|--format|--pretty|--walk-reflogs)",
    re.I)

PERSON = (r"(?:him|her|them|his|hers|their|theirs|the CEO|the founder|the user|"
          r"the lead|you|your)")
PREDICATE = {
    "act": re.compile(
        r"\b(?:merged|landed|deleted|removed|reverted|pushed|deployed|introduced|arrived|"
        r"shipped|reaped|cherry-picked|rebased|abandoned|discarded|"
        r"brought\s+(?:it|them|in)|carried\s+(?:it|them)\s+in|"
        r"(?:has|have|had)\s+been\s+\w+ed|(?:was|were)\s+(?:performed|done|run))\b", re.I),
    "authority": re.compile(
        r"\b(?:authori[sz]ed|approved|sanctioned|permitted|allowed|signed\s+off|"
        r"greenlit|consented|blessed|(?:he|she|they)\s+(?:agreed|said\s+yes|asked\s+for))\b",
        re.I),
    # Requires a PERSON. "waiting on the lock" is an engineering fact a command can
    # settle; "waiting on him" is a statement about whose move it is, and no command
    # has ever been able to see that.
    "expectation": re.compile(
        r"\b(?:waiting\s+(?:on|for)|awaiting|blocked\s+on|owed\s+(?:by|to)|assigned\s+to)"
        r"\s+" + PERSON + r"\b|"
        r"\b" + PERSON + r"\s+(?:to\s+(?:decide|approve|answer|rule|act)|"
        r"must\s+(?:decide|approve|answer))\b", re.I),
    "identity": re.compile(
        r"\b(?:carried\s+(?:it|them)\s+in|arrived\s+(?:as\s+a\s+passenger|inside|with|"
        r"through)|came\s+in\s+(?:with|inside|through)|brought\s+(?:it|them)\s+in|"
        r"inside\s+that\s+merge|by\s+that\s+merge|it\s+was\s+\S+\s+that\s+\w+ed\s+it)\b",
        re.I),
    "agent": re.compile(
        r"\b(?:by\s+(?:him|her|them|the\s+CEO|the\s+lead|somebody|someone)|"
        r"(?:he|she|they)\s+(?:merged|deleted|landed|pushed|removed|ran))\b", re.I),
}
CANNOT_SAY = {
    "act": "that any operation was performed",
    "agent": "who performed it",
    "authority": "whether it was authorized",
    "time": "when it happened",
    "identity": "which of several equally-consistent candidates it was",
    "status": "the current status of the things it counted",
    "expectation": "whose action is awaited",
}
ASSERTS = {
    "act": "an act - something having been done",
    "agent": "an actor",
    "authority": "authorization",
    "time": "a time",
    "identity": "one candidate out of several",
    "status": "a status the selector did not filter on",
    "expectation": "that a named person's action is awaited",
}
# A count's name, and the words in a command that would justify it. A qualifier is
# satisfied by ANY of its aliases appearing anywhere in the cited command or its
# captured output - a loose test on purpose, because a missed row and a spurious row
# cost the same thing here, and the loose direction is the quiet one.
STATUS_WORD = {
    "outstanding": ("outstand", "open", "pending", "unresolved", "unanswered", "ack"),
    "unresolved": ("unresolv", "open", "outstand", "ack"),
    "unacknowledged": ("unack", "ack"),
    "pending": ("pending", "queue", "wait", "open"),
    "remaining": ("remain", "left", "rest"),
    "orphaned": ("orphan", "dangl"),
    "stale": ("stale", "age", "mtime", "older", "since"),
    "failing": ("fail", "red", "error", "exit"),
    "unmerged": ("merged", "merge"),
}
# A SPECIFICATION IS NOT A REPORT. "the record must be reconciled as landed", "a probe
# deleted with its manifest line would leave no trace" - these state an obligation or a
# hypothesis, and neither claims that anything happened, so no source could be expected
# to establish one. This corpus is mostly specifications, and without this the check
# reads a test matrix as a pile of assertions.
HYPOTHETICAL = re.compile(
    r"\b(?:must|should|shall|would|could|can|may|might|ought)\b|"
    r"^\s*(?:if|when|unless|suppose|given|were)\b|\b(?:if|unless)\s", re.I)
UNIVERSAL = {"authority", "expectation"}
MAX_PER_CAPABILITY = 3


def scope_commands(blocks, idx):
    """(commands in scope, the cited text) for a block.

    THE CITED TEXT IS THE COMMANDS AND THEIR CAPTURED OUTPUT, AND NEVER THE PROSE
    BLOCK ITSELF. Rule B asks whether the COMMAND says `outstanding`; feeding it the
    sentence under test lets the claim exonerate itself, which is exactly how the
    first draft of this check silently passed instance 2.

    The scoping is otherwise the evidence check's: the block, and a code block
    directly adjacent on either side, never across a heading."""
    cmds, cited = [], []
    for span in INLINE_CMD.findall(blocks[idx]["text"]):
        if is_command(span):
            cmds.append(span.strip())
            cited.append(span)
    for j in (idx - 1, idx + 1):
        if 0 <= j < len(blocks) and blocks[j]["kind"] == "code":
            cited.append(blocks[j]["text"])
            for line in blocks[j]["text"].split("\n"):
                line = re.sub(r"^\s*(?:```\w*|[$#>]\s*)", "", line).strip()
                if line and is_command(line):
                    cmds.append(line)
    return cmds, "\n".join(cited)


QUOTE_MARK = re.compile(u'["\u201c\u201d]')
CODE_SPAN = re.compile(r"`[^`\n]*`")
# "whether it was authorized" is a QUESTION; "what is allowed" is a DEFINITION. Neither
# asserts that anything was authorized, and both put a row on a brief that had done
# nothing wrong - one of them on the brief that commissioned this check, which uses the
# word in the course of defining it.
DEFINITIONAL = re.compile(r"\b(?:what|whether|which)\b[^.;:]*$", re.I)


def blank_code(text):
    """The sentence with every backticked span replaced by filler OF THE SAME LENGTH,
    so offsets still line up with the original.

    Two reasons, and the first is a defect this had: `--no-merged` contains `merged`
    between two word boundaries, so the act predicate matched INSIDE the very command
    being cited, at an offset outside the quotation, which then defeated the quotation
    test. The second: a quote mark inside a command (`grep -c '\"for\": \"ceo\"'`)
    corrupts the parity count that test depends on."""
    return CODE_SPAN.sub(lambda m: "x" * len(m.group(0)), text)


def inside_quotes(text, pos):
    """True when an odd number of double-quote marks precede this position, i.e. the
    match is inside somebody else's words.

    mask_quotes() already blanks quotations, and it is not enough here. It pairs quote
    marks with a non-greedy 12-character floor, so a SHORT quotation earlier in the
    sentence (`"32.6 GB"`) is skipped and the pairing walks off by one, leaving the
    real quotation exposed. That is how a sentence CORRECTING the phrase "waiting on
    him" was flagged for containing it. Counting marks needs no pairing and does not
    care how short the first quotation was."""
    return len(QUOTE_MARK.findall(text[:pos])) % 2 == 1


def capability_of(cmds, established=()):
    """(command, what it establishes, what it cannot carry) for the first command in
    scope this table can speak about, with the classes any OTHER command in scope does
    establish removed. A block that runs `git merge` and `git status` has performed an
    act, whatever the second command measures."""
    established = set(established)
    for c in cmds:
        if MUTATION.search(c):
            established |= {"act", "time"}
        if HISTORY_CAPABLE.search(c):
            established |= {"act", "agent", "time", "identity"}
    for c in cmds:
        for pat, says, cannot in CAPABILITY_TABLE:
            if pat.search(c):
                return c, says, cannot - established
    return "", "", set()


def miscalled_count(sentence, cited_text):
    """RULE B. A count called `outstanding` whose command never mentions anything of
    the kind may carry a filter that was never applied.

    THE STATUS WORD MUST BE NEXT TO THE NUMBER. Without that, any sentence containing
    the word "failing" anywhere and a number anywhere is a finding - which is what put
    three rows on three briefs discussing a two-space grep anchor and a suite's failing
    cases. Naming a count and using the word in a different clause are not the same
    thing, and only the first is this failure."""
    low = " " + re.sub(r"\s+", " ", cited_text.lower()) + " "
    for word, aliases in STATUS_WORD.items():
        w = re.escape(word)
        near = (r"\b\d[\d,]*\s+(?:\w+\s+){0,2}" + w + r"\b|"
                r"\b" + w + r"\s+(?:\w+\s+){0,2}\d[\d,]*\b|"
                r"\b(?:are|is|were|was)\s+\d[\d,]*\s+" + w + r"\b")
        if not re.search(near, sentence, re.I):
            continue
        if any(a in low for a in aliases):
            continue
        return word
    return None


def check_capability(blocks):
    findings = []
    seen = {}
    for i, b in enumerate(blocks):
        if b["kind"] in ("code", "heading", "quote"):
            continue
        if SKIP_SECTION.search(b["section"] or ""):
            continue
        # SOURCED IS THE ENTRY CONDITION. Without it this would be check 3 again, and
        # the whole point of type Y is that the source is present and real.
        got = scope_evidence(blocks, i)
        if not (got - {"marked-unverified"}):
            continue
        if "marked-unverified" in got:
            continue                      # the author already said not to trust it
        # A COMMIT SHA IS EVIDENCE OF AN ACT. Somebody made that commit, the history
        # records who and when, and a sentence citing one is not guessing that
        # something happened. Leaving this out generated rows on sentences of the exact
        # form "deleted at `3ddc05a3` on 2026-08-28" - the best-sourced shape there is.
        from_commit = ({"act", "agent", "time"}
                       if "commit" in scope_evidence(blocks, i) else set())
        cmds, cited_text = scope_commands(blocks, i)
        cmd, says, cannot = capability_of(cmds, from_commit)
        shown = sentences(b["text"])
        masked = sentences(mask_quotes(b["text"]))
        if len(masked) != len(shown):
            masked = shown
        for s, m in zip(shown, masked):
            if MARKER_LINE.match(s):
                continue
            p, pm = plain(s), plain(m)
            if b["kind"] == "table":
                # A TABLE ROW IS ITS OWN ITEM. A code block above a table does not
                # source each of its rows, and letting it do so put one command's
                # capability onto four unrelated rows of a test-case matrix. A row is
                # read only against a command printed in that row.
                row = [x.strip() for x in INLINE_CMD.findall(s) if is_command(x)]
                if not row:
                    continue
                cmd, says, cannot = capability_of(row, from_commit)
                cmds, cited_text = row, " ".join(row)
            pa = blank_code(pm if len(pm) == len(p) else p)
            if HYPOTHETICAL.search(pa):
                continue
            classes = []
            for cls in ("act", "agent", "authority", "expectation", "identity"):
                hit = PREDICATE[cls].search(pa)
                if not hit:
                    continue
                if inside_quotes(pa, hit.start()):
                    continue
                # A hyphen is a word boundary, so `\bsanctioned\b` matches inside the
                # PATH `sanctioned-edits` and `\bmerged\b` inside `--no-merged`. A verb
                # welded into a compound is a name, not a predicate.
                if pa[max(hit.start() - 1, 0):hit.start()] == "-" \
                        or pa[hit.end():hit.end() + 1] == "-":
                    continue
                if cls == "authority" and DEFINITIONAL.search(pa[:hit.start()]):
                    continue
                if cls in UNIVERSAL:
                    classes.append(cls)             # rule A1 - no table, no command
                elif cmd and cls in cannot:
                    classes.append(cls)             # rule A2 - table-driven
            # "status" is in the cannot-set of exactly one table entry: the counter.
            # So this condition IS "a counting command is in scope", and without it the
            # rule fires on any command at all, including `rm -f`.
            name = (miscalled_count(pm, cited_text)
                    if (cmd and "status" in cannot and counts_in(pm)) else None)
            if name:
                classes.append("status")            # rule B
            if not classes:
                continue
            source = cmd or (cmds[0] if cmds else "")
            key = (source, tuple(sorted(classes)))
            seen[key] = seen.get(key, 0) + 1
            if seen[key] > MAX_PER_CAPABILITY:
                continue
            if says:
                what = "`%s` establishes %s" % (source[:90], says)
            elif source:
                what = "`%s` is the source in scope" % source[:90]
            else:
                what = "the captured output in scope is a measurement"
            extra = (" The count is called “%s”, and nothing in the command or its "
                     "output filters on that." % name) if name else ""
            findings.append({
                "check": "capability",
                "detail": "%s. It cannot establish %s — and the sentence asserts %s.%s"
                          % (what,
                             " or ".join(CANNOT_SAY[c] for c in classes),
                             " and ".join(ASSERTS[c] for c in classes),
                             extra),
                "text": p[:MAX_QUOTE],
                "section": b["section"],
            })
    return findings


# ---------------------------------------------------------------------------
# CHECK 5 — SELECTOR BLINDNESS: a negative claim resting on a search for ONE SPELLING
#
# THE RULE THIS MOVES. `feedback_negative_tests_pass_for_wrong_reason.md`, 2026-05-08,
# four months before this check and never pointed anywhere but at tests: a negative
# result cannot distinguish "the thing is absent" from "THE CHECK COULD NOT SEE IT."
# Its remedy there is a positive-shape probe. This is the same finding moved from
# tests to CLAIMS, and the remedy does NOT move with it — see below.
#
# THE THREE INSTANCES IT IS BUILT FROM, one night, each becoming a briefed premise:
#
#   `grep "git init" contract-integrity.test.sh`  -> 1 hit, THE COMMENT MAKING THE CLAIM
#       claimed: "the suite builds no repositories"
#       truth:   it had built them all along, spelled `git -C "$root" init -q -b main`
#   `grep -rn "ALREADY ACKED 26 TIME" engine/`    -> 0 hits
#       claimed: "the guard does not log its own usage"
#       truth:   guard-ci-red-lands.sh prints it from a `%d` template; the rendered
#                form with a number in it cannot appear in the source that renders it
#   a count of a typed list                       -> 53
#       claimed: "53 files carry the bootstrap"    truth: 60 do; 53 was A LIST'S LENGTH
#
# WHERE THE LINE IS, and getting it wrong is what kills a check like this. "The working
# tree is clean" also rests on a search that found nothing, and it is fine. The
# difference is not the emptiness:
#
#   A CENSUS enumerates its domain. `git status`, `ls`, `git branch` -- nothing the
#   author wrote stands between the domain and the answer, so an empty output IS the
#   absence.
#
#   A SELECTOR SEARCH puts an author-written pattern between the domain and the answer.
#   `grep PATTERN`, `find -name PATTERN`. An empty output is then ambiguous between the
#   thing being absent and the pattern not matching its spelling, and no exit code and
#   no re-run can tell the two apart.
#
# THAT ALONE IS NOT ENOUGH, and the second condition does the discriminating work.
# "`freshness-check.sh` is not referenced in the deploy scripts", sourced by a grep for
# `freshness-check`, is a sentence ABOUT THE STRING, and grep answers exactly that
# question. What went wrong in all three instances is a TRANSLATION from a string to a
# CONCEPT -- repositories, logging, carrying the bootstrap -- performed silently between
# the command and the sentence. So:
#
#   FLAG = a negative-existence claim + a selector search in scope + THE SENTENCE DOES
#          NOT NAME THE PATTERN THAT WAS SEARCHED.
#
# In instance 1 that lands exactly where the harm was: "`git init` appears nowhere in
# this file" names the pattern and is SILENT (it is true, and it is grep's own
# question); "the suite builds no repositories" does not, and is flagged. The second
# sentence is the one that misled the engineer.
#
# WHY "DID THE SEARCH RETURN NOTHING?" IS NOT A CONDITION, though it is the obvious one
# and the brief that commissioned this asked for it. It is not detectable -- a brief
# quotes a command and rarely its output -- and it is NOT NEEDED: instance 1's search
# returned a hit and was just as wrong. The negative claim lives in the brief's own
# prose, which is always there. The emptiness was never the signal.
#
# WHY NOT THE POSITIVE CONTROL, which is what the 2026-05-08 rule literally prescribes.
# Measured against the three, a control pairing (cite a second search of the same shape
# that DOES find something) catches NONE. Instance 1's own search found something, so
# the control is self-satisfying and adds no row. Instance 2's control would prove the
# path readable, which it was, and the pattern would still be a rendering of a template.
# Instance 3 has no search at all. A control proves the PLUMBING; every one of these
# failed at the SELECTOR. What actually recovers instances 1 and 2 is a strictly WEAKER
# selector -- `init -q` finds line 1621, `ALREADY ACKED` finds the print -- and "weaker"
# is a direction a person chooses, not a property two strings have. It also is not free:
# broadening instance 1 all the way to `init` returns 51 lines, most of them the word
# "definition". So the check NAMES the discipline and does not pretend to verify it,
# which is this file's shape everywhere else.
# ---------------------------------------------------------------------------
SEARCH_PROGRAM = re.compile(r"^(?:sudo\s+)?(?:git\s+grep|grep|egrep|fgrep|rg|ag|ack)\b")
FIND_PROGRAM = re.compile(r"^(?:sudo\s+)?find\b")
# Flags that swallow the operand after them, so the operand is not the pattern.
VALUE_FLAG = {"-e", "--regexp", "-f", "--file", "--include", "--exclude", "--exclude-dir",
              "-m", "--max-count", "-A", "-B", "-C", "-g", "--glob", "-t", "--type",
              "--color", "-d", "--devices", "--colour"}
# ...except these two, where the operand after them IS the pattern.
PATTERN_FLAG = {"-e", "--regexp"}
FIND_PATTERN_FLAG = {"-name", "-iname", "-path", "-ipath", "-regex", "-iregex", "-wholename"}
PAT_TOKEN = re.compile(r"[A-Za-z][A-Za-z0-9_]{2,}")
# A claim that something is ABSENT. Deliberately narrower than "contains a negation":
# "this is not the same as X" negates an identity, not an existence, and a check that
# read every `not` as an absence claim would fire on half of every brief.
NEGATIVE_EXISTENCE = re.compile(
    r"\b(?:nowhere|no(?:t)? anywhere|returns? nothing|finds? nothing|matche[sd] nothing|"
    r"no (?:hits|matches|results|occurrences|instances|such)\b|"
    r"not (?:present|found|referenced|there|defined|wired|registered|called|invoked)\b|"
    r"\babsent\b|\bnonexistent\b|\bnon-existent\b|"
    r"never (?:appears|occurs|called|invoked|run|runs|ran|logged|logs|fires|fired|"
    r"happens|happened|reached|reaches)\b|"
    # A BEHAVIOR NEGATION ("the guard does not log its own usage") is the class. An
    # IDENTITY NEGATION is not, and `(is|are|was|were) not <word>` was in this list for
    # one draft: it produced FOUR of the first six rows on the records corpus, every one
    # of them a sentence saying a thing is not some OTHER thing — "`WorkerRunEnded` is
    # not a platform event", "32.6 GB was not a stale measurement". Nothing is claimed
    # absent there, so no search could be too narrow for it. The explicit
    # `not present|found|referenced|...` alternative above keeps every existence sense
    # of `is not`, so dropping the generic form costs nothing.
    r"(?:does|do|did) n[o']t\s+\w+|"
    # "no X <verb>" — a negative existential quantifier with a predicate after it. The
    # VERB is what is anchored on, not the noun. The first draft enumerated the nouns
    # and case S9b caught it committing this check's own failure: an enumerated list is
    # ONE SPELLING of the thing, and "no harness covers that branch" was outside it.
    # `no longer` is a TEMPORAL, not a quantifier: "45 name paths that no longer exist"
    # says a thing stopped being, which is a history claim and not an absence a search
    # could have been too narrow for. Two of the seven rows this broadening produced.
    r"\bno\s+(?!longer\b)\w+(?:[ -]\w+){0,2}\s+"
    r"(?:covers?|carries|carry|calls?|invokes?|references?|exercises?|reaches|reads?|"
    r"writes?|emits?|logs?|records?|names?|matches|contains?|uses?|builds?|creates?|"
    r"touches|asserts?|checks?|tests?|fires?|runs?|exists?|loads?|sets?|wires?|"
    r"registers?|enforces?|declares?|mentions?|appears?|handles?|guards?|catches)\b|"
    r"\bno\s+(?:\w+\s+){0,2}"
    r"(?:file|files|hook|hooks|script|scripts|test|tests|suite|suites|call|calls|"
    r"caller|callers|site|sites|reference|references|repositor\w+|entry|entries|"
    r"line|lines|place|places|path|paths|code|guard|guards|check|checks|one|thing|"
    r"mechanism|record|records|log|logs|way|means)\b|"
    r"\bnone of\b|\bnothing (?:in|else|at all|anywhere)\b)", re.I)
MAX_PER_SELECTOR = 3


def _pipeline_segments(cmd):
    """A command split at the places a new program starts. `... | grep foo` has to
    yield its grep, or a pattern search behind a pipe is invisible to this."""
    return [s.strip() for s in re.split(r"\|\||&&|[|;]", cmd) if s.strip()]


def search_selector(cmd):
    """(the search segment, the author-written pattern) or None.

    The pattern is the first operand that is not a flag and is not swallowed by one.
    Paths after it are not patterns; `-e PATTERN` is. A command with no extractable
    pattern yields nothing, which is the same allowlist discipline check 4 uses: this
    speaks only about searches somebody can point at the selector in."""
    for seg in _pipeline_segments(cmd):
        if FIND_PROGRAM.match(seg):
            try:
                toks = shlex.split(seg)
            except ValueError:
                toks = seg.split()
            for j, t in enumerate(toks):
                if t in FIND_PATTERN_FLAG and j + 1 < len(toks):
                    return seg, toks[j + 1]
            continue
        if not SEARCH_PROGRAM.match(seg):
            continue
        try:
            toks = shlex.split(seg)
        except ValueError:
            toks = seg.split()
        if not toks:
            continue
        i = 2 if toks[0] in ("git", "sudo") else 1
        while i < len(toks):
            t = toks[i]
            if t in PATTERN_FLAG:
                return (seg, toks[i + 1]) if i + 1 < len(toks) else None
            if t in VALUE_FLAG:
                i += 2
                continue
            if t.startswith("-") and t != "-":
                i += 1
                continue
            return seg, t
        return None
    return None


def pattern_tokens(pat):
    """The words a pattern is looking for, with regex punctuation and one- and
    two-character fragments dropped. `ALREADY ACKED 26 TIME` -> already, acked, time;
    the 26 is gone because a digit inside a searched string is the single strongest
    sign it is a RENDERED value being sought in the source that renders it."""
    core = re.sub(r"[\\^$.\[\]()*+?{}|/-]", " ", pat)
    return [t.lower() for t in PAT_TOKEN.findall(core)]


# The literal the sentence says is absent. A backticked span, or a quoted one — the two
# forms a brief writes a string it looked for. A LONG span is excluded: "`the record must
# be reconciled`" is a phrase being discussed, not a needle.
NEEDLE = re.compile(r"`([^`\n]{2,48})`|[\"“]([^\"“”\n]{2,48})[\"”]")
# Branch (b) needs a NARROWER negation than branch (a), and the measurement is why. Run
# with the general one it put 20 rows on 30 briefs, 16 documents of 30 — over half of
# every brief, which is how a report becomes wallpaper. Nearly all of the surplus was
# one shape: "`notice-hook-staleness.sh` does NOT cover this", where the quoted span is
# the ACTOR and not the needle. Nothing is claimed absent there.
#
# So branch (b) fires only on a PRESENCE predicate — the sentence has to be saying the
# string itself is not to be found, which is the only claim that is necessarily a search
# result. `does not <any verb>` is dropped; `does not appear` is kept.
ABSENT_PRESENCE = re.compile(
    r"\bnowhere\b|"
    r"\b(?:is|are|was|were)\s+(?:\w+\s+){0,2}"
    r"(?:absent|nonexistent|non-existent|not present|not found|not there|not referenced|"
    r"not named|not mentioned|not used|not called|not wired|not registered|not defined|"
    # `not in` was in this list for one measurement and produced a row on "wherever DST
    # is not in force" — a state of the world, not the absence of a string.
    r"not anywhere)\b|"
    r"\b(?:is|are|was|were)\s+(?:named|referenced|used|carried|called|covered|matched|"
    r"mentioned|listed|required|imported|loaded)\s+(?:by|in|from)\s+no\b|"
    r"\bno\s+\w+(?:[ -]\w+){0,2}\s+"
    r"(?:names?|references?|mentions?|lists?|contains?|matches|includes?|carries|carry|"
    r"imports?|loads?)\b|"
    r"\b(?:does|do|did) n[o']t\s+(?:appear|occur|exist|show\s+up)\b", re.I)


def _absent_literal_rows(b, seen):
    """Rows for 'this string is absent', with no search shown."""
    out = []
    for s in sentences(b["text"]):
        if MARKER_LINE.match(s):
            continue
        p = plain(s)
        pa = blank_code(p)
        if IMPERATIVE.match(pa.strip()) or HYPOTHETICAL.search(pa):
            continue
        hit = ABSENT_PRESENCE.search(pa)
        if not hit:
            continue
        m = NEEDLE.search(p)
        if not m:
            continue
        needle = (m.group(1) or m.group(2)).strip()
        # The needle has to be the SUBJECT of the absence, not something further along
        # the sentence. "`X` appears nowhere" qualifies; "the guard does not run, see
        # `land.sh`" does not, and without this the row lands on half the corpus.
        if m.start() > hit.start():
            continue
        key = "absent:" + needle
        seen[key] = seen.get(key, 0) + 1
        if seen[key] > MAX_PER_SELECTOR:
            continue
        out.append({
            "check": "selector",
            "detail": "this says the string “%s” is absent, and NO SEARCH IS SHOWN. The "
                      "only way anyone knows a string is absent is by looking for it, so "
                      "there is a search behind this sentence and the brief does not say "
                      "what it was. Ask for it, widen it, and read what comes back."
                      % needle[:60],
            "text": p[:MAX_QUOTE],
            "section": b["section"],
        })
    return out


def check_selector(blocks):
    findings, seen = [], {}
    for i, b in enumerate(blocks):
        if b["kind"] in ("code", "heading", "quote"):
            continue
        if SKIP_SECTION.search(b["section"] or ""):
            continue
        got = scope_evidence(blocks, i)
        if "marked-unverified" in got:
            continue
        cmds, _cited = scope_commands(blocks, i)
        sel = None
        for c in cmds:
            sel = search_selector(c)
            if sel:
                break
        if not sel:
            # THE SEARCH LEFT OUT. This is the flagship instance as it ACTUALLY SHIPPED,
            # and it is why the entry condition is not simply "a search is cited".
            # `tom7-brief.md` line 11 relayed a conclusion — "`git init` appears nowhere
            # in that suite" — with NO command anywhere near it. A claim that a QUOTED
            # LITERAL is absent is a search result whether or not a search is shown; the
            # only way anyone knows a string is absent is by looking for it. So the
            # claim is answered here rather than by check 3, because the instruction it
            # needs is check 5's (broaden, and show it) and not check 3's (re-derive,
            # which reproduces the same narrow search and confirms it).
            findings += _absent_literal_rows(b, seen)
            continue
        seg, pat = sel
        toks = pattern_tokens(pat)
        if not toks:
            continue
        # THE DISCRIMINATOR, AND ITS SCOPE IS THE BLOCK. A brief that names the searched
        # string anywhere in this paragraph has shown the reader the translation; the row
        # is worth something only when the string is nowhere in sight.
        #
        # Scoping it to the SENTENCE was the first draft and it leaked, structurally
        # rather than occasionally: `sentences()` splits per LINE, so wrapped prose —
        # which is every brief this project writes — hands the second half of "`X` appears
        # nowhere in this file" to the test with the `X` left behind on the line above.
        # The well-formed text claim then flags itself. Block scope is also what the
        # header of this file already declares for evidence, so the two now agree.
        #
        # THE MISS THIS BUYS, stated rather than discovered later: a translated claim
        # sitting in the SAME paragraph as the string is exempted with it. Test S6 pins
        # that miss out loud.
        block_low = plain(b["text"]).lower()
        if all(t in block_low for t in toks):
            continue
        shown = sentences(b["text"])
        masked = sentences(mask_quotes(b["text"]))
        if len(masked) != len(shown):
            masked = shown
        for s, m in zip(shown, masked):
            if MARKER_LINE.match(s):
                continue
            p, pm = plain(s), plain(m)
            pa = blank_code(pm if len(pm) == len(p) else p)
            # An instruction is not a report. "do not add a second reaper" and "no file
            # should carry it" state what must be, and no search could establish either.
            if IMPERATIVE.match(pa.strip()) or HYPOTHETICAL.search(pa):
                continue
            hit = NEGATIVE_EXISTENCE.search(pa)
            if not hit or inside_quotes(pa, hit.start()):
                continue
            key = pat
            seen[key] = seen.get(key, 0) + 1
            if seen[key] > MAX_PER_SELECTOR:
                continue
            findings.append({
                "check": "selector",
                "detail": "`%s` can only answer about the string “%s” in the files it was "
                          "pointed at. This sentence is about something else, and the "
                          "sentence does not name that string — so between the search and "
                          "the claim there is a translation nobody checked. Broaden the "
                          "pattern until it returns hits and read them; re-running this one "
                          "will agree with it." % (seg[:90], pat[:60]),
                "text": p[:MAX_QUOTE],
                "section": b["section"],
            })
    return findings


def dedupe(findings):
    """One statement, one row. A sentence that is both a restatement and an unsourced
    account is one thing to go and check, not two."""
    out, seen = [], {}
    for f in findings:
        key = norm(f["text"])
        if key in seen:
            if f["detail"] not in seen[key]["detail"]:
                seen[key]["detail"] += "; " + f["detail"]
            continue
        seen[key] = dict(f)
        out.append(seen[key])
    return out


def review(prompt, repo=""):
    blocks = segment(prompt)
    return dedupe(check_quotations(blocks, repo) + check_restatements(blocks)
                  + check_capability(blocks) + check_selector(blocks)
                  + check_provenance(blocks))


# ---------------------------------------------------------------------------
# WHAT THE AGENT IS TOLD — the only part that changes what anyone does
# ---------------------------------------------------------------------------
HEAD = "## Provenance of this brief — generated, not written by the lead"

BODY = """The statements below were written by your orchestrator without a command, a commit,
a captured output or a file:line beside them. They are its ACCOUNT of the system, and an
account is a recollection until you check it. Nothing here says any of them is wrong.

**Re-derive any of them you are about to build on, and say what you found.** If one of them is
the reason for an approach this brief prescribes, treat the approach as a hypothesis and not as
a constraint: a prescription resting on an unchecked account has already chosen your design for
you. If the code contradicts it, that is a finding — report it and stop rather than forcing the
brief's shape onto what is actually there."""


# The capability rows are the OPPOSITE finding to the other three, so they cannot sit
# under a heading that says "no source". They need their own instruction, because the
# right response to them is not "go and re-run it" — re-running it CONFIRMS the number,
# which is precisely how all four instances of this failure survived.
CAPABILITY_LEAD = """These are not unsourced. The command was run, the output is real, and
re-running it will agree with the number — that is why they are listed apart. **What is in
question is the WORD, not the measurement.** A command reports what IS; none of them reports
who did a thing, when, whether it was allowed, or which of several candidates it was.

**Before you build on one of these, decide which you actually need.** If you need the
measurement, it is there and it is good. If you need the claim — that somebody did something,
that it was authorized, that a particular one of them is the one — then this source does not
carry it, and neither does re-running it: you need different evidence, or the honest answer
that the record cannot settle it."""


# Selector rows need a third instruction again, and for a third reason. Check 3 says
# GO AND DERIVE IT. Check 4 says the number is fine and the WORD is not, so do not
# bother re-deriving. This one says the command is fine, the word may be fine, and the
# QUESTION PUT TO THE COMMAND was narrower than the sentence — so the response is
# neither re-run nor find other evidence: it is BROADEN, and look at what comes back.
SELECTOR_LEAD = """Every sentence below says something is NOT THERE, and rests on a search —
shown or unshown — to say it. **The search is not in question; what it was ASKED is.** A search
answers about the STRING you gave it, in the FILES you pointed it at. Where the command is shown,
the sentence is about something wider than that string and does not name it, so a translation
happened between the two that nobody checked. Where no command is shown, the search still happened
— nobody learns that a string is absent by any other means — and the brief has not said what it was.

**Re-running the search is zero defense. It will agree with itself.** The rule that applies is this
project's own, written on 2026-05-08 for tests and never pointed at claims until now
(`feedback_negative_tests_pass_for_wrong_reason.md`): a negative result cannot tell *the thing is
absent* from *the check could not see it*.

**So before you build on one: BROADEN the pattern until it returns hits, and READ them.** `CLAUDE.md`
— *"A value has more than one spelling ... Grep for the thing, not for one of its forms."* The
instance that forced this row into existence: a two-word search returned exactly one hit, the comment
making the claim, while the suite it denied had been building real repositories all along under a
spelling those two words cannot match."""


def annotation(findings):
    if not findings:
        return ""
    out = [HEAD, ""]
    if any(f["check"] not in ("capability", "selector") for f in findings):
        out += [BODY, ""]
    for kind, title in (("quotation", "Presented as a quotation, and NOT found in the file named"),
                        ("restatement", "Restated from a source this brief names — read the "
                                        "source, not this"),
                        ("capability", "SOURCED — and the source does NOT establish "
                                       "what the sentence says"),
                        ("selector", "A NEGATIVE claim, resting on a search for ONE SPELLING "
                                     "of the thing"),
                        ("provenance", "Asserted without a source in this brief")):
        rows = [f for f in findings if f["check"] == kind]
        if not rows:
            continue
        out.append("**%s:**" % title)
        out.append("")
        if kind == "capability":
            out.append(CAPABILITY_LEAD)
            out.append("")
        if kind == "selector":
            out.append(SELECTOR_LEAD)
            out.append("")
        for f in rows:
            out.append("- “%s”" % f["text"])
            out.append("  (%s)" % f["detail"])
        out.append("")
    return "\n".join(out).rstrip() + "\n"


def annotate(prompt, repo=""):
    f = review(prompt, repo)
    if not f:
        return prompt, f
    return prompt.rstrip("\n") + "\n\n" + annotation(f), f


def report(findings, name="", out=sys.stderr):
    if not findings:
        print("  provenance:  nothing asserted without a source%s"
              % (" in %s's brief" % name if name else ""), file=out)
        return
    # The counts are separated because they are opposite findings, and one line
    # calling both "carry no source" would be this file committing the failure its
    # own fourth check exists to catch.
    cap = sum(1 for f in findings if f["check"] == "capability")
    sel = sum(1 for f in findings if f["check"] == "selector")
    rest = len(findings) - cap - sel
    parts = []
    if rest:
        parts.append("%d statement(s) carry no source" % rest)
    if cap:
        parts.append("%d cite a source that does not establish what they say" % cap)
    if sel:
        parts.append("%d rest on a search narrower than the claim" % sel)
    print("  provenance:  %s; the brief now says so to the agent"
          % "; ".join(parts), file=out)
    for f in findings:
        print("    [%-11s] %s" % (f["check"], f["text"][:96]), file=out)
        print("                  %s" % f["detail"], file=out)


def main(argv):
    ap = argparse.ArgumentParser(
        description="Separate what a brief cites from what it asserts. Never refuses anything.")
    ap.add_argument("target", help="a brief file, or a spawn payload JSON")
    ap.add_argument("--repo", default="", help="repository the brief's paths are relative to; "
                                               "quotations are checked against it")
    ap.add_argument("--annotate", action="store_true", help="print the annotated brief")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args(argv)

    with open(a.target, encoding="utf-8") as fh:
        raw = fh.read()
    if a.target.endswith(".json"):
        try:
            raw = json.loads(raw)["prompt"]
        except (ValueError, KeyError, TypeError):
            pass
    findings = review(raw, a.repo)
    if a.json:
        print(json.dumps({"findings": findings, "annotation": annotation(findings)}, indent=2))
    elif a.annotate:
        print(annotate(raw, a.repo)[0], end="")
    else:
        report(findings, out=sys.stdout)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
