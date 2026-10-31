#!/usr/bin/env bash
#
# guard-row-currency-commits.sh — BLOCKING PreToolUse guard on the Bash tool.
#
# Refuses a LANDING whose record no longer describes the work it claims to.
#
# The predicate, the warrant grammar, the precision rules, the override
# decision and the honest list of what none of this can catch all live in
# scripts/lib/row-currency.{sh,py} — one place, because a predicate in two
# copies is the defect class this engine keeps finding in itself. READ THAT
# FILE FIRST; this one is the wiring.
#
# ===========================================================================
# WHY THE COMMIT AND THE MERGE, AND WHY NOTHING ELSE
# ===========================================================================
# The sibling guard-ceo-todos-commits.sh matches `git commit` only, and names
# `git merge` as an accepted gap on the grounds that the merged content was
# gated when it was committed on the source branch. That reasoning does not
# transfer here, and the difference is the whole point of this guard:
#
#   THE MERGE IS NOT A RE-COMMIT OF ALREADY-GATED CONTENT. IT IS THE MOMENT
#   A PROPOSAL BECOMES THE TRUTH THE RECORD IS DESCRIBING.
#
# All four rows that rotted in one day rotted at a merge. A guard on `git
# commit` alone would have watched every one of them go past. So `git merge`
# is matched, and the tree the merge is about to produce is computed exactly
# (`merge-tree --write-tree`) rather than guessed at from the branch tip.
#
# `git cherry-pick`, `git am`, `git rebase` and `git revert` also create
# commits without `git commit`. They are NOT matched, and that is a stated
# gap rather than an oversight: none of them is how work lands here, and a
# guard that tried to model a rebase's eventual tree would be modelling
# instead of measuring.
#
# ===========================================================================
# ONLY AT A LANDING
# ===========================================================================
# Main checkout, attached HEAD. An engineer's worktree branch is a proposal,
# it has changed nothing the record describes, and blocking it would block the
# wrong person on a file they do not own and cannot reach from there. That
# exit is SCOPE, not a skipped check, which is why it is silent — the same way
# this guard is silent in a repository that declares no contract at all.
#
# ===========================================================================
# NO LIVE OVERRIDE
# ===========================================================================
# Deliberately, and for the publication pair's reason applied with more force:
# the judgment that failed four times in one day was "I will update the row
# after the deploy", made by the lander, in the moment, under exactly the
# pressure an escape token is reached for. The way through is to fix the row,
# or to delete the declaration in a committed, reviewable diff.
#
# PRECISION: fires only on a command that really runs `git commit` or
# `git merge`, parsed as a command line rather than matched as a substring.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-row-currency-commits.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/guard-row-currency-commits.sh
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

# --- JURISDICTION ----------------------------------------------------------
# Deliberately BELOW the root-resolution bootstrap, never inside it: Layer R of
# contract-integrity-probe.sh extracts that block verbatim and asserts it is
# byte-identical across every rooted hook, so anything added inside it would
# read as divergence.
#
# The seat resolved above answers "am I governed?". It does NOT answer "does
# the artifact I was just handed belong to the repository I govern?" — and
# until 2026-08-30 nothing asked. See scripts/lib/seat-jurisdiction.sh.
_SJ_LIB="$SCRIPT_DIR/../lib/seat-jurisdiction.sh"
if [ ! -f "$_SJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-row-currency-commits.sh"
        echo "  scripts/lib/seat-jurisdiction.sh is missing at: $_SJ_LIB"
        echo "  Without it this guard cannot tell whether the artifact it was"
        echo "  handed belongs to the repository it governs, and a guard that"
        echo "  cannot tell must not answer."
    } >&2
    exit 2
fi
# shellcheck source=../lib/seat-jurisdiction.sh
. "$_SJ_LIB"

# --- GIT JURISDICTION ------------------------------------------------------
# The question UNDERNEATH the one above: which repository is this git command
# talking to? It was answered in five hand-copied blocks and every one of them
# missed `cd <repo> && git commit`. REFUSING TO START is deliberate — a guard
# that resolved the repository by guessing would be the 2026-09-01 bypass with
# a nicer error message.
_GJ_LIB="$SCRIPT_DIR/../lib/git-jurisdiction.sh"
if [ ! -f "$_GJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-row-currency-commits.sh"
        echo "  scripts/lib/git-jurisdiction.sh is missing at: $_GJ_LIB"
        echo "  Without it this guard cannot tell WHICH REPOSITORY the command"
        echo "  it was handed will actually commit to, and the fallback it used"
        echo "  to carry is the exact bypass that library exists to close."
    } >&2
    exit 2
fi
# shellcheck source=../lib/git-jurisdiction.sh
. "$_GJ_LIB"

INPUT="$(cat)"

if resolve_entity_root "$INPUT"; then
    # CAPTURED, not discarded. This used to be `:` — the seat decided whether
    # this guard ran and then had no say in WHAT it judged, which is the
    # divergence in its purest form.
    SEAT_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # DELIBERATELY NOT AN EXIT. This guard reads its contract out of the TARGET
    # repository, not out of the seat, so an unadopted seat is not a reason to
    # stop — the artifact's own repository still gets to govern itself below
    # by its own declaration. Exiting here is what made richos-hq's committed
    # .row-currency and .ceo-todos readable by nothing at all.
    SEAT_ROOT=""
else
    root_failure_banner "scripts/hooks/guard-row-currency-commits.sh" >&2
    exit 2
fi
# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this guard takes the SAME silent exit 0 that a
# well-formed payload for a DIFFERENT tool takes: the tool-name extraction ends
# in `|| true`, so "this call is not mine" and "I could not tell whose call this
# is" are one exit. That is why 17 of 25 PreToolUse guards were measured passing
# a call in complete silence on 2026-09-05. This separates the two. NO VERDICT
# CHANGES — the exit is the one already taken — only the silence does. The
# measurement, the channel and the argument: scripts/lib/unevaluated-notice.sh.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-row-currency-commits.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this commit leaves a row of the record describing work that has moved on"
fi

_RC_LIB="$SCRIPT_DIR/../lib/row-currency.sh"
if [ ! -f "$_RC_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-row-currency-commits.sh"
        echo "  scripts/lib/row-currency.sh is missing at: $_RC_LIB"
        echo "  This guard's entire predicate lives there. Without it it cannot"
        echo "  tell a current row from a stale one, and it will not guess."
    } >&2
    exit 2
fi
# shellcheck source=../lib/row-currency.sh
. "$_RC_LIB"

# --- What is this command, and where? --------------------------------------
# Parsed as a COMMAND LINE, not matched as a substring: `echo "git commit"` is
# not a commit, and a guard that thinks it is teaches people it cries wolf.
# Assigned via a quoted heredoc first for the bash 3.2 reason
# guard-worktree-removal.sh documents: a `)` inside a character class
# mis-scans as the close of a $( ) substitution on macOS's /bin/bash.
# dispatch-words: git
#   (dispatch-pretooluse.sh skips this rule on a call that names none of these
#   words: they are the words the filter below needs before it can do anything.)
read -r -d '' _RC_CLASSIFIER <<'PYEOF' || true
import json, os, re, shlex, sys

def out(*fields):
    sys.stdout.write("\t".join(str(f) for f in fields) + "\n")
    raise SystemExit

try:
    d = json.load(sys.stdin)
except Exception:
    out("PASS")
if not isinstance(d, dict) or d.get("tool_name") != "Bash":
    out("PASS")
ti = d.get("tool_input") or {}
cmd = (ti.get("command", "") if isinstance(ti, dict) else "") or ""
cwd = str(d.get("cwd", "") or "")

if not re.search(r"\bgit\b", cmd):
    out("PASS")

# --- heredocs, kept aside so `-F -` can still be read ----------------------
heredocs = {}
lines = cmd.split("\n")
i = 0
while i < len(lines):
    m = re.search(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1", lines[i])
    if m:
        tag = m.group(2)
        body, j = [], i + 1
        while j < len(lines) and lines[j].strip() != tag:
            body.append(lines[j])
            j += 1
        heredocs.setdefault(tag, "\n".join(body))
        i = j
    i += 1

# --- segments -------------------------------------------------------------
# SPLIT ON TOP-LEVEL SEPARATORS ONLY. This function is guard-vendoring-commits.sh's,
# adopted verbatim rather than reinvented, and its mutation harness pins the
# difference as `naive-segment-split` whose replacement text is the line this
# one replaced.
#
# WHAT THE REPLACED LINE DID, MEASURED 2026-09-05 RATHER THAN REASONED. It was
# `re.split(r"(?:\|\||&&|[;\n|])", cmd)`, which cuts inside quotes. A commit
# message written the way every commit message in this project is written —
#
#     git commit -m "the subject line
#
#     the body"
#
# — was cut at the blank line, both halves failed to shlex, and NO `git commit`
# was recognized in the call at all. The guard then exited 0 having looked at
# nothing, which is the same byte as a clean pass.
#
# The size of that hole, measured over every transcript on this machine since
# the guard shipped (docs/verification/row-currency-splitter-gap-2026-09-05/):
#
#     592  git commit/merge calls anchored at a GOVERNED MAIN CHECKOUT
#     403  recognized by the shipped splitter
#     189  NEVER RECOGNIZED — 31.9% of all landings went unchecked
#
# and the check it was skipping was not hypothetical: 29 commits reached
# richos-hq's main carrying a section-3 row whose own pin no longer matched the
# tree, three of them (`0015d1dd`, `3a5ab7de`, `c085a449`) produced by a call
# this splitter could not see.
#
# It also removed a FALSE POSITIVE nobody had noticed. The naive split shreds a
# heredoc body into lines, so a documentation line reading `git commit -m ...`
# inside a `cat > file <<'EOF'` payload shlexed into a commit that was never
# going to run: 33 such calls in the same corpus. Describing a command is not
# issuing one.
def top_level_segments(text):
    segs, cur, quote, esc = [], [], None, False
    i = 0
    while i < len(text):
        ch = text[i]
        if esc:
            cur.append(ch); esc = False; i += 1; continue
        if quote:
            if ch == "\\" and quote == '"':
                cur.append(ch); esc = True; i += 1; continue
            if ch == quote:
                quote = None
            cur.append(ch); i += 1; continue
        if ch == "\\":
            cur.append(ch); esc = True; i += 1; continue
        if ch in ("'", '"'):
            quote = ch; cur.append(ch); i += 1; continue
        if text[i:i + 2] in ("&&", "||"):
            segs.append("".join(cur)); cur = []; i += 2; continue
        if ch in ";\n|":
            segs.append("".join(cur)); cur = []; i += 1; continue
        cur.append(ch); i += 1
    segs.append("".join(cur))
    return segs


# --- AND A HEREDOC PAYLOAD IS A DOCUMENT, NOT A COMMAND -------------------
# The quote-aware walker above fixes the commits this guard could not SEE. This
# fixes the commits it saw that were never there.
#
# A heredoc body is data on stdin. `cat > runbook.md <<EOF` followed by a line
# reading `git commit -m "..."` is a document being WRITTEN, and the walker
# happily shlexes that line into a commit that is not going to run -- at which
# point this guard runs the row predicate and can REFUSE the write. Measured
# over every transcript since the guard shipped: 12 calls recognized only
# because of a heredoc body, 2 of them anchored at a governed main checkout
# where this guard acts. Small, and it is a blocking refusal of a document.
#
# THE RECALL IS KEPT BY THE ONE EXCEPTION THAT MATTERS: a heredoc fed to a
# SHELL is left alone, because `bash <<EOF` really does execute its body. Every
# shell-fed heredoc in the corpus is still read in full.
#
# The blanking is by SPACES, never deletion, so line structure and offsets
# survive -- and it is applied ONLY to the text the walker segments. The
# `heredocs` map above is built from the original, so `git commit -F -` with an
# inline message still reads its message.
_HEREDOC_START = re.compile(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")
_SHELL_CONSUMER = re.compile(
    r"(?:^|[|;&]|\s)(?:env\s+\S+\s+)?(?:\S*/)?(?:sh|bash|zsh|ksh|dash)\b")


def executable_text(src):
    chars = list(src)
    _lines = src.split("\n")
    offs, o = [], 0
    for ln in _lines:
        offs.append(o)
        o += len(ln) + 1
    i = 0
    while i < len(_lines):
        m = _HEREDOC_START.search(_lines[i])
        if m:
            tag = m.group(2)
            j = i + 1
            while j < len(_lines) and _lines[j].strip() != tag:
                j += 1
            a = offs[i + 1] if i + 1 < len(_lines) else len(src)
            b = offs[j] if j < len(_lines) else len(src)
            if b > a and not _SHELL_CONSUMER.search(_lines[i][:m.start()]):
                for k in range(a, b):
                    if chars[k] != "\n":
                        chars[k] = " "
            i = j
        i += 1
    return "".join(chars)


segments = top_level_segments(executable_text(cmd))

def parse(seg):
    try:
        argv = shlex.split(seg, comments=False)
    except ValueError:
        return None
    while argv and re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", argv[0]):
        argv.pop(0)
    if not argv:
        return None
    if os.path.basename(argv[0]) != "git":
        return None
    return argv

for seg in segments:
    argv = parse(seg)
    if not argv:
        continue
    repo = ""
    k = 1
    sub = ""
    while k < len(argv):
        a = argv[k]
        if a == "-C" and k + 1 < len(argv):
            repo = argv[k + 1]; k += 2; continue
        if a.startswith("--git-dir") or a.startswith("--work-tree"):
            k += 2 if "=" not in a else 1
            continue
        if a.startswith("-c") and a != "-c":
            k += 1; continue
        if a == "-c" and k + 1 < len(argv):
            k += 2; continue
        if a.startswith("-"):
            k += 1; continue
        sub = a
        k += 1
        break
    if sub not in ("commit", "merge"):
        continue

    rest = argv[k:]
    message = None
    msource = "unavailable"
    stage_all = 0
    merge_ref = ""
    skip = False
    n = 0
    parts = []
    while n < len(rest):
        a = rest[n]
        if a in ("--dry-run", "--abort", "--continue", "--quit", "--no-commit"):
            skip = True
            break
        if a in ("-m", "--message") and n + 1 < len(rest):
            parts.append(rest[n + 1]); msource = sub + " -m"; n += 2; continue
        if a.startswith("--message="):
            parts.append(a.split("=", 1)[1]); msource = sub + " -m"; n += 1; continue
        if a.startswith("-m") and len(a) > 2:
            parts.append(a[2:]); msource = sub + " -m"; n += 1; continue
        if a in ("-F", "--file") and n + 1 < len(rest):
            src = rest[n + 1]
            if src == "-":
                if len(heredocs) == 1:
                    parts.append(list(heredocs.values())[0])
                    msource = sub + " -F - (heredoc)"
                else:
                    msource = sub + " -F - (stdin, not readable here)"
            else:
                p = src if os.path.isabs(src) else os.path.join(cwd or ".", src)
                try:
                    with open(p, encoding="utf-8") as fh:
                        parts.append(fh.read())
                    msource = sub + " -F"
                except Exception:
                    msource = sub + " -F (file unreadable)"
            n += 2
            continue
        if a in ("-a", "--all"):
            stage_all = 1; n += 1; continue
        if a.startswith("-") and not a.startswith("--") and len(a) > 1 and "a" in a[1:] and sub == "commit":
            stage_all = 1; n += 1; continue
        if a == "--amend":
            if msource == "unavailable":
                msource = "commit --amend (message from HEAD or an editor)"
            n += 1; continue
        if a.startswith("-"):
            # A flag that takes a value we do not care about.
            if a in ("-s", "--strategy", "-X", "--strategy-option", "--into-name",
                     "-C", "--reuse-message", "-c", "--reedit-message",
                     "--author", "--date", "--cleanup", "--gpg-sign", "-S"):
                n += 2
            else:
                n += 1
            continue
        if sub == "merge" and not merge_ref:
            merge_ref = a
        n += 1
    if skip:
        continue
    if parts:
        message = "\n\n".join(parts)
    # `repo` is deliberately NOT emitted. WHERE the command points is resolved
    # by scripts/lib/git-jurisdiction.sh, which understands the `cd <repo> &&
    # git commit` form this walk cannot see. Emitting a second answer here would
    # be the divergent copy that put the same hole in five files.
    out("ACT", sub, stage_all, merge_ref, msource,
        json.dumps(message) if message is not None else "")

out("PASS")
PYEOF

# A valid escaped payload can exceed the OS environment-entry limit even
# when its decoded shell command fits. An execution failure is not a PASS.
CLASS="$(python3 -c "$_RC_CLASSIFIER" <<< "$INPUT" 2>/dev/null)" || {
    echo "ERROR: guard-row-currency-commits.sh: command classifier could not run; refusing." >&2
    exit 2
}
KIND="$(printf '%s' "$CLASS" | cut -f1)"
[ "$KIND" = "ACT" ] || exit 0

ACTION="$(printf '%s' "$CLASS" | cut -f2)"
STAGE_ALL="$(printf '%s' "$CLASS" | cut -f3)"
MERGE_REF="$(printf '%s' "$CLASS" | cut -f4)"
MSRC="$(printf '%s' "$CLASS" | cut -f5)"
MSG_JSON="$(printf '%s' "$CLASS" | cut -f6-)"

# --- WHICH REPOSITORY IS THIS COMMAND TALKING TO? --------------------------
# ONE resolver, shared by every guard that asks (scripts/lib/git-jurisdiction.sh),
# never a local copy — a copy is how the same hole ended up in five files.
_RC_GJ="$(richos_git_anchor "$INPUT" "commit merge")"
ANCHOR="$(printf '%s' "$_RC_GJ" | cut -f2)"
[ -n "$ANCHOR" ] || ANCHOR="$PWD"

rc_require_ceo_todos_lib || { rc_broken_banner "guard-row-currency-commits.sh" "$RC_BROKEN_REASON" >&2; exit 2; }

REPO="$(ct_repo_root "$ANCHOR" 2>/dev/null || true)"
[ -n "$REPO" ] || exit 0

# --- GOVERNANCE: the artifact's OWN repository decides ---------------------
# "Am I governed?" and "what am I inspecting?" are the same question here, and
# they are now asked of the SAME repository: the declaration loaded immediately
# below is read out of $REPO — which is also the thing being judged. Two
# questions about one repository cannot disagree; that is the whole fix, and it
# needs no extra comparison to hold.
#
# The seat is deliberately given NO VETO. It used to have one: an unadopted seat
# exited before this point, which is exactly why richos-hq's committed
# .row-currency and .ceo-todos were read by nothing at all while the repository
# took 28 commits in a day. The seat is REPORTED when it differs from the
# artifact's repository, and never obeyed — a guard that switched itself off on
# a seat mismatch would have let through a merge that was correctly refused.
if [ -n "${SEAT_ROOT}" ]; then
    richos_assert_jurisdiction "scripts/hooks/guard-row-currency-commits.sh" "${SEAT_ROOT}" "$REPO" "commit in" "proceeds" || true
fi

DECL_RC=0
rc_load_declaration "$REPO" || DECL_RC=$?
case "$DECL_RC" in
  0) ;;
  1) exit 0 ;;   # this repository declares no row-currency contract
  *) rc_broken_banner "guard-row-currency-commits.sh" "$RC_BROKEN_REASON" >&2; exit 2 ;;
esac

# SCOPE, not a skipped check. See the header.
rc_is_landing "$REPO" || exit 0

# --- THE OWNER LINE: ONE WRITER AT CUT-OVER --------------------------------
# Daily-driver plan step 8, two-installs spec point 27. When the record's own
# declaration carries ROW_RECORD_OWNER="app ...", the RichOS app owns this
# record and a landing here from his plain terminal is refused. Absent (the
# state until cut-over), this block does nothing at all. Only the RECORD form
# is judged: a landing in a peer repository is not a record commit.
#
# WHO IS CALLING is scripts/lib/record_owner.py's answer (the platform's own
# session record, by ancestry, pid plus kernel start time), never a guess from
# a name: the app's operator lead runs these same hooks and must pass. A caller
# that cannot be identified is refused, because the owner line has been set and
# "could not tell" must not read as "allowed".
#
# `git cherry-pick`, `am`, `rebase` and `revert` are not matched here, the gap
# the header states for the whole guard; and a commit typed at a shell outside
# Claude Code never reaches a PreToolUse hook at all (spec point 27).
if [ "$RC_MODE" = "record" ] && [ "${RC_RECORD_OWNER:-terminal}" = "app" ]; then
    _RC_TAB=$'\t'
    _RC_SIDE="$(python3 "$SCRIPT_DIR/../lib/record_owner.py" caller 2>/dev/null)" \
        || _RC_SIDE="unknown${_RC_TAB}the identity check could not run"
    _RC_WHO="${_RC_SIDE%%"$_RC_TAB"*}"
    _RC_WHY="${_RC_SIDE#*"$_RC_TAB"}"
    if [ "$_RC_WHO" != "app" ]; then
        {
            echo "=== THE RICHOS APP OWNS THIS RECORD — REFUSING THIS $(printf '%s' "$ACTION" | tr '[:lower:]' '[:upper:]') ==="
            echo "  repository : $REPO"
            echo "  owner line : ROW_RECORD_OWNER in ${RC_DECLARATION_FILE}"
            if [ "$_RC_WHO" = "terminal" ]; then
                echo "  caller     : this terminal ($_RC_WHY)"
            else
                echo "  caller     : could not be identified ($_RC_WHY)"
            fi
            echo ""
            echo "  Since the cut-over the app writes and lands his record, and the"
            echo "  terminal does not. Make the change in the app."
            echo "  To give the record back to the terminal, run record-owner.sh off"
            echo "  (or delete that one line); this landing then goes through."
            echo "(hook: scripts/hooks/guard-row-currency-commits.sh)"
        } >&2
        exit 2
    fi
fi

RES_RC=0
rc_resolve_record "$REPO" || RES_RC=$?
case "$RES_RC" in
  0) ;;
  1)
    # LOUD, and never a block. A public repository whose private record nobody
    # cloned must not have every commit in it refused.
    {
        echo "=== ROW CURRENCY: STOOD DOWN — THE RECORD IS NOT ON THIS MACHINE ==="
        echo "  repository : $REPO"
        echo "  reason     : $RC_STANDDOWN_REASON"
        echo "  Nothing is being checked. This is announced rather than silent so"
        echo "  that 'the guard is on' never means 'the guard looked'."
    } >&2
    exit 0 ;;
  *) rc_broken_banner "guard-row-currency-commits.sh" "$RC_BROKEN_REASON" >&2; exit 2 ;;
esac

WORK="$(mktemp -d -t row-currency.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

# --- The tree this operation is about to create ----------------------------
MODE="index"
[ "$STAGE_ALL" = "1" ] && MODE="all"
[ "$ACTION" = "merge" ] && MODE="merge"
SELF_TREE="$(rc_pending_tree "$REPO" "$MODE" "$MERGE_REF" 2>/dev/null || true)"
[ -n "$SELF_TREE" ] || SELF_TREE="-"

# --- The record, as it will be after this operation -------------------------
# When the record lives in THIS repository the pending tree is the truth; when
# it lives in a sibling, the sibling's worktree is. Comparing a pending tree
# here against a committed record there would be two different moments in time
# called consistent — the mistake guard-ceo-todos-commits.sh documents in its
# own stage_view().
RECORD="$WORK/record.md"
if [ "$RC_MODE" = "record" ] && [ "$SELF_TREE" != "-" ]; then
    git -C "$REPO" show "$SELF_TREE:$RC_RECORD_REL" > "$RECORD" 2>/dev/null || rm -f "$RECORD"
fi
if [ ! -s "$RECORD" ] && [ -f "$RC_RECORD_REPO/$RC_RECORD_REL" ]; then
    cp "$RC_RECORD_REPO/$RC_RECORD_REL" "$RECORD" 2>/dev/null || true
fi
if [ ! -s "$RECORD" ]; then
    {
        echo "=== ROW CURRENCY: THE DECLARED RECORD IS NOT ON DISK — REFUSING ==="
        echo "  repository : $RC_RECORD_REPO"
        echo "  declared   : $RC_RECORD_REL  (in $CEO_TODOS_DECLARATION)"
        echo ""
        echo "  A guard whose subject has vanished protects nothing while looking"
        echo "  switched on. Restore the record, or delete ${RC_DECLARATION_FILE:-$ROW_CURRENCY_DECLARATION}"
        echo "  to stand this mechanism down deliberately and visibly."
        echo "(hook: scripts/hooks/guard-row-currency-commits.sh)"
    } >&2
    exit 2
fi

B1="$WORK/base1.md"; B2="$WORK/base2.md"
git -C "$RC_RECORD_REPO" show "HEAD:$RC_RECORD_REL" > "$B1" 2>/dev/null || rm -f "$B1"
# ONE COMMIT OF LOOKBACK, and no more. A landing is a small, serialized
# sequence, and the row edit is legitimately allowed to be the commit
# immediately before the one that lands the work — which is how a lander who
# does the record half FIRST would otherwise be refused for doing it right.
git -C "$RC_RECORD_REPO" show "HEAD~1:$RC_RECORD_REL" > "$B2" 2>/dev/null || rm -f "$B2"
[ -f "$B1" ] || B1="-"
[ -f "$B2" ] || B2="-"

MSGF="-"
if [ -n "$MSG_JSON" ]; then
    MSGF="$WORK/message.txt"
    python3 -c 'import json,sys
sys.stdout.write(json.load(sys.stdin))' <<< "$MSG_JSON" > "$MSGF" 2>/dev/null || {
        echo "ERROR: guard-row-currency-commits.sh: commit message could not be decoded; refusing." >&2
        exit 2
    }
fi

JOB="$WORK/job.json"
rc_build_job "$JOB" "$RECORD" "$B1" "$B2" "$MSGF" "$MSRC" "$ACTION" "$SELF_TREE" || {
    echo "ERROR: guard-row-currency-commits.sh: could not assemble the check — refusing (fail-closed), because a checker that cannot run is not a current record." >&2
    exit 2
}

RESULT="$(rc_run "$JOB")" || {
    echo "ERROR: guard-row-currency-commits.sh: the row-currency predicate could not run — refusing (fail-closed)." >&2
    exit 2
}

# Consume the complete verdict stream: notes can exceed the pipe buffer.
# An early reader exit would turn an intentional pass/refusal into SIGPIPE.
VERDICT="$(printf '%s' "$RESULT" | sed -n '1p' | cut -f1)"
BODY="$(printf '%s\n' "$RESULT" | tail -n +2)"

case "$VERDICT" in
  CLEAN)
    # A clean pass still says what it did NOT check. The only way to silence
    # these lines is to fix what they name.
    # THE POSITIVE PROBE, on every single clean pass. `PC evaluated=0` and no PC
    # line at all are two very different facts — "the premise check ran and had
    # nothing to say" versus "the premise check did not run" — and a reader who
    # cannot tell them apart is the reader who trusted a green layer over a
    # scanner that never started.
    printf '%s\n' "$BODY" | awk -F'\t' '$1=="PC" {printf "  ROW CURRENCY — PREMISE CENSUS: %s %s %s %s %s %s %s %s %s %s\n", $2,$3,$4,$5,$6,$7,$8,$9,$10,$11}' >&2
    printf '%s\n' "$BODY" | awk -F'\t' '$1=="HC" {printf "  ROW CURRENCY — HEADLINE CENSUS: %s %s %s %s %s %s %s %s %s %s\n", $2,$3,$4,$5,$6,$7,$8,$9,$10,$11}' >&2
    printf '%s\n' "$BODY" | awk -F'\t' '$1=="NOTE" {printf "  ROW CURRENCY — NOTE: %s\n         %s\n", $2, $3}' >&2
    printf '%s\n' "$BODY" | awk -F'\t' '$1=="SKIP" {printf "  ROW CURRENCY — NOT CHECKED: item %s, %s\n         %s\n", $2, $3, $4}' >&2
    exit 0 ;;
  BROKEN)
    rc_broken_banner "guard-row-currency-commits.sh" "$(printf '%s' "$RESULT" | sed -n '1p' | cut -f2-)" >&2
    exit 2 ;;
  VIOLATIONS)
    N="$(printf '%s' "$RESULT" | sed -n '1p' | cut -f2)"
    rc_refusal "guard-row-currency-commits.sh" \
        "$N row(s) of this record no longer describe the work — REFUSING THIS $(printf '%s' "$ACTION" | tr '[:lower:]' '[:upper:]')" \
        "$BODY" "$RC_RECORD_REPO/$RC_RECORD_REL" >&2
    exit 2 ;;
  *)
    echo "ERROR: guard-row-currency-commits.sh: unexpected verdict from the predicate — refusing (fail-closed)" >&2
    exit 2 ;;
esac
