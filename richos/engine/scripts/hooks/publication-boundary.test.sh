#!/usr/bin/env bash
#
# publication-boundary.test.sh — regression tests for the publication-boundary
# mechanism: scripts/lib/publication-boundary.sh, its python predicate, and the
# two guards that run it (guard-publication-writes.sh on Write/Edit/MultiEdit/
# NotebookEdit, guard-publication-commits.sh on Bash `git commit`).
#
# ONE suite for all three files on purpose. They are one subject — a predicate
# and its two chokepoints — and splitting them would create two places to
# remember to update, which is the defect class the whole mechanism exists to
# remove.
#
# A NOTE ON THE HARNESS, because it got this wrong once and the lesson is the
# same one the mechanism is about. The first version fed content to each case
# through a PIPE (`make_transcript | write_case ...`). Bash runs the right-hand
# side of a pipe in a SUBSHELL, so every PASS/FAIL increment inside it was
# discarded: the suite printed 48 result lines and reported "26 passed, 0
# failed", and a genuine failure inside a piped case would have been counted by
# nothing and exited 0. A tally that is not the inventory it claims to describe
# — exactly the "18/18 suites" defect, reproduced inside the test suite written
# to prevent that defect's cousin. Content now travels by FILE, and every
# assertion runs in the top-level shell where its counter lives.
#
# Covers:
#   (a) STAND-DOWN — a repository with no .publication-boundary is untouched.
#       This is the precision floor: get it wrong and the guard fires in every
#       repository on the machine.
#   (b) THE REAL FAILURE — a transcript-shaped file, and prose reproducing the
#       private corpus verbatim, both refused, across every tool shape.
#   (c) POSITIVE CONTROLS — ordinary source, docs, a technology evaluation that
#       quotes nothing, measurement conclusions with the speech removed, a log
#       file and an attributed changelog all write cleanly. These matter more
#       than the block: a guard that blocks legitimate work gets disabled, and
#       then protects nothing.
#   (d) GITIGNORED DESTINATION — the same private bytes to an ignored path are
#       ALLOWED. That is the correct destination, not a grudging exception.
#   (e) ALLOWLIST — a committed exemption works, and it is the ONLY way through,
#       because these two guards deliberately ship no in-prompt override token.
#   (m) THE GROUPED DIRECTORY — the declaration may live at
#       `.richos/publication-boundary`, and the move must not stand the guard
#       down. Proved by a REFUSAL, never by a load: a guard that found the file
#       and then decided nothing would pass any test that only asked whether it
#       was found. Declared in both places at once BLOCKS, and a DECLARATION in
#       `.richos/` that nothing resolves BLOCKS, because the alternative is a
#       publication contract switched off by a `git mv`. Anything else sharing
#       that directory is left alone: it is a shared RichOS directory, and a
#       guard policing all of it would take an adopter offline.
#   (f) BROKEN DECLARATIONS — unknown key, non-KEY=value line, a threshold below
#       the measured floor, and a PRIVATE_SOURCES tree that is inside the repo
#       and NOT gitignored: every one BLOCKS rather than degrading quietly.
#   (g) THE COMMIT CHOKEPOINT — content that never passed through a Write tool
#       (cp, git mv, a generator) is caught at `git commit`; unrelated Bash and
#       clean commits pass; `git commit -m "... -a ..."` is not mistaken for a
#       `-a` commit; a binary blob is never handed to the text scanner.
#   (h) WHAT THE COMMAND IS ABOUT TO STAGE. The hook runs BEFORE the command,
#       so `git add X && git commit` has an empty index at check time; a whole
#       new directory of transcripts arrives as ONE porcelain entry with no
#       bytes behind it; and a pathspec or `-a` commit records the WORKTREE
#       copy, not the index copy. Every one of those passed before, with the
#       controls that keep ordinary `git add src/newmod && git commit` clean.
#   (i) THE CORPUS CLOSURE. A second rendering of a recording — whisper's plain
#       text output, no timestamps — joins the corpus; a brief that merely
#       QUOTES the recording does not, so its boilerplate never becomes
#       "private" and never blocks ordinary work.
#   (i2) MEDIA PROVENANCE. The FIRST rendering of a recording, when that
#       rendering is plain text. The shape filter cannot see it and the closure
#       can only extend a seed, never create one — so a recording transcribed
#       straight to plain text was invisible to the corpus whole, and both
#       guards passed 6,000 characters of one in silence. A text file named as a
#       rendering of a media file beside it now seeds; an unrelated stem, a stem
#       below the length floor, and the same name with no media beside it do
#       not, and those three refusals are why the narrow rule was chosen.
#   (i3) THE VACUITY FLOOR AND THE ORACLE. A corpus that is empty is BROKEN, not
#       CLEAN, because everything after the corpus is conditional on it and a
#       guard that reports clean having read nothing is this whole mechanism's
#       defect in one line. Plus the negative control for THIS SUITE: the
#       scanner reports the corpus it examined, so a green run cannot mean
#       "every case passed because there was nothing to compare against".
#   (l) PRIVATE BY IDENTITY. The file class both scanners score at zero: a
#       note that is private because of WHICH FILE IT IS. Declared as
#       `<sha256>:<name>` in PRIVATE_FILES, it is refused renamed, reformatted,
#       re-encoded, into a gitignored path, and as a binary blob the NUL filter
#       used to drop — with the positive controls that keep that from being
#       paranoia, the malformed-declaration refusals, and the oracle for how
#       many declarations a run actually compared against. Every fixture is
#       synthetic: the file this was built for never enters this repository,
#       including as a test fixture, which would be the failure executed.
#   (p) ALREADY PUBLISHED. A line the public remote's main already carries in
#       the same file commits even when a private record quotes it; a NEW
#       private phrase in that file is refused; a new file carrying the public
#       line, or the same commit with no published copy, is refused. Three
#       mutations of the rule, each caught.
#   (j) FAIL-CLOSED / FAIL-OPEN conventions, matching the hook family.
#   (k) REGISTRATION on BOTH surfaces plus the probe's oracle.
#
# Run directly: scripts/hooks/publication-boundary.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# The registration question has ONE answer in this engine, and it is not a
# grep: seventeen guards are wired by scripts/hooks/dispatch-pretooluse.manifest
# rather than by an entry of their own. hook_enforced_on_surface asks whether
# the surface causes the guard to RUN, which is the property this case is about.
# shellcheck source=../lib/registered-hooks.sh
. "$ENGINE_ROOT/scripts/lib/registered-hooks.sh"


# Declare the root under test, for the reason scan-secrets.test.sh states: run
# from a session seated elsewhere, the guards would resolve THAT repository,
# find no adoption marker, stand down, and every case below would pass by never
# running.
RICHOS_ENTITY_ROOT="$ENGINE_ROOT"
export RICHOS_ENTITY_ROOT
unset CLAUDE_PROJECT_DIR

WRITE_HOOK="$SCRIPT_DIR/guard-publication-writes.sh"
COMMIT_HOOK="$SCRIPT_DIR/guard-publication-commits.sh"
BASH_BIN="$(command -v bash)"

PASS=0
FAIL=0
SCRATCH="$(mktemp -d -t pubtest-scratch.XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT

ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

# tf — capture stdin into a scratch file and print its path. Content travels by
# file so that no assertion ever runs on the far side of a pipe.
#
# The filename comes from mktemp, NOT from a counter. The first version used
# `TF_N=$((TF_N+1))`, and every call site is `$(... | tf)` — a subshell — so the
# counter never advanced in the parent and all eleven fixtures were the SAME
# path, each overwriting the last. The transcript fixture was silently holding
# SRT bytes, and the "names what it skipped" case was asserting against the
# string "hello world". Same subshell trap as the tally bug above, one level
# down, and it is why that case failed: it was the only assertion whose fixture
# had to still be intact several cases later.
tf() {
    local f
    f="$(mktemp "$SCRATCH/content.XXXXXX")"
    cat > "$f"
    printf '%s' "$f"
}

# --- fixtures ---------------------------------------------------------------
# A synthetic transcript in the exact shape the 2026-08-29 whisper output had.
#
# Synthetic on two counts, and both are deliberate.
#
# The WORDS are invented. A suite that shipped the CEO's actual webinar speech
# in order to prove the CEO's webinar speech must not ship would be
# self-refuting.
#
# The SHAPE is ASSEMBLED AT RUN TIME, never written out as literals. Nine lines
# of `[timestamp] Speaker: prose` sitting in this file are precisely what this
# guard refuses, so with the guard live the guard REFUSED ITS OWN TEST SUITE —
# observed, on the first attempt to commit this work. The alternative was an
# ALLOWLIST entry for the suite, which would carve a permanent hole in the one
# file most likely to be edited by whoever next changes the predicate.
# scan-secrets.test.sh assembles its key fixtures at run time for the identical
# reason; this is the same move. What lands on disk is nine ordinary sentences.
make_transcript() {
    local ts=0 n=0 who line
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        if [ $((n % 2)) -eq 0 ]; then who="Dana"; else who="Ellis"; fi
        printf '%s00%s%02d%s %s%s %s\n' '[' ':' "$ts" ']' "$who" ':' "$line"
        ts=$((ts + 7)); n=$((n + 1))
    done <<'EOF'
right so we are recording now and everything looks good on my end
yes I can see you fine let us start whenever you are ready to go
okay so the thing I wanted to walk through today is the migration
before you do that can I ask about the timeline you mentioned earlier
sure the timeline is the part I am least confident about right now
that is fair and honestly I would rather hear that than a guess
exactly so let us take it one piece at a time and see where we land
sounds good to me go ahead and share whatever you have prepared
alright I will start with the numbers and then we can talk shape
EOF
}

make_srt() {
    local i
    for i in 1 2 3 4 5 6 7 8 9 10; do
        printf '%d\n00:0%d:00,000 --> 00:0%d:05,000\nsome caption text here\n\n' \
            "$i" "$((i % 10))" "$((i % 10))"
    done
}

# make_speech_lines <lines> <words-per-line> <seed> — deterministic pseudo-random
# sentences drawn from an ordinary word pool.
#
# VARIED ON PURPOSE. The corpus closure counts DISTINCT word-runs shared with
# the corpus, so a fixture built by repeating one sentence with a changing number
# in it collapses to a handful of distinct runs in a set and would prove nothing
# about a threshold of hundreds. A pseudo-random walk over a 64-word pool makes
# essentially every 10-word window unique, which is what real speech is like.
#
# Still invented, still assembled at run time — the same rule the whole fixture
# section follows: this suite never carries a line of anybody's real speech.
make_speech_lines() {
    awk -v lines="$1" -v per="$2" -v seed="$3" 'BEGIN {
        split("the and but so then when where because after before while about \
into over under again really maybe always never people number reason system \
change moment question answer problem morning evening meeting minute matter \
whole point thing piece order price value market signal window record message \
window driver builder holder keeper marker anchor pattern segment version \
handle bridge ladder circle corner border member letter picture", pool, " ");
        n = 0; for (k in pool) n++;
        srand(seed);
        for (i = 1; i <= lines; i++) {
            out = "";
            for (j = 1; j <= per; j++) {
                w = pool[int(rand() * n) + 1];
                out = (j == 1) ? w : out " " w;
            }
            print out;
        }
    }'
}

# A transcript in the whisper "[timestamp] Speaker:" shape, long enough to be a
# corpus member several hundred distinct runs deep.
make_long_transcript() {
    local n=0 who
    while IFS= read -r line; do
        if [ $((n % 2)) -eq 0 ]; then who="Dana"; else who="Ellis"; fi
        printf '%s00%s%02d%s %s%s %s\n' '[' ':' "$((n % 60))" ']' "$who" ':' "$line"
        n=$((n + 1))
    done
}

default_declaration() {
    cat <<'EOF'
PRIVATE_RECORD="the-private-record"
PRIVATE_SOURCES="private"
MIN_SPEECH_LINES=8
MIN_QUOTE_WORDS=10
EOF
}

# make_sandbox [declaration-line ...] — a repository declaring a boundary, with
# a gitignored private/ tree holding the transcript the corpus is built from.
make_sandbox() {
    local sb
    sb="$(mktemp -d -t pubtest.XXXXXX)"
    git -C "$sb" init -q
    git -C "$sb" config user.email t@example.com
    git -C "$sb" config user.name T
    git -C "$sb" config commit.gpgsign false
    mkdir -p "$sb/src" "$sb/docs" "$sb/private"
    printf '/private/\n' > "$sb/.gitignore"
    make_transcript > "$sb/private/recording.transcript.txt"
    printf 'export const MAX_CONTEXT_TOKENS = 0;\n' > "$sb/src/config.js"
    printf '%s\n' "$@" > "$sb/.publication-boundary"
    printf '%s' "$sb"
}

make_default_sandbox() { make_sandbox "$(default_declaration)"; }

sandbox_commit() { # <sb> <msg> — --no-verify so an operator's own pre-commit
                   # hooks (identity guards etc.) cannot decide this suite.
    git -C "$1" -c user.email=t@example.com -c user.name=T \
        commit -qm "$2" --no-verify >/dev/null 2>&1
}

write_payload() { # <tool> <file_path> <cwd> <content-file>
    PB_T="$1" PB_F="$2" PB_C="$3" PB_SRC="$4" python3 -c '
import json, os
with open(os.environ["PB_SRC"], encoding="utf-8") as fh:
    content = fh.read()
tool = os.environ["PB_T"]
ti = {"file_path": os.environ["PB_F"]}
if tool == "Write":
    ti["content"] = content
elif tool == "Edit":
    ti["old_string"] = "PLACEHOLDER"; ti["new_string"] = content
elif tool == "MultiEdit":
    ti["edits"] = [{"old_string": "PLACEHOLDER", "new_string": content}]
elif tool == "NotebookEdit":
    ti = {"notebook_path": os.environ["PB_F"], "new_source": content}
print(json.dumps({"tool_name": tool, "cwd": os.environ["PB_C"], "tool_input": ti}))
'
}

bash_payload() { # <command> <cwd>
    PB_CMD="$1" PB_C="$2" python3 -c '
import json, os
print(json.dumps({"tool_name": "Bash", "cwd": os.environ["PB_C"],
                  "tool_input": {"command": os.environ["PB_CMD"]}}))
'
}

write_case() { # <name> <expect-rc> <tool> <dest> <sandbox> <content-file>
    local name="$1" want="$2" rc
    write_payload "$3" "$4" "$5" "$6" | "$WRITE_HOOK" >/dev/null 2>&1
    rc=$?
    if [ "$rc" -eq "$want" ]; then ok "$name"; else bad "$name (expected exit $want, got $rc)"; fi
}

msg_case() { # <name> <needle> <tool> <dest> <sandbox> <content-file>
    local name="$1" needle="$2" out
    out="$(write_payload "$3" "$4" "$5" "$6" | "$WRITE_HOOK" 2>&1 >/dev/null)"
    if printf '%s' "$out" | grep -qF -- "$needle"; then
        ok "$name"
    else
        bad "$name (message did not mention \"$needle\")"
    fi
}

commit_case() { # <name> <expect-rc> <sandbox> [command]
    local name="$1" want="$2" sb="$3" cmd="${4:-git -C $3 commit -m msg}" rc
    bash_payload "$cmd" "$sb" | "$COMMIT_HOOK" >/dev/null 2>&1
    rc=$?
    if [ "$rc" -eq "$want" ]; then ok "$name"; else bad "$name (expected exit $want, got $rc)"; fi
}

echo "=== publication boundary ==="

TRANSCRIPT="$(make_transcript | tf)"
SRT="$(make_srt | tf)"

# ---------------------------------------------------------------------------
# (a) STAND-DOWN — the precision floor.
# ---------------------------------------------------------------------------
NODECL="$(mktemp -d -t pubtest-nodecl.XXXXXX)"
git -C "$NODECL" init -q
mkdir -p "$NODECL/docs"
write_case "undeclared repo: a transcript writes cleanly" 0 Write "$NODECL/docs/t.txt" "$NODECL" "$TRANSCRIPT"
write_case "a path in no repository at all is untouched"  0 Write "/tmp/pubtest-nowhere-$$.txt" "$NODECL" "$TRANSCRIPT"
commit_case "undeclared repo: commit untouched" 0 "$NODECL"
rm -rf "$NODECL"

SB="$(make_default_sandbox)"

# ---------------------------------------------------------------------------
# (b) THE REAL FAILURE — both detectors, across every tool shape.
# ---------------------------------------------------------------------------
write_case "Write: a transcript is refused"       2 Write       "$SB/docs/t.txt"   "$SB" "$TRANSCRIPT"
write_case "Edit: a transcript is refused"        2 Edit        "$SB/docs/t.txt"   "$SB" "$TRANSCRIPT"
write_case "MultiEdit: a transcript is refused"   2 MultiEdit   "$SB/docs/t.txt"   "$SB" "$TRANSCRIPT"
write_case "NotebookEdit: a transcript is refused" 2 NotebookEdit "$SB/docs/t.ipynb" "$SB" "$TRANSCRIPT"
write_case "SRT caption cues are refused"         2 Write       "$SB/docs/c.srt"   "$SB" "$SRT"

# The half no shape rule can see: a quote embedded in ordinary prose. This is
# the shape of all 28 quotes in the 2026-08-29 brief bodies.
QUOTED="$({
    printf '# Measurement notes\n\n'
    printf 'The decoder looped on this line, which the speaker actually said once:\n\n'
    printf '> "the timeline is the part I am least confident about right now"\n\n'
    printf 'The physical veto preserves it because three bursts are present.\n'
} | tf)"
write_case "prose quoting the private corpus is refused" 2 Write "$SB/docs/brief.md" "$SB" "$QUOTED"

msg_case "the refusal names the private record"        "the-private-record"  Write "$SB/docs/t.txt" "$SB" "$TRANSCRIPT"
msg_case "the refusal offers a gitignored destination" "gitignored path"     Write "$SB/docs/t.txt" "$SB" "$TRANSCRIPT"
msg_case "the refusal names the ALLOWLIST escape"      "ALLOWLIST"           Write "$SB/docs/t.txt" "$SB" "$TRANSCRIPT"
msg_case "the refusal quotes the evidence"  "reproduces private source material" Write "$SB/docs/t.txt" "$SB" "$TRANSCRIPT"

# ---------------------------------------------------------------------------
# (c) POSITIVE CONTROLS — these matter more than the block.
# ---------------------------------------------------------------------------
CODE="$(printf 'export const MAX_CONTEXT_TOKENS = 0;\nexport const TIER = "turbo";\n' | tf)"
write_case "ordinary product code" 0 Write "$SB/src/config.js" "$SB" "$CODE"

# A technology evaluation that quotes nothing — the exact shape of Clark's
# Parakeet viability brief, which the removal commit deliberately KEPT.
EVAL="$({
    printf '# Parakeet viability\n\n'
    printf 'Parakeet is an NVIDIA ASR family. On Apple silicon the MLX port runs\n'
    printf 'at roughly 40x realtime for a 90 minute file, against 12x for whisper\n'
    printf 'large-v3-turbo at q5_0. Word error rate on the standard benchmark is\n'
    printf 'within a point. The integration cost is a new model loader and a\n'
    printf 'different segment schema. Recommendation: evaluate, do not adopt yet.\n'
} | tf)"
write_case "technology evaluation quoting nothing" 0 Write "$SB/docs/viability.md" "$SB" "$EVAL"

# Measurement CONCLUSIONS with the speech stripped out: counts, parameters and
# decisions, which are not private.
CONC="$({
    printf '# Long-form decode: conclusions\n\n'
    printf '| tier | segments before | segments after | words removed |\n'
    printf '|---|---|---|---|\n'
    printf '| turbo | 204 | 20 | 1032 |\n'
    printf '| q5_0 | 709 | 92 | 4974 |\n\n'
    printf 'MAX_CONTEXT_TOKENS is now a pipeline-wide invariant rather than a\n'
    printf 'property of the opt-in accuracy tier. The repetition guard gained a\n'
    printf 'physical speech-burst veto with a capacity of one delivery per burst.\n'
} | tf)"
write_case "measurement conclusions, speech stripped" 0 Write "$SB/docs/conclusions.md" "$SB" "$CONC"

# A LOG file — the near-miss the shape detector is built against.
LOG="$({
    for i in 1 2 3 4 5 6 7 8 9 10 11 12; do
        printf '[12:34:%02d] ERROR: connection refused while contacting the upstream host\n' "$i"
        printf '[12:34:%02d] INFO: retrying the request after a short backoff period\n' "$i"
    done
} | tf)"
write_case "a log file is not a transcript" 0 Write "$SB/docs/server.log" "$SB" "$LOG"

# A CHANGELOG of timestamped, attributed markdown list items — the other
# near-miss, and the one that made the shape detector narrower.
CHLOG="$({
    printf '# Changelog\n\n'
    for i in 1 2 3 4 5 6 7 8 9 10; do
        printf -- '- 12:0%d Dana: rewrote the loader so it no longer reads the whole file\n' "$((i % 10))"
    done
} | tf)"
write_case "a timestamped attributed changelog is not a transcript" 0 Write "$SB/docs/CHANGELOG.md" "$SB" "$CHLOG"

# The cost of that narrowing, pinned so nobody has to rediscover it: a
# transcript pasted as a markdown LIST is invisible to the shape detector, and
# is caught by the corpus detector, which does not look at line shape.
LISTED="$(make_transcript | sed 's/^/- /' | tf)"
write_case "transcript-as-a-list: shape blind, corpus detector catches it" 2 Write "$SB/docs/listed.md" "$SB" "$LISTED"

# ---------------------------------------------------------------------------
# (d) GITIGNORED DESTINATION — the correct home for private material.
# ---------------------------------------------------------------------------
write_case "same transcript, gitignored destination" 0 Write "$SB/private/new.transcript.txt" "$SB" "$TRANSCRIPT"

# ---------------------------------------------------------------------------
# (e) ALLOWLIST — the only sanctioned way through, and it is committed.
# ---------------------------------------------------------------------------
SB_AL="$(make_sandbox 'PRIVATE_RECORD="rec"' 'PRIVATE_SOURCES="private"' 'ALLOWLIST="docs/sanctioned"')"
write_case "allowlisted prefix writes cleanly" 0 Write "$SB_AL/docs/sanctioned/t.txt" "$SB_AL" "$TRANSCRIPT"
write_case "a sibling path is NOT allowlisted"  2 Write "$SB_AL/docs/other/t.txt"     "$SB_AL" "$TRANSCRIPT"
rm -rf "$SB_AL"

# ---------------------------------------------------------------------------
# (f) BROKEN DECLARATIONS — every one BLOCKS. No silent degradation.
# ---------------------------------------------------------------------------
HELLO="$(printf 'hello world, nothing private here at all\n' | tf)"

SB_BAD="$(make_sandbox 'PRIVATE_SOURCES="private"' 'MIN_QUOTE_WORDZ=10')"
write_case "unknown key BLOCKS"           2             Write "$SB_BAD/src/a.js" "$SB_BAD" "$HELLO"
msg_case  "unknown key names the typo"    "MIN_QUOTE_WORDZ" Write "$SB_BAD/src/a.js" "$SB_BAD" "$HELLO"
rm -rf "$SB_BAD"

SB_BAD="$(make_sandbox 'PRIVATE_SOURCES="private"' 'this is not a setting')"
write_case "a non KEY=value line BLOCKS" 2 Write "$SB_BAD/src/a.js" "$SB_BAD" "$HELLO"
rm -rf "$SB_BAD"

SB_BAD="$(make_sandbox 'PRIVATE_SOURCES="private"' 'MIN_QUOTE_WORDS=3')"
write_case "a threshold below the measured floor BLOCKS" 2 Write "$SB_BAD/src/a.js" "$SB_BAD" "$HELLO"
rm -rf "$SB_BAD"

# A "private source" that is itself published is not a private source.
SB_BAD="$(make_sandbox 'PRIVATE_SOURCES="docs"')"
write_case "PRIVATE_SOURCES pointing at a TRACKED tree BLOCKS" 2 Write "$SB_BAD/src/a.js" "$SB_BAD" "$HELLO"
msg_case  "and says why"                        "NOT gitignored" Write "$SB_BAD/src/a.js" "$SB_BAD" "$HELLO"
rm -rf "$SB_BAD"

# A declared source that is simply not on this machine is SKIPPED, not broken —
# and the refusal says so, so a block never overstates its own coverage.
SB_ABS="$(make_sandbox 'PRIVATE_RECORD="rec"' 'PRIVATE_SOURCES="private ../not-cloned-anywhere"')"
write_case "an absent declared source is skipped, not broken" 0 Write "$SB_ABS/src/a.js" "$SB_ABS" "$HELLO"
msg_case  "and a refusal names what it skipped" "not present on this machine" Write "$SB_ABS/docs/t.txt" "$SB_ABS" "$TRANSCRIPT"
rm -rf "$SB_ABS"

# ---------------------------------------------------------------------------
# (g) THE COMMIT CHOKEPOINT — provenance stops mattering.
#
# A FRESH sandbox per group. Reusing one across `git mv`, `reset --hard` and a
# refused commit leaves residue in the index, and a case that passes because of
# leftover state is a case asserting something other than its own name.
# ---------------------------------------------------------------------------
SB_C="$(make_default_sandbox)"
git -C "$SB_C" add -A >/dev/null 2>&1
sandbox_commit "$SB_C" base
commit_case "a clean tree commits"                    0 "$SB_C"
commit_case "an unrelated Bash command is untouched"  0 "$SB_C" "ls -la $SB_C"
commit_case "git status is untouched"                 0 "$SB_C" "git -C $SB_C status"
printf 'export const X = 1;\n' >> "$SB_C/src/config.js"
git -C "$SB_C" add -A >/dev/null 2>&1
commit_case "an ordinary staged source edit commits"  0 "$SB_C"
rm -rf "$SB_C"

# cp — exactly how the 137 generated asset files arrived. No Write tool
# involved, so the write guard never sees this at all.
SB_C="$(make_default_sandbox)"
git -C "$SB_C" add -A >/dev/null 2>&1
sandbox_commit "$SB_C" base
mkdir -p "$SB_C/docs/assets"
cp "$SB_C/private/recording.transcript.txt" "$SB_C/docs/assets/t.txt"
git -C "$SB_C" add -A >/dev/null 2>&1
commit_case "cp of a generated transcript is refused at commit" 2 "$SB_C"
rm -rf "$SB_C"

# git mv out of the ignored tree into a tracked one.
SB_C="$(make_default_sandbox)"
git -C "$SB_C" add -A >/dev/null 2>&1
sandbox_commit "$SB_C" base
git -C "$SB_C" add -f private/recording.transcript.txt >/dev/null 2>&1
sandbox_commit "$SB_C" priv
git -C "$SB_C" mv private/recording.transcript.txt docs/moved.txt >/dev/null 2>&1
commit_case "git mv into a tracked path is refused at commit" 2 "$SB_C"
rm -rf "$SB_C"

# `-m` text containing "-a" must not be read as a `-a` commit: doing so would
# drag every unstaged modification into the scan and refuse a commit over a
# file the author was not committing.
SB_C="$(make_default_sandbox)"
git -C "$SB_C" add -A >/dev/null 2>&1
sandbox_commit "$SB_C" base
cp "$SB_C/private/recording.transcript.txt" "$SB_C/docs/dirty.txt"
git -C "$SB_C" add -f docs/dirty.txt >/dev/null 2>&1
sandbox_commit "$SB_C" dirty
printf 'export const Y = 2;\n' >> "$SB_C/src/config.js"
commit_case "a -m message containing -a is not a -a commit" 0 "$SB_C" \
    "git -C $SB_C commit -m 'handle -a properly'"
printf '%s09%s59%s Dana%s one more line of speech appended to the transcript\n' '[' ':' ']' ':' >> "$SB_C/docs/dirty.txt"
commit_case "a real -a commit DOES see the modified tracked file" 2 "$SB_C" \
    "git -C $SB_C commit -am wip"
rm -rf "$SB_C"

# A binary blob must never be handed to the text scanner.
SB_C="$(make_default_sandbox)"
git -C "$SB_C" add -A >/dev/null 2>&1
sandbox_commit "$SB_C" base
printf 'PK\003\004\000\000\000\000binary' > "$SB_C/docs/thing.bin"
git -C "$SB_C" add -A >/dev/null 2>&1
commit_case "a binary blob is skipped, not scanned" 0 "$SB_C"
rm -rf "$SB_C"

# ---------------------------------------------------------------------------
# (h) WHAT THE COMMAND IS ABOUT TO STAGE — the not-yet-in-the-index shapes.
#
# This hook runs BEFORE the command it inspects, so for `git add X && git
# commit` nothing is staged at the moment of the check. Measured 2026-08-30
# against the shipped guard: every shape below PASSED with a wholly-new
# directory of transcripts sitting in the working tree, because `git diff
# --cached` was empty and the guard exited 0 having read nothing.
#
# The directory is the point. `git status --porcelain` reports a wholly-new
# directory as ONE entry — `?? docs/session-notes/` — which is a path with no
# bytes behind it, so an enumeration that stops there examines zero and reports
# clean. The controls under it matter as much: a guard that starts refusing
# ordinary `git add src/newmod && git commit` is a guard somebody switches off.
# ---------------------------------------------------------------------------
new_dir_of_speech() { # <sandbox>
    mkdir -p "$1/docs/session-notes"
    make_transcript > "$1/docs/session-notes/part1.txt"
    make_transcript > "$1/docs/session-notes/part2.txt"
}

SB_S="$(make_default_sandbox)"
git -C "$SB_S" add -A >/dev/null 2>&1
sandbox_commit "$SB_S" base

new_dir_of_speech "$SB_S"
git -C "$SB_S" add -A >/dev/null 2>&1
commit_case "a NEW DIRECTORY of transcripts, pre-staged, is refused" 2 "$SB_S"
git -C "$SB_S" reset -q --hard HEAD >/dev/null 2>&1; rm -rf "$SB_S/docs/session-notes"

new_dir_of_speech "$SB_S"
commit_case "git add <new dir> && git commit — one command — is refused" 2 "$SB_S" \
    "git -C $SB_S add docs/session-notes && git -C $SB_S commit -m notes"
commit_case "git add -A && git commit is refused"                       2 "$SB_S" \
    "git -C $SB_S add -A && git -C $SB_S commit -m notes"
commit_case "cd repo; git add . ; git commit is refused"                2 "$SB_S" \
    "cd $SB_S; git add . ; git commit -m notes"
rm -rf "$SB_S/docs/session-notes"

mkdir -p "$SB_S/docs/session notes"
make_transcript > "$SB_S/docs/session notes/part1.txt"
commit_case "a quoted pathspec containing a space is not a hole" 2 "$SB_S" \
    "git -C $SB_S add 'docs/session notes' && git -C $SB_S commit -m n"
rm -rf "$SB_S/docs/session notes"

# `git commit -- <path>` commits the WORKING TREE copy of a tracked file, which
# never passes through the index at all. Reading `git show :path` for it scans
# the bytes being replaced instead of the bytes being recorded.
printf '# notes\n\nordinary prose\n' > "$SB_S/docs/tracked.md"
git -C "$SB_S" add -A >/dev/null 2>&1
sandbox_commit "$SB_S" tracked
make_transcript > "$SB_S/docs/tracked.md"
commit_case "git commit -m x <tracked modified path> is refused" 2 "$SB_S" \
    "git -C $SB_S commit -m x docs/tracked.md"
commit_case "and so is a -a commit of the same modification"     2 "$SB_S" \
    "git -C $SB_S commit -am wip"
git -C "$SB_S" checkout -q -- docs/tracked.md

# --- controls: ordinary work must still commit -----------------------------
mkdir -p "$SB_S/src/newmod"
printf 'export const Y = 2;\n' > "$SB_S/src/newmod/index.js"
printf 'export const Z = 3;\n' > "$SB_S/src/newmod/other.js"
commit_case "CONTROL: an ordinary new directory, added and committed in one go" 0 "$SB_S" \
    "git -C $SB_S add src/newmod && git -C $SB_S commit -m mod"
rm -rf "$SB_S/src/newmod"

# The gitignored destination stays allowed even under `git add -A`, because git
# does not stage ignored paths — the correct home for private material is still
# the correct home.
make_transcript > "$SB_S/private/second.transcript.txt"
commit_case "CONTROL: git add -A does not drag in the gitignored private tree" 0 "$SB_S" \
    "git -C $SB_S add -A && git -C $SB_S commit -m ordinary"

SB_OTHER="$(make_default_sandbox)"
new_dir_of_speech "$SB_OTHER"
commit_case "CONTROL: a git add in ANOTHER repository does not widen this scan" 0 "$SB_S" \
    "git -C $SB_OTHER add -A && git -C $SB_S commit -m unrelated"
rm -rf "$SB_OTHER"

new_dir_of_speech "$SB_S"
commit_case "CONTROL: a git add AFTER the commit is not part of it" 0 "$SB_S" \
    "git -C $SB_S commit -m nothing ; git -C $SB_S add docs/session-notes"
rm -rf "$SB_S/docs/session-notes"

mkdir -p "$SB_S/docs/media"
printf 'PK\003\004\000\000\000\000binary' > "$SB_S/docs/media/thing.bin"
commit_case "CONTROL: a new directory of binary files commits" 0 "$SB_S" \
    "git -C $SB_S add docs/media && git -C $SB_S commit -m media"
rm -rf "$SB_S/docs/media"
rm -rf "$SB_S"

# --- the same walk-past, one level down: the scanner itself ----------------
# Every caller shares one predicate, so the expansion lives there and not in a
# hook. A directory handed to the scanner used to raise IsADirectoryError inside
# the unreadable-path branch and come back CLEAN.
scan_case() { # <name> <expect: BLOCK|CLEAN> <sandbox> <item-path> <label>
    local name="$1" want="$2" sb="$3" ipath="$4" label="$5" job out got
    job="$(mktemp "$SCRATCH/job.XXXXXX")"
    PB_JOB="$job" PB_SRC="$sb/private" PB_PATH="$ipath" PB_LABEL="$label" python3 -c '
import json, os
json.dump({"min_speech_lines": 8, "min_quote_words": 10,
           "corpus_max_files": 4000, "corpus_max_bytes": 67108864,
           "sources": [os.environ["PB_SRC"]],
           "items": [{"label": os.environ["PB_LABEL"], "path": os.environ["PB_PATH"]}]},
          open(os.environ["PB_JOB"], "w"))'
    # SCAN_OUT is left behind for the assertion that follows: the evidence a
    # directory finding carries is as much the contract as the verdict.
    SCAN_OUT="$(python3 "$ENGINE_ROOT/scripts/lib/publication-boundary.py" "$job" 2>/dev/null)"
    got="$(printf '%s' "$SCAN_OUT" | head -1 | cut -f1)"
    if [ "$got" = "$want" ]; then ok "$name"; else bad "$name (expected $want, got '$got')"; fi
}

SB_D="$(make_default_sandbox)"
new_dir_of_speech "$SB_D"
scan_case "the scanner handed a DIRECTORY expands it and blocks" \
    BLOCK "$SB_D" "$SB_D/docs/session-notes" "docs/session-notes/"
if printf '%s' "$SCAN_OUT" | grep -q "docs/session-notes/part2.txt"; then
    ok "and names the FILE inside it, not the directory"
else
    bad "a directory finding must name the file it came from"
fi
rm -rf "$SB_D/docs/session-notes"
mkdir -p "$SB_D/docs/media"
printf 'PK\003\004\000\000\000\000binary' > "$SB_D/docs/media/thing.bin"
scan_case "a directory of binary files scans clean, never fails closed on it" \
    CLEAN "$SB_D" "$SB_D/docs/media" "docs/media/"
rm -rf "$SB_D"

# ---------------------------------------------------------------------------
# (i) THE CORPUS CLOSURE — another RENDERING of a recording is the recording.
#
# Measured on the real private record 2026-08-30: 481 candidate files, and the
# shape filter kept TWO, while seven more two-channel transcripts of real
# recordings sat in the same tree carrying no timestamps and no speaker labels
# — whisper's plain `.txt` output. The verbatim-quote detector, which is the
# half that catches speech quoted inside prose, was matching against one
# recording.
#
# The closure admits a private file that REPRODUCES corpus speech in bulk, and
# refuses one that merely QUOTES it. Both halves are asserted below, because the
# second is what keeps the corpus clean: admitting a mixed engineering document
# puts its BOILERPLATE into the private corpus, and measured on the real tree
# that blocked LICENSE files, .gitignore, package.json and a brief the
# declaration itself names as deliberately public.
# ---------------------------------------------------------------------------
SB_CL="$(make_default_sandbox)"
WORDS_A="$(make_speech_lines 40 30 7 | tf)"      # the recording
WORDS_B="$(make_speech_lines 12 30 91 | tf)"     # only in the second rendering
make_long_transcript < "$WORDS_A" > "$SB_CL/private/long.transcript.txt"
cat "$WORDS_A" "$WORDS_B" > "$SB_CL/private/long.txt"

TAIL_ONLY="$({ printf '# Notes\n\n'; head -3 "$WORDS_B"; } | tf)"
write_case "text only in the untimestamped rendering is refused" 2 Write \
    "$SB_CL/docs/tail.md" "$SB_CL" "$TAIL_ONLY"

# The brief QUOTES one line of the recording and is otherwise its own prose. It
# must NOT join the corpus, so its own sentences stay ordinary work.
{
    printf '# Measurement notes\n\n'
    printf 'The parakeet loader keeps its own segment schema, which is the only\n'
    printf 'reason the integration cost is a loader and not a rewrite.\n\n> '
    head -1 "$WORDS_A"
} > "$SB_CL/private/brief.md"

BOILER="$({
    printf '# Viability\n\n'
    printf 'The parakeet loader keeps its own segment schema, which is the only\n'
    printf 'reason the integration cost is a loader and not a rewrite.\n'
} | tf)"
write_case "a private brief that only QUOTES does not join the corpus" 0 Write \
    "$SB_CL/docs/viability.md" "$SB_CL" "$BOILER"

QUOTED_LINE="$({ printf 'As recorded:\n\n> '; head -1 "$WORDS_A"; } | tf)"
write_case "and the line it quoted is still caught, from the transcript" 2 Write \
    "$SB_CL/docs/quote.md" "$SB_CL" "$QUOTED_LINE"

# A file with a single shared run is a quotation, not a rendering: 400 distinct
# shared runs and 8% coverage are both required, and both were measured.
SHORT_ECHO="$({ printf 'One line only:\n\n'; head -1 "$WORDS_A"; printf '\nand then ordinary engineering prose about loaders and schemas.\n'; } | tf)"
cp "$SHORT_ECHO" "$SB_CL/private/echo.md"
write_case "ordinary prose still writes cleanly with the corpus widened" 0 Write \
    "$SB_CL/src/config.js" "$SB_CL" "$CODE"
rm -rf "$SB_CL"

# ---------------------------------------------------------------------------
# (i2) MEDIA PROVENANCE — the FIRST rendering of a recording.
#
# THE HOLE THESE CASES PIN, measured 2026-08-30. The shape filter admits a
# transcript that LOOKS like one; the closure admits another rendering of
# something already admitted. Neither can admit the first and only rendering of
# a recording that was transcribed straight to plain text — whisper `.txt` has
# no timestamps and no speaker labels, so it has zero transcript-shaped lines,
# and the closure can only EXTEND a seed, never create one.
#
# On the real private record that left three podcast transcripts of two named
# third-party guests — 5,713, 6,424 and 22,375 words — entirely outside a corpus
# that held ten files. A 6,000-character extract of one of them was written into
# the publication-bound repository and BOTH guards returned exit 0 in silence:
# the commit guard, which exists precisely to catch what the write hook misses,
# runs the same predicate and missed it identically.
#
# The signal is provenance, not content: the transcript sits next to the
# recording it came from, under a name derived from it. The three refusal cases
# below are the precision controls, and they are the point — the WIDER rule
# ("any text file in a directory holding media") reaches the same corpus on the
# real tree by admitting a mixed 51 KB worksheet on a coincidence of directory,
# and admitting mixed documents on weak evidence is what once put engineering
# boilerplate into the private corpus and blocked LICENSE files.
# ---------------------------------------------------------------------------
SB_MP="$(make_default_sandbox)"
mkdir -p "$SB_MP/private/recordings"
# The media fixtures are never read — only their NAMES are — so a few bytes of
# ASCII stand in for audio and nothing here carries a recording.
printf 'not audio, only a name\n' > "$SB_MP/private/recordings/interview-002.mp3"
printf 'not audio, only a name\n' > "$SB_MP/private/recordings/audio.mp3"

# A plain-text rendering: real prose, and NOT ONE transcript-shaped line.
PLAIN="$(make_speech_lines 30 30 4242 | tf)"
cp "$PLAIN" "$SB_MP/private/recordings/interview-002 transcript.txt"

MP_QUOTE="$({ printf 'Notes from the session:\n\n'; head -1 "$PLAIN"; } | tf)"
msg_case "plain-text transcript beside its recording joins the corpus" \
    "reproduces private source material verbatim" Write \
    "$SB_MP/docs/session.md" "$SB_MP" "$MP_QUOTE"
write_case "  ... and the write is refused" 2 Write \
    "$SB_MP/docs/session.md" "$SB_MP" "$MP_QUOTE"

# CONTROL 1 — same directory, same media, UNRELATED stem. Provenance is the
# stem relation; sharing a directory with an mp3 is a coincidence, and a
# coincidence must not make an ordinary document private.
UNREL="$(make_speech_lines 30 30 909 | tf)"
cp "$UNREL" "$SB_MP/private/recordings/project-notes-and-decisions.txt"
UNREL_Q="$({ printf 'Notes:\n\n'; head -1 "$UNREL"; } | tf)"
write_case "an unrelated stem beside the same media does NOT join" 0 Write \
    "$SB_MP/docs/unrelated.md" "$SB_MP" "$UNREL_Q"

# CONTROL 2 — the stem floor. `audio` is five characters: a generic word that
# matches by accident rather than by naming, so it is below MEDIA_STEM_MIN_CHARS
# and does not seed. This case is what pins that constant.
SHORTSTEM="$(make_speech_lines 30 30 313 | tf)"
cp "$SHORTSTEM" "$SB_MP/private/recordings/audio.txt"
SHORT_Q="$({ printf 'Notes:\n\n'; head -1 "$SHORTSTEM"; } | tf)"
write_case "a stem below the floor does NOT join on a name collision" 0 Write \
    "$SB_MP/docs/shortstem.md" "$SB_MP" "$SHORT_Q"

# CONTROL 3 — no media at all beside it. Plain prose in a private tree is not
# speech, and the corpus must not bootstrap itself out of ordinary documents.
mkdir -p "$SB_MP/private/plain"
ORPHAN="$(make_speech_lines 30 30 5150 | tf)"
cp "$ORPHAN" "$SB_MP/private/plain/interview-002 transcript.txt"
ORPHAN_Q="$({ printf 'Notes:\n\n'; head -1 "$ORPHAN"; } | tf)"
write_case "the same name with NO media beside it does NOT join" 0 Write \
    "$SB_MP/docs/orphan.md" "$SB_MP" "$ORPHAN_Q"

write_case "ordinary source still writes cleanly with media seeding on" 0 Write \
    "$SB_MP/src/config.js" "$SB_MP" "$CODE"

# The commit arm runs the same predicate, and the 2026-08-30 probe proved it
# fails in exactly the same place — so it is asserted here rather than assumed.
cp "$MP_QUOTE" "$SB_MP/docs/session.md"
git -C "$SB_MP" add docs/session.md >/dev/null 2>&1
if bash_payload "git commit -m notes" "$SB_MP" | "$COMMIT_HOOK" >/dev/null 2>&1; then
    bad "the commit arm refuses the same plain-text rendering"
else
    ok "the commit arm refuses the same plain-text rendering"
fi
rm -rf "$SB_MP"

# ---------------------------------------------------------------------------
# (i3) THE VACUITY FLOOR — a scan that read NOTHING must never report CLEAN.
#
# The derived-from-private detector is conditional on the corpus: an empty
# corpus means an empty index, verbatim_run returns None, and the run prints
# CLEAN. A guard announcing it found no private material when it never had any
# private material to compare against — the "no media committed" check wearing
# a different hat, and the "18/18 suites" tally that described a glob instead
# of an inventory.
#
# This is not a hypothetical either: pb_resolve_sources records the day
# `../richos-hq` resolved, inside a linked worktree, to a path that does not
# exist. The sharpest detector this mechanism has went inert in exactly the
# place all the work happens, and the only symptom was one honest line in a
# message nobody reads on a PASS. That fix made the path resolve. These cases
# make the silence impossible.
#
# The LAST case is the negative control for this entire suite: it asserts the
# scanner reports what it examined, so a green run cannot mean "every case
# passed because the corpus was empty" — which is the very failure under test.
# ---------------------------------------------------------------------------
SB_V="$(make_sandbox "$(default_declaration)")"
rm -f "$SB_V/private/recording.transcript.txt"
make_speech_lines 30 30 2024 > "$SB_V/private/ordinary-engineering-notes.md"

write_case "a declared private tree with no speech in it is BROKEN" 2 Write \
    "$SB_V/docs/anything.md" "$SB_V" "$HELLO"
msg_case "  ... and it says it refuses to scan an empty corpus" \
    "Refusing to scan an empty corpus" Write \
    "$SB_V/docs/anything.md" "$SB_V" "$HELLO"
msg_case "  ... and it names the way through" \
    "CORPUS_MAY_BE_EMPTY=1" Write \
    "$SB_V/docs/anything.md" "$SB_V" "$HELLO"

# The commit arm shares the predicate, so it shares the floor.
cp "$HELLO" "$SB_V/docs/anything.md"
git -C "$SB_V" add docs/anything.md >/dev/null 2>&1
if bash_payload "git commit -m x" "$SB_V" | "$COMMIT_HOOK" >/dev/null 2>&1; then
    bad "the commit arm also refuses an empty corpus"
else
    ok "the commit arm also refuses an empty corpus"
fi
git -C "$SB_V" reset -q >/dev/null 2>&1
rm -rf "$SB_V"

# The sanctioned way through is committed and diffable, like ALLOWLIST and
# unlike an in-prompt token — the failure being fixed here was in-the-moment
# judgment, so there is no in-the-moment override.
SB_VE="$(make_sandbox "$(default_declaration)" 'CORPUS_MAY_BE_EMPTY=1')"
rm -f "$SB_VE/private/recording.transcript.txt"
write_case "CORPUS_MAY_BE_EMPTY=1 is the committed way through" 0 Write \
    "$SB_VE/docs/anything.md" "$SB_VE" "$HELLO"
rm -rf "$SB_VE"

SB_VB="$(make_sandbox "$(default_declaration)" 'CORPUS_MAY_BE_EMPTY=maybe')"
write_case "a non-boolean CORPUS_MAY_BE_EMPTY is BROKEN" 2 Write \
    "$SB_VB/docs/anything.md" "$SB_VB" "$HELLO"
msg_case "  ... and it says so by name" "CORPUS_MAY_BE_EMPTY must be 0 or 1" \
    Write "$SB_VB/docs/anything.md" "$SB_VB" "$HELLO"
rm -rf "$SB_VB"

# --- THE NEGATIVE CONTROL --------------------------------------------------
# Every case above asserts a VERDICT. A verdict is exactly what an empty corpus
# produces for free, so the suite also asserts the scan looked at something:
# the scanner reports its corpus size on the last line of every completed
# analysis, and these two cases read it.
SB_OR="$(make_default_sandbox)"
scan_corpus_trailer() { # <sources-dir> <content-file> -> "files<TAB>words"
    local job
    job="$(mktemp "$SCRATCH/job.XXXXXX")"
    PB_SRC_DIR="$1" PB_TXT="$2" PB_JOB="$job" python3 -c '
import json, os
job = {"min_speech_lines": 8, "min_quote_words": 10,
       "sources": [os.environ["PB_SRC_DIR"]],
       "items": [{"label": "probe", "path": os.environ["PB_TXT"]}]}
open(os.environ["PB_JOB"], "w", encoding="utf-8").write(json.dumps(job))
'
    python3 "$ENGINE_ROOT/scripts/lib/publication-boundary.py" "$job" \
        | grep '^CORPUS' | cut -f2,3
}

TRAILER="$(scan_corpus_trailer "$SB_OR/private" "$CODE")"
TR_FILES="$(printf '%s' "$TRAILER" | cut -f1)"
TR_WORDS="$(printf '%s' "$TRAILER" | cut -f2)"
if [ -n "$TR_FILES" ] && [ "$TR_FILES" -ge 1 ] 2>/dev/null &&
   [ -n "$TR_WORDS" ] && [ "$TR_WORDS" -gt 0 ] 2>/dev/null; then
    ok "a CLEAN verdict reports the non-empty corpus it examined"
else
    bad "a CLEAN verdict reports the non-empty corpus it examined (got files='$TR_FILES' words='$TR_WORDS')"
fi

# And the same trailer on a BLOCKING run, so the oracle is not itself
# conditional on the verdict.
TRANSCRIPT_F="$(make_transcript | tf)"
TRAILER_B="$(scan_corpus_trailer "$SB_OR/private" "$TRANSCRIPT_F")"
if [ -n "$TRAILER_B" ]; then
    ok "a BLOCKING verdict reports its corpus too"
else
    bad "a BLOCKING verdict reports its corpus too (no CORPUS line)"
fi
rm -rf "$SB_OR"

# ---------------------------------------------------------------------------
# (l) PRIVATE BY IDENTITY. The class of file the two scanners above score at
#     zero: a short note whose privacy is a fact about the file, not a property
#     of its prose. Every fixture here is SYNTHETIC — the file this mechanism
#     was seeded with must never appear in the repository built to keep it out,
#     and a suite that shipped it in order to prove it cannot ship would be the
#     failure it is testing for, executed.
# ---------------------------------------------------------------------------
echo "--- private by identity ---"

# make_identity_sandbox [extra-declaration-line ...] — a sandbox whose gitignored
# private/ tree holds a synthetic note, with that note declared in PRIVATE_FILES.
#
# The entry is MINTED BY THE SHIPPED MINTER rather than typed. A hand-written
# digest in a test proves the guard agrees with the test author; what has to be
# true is that the guard agrees with the tool an operator will actually use, so
# a drift between the two fails here first.
#
# The note's path is FIXED rather than returned. Every call site is
# `$(make_identity_sandbox)` — a subshell — so a variable set in here would be
# read back empty in the parent, which is the trap documented at the top of this
# file twice over.
IDENT_NOTE_REL="private/note-about-a-typeface.md"
make_identity_sandbox() {
    local sb line l
    sb="$(make_default_sandbox)"
    printf '# Wordmark\n\nDrawn in Example Sans Display, licensed to nobody.\n' \
        > "$sb/$IDENT_NOTE_REL"
    line="$(python3 "$ENGINE_ROOT/scripts/lib/publication-boundary.py" \
              --digest "$sb/$IDENT_NOTE_REL" | grep '^PRIVATE_FILES=')"
    printf '%s\n' "$line" >> "$sb/.publication-boundary"
    for l in "$@"; do printf '%s\n' "$l" >> "$sb/.publication-boundary"; done
    printf '%s' "$sb"
}

SB_ID="$(make_identity_sandbox)"
NOTE="$SB_ID/$IDENT_NOTE_REL"

# The minter has to have produced something, or every case below would pass by
# declaring nothing — the same shape as a scan over an empty corpus.
if grep -q '^PRIVATE_FILES="[0-9a-f]\{64\}:' "$SB_ID/.publication-boundary"; then
    ok "the shipped minter produces a well-formed PRIVATE_FILES entry"
else
    bad "the minter printed no usable PRIVATE_FILES entry (cases below are void)"
fi

# The round trip that matters: minted by the tool, honored by the guard.
write_case "the minted entry is honored: same name" 2 Write "$SB_ID/docs/note-about-a-typeface.md" "$SB_ID" "$NOTE"

# IDENTITY, not location: a rename is the obvious way past a path rule.
write_case "the same content under ANOTHER NAME" 2 Write "$SB_ID/docs/harmless-notes.md" "$SB_ID" "$NOTE"

# ... and the name rule is what survives a rewrite. A file with that name does
# not belong here whatever it says.
ORDINARY="$(printf 'Nothing private here. Ordinary prose about ordinary work.\n' | tf)"
write_case "the same NAME with different content" 2 Write "$SB_ID/docs/note-about-a-typeface.md" "$SB_ID" "$ORDINARY"

# REFORMATTING must not defeat it, which is why the digest covers the word
# sequence and not only the bytes. Uppercased, double-spaced, renamed.
LOUD="$(tr '[:lower:]' '[:upper:]' < "$NOTE" | awk '{print $0 "\n"}' | tf)"
write_case "reformatted, recased and renamed" 2 Write "$SB_ID/docs/loud.md" "$SB_ID" "$LOUD"

# THE SAME BYTES ARRIVING AS A STRING. A tool payload carries TEXT, not bytes,
# so UTF-16 content read as UTF-8 arrives with every character NUL-separated,
# and the word sequence of that is a run of single letters which matches
# nothing. The seeded file's own encoding is UTF-16, so this is the shape the
# real material would take on the way through a Write.
python3 - "$NOTE" "$SCRATCH/nul-separated.md" <<'PYEOF'
import sys
raw = open(sys.argv[1], 'rb').read()
carried = raw.decode('utf-8').encode('utf-16').decode('utf-8', 'ignore')
open(sys.argv[2], 'w', encoding='utf-8').write(carried)
PYEOF
write_case "the same words carried NUL-separated, renamed" 2 Write \
    "$SB_ID/docs/carried.md" "$SB_ID" "$SCRATCH/nul-separated.md"

# THE POSITIVE CONTROLS. A guard that blocks everything is satisfied by
# paranoia; these are what say it discriminates.
write_case "an ordinary file, with identity declared" 0 Write "$SB_ID/docs/ordinary.md" "$SB_ID" "$ORDINARY"
write_case "ordinary product code, with identity declared" 0 Write "$SB_ID/src/config.js" "$SB_ID" "$CODE"
write_case "a technology evaluation, with identity declared" 0 Write "$SB_ID/docs/viability.md" "$SB_ID" "$EVAL"

# A GITIGNORED DESTINATION IS NOT A WAY IN FOR THIS ONE. For speech it is the
# correct home — case (d) above, still true. For a file whose declaration says
# it lives in the private record, a rule that stopped at .gitignore would be a
# rule about publication when the instruction was about the file.
write_case "identity: gitignored destination is refused too" 2 Write "$SB_ID/private/copy-of-note.md" "$SB_ID" "$NOTE"
write_case "identity: an ordinary gitignored write is untouched" 0 Write "$SB_ID/private/scratch.md" "$SB_ID" "$ORDINARY"

# THE REFUSAL TEACHES THE RULE. A block the author cannot act on is a block the
# author routes around.
msg_case "the refusal names the declared file" "note-about-a-typeface.md" \
    Write "$SB_ID/docs/harmless-notes.md" "$SB_ID" "$NOTE"
msg_case "the refusal says it is identity, not resemblance" "DECLARED PRIVATE BY IDENTITY" \
    Write "$SB_ID/docs/harmless-notes.md" "$SB_ID" "$NOTE"
msg_case "the refusal says where it belongs" "the-private-record" \
    Write "$SB_ID/docs/harmless-notes.md" "$SB_ID" "$NOTE"
msg_case "the refusal names the only way through" "ALLOWLIST" \
    Write "$SB_ID/docs/harmless-notes.md" "$SB_ID" "$NOTE"
msg_case "the refusal rules out every form, including a gitignored one" \
    "enters this tree in no form" Write "$SB_ID/docs/harmless-notes.md" "$SB_ID" "$NOTE"

# The advice a transcript block gives is WRONG for this one — "rewrite it so it
# carries no speech" and "put it in a gitignored path" are both routes around
# the rule here — so the speech paragraph must not appear on an identity-only
# refusal. Two blocks, one message, and the author acting on the wrong half is
# how a guard teaches the wrong lesson.
OUT_ID="$(write_payload Write "$SB_ID/docs/harmless-notes.md" "$SB_ID" "$NOTE" | "$WRITE_HOOK" 2>&1 >/dev/null)"
if printf '%s' "$OUT_ID" | grep -qF 'WHERE IT SHOULD GO'; then
    bad "an identity-only refusal still prints the speech guidance"
else
    ok "an identity-only refusal does not print the speech guidance"
fi

# THE REFUSAL DOES NOT REPRODUCE WHAT IT IS PROTECTING. An error message that
# quoted the private file in order to say the private file may not be copied
# would be this mechanism's own defect, one level up.
if printf '%s' "$OUT_ID" | grep -qF 'Example Sans Display'; then
    bad "the identity refusal quotes the private content back"
else
    ok "the identity refusal never quotes the content it protects"
fi

rm -rf "$SB_ID"

# THE COMMITTED WAY THROUGH still works, and it is the only one.
SB_IDA="$(make_identity_sandbox 'ALLOWLIST="docs/sanctioned"')"
write_case "identity: an ALLOWLISTed prefix is exempt" 0 Write \
    "$SB_IDA/docs/sanctioned/note-about-a-typeface.md" "$SB_IDA" "$SB_IDA/$IDENT_NOTE_REL"
rm -rf "$SB_IDA"

# THE COMMIT HALF. Everything above is a tool call; these files arrive by `cp`,
# which is how 137 of the 2026-08-29 files arrived.
SB_IDC="$(make_identity_sandbox)"
cp "$SB_IDC/$IDENT_NOTE_REL" "$SB_IDC/docs/copied-under-a-new-name.md"
git -C "$SB_IDC" add -A >/dev/null 2>&1
commit_case "identity: a staged copy under a new name is refused" 2 "$SB_IDC"
git -C "$SB_IDC" rm -q --cached docs/copied-under-a-new-name.md >/dev/null 2>&1
rm -f "$SB_IDC/docs/copied-under-a-new-name.md"

# THE UTF-16 CASE, WHICH IS THE SEEDED FILE'S OWN ENCODING. A file out of a Mac
# text editor carries NUL bytes, every text-shaped filter in this pipeline calls
# it binary, and the commit guard used to drop binaries before anything looked
# at them. A rule that says "this exact file never enters the repository" and
# then discards the file is enforcement in name only.
python3 - "$SB_IDC/$IDENT_NOTE_REL" "$SCRATCH/utf16-copy.md" <<'PYEOF'
import sys
raw = open(sys.argv[1], 'rb').read()
open(sys.argv[2], 'wb').write(raw.decode('utf-8').encode('utf-16'))
PYEOF
cp "$SCRATCH/utf16-copy.md" "$SB_IDC/docs/reencoded.md"
git -C "$SB_IDC" add -A >/dev/null 2>&1
commit_case "identity: a UTF-16 re-encoding, which reads as binary, is refused" 2 "$SB_IDC"
git -C "$SB_IDC" rm -q --cached docs/reencoded.md >/dev/null 2>&1
rm -f "$SB_IDC/docs/reencoded.md"

# ... and the control that keeps that from being paranoia: ordinary work still
# commits with identity declared.
printf 'export const TIER = "turbo";\n' > "$SB_IDC/src/newmod.js"
git -C "$SB_IDC" add -A >/dev/null 2>&1
commit_case "identity: an ordinary staged file still commits" 0 "$SB_IDC"
rm -rf "$SB_IDC"

# ACCUMULATION. Declaring the next file must be an ADDED LINE — a one-line diff
# is what makes a committed exemption reviewable at a glance — so the key
# repeats and the entries accumulate.
SB_ID2="$(make_identity_sandbox)"
printf 'A second note, about something else entirely.\n' > "$SB_ID2/private/second-note.md"
python3 "$ENGINE_ROOT/scripts/lib/publication-boundary.py" --digest "$SB_ID2/private/second-note.md" \
    | grep '^PRIVATE_FILES=' >> "$SB_ID2/.publication-boundary"
write_case "two PRIVATE_FILES lines: the first still blocks" 2 Write \
    "$SB_ID2/docs/a.md" "$SB_ID2" "$SB_ID2/$IDENT_NOTE_REL"
write_case "two PRIVATE_FILES lines: the second blocks too" 2 Write \
    "$SB_ID2/docs/b.md" "$SB_ID2" "$SB_ID2/private/second-note.md"
rm -rf "$SB_ID2"

# ... and every OTHER key refuses a second occurrence, because last-line-wins
# would silently drop the first and an operator who wrote both believes both.
HELLO_ID="$(printf 'hello world\n' | tf)"
SB_DUP="$(make_sandbox "$(default_declaration)" 'PRIVATE_SOURCES="private"')"
write_case "a key set twice is BROKEN, not last-line-wins" 2 Write \
    "$SB_DUP/docs/anything.md" "$SB_DUP" "$HELLO_ID"
msg_case "  ... and it names the key" "'PRIVATE_SOURCES' is set more than once" \
    Write "$SB_DUP/docs/anything.md" "$SB_DUP" "$HELLO_ID"
rm -rf "$SB_DUP"

# MALFORMED ENTRIES ARE BROKEN, LOUDLY. An operator who mistypes a digest
# believes a file is protected; the only safe answer to that belief is a
# refusal, never a silently skipped entry.
for bad_entry in \
    'PRIVATE_FILES="not-a-digest-at-all"' \
    'PRIVATE_FILES="abc123:short.md"' \
    'PRIVATE_FILES="zz7384288415b5f49551594b09dbb7d0ea076526852d599e23c1bb776bd7d300:x.md"' \
    'PRIVATE_FILES="ac7384288415b5f49551594b09dbb7d0ea076526852d599e23c1bb776bd7d300:"'
do
    SB_BAD="$(make_sandbox "$(default_declaration)" "$bad_entry")"
    write_case "malformed PRIVATE_FILES is BROKEN: $bad_entry" 2 Write \
        "$SB_BAD/docs/anything.md" "$SB_BAD" "$HELLO_ID"
    msg_case "  ... and the refusal names PRIVATE_FILES" "PRIVATE_FILES entry" \
        Write "$SB_BAD/docs/anything.md" "$SB_BAD" "$HELLO_ID"
    rm -rf "$SB_BAD"
done

# THE ORACLE, for identity. An identity-only run reads no corpus at all, so
# CORPUS 0 0 is all a caller would see, and "compared against nothing" would
# look exactly like "compared against one declared file". The scanner reports
# how many declarations it actually loaded.
identity_trailer() { # <entries-json> <content-file> -> declarations examined
    local job
    job="$(mktemp "$SCRATCH/idjob.XXXXXX")"
    PB_PF="$1" PB_TXT="$2" PB_JOB="$job" python3 -c '
import json, os
job = {"min_speech_lines": 8, "min_quote_words": 10, "sources": [],
       "private_files": json.loads(os.environ["PB_PF"]),
       "items": [{"label": "probe", "path": os.environ["PB_TXT"],
                  "identity_only": True}]}
open(os.environ["PB_JOB"], "w", encoding="utf-8").write(json.dumps(job))
'
    python3 "$ENGINE_ROOT/scripts/lib/publication-boundary.py" "$job" \
        | grep '^IDENTITY' | cut -f2
}
IDT="$(identity_trailer '["ac7384288415b5f49551594b09dbb7d0ea076526852d599e23c1bb776bd7d300:x.md"]' "$ORDINARY")"
if [ "$IDT" = "1" ]; then
    ok "a CLEAN identity run reports the declarations it compared against"
else
    bad "identity oracle: expected 1 declaration examined, got '$IDT'"
fi
IDT0="$(identity_trailer '[]' "$ORDINARY")"
if [ "$IDT0" = "0" ]; then
    ok "and reports ZERO when nothing was declared, so silence is visible"
else
    bad "identity oracle: expected 0 with no declarations, got '$IDT0'"
fi

# ---------------------------------------------------------------------------
# (j) FAIL-OPEN / FAIL-CLOSED conventions.
# ---------------------------------------------------------------------------
# Seated on a sandbox copy of the adoption, never the engine: an unreadable payload appends a
# line to <entity>/.claude/state/unevaluated-payloads.log (ci-shard.sh STATE-WRITTEN, 2026-10-05).
. "$SCRIPT_DIR/../lib/entity-sandbox.sh"
ENT_MALFORMED="$(entity_sandbox "$ENGINE_ROOT")"
rc=0; printf 'not json at all' | RICHOS_ENTITY_ROOT="$ENT_MALFORMED" "$WRITE_HOOK" >/dev/null 2>&1 || rc=$?
rm -rf "$ENT_MALFORMED"
if [ "$rc" -eq 0 ]; then ok "malformed payload fails OPEN (sibling convention)"
else bad "malformed payload should fail open, got $rc"; fi

rc=0; printf '{"tool_name":"Read","tool_input":{}}' | "$WRITE_HOOK" >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then ok "a non-write tool is untouched"
else bad "non-write tool should pass, got $rc"; fi

# python3 missing -> fail CLOSED. The fakebin carries every tool the guard needs
# EXCEPT python3, and the hook is invoked through an ABSOLUTE bash: invoking it
# by shebang would make `env` itself unresolvable and the case would "pass" on
# exit 127 — a different failure wearing the right exit code.
make_fakebin_no_python3() {
    local dir t p
    dir="$(mktemp -d -t pubtest-nopy.XXXXXX)"
    for t in cat grep sed cut tr mkdir git mktemp basename dirname rm ln awk sort uniq wc head tail env; do
        p="$(command -v "$t" 2>/dev/null || true)"
        [ -n "$p" ] && ln -sf "$p" "$dir/$t"
    done
    printf '%s' "$dir"
}
FAKEBIN="$(make_fakebin_no_python3)"
rc=0
out="$(printf '{}' | PATH="$FAKEBIN" "$BASH_BIN" "$WRITE_HOOK" 2>&1 >/dev/null)" || rc=$?
if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -qF 'python3'; then
    ok "write guard: python3 missing fails CLOSED and names the interpreter"
else
    bad "write guard python3-missing should fail closed naming python3 (rc=$rc)"
fi
rc=0
out="$(printf '{}' | PATH="$FAKEBIN" "$BASH_BIN" "$COMMIT_HOOK" 2>&1 >/dev/null)" || rc=$?
if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -qF 'python3'; then
    ok "commit guard: python3 missing fails CLOSED and names the interpreter"
else
    bad "commit guard python3-missing should fail closed naming python3 (rc=$rc)"
fi
rm -rf "$FAKEBIN"

# The predicate library must be present, and its absence must be LOUD.
TMPENG="$(mktemp -d -t pubtest-eng.XXXXXX)"
mkdir -p "$TMPENG/scripts/hooks" "$TMPENG/scripts/lib"
cp "$WRITE_HOOK" "$TMPENG/scripts/hooks/"
cp "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" "$ENGINE_ROOT/scripts/lib/seat-jurisdiction.sh" "$TMPENG/scripts/lib/"
rc=0
out="$(printf '{"tool_name":"Write","tool_input":{"file_path":"/tmp/x","content":"y"}}' \
       | "$BASH_BIN" "$TMPENG/scripts/hooks/guard-publication-writes.sh" 2>&1 >/dev/null)" || rc=$?
if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -qF "publication-boundary.sh is missing"; then
    ok "a missing predicate library is a LOUD refusal, never a quiet skip"
else
    bad "missing predicate library should block loudly (rc=$rc)"
fi
rm -rf "$TMPENG"

# ---------------------------------------------------------------------------
# (m) THE GROUPED DECLARATION DIRECTORY.
# ---------------------------------------------------------------------------
# The declaration may live at `.richos/publication-boundary` rather than at the
# repository root. Every case above already proves the root form, so what these
# have to prove is the dangerous half: THE MOVE MUST NOT STAND THE GUARD DOWN.
#
# That is why the first case is a BLOCK and not a load. A guard that found the
# file and then decided nothing would satisfy any test that only asked whether
# the file was found — and a stood-down publication guard is byte-identical, at
# every place an operator looks, to one that is working.
SBG="$(make_default_sandbox)"
mkdir -p "$SBG/.richos"
mv "$SBG/.publication-boundary" "$SBG/.richos/publication-boundary"
write_case "grouped declaration: a transcript is still REFUSED" 2 Write "$SBG/docs/t.txt" "$SBG" "$TRANSCRIPT"
write_case "grouped declaration: ordinary product code still passes" 0 Write "$SBG/src/config.js" "$SBG" "$CODE"
msg_case "grouped declaration: the refusal names the file it actually read" \
    ".richos/publication-boundary" Write "$SBG/docs/t.txt" "$SBG" "$TRANSCRIPT"

# Both at once is BROKEN, and BROKEN blocks. Choosing one quietly is how the
# wrong one stays live.
cp "$SBG/.richos/publication-boundary" "$SBG/.publication-boundary"
write_case "declared in BOTH places at once: BLOCKS" 2 Write "$SBG/src/config.js" "$SBG" "$CODE"
msg_case "...and says which two files disagree" \
    "carries BOTH .richos/publication-boundary and .publication-boundary" \
    Write "$SBG/src/config.js" "$SBG" "$CODE"
rm -f "$SBG/.publication-boundary"

# A DECLARATION in `.richos/` that nothing resolves is BROKEN too. Without this,
# moving a declaration this engine does not read from there switches its
# contract off in silence — the one failure this directory must not introduce.
printf 'TODO_RECORD="x"\n' > "$SBG/.richos/ceo-todos"
write_case "an unresolved DECLARATION in .richos/: BLOCKS" 2 Write "$SBG/src/config.js" "$SBG" "$CODE"
msg_case "...and names the declaration that nothing reads" \
    ".richos/ceo-todos is a declaration, and nothing reads a declaration from there" \
    Write "$SBG/src/config.js" "$SBG" "$CODE"
rm -f "$SBG/.richos/ceo-todos"

# ...and everything else in there is somebody else's. `.richos/` is a SHARED
# RichOS directory: the ECS entity manifest has sat at `.richos/entity.json` in
# femcboost since 2026-08-27. The first draft of this rule policed the whole
# directory, and it would have BLOCKED EVERY WRITE in that repository — a guard
# taking an adopter offline over files that were never its business.
printf '{"schemaVersion":1,"entityId":"sample"}\n' > "$SBG/.richos/entity.json"
printf '# what lives here\n' > "$SBG/.richos/README.md"
write_case "entity.json and a README share .richos/ and nothing blocks" 0 Write "$SBG/src/config.js" "$SBG" "$CODE"
write_case "...and the boundary is still enforced beside them" 2 Write "$SBG/docs/t.txt" "$SBG" "$TRANSCRIPT"
rm -rf "$SBG"

# The RESOLVER itself is sidecar-hashed. Hashing the guards and leaving the file
# that decides WHERE their contract lives unverified checks the lock and ignores
# the key — the same sentence install.sh carries beside every entry on that list.
if grep -q "scripts/lib/declaration-path.sh" "$ENGINE_ROOT/scripts/hooks/install.sh" 2>/dev/null; then
    ok "scripts/lib/declaration-path.sh is sidecar-hashed by install.sh"
else
    bad "scripts/lib/declaration-path.sh NOT hashed by install.sh"
fi

# ---------------------------------------------------------------------------
# (p) ALREADY PUBLISHED — a private record quoting a PUBLIC line does not make
#     the line private (2026-09-25).
# ---------------------------------------------------------------------------
# native-ios-app.test.sh could not be committed at all: its existing line 118
# was quoted whole in a private record, and proof-run.py had the same problem
# with console logs (esc-20260924T234750Z-6bbc3ae9). Text the public remote's
# main already carries, in the same file, is not a new disclosure. A new private
# phrase added to that file still is, and without a published copy nothing is
# exempt. Each rule has a mutation this section must catch.
PUBLIC_LINE="the runner stopped every check it started and kept each log beside its summary"
NEW_PRIVATE="okay so the thing I wanted to walk through today is the migration"
make_published_sandbox() {  # a repo whose origin/main carries PUBLIC_LINE in src/runner.sh
    local sb
    sb="$(make_default_sandbox)"
    # The private record quotes the public line: one more speaker line of the
    # recording, assembled at run time like every fixture here.
    printf '%s00%s59%s %s%s %s\n' '[' ':' ']' "Dana" ':' "$PUBLIC_LINE" >> "$sb/private/recording.transcript.txt"
    printf '#!/bin/sh\necho "%s"\n' "$PUBLIC_LINE" > "$sb/src/runner.sh"
    git -C "$sb" add -A >/dev/null 2>&1
    sandbox_commit "$sb" public
    git -C "$sb" update-ref refs/remotes/origin/main HEAD
    printf '%s' "$sb"
}
SB_P="$(make_published_sandbox)"
printf 'echo done\n' >> "$SB_P/src/runner.sh"
git -C "$SB_P" add src/runner.sh
commit_case "published: a file whose public line a private record quotes still commits" 0 "$SB_P"
printf 'echo "%s"\n' "$NEW_PRIVATE" >> "$SB_P/src/runner.sh"
git -C "$SB_P" add src/runner.sh
commit_case "published: a NEW private phrase added to that public file is refused" 2 "$SB_P"
git -C "$SB_P" checkout -q HEAD -- src/runner.sh
printf 'echo done\n' >> "$SB_P/src/runner.sh"
git -C "$SB_P" add src/runner.sh
cp "$SB_P/src/runner.sh" "$SB_P/src/copy.sh"
git -C "$SB_P" add src/copy.sh
commit_case "published: the exemption is the SAME path's published copy (a new file carrying the line is refused)" 2 "$SB_P"
git -C "$SB_P" rm -q --cached src/copy.sh; rm -f "$SB_P/src/copy.sh"
git -C "$SB_P" update-ref -d refs/remotes/origin/main
commit_case "published: without a published copy the same line is refused (the corpus does hold it)" 2 "$SB_P"
git -C "$SB_P" update-ref refs/remotes/origin/main HEAD

# The mutations run a copy of the guard and its libraries with one rule broken.
mutant_engine() {  # mutant_engine <dir> <file under scripts/> <python replace expression over s>
    mkdir -p "$1/scripts"
    cp -R "$ENGINE_ROOT/scripts/lib" "$1/scripts/lib"
    mkdir -p "$1/scripts/hooks"
    cp "$COMMIT_HOOK" "$1/scripts/hooks/guard-publication-commits.sh"
    python3 - "$1/scripts/$2" "$3" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
t = eval(sys.argv[2], {"s": s})
assert t != s, "mutation did not apply"
open(p, "w").write(t)
PY
}
mutant_case() {  # mutant_case <name> <expect-rc-of-the-MUTANT> <mutant engine> <sandbox>
    local rc
    bash_payload "git -C $4 commit -m msg" "$4" | "$3/scripts/hooks/guard-publication-commits.sh" >/dev/null 2>&1
    rc=$?
    if [ "$rc" -eq "$2" ]; then ok "$1"; else bad "$1 (the mutant exited $rc; the case above would not catch it)"; fi
}
MUT_A="$SCRATCH/mutant-no-exemption"
if mutant_engine "$MUT_A" lib/publication-boundary.py 's.replace("        if window in published:     # already public: not a new disclosure\n            continue\n", "")'; then
    mutant_case "MUTATION caught: with the exemption removed, the published-line commit is refused" 2 "$MUT_A" "$SB_P"
else bad "MUTATION the exemption line is no longer where the mutation expects it"; fi
MUT_B="$SCRATCH/mutant-exempt-all"
if mutant_engine "$MUT_B" lib/publication-boundary.py 's.replace("if window in published:", "if published:")'; then
    printf 'echo "%s"\n' "$NEW_PRIVATE" >> "$SB_P/src/runner.sh"
    git -C "$SB_P" add src/runner.sh
    mutant_case "MUTATION caught: exempting any file that has a published copy lets the new private phrase through" 0 "$MUT_B" "$SB_P"
    git -C "$SB_P" checkout -q HEAD -- src/runner.sh
    printf 'echo done\n' >> "$SB_P/src/runner.sh"
    git -C "$SB_P" add src/runner.sh
else bad "MUTATION the published test is no longer where the mutation expects it"; fi
MUT_C="$SCRATCH/mutant-no-published-copy"
if mutant_engine "$MUT_C" hooks/guard-publication-commits.sh 's.replace("item[\"published_path\"] = parts[3]", "pass")'; then
    mutant_case "MUTATION caught: a guard that does not hand over the published copy refuses the published-line commit" 2 "$MUT_C" "$SB_P"
else bad "MUTATION the published-copy hand-over is no longer where the mutation expects it"; fi
rm -rf "$SB_P" "$MUT_A" "$MUT_B" "$MUT_C"

# ---------------------------------------------------------------------------
# (k) REGISTRATION — both surfaces, or the engine ships a guard nobody loads.
# ---------------------------------------------------------------------------
for g in guard-publication-writes.sh guard-publication-commits.sh; do
    if hook_enforced_on_surface "$ENGINE_ROOT/hooks/hooks.json" "$g"; then
        ok "$g registered in hooks/hooks.json (plugin surface)"
    else
        bad "$g NOT registered in hooks/hooks.json"
    fi
    if hook_enforced_on_surface "$ENGINE_ROOT/.claude/settings.local.json" "$g"; then
        ok "$g registered in .claude/settings.local.json (seated surface)"
    else
        bad "$g NOT registered in .claude/settings.local.json"
    fi
    if grep -q "$g" "$ENGINE_ROOT/scripts/hooks/contract-integrity-probe.sh" 2>/dev/null; then
        ok "$g declared in the probe's BR_EXPECTED oracle"
    else
        bad "$g NOT declared in the probe's managed set"
    fi
done

# ---------------------------------------------------------------------------
# (n) THE OWNER'S OWN TEST DEVICES. A serial reached four public audit files and
# no scanner flagged it. The identifiers live in a private list read at check
# time; the ids below are SYNTHETIC — the real serial must never enter this file.
# ---------------------------------------------------------------------------
DI_ID="ZZDEVSERIAL0042"
DI_LIST="$SCRATCH/device-identifiers"
printf '# synthetic test device\n%s\n' "$DI_ID" > "$DI_LIST"
SB_DI="$(make_default_sandbox)"
di_commit() { # <name> <expect-rc> <list-file> [needle-in-stderr]
    local out rc
    out="$(bash_payload "git -C $SB_DI commit -m msg" "$SB_DI" \
        | RICHOS_DEVICE_IDENTIFIERS_FILE="$3" "$COMMIT_HOOK" 2>&1 >/dev/null)"
    rc=$?
    if [ "$rc" -ne "$2" ]; then bad "$1 (expected exit $2, got $rc)"; return; fi
    if [ -n "${4:-}" ] && ! printf '%s' "$out" | grep -qF -- "$4"; then
        bad "$1 (stderr did not say \"$4\")"; return
    fi
    if printf '%s' "$out" | grep -qF -- "$DI_ID"; then bad "$1 (the refusal printed the identifier)"; return; fi
    ok "$1"
}
printf 'Devices: phone (serial %s)\n' "$DI_ID" > "$SB_DI/docs/audit.md"
git -C "$SB_DI" add docs/audit.md
di_commit "device id: a staged serial is refused, with the fix named" 2 "$DI_LIST" "neutral label"
printf 'Devices: phone (serial %s)\n' "$(printf '%s' "$DI_ID" | tr 'A-Z' 'a-z')" > "$SB_DI/docs/audit.md"
git -C "$SB_DI" add docs/audit.md
di_commit "device id: the match is case-insensitive" 2 "$DI_LIST"
printf 'Devices: phone (serial <android-phone>)\n' > "$SB_DI/docs/audit.md"
git -C "$SB_DI" add docs/audit.md
di_commit "device id: a neutral label commits" 0 "$DI_LIST"
printf 'Devices: phone (serial %s)\n' "$DI_ID" > "$SB_DI/docs/audit.md"
git -C "$SB_DI" add docs/audit.md
di_commit "device id: a public clone WITHOUT the list still runs, says what it could not check" 0 "$SCRATCH/no-such-list" "NOT CHECKED"
mkdir -p "$SB_DI/inside"; printf '%s\n' "$DI_ID" > "$SB_DI/inside/list"
di_commit "device id: a list inside a git work tree is BROKEN and refuses" 2 "$SB_DI/inside/list" "work tree"
printf 'short\n' > "$SCRATCH/short-list"
di_commit "device id: an entry shorter than 8 characters is BROKEN and refuses" 2 "$SCRATCH/short-list" "shorter"
rm -rf "$SB_DI"
# The predicate itself (scripts/lib/device-identifiers.py): the list path honors the override and defaults outside every repository.
if [ "$(RICHOS_DEVICE_IDENTIFIERS_FILE=/x/y python3 "$ENGINE_ROOT/scripts/lib/device-identifiers.py" --list-path)" = "/x/y" ] &&
   [ "$(env -u RICHOS_DEVICE_IDENTIFIERS_FILE HOME=/h python3 "$ENGINE_ROOT/scripts/lib/device-identifiers.py" --list-path)" = "/h/.richos-privacy/device-identifiers" ]; then
    ok "device id: device-identifiers.py --list-path honors the override and defaults to ~/.richos-privacy"
else
    bad "device id: device-identifiers.py --list-path"
fi

# --scan-tree: the merge gate's whole-index read (autocheck land_check). An identifier that is
# ALREADY in a branch's tree (committed before the list existed, or carried in by a merge) is
# found with file:line and a masked preview, never the identifier; a clean tree passes.
DI_PY="$ENGINE_ROOT/scripts/lib/device-identifiers.py"
DI_TREE="$SCRATCH/di-tree"
rm -rf "$DI_TREE"; mkdir -p "$DI_TREE/docs"
git -C "$DI_TREE" init -q
printf 'one\ntwo\nphone (serial %s)\n' "$DI_ID" > "$DI_TREE/docs/old.md"
printf 'nothing here\n' > "$DI_TREE/docs/clean.md"
git -C "$DI_TREE" add -A
di_tree() { RICHOS_DEVICE_IDENTIFIERS_FILE="$1" python3 "$DI_PY" --scan-tree "$DI_TREE"; }
DI_OUT="$(di_tree "$DI_LIST")"
if [ "$(printf '%s' "$DI_OUT" | sed -n 1p)" = "FOUND" ] && printf '%s' "$DI_OUT" | grep -qF 'docs/old.md:3' &&
   ! printf '%s' "$DI_OUT" | grep -qF "$DI_ID"; then
    ok "device id: --scan-tree finds a listed identifier already in the tree, names file:line, never prints it"
else
    bad "device id: --scan-tree on a tree holding an identifier ($DI_OUT)"
fi
printf 'phone (serial <android-phone>)\n' > "$DI_TREE/docs/old.md"
git -C "$DI_TREE" add -A
if [ "$(di_tree "$DI_LIST")" = "CLEAN" ]; then ok "device id: --scan-tree passes a tree without a listed identifier"; else bad "device id: --scan-tree clean tree"; fi
if [ "$(di_tree "$SCRATCH/no-such-list" | cut -f1)" = "ABSENT" ]; then ok "device id: --scan-tree with no list says ABSENT, not CLEAN"; else bad "device id: --scan-tree ABSENT"; fi
rm -rf "$DI_TREE"

# A private record must reject oversized staged text before it breaks public scans.
if python3 - "$SCRATCH/private-source-index" "$ENGINE_ROOT/scripts/lib/publication-boundary.py" <<'PY_PRIVATE_INDEX'
import gzip,hashlib,json,os,subprocess,sys,shlex,importlib.util
from pathlib import Path
ROOT=Path(sys.argv[1]);ROOT.mkdir()
CHECKER=Path(sys.argv[2])
spec=importlib.util.spec_from_file_location("private_commit_checker",CHECKER)
checker=importlib.util.module_from_spec(spec);spec.loader.exec_module(checker)
LIMIT=checker.SOURCE_MAX_BYTES
assert LIMIT==8000000
env=dict(os.environ,GIT_CONFIG_GLOBAL='/dev/null',GIT_CONFIG_NOSYSTEM='1')
results=[]
def git(repo,*args):
 return subprocess.run(['git','-C',str(repo),'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','-c','commit.gpgSign=false',*args],env=env,capture_output=True,text=True)
def install(repo):
 hook=repo/'.git/hooks/pre-commit'
 hook.write_text('#!/bin/sh\nexec '+shlex.quote(sys.executable)+' '+shlex.quote(str(CHECKER))+' --check-staged-private-sizes\n');hook.chmod(0o755)
def setup(name,hook):
 p=ROOT/name;p.mkdir();assert git(p,'init','-q','-b','main').returncode==0
 assert git(p,'commit','--allow-empty','-qm','Fixture').returncode==0
 if hook:install(p)
 return p
def commit(repo,name,data,expected,stage_only=False):
 p=repo/name;p.write_bytes(data);assert git(repo,'add','--',name).returncode==0
 if stage_only:p.write_bytes(b'working tree is small; index remains oversized')
 r=git(repo,'commit','-qm','Synthetic source size case');assert (r.returncode==0)==expected,(name,r.returncode,r.stderr)
 results.append({'repository':repo.name,'file':name,'bytes':len(data),'accepted':r.returncode==0,'stage_only':stage_only,'stderr':r.stderr})
 return r
original=setup('original',False)
commit(original,'oversized.json',b'x'*(LIMIT+1),True)
# The exact committed baseline is recoverable without widening the corpus limit.
install(original)
assert git(original,'rm','--','oversized.json').returncode==0
assert git(original,'commit','-qm','Remove old oversized source').returncode==0
candidate=setup('candidate with trailing space ',True)
commit(candidate,'large file\nwith spaces.json',b'x'*(LIMIT+1),False,True)
assert git(candidate,'reset','--','large file\nwith spaces.json').returncode==0
commit(candidate,'boundary.json',b'x'*LIMIT,True)
commit(candidate,'oversized.TXT',b'x'*(LIMIT+1),False)
assert git(candidate,'reset','--','oversized.TXT').returncode==0
commit(candidate,'evidence.json.gz',gzip.compress(b'x'*(LIMIT+1),mtime=0),True)
link=candidate/'converted.json';link.symlink_to('target');assert git(candidate,'add','converted.json').returncode==0;assert git(candidate,'commit','-qm','Synthetic symlink').returncode==0
link.unlink();commit(candidate,'converted.json',b'x'*(LIMIT+1),False);assert git(candidate,'reset','--','converted.json').returncode==0
hook=candidate/'.git/hooks/pre-commit';saved=hook.read_text();hook.write_text('#!/bin/sh\nexec '+shlex.quote(sys.executable)+' '+shlex.quote(str(ROOT/'missing-checker.py'))+' --check-staged-private-sizes\n')
try:commit(candidate,'missing-checker.txt',b'ordinary',False)
finally:hook.write_text(saved)
(ROOT/'result.json').write_text(json.dumps({'results':results,'deleting_old_oversized_source_allowed':True,'candidate_sha256':hashlib.sha256(CHECKER.read_bytes()).hexdigest(),'verdict':'Unprotected Git admits oversized source; native candidate hook refuses staged oversized text, permits exact boundary, compression and cleanup. No corpus scan or new limit.'},indent=2)+'\n');print('PASS actual native Git hook, exact boundary, staged-vs-working distinction, unusual filenames and compression')
PY_PRIVATE_INDEX
then
    ok "native private commit: staged blob boundary, partial staging, compression and cleanup"
else
    bad "native private commit: staged source size check"
fi

rm -rf "$SB"

# The kept corpus (publication-boundary.py, THE BUILT CORPUS IS KEPT ON DISK):
# no cache, a cold cache and a warm cache give one answer, and an edited,
# removed, new or unreadable private file is noticed.
if python3 "$ENGINE_ROOT/scripts/lib/publication-boundary-cache.test.py" >"$SCRATCH/cache-test.log" 2>&1; then
    ok "kept corpus: same verdict cold, warm and uncached; every private change noticed"
else
    sed 's/^/    /' "$SCRATCH/cache-test.log"
    bad "kept corpus: publication-boundary-cache.test.py"
fi

echo ""
TOTAL=$((PASS + FAIL))
if [ "$FAIL" -eq 0 ]; then
    printf '  %s/%s cases passed\n' "$PASS" "$TOTAL"
    exit 0
fi
printf '  %s/%s cases passed — %s FAILED\n' "$PASS" "$TOTAL" "$FAIL"
exit 1
