#!/usr/bin/env bash
#
# notice-claim-capability.sh — PostToolUse. THE CLAIM-CAPABILITY CHECK, DELIVERED WHERE IT
#                              ACTUALLY FIRES: the moment a RECORD is written.
#
# ===========================================================================================
# WHY THIS EXISTS — the check was built and pointed at the wrong surface
# ===========================================================================================
# `scripts/brief-provenance.py` grew a fourth check on 2026-09-14 (record:
# docs/verification/claim-capability-2026-09-14.md). It reads a sentence that DOES cite a
# source and states what that source is capable of establishing — a state measurement answers
# what IS; it never answers who did it, when, whether it was allowed, or which of several
# candidates it was.
#
# It was wired into the spawn path, because that is where `brief-provenance.py` already ran.
# Its author measured the result and reported it rather than burying it:
#
#     23 spawn payloads -> 1 row, and that row is a false positive.
#     63 landed records -> 37 rows.
#
# NOT ONE of the four real instances of the failure happened in a brief. Two were in landed
# `docs/verification/` records, one in a memory note, one typed to the CEO in chat. So the
# check sat on a surface where type Y does not happen.
#
# This hook is the delivery surface, and NOTHING ELSE. It reuses
# `brief-provenance.py`'s predicate by importing it; there is no second copy of the rules
# here, and none of them is re-tuned. A hook-shaped reimplementation beside the real one is
# the defect class this engine keeps finding in itself.
#
# ===========================================================================================
# THE CONSTRAINT THIS WAS BUILT UNDER, AND THE MEASUREMENT THAT ANSWERS IT
# ===========================================================================================
# The brief was explicit: this session has repeatedly produced mechanisms nobody reads, and
# "if your own measurement says this becomes wallpaper, say so and do not ship it."
#
# THIS HOOK, not a model of it, was replayed against every write-shaped tool call recorded in
# the 753 session transcripts under `~/.claude/projects/` that names a record path:
#
#     1035 calls replayed
#       14 of them (1.4%) produced a notice, carrying 15 rows in total
#     1021 stayed SILENT, and the reasons are separated rather than pooled:
#          500  the file no longer exists at that path — a REPLAY ARTIFACT, not the hook
#          296  the file has no capability rows at all
#          168  the file HAS rows and this call wrote none of them  <- the narrowing working
#           57  the path shape did not resolve
#     excluding the replay artifact: 535 live calls, 14 notices = 2.6%
#
# Fifteen lines across two months is not wallpaper, and the honest reading runs the other way:
# the risk is that it is too RARE to build a habit around. The 500-call artifact class is
# biased toward first full writes, which are the likeliest to fire, so the production rate is
# higher than 2.6% — bounded above by the 88 rows the four corpora hold today, since a row is
# introduced by at least one write. Somewhere between 15 and 88 notices over two months.
# The argument, the bound and the hand-classified false-positive rate are in the record.
#
# ===========================================================================================
# THE THREE DESIGN DECISIONS THAT KEEP IT OFF THE WALL, each forced by a measurement
# ===========================================================================================
# 1. CHECK 4 ONLY. Running the whole of `brief-provenance.py` on a record emits 1633 rows
#    over 64 records — 25.5 per record from checks 1-3. Delivering "the provenance report" on
#    a record write would bury the eight true capability rows under sixteen hundred others.
#    So `check_capability` is called directly. Verified on the six largest documents in the
#    corpus: the row sets from `review()`-filtered and `check_capability()` are identical,
#    and the direct call is 2-4x faster (0.03s on a 160 KB page).
#
# 2. ONLY SENTENCES THIS CALL ACTUALLY WROTE. The hook re-scans the WHOLE file, so a
#    document with N rows edited K times would cost N*K rows. Measured, that is the single
#    largest source of repetition in the corpus. The fix is not a dedupe ledger — it is a
#    narrower and MORE CORRECT question: does the flagged sentence appear in the text THIS
#    call wrote? A row about a paragraph the author did not touch is not about this write.
#    Stateless, and in the replay above it is what keeps 168 of the 1035 calls quiet — every
#    one of them a call that touched a flagged record and wrote none of its flagged lines.
#
#    It also makes reads silent for free: `sed -n '1,50p' record.md` and
#    `git add record.md` name the path and write no sentence, so nothing matches.
#
# 3. A SURFACE LIST DERIVED FROM RATES, NOT FROM INTUITION. Measured rows per document:
#
#        docs/verification/  0.59-0.79    <- 2 of the 4 instances live here
#        wiki/               0.66         <- the durable record; the noisiest, and kept
#        memory/*.md         0.04         <- 1 of the 4 instances; the best precision
#        RICH/CEO-TODOs.md   2.00         <- EXCLUDED, see below
#
#    RICH-TODOs.md and CEO-TODOs.md are excluded on the measurement, not on taste: their
#    whole PURPOSE is to record what is awaited by a person, and rule A1 holds "awaited" to
#    be universally unestablishable. Eight recorded writes to RICH-TODOs.md would each have
#    re-emitted the same four rows — one file accounting for 32 emissions, more than the
#    other three surfaces put together, over rows that are correct about the WORDS and wrong
#    about the DOCUMENT. A page that would be flagged on every write is the wallpaper this
#    was forbidden to become.
#
#    Source files, `fixtures/` and `*.corpus.md` are out by construction: the check reads
#    prose, and a corpus file asserts known-bad sentences ON PURPOSE.
#
# ===========================================================================================
# WHAT IT WILL NOT DO
# ===========================================================================================
#   * IT NEVER BLOCKS AND NEVER FAILS A TOOL CALL — and it never LOOKS like it did either,
#     which took a measurement to get right. It exits 0 on every path and speaks through
#     `hookSpecificOutput.additionalContext`. The obvious alternative, stderr with exit 2, is
#     what the engine's other PostToolUse reporter uses and is what this hook used first; both
#     were measured in headless sessions and both reach the author, but the host wraps the
#     exit-2 form in "PostToolUse:Write hook BLOCKING ERROR from command". The table is at the
#     emission site. A check that could not run is silent, because it must never look like a
#     failed write.
#   * IT NEVER EDITS THE RECORD. The rows are transient, addressed to the author at the one
#     moment the word can still be changed. 49 of 88 hand-classified rows are sentences that
#     turn out to be fine; a permanent section of those inside a landed record would be noise
#     in the durable record, forever, which is the opposite of what a record is for.
#   * IT DOES NOT COVER THE FOURTH INSTANCE, and this is said plainly rather than implied
#     away: one of the four was a sentence typed straight to the CEO in chat. It passes
#     through no file and no tool. Nothing textual will ever catch it, and this hook does not.
#
# NOTE: hooks are snapshotted at session start, so this is inert until the next session.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROV="$SCRIPT_DIR/../../ass-kicker/brief-provenance.py"

# THE SURFACE LIST, DECLARED ONCE. Both readers below use this and only this: the bash fast
# path and the python path resolver. It was two literals for about twenty minutes, and the
# mutation harness caught it immediately — widening the python one left the grep narrow, so
# the mutant survived while looking applied. Two copies of a predicate is the defect class
# this engine keeps finding in itself; here it is, one level down, inside a fix for it.
RECORD_DIRS='docs/verification/|/wiki/|/memory/'
export RECORD_DIRS

INPUT=""
IFS= read -r -t "${RICHOS_HOOK_STDIN_TIMEOUT:-3}" -d '' INPUT || true

# THE FAST PATH. A record-shaped markdown path is named in this payload, or there is nothing
# here for this hook. Registered against every write-shaped tool, so the overwhelmingly
# common answer is reached before python3 is started at all.
if ! printf '%s' "$INPUT" | grep -qE "($RECORD_DIRS)[^\"]*\.md"; then
    exit 0
fi
[ -f "$PROV" ] || exit 0
command -v python3 >/dev/null 2>&1 || exit 0

export RICHOS_PROVENANCE="$PROV"
# Keep the payload out of argv and the environment; descriptor 3 carries its bytes, the same
# way every other payload-reading hook in this engine does it.
python3 - 3<<< "$INPUT" <<'PY'
import importlib.util
import json
import os
import re
import sys

try:
    with os.fdopen(3, "r", encoding="utf-8") as fh:
        payload = json.load(fh)
except Exception:
    sys.exit(0)

ti = payload.get("tool_input") or {}
cwd = payload.get("cwd") or os.getcwd()

# --- WHAT THIS CALL WROTE --------------------------------------------------------------
# Not "what is in the file" — what THIS tool call put there. Each write-shaped tool carries
# it in a different field, and Bash carries it in the command itself, heredoc body included.
# A command that only READS a record contributes no sentences and therefore matches nothing.
written = []
for key in ("content", "new_string", "new_source", "command"):
    v = ti.get(key)
    if isinstance(v, str):
        written.append(v)
for e in (ti.get("edits") or []):
    if isinstance(e, dict) and isinstance(e.get("new_string"), str):
        written.append(e["new_string"])
if not written:
    sys.exit(0)
written = "\n".join(written)

# --- WHICH RECORDS ---------------------------------------------------------------------
# Absolute or relative to the call's own cwd; it must exist on disk, because the check reads
# the file to get the citation into scope with the sentence.
RECORD = re.compile(r"(?:^|[\s\"'=(])((?:/|\./|\.\./)?[^\s\"'<>|;:()]*"
                    r"(?:%s)[^\s\"'<>|;:()]*\.md)" % os.environ["RECORD_DIRS"])
EXCLUDE = re.compile(r"(/fixtures/|\.corpus\.md$|/node_modules/)")

blob = " ".join(str(v) for v in ti.values() if isinstance(v, (str, int, float)))
paths, seen = [], set()
for m in RECORD.finditer(blob):
    p = m.group(1)
    if EXCLUDE.search(p):
        continue
    full = p if os.path.isabs(p) else os.path.normpath(os.path.join(cwd, p))
    if full in seen or not os.path.isfile(full):
        continue
    seen.add(full)
    paths.append(full)
if not paths:
    sys.exit(0)

# --- THE PREDICATE, IMPORTED RATHER THAN REIMPLEMENTED ----------------------------------
try:
    spec = importlib.util.spec_from_file_location("brief_provenance",
                                                  os.environ["RICHOS_PROVENANCE"])
    PROV = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(PROV)
except Exception:
    sys.exit(0)                        # a check that cannot run is silent, never a failure

norm_written = PROV.norm(written)
out = []
for path in paths:
    try:
        with open(path, encoding="utf-8") as fh:
            body = fh.read()
        rows = PROV.dedupe(PROV.check_capability(PROV.segment(body)))
    except Exception:
        continue
    # Only what this call wrote. `text` is already markdown-stripped and truncated, so the
    # comparison is made in the same normal form on both sides.
    rows = [r for r in rows if PROV.norm(r["text"]) and PROV.norm(r["text"]) in norm_written]
    if rows:
        out.append((path, rows))

if not out:
    sys.exit(0)

total = sum(len(r) for _, r in out)
lines = [
    "Claim-capability check on the record you just wrote. Nothing failed and nothing is "
    "blocked; the write succeeded.",
    "",
    "%d sentence%s you just wrote cite%s a source that does NOT establish what "
    "%s say%s:" % (total, "" if total == 1 else "s", "s" if total == 1 else "",
                   "it" if total == 1 else "they", "s" if total == 1 else ""),
]
for path, rows in out:
    lines.append("")
    lines.append("  %s" % path)
    for r in rows:
        lines.append("    [capability] %s" % r["text"])
        lines.append("                 %s" % r["detail"])
lines += ["", PROV.CAPABILITY_LEAD, "",
          "This is a DISCLOSURE, not a verdict: the first half of each row is a fact about "
          "the command, and the check never judges your sentence. It is also often "
          "unnecessary: of 88 rows hand-classified across these surfaces, 49 were sentences "
          "that turn out to be fine. So the expected cost of a row is one re-read, and more "
          "often than not that re-read ends in nothing. "
          "Nothing is blocked and there is no waiver to grant. "
          "Rules, corpus and counts: docs/verification/claim-capability-2026-09-14.md and "
          "docs/verification/claim-capability-delivery-2026-09-14.md."]

# THE CHANNEL, MEASURED RATHER THAN ASSUMED — Claude Code, macOS, 2026-09-14. Two PostToolUse
# hooks were registered in two headless sessions, each emitting a unique marker on a different
# channel, and the model was asked to quote back whatever a hook returned to it:
#
#   channel                                        reaches the author   framed as
#   --------------------------------------------   ------------------   ---------------------
#   stderr, exit 2            (ZACHCHANNEL_..QX71)        YES            "PostToolUse:Write
#                                                                        hook BLOCKING ERROR
#                                                                        from command: ..."
#   additionalContext, exit 0 (ZACHCHANNEL_..QX72)        YES            the text, and nothing
#                                                                        else
#
# BOTH reach the author verbatim, so the choice is made entirely by the second column. The
# exit-2 form — which is what `detect-nonnative-worktree.sh` has used since it landed, and
# what this hook used for its first hour — announces a NON-BLOCKING disclosure as a BLOCKING
# ERROR. A notice whose first sentence says "nothing failed" arriving under a host banner that
# says it did is a mechanism arguing with itself, and the reader believes the banner.
#
# The operator is deliberately NOT told. `suppressOutput` keeps the raw JSON out of the
# rendered transcript, and the engine's own measured table (scripts/lib/stop-hook-notice.sh)
# says neither of these channels reaches the operator's stream anyway. That is correct here:
# the one person who can act on a row is the author, in the seconds after writing it.
print(json.dumps({
    "hookSpecificOutput": {"hookEventName": "PostToolUse",
                           "additionalContext": "\n".join(lines)},
    "suppressOutput": True,
}))
sys.exit(0)
PY
# ALWAYS 0. The tool call already succeeded; a non-zero exit here would be the host telling
# the author its write failed, which is both false and the opposite of what this hook is for.
exit 0
