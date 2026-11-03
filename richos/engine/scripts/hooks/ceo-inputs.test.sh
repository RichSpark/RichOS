#!/usr/bin/env bash
#
# ceo-inputs.test.sh — THE INGRESS, END TO END, IN SANDBOX REPOSITORIES,
#                      DRIVEN THROUGH THE SHIPPED HOOKS AND THE SHIPPED GATES.
#
# ===========================================================================
# WHAT IS ASSERTED, AND WHY EACH SECTION EXISTS
# ===========================================================================
#   A. IT COMMITS. The specimen — a message handing over an untracked file —
#      ends with that file IN GIT, unmodified, in a commit of its own. This
#      section runs FIRST, because every silence assertion below is worthless
#      if the mechanism is simply broken and silent about everything.
#   B. IT DOES NOT TOUCH ANYBODY ELSE'S WORK. Staged work is not swept into
#      his commit, the working tree is not written, and `git status` does not
#      afterwards report his file as a staged deletion.
#   C. IT IS SILENT ON EVERYTHING ELSE. Tracked files, ignored files, bare
#      words, prose containing slashes. Each silence is paired with the
#      specimen firing in the same run, so a silence is provably a decision
#      rather than a hook that never ran.
#   D. THE GATES REFUSE, AND NOTHING IS COMMITTED WHEN THEY DO. A credential,
#      a path at the repository root, an unadopted repository, an unreadable
#      encoding, a merge in progress, a detached HEAD.
#   E. THE ABSENCE OF A FINDING IS DISTINGUISHABLE FROM THE ABSENCE OF A
#      CHECK. Stood down, no analyzer, an unreadable payload — each SAYS so,
#      and a clean run still writes its ledger line.
#   F. IT NEVER BLOCKS. Exit 0 on every path above. Measured against the
#      binary: exit 2 on this event ERASES the message he typed.
#   G. THE CONTENT NEVER LEAVES. A distinctive string inside every specimen
#      file is asserted absent from every byte this hook prints.
#   H. THE TURN-END PARTNER. A refusal is re-announced until the fact behind
#      it ends, and the recovery is announced when it does.
#
# Run directly:  scripts/hooks/ceo-inputs.test.sh [--verbose]
# Exit 0 = all green.

set -uo pipefail

VERBOSE=0
[ "${1:-}" = "--verbose" ] && VERBOSE=1

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$SRC_DIR/commit-ceo-inputs.sh"
STOP_HOOK="$SRC_DIR/notice-ceo-inputs-unheld.sh"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '         %s\n' "$2"; FAIL=$((FAIL + 1)); }
say() { [ "$VERBOSE" -eq 1 ] && printf '\n----- %s -----\n%s\n' "$1" "$2"; return 0; }

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }
command -v git     >/dev/null 2>&1 || { echo "FATAL: git required" >&2; exit 1; }

SANDBOX="$(cd "$(mktemp -d "${TMPDIR:?}/ceo-inputs.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

# A string that appears inside every specimen file and nowhere else. Section G
# asserts it never reaches stdout — the requirement is that this mechanism
# reports paths and states, never a byte of what he handed over.
CANARY="CANARY-CONTENT-MUST-NEVER-BE-PRINTED-7f3a91"

mkdir -p "$SANDBOX/no-git-hooks"

# ---------------------------------------------------------------------------
# new_repo <name> [--no-config]
# ---------------------------------------------------------------------------
new_repo() {
    local name="$1" repo="$SANDBOX/$1"
    mkdir -p "$repo/docs"
    if [ "${2:-}" != "--no-config" ]; then
        printf 'PROTECTED_PATHS="src"\n' > "$repo/orchestration.config"
    fi
    echo "# sandbox $name" > "$repo/README.md"
    printf 'ignored-dir/\n*.secretext\n' > "$repo/.gitignore"
    git -C "$repo" init -q -b main
    git -C "$repo" config user.email "tester@example.invalid"
    git -C "$repo" config user.name  "Ingress Tester"
    # A sandbox must not inherit the OPERATOR'S global git hooks. This machine
    # runs a core.hooksPath identity guard that refuses any commit not carrying
    # one particular address, and it refused every fixture commit in this suite
    # the first time it ran — a suite that fails because of the developer's
    # personal git configuration is a suite nobody can trust on a second
    # machine. (The ingress itself is unaffected: it commits through plumbing,
    # which runs no git hooks, and takes its identity from the repository's own
    # config, so a real commit still carries the operator's real identity.)
    git -C "$repo" config core.hooksPath "$SANDBOX/no-git-hooks"
    git -C "$repo" add -A >/dev/null 2>&1
    git -C "$repo" commit -q -m "base" >/dev/null 2>&1
    printf '%s' "$repo"
}

# payload <repo> <prompt-text>
payload() {
    PAY_REPO="$1" PAY_TEXT="$2" python3 -c '
import json, os
print(json.dumps({
    "session_id": "abcd1234-testsession",
    "transcript_path": "",
    "cwd": os.environ["PAY_REPO"],
    "hook_event_name": "UserPromptSubmit",
    "prompt": os.environ["PAY_TEXT"],
}))'
}

# run_hook <repo> <prompt-text>  -> stdout in OUT, exit code in RC
run_hook() {
    local repo="$1" text="$2"
    OUT="$(payload "$repo" "$text" | RICHOS_ENTITY_ROOT="$repo" bash "$HOOK" 2>/dev/null)"
    RC=$?
    say "hook over: $text" "$OUT"
}

head_count() { git -C "$1" rev-list --count HEAD 2>/dev/null || echo 0; }
is_tracked() { git -C "$1" ls-files --error-unmatch -- "$2" >/dev/null 2>&1; }

ALL_OUTPUT=""
record_output() { ALL_OUTPUT="$ALL_OUTPUT
$OUT"; }

echo "=== A. IT COMMITS ==="

REPO_A="$(new_repo repo-a)"
SPEC_A="$REPO_A/docs/handed-over-spec.md"
printf '# The specification\n\n%s\n\nBody text.\n' "$CANARY" > "$SPEC_A"
BEFORE_A="$(head_count "$REPO_A")"
BYTES_A="$(cksum < "$SPEC_A")"

run_hook "$REPO_A" "Handle this: $SPEC_A — it is the spec for the next round."
record_output

[ "$RC" = "0" ] && ok "A1  exit 0 (never blocks, never erases his message)" \
                || bad "A1  exit 0" "got $RC"

if is_tracked "$REPO_A" "$SPEC_A"; then
    ok "A2  the handed-over file is now TRACKED"
else
    bad "A2  the handed-over file is now TRACKED" "git does not hold $SPEC_A"
fi

AFTER_A="$(head_count "$REPO_A")"
[ "$AFTER_A" = "$((BEFORE_A + 1))" ] \
    && ok "A3  exactly one commit was created" \
    || bad "A3  exactly one commit was created" "$BEFORE_A -> $AFTER_A"

[ "$(cksum < "$SPEC_A")" = "$BYTES_A" ] \
    && ok "A4  HIS FILE IS BYTE-IDENTICAL — never modified" \
    || bad "A4  HIS FILE IS BYTE-IDENTICAL" "the file on disk changed"

MSG_A="$(git -C "$REPO_A" log -1 --format=%B 2>/dev/null)"
case "$MSG_A" in
    *"CEO input captured"*docs/handed-over-spec.md*)
        ok "A5  the commit message says it is his input and where it came from" ;;
    *)  bad "A5  the commit message says it is his input" "$MSG_A" ;;
esac

case "$OUT" in
    *COMMITTED*) ok "A6  the outcome is reported to the orchestrator" ;;
    *)           bad "A6  the outcome is reported" "$OUT" ;;
esac

case "$OUT" in
    *'"hookEventName":"UserPromptSubmit"'*)
        ok "A7  the context payload names the event (a mismatch is DROPPED by the host)" ;;
    *)  bad "A7  the context payload names the event" "$OUT" ;;
esac

# A tilde-rooted mention and a backticked relative mention are the two other
# spellings he actually uses.
SPEC_A2="$REPO_A/docs/second.md"
printf 'second %s\n' "$CANARY" > "$SPEC_A2"
run_hook "$REPO_A" "and this one too: \`docs/second.md\`"
record_output
is_tracked "$REPO_A" "$SPEC_A2" \
    && ok "A8  a BACKTICKED relative path is captured" \
    || bad "A8  a BACKTICKED relative path is captured" "$OUT"

# A9 IS A REGRESSION CASE WITH A DATE. The first field replay of the
# 2026-09-05 incident failed here while this suite was green, because the
# suite's own sandbox path had already been physicalized by `pwd -P` and the
# replay's had not: `git rev-parse --show-toplevel` answers physically, macOS
# routes /tmp and /var through symbolic links, and the resulting relative path
# climbed out of the repository and came back as a bare `update-index rc=128`.
# So the path is deliberately reached through a SYMLINK here.
LINK_DIR="$SANDBOX/link-to-repo-a"
ln -s "$REPO_A" "$LINK_DIR" 2>/dev/null || true
SPEC_A3="$REPO_A/docs/third.md"
printf 'third %s\n' "$CANARY" > "$SPEC_A3"
run_hook "$REPO_A" "handle $LINK_DIR/docs/third.md"
record_output
is_tracked "$REPO_A" "$SPEC_A3" \
    && ok "A9  a path reached through a SYMLINK is captured (one spelling, not two)" \
    || bad "A9  a path reached through a symlink is captured" "$OUT"

# A10 (hunt 2026-09-29 part 3, finding 8): when the REAL index cannot be updated
# after the commit, HEAD holds his file and the index does not, so the next
# ordinary commit would delete it. The ingress must say so instead of reporting
# a clean capture. Fault injection: a stale index.lock makes ONLY the real-index
# update fail; the ingress commits through a private index and is unaffected.
SPEC_A4="$REPO_A/docs/index-failure.md"
printf 'fourth %s\n' "$CANARY" > "$SPEC_A4"
: > "$REPO_A/.git/index.lock"
run_hook "$REPO_A" "handle $SPEC_A4"
rm -f "$REPO_A/.git/index.lock"
record_output
case "$OUT" in
    *"INDEX STALE"*"docs/index-failure.md"*|*"docs/index-failure.md"*"INDEX STALE"*)
        ok "A10 a failed real-index update is REPORTED, naming the file" ;;
    *)  bad "A10 a failed real-index update is reported" "$OUT" ;;
esac
STAGED_A4="$(git -C "$REPO_A" diff --cached --name-status 2>/dev/null)"
case "$OUT" in
    *"update-index --add --cacheinfo"*) ok "A11 ... and the report carries the repair command" ;;
    *)  bad "A11 the report carries the repair command" "$OUT (staged: $STAGED_A4)" ;;
esac

echo
echo "=== B. IT TOUCHES NOBODY ELSE'S WORK ==="

REPO_B="$(new_repo repo-b)"
# The orchestrator has work staged. It must not end up in his commit.
echo "orchestrator work in progress" > "$REPO_B/docs/engineer-file.md"
git -C "$REPO_B" add docs/engineer-file.md >/dev/null 2>&1
SPEC_B="$REPO_B/docs/his-doc.md"
printf 'his document %s\n' "$CANARY" > "$SPEC_B"

run_hook "$REPO_B" "handle $SPEC_B"
record_output

FILES_IN_COMMIT="$(git -C "$REPO_B" show --name-only --format= HEAD 2>/dev/null | grep -c . || true)"
[ "$FILES_IN_COMMIT" = "1" ] \
    && ok "B1  the commit contains EXACTLY ONE file — staged work was not swept in" \
    || bad "B1  the commit contains exactly one file" "$(git -C "$REPO_B" show --name-only --format= HEAD)"

git -C "$REPO_B" diff --cached --name-only 2>/dev/null | grep -q 'engineer-file' \
    && ok "B2  the orchestrator's staged work is STILL staged" \
    || bad "B2  the orchestrator's staged work is still staged" "$(git -C "$REPO_B" diff --cached --name-only)"

git -C "$REPO_B" status --porcelain -- docs/his-doc.md 2>/dev/null | grep -q '^D' \
    && bad "B3  his file is NOT reported as a staged deletion" "status shows a deletion" \
    || ok "B3  his file is NOT reported as a staged deletion (the real index was updated)"

echo
echo "=== C. IT IS SILENT ON EVERYTHING ELSE ==="

REPO_C="$(new_repo repo-c)"
TRACKED_C="$REPO_C/docs/already-held.md"
printf 'already held %s\n' "$CANARY" > "$TRACKED_C"
git -C "$REPO_C" add -A >/dev/null 2>&1
git -C "$REPO_C" commit -q -m "hold it" >/dev/null 2>&1

BASE_C="$(head_count "$REPO_C")"

run_hook "$REPO_C" "look at $TRACKED_C please"
record_output
[ "$(head_count "$REPO_C")" = "$BASE_C" ] \
    && ok "C1  a TRACKED file mentioned in passing produces no commit" \
    || bad "C1  a tracked file produces no commit" "$OUT"
case "$OUT" in *COMMITTED*) bad "C1b tracked file stays out of the report" "$OUT" ;;
               *)           ok "C1b tracked file stays out of the report" ;; esac

run_hook "$REPO_C" "Let's talk about the roadmap and what we ship first."
record_output
[ "$(head_count "$REPO_C")" = "$BASE_C" ] \
    && ok "C2  a message with NO paths produces no commit" \
    || bad "C2  a message with no paths produces no commit" "$OUT"

# Bare words that look like filenames. NONE of these may be guessed at.
BARE_C="$REPO_C/docs/bare.md"
printf 'bare %s\n' "$CANARY" > "$BARE_C"
run_hook "$REPO_C" "check spec and notes.md and bare.md against the plan"
record_output
[ "$(head_count "$REPO_C")" = "$BASE_C" ] \
    && ok "C3  BARE WORDS are never guessed at (spec, notes.md, bare.md)" \
    || bad "C3  bare words are never guessed at" "$OUT"

# The positive control for C3, in the same run: the SAME file, named the way he
# actually names one, IS captured. Without this, C3 passes for a broken hook.
run_hook "$REPO_C" "check \`$BARE_C\` against the plan"
record_output
is_tracked "$REPO_C" "$BARE_C" \
    && ok "C3b POSITIVE CONTROL — the same file, properly named, IS captured" \
    || bad "C3b positive control" "$OUT"

BASE_C="$(head_count "$REPO_C")"
run_hook "$REPO_C" "the ratio is 1 / 2, run s/foo/bar/ on it, and/or skip it — due 9/5"
record_output
[ "$(head_count "$REPO_C")" = "$BASE_C" ] \
    && ok "C4  slashes in ordinary prose are not paths" \
    || bad "C4  slashes in ordinary prose are not paths" "$OUT"

IGNORED_C="$REPO_C/ignored-dir/private.md"
mkdir -p "$REPO_C/ignored-dir"
printf 'private %s\n' "$CANARY" > "$IGNORED_C"
run_hook "$REPO_C" "and $IGNORED_C"
record_output
is_tracked "$REPO_C" "$IGNORED_C" \
    && bad "C5  a GITIGNORED file is left alone" "it was committed" \
    || ok "C5  a GITIGNORED file is left alone (an ignore is a decision)"

echo
echo "=== D. THE GATES REFUSE ==="

REPO_D="$(new_repo repo-d)"
SECRET_D="$REPO_D/docs/with-credential.md"
# A vendor-prefix shape scan-secrets.sh already recognizes, assembled from two
# fragments at run time so this suite is not itself a file containing a
# key-shaped literal. NOT the vendor's published example string: that one
# carries the word "example", which the scanner correctly treats as a
# placeholder — the first draft of this case used it and the case passed for
# the wrong reason, green over a gate that had decided the opposite.
SECRET_LITERAL="AKIA""3QZ7MTX9WL2BVK4D"
printf '# notes\n\n%s\n\naws_key = "%s"\n' "$CANARY" "$SECRET_LITERAL" > "$SECRET_D"
BASE_D="$(head_count "$REPO_D")"
run_hook "$REPO_D" "handle $SECRET_D"
record_output
if is_tracked "$REPO_D" "$SECRET_D"; then
    bad "D1  a CREDENTIAL is refused" "it was committed"
else
    ok "D1  a CREDENTIAL is refused — nothing committed"
fi
case "$OUT" in *REFUSED*) ok "D1b the refusal is stated with a reason" ;;
               *)         bad "D1b the refusal is stated" "$OUT" ;; esac
case "$OUT" in *"$SECRET_LITERAL"*) bad "D1c the refusal does not repeat the secret" "leaked" ;;
               *)                   ok "D1c the refusal does not repeat the secret" ;; esac

ROOT_D="$REPO_D/at-the-root.md"
printf 'root file %s\n' "$CANARY" > "$ROOT_D"
run_hook "$REPO_D" "handle $ROOT_D"
record_output
is_tracked "$REPO_D" "$ROOT_D" \
    && bad "D2  a path at the repository ROOT is refused" "it was committed" \
    || ok "D2  a path at the repository ROOT is refused"
case "$OUT" in *ROOT*) ok "D2b the refusal names the root and says where to put it" ;;
               *)      bad "D2b the refusal names the root" "$OUT" ;; esac

# A file in a repository that never adopted the engine, handed over from a
# session seated in one that did. THIS PAIR IS THE FIELD DEFECT, and it is here
# because the first draft failed it in the dangerous direction while looking
# safe: it tested the FILE's repository for orchestration.config, and neither
# `richos` nor `richos-hq` carries one at its root — so it would have refused
# every hand-over in the two repositories his specifications live in, with a
# safety-shaped reason. Both halves are asserted: the gates STILL RUN there,
# and a clean file there IS captured.
REPO_D2="$(new_repo repo-d2 --no-config)"
UNADOPTED_SECRET="$REPO_D2/docs/creds.md"
printf 'notes %s\n\napi_key = "%s"\n' "$CANARY" "$SECRET_LITERAL" > "$UNADOPTED_SECRET"
OUT="$(payload "$REPO_D" "handle $UNADOPTED_SECRET" | RICHOS_ENTITY_ROOT="$REPO_D" bash "$HOOK" 2>/dev/null)"
RC=$?
record_output
is_tracked "$REPO_D2" "$UNADOPTED_SECRET" \
    && bad "D3  an UNADOPTED repository is still SCANNED" "a credential was committed there" \
    || ok "D3  an UNADOPTED repository is still SCANNED — the credential is refused"

UNADOPTED_OK="$REPO_D2/docs/ordinary.md"
printf 'an ordinary document %s\n' "$CANARY" > "$UNADOPTED_OK"
OUT="$(payload "$REPO_D" "handle $UNADOPTED_OK" | RICHOS_ENTITY_ROOT="$REPO_D" bash "$HOOK" 2>/dev/null)"
RC=$?
record_output
is_tracked "$REPO_D2" "$UNADOPTED_OK" \
    && ok "D3b and a CLEAN file in that same repository IS captured (the field defect)" \
    || bad "D3b a clean file in an unadopted repository is captured" "$OUT"

BIN_D="$REPO_D/docs/binary.md"
printf '%s' "$CANARY" > "$BIN_D"
printf '\xff\xfe\x00\x01' >> "$BIN_D"
run_hook "$REPO_D" "handle $BIN_D"
record_output
is_tracked "$REPO_D" "$BIN_D" \
    && bad "D4  a file the gates cannot read is refused" "it was committed" \
    || ok "D4  a file the gates cannot READ is refused (never rounded to clean)"

REPO_D3="$(new_repo repo-d3)"
MERGING="$REPO_D3/docs/mid-merge.md"
printf 'mid merge %s\n' "$CANARY" > "$MERGING"
git -C "$REPO_D3" rev-parse HEAD > "$REPO_D3/.git/MERGE_HEAD"
run_hook "$REPO_D3" "handle $MERGING"
record_output
is_tracked "$REPO_D3" "$MERGING" \
    && bad "D5  a merge in progress is refused" "it was committed" \
    || ok "D5  a MERGE IN PROGRESS is refused"
rm -f "$REPO_D3/.git/MERGE_HEAD"

REPO_D4="$(new_repo repo-d4)"
DETACHED="$REPO_D4/docs/detached.md"
printf 'detached %s\n' "$CANARY" > "$DETACHED"
git -C "$REPO_D4" checkout -q --detach HEAD >/dev/null 2>&1
run_hook "$REPO_D4" "handle $DETACHED"
record_output
is_tracked "$REPO_D4" "$DETACHED" \
    && bad "D6  a DETACHED HEAD is refused" "it was committed" \
    || ok "D6  a DETACHED HEAD is refused"

echo
echo "=== E. ABSENCE OF A FINDING IS NOT ABSENCE OF A CHECK ==="

REPO_E="$(new_repo repo-e)"
run_hook "$REPO_E" "nothing to see here"
record_output
LEDGER_E="$REPO_E/.claude/state/ceo-inputs.jsonl"
[ -s "$LEDGER_E" ] \
    && ok "E1  a CLEAN run still writes its ledger line (the positive trace)" \
    || bad "E1  a clean run writes its ledger line" "no $LEDGER_E"
case "$OUT" in
    *systemMessage*\"\"*|"") ok "E1b a clean run says nothing to the operator" ;;
    *) case "$OUT" in *COMMITTED*|*REFUSED*) bad "E1b a clean run is quiet" "$OUT" ;;
                      *) ok "E1b a clean run says nothing to the operator" ;; esac ;;
esac

printf 'PROTECTED_PATHS="src"\nCHECK_CEO_INPUTS=0\n' > "$REPO_E/orchestration.config"
run_hook "$REPO_E" "handle something"
record_output
case "$OUT" in *"STOOD DOWN"*) ok "E2  STOOD DOWN says so, loudly, on both channels" ;;
               *)              bad "E2  stood down says so" "$OUT" ;; esac
[ "$RC" = "0" ] && ok "E2b stood down still exits 0" || bad "E2b stood down exits 0" "got $RC"
printf 'PROTECTED_PATHS="src"\n' > "$REPO_E/orchestration.config"

# A payload with no `prompt` field: the shape moved, or this is not the event.
# It must never read as "checked, nothing found".
OUT="$(printf '{"session_id":"abcd1234","cwd":"%s","hook_event_name":"UserPromptSubmit"}' "$REPO_E" \
       | RICHOS_ENTITY_ROOT="$REPO_E" bash "$HOOK" 2>/dev/null)"
RC=$?
record_output
case "$OUT" in *"DID NOT COMPLETE"*) ok "E3  an unreadable payload SAYS the check did not run" ;;
               *)                    bad "E3  an unreadable payload says so" "$OUT" ;; esac
[ "$RC" = "0" ] && ok "E3b and still exits 0" || bad "E3b exits 0" "got $RC"

# The analyzer removed: the wrapper must announce, not exit quietly.
COPY_DIR="$SANDBOX/copy/scripts/hooks"
mkdir -p "$COPY_DIR" "$SANDBOX/copy/scripts/lib"
cp "$HOOK" "$COPY_DIR/"
for l in resolve-roots.sh resolve-main-checkout.sh seat-jurisdiction.sh stop-hook-notice.sh; do
    cp "$SRC_DIR/../lib/$l" "$SANDBOX/copy/scripts/lib/$l" 2>/dev/null || true
done
OUT="$(payload "$REPO_E" "handle $REPO_E/docs/x.md" \
       | RICHOS_ENTITY_ROOT="$REPO_E" bash "$COPY_DIR/commit-ceo-inputs.sh" 2>/dev/null)"
RC=$?
record_output
case "$OUT" in *"NOT RUNNING"*) ok "E4  a MISSING ANALYZER says the check is not running" ;;
               *)               bad "E4  a missing analyzer says so" "$OUT" ;; esac

echo
echo "=== F. IT NEVER BLOCKS ==="
# Every run above captured RC; the destructive ones are re-asserted here as a
# single statement because exit 2 on this event ERASES the message he typed.
FBAD=0
for t in "handle $SECRET_D" "handle $ROOT_D" "handle $BIN_D" "" "a/b/c"; do
    run_hook "$REPO_D" "$t"
    [ "$RC" = "0" ] || FBAD=1
done
[ "$FBAD" = "0" ] \
    && ok "F1  exit 0 on every path including every refusal" \
    || bad "F1  exit 0 on every path" "a run exited non-zero"

echo
echo "=== G. THE CONTENT NEVER LEAVES ==="
case "$ALL_OUTPUT" in
    *"$CANARY"*) bad "G1  no byte of any handed-over file is ever printed" "the canary reached stdout" ;;
    *)           ok "G1  no byte of any handed-over file is ever printed" ;;
esac

echo
echo "=== H. THE TURN-END PARTNER ==="

REPO_H="$(new_repo repo-h)"
ROOT_H="$REPO_H/refused-at-root.md"
printf 'root %s\n' "$CANARY" > "$ROOT_H"
run_hook "$REPO_H" "handle $ROOT_H"
record_output

stop_payload() {
    printf '{"session_id":"abcd1234-testsession","transcript_path":"","cwd":"%s","hook_event_name":"Stop"}' "$1"
}
SOUT="$(stop_payload "$REPO_H" | RICHOS_ENTITY_ROOT="$REPO_H" bash "$STOP_HOOK" 2>/dev/null)"
SRC=$?
say "stop hook, unresolved" "$SOUT"
[ "$SRC" = "0" ] && ok "H0  the turn-end partner exits 0" || bad "H0  exits 0" "got $SRC"
case "$SOUT" in
    *"HELD BY NOTHING BUT YOUR DISK"*"refused-at-root.md"*)
        ok "H1  a REFUSAL is re-announced at turn end, naming the file" ;;
    *)  bad "H1  a refusal is re-announced at turn end" "$SOUT" ;;
esac

# Unchanged set, second turn: quiet. A persistent line under every turn is the
# line the eye learns to skip.
SOUT2="$(stop_payload "$REPO_H" | RICHOS_ENTITY_ROOT="$REPO_H" bash "$STOP_HOOK" 2>/dev/null)"
say "stop hook, repeat" "$SOUT2"
case "$SOUT2" in
    *"HELD BY NOTHING"*) bad "H2  an unchanged set does not repeat every turn" "$SOUT2" ;;
    *)                   ok "H2  an unchanged set does not repeat every turn" ;;
esac

# It clears itself the moment the FACT ends — no acknowledgement to remember.
git -C "$REPO_H" add refused-at-root.md >/dev/null 2>&1
git -C "$REPO_H" commit -q -m "operator put it in the record by hand" >/dev/null 2>&1
SOUT3="$(stop_payload "$REPO_H" | RICHOS_ENTITY_ROOT="$REPO_H" bash "$STOP_HOOK" 2>/dev/null)"
say "stop hook, resolved" "$SOUT3"
case "$SOUT3" in
    *"RUNNING AGAIN"*|*"nothing you handed over is left unheld"*)
        ok "H3  once the file is tracked the condition CLEARS and the recovery is said" ;;
    *)  bad "H3  the condition clears when the fact ends" "$SOUT3" ;;
esac
case "$SOUT3" in
    *"refused-at-root.md"*) bad "H3b the resolved path is no longer named" "$SOUT3" ;;
    *)                      ok "H3b the resolved path is no longer named" ;;
esac

printf 'PROTECTED_PATHS="src"\nCHECK_CEO_INPUTS_UNHELD=0\n' > "$REPO_H/orchestration.config"
SOUT4="$(stop_payload "$REPO_H" | RICHOS_ENTITY_ROOT="$REPO_H" bash "$STOP_HOOK" 2>/dev/null)"
say "stop hook, stood down" "$SOUT4"
case "$SOUT4" in *"STOOD DOWN"*) ok "H4  the partner announces its own stand-down" ;;
                 *)              bad "H4  the partner announces its stand-down" "$SOUT4" ;; esac

# --- P3-43: an unresolved input is still named after 500 unrelated records ---
# The reader kept only the last 500 ledger rows, so a refusal followed by 500
# unrelated hand-overs vanished while git still did not hold the file.
# Its fixture is its own (repo-p343): I1 below builds repo-i, and a shared
# name re-initializes a repository that already holds this test's commits.
REPO_P343="$(new_repo repo-p343)"
AGED_P343="$REPO_P343/aged-input.md"
printf 'aged input\n' > "$AGED_P343"
mkdir -p "$REPO_P343/.claude/state"
python3 - "$REPO_P343/.claude/state/ceo-inputs.jsonl" "$AGED_P343" <<'PYL'
import json, sys
ledger, aged = sys.argv[1], sys.argv[2]
with open(ledger, "w") as fh:
    fh.write(json.dumps({"refused": [{"path": aged, "why": "fixture refusal"}]}) + "\n")
    for i in range(500):
        fh.write(json.dumps({"committed": ["/invented/unrelated-%d" % i]}) + "\n")
PYL
SOUT_P343="$(stop_payload "$REPO_P343" | RICHOS_ENTITY_ROOT="$REPO_P343" bash "$STOP_HOOK" 2>/dev/null)"
case "$SOUT_P343" in
    *"HELD BY NOTHING BUT YOUR DISK"*"aged-input.md"*)
        ok "P3-43 a refused input is still named after 500 unrelated ledger records" ;;
    *)  bad "P3-43 a refused input is still named after 500 unrelated ledger records" "$SOUT_P343" ;;
esac

# --- P3-22: the commit holds the bytes the gates checked, not the live file ---
# Its fixture repository is removed however the fragment ends, and it is made
# in a temp directory of its own so the suite can SEE that nothing was left
# behind (hunt part 3 v3, finding 42: every run used to leave a repository).
P322_TMP="$(mktemp -d "${TMPDIR:?}/p322-tmp.XXXXXX")"
P322_OUT="$(TMPDIR="$P322_TMP" python3 - "$SRC_DIR/commit-ceo-inputs.py" <<'PYT' 2>&1
import importlib.util, os, shutil, subprocess, sys, tempfile
spec = importlib.util.spec_from_file_location("cci", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
d = tempfile.mkdtemp(prefix="p322.")
try:
    def g(*a):
        return subprocess.run(["git", "-C", d] + list(a), check=True, stdout=subprocess.PIPE,
                              env=dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="webdevbooster@gmail.com",
                                       GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="webdevbooster@gmail.com")).stdout
    g("init", "-q", "-b", "main")
    open(os.path.join(d, "seed.txt"), "w").write("seed\n")
    g("add", "seed.txt"); g("commit", "-q", "-m", "seed")
    f = os.path.join(d, "in.txt")
    open(f, "w").write("REPLACED AFTER THE SCAN\n")
    ok, res = m.commit(f, d, "s", "stamp", data=b"the checked bytes\n")
    got = g("show", "HEAD:in.txt").decode()
    print("OK" if ok and got == "the checked bytes\n" else "BAD ok=%s got=%r" % (ok, got))
finally:
    shutil.rmtree(d, ignore_errors=True)
PYT
)"
case "$P322_OUT" in
    *OK) ok "P3-22 the commit holds the bytes the gates checked, not the replaced live file" ;;
    *)   bad "P3-22 the commit holds the bytes the gates checked" "$P322_OUT" ;;
esac
P322_LEFT="$(ls -A "$P322_TMP" 2>/dev/null)"
rm -rf "$P322_TMP"
if [ -z "$P322_LEFT" ]; then ok "P3-42 the P3-22 fragment leaves no fixture repository behind"
else bad "P3-42 the P3-22 fragment leaves no fixture repository behind" "left: $P322_LEFT"; fi

# --- I. companion files: an input under wiki/raw/ is committed WITH its
# wiki/raw/assets/<id>-.../ folder (the 2026-10-04 transcript left that folder
# untracked for seven hours).
REPO_I="$(new_repo repo-i)"
mkdir -p "$REPO_I/wiki/raw/assets/D8PikZ1KhUo-default"
IN_I="$REPO_I/wiki/raw/2026-10-04-some-video-D8PikZ1KhUo.md"
echo "# transcript $CANARY" > "$IN_I"
echo "{}" > "$REPO_I/wiki/raw/assets/D8PikZ1KhUo-default/session.json"
echo "decoded" > "$REPO_I/wiki/raw/assets/D8PikZ1KhUo-default/original.md"
run_hook "$REPO_I" "handle $IN_I"
if is_tracked "$REPO_I" "wiki/raw/assets/D8PikZ1KhUo-default/session.json" \
   && is_tracked "$REPO_I" "wiki/raw/assets/D8PikZ1KhUo-default/original.md" \
   && is_tracked "$REPO_I" "wiki/raw/2026-10-04-some-video-D8PikZ1KhUo.md" \
   && [ -z "$(git -C "$REPO_I" status --porcelain -- wiki)" ] \
   && [ "$(head_count "$REPO_I")" = "2" ]; then
    ok "I1  the input and its untracked companion folder are committed together, one commit"
else
    bad "I1  input and companion folder committed together" "$(git -C "$REPO_I" status --porcelain -- wiki)"
fi

echo
echo "==========================================="
printf 'ceo-inputs.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
