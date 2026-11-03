#!/usr/bin/env bash
#
# named-persons.test.sh — regression tests for the named-person deny-list:
# scripts/lib/named-persons.py, scripts/lib/named-persons.sh, the two guards
# that run the predicate (guard-named-persons-writes.sh on Write/Edit/
# MultiEdit/NotebookEdit, guard-named-persons-commands.sh on Bash) and the
# release-time front end scripts/named-persons.sh.
#
# ONE suite for the whole mechanism, following publication-boundary.test.sh:
# it is one subject — a predicate and its three chokepoints — and splitting it
# would create three places to remember to update, which is the defect class
# this mechanism exists to remove.
#
# EVERY FIXTURE NAME IN THIS FILE IS INVENTED. The people this list actually
# protects never appear here — not in a fixture, not in a comment, not in an
# assertion. A suite that shipped a real client's name in order to prove a real
# client's name must not ship would be the failure executed, and it would put
# that name in the one file most likely to be read by a stranger.
#
# Covers:
#   (a) STAND-DOWN — a repository with no .publication-boundary is untouched.
#       The precision floor: get it wrong and this fires in every repository on
#       the machine, including the ones holding the private material.
#   (b) FILE CONTENT — a listed name refused across all four write tools.
#   (c) THE FILE PATH — a listed name in the DESTINATION PATH alone, with clean
#       content, is refused. This is the shape the real incident took, and a
#       guard that only read content would have passed it.
#   (d) NORMALIZATION — the same name in underscores, hyphens, dots, camelCase,
#       percent-encoding, reversed order, upper case and with an accent is one
#       match. Each of these is a real filename or citation shape.
#   (e) POSITIVE CONTROLS — the given name alone, the surname alone, a
#       different pairing, an initial-plus-surname, and ordinary prose all pass.
#       These matter more than the blocks: a guard that refuses ordinary work
#       gets switched off, and then protects nothing.
#   (f) A GITIGNORED DESTINATION is allowed. It is the CORRECT home for a note
#       about a real person, exactly as it is for the private audio.
#   (g) THE COMMIT MESSAGE — the surface the original scrub made WORSE. `-m`,
#       `--message=`, a heredoc body and `-F <file>` all refused; a clean commit
#       passes.
#   (h) THE BRANCH NAME — checkout -b, switch -c, branch, worktree add -b and
#       push all refused; an ordinary branch passes.
#   (i) THE PR / ISSUE TITLE — gh pr create, gh issue create, gh release create
#       refused; a clean gh command passes.
#   (j) BASH PRECISION — reading, grepping and building a file that mentions a
#       listed name are NOT publishing acts and are never touched. Nothing here
#       makes a name unmentionable; it makes a name unpublishable.
#   (k) THE MISSING LIST IS LOUD, AND IT IS ITS OWN TEST. With no list the write
#       guard exits 0 and SAYS SO, naming the file; the release check REFUSES
#       with exit 2. A missing list that read as "no names to check" is the
#       false-green class this repository has been bitten by repeatedly.
#   (l) BROKEN LISTS — a list inside a git repository, an empty list, an unknown
#       keyword, a single-token `name:`, a too-short `token:` and a malformed
#       digest all BLOCK rather than degrading quietly.
#   (m) THE DIGEST FORM — a sha256 entry blocks the same content, so an operator
#       who will not keep plaintext on disk loses nothing.
#   (n) THE BLOCK MESSAGE NEVER CARRIES THE NAME. Asserted directly against the
#       stderr, because a refusal that repeats the leak is a second copy of it.
#   (o) FAIL-OPEN on an unparseable payload, matching the hook family.
#   (p) REGISTRATION on all four surfaces, or the engine ships a guard nobody
#       loads and a probe that never counts it.
#   (q) THE ORACLE — the suite proves its own fixture list actually loaded. A
#       green run in which every case passed because there was nothing to
#       compare against is the failure this whole file is about.
#
# Run directly: scripts/hooks/named-persons.test.sh
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


WRITE_HOOK="$SCRIPT_DIR/guard-named-persons-writes.sh"
CMD_HOOK="$SCRIPT_DIR/guard-named-persons-commands.sh"
PREDICATE="$ENGINE_ROOT/scripts/lib/named-persons.py"
FRONTEND="$ENGINE_ROOT/scripts/named-persons.sh"

unset CLAUDE_PROJECT_DIR

PASS=0
FAIL=0
SCRATCH="$(mktemp -d "${TMPDIR:?}/nptest.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT

ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

# --- the fixture roster, invented ------------------------------------------
# Assembled from parts at run time for the same reason publication-boundary's
# transcript fixture is: a literal roster sitting in a file is the shape this
# mechanism refuses, and a future maintainer must never be able to mistake
# these for the real list.
F_GIVEN="Quillan"
F_FAMILY="Marchetti"
F_FULL="$F_GIVEN $F_FAMILY"
F_ORG_A="Verrick"
F_ORG_B="Dynamics"
F_ORG="$F_ORG_A $F_ORG_B"
F_OTHER="Bramwell Oakes"       # never listed — the control for "a name is not a match"

LIST="$SCRATCH/named-persons"
{
    printf '# fixture list\n'
    printf 'name: %s\n' "$F_FULL"
    printf 'org: %s\n' "$F_ORG"
    printf 'token: %s\n' "Zephyrine"
} > "$LIST"
chmod 600 "$LIST"
export RICHOS_NAMED_PERSONS_FILE="$LIST"

# --- (q) THE ORACLE, first, because everything below is conditional on it ---
DOC="$(python3 "$PREDICATE" --doctor 2>&1)"
if printf '%s' "$DOC" | grep -q '^OK'; then
    ok "oracle: the fixture list loads"
else
    bad "oracle: the fixture list did NOT load — every case below would pass over nothing"
fi
if printf '%s' "$DOC" | grep -q 'entries       : 3'; then
    ok "oracle: 3 entries compared against, not 0"
else
    bad "oracle: entry count is not 3 ($(printf '%s' "$DOC" | sed -n '2p'))"
fi

# --- sandboxes --------------------------------------------------------------
make_repo() { # <declare-boundary: yes|no>
    local sb
    sb="$(mktemp -d "$SCRATCH/repo.XXXXXX")"
    git -C "$sb" init -q
    git -C "$sb" config user.email t@example.com
    git -C "$sb" config user.name T
    git -C "$sb" config commit.gpgsign false
    mkdir -p "$sb/src" "$sb/docs" "$sb/private"
    printf '/private/\n' > "$sb/.gitignore"
    printf 'export const A = 1;\n' > "$sb/src/a.js"
    git -C "$sb" add -A >/dev/null 2>&1
    git -C "$sb" commit -qm init --no-verify >/dev/null 2>&1
    if [ "$1" = "yes" ]; then
        printf 'PRIVATE_RECORD="the-private-record"\n' > "$sb/.publication-boundary"
    fi
    printf '%s' "$sb"
}

SB="$(make_repo yes)"      # publication-bound
SB_NO="$(make_repo no)"    # not publication-bound

write_payload() { # <tool> <file_path> <cwd> <content>
    NP_T="$1" NP_F="$2" NP_C="$3" NP_S="$4" python3 -c '
import json, os
tool = os.environ["NP_T"]; content = os.environ["NP_S"]
ti = {"file_path": os.environ["NP_F"]}
if tool == "Write":
    ti["content"] = content
elif tool == "Edit":
    ti["old_string"] = "PLACEHOLDER"; ti["new_string"] = content
elif tool == "MultiEdit":
    ti["edits"] = [{"old_string": "PLACEHOLDER", "new_string": content}]
elif tool == "NotebookEdit":
    ti = {"notebook_path": os.environ["NP_F"], "new_source": content}
print(json.dumps({"tool_name": tool, "cwd": os.environ["NP_C"], "tool_input": ti}))
'
}

bash_payload() { # <command> <cwd>
    NP_CMD="$1" NP_C="$2" python3 -c '
import json, os
print(json.dumps({"tool_name": "Bash", "cwd": os.environ["NP_C"],
                  "tool_input": {"command": os.environ["NP_CMD"]}}))
'
}

wcase() { # <name> <expect-rc> <tool> <dest> <sandbox> <content>
    local name="$1" want="$2" rc=0
    write_payload "$3" "$4" "$5" "$6" | "$WRITE_HOOK" >/dev/null 2>&1 || rc=$?
    if [ "$rc" -eq "$want" ]; then ok "$name"; else bad "$name (expected exit $want, got $rc)"; fi
}

# wcase_with — the same, against a DIFFERENT list file. Written as a function
# taking an env override rather than a nested `bash -c`: the first version
# built the payload inside a single-quoted `bash -c` string containing a
# double-quoted python program, and the quoting collapsed so quietly that two
# cases were silently asserting against the wrong content.
wcase_with() { # <list-file> <name> <expect-rc> <dest> <sandbox> <content>
    local lst="$1" name="$2" want="$3" rc=0
    RICHOS_NAMED_PERSONS_FILE="$lst" write_payload Write "$4" "$5" "$6" \
        | RICHOS_NAMED_PERSONS_FILE="$lst" "$WRITE_HOOK" >/dev/null 2>&1 || rc=$?
    if [ "$rc" -eq "$want" ]; then ok "$name"; else bad "$name (expected exit $want, got $rc)"; fi
}

ccase() { # <name> <expect-rc> <command> <sandbox>
    local name="$1" want="$2" rc=0
    bash_payload "$3" "$4" | "$CMD_HOOK" >/dev/null 2>&1 || rc=$?
    if [ "$rc" -eq "$want" ]; then ok "$name"; else bad "$name (expected exit $want, got $rc)"; fi
}

echo "=== named-person deny-list ==="

# ---------------------------------------------------------------------------
# (a) STAND-DOWN — the precision floor.
# ---------------------------------------------------------------------------
wcase "(a) no .publication-boundary: a listed name is NOT this guard's business" \
      0 Write "$SB_NO/docs/note.md" "$SB_NO" "A meeting with $F_FULL."
ccase "(a) no .publication-boundary: a commit message is not touched" \
      0 "git -C $SB_NO commit -m 'notes from $F_FULL'" "$SB_NO"

# ---------------------------------------------------------------------------
# (b) FILE CONTENT — every write tool.
# ---------------------------------------------------------------------------
for tool in Write Edit MultiEdit NotebookEdit; do
    wcase "(b) $tool carrying a listed name is REFUSED" \
          2 "$tool" "$SB/docs/note.md" "$SB" "Notes from the call with $F_FULL about scope."
done
wcase "(b) a listed ORGANIZATION is refused too" \
      2 Write "$SB/docs/org.md" "$SB" "Copyright $F_ORG, all rights reserved."
wcase "(b) a listed single TOKEN is refused" \
      2 Write "$SB/docs/tok.md" "$SB" "See the Zephyrine appendix for detail."

# ---------------------------------------------------------------------------
# (c) THE FILE PATH — the shape the real incident took.
# ---------------------------------------------------------------------------
LOWER_UNDER="$(printf '%s_%s' "$F_GIVEN" "$F_FAMILY" | tr 'A-Z' 'a-z')"
wcase "(c) a listed name in the DESTINATION PATH, with clean content, is refused" \
      2 Write "$SB/docs/${LOWER_UNDER}_webinar_2026.md" "$SB" "Nothing identifying in here at all."

# ---------------------------------------------------------------------------
# (d) NORMALIZATION — one name, many spellings.
# ---------------------------------------------------------------------------
wcase "(d) underscores"      2 Write "$SB/docs/n1.md" "$SB" "source=${LOWER_UNDER}_raw.mp3"
wcase "(d) hyphens"          2 Write "$SB/docs/n2.md" "$SB" "$(printf '%s-%s' "$F_GIVEN" "$F_FAMILY" | tr 'A-Z' 'a-z')"
wcase "(d) dots"             2 Write "$SB/docs/n3.md" "$SB" "$(printf '%s.%s@example.com' "$F_GIVEN" "$F_FAMILY" | tr 'A-Z' 'a-z')"
wcase "(d) camelCase"        2 Write "$SB/docs/n4.md" "$SB" "const speaker = ${F_GIVEN}${F_FAMILY};"
wcase "(d) percent-encoded"  2 Write "$SB/docs/n5.md" "$SB" "https://example.com/${F_GIVEN}%20${F_FAMILY}"
wcase "(d) upper case"       2 Write "$SB/docs/n6.md" "$SB" "$(printf '%s' "$F_FULL" | tr 'a-z' 'A-Z')"
wcase "(d) reversed, citation order" 2 Write "$SB/docs/n7.md" "$SB" "$F_FAMILY, $F_GIVEN (2026)."
wcase "(d) possessive"       2 Write "$SB/docs/n8.md" "$SB" "This was ${F_FULL}'s own recording."
wcase "(d) across a line break" 2 Write "$SB/docs/n9.md" "$SB" "spoke to $F_GIVEN
$F_FAMILY yesterday"

# ---------------------------------------------------------------------------
# (e) POSITIVE CONTROLS — the expensive failure is the false one.
# ---------------------------------------------------------------------------
wcase "(e) the GIVEN NAME alone passes (a common first name must not block prose)" \
      0 Write "$SB/docs/p1.md" "$SB" "$F_GIVEN asked whether the build was green."
wcase "(e) the SURNAME alone passes" \
      0 Write "$SB/docs/p2.md" "$SB" "The $F_FAMILY algorithm is unrelated to any person."
wcase "(e) a DIFFERENT pairing of the same parts passes" \
      0 Write "$SB/docs/p3.md" "$SB" "$F_GIVEN Hollis and Dara $F_FAMILY are not on the list."
wcase "(e) an unlisted full name passes" \
      0 Write "$SB/docs/p4.md" "$SB" "Reviewed with $F_OTHER this morning."
wcase "(e) initial-plus-surname passes (the documented gap, asserted rather than assumed)" \
      0 Write "$SB/docs/p5.md" "$SB" "Reviewed with ${F_GIVEN:0:1}. $F_FAMILY."
wcase "(e) ordinary source passes" \
      0 Write "$SB/src/b.js" "$SB" "export const MAX = 10;"
wcase "(e) half the organization name passes" \
      0 Write "$SB/docs/p6.md" "$SB" "The $F_ORG_A protocol has nothing to do with anybody."

# ---------------------------------------------------------------------------
# (f) A GITIGNORED DESTINATION is the correct home, not an exception.
# ---------------------------------------------------------------------------
wcase "(f) the same name into a gitignored path is ALLOWED" \
      0 Write "$SB/private/notes.md" "$SB" "Call notes: $F_FULL, $F_ORG."

# ---------------------------------------------------------------------------
# (g) THE COMMIT MESSAGE — the surface the original scrub made worse.
# ---------------------------------------------------------------------------
ccase "(g) git commit -m carrying a listed name is REFUSED" \
      2 "git -C $SB commit -m 'remove $F_FULL from the fixture'" "$SB"
ccase "(g) --message= form is refused" \
      2 "git -C $SB commit --message='scrub $F_FULL'" "$SB"
ccase "(g) a heredoc body is refused" \
      2 "git -C $SB commit -F - <<'EOF'
scrub the name

It named $F_FULL of $F_ORG.
EOF" "$SB"
MSGF="$SCRATCH/msg.txt"
printf 'scrub the fixture\n\nIt named %s.\n' "$F_FULL" > "$MSGF"
ccase "(g) -F <file> is read, so the message in a file is refused" \
      2 "git -C $SB commit -F $MSGF" "$SB"
ccase "(g) an ordinary commit message passes" \
      0 "git -C $SB commit -m 'take the speaker filenames from the environment'" "$SB"
ccase "(g) git tag -m carrying a listed name is refused" \
      2 "git -C $SB tag -a v1 -m 'thanks to $F_FULL'" "$SB"

# ---------------------------------------------------------------------------
# A shell script carried on stdin is not limited to one execve argument.
# Keep the fixture payload out of argv/environment so the test reaches the guard.
for PAD in 0 150000; do
    for MODE in listed clean; do
        python3 - "$SB" "$F_FULL" "$PAD" "$MODE" "$SCRATCH/large-command.sh" > "$SCRATCH/large-command.json" <<'NP_TRANSPORT'
import json, sys
name = sys.argv[2] if sys.argv[4] == 'listed' else 'ordinary implementation change'
command = ('# ' + 'x' * int(sys.argv[3]) + '\n' if int(sys.argv[3]) else '') + 'git commit -m "' + name + '"'
if int(sys.argv[3]):
    assert len(command.encode()) > 131072
with open(sys.argv[5], 'w') as f:
    f.write(command)
print(json.dumps({'tool_name': 'Bash', 'cwd': sys.argv[1], 'tool_input': {'command': command}}))
NP_TRANSPORT
        bash -n < "$SCRATCH/large-command.sh" || { bad "large command fixture is not valid shell syntax"; continue; }
        OUT="$(bash "$CMD_HOOK" < "$SCRATCH/large-command.json" 2>&1)"; RC=$?
        WANT=0; [ "$MODE" = listed ] && WANT=2
        if [ "$RC" -eq "$WANT" ] && ! grep -F 'Argument list too long' <<< "$OUT" >/dev/null &&
           { [ "$MODE" != listed ] || grep -F 'NAMED-PERSON DENY-LIST: BLOCKED' <<< "$OUT" >/dev/null; }; then
            ok "command transport preserves $MODE decision (padding=$PAD)"
        else
            bad "command transport lost $MODE decision (padding=$PAD expected=$WANT rc=$RC): $OUT"
        fi
        if grep -F "$F_FULL" <<< "$OUT" >/dev/null; then
            bad "command transport refusal repeated the listed name"
        fi
    done
done
# Interpreter failure must not be mistaken for an unrelated command.
REAL_NP_PYTHON="$(command -v python3)"
mkdir -p "$SCRATCH/classifier-failure"
cat > "$SCRATCH/classifier-failure/python3" <<'NP_FAILURE'
#!/usr/bin/env bash
case "${2:-}" in *'a branch name'*) exit 73 ;; esac
exec "$TRANSPORT_REAL_PYTHON" "$@"
NP_FAILURE
chmod +x "$SCRATCH/classifier-failure/python3"
bash_payload 'git commit -m ordinary' "$SB" > "$SCRATCH/classifier-failure.json"
OUT="$(PATH="$SCRATCH/classifier-failure:$PATH" TRANSPORT_REAL_PYTHON="$REAL_NP_PYTHON" bash "$CMD_HOOK" < "$SCRATCH/classifier-failure.json" 2>&1)"; RC=$?
if [ "$RC" -eq 2 ] && grep -F 'command classifier could not run' <<< "$OUT" >/dev/null; then
    ok "command classifier execution failure refuses explicitly"
else
    bad "command classifier execution failure silently passed (rc=$RC): $OUT"
fi

# (h) THE BRANCH NAME.
# ---------------------------------------------------------------------------
BR="$(printf 'fix-%s-%s' "$F_GIVEN" "$F_FAMILY" | tr 'A-Z' 'a-z')"
ccase "(h) git checkout -b <listed name> is refused" 2 "git -C $SB checkout -b $BR" "$SB"
ccase "(h) git switch -c <listed name> is refused"   2 "git -C $SB switch -c $BR" "$SB"
ccase "(h) git branch <listed name> is refused"      2 "git -C $SB branch $BR" "$SB"
ccase "(h) git worktree add -b <listed name> is refused" 2 "git -C $SB worktree add -b $BR /tmp/x" "$SB"
ccase "(h) git push of a branch named after somebody is refused" 2 "git -C $SB push origin $BR" "$SB"
ccase "(h) an ordinary branch name passes" 0 "git -C $SB checkout -b fix-splash-flake" "$SB"

# ---------------------------------------------------------------------------
# (i) THE PR / ISSUE TITLE.
# ---------------------------------------------------------------------------
ccase "(i) gh pr create --title carrying a listed name is refused" \
      2 "cd $SB && gh pr create --title 'scrub $F_FULL' --body x" "$SB"
ccase "(i) gh issue create -t is refused" \
      2 "cd $SB && gh issue create -t 'reported by $F_FULL' -b x" "$SB"
ccase "(i) gh release create notes are refused" \
      2 "cd $SB && gh release create v1 --notes 'with thanks to $F_ORG'" "$SB"
ccase "(i) an ordinary gh command passes" \
      0 "cd $SB && gh pr create --title 'take filenames from the environment' --body x" "$SB"

# ---------------------------------------------------------------------------
# (j) BASH PRECISION — a name is unpublishable, never unmentionable.
# ---------------------------------------------------------------------------
ccase "(j) reading a file that mentions a listed name is untouched" \
      0 "cat $SB/private/notes.md | grep '$F_FULL'" "$SB"
ccase "(j) an unrelated build command is untouched" \
      0 "npm run build -- --name '$F_FULL'" "$SB"
ccase "(j) git status is untouched" 0 "git -C $SB status --porcelain" "$SB"
ccase "(j) git log is untouched even when the history holds the name" \
      0 "git -C $SB log --grep '$F_FULL'" "$SB"

# ---------------------------------------------------------------------------
# (n) THE BLOCK MESSAGE NEVER CARRIES THE NAME.
# ---------------------------------------------------------------------------
OUT="$(write_payload Write "$SB/docs/leak.md" "$SB" "A call with $F_FULL." | "$WRITE_HOOK" 2>&1 >/dev/null)"
if printf '%s' "$OUT" | grep -qF "$F_FULL"; then
    bad "(n) the block message REPEATS the name — a second copy of the leak"
else
    ok "(n) the block message does not carry the name in full"
fi
if printf '%s' "$OUT" | grep -q 'redacted:'; then
    ok "(n) the block message says which entry fired, redacted"
else
    bad "(n) the block message names no entry, so nobody can act on it"
fi
if printf '%s' "$OUT" | grep -qF "$(printf '%s' "$LIST")"; then
    ok "(n) the block message names the list to edit"
else
    bad "(n) the block message does not say where the list is"
fi

# THE PATH CASE, and it is here because it was a real defect rather than a
# hypothetical one. The banner names the artifact it is refusing; when the
# artifact is a file path, the name is IN that path. Running the guard against
# the real repository — not a sandbox — printed the very name it had just
# refused, in full, on the surface the original incident used.
OUT="$(write_payload Write "$SB/docs/${LOWER_UNDER}_webinar_2026.md" "$SB" "clean body" \
       | "$WRITE_HOOK" 2>&1 >/dev/null)"
if printf '%s' "$OUT" | grep -qiF "$LOWER_UNDER"; then
    bad "(n) the block message PRINTS THE PATH IT REFUSED, name and all"
else
    ok "(n) a path hit is masked in the block message too"
fi
if printf '%s' "$OUT" | grep -q 'webinar_2026'; then
    ok "(n) the masked path is still recognizable (only the name is replaced)"
else
    bad "(n) masking destroyed the path, so the author cannot tell which file"
fi

# The masker on its own, both directions.
MASKED="$(python3 "$PREDICATE" --mask "a/$LOWER_UNDER/b")"
if printf '%s' "$MASKED" | grep -qiF "$F_FAMILY"; then
    bad "(n) --mask left the name in place"
else
    ok "(n) --mask replaces the name"
fi
if printf '%s' "$MASKED" | grep -q '^a/' && printf '%s' "$MASKED" | grep -q '/b$'; then
    ok "(n) --mask leaves everything around the name untouched"
else
    bad "(n) --mask damaged the surrounding text ($MASKED)"
fi
UNTOUCHED="$(python3 "$PREDICATE" --mask "docs/ordinary-file-name.md")"
if [ "$UNTOUCHED" = "docs/ordinary-file-name.md" ]; then
    ok "(n) --mask is byte-identical on text with no match"
else
    bad "(n) --mask altered text containing no listed name ($UNTOUCHED)"
fi

# ---------------------------------------------------------------------------
# (m) THE DIGEST FORM — no plaintext on disk, same block.
# ---------------------------------------------------------------------------
DLIST="$SCRATCH/digest-list"
python3 "$PREDICATE" --mint "$F_FULL" | grep '^sha256:' > "$DLIST"
chmod 600 "$DLIST"
wcase_with "$DLIST" "(m) a sha256-only list blocks the same content" \
           2 "$SB/docs/d.md" "$SB" "A call with $F_FULL."
wcase_with "$DLIST" "(m) a sha256-only list does not block an unlisted name" \
           0 "$SB/docs/d2.md" "$SB" "A call with $F_OTHER."
wcase_with "$DLIST" "(m) a sha256-only list matches the reversed order too" \
           2 "$SB/docs/d3.md" "$SB" "$F_FAMILY, $F_GIVEN (2026)."

# ---------------------------------------------------------------------------
# (k) THE MISSING LIST IS LOUD — its own test, in both directions.
# ---------------------------------------------------------------------------
GONE="$SCRATCH/no-such-list"
# THE ANNOUNCEMENT IS CAPTURED FIRST, and the order is load-bearing: the banner
# is said once per hook per repository per session, so whichever call runs first
# is the only one that carries it. Asserting on the second call would report a
# SILENT stand-down over a mechanism working exactly as designed — a false red
# in the one test whose whole subject is a false green.
rc=0
OUT="$(write_payload Write "$SB/docs/k.md" "$SB" "A call with $F_FULL." \
       | RICHOS_NAMED_PERSONS_FILE="$GONE" "$WRITE_HOOK" 2>&1 >/dev/null)" || rc=$?
if [ "$rc" -eq 0 ]; then ok "(k) a missing list does not brick a fresh clone (write passes)"
else bad "(k) a missing list blocked a write (exit $rc) — a stranger's clone would be unusable"; fi
if printf '%s' "$OUT" | grep -q 'ABSENT — NOTHING WAS CHECKED'; then
    ok "(k) a missing list ANNOUNCES itself — not a silent pass"
else
    bad "(k) a missing list was SILENT. That is the false green this exists to remove."
fi
if printf '%s' "$OUT" | grep -qF "$GONE"; then
    ok "(k) the announcement names the file it expected"
else
    bad "(k) the announcement does not say which file is missing"
fi
rc=0
RICHOS_NAMED_PERSONS_FILE="$GONE" "$FRONTEND" --tree --repo "$SB" >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 2 ]; then ok "(k) the RELEASE check REFUSES on a missing list"
else bad "(k) the release check returned $rc on a missing list — it must refuse"; fi

# ---------------------------------------------------------------------------
# (l) BROKEN LISTS — every one blocks.
# ---------------------------------------------------------------------------
broken_case() { # <name> <content-of-list>
    local nm="$1" body="$2" f
    f="$(mktemp "$SCRATCH/broken.XXXXXX")"
    printf '%s\n' "$body" > "$f"
    # Content deliberately innocuous: a BROKEN list must block a write that has
    # nothing wrong with it, because the fault is the list, not the write.
    wcase_with "$f" "(l) $nm BLOCKS" 2 "$SB/docs/l.md" "$SB" "ordinary text"
}
broken_case "an empty list"                  "# nothing here"
broken_case "an unknown keyword"             "person: Somebody Else"
broken_case "a single-token name: entry"     "name: Quillan"
broken_case "a too-short token: entry"       "token: Qu"
broken_case "a malformed digest"             "sha256:2:not-a-digest"
broken_case "a digest with no token count"   "sha256:abcdef"

# A list that lives INSIDE a repository is the failure this whole location
# decision exists to prevent, so it is refused by construction rather than by
# instruction.
INREPO="$SB/docs/named-persons"
printf 'name: %s\n' "$F_FULL" > "$INREPO"
wcase_with "$INREPO" "(l) a deny-list INSIDE a git repository BLOCKS" \
           2 "$SB/docs/l2.md" "$SB" "ordinary text"
rm -f "$INREPO"

# ---------------------------------------------------------------------------
# (o) FAIL-OPEN on an unparseable payload, matching the family.
# ---------------------------------------------------------------------------
rc=0
printf 'not json at all' | "$WRITE_HOOK" >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then ok "(o) malformed payload fails OPEN (sibling convention)"
else bad "(o) malformed payload should fail open, got $rc"; fi

# ---------------------------------------------------------------------------
# release front end, clean direction
# ---------------------------------------------------------------------------
rc=0
"$FRONTEND" --tree --repo "$SB" >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then ok "(r) --tree is CLEAN over a repository with no listed name"
else bad "(r) --tree returned $rc over a clean repository"; fi
printf 'A call with %s.\n' "$F_FULL" > "$SB/docs/tracked.md"
git -C "$SB" add docs/tracked.md >/dev/null 2>&1
git -C "$SB" commit -qm add --no-verify >/dev/null 2>&1
rc=0
"$FRONTEND" --tree --repo "$SB" >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 1 ]; then ok "(r) --tree REFUSES a tree carrying a listed name (exit 1)"
else bad "(r) --tree returned $rc over a tree carrying a listed name"; fi

# ---------------------------------------------------------------------------
# (p) REGISTRATION — four surfaces, or the engine ships a guard nobody loads.
# ---------------------------------------------------------------------------
for g in guard-named-persons-writes.sh guard-named-persons-commands.sh; do
    if hook_enforced_on_surface "$ENGINE_ROOT/hooks/hooks.json" "$g"; then
        ok "(p) $g registered in hooks/hooks.json (plugin surface)"
    else
        bad "(p) $g NOT registered in hooks/hooks.json"
    fi
    if hook_enforced_on_surface "$ENGINE_ROOT/.claude/settings.local.json" "$g"; then
        ok "(p) $g registered in .claude/settings.local.json (seated surface)"
    else
        bad "(p) $g NOT registered in .claude/settings.local.json"
    fi
    if grep -q "$g" "$ENGINE_ROOT/scripts/hooks/contract-integrity-probe.sh" 2>/dev/null; then
        ok "(p) $g declared in the probe's BR_EXPECTED oracle"
    else
        bad "(p) $g NOT declared in the probe's managed set"
    fi
done
# INVERTED 2026-09-14 WITH LAYER R. This used to assert the hook was NAMED in a
# typed R_ROOTED_HOOKS. Layer R now derives the hooks it walks from
# hooks/hooks.json, so being registered IS being walked and there is no list to
# be named in. What can still go wrong is the opposite: somebody exempting these
# hooks as rootless, which would silently take their bootstrap back out of the
# comparison. So the assertion is that they are NOT exempted.
_np_rootless="$(sed -n '/^    R_ROOTLESS_HOOKS="/,/"$/p' \
    "$ENGINE_ROOT/scripts/hooks/contract-integrity-probe.sh" 2>/dev/null || true)"
if [ -z "$_np_rootless" ]; then
    bad "(p) could not read R_ROOTLESS_HOOKS from the probe — its shape changed, and a membership test against a block this suite can no longer locate would pass everything"
else
    for h in guard-named-persons-writes guard-named-persons-commands; do
        if printf '%s' "$_np_rootless" | grep -q "\b$h\b"; then
            bad "(p) $h is EXEMPTED in R_ROOTLESS_HOOKS — Layer R skips it, so its bootstrap is never checked for divergence"
        else
            ok "(p) $h is not exempted, so Layer R derives it from the registration and checks its bootstrap"
        fi
    done
fi
for f in scripts/lib/named-persons.sh scripts/lib/named-persons.py; do
    if grep -q "$f" "$ENGINE_ROOT/scripts/hooks/install.sh" 2>/dev/null; then
        ok "(p) $f is sidecar-hashed by install.sh"
    else
        bad "(p) $f NOT hashed by install.sh — the file that makes the decision is unverified"
    fi
done

# Default seed discovery follows the main checkout, including from worktrees.
# Only synthetic entities are used; no operator HQ is read by these cases.
SEED_PARENT="$SCRATCH/seed space"
SEED_REPO="$SEED_PARENT/product"
SEED_SCRIPTS="$SEED_REPO/richos/engine/scripts"
mkdir -p "$SEED_SCRIPTS/lib" "$SEED_PARENT/richos-hq/ceo"
cp "$FRONTEND" "$SEED_SCRIPTS/"
cp "$ENGINE_ROOT/scripts/lib/"named-persons.{sh,py} \
   "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" "$SEED_SCRIPTS/lib/"
python3 - "$SEED_PARENT/richos-hq/ceo/entities.json" "$F_FULL" <<'PY'
import json, sys
with open(sys.argv[1], 'w') as stream:
    json.dump({'entities': [{'type': 'person', 'canonical': sys.argv[2]}]}, stream)
PY
git -C "$SEED_REPO" init -q
git -C "$SEED_REPO" add -A
git -C "$SEED_REPO" commit -qm "synthetic seed fixture"
OUT="$(bash "$SEED_SCRIPTS/named-persons.sh" --seed 2>&1)"; rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$OUT" | grep -Fq "# name: $F_FULL"; then
    ok "(s) grouped checkout discovers sibling HQ by default"
else
    bad "(s) grouped checkout could not seed from sibling HQ (exit $rc)"
fi
SEED_WORKTREE="$SCRATCH/linked-worktree"
git -C "$SEED_REPO" worktree add -q --detach "$SEED_WORKTREE"
OUT="$(bash "$SEED_WORKTREE/richos/engine/scripts/named-persons.sh" --seed 2>&1)"; rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$OUT" | grep -Fq "# name: $F_FULL"; then
    ok "(s) linked worktree discovers HQ beside its main checkout"
else
    bad "(s) linked worktree could not seed from main checkout's sibling HQ (exit $rc)"
fi
git -C "$SEED_REPO" worktree remove "$SEED_WORKTREE"
mv "$SEED_PARENT/richos-hq" "$SEED_PARENT/explicit-hq"
OUT="$(bash "$SEED_SCRIPTS/named-persons.sh" --seed --hq "$SEED_PARENT/explicit-hq" 2>&1)"; rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$OUT" | grep -Fq "# name: $F_FULL"; then
    ok "(s) explicit HQ override still works"
else
    bad "(s) explicit HQ override failed (exit $rc)"
fi
OUT="$(bash "$SEED_SCRIPTS/named-persons.sh" --seed 2>&1)"; rc=$?
if [ "$rc" -eq 2 ] && printf '%s' "$OUT" | grep -Fq 'Pass --hq'; then
    ok "(s) missing sibling HQ reports how to supply its location"
else
    bad "(s) missing sibling HQ did not give an actionable refusal (exit $rc)"
fi

rm -rf "$SB" "$SB_NO"

echo ""
TOTAL=$((PASS + FAIL))
if [ "$FAIL" -eq 0 ]; then
    printf '  %s/%s cases passed\n' "$PASS" "$TOTAL"
    exit 0
fi
printf '  %s/%s cases passed — %s FAILED\n' "$PASS" "$TOTAL" "$FAIL"
exit 1
