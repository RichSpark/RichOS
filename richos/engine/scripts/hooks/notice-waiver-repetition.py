#!/usr/bin/env python3
"""notice-waiver-repetition.py — A HATCH USED OVER AND OVER IS A BROKEN GUARD.

===========================================================================
THE DEFECT THIS FILE EXISTS TO END
===========================================================================
Measured on 2026-09-02, in ONE day, in one repository:

    .claude/state/resume-acks.log       228 entries
    .claude/state/ceo-todos-defers.log   23 entries

251 waivers, every one of them written by the lead, every one individually
justified and true on its own terms.

The 228 are the diagnostic ones. guard-resume-isolation.sh resolved a
teammate's liveness from the session roster, and the roster NEVER lists a
background isolation agent. So every message to a live background agent
tripped the guard — a 100% false-positive rate on the normal case — and every
one was answered with a `resume-ack:` line. That went on for weeks. The guard
was then fixed in about an hour, and only after the CEO asked how many more
times he would have to hear about it.

Nothing had decayed. Zero guards have ever been deleted from this engine; 47
guard and notice scripts are present and wired. The engine's own doctrine
already names the failure — habitual waiving is how a defense decays into a
formality — written in a file the lead reads, while he was doing it 228 times.

SO THE DEFECT IS NOT COVERAGE AND IT IS NOT DECAY. A waiver costs nothing, so
it is always cheaper than the fix. Nothing ever put the price of the 229th
waiver next to the price of the fix.

===========================================================================
WHY A REPORT IS THE MECHANISM HERE, AND NOT A CONSOLATION PRIZE
===========================================================================
The standing bar in this engine is that a deliverable which only reports,
warns or counts has FAILED, unless the report itself is the mechanism. This
one is, and the argument is specific rather than convenient.

The waiver ledgers ALREADY CONTAINED THE ENTIRE PROOF. 226 distinct entries,
122 distinct recipients, 27 distinct days, all in one file, all readable by
`wc -l`. Nothing was hidden and nothing was missing. What was missing was that
NO CODE AND NO HUMAN EVER READ THAT FILE. The escape hatches write; nobody
reads. So the intervention that changes the outcome is not a new rule — the
rule exists and was ignored — it is the arithmetic, performed automatically,
on the operator's screen, at the end of the turn in which he is about to write
waiver number 229.

WHY IT MUST NOT BLOCK, stated as a design decision rather than timidity. The
act it would block is the WAIVER, and in every case measured here the waiver
was the CORRECT act: the teammate really was alive, the message really had to
go. Refusing it would punish the operator for a DIFFERENT guard's false
positive and wedge the session — which is exactly guard-inflight-notify.sh's
own reasoning one event over ("blocking a land until another party acts is how
a guard wedges a session"; THE SEND IS ENFORCED, THE ACK IS SURFACED). Worse,
a blocking waiver-watcher would itself need an escape hatch, and by this
file's own thesis that hatch would be waived 228 times. A defense whose
failure mode is to recreate the defect it was built for is not a defense.

AND SO THIS ONE HAS NO ESCAPE HATCH AT ALL. There is no marker, no config key,
no `waiver-repetition-ack:` line. An opt-out on the thing that watches opt-outs
is self-evidently absurd, and its absence is the reason this notice cannot go
the way of the 228.

===========================================================================
WHAT COUNTS AS A DEFECT — the rule, and the real numbers behind it
===========================================================================
A hatch used ONCE is a considered exception and this file says nothing about
it, ever. The question is when repetition stops being a series of exceptions
and starts being a guard with a false-positive class. The rule is:

    THE SAME REASON, GIVEN FOR A SET OF INDEPENDENT ACTS, AT LEAST 3 TIMES,
    STILL HAPPENING NOW.

Independence has two observable forms in these ledgers — different days, or
different subjects. Either way the operator restated the same justification
for a case the guard had already gotten wrong at least once, and chose the
waiver over the fix again.

Every constant below was read off the real ledgers, not picked round.

MIN_CLASS = 3.
    Pooling every class in the six ledgers that exist today, class sizes run
    1 (x38), 2 (x5), 3 (x3), 5, 6, 7, 8, 9, 15, 16, 20, 26, 35, 36, 42, 74,
    79, 86, 88, 143. There is no gap at the bottom to hide behind, so the
    floor is argued rather than fitted: one is an exception, two is that
    exception recurring, and the THIRD is the first use at which the operator
    can no longer claim surprise — the guard demonstrably has a CLASS of cases
    it gets wrong, and writing a fourth waiver is a choice to keep paying.
    Dropping to 2 would add 5 classes totalling 10 entries to a report whose
    real content is 800+; raising to 4 would delete a live ceo-todos-defers
    class the data shows is real. 3 also keeps the required quiet side: one
    considered waiver, and two, say nothing.

JACCARD = 0.35, single-linkage, on identifier-stripped content words.
    Exact text matching is useless here and that is measured: the 228
    resume-acks contain 226 DISTINCT strings, and the largest byte-identical
    group is 7. The same reason gets retyped in different words every time.
    Sensitivity, measured on resume-acks.log (226 unique entries):

        0.25 ->  6 classes, largest 221   (over-merges: swallows the
                                           in-flight-land-notice class, which
                                           is a different guard interaction)
        0.35 -> 16 classes, largest  88   CHOSEN
        0.45 -> 25 classes, largest  53   (fragments one class into four)
        0.55 -> 29 classes, largest  53

    THE VERDICT IS THE SAME AT ALL FOUR. Every ledger flagged at 0.35 is
    flagged at 0.25, 0.45 and 0.55; only the sizes move. So the constant is
    not load-bearing, and notice-waiver-repetition.test.sh asserts exactly
    that, so a future edit cannot quietly make it load-bearing.

ACTIVE_DAYS = 14.
    A guard that HAS BEEN FIXED must stop being reported, or this notice
    becomes the muted line the engine already refused to ship. So a class
    counts only while it is still being used. The window is where it is
    because of a hole in the real data: sorting every qualifying class by days
    since its last use gives

        0, 0, 0, 0, 1, 1, 2, 2, 3, 4, 4, 5, 7, 8,  ... then nothing until 29,
        then 46, 50, 110, 115, 115, 116, 116, 118, 128, 128, 128, 129, 131,
        131, 132, 132, 132, 133

    Live work and dead history are separated by a 21-day void. ANY window from
    9 to 28 days produces the identical live set on today's data, which is the
    definition of a constant that is not tuned. 14 sits in the middle of that
    void.

===========================================================================
HOW THE LEDGERS ARE FOUND — derived from the guards, checked against disk
===========================================================================
There is NO TYPED LIST OF LEDGERS in this file. A typed inventory is precisely
the thing that falls behind: scripts/lib/registered-hooks.sh exists because a
typed list of guards drifted twice in two days, and scripts/lib/sandbox-
completeness.sh exists because a typed list of sandbox files left Layer K green
over a scanner that never ran.

But sandbox-completeness.sh also states the counter-argument, and it applies
here: A SCAN THAT STOPS MATCHING PRODUCES A SHORTER LIST, SILENTLY, and a
shorter list makes every report cleaner. So the discovery is TWO-SIDED, and
neither side can shorten the other:

  SOURCE SIDE — every .sh/.py under the engine's scripts/ is read for an
    APPEND SITE (a `>>` redirect, an append_log call, or a Python "a" open)
    whose target resolves to a *.log / *.jsonl basename, locally first and
    then engine-wide for the shared `*_LOG_NAME` convention. An append site is
    an ESCAPE-HATCH ledger when hatch vocabulary (ack / waive / exempt /
    bypass / defer / opt-out / escape hatch / override / allowed) appears in
    its immediate context. That vocabulary is a list of ENGLISH WORDS, not a
    list of files — it generalizes to a hatch that does not exist yet, which
    is the whole difference. Finding ZERO hatch ledgers is a hard failure, not
    an all-clear.

  DISK SIDE — the entity's .claude/state and every teams directory are listed.
    For the question this file asks, the disk side is COMPLETE BY
    CONSTRUCTION: a hatch that has never been used has no file, and a hatch
    that has never been used cannot have been used repeatedly. Any *.log /
    *.jsonl found there that the source side does not claim is reported as
    UNATTRIBUTED rather than dropped — that is the line where a broken deriver
    becomes visible instead of quiet.

WHAT THIS CANNOT SEE, said here rather than in a postmortem. It reads the
ledgers the hatches write. A hatch that writes NOTHING is invisible to it, and
so is a guard that is being satisfied by theater rather than by a marker. It
also cannot judge whether a reason is TRUE; it only observes that the same one
was given for independent acts and that the guard was not changed.

WHERE IT IS KNOWN TO BE WRONG TODAY, and in which direction. On the engine as
it stands, `definition-drift.log` is classified as a hatch and is not one: it
records agent creations, and the hatch vocabulary appears near its append site
because the REAL hatch, definition-drift-acks.log, is written eleven lines away
in the same guard. The consequence is that four ancient rows are read and
reported as a hatch that has never fired, which is visible in the lint output
and costs one line. That is the direction the classifier is built to fail in:
an over-classified record is a line the operator can dismiss, while an
under-classified hatch is a defect nobody is told about. The engine has already
ruled on this trade — degrading toward noise is recoverable by an operator who
can read it; degrading toward silence rebuilds the defect.
"""

import argparse
import collections
import datetime
import json
import os
import re
import sys

# --------------------------------------------------------------------------
# constants — every one of them argued in the module docstring above
# --------------------------------------------------------------------------
MIN_CLASS = 3
JACCARD = 0.35
ACTIVE_DAYS = 14
# DERIVED FROM ACTIVE_DAYS, not chosen beside it. The window's own half: inside
# it a class is what is happening this week, outside it a class is what was
# happening earlier in the same window. Used only to ORDER the report, never to
# include or exclude — that is ACTIVE_DAYS' job and this must not become a
# second, quieter one. Argued at the sort site.
RECENCY_BUCKET = max(1, ACTIVE_DAYS // 2)
MAX_LINES = 20000          # bound the work; a Stop hook has 25 seconds
CONTEXT_LINES = 10         # lines either side of an append site scanned for vocabulary

# ENGLISH WORDS, NOT FILENAMES. This is the one typed list in the file and it
# is typed deliberately: a vocabulary generalizes to a hatch that does not
# exist yet, which is exactly what a list of ledger names cannot do.
#
# `\backs?\b` and `\back_` rather than a bare `\back`: guard-worktree-removal.sh
# names its reason ACK_REASON at the append site and has to match, while the
# word BACKGROUND appears next to half the append sites in this engine and must
# not. A prefix match would have classified every one of them as a hatch.
#
# `(s|ed|ing)?` rather than `s?`: the append site of the most-used hatch in this
# engine says "Every red workflow was acked with a real reason" and carries an
# `acked_before` count two lines up, and NONE of that matched a pattern that
# required the word to end at "ack". The past tense is the tense a guard writes
# its log line in, which is the one place this vocabulary has to work. It stays
# a suffix list rather than a prefix match for the reason the docstring gives —
# `\back\w*` would swallow BACKGROUND, which sits beside half the append sites
# in this engine. Measured across the whole engine when it was widened: one
# ledger promoted (ci-red-acks.log), none demoted.
HATCH_VOCAB = re.compile(
    r"\back(s|ed|ing)?\b|\back_|acknowledg|waiv|exempt|bypass|defer|opt-?out"
    r"|escape hatch|override|marker|live prompt line|audit trail"
    r"|allowed \+ logged|allowed and logged",
    re.I,
)

STOPWORDS = set("""
a an the is are was were be been being to of in on it its this that and or for by as at
no not any all only from into so which who whose than then there their they them he she
his her i you your our we us but if while when where what has have had do does did other
same own each more most very can will would should could may might must with
""".split())

_TS_RE = re.compile(r"^(\d{4}-\d{2}-\d{2})T\d{2}:\d{2}:\d{2}")
_KV_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
_MARKER_RE = re.compile(r"^[a-z][a-z0-9-]*:\s*")
_SUBJECT_RE = re.compile(r"^(to|name|teammate|agent|worktree)=(.*)$")
# Most specific first. A ledger row often carries BOTH `agent=zach` (the role)
# and `name=zach-opus-waiv1` (the instance); taking the role would collapse
# thirteen independent dispatches into five and hide the very independence the
# verdict turns on.
_SUBJECT_PRIORITY = ("to", "teammate", "name", "worktree", "agent")
# The `(?<![$\w])` matters: run-all-tests.sh writes "$LOG_DIR/$i.log", and
# without it the scan invents a ledger called i.log out of a loop variable.
_LEDGER_NAME_RE = re.compile(r"(?<![$\w])([a-z0-9][a-z0-9._-]*\.(?:log|jsonl))")
_ASSIGN_RE = re.compile(
    r"""^\s*(?:local\s+|export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+)$""")
_DEF_RE = re.compile(r"^\s*def\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(")
# THE SHELL -> ENVIRONMENT -> PYTHON HOP. Five guards in this engine embed a
# Python program in a shell heredoc and hand it the ledger path through the
# environment, so the append site reads `open(log_path, "a")` while the only
# place the name is spelled is a shell assignment hundreds of lines above:
#
#     guard-ci-red-lands.sh:224  ACK_LOG="${CI_RED_ACK_LOG:-$HOME/...}"
#     guard-ci-red-lands.sh:608  log_path = os.environ["ACK_LOG"]
#     guard-ci-red-lands.sh:694  with open(log_path, "a", ...) as f:
#
# The resolver already followed a variable to a variable, but only through the
# SHELL spelling `$VAR` — so the chain broke at the one link that changes
# language, and ci-red-acks.log was never named by the scan at all. It is the
# same defect the module docstring warns about: a resolver that cannot follow a
# hop drops the site with `continue`, which shortens the census in silence.
_ENVREF_RE = re.compile(
    r"""os\.environ\s*\[\s*["']([A-Za-z_][A-Za-z0-9_]*)["']\s*\]"""
    r"""|os\.environ\.get\(\s*["']([A-Za-z_][A-Za-z0-9_]*)["']"""
    r"""|os\.getenv\(\s*["']([A-Za-z_][A-Za-z0-9_]*)["']""")


def _var_refs(text):
    """Every name this text reads a value OUT of, in either language.

    Shell `$VAR` / `${VAR}` first, because that is the common case and the
    order decides ties; then the environment reads a heredoc'd Python program
    uses to receive what the shell around it computed.
    """
    out = list(re.findall(r"\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?", text))
    for groups in _ENVREF_RE.findall(text):
        for g in groups:
            if g:
                out.append(g)
    return out


# ==========================================================================
# normalization
# ==========================================================================
def normalize_tokens(text):
    """The content words of a reason, with every identifier removed.

    Identifiers are stripped because they are what makes two statements of the
    same reason look different: a path, a SHA, a teammate name and a count all
    change on every use while the reason does not. What remains is the claim.
    """
    s = (text or "").lower()
    s = re.sub(r"https?://\S+", " ", s)
    s = re.sub(r"[/~][\w./~+-]{3,}", " ", s)                       # paths
    s = re.sub(r"\b[0-9a-f]{7,}\b", " ", s)                        # shas / ids
    s = re.sub(r"\b[a-z]+-(?:opus|sonnet|fable|haiku)-[a-z0-9]+\b", " ", s)
    s = re.sub(r"\bagent-[a-z0-9]{6,}\b", " ", s)
    s = re.sub(r"\b\d+\b", " ", s)
    s = re.sub(r"[^a-z]+", " ", s)
    return frozenset(t for t in s.split() if t not in STOPWORDS and len(t) > 2)


def parse_entry(line):
    """(day, subject, reason) from one ledger line, tab- or JSON-shaped."""
    line = line.rstrip("\n")
    if not line.strip():
        return None
    stripped = line.lstrip()
    if stripped.startswith("{"):
        try:
            obj = json.loads(stripped)
        except Exception:
            return None
        if not isinstance(obj, dict):
            return None
        day = ""
        for k in ("ts", "timestamp", "at", "time", "date", "when"):
            v = obj.get(k)
            if isinstance(v, str):
                m = _TS_RE.match(v)
                day = m.group(1) if m else v[:10]
                break
        subject = ""
        for k in _SUBJECT_PRIORITY:
            v = obj.get(k)
            if isinstance(v, str) and v:
                subject = v
                break
        reason = ""
        for k in ("reason", "why", "justification", "note"):
            v = obj.get(k)
            if isinstance(v, str) and v:
                reason = v
                break
        return (day, subject, reason)

    fields = line.split("\t")
    m = _TS_RE.match(fields[0])
    day = m.group(1) if m else ""
    found = {}
    rest = []
    for f in fields[1:] if m else fields:
        sm = _SUBJECT_RE.match(f)
        if sm:
            found.setdefault(sm.group(1), sm.group(2))
            continue
        if _KV_RE.match(f):
            continue
        rest.append(f)
    subject = ""
    for k in _SUBJECT_PRIORITY:
        if found.get(k):
            subject = found[k]
            break
    reason = " ".join(x for x in rest if x.strip())
    reason = _MARKER_RE.sub("", reason.strip())
    return (day, subject, reason)


# ==========================================================================
# clustering — single linkage over Jaccard on the token sets
# ==========================================================================
def cluster(signatures, threshold):
    """Indices grouped so that any two entries linked by >= threshold overlap
    land in one group. Single linkage, deliberately: a reason restated a dozen
    ways forms a chain, and pairwise-to-a-representative clustering splits that
    chain by the accident of which entry it met first.

    IDENTICAL SIGNATURES ARE LINKED ONCE (2026-10-22). Two entries with the same token set
    overlap 1.0, so they always share a group; the pairwise pass runs over the distinct sets
    only and every entry takes its set's group. ceo-todos-defers.log's 4,946 lines carry 858
    distinct sets, and comparing every pair of lines was most of the Stop hook's 6-9 CPU-s a
    turn (Hank's after-measurement). The groups, their members and their order are unchanged:
    each group lists its entries in index order, groups in order of their first entry."""
    if threshold > 1.0:
        return _cluster_distinct(signatures, threshold)   # nothing links, not even equal sets
    distinct = {}
    for sig in signatures:
        if sig:
            distinct.setdefault(sig, len(distinct))
    keys = list(distinct)
    group_of = {}
    for members in _cluster_distinct(keys, threshold):
        for k in members:
            group_of[keys[k]] = members[0]
    groups = collections.defaultdict(list)
    for i, sig in enumerate(signatures):
        groups[("set", group_of[sig]) if sig else ("entry", i)].append(i)
    return list(groups.values())


def _cluster_distinct(signatures, threshold):
    """cluster()'s single-linkage pass itself, over signatures that are all distinct."""
    n = len(signatures)
    parent = list(range(n))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    index = collections.defaultdict(list)
    for i, sig in enumerate(signatures):
        for tok in sig:
            index[tok].append(i)

    for i, sig in enumerate(signatures):
        if not sig:
            continue
        candidates = set()
        for tok in sig:
            candidates.update(index[tok])
        for j in candidates:
            if j <= i:
                continue
            other = signatures[j]
            if not other:
                continue
            if find(i) == find(j):
                continue
            if len(sig & other) / float(len(sig | other)) >= threshold:
                parent[find(i)] = find(j)

    groups = collections.defaultdict(list)
    for i in range(n):
        groups[find(i)].append(i)
    return list(groups.values())


# ==========================================================================
# discovery — SOURCE SIDE
# ==========================================================================
def source_files(engine_root):
    """The engine scripts discover_from_source reads, sorted."""
    files = []
    for dirpath, _dirs, filenames in os.walk(os.path.join(engine_root, "scripts")):
        for fn in sorted(filenames):
            if not (fn.endswith(".sh") or fn.endswith(".py")):
                continue
            if fn.endswith(".test.sh") or fn.endswith(".mutation.sh"):
                continue
            if fn.endswith(".selftest.sh"):
                continue
            files.append(os.path.join(dirpath, fn))
    files.sort()
    return files


def discover_from_source(engine_root):
    """{ledger basename: {"writers": [...], "hatch": bool, "evidence": str}}

    Derived by reading the guards. Never a typed list; see the module
    docstring for why, and for why the disk side exists beside it.
    """
    scripts_dir = os.path.join(engine_root, "scripts")
    if not os.path.isdir(scripts_dir):
        return {}, ["scripts/ not found under %s" % engine_root]

    files = source_files(engine_root)

    # Pass 1 — variable -> ledger basename, per file and engine-wide. The
    # engine-wide map exists for exactly one real shape: guard-ceo-ask-first.sh
    # appends to "$LOG_DIR/$CA_DEFER_LOG_NAME", and CA_DEFER_LOG_NAME is
    # assigned in scripts/lib/ceo-asks.sh. Local wins, because LOG_FILE means a
    # different ledger in every guard that uses it.
    local_vars = {}
    local_fns = {}
    global_vars = {}
    global_fns = {}
    global_conflicts = set()
    deferred = {}
    texts = {}
    for path in files:
        try:
            with open(path, "r", errors="replace") as fh:
                texts[path] = fh.read().splitlines()
        except Exception:
            continue
    for path in files:
        lines = texts.get(path)
        if lines is None:
            continue
        # Functions that RETURN a ledger path, so a Python append site of the
        # shape `path = inflight.waiver_ledger_path(teams)` /
        # `open(path, "a")` resolves. inflight-waivers.jsonl is written that
        # way and nothing else in the engine names it at its append site.
        fnmap = {}
        cur_fn = ""
        cur_left = 0
        for line in lines:
            dm = _DEF_RE.match(line)
            if dm:
                cur_fn = dm.group(1)
                cur_left = 8
                continue
            if cur_fn and cur_left > 0:
                cur_left -= 1
                nm = _LEDGER_NAME_RE.search(line)
                if nm:
                    fnmap[cur_fn] = nm.group(1)
                    cur_fn = ""
        local_fns[path] = fnmap
        for k, v in fnmap.items():
            global_fns.setdefault(k, v)

        vmap = {}
        pending = []
        for line in lines:
            am = _ASSIGN_RE.match(line)
            if not am:
                continue
            rhs = am.group(2)
            nm = _LEDGER_NAME_RE.search(rhs)
            if nm:
                vmap[am.group(1)] = nm.group(1)
                continue
            hit = ""
            for fn, led in list(fnmap.items()) + list(global_fns.items()):
                if re.search(r"\b%s\s*\(" % re.escape(fn), rhs):
                    hit = led
                    break
            if hit:
                vmap[am.group(1)] = hit
            else:
                pending.append((am.group(1), rhs))
        local_vars[path] = vmap
        deferred[path] = pending
        for k, v in vmap.items():
            if k in global_vars and global_vars[k] != v:
                global_conflicts.add(k)
            global_vars[k] = v
    for k in global_conflicts:
        global_vars.pop(k, None)

    # VARIABLE-OF-A-VARIABLE, to a fixpoint. scripts/ceo-ruled-exempt.sh does
    # LOG="$LOG_DIR/$CR_EXEMPT_LOG_NAME" and appends to "$LOG", while
    # CR_EXEMPT_LOG_NAME is assigned in scripts/lib/ceo-ruled.sh — two files
    # and two hops from the append site to the name. One pass in filename
    # order resolves it only by luck, so it is iterated until nothing more
    # resolves. Three rounds is the cap because a fourth has never been
    # needed and an unbounded loop in a Stop hook is its own defect.
    for _round in range(3):
        progressed = False
        for path, pending in deferred.items():
            vmap = local_vars.get(path, {})
            still = []
            for name, rhs in pending:
                hit = ""
                for var in _var_refs(rhs):
                    if var in vmap:
                        hit = vmap[var]
                        break
                    if var in global_vars:
                        hit = global_vars[var]
                        break
                if hit:
                    vmap[name] = hit
                    progressed = True
                else:
                    still.append((name, rhs))
            deferred[path] = still
        if not progressed:
            break

    # Pass 2 — append sites.
    ledgers = {}
    notes = []
    for path in files:
        lines = texts.get(path, [])
        rel = os.path.relpath(path, engine_root)
        for i, line in enumerate(lines):
            if not _is_append_site(line):
                continue
            name = _resolve_ledger(line, local_vars.get(path, {}), global_vars)
            if not name:
                continue
            context = "\n".join(
                lines[max(0, i - CONTEXT_LINES):i + CONTEXT_LINES + 1])
            hatch = bool(HATCH_VOCAB.search(context))
            rec = ledgers.setdefault(
                name, {"writers": [], "hatch": False, "evidence": ""})
            entry = "%s:%d" % (rel, i + 1)
            if entry not in rec["writers"]:
                rec["writers"].append(entry)
            if hatch and not rec["hatch"]:
                rec["hatch"] = True
                rec["evidence"] = entry

    if not ledgers:
        notes.append(
            "read %d engine scripts and found NO append site at all — the "
            "discovery scan is broken, not the engine" % len(files))
    elif not any(r["hatch"] for r in ledgers.values()):
        notes.append(
            "found %d appended ledgers and classified NONE of them as an "
            "escape hatch — the vocabulary test is broken, not the engine"
            % len(ledgers))
    return ledgers, notes


def _is_append_site(line):
    if ">>" in line:
        return True
    if re.search(r"\bappend_log\b|\bappend_line\b", line):
        return True
    if re.search(r"""open\([^)]*,\s*["']a""", line):
        return True
    return False


def _resolve_ledger(line, vmap, gmap):
    m = _LEDGER_NAME_RE.search(line)
    if m:
        return m.group(1)
    for var in _var_refs(line):
        if var in vmap:
            return vmap[var]
    for var in _var_refs(line):
        if var in gmap:
            return gmap[var]
    for var in re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)\b", line):
        if var in vmap:
            return vmap[var]
    return ""


# ==========================================================================
# discovery — DISK SIDE
# ==========================================================================
def discover_on_disk(state_dirs):
    found = collections.defaultdict(list)
    for d in state_dirs:
        if not os.path.isdir(d):
            continue
        try:
            names = sorted(os.listdir(d))
        except Exception:
            continue
        for fn in names:
            if not (fn.endswith(".log") or fn.endswith(".jsonl")):
                continue
            p = os.path.join(d, fn)
            if os.path.isfile(p):
                found[fn].append(p)
    return found


def state_dirs_for(entity_root, teams_root, config_root=None):
    """Every directory a hatch ledger is written to — THREE of them, not two.

    THE MACHINE-WIDE ONE WAS MISSING, and the docstring above claimed the disk
    side was "COMPLETE BY CONSTRUCTION" while it was not. Measured on
    2026-09-14: guard-ci-red-lands.sh writes
    "${CI_RED_ACK_LOG:-$HOME/.claude/state/ci-red-acks.log}" — outside every
    repository, deliberately, because ONE operator lands into several
    repositories and the habit is his, not any one repository's. That file held
    93 acks across two repositories, 35 of them for a single workflow, and this
    watcher could not see one of them: not as a hatch, not as unused, not even
    on the unattributed line. A ledger nobody looks for is indistinguishable
    from a ledger nobody writes, and those are the two answers this whole file
    exists to keep apart.

    DERIVED FROM CLAUDE_CONFIG_DIR, never from $HOME directly, and that is the
    same expression main() already uses to find the team directories. It
    matters for the suite as much as for production: waiver-repetition.test.sh
    sandboxes CLAUDE_CONFIG_DIR precisely because reading the operator's real
    machine made a case go red with a count it had never written. Reaching for
    $HOME here would have re-opened that hole one directory over.
    """
    dirs = []
    if entity_root:
        dirs.append(os.path.join(entity_root, ".claude", "state"))
    if config_root is None:
        config_root = (os.environ.get("CLAUDE_CONFIG_DIR")
                       or os.path.expanduser("~/.claude"))
    if config_root:
        machine_state = os.path.join(config_root, "state")
        if machine_state not in dirs:
            dirs.append(machine_state)
    if teams_root and os.path.isdir(teams_root):
        try:
            for d in sorted(os.listdir(teams_root)):
                p = os.path.join(teams_root, d)
                if os.path.isdir(p):
                    dirs.append(p)
        except Exception:
            pass
    return dirs


# ==========================================================================
# analysis
# ==========================================================================
def analyze_ledger(name, paths, today, jaccard=JACCARD):
    entries = []
    seen = set()
    duplicates = 0
    for p in paths:
        try:
            with open(p, "r", errors="replace") as fh:
                lines = fh.read().splitlines()
        except Exception:
            continue
        if len(lines) > MAX_LINES:
            lines = lines[-MAX_LINES:]
        for line in lines:
            parsed = parse_entry(line)
            if parsed is None:
                continue
            # Byte-identical repeats are the double-fire artifact install.sh
            # documents (one tool event, two identical rows). Counting them
            # would inflate every class by whatever the host did twice.
            if line in seen:
                duplicates += 1
                continue
            seen.add(line)
            entries.append(parsed)

    result = {
        "ledger": name,
        "paths": paths,
        "total": len(entries),
        "duplicate_lines": duplicates,
        "has_subject_field": any(e[1] for e in entries),
        "classes": [],
    }
    if not entries:
        return result

    signatures = [normalize_tokens(e[2]) for e in entries]
    non_empty = [i for i, s in enumerate(signatures) if s]
    empty = [i for i, s in enumerate(signatures) if not s]
    groups = [[non_empty[j] for j in g]
              for g in cluster([signatures[i] for i in non_empty], jaccard)]
    # Entries whose reason normalizes to nothing gave NO reason at all — a
    # hash, a bare token, an empty field. They are one class, and their being
    # one class is itself the finding: a hatch discharged with no stated
    # justification is a hatch discharged mechanically.
    if empty:
        groups.append(empty)

    for g in groups:
        days = sorted({entries[i][0] for i in g if entries[i][0]})
        subjects = sorted({entries[i][1] for i in g if entries[i][1]})
        sample = ""
        for i in g:
            if entries[i][2].strip():
                sample = entries[i][2].strip()
                break
        if not sample:
            sample = "(no stated reason)"
        idle = _days_since(days[-1], today) if days else None
        result["classes"].append({
            "size": len(g),
            "days": len(days),
            "first_day": days[0] if days else "",
            "last_day": days[-1] if days else "",
            "idle_days": idle,
            "subjects": len(subjects),
            "sample": sample,
        })
    result["classes"].sort(key=lambda c: (-c["size"], c["sample"]))
    return result


def _days_since(day, today):
    try:
        d = datetime.date(*[int(x) for x in day.split("-")])
    except Exception:
        return None
    return (today - d).days


def is_repeated(cls, has_subject_field):
    """The whole verdict, in one place, so a reader can check it against the
    docstring without tracing call sites."""
    if cls["size"] < MIN_CLASS:
        return False
    if cls["idle_days"] is None or cls["idle_days"] > ACTIVE_DAYS:
        return False
    independent = cls["days"] >= 2 or cls["subjects"] >= 2
    if not independent and has_subject_field:
        return False
    if not independent and not has_subject_field:
        # The ledger records no subject at all, so "different subjects" is
        # unobservable here and the day test is the only evidence available.
        # Saying so beats inventing agreement.
        return False
    return True


# WHAT DID NOT CHANGE IS NOT READ AGAIN (2026-10-22). The Stop hook re-read every engine script
# and re-clustered every ledger at every turn end, 6-9 CPU-s a turn (Hank's after-measurement,
# richos-hq docs/research/2026-10-22-mac-system-cpu-after.md), while between two turns most of
# them had not changed by a byte. So the source scan and each ledger's analysis are remembered
# beside the machine's other state, keyed by every file they read (path, inode, size, mtime in
# nanoseconds) and by this file itself, and reused only while all of those are unchanged. A
# ledger that grew is analyzed again in full; nothing is ever merged into an old answer. The
# only thing that depends on the day, a class's idle days, is worked out fresh each time.
# One file per engine and seat, so two sessions seated in different repositories do not keep
# replacing each other's answers.
MEMO_NAME = "waiver-repetition-memo-%s.json"


def _stamp(paths):
    out = []
    for p in paths:
        try:
            st = os.stat(p)
            out.append([p, st.st_ino, st.st_size, st.st_mtime_ns])
        except OSError:
            out.append([p, None])
    return out


class Memo:
    def __init__(self, engine_root, entity_root, teams_root, config_root=None):
        import hashlib
        root = config_root or os.environ.get("CLAUDE_CONFIG_DIR") or os.path.expanduser("~/.claude")
        seat = hashlib.sha256(json.dumps([engine_root, entity_root, teams_root]).encode()).hexdigest()[:16]
        self.path = os.path.join(root, "state", MEMO_NAME % seat) if root else ""
        self.entries, self.used, self.dirty = {}, {}, False
        try:
            with open(self.path) as fh:
                loaded = json.load(fh)
            if isinstance(loaded, dict) and isinstance(loaded.get("entries"), dict):
                self.entries = loaded["entries"]
        except (OSError, ValueError):
            pass

    def get(self, key, paths, compute):
        """compute()'s answer, or the remembered one while every file in `paths` is unchanged."""
        name = json.dumps([key, _stamp(list(paths) + [os.path.abspath(__file__)])])
        if name in self.entries:
            self.used[name] = self.entries[name]
            return json.loads(json.dumps(self.entries[name]))
        answer = compute()
        self.used[name] = json.loads(json.dumps(answer))
        self.dirty = True
        return answer

    def save(self):
        if not self.path or not (self.dirty or set(self.entries) != set(self.used)):
            return
        tmp = "%s.%d.tmp" % (self.path, os.getpid())
        try:
            os.makedirs(os.path.dirname(self.path), exist_ok=True)
            with open(tmp, "w") as fh:
                json.dump({"schema": 1, "entries": self.used}, fh)
            os.replace(tmp, self.path)
        except OSError:
            try:
                os.unlink(tmp)
            except OSError:
                pass


def run(engine_root, entity_root, teams_root, today, jaccard=JACCARD):
    memo = Memo(engine_root, entity_root, teams_root)
    source, notes = memo.get(["source", engine_root], source_files(engine_root),
                             lambda: list(discover_from_source(engine_root)))
    dirs = state_dirs_for(entity_root, teams_root)
    on_disk = discover_on_disk(dirs)

    report = {
        "engine_root": engine_root,
        "entity_root": entity_root,
        "state_dirs": dirs,
        "today": today.isoformat(),
        "min_class": MIN_CLASS,
        "jaccard": jaccard,
        "active_days": ACTIVE_DAYS,
        "broken": [],
        "hatches_declared": sorted(k for k, v in source.items() if v["hatch"]),
        "records_declared": sorted(k for k, v in source.items() if not v["hatch"]),
        "unattributed_on_disk": [],
        "unused": [],
        "ledgers": [],
        "flagged": [],
    }
    report["broken"].extend(notes)
    if not dirs:
        report["broken"].append("no state directory resolved — nothing to read")

    hatch_names = set(report["hatches_declared"])
    known = set(source.keys())
    for fn in sorted(on_disk):
        if fn not in known:
            report["unattributed_on_disk"].append(fn)

    for name in sorted(hatch_names):
        paths = on_disk.get(name, [])
        if not paths:
            report["unused"].append(name)
            continue
        analysis = memo.get(["ledger", name, jaccard, MAX_LINES], paths,
                            lambda: analyze_ledger(name, paths, today, jaccard))
        for c in analysis["classes"]:
            c["idle_days"] = _days_since(c["last_day"], today) if c["last_day"] else None
        analysis["writers"] = source[name]["writers"]
        analysis["evidence"] = source[name]["evidence"]
        repeated = [c for c in analysis["classes"]
                    if is_repeated(c, analysis["has_subject_field"])]
        analysis["repeated"] = repeated
        report["ledgers"].append(analysis)
        if repeated:
            report["flagged"].append({
                "ledger": name,
                "guard": _guard_of(source[name]["writers"]),
                "writers": source[name]["writers"],
                "total": analysis["total"],
                "has_subject_field": analysis["has_subject_field"],
                "largest": repeated[0],
                "repeated_classes": len(repeated),
                "repeated_entries": sum(c["size"] for c in repeated),
            })

    # WHAT IS STILL HAPPENING, THEN HOW BIG. The one-liner names three and
    # counts the rest, so this ordering decides what the operator is actually
    # told — and ordering by size alone told him about the past.
    #
    # Measured on 2026-09-14, ten flagged hatches in one repository. The three
    # named were 86x idle 12 days, 46x idle 1 day, 42x idle 14 days; the hatch
    # its owner had used 21 times THAT EVENING was 40x idle 1 day, and it sat
    # unnamed in "+7 more". Two of the three he was shown had not been touched
    # in a fortnight, and one of them was a day from ageing out of the window
    # entirely.
    #
    # This is ACTIVE_DAYS' own argument applied one step further in. That
    # constant exists because "a guard that HAS BEEN FIXED must stop being
    # reported"; a class idle for two weeks being announced ahead of one used
    # yesterday is the same defect at the granularity of ordering rather than
    # inclusion. Size still decides inside a bucket, because among things
    # happening now the big one is the one to fix.
    #
    # THE CONSTANT IS NOT LOAD-BEARING AND THAT WAS CHECKED, not assumed, in
    # the way this file checks JACCARD. Re-running the same ten rows with the
    # bucket at 4, 5, 6, 7, 8 and 9 days gives the IDENTICAL three; only at 10
    # does the third slot move. RECENCY_BUCKET is ACTIVE_DAYS // 2 = 7, in the
    # middle of that range, so it is derived from a constant already argued
    # rather than fitted to this data.
    report["flagged"].sort(
        key=lambda f: (f["largest"]["idle_days"] // RECENCY_BUCKET,
                       -f["largest"]["size"]))
    memo.save()
    return report


def _guard_of(writers):
    if not writers:
        return "(unknown)"
    return os.path.basename(writers[0].split(":")[0])


# ==========================================================================
# rendering
# ==========================================================================
def one_liner(report):
    """The single sentence the Stop hook puts on the operator's screen."""
    flagged = report["flagged"]
    if not flagged:
        return ""
    parts = []
    for f in flagged[:3]:
        c = f["largest"]
        if f["has_subject_field"] and c["subjects"] >= 2:
            detail = "%d subjects over %d days" % (c["subjects"], c["days"])
        else:
            detail = "over %d days" % c["days"]
        parts.append("%s (%dx one reason, %s)" % (f["guard"], c["size"], detail))
    more = ""
    if len(flagged) > 3:
        more = " (+%d more)" % (len(flagged) - 3)
    noun = "hatch was" if len(flagged) == 1 else "hatches were"
    return ("REPEATED WAIVERS, NOT EXCEPTIONS — %d escape %s used over and "
            "over for the same reason instead of the guard being fixed: %s%s. "
            "Each repeat is a false-positive class the guard STILL HAS. "
            "Detail: scripts/waiver-repetition-lint.sh"
            % (len(flagged), noun, ", ".join(parts), more))


def state_key(report):
    """What has to change before the operator hears this again.

    The SET of flagged hatches, plus each one's largest class rounded down to a
    power of two. So a new hatch crossing the line speaks, a fixed one going
    quiet speaks, and a class doubling speaks — while the count ticking from 88
    to 89 does not, because a line repeated under every turn is a line the eye
    is trained to skip."""
    if not report["flagged"]:
        return "ok"
    parts = []
    for f in sorted(report["flagged"], key=lambda x: x["ledger"]):
        n = f["largest"]["size"]
        bucket = 1
        while bucket * 2 <= n:
            bucket *= 2
        parts.append("%s:%d" % (f["ledger"], bucket))
    return "repeated:" + ",".join(parts)


def render_text(report):
    out = []
    a = out.append
    a("=== waiver repetition — a hatch used over and over is a broken guard ===")
    a("")
    a("  entity root : %s" % (report["entity_root"] or "(none)"))
    a("  engine root : %s" % report["engine_root"])
    a("  as of       : %s" % report["today"])
    a("  rule        : same reason, >= %d times, on >= 2 days or >= 2 subjects,"
      % report["min_class"])
    a("                last used within %d days.  reason grouping: Jaccard >= %s"
      % (report["active_days"], report["jaccard"]))
    a("")
    for b in report["broken"]:
        a("  BROKEN: %s" % b)
    if report["broken"]:
        a("")

    a("  ESCAPE-HATCH LEDGERS DERIVED FROM THE GUARDS (%d):"
      % len(report["hatches_declared"]))
    for n in report["hatches_declared"]:
        used = next((l for l in report["ledgers"] if l["ledger"] == n), None)
        if used is None:
            a("    %-30s  never used" % n)
        else:
            a("    %-30s  %d entries" % (n, used["total"]))
    a("")
    if report["records_declared"]:
        a("  APPENDED BUT NOT CLASSIFIED AS A HATCH (%d) — check this list if a"
          % len(report["records_declared"]))
        a("  hatch is missing above; a wrong classification hides it:")
        a("    " + ", ".join(report["records_declared"]))
        a("")
    if report["unattributed_on_disk"]:
        a("  ON DISK, CLAIMED BY NO GUARD (%d) — either dead state or a ledger"
          % len(report["unattributed_on_disk"]))
        a("  whose writer the scan failed to find. Not analyzed, not dropped:")
        a("    " + ", ".join(report["unattributed_on_disk"]))
        a("")

    if not report["flagged"]:
        a("  NO REPEATED WAIVER CLASS. Every hatch use is either a one-off, a")
        a("  pair, or older than the activity window.")
        a("")
    for f in report["flagged"]:
        a("  --------------------------------------------------------------")
        a("  SUSPECTED BROKEN GUARD: %s" % f["guard"])
        a("    ledger        : %s  (%d entries)" % (f["ledger"], f["total"]))
        a("    written at    : %s" % ", ".join(f["writers"][:4]))
        a("    repeated      : %d class(es), %d of the %d entries"
          % (f["repeated_classes"], f["repeated_entries"], f["total"]))
        led = next(l for l in report["ledgers"] if l["ledger"] == f["ledger"])
        for c in led["repeated"]:
            a("      x%-4d %s..%s (%d days, %s subjects, last used %d day(s) ago)"
              % (c["size"], c["first_day"], c["last_day"], c["days"],
                 c["subjects"] if led["has_subject_field"] else "n/a",
                 c["idle_days"]))
            a("            %s" % _trim(c["sample"], 96))
        a("    THE FIX IS IN THE GUARD, NOT IN THE NEXT WAIVER.")
        a("")
    return "\n".join(out)


def _trim(s, n):
    s = " ".join((s or "").split())
    return s if len(s) <= n else s[:n - 1] + "…"


def main(argv=None):
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("--engine-root", required=True)
    ap.add_argument("--entity-root", default="")
    ap.add_argument("--teams-root", default="")
    ap.add_argument("--today", default="")
    ap.add_argument("--jaccard", type=float, default=JACCARD)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--one-liner", action="store_true")
    ap.add_argument("--state-key", action="store_true")
    # The Stop hook's mode: state key, broken-count, then the sentence — one
    # invocation, so the wrapper cannot end up disagreeing with the lint output
    # about what the numbers are.
    ap.add_argument("--hook-summary", action="store_true")
    args = ap.parse_args(argv)

    if args.today:
        today = datetime.date(*[int(x) for x in args.today.split("-")])
    else:
        today = datetime.date.today()

    teams_root = args.teams_root
    if not teams_root:
        cfg = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.expanduser("~/.claude")
        teams_root = os.path.join(cfg, "teams")

    report = run(args.engine_root, args.entity_root, teams_root, today,
                 args.jaccard)

    if args.hook_summary:
        print(state_key(report))
        print(len(report["broken"]))
        print(one_liner(report).replace("\n", " "))
    elif args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    elif args.state_key:
        print(state_key(report))
    elif args.one_liner:
        line = one_liner(report)
        if line:
            print(line)
    else:
        print(render_text(report))

    if report["broken"]:
        return 2
    return 1 if report["flagged"] else 0


if __name__ == "__main__":
    sys.exit(main())
