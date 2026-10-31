#!/usr/bin/env bash
#
# guard-public-record-repo.sh — BLOCKING PreToolUse guard on TWO surfaces:
# Agent, and Bash (through dispatch-pretooluse.sh).
#
# THE RECORD DOES NOT GO IN THE PUBLIC REPOSITORY. A dispatch whose deliverable
# is a research read, a brief or a plan is REFUSED when its workspace resolves
# to a publication-bound repository; a commit into such a repository that ADDS
# a file under docs/research/, docs/briefs/ or docs/plans/ is REFUSED.
#
# ===========================================================================
# THE RULING, AND THE FAILURE IT WAS GIVEN FOR
# ===========================================================================
# CEO, 2026-09-20 00:10Z, verbatim:
#
#     "how many more times will the wrong shit be put into the Git history in
#      the public repo????"
#
# WHAT HAPPENED. On 2026-09-19 a research read of a third-party project was
# dispatched with `--repo richos` — the PUBLIC, publication-bound repository —
# and its 200-line adoption ledger was committed to `docs/research/` there. It
# was then moved to richos-hq, which does not remove it from history, so the
# repository's main had to be rewritten to drop the two commits. The read
# itself was correct and useful. Only its destination was wrong, and nothing
# between the dispatch and the push asked the question.
#
# THE EXISTING BOUNDARY IS ABOUT CONTENT, AND THIS IS ABOUT CLASS. Its sibling
# guards — guard-publication-writes.sh and guard-publication-commits.sh — scan
# what a file SAYS for private material: recordings, transcripts, a third
# party's speech. They were working, and they passed this file, because a
# tooling read of a public MIT repository contains no private speech at all.
# The defect is not that the ledger leaked something. It is that the record
# BELONGS in the private repository whatever it says, and no rule said so.
#
# So this guard asks a different question of the same declaration, and shares
# its data: `.publication-boundary` (or `.richos/publication-boundary`) is what
# makes a repository public, and PRIVATE_RECORD in it is the answer to "then
# where does this go?". Nothing new is declared and nothing is inferred.
#
# ===========================================================================
# WHAT IS IN SCOPE, AND THE ONE THING DELIBERATELY OUT OF IT
# ===========================================================================
# RECORD CLASSES: docs/research/, docs/briefs/, docs/plans/.
#
# docs/verification/ IS NOT IN SCOPE, and that is the CEO's own line rather
# than a judgment call: the ruling was "remove the two ledger commits and
# nothing else". Verification audits are the evidence a release was walked and
# they already live in the public tree by design. A guard that quietly widened
# a ruling would be the fourth thing in three days to make him ask how many
# more times.
#
# ===========================================================================
# HOW A DISPATCH'S DESTINATION IS DECIDED
# ===========================================================================
# From the prompt's own `cross-repo-worktree:` line(s), and from the payload's
# cwd for a native-isolation spawn. A workspace path that does not exist yet —
# spawn.sh evaluates every PreToolUse[Agent] guard BEFORE creating anything —
# is resolved by the `<repo>-wt/<name>` convention this machine uses, and a
# path that resolves to nothing at all is ANNOUNCED rather than guessed at.
#
# HOW A DELIVERABLE IS DECIDED: a RECORD-CLASS PATH, WRITTEN BARE, ON A LINE
# THAT ALSO CARRIES A DELIVERY VERB. All three conjuncts are load-bearing and
# every one of them is measured against the real prompt of 2026-09-19:
#
#   BARE    `richos-hq/docs/research/t3code-mobile-vs-richos-phone-2026-09-18.md`
#           appears in that same prompt as prior art. It is prefixed with the
#           private repository's name, so it is somebody else's file and not
#           this dispatch's destination. A path preceded by `/` or a word
#           character is never a deliverable here.
#   DELIVERY VERB
#           `wiki/ceo-decisions.md` is cited in that prompt too, as a source.
#           The line that matters reads "## What to deliver — a committed brief
#           in your worktree, `docs/research/...`", and it is the verb that
#           tells a destination from a citation.
#   RECORD CLASS
#           see above; docs/verification/ is out.
#
# ===========================================================================
# THE COMMIT SURFACE
# ===========================================================================
# `git commit` whose repository carries a publication declaration and whose
# ADDED set contains a record-class path. ADDED, never modified: an existing
# file is already in history and refusing a change to it would be theater.
# The added set comes from the index and from any `git add` in the same
# command; a command this cannot tokenize falls back to the index alone, which
# is narrower and never wider.
#
# ===========================================================================
# THE HATCH
# ===========================================================================
#     public-record-ack: <why this record belongs in the public repository>
#
# on its own line in the prompt, or after a `#` on the commit command line.
# At least 30 characters, a bare marker exempts nothing, and every accepted use
# is appended to <entity root>/.claude/state/public-record-acks.log. There is a
# real class it exists for — a README, a published research note the project
# means to ship — and the file's own name is what makes a habit of it visible.
#
# ===========================================================================
# FAIL OPEN, AND LOUDLY
# ===========================================================================
#   NOT ADOPTED / no publication declaration   -> STAND DOWN, silent. This is
#     every repository that has not declared itself public, which is most of
#     them, and it is why this guard costs nothing almost everywhere.
#   PUBLIC_RECORD_REPO_GUARD="off"             -> STAND DOWN, silent.
#   NO RECORD-CLASS DELIVERABLE / NOTHING ADDED -> SILENT.
#   A WORKSPACE PATH THAT RESOLVES TO NOTHING  -> ALLOW + ANNOUNCE, and only
#     when a record deliverable was actually seen. "I cannot tell which
#     repository" is never "forbidden", and it is never silent either.
#   PAYLOAD UNPARSEABLE, python3 MISSING       -> ALLOW + ANNOUNCE.
#
# The one thing that is NOT fail-open: scripts/lib/resolve-roots.sh missing.
#
# NOTE: hooks are snapshotted at session start. This one is INERT until the
# next session.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail
# --- ROOT RESOLUTION -------------------------------------------------------
# TWO ROOTS, NEVER ONE. The full contract, and why the old single-root
# resolution was wrong the moment the engine became loadable by reference,
# is in scripts/lib/resolve-roots.sh. This bootstrap block is byte-identical
# in every hook that needs a root; contract-integrity-probe.sh Layer R asserts
# that, so a divergent copy is a probe failure rather than a surprise.
#
# THE BANNER BELOW GOES OUT ON TWO CHANNELS AND ONLY THE SECOND IS HEARD.
# Measured on Claude Code 2.1.270 (macOS, 2026-09-14) by registering one probe
# hook per channel on five events at once and reading the transcript back:
#
#   channel, exit 0     SessionStart  UserPromptSubmit  PreToolUse  PostToolUse  Stop
#   stderr              silent        silent            silent      silent       silent
#   stdout, plain text  model         model             silent      silent       silent
#   {"systemMessage"}   PERSON        PERSON            PERSON      PERSON       PERSON
#   additionalContext   model         model             model       model        model
#
# `silent` is not shorthand: the host records a `hook_success` attachment
# carrying the text in its stderr field and renders it to NO ONE. So a hook
# that could not find its own engine announced a dead enforcement layer
# exactly as loudly as a clean pass. Of the 60 files carrying this block, 35
# exit 2 here — where the host does render stderr, as the refusal reason — and
# 24 exit 0 and were inaudible. The 24 are the notices and observers, which is
# the trap: the hooks that must never block are the hooks nobody could hear.
#
# WHY `systemMessage` AND NOT `additionalContext`. additionalContext must name
# its own event in the envelope, and this block is identical in hooks
# registered on eight different events — it cannot know which one it is on.
# `systemMessage` is event-agnostic and it reaches the operator rather than
# only the model, which is the right audience for "your guards are off".
# Measured too: adding it to an exit-2 hook leaves the refusal untouched —
# same `hook error:` tool result, same blocked write — and only adds a render.
# Nothing here changes what any hook detects, refuses, or exits with.
#
# The escaping is deliberately pure bash (verified on 3.2.57, the macOS system
# shell) and calls nothing external: this is the one code path in the engine
# that runs when the install is already known to be broken, so it must not
# depend on python3, jq, or any file it has just failed to find.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_RR_LIB="$SCRIPT_DIR/../lib/resolve-roots.sh"
if [ ! -f "$_RR_LIB" ]; then
    _RR_MSG="=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ===
  hook: scripts/hooks/guard-public-record-repo.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 2
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

INPUT="$(cat)"

announce_off() {
    printf '%s\n' "$1" >&2
    if command -v python3 >/dev/null 2>&1; then
        SYSMSG="$1" python3 -c '
import json, os
print(json.dumps({"systemMessage": os.environ.get("SYSMSG", "")}))
' 2>/dev/null || true
    fi
}

if ! command -v python3 >/dev/null 2>&1; then
    announce_off "PUBLIC-RECORD-REPO GUARD IS OFF: python3 is not on PATH, so this call was NOT checked for a record going into a publication-bound repository."
    exit 0
fi

if ! resolve_entity_root "$INPUT"; then
    if [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
        exit 0
    fi
    announce_off "PUBLIC-RECORD-REPO GUARD IS OFF: it cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed})."
    exit 0
fi
ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
_SJ_LIB="$SCRIPT_DIR/../lib/seat-jurisdiction.sh"
if [ ! -f "$_SJ_LIB" ]; then
    announce_off "BROKEN INSTALL: seat-jurisdiction.sh is missing; the target repository cannot be checked."
    exit 0
fi
. "$_SJ_LIB"

_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-public-record-repo.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this call puts a research read, a brief or a plan into a publication-bound repository"
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"

: "${PUBLIC_RECORD_REPO_GUARD:=on}"
case "$PUBLIC_RECORD_REPO_GUARD" in
    off|OFF|0|no|NO) exit 0 ;;
esac

# --- THE PUBLICATION DECLARATION, SHARED WITH ITS SIBLING GUARDS -----------
_PB_LIB="$SCRIPT_DIR/../lib/publication-boundary.sh"
if [ ! -f "$_PB_LIB" ]; then
    announce_off "PUBLIC-RECORD-REPO GUARD IS OFF: scripts/lib/publication-boundary.sh is missing, so this guard cannot tell a public repository from a private one."
    exit 0
fi
# shellcheck source=../lib/publication-boundary.sh
. "$_PB_LIB"

# is_public <repo-root> — rc 0 and PB_PRIVATE_RECORD set when the repository
# declares a publication boundary. A BROKEN declaration is left to its owners:
# guard-publication-commits.sh blocks on it with a full banner, and two hooks
# shouting the same broken-config message at one commit is noise.
is_public() {
    local root="${1:-}" rc=0
    [ -n "$root" ] || return 1
    pb_load_declaration "$root" >/dev/null 2>&1 || rc=$?
    [ "$rc" -eq 0 ] || return 1
    return 0
}

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"

LOG_DIR="$ENTITY_ROOT/.claude/state"
ACK_LOG="$LOG_DIR/public-record-acks.log"
SESSION_ID="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("session_id","") or "")' 2>/dev/null || true)"

# log_ack <what> <reason>
log_ack() {
    mkdir -p "$LOG_DIR" 2>/dev/null || true
    printf '%s\tsession=%s\ttool=%s\t%s\t%s\n' \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "${SESSION_ID:-<unset>}" \
        "${TOOL_NAME:-<unset>}" "$1" "$2" >>"$ACK_LOG" 2>/dev/null || true
}

# read_ack <text> — sets ACK_REASON (an accepted reason) and ACK_SHORT (a
# marker that was there and was too short). rc 0 when a reason was accepted.
#
# IT SETS VARIABLES RATHER THAN PRINTING, deliberately: a `$(...)` capture runs
# in a subshell, so a too-short marker would come back indistinguishable from
# no marker at all and the operator would be told nothing about why their line
# did not work. The same reasoning resolve-roots.sh gives for its own API.
ACK_REASON=""
ACK_SHORT=""
read_ack() {
    local txt="$1" arg
    ACK_REASON=""
    ACK_SHORT=""
    arg="$(printf '%s' "$txt" | sed -nE 's/^[[:space:]]*#?[[:space:]]*public-record-ack:[[:space:]]*(.+)$/\1/p' | sed -n 1p)"
    if [ -z "$arg" ]; then
        arg="$(printf '%s' "$txt" | sed -nE 's/.*#[[:space:]]*public-record-ack:[[:space:]]*(.+)$/\1/p' | sed -n 1p)"
    fi
    [ -n "$arg" ] || return 1
    if [ "$(printf '%s' "$arg" | wc -c | tr -d ' ')" -lt 31 ]; then
        ACK_SHORT="$arg"
        return 1
    fi
    ACK_REASON="$arg"
    return 0
}

ack_too_short_banner() {
    {
        echo "=== THE public-record-ack: WAS NOT ACCEPTED ==="
        echo "  you gave : public-record-ack: ${ACK_SHORT}"
        echo "  problem  : under 30 characters is not a reason. A bare marker, and a"
        echo "             one-word marker, exempt nothing — say why this record belongs"
        echo "             in the PUBLIC repository's history forever."
        echo "(hook: scripts/hooks/guard-public-record-repo.sh)"
    } >&2
    exit 2
}

# --- THE CLASSIFIERS, DEFINED BY HEREDOC AND NEVER INLINE ------------------
# A python block written directly inside a $( ) substitution is scanned by
# bash BEFORE python ever sees it, and on macOS's /bin/bash 3.2.57 that
# scanner gets two things wrong that this guard needs: an unbalanced `)`
# inside a character class reads as the end of the substitution, and a LONE
# BACKTICK — in a regex, in a string, or in a COMMENT — opens a command
# substitution that swallows the rest of the file. Both were live defects
# here: the second reported a syntax error 200 lines below its cause.
# guard-worktree-removal.sh and guard-publication-commits.sh record the first.
# A quoted heredoc is scanned by nothing, so the text reaches python as typed.
read -r -d '' _PR_WORKSPACES <<'PYEOF' || true
import json, re, sys
try:
    d = json.load(sys.stdin)
    ti = d.get("tool_input") or {}
    blob = "\n".join(str(ti.get(k, "") or "") for k in ("prompt", "description"))
except Exception:
    sys.exit(0)
for m in re.finditer(r"^[ \t]*cross-repo-worktree:[ \t]*(\S+)[ \t]*$", blob, re.MULTILINE):
    print(m.group(1))
PYEOF

# Prints one line per site:  <class>|<path>|<excerpt>
read -r -d '' _PR_DELIVERABLES <<'PYEOF' || true
import json, re, sys
try:
    d = json.load(sys.stdin)
    ti = d.get("tool_input") or {}
except Exception:
    sys.exit(0)
blob = "\n".join(str(ti.get(k, "") or "") for k in ("prompt", "description"))

# A path preceded by "/" or a word character belongs to somebody else: it is
# richos-hq/docs/research/..., the prior art this dispatch is told to read.
# Trailing punctuation is stripped afterwards rather than excluded in a
# character class, because a markdown path is usually wrapped in code quotes.
PATH_RE = re.compile(r"(?<![\w/.-])docs/(research|briefs|plans)/(\S+)")
TRAILING = "\x60\x29\x5d,.;:\"'*"
# NARROW, AND EVERY OMISSION IS MEASURED. A first draft carried land, lands,
# output, save and saves, and the Reed prompt's PRIOR-ART line matched on
# "lands" — it reads "8 of 19 lands on richos main were our own tooling" and
# names a bare docs/research/ path a sentence earlier. A verb here must be a
# verb of PRODUCING THIS DISPATCH'S OWN ARTIFACT, never a verb that happens to
# appear beside a citation.
VERB_RE = re.compile(
    r"\b(?:deliver|delivers|delivered|deliverable|write|writes|written|commit|"
    r"commits|committed|create|creates|produce|produces|file it|put it)\b",
    re.IGNORECASE)
out = []
for raw in blob.split("\n"):
    m = PATH_RE.search(raw)
    if not m:
        continue
    if not VERB_RE.search(raw):
        continue
    ex = raw.strip()
    if len(ex) > 150:
        ex = ex[:150] + "..."
    tail = m.group(2).rstrip(TRAILING)
    out.append("%s|docs/%s/%s|%s" % (m.group(1), m.group(1), tail, ex.replace("|", "/")))
    if len(out) >= 5:
        break
for line in out:
    print(line)
PYEOF

read -r -d '' _PR_PENDING_ADDS <<'PYEOF' || true
import re, sys
cmd = sys.stdin.read()
out = []
for seg in re.split(r"[;&|]+", cmd):
    if not re.search(r"\bgit\b[^\n]*\badd\b", seg):
        continue
    for tok in seg.split():
        if tok.startswith("-") or tok in ("git", "add"):
            continue
        out.append(tok.strip("\x22\x27"))
print("\n".join(out))
PYEOF

read -r -d '' _PR_COMMAND <<'PYEOF' || true
import json, sys
try:
    d = json.load(sys.stdin)
    ti = d.get("tool_input") or {}
    print(str(ti.get("command", "") or ""))
except Exception:
    pass
PYEOF

read -r -d '' _PR_BLOB <<'PYEOF' || true
import json, sys
try:
    d = json.load(sys.stdin)
    ti = d.get("tool_input") or {}
    print("\n".join(str(ti.get(k, "") or "") for k in ("prompt", "description")))
except Exception:
    pass
PYEOF

case "$TOOL_NAME" in
Agent)
    # --- WHERE IS THIS TEAMMATE GOING TO WORK? -----------------------------
    WS_PATHS="$(printf '%s' "$INPUT" | python3 -c "$_PR_WORKSPACES" 2>/dev/null || true)"

    # --- IS A RECORD THE DELIVERABLE? --------------------------------------
    DELIVERABLES="$(printf '%s' "$INPUT" | python3 -c "$_PR_DELIVERABLES" 2>/dev/null || true)"

    [ -n "$DELIVERABLES" ] || exit 0

    # --- RESOLVE EACH WORKSPACE TO A REPOSITORY ----------------------------
    PUBLIC_REPO=""
    UNRESOLVED=""
    for ws in $WS_PATHS; do
        repo="$(pb_repo_root "$ws" 2>/dev/null || true)"
        if [ -z "$repo" ]; then
            # The workspace does not exist yet — spawn.sh evaluates the guards
            # before it creates anything. `<repo>-wt/<name>` is this machine's
            # convention and is the only inference made here; anything else is
            # announced rather than assumed.
            parent="$(dirname "$ws")"
            case "$(basename "$parent")" in
                *-wt)
                    cand="$(dirname "$parent")/$(basename "$parent" | sed 's/-wt$//')"
                    repo="$(pb_repo_root "$cand" 2>/dev/null || true)"
                    ;;
            esac
        fi
        if [ -z "$repo" ]; then
            UNRESOLVED="$UNRESOLVED $ws"
            continue
        fi
        richos_assert_jurisdiction "guard-public-record-repo.sh" "$ENTITY_ROOT" "$repo" "repository" "proceeds" || true
        if is_public "$repo"; then
            PUBLIC_REPO="$repo"
            break
        fi
    done

    # A native-isolation spawn carries no cross-repo line; its workspace is in
    # the session's own repository.
    if [ -z "$PUBLIC_REPO" ] && [ -z "$WS_PATHS" ]; then
        if is_public "$ENTITY_ROOT"; then
            PUBLIC_REPO="$ENTITY_ROOT"
        fi
    fi

    if [ -z "$PUBLIC_REPO" ]; then
        if [ -n "$UNRESOLVED" ]; then
            announce_off "PUBLIC-RECORD-REPO GUARD: this dispatch names a record deliverable ($(printf '%s' "$DELIVERABLES" | head -1 | cut -d'|' -f2)) and its workspace path(s)${UNRESOLVED} resolve to no repository, so it was NOT checked. If that workspace is in a public repository, the record belongs in the private one."
        fi
        exit 0
    fi

    # The blob is extracted into a VARIABLE and the hatch is read from it in
    # this shell. Piping it into ack_or_empty would set ACK_SHORT in a
    # subshell, so a too-short marker would come back indistinguishable from no
    # marker at all — and the operator would be told nothing about why their
    # line did not work.
    PROMPT_BLOB="$(printf '%s' "$INPUT" | python3 -c "$_PR_BLOB" 2>/dev/null || true)"
    if read_ack "$PROMPT_BLOB"; then
        log_ack "repo=$PUBLIC_REPO" "$ACK_REASON"
        exit 0
    fi
    [ -z "$ACK_SHORT" ] || ack_too_short_banner

    {
        echo "=== A RECORD IS BEING DISPATCHED INTO THE PUBLIC REPOSITORY ==="
        echo "  workspace repository : $PUBLIC_REPO"
        echo "  it declares a publication boundary, so its history is PUBLISHED."
        echo ""
        echo "  THE DELIVERABLE(S) THIS PROMPT NAMES:"
        printf '%s\n' "$DELIVERABLES" | while IFS='|' read -r _cls _path _ex; do
            printf '    %s\n        %s\n' "$_path" "$_ex"
        done
        echo ""
        echo "  WHERE IT GOES INSTEAD: ${PB_PRIVATE_RECORD:-the private record repository}"
        echo ""
        echo "  THE CEO, 2026-09-20:"
        echo "      \"how many more times will the wrong shit be put into the Git"
        echo "       history in the public repo????\""
        echo ""
        echo "  WHAT HAPPENED: on 2026-09-19 a research read was dispatched with"
        echo "  --repo pointed at this repository and its ledger was committed to"
        echo "  docs/research/ here. Moving a file does not remove it from history;"
        echo "  main had to be rewritten to drop two commits."
        echo ""
        echo "  WHY THE CONTENT GUARDS DID NOT CATCH IT: they scan what a file SAYS"
        echo "  for private material, and that read contained none. This is about"
        echo "  CLASS, not content — research, briefs and plans belong to the private"
        echo "  record whatever they say. docs/verification/ is deliberately NOT in"
        echo "  scope; verification audits belong in the public tree."
        echo ""
        echo "  THE USUAL ANSWER: give the teammate a workspace in the private"
        echo "  repository and name the deliverable there."
        echo ""
        echo "  OR SAY WHY THIS ONE IS PUBLISHED, in 30+ characters on its own line:"
        echo ""
        echo "      public-record-ack: <why this record belongs in public history>"
        echo ""
        echo "  Accepted uses are appended to .claude/state/public-record-acks.log."
        echo "(hook: scripts/hooks/guard-public-record-repo.sh)"
    } >&2
    exit 2
    ;;

Bash)
    _GJ_LIB="$SCRIPT_DIR/../lib/git-jurisdiction.sh"
    [ -f "$_GJ_LIB" ] || exit 0
    # shellcheck source=../lib/git-jurisdiction.sh
    . "$_GJ_LIB"

    COMMAND="$(printf '%s' "$INPUT" | python3 -c "$_PR_COMMAND" 2>/dev/null || true)"
    printf '%s' "$COMMAND" | grep -qE '\bgit\b[^\n;|&]*\bcommit\b' || exit 0

    _GJ="$(richos_git_anchor "$INPUT" "commit" 2>/dev/null || true)"
    ANCHOR="$(printf '%s' "$_GJ" | cut -f2)"
    [ -n "$ANCHOR" ] || ANCHOR="$PWD"
    REPO="$(pb_repo_root "$ANCHOR" 2>/dev/null || true)"
    [ -n "$REPO" ] || exit 0
    richos_assert_jurisdiction "guard-public-record-repo.sh" "$ENTITY_ROOT" "$REPO" "repository" "proceeds" || true
    is_public "$REPO" || exit 0

    # --- WHAT THIS COMMIT ADDS --------------------------------------------
    # The index, plus any path this same command stages first. ADDED only: an
    # existing file is already in history.
    ADDED="$(git -C "$REPO" diff --cached --name-only --diff-filter=A 2>/dev/null || true)"
    PENDING="$(printf '%s' "$COMMAND" | python3 -c "$_PR_PENDING_ADDS" 2>/dev/null || true)"

    HITS="$(printf '%s\n%s\n' "$ADDED" "$PENDING" \
        | sed 's|^\./||' \
        | grep -E '(^|/)docs/(research|briefs|plans)/' 2>/dev/null || true)"
    [ -n "$HITS" ] || exit 0

    if read_ack "$COMMAND"; then
        log_ack "repo=$REPO" "$ACK_REASON"
        exit 0
    fi
    [ -z "$ACK_SHORT" ] || ack_too_short_banner

    {
        echo "=== THIS COMMIT ADDS A RECORD TO THE PUBLIC REPOSITORY'S HISTORY ==="
        echo "  repository : $REPO"
        echo "  it declares a publication boundary, so its history is PUBLISHED."
        echo ""
        echo "  THE FILE(S) BEING ADDED:"
        printf '%s\n' "$HITS" | sed 's/^/    /'
        echo ""
        echo "  WHERE THEY GO INSTEAD: ${PB_PRIVATE_RECORD:-the private record repository}"
        echo ""
        echo "  THE CEO, 2026-09-20:"
        echo "      \"how many more times will the wrong shit be put into the Git"
        echo "       history in the public repo????\""
        echo ""
        echo "  A MOVE DOES NOT UNDO THIS. Committing a research note here and moving"
        echo "  it tomorrow leaves it in history forever; on 2026-09-19 that cost a"
        echo "  rewrite of main. Only a commit that never happens is free."
        echo ""
        echo "  docs/verification/ is deliberately NOT in scope — verification audits"
        echo "  belong in the public tree, and this refuses research, briefs and plans."
        echo ""
        echo "  THE USUAL ANSWER: commit it in the private record repository."
        echo ""
        echo "  OR SAY WHY THIS ONE IS PUBLISHED, in 30+ characters, on the command:"
        echo ""
        echo "      git commit ...   # public-record-ack: <why it belongs in public history>"
        echo ""
        echo "  Accepted uses are appended to .claude/state/public-record-acks.log."
        echo "(hook: scripts/hooks/guard-public-record-repo.sh)"
    } >&2
    exit 2
    ;;

*)
    exit 0
    ;;
esac
