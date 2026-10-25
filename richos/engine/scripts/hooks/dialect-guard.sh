#!/usr/bin/env bash
#
# dialect-guard.sh — PreToolUse hook (Write|Edit|MultiEdit|NotebookEdit) that
# FIXES American spelling where a fix is safe and REFUSES the rest.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# On 2026-08-29 the CEO ruled, verbatim: *"American English must be the language
# for UI as well as things like 'CEO queue'."* On 2026-08-30 a sweep fixed 654
# sites across 205 files. WITHIN HOURS, on 2026-08-31, ~20 fresh violations were
# written into the same record — including into the decision page that carries
# the ruling. The sweep CLEANED what existed and CONSTRAINED NOTHING THAT CAME
# AFTER, so the rule got a write-time chokepoint: this file.
#
# On 2026-09-27 he set the bar for it: *"The goal is to have a guard that is as
# fast as possible and eliminates the need for slow inference as much as
# possible i.e. automates as much as possible on this matter."* (§93). A refusal
# costs a model turn to rewrite one word. So since then the hook FIXES what can
# be fixed mechanically, through `hookSpecificOutput.updatedInput`, and refuses
# only what cannot (plan: richos-hq docs/plans/
# 2026-09-27-american-spelling-autofix-plan-check.md, section 1(b)).
# Named subject first, then role, by his instruction (2026-10-24); until then it
# was guard-dialect.sh.
#
# ===========================================================================
# ONE HOOK THAT FIXES AND REFUSES, NEVER A FIXER BESIDE A QUIET GUARD
# ===========================================================================
# A separate fixer plus a guard that stops refusing the fixed words lets British
# text through in silence whenever the fixer fails to start. So one process
# decides both, with the fail-closed start below, and every match the old guard
# refused is now either FIXED or still REFUSED; none is dropped. The decisions
# live in scripts/lib/dialect/dialect-fix.py (shared with the app's lint rule,
# richos/app/scripts/lint/dialect.py) and read one table,
# scripts/lib/dialect/dialect-en-US.generated.dict (3,802 pairs from VarCon's
# verified clusters plus hand-kept additions; it replaced the 173-word
# dialect-en-US.dict this hook used to read, every line of which it carries).
#
#   FIXED    prose files (md, markdown, mdx, txt, rst) outside code, quotations
#            and declared exemptions; in code files (rs, swift, kt, js, ts, py,
#            sh and kin) comments and string literals that contain whitespace.
#            The word is lowercase, all capitals, or capitalized at the start of
#            a sentence. The model is told every change (`colour → color ×3
#            (lines 4, 9, 12)`) through additionalContext, so a later Edit quotes
#            the text that is really on disk.
#   REFUSED  (exit 2, as before) markup, styles, config and every other file
#            class; in code, the code itself and single-token string literals
#            (`"colour"` may be a third-party wire value); a quotation; a capital
#            mid-sentence (a name); a hyphenated token, markup or interpolation
#            inside a code string; and `CEO queue`, which is a rename to
#            CEO-TODOs, not a spelling.
#   REPORTED `the queue` on a line about the CEO or TODOs: a note the model sees,
#            never a block, because one line cannot adjudicate it.
#
# IT NEVER SETS permissionDecision. `allow` would approve the write and bypass
# the permission flow; this hook only changes WHAT is written. It is registered
# ON ITS OWN on Write|Edit|MultiEdit|NotebookEdit, never inside the Write
# dispatcher (dispatch-pretooluse.sh folds no updatedInput emitter, the rule
# shell-evidence.sh set on Bash), and NEVER on Bash: shell writes are checked
# at land time instead.
#
# EDIT, MULTIEDIT AND NOTEBOOKEDIT carry a fragment. The fixer reads the target
# file, places the fragment where `old_string` is and decides it in the context
# it lands in (inside a fenced block, a quotation or a code string it is not
# prose). One file read, and only when the fragment has a match at all. When
# `old_string` is not in the file the hook does nothing: the edit fails on its
# own.
#
# ===========================================================================
# THE EXEMPTIONS (dialect-fix.py states each one; summarized here)
# ===========================================================================
#   PATH-LEVEL (whole write skipped): vendored third-party material by registry
#     LOOKUP (`.richos/vendored-material`, origin=third-party only — the 2026-08-30
#     incident below); vendor legal text; lockfiles and generated data;
#     captured evidence by path segment BELOW the governing repository (raw/,
#     fixtures/, logs/, transcripts/ ...); run output (*.log, *.jsonl); the
#     dialect files made of the words they govern (the table, its two hand-kept
#     lists, dialect-en-US.dict, this hook, its suite and its mutation script —
#     exact basenames, never a pattern); DIALECT_EXEMPT_PATHS.
#   CONTENT-LEVEL: fenced blocks, `>` lines, inline code spans, a line carrying
#     `dialect-exempt: <reason>` (a bare marker exempts nothing), paths, URLs and
#     identifiers (`colourPicker`, `text_colour`, `--brand-colour`, dotted
#     names), per-word ext-exempt (`grey` is a CSS named color), and
#     DIALECT_SCAN_ALLOWLIST substrings.
#
# ===========================================================================
# JURISDICTION — AND WHY IT DIFFERS FROM scan-secrets.sh ON PURPOSE
# ===========================================================================
# A dialect is a POLICY RELATIVE TO A DECLARED LOCALE, and a British company's
# repository is not committing a defect by writing `colour`. So:
#
#   * no `DIALECT_TARGET` in the governing orchestration.config -> SILENT
#     stand-down. Nothing declared, nothing to enforce.
#   * a `DIALECT_TARGET` other than `en-US` -> announced no-op. The only table
#     shipped is en-GB -> en-US.
#   * repository never adopted the engine -> LOUD stand-down, same as every
#     other guard.
#
# A cross-repository write takes the target repository's own DIALECT_TARGET
# when that repository has adopted, and otherwise the SEAT's — the 2026-08-31
# regression was a session seated in one repository writing British prose into
# another.
#
# FAIL-CLOSED on a missing python3, a missing table or fixer, a table that
# parses to nothing, and a fixer that crashes. FAILS OPEN (exit 0) on a malformed
# payload, matching its Write/Edit siblings' convention.

set -eo pipefail

command -v python3 >/dev/null 2>&1 || { echo "ERROR: dialect-guard.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/dialect-guard.sh
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
_SJ_LIB="$SCRIPT_DIR/../lib/seat-jurisdiction.sh"
if [ ! -f "$_SJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/dialect-guard.sh"
        echo "  scripts/lib/seat-jurisdiction.sh is missing at: $_SJ_LIB"
        echo "  Without it this guard cannot tell whether the artifact it was"
        echo "  handed belongs to the repository it governs, and a guard that"
        echo "  cannot tell must not answer."
    } >&2
    exit 2
fi
# shellcheck source=../lib/seat-jurisdiction.sh
. "$_SJ_LIB"

# Read the payload BEFORE resolving, so the payload's `cwd` is available as a
# resolution candidate. It is the only candidate a subagent session is
# guaranteed to carry.
INPUT="$(cat)"

if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    richos_announce_stand_down "scripts/hooks/dialect-guard.sh" \
        "this repository has not adopted the engine, so nothing written here is checked against a declared dialect"
    exit 0
else
    root_failure_banner "scripts/hooks/dialect-guard.sh" >&2
    exit 2
fi
# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this hook takes the SAME silent exit 0 that a
# well-formed payload for a DIFFERENT tool takes. This separates the two. NO
# VERDICT CHANGES — the exit is the one already taken — only the silence does.
# The measurement, the channel and the argument: scripts/lib/unevaluated-notice.sh.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "dialect-guard.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether the new content introduces a word that is not American English"
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${DIALECT_TARGET:=}"
: "${DIALECT_SCAN_ALLOWLIST:=}"
: "${DIALECT_EXEMPT_PATHS:=}"

# The tool and the file in ONE python3 start (it used to be two).
TOOL_NAME=""
FILE_PATH=""
{ IFS= read -r TOOL_NAME || true; IFS= read -r FILE_PATH || true; } <<META_EOF
$(printf '%s' "$INPUT" | python3 -c 'import json,sys
try:
    d = json.load(sys.stdin)
    ti = d.get("tool_input") or {}
    fp = ti.get("file_path") or ti.get("notebook_path") or ""
    print(d.get("tool_name", "") if isinstance(d.get("tool_name"), str) else "")
    print(fp.replace("\n", " ") if isinstance(fp, str) else "")
except Exception:
    print("")' 2>/dev/null || true)
META_EOF
case "$TOOL_NAME" in
  Write|Edit|MultiEdit|NotebookEdit) ;;
  *) exit 0 ;;
esac

# The dialect that governs THIS FILE, which is not necessarily the seat's — see
# the jurisdiction section. A target repository that has adopted the engine
# speaks for itself; one that has not inherits the seat's declaration.
DG_GOV=""
DG_GOV="$(richos_governing_root "$FILE_PATH" "${ENTITY_ROOT}" 2>/dev/null || true)"
# BOTH SIDES IN THE SAME SPELLING before the comparison — same reason as
# scan-secrets.sh: richos_governing_root answers physically, the seat arrives in
# whatever spelling the payload carried.
DG_SEAT_PHYS="$(richos_physical "${ENTITY_ROOT:-}" 2>/dev/null || printf '%s' "${ENTITY_ROOT:-}")"
if [ -n "$DG_GOV" ] && [ "$DG_GOV" != "$DG_SEAT_PHYS" ]; then
    DIALECT_TARGET=""
    DIALECT_SCAN_ALLOWLIST=""
    DIALECT_EXEMPT_PATHS=""
    # shellcheck disable=SC1090
    [ -f "$DG_GOV/orchestration.config" ] && . "$DG_GOV/orchestration.config"
    : "${DIALECT_TARGET:=}"
    : "${DIALECT_SCAN_ALLOWLIST:=}"
    : "${DIALECT_EXEMPT_PATHS:=}"
fi

# NOTHING DECLARED, NOTHING ENFORCED — silently. A repository with no
# DIALECT_TARGET has made no claim about what dialect its words are in.
[ -n "$DIALECT_TARGET" ] || exit 0

if [ "$DIALECT_TARGET" != "en-US" ]; then
    echo "NOTE: dialect-guard.sh: DIALECT_TARGET=\"$DIALECT_TARGET\" but the only table shipped is scripts/lib/dialect/dialect-en-US.generated.dict (en-GB -> en-US). Nothing was checked. Set DIALECT_TARGET=\"en-US\" or add a table for your locale — this hook will not pretend to enforce a locale it has no data for. (hook: scripts/hooks/dialect-guard.sh)" >&2
    exit 0
fi

DICT="$ENGINE_ROOT/scripts/lib/dialect/dialect-en-US.generated.dict"
if [ ! -f "$DICT" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — DIALECT ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/dialect-guard.sh"
        echo "  the American spelling table is missing at: $DICT"
        echo "  DIALECT_TARGET is declared, so this repository believes it is"
        echo "  governed. Refusing rather than passing every write through."
    } >&2
    exit 2
fi

# THE FIXER is code, not data, so it is found beside this hook (like the libraries
# above), not under the engine root a caller may point elsewhere.
FIXER="$SCRIPT_DIR/../lib/dialect/dialect-fix.py"
if [ ! -f "$FIXER" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — DIALECT ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/dialect-guard.sh"
        echo "  scripts/lib/dialect/dialect-fix.py is missing at: $FIXER"
        echo "  Every fix and every refusal this hook makes comes out of that file."
    } >&2
    exit 2
fi

# --- JURISDICTION: ANNOUNCED, NEVER SILENT --------------------------------
# This hook does NOT decline an artifact outside its seat — the 2026-08-31
# regression WAS a cross-repository write. But it must not judge another
# repository's file without saying so. It sits BELOW every stand-down: an
# announcement for a repository with a BLANK DIALECT_TARGET is noise with a
# serious face on (case D1 asserts the silent no-op is SILENT).
richos_assert_jurisdiction "scripts/hooks/dialect-guard.sh" "$ENTITY_ROOT" "$FILE_PATH" "file" "proceeds" || true

# --- IS THIS SOMEBODY ELSE'S PROSE? ---------------------------------------
# On 2026-08-30, commit 06f4a8221a61, this project's dialect sweep rewrote
# FOURTEEN lines across TWO vendored MIT-licensed skills
# (`engine/skills/copywriting/references/natural-transitions.md`, Corey Haines;
# `engine/skills/landing-page-taste/SKILL.md`, Leonxlnx), found six weeks later
# by accident. And the guard then refused both the verbatim re-vendoring and
# the notice that quoted the difference: one missing fact, three failures.
# A path-name heuristic cannot answer a provenance question, so the answer comes
# from the fact: `.richos/vendored-material`. Only `origin=third-party` exempts;
# material recorded as `richos` is still checked, because a registry that
# exempted everything it named would be an off switch with an inventory attached.
#
# DEFERRED UNTIL THERE IS SOMETHING TO FIX OR REFUSE: the lookup cost 250ms of
# pure bash on EVERY write when it ran up front, and it can only change the
# answer when the fixer found something. A silent fixer also removes the signal
# that found the 2026-08-30 damage (the refusal), which is why every fix is
# reported to the model and why this lookup still runs before any fix is made.
_DG_VM_LIB="$SCRIPT_DIR/../lib/vendored-material.sh"
if [ ! -f "$_DG_VM_LIB" ]; then
    # FAIL CLOSED, checked UP HERE rather than at the point of use:
    # contract-integrity.test.sh's SC1 starts every registered hook in a sandbox
    # and reads exactly this refusal.
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/dialect-guard.sh"
        echo "  scripts/lib/vendored-material.sh is missing at: $_DG_VM_LIB"
        echo "  Without it this guard cannot tell whose bytes it is about to"
        echo "  rewrite, and a guard that cannot tell must not answer."
    } >&2
    exit 2
fi

# _dg_is_vendored — rc 0 when the registry records this file as SOMEBODY
# ELSE'S. Exits 2 itself on a declared-but-unreadable registry: a guard that
# cannot tell whose bytes these are must not answer.
DG_VENDOR_MATCH=""
_dg_is_vendored() {
    [ -n "$FILE_PATH" ] || return 1
    # shellcheck source=../lib/vendored-material.sh
    . "$_DG_VM_LIB"
    local rc=0 rel
    vm_load_upward "$(dirname "$FILE_PATH")" || rc=$?
    case "$rc" in
      0) ;;
      1) return 1 ;;   # no repository above this file declares the contract
      *)
        {
            vm_broken_banner "scripts/hooks/dialect-guard.sh" "$VM_BROKEN_REASON"
            echo "  This guard refuses to judge a word in a file whose ownership it"
            echo "  cannot establish. Fix the registry, or delete it to stand the"
            echo "  vendoring contract down deliberately."
            echo "(hook: scripts/hooks/dialect-guard.sh)"
        } >&2
        exit 2 ;;
    esac
    rel="${FILE_PATH#"$VM_ROOT"/}"
    [ "$rel" != "$FILE_PATH" ] || return 1
    vm_is_third_party "$rel" || return 1
    DG_VENDOR_MATCH="$VM_MATCH"
    return 0
}

# EVIDENCE IS A PLACE INSIDE A REPOSITORY, NOT ANYWHERE ABOVE IT. Matched against
# the WHOLE absolute path, the captured-evidence segments (logs/, fixtures/ ...)
# also matched the directories a repository happens to LIVE in, and eight
# contract-integrity sections went red only inside the merge gate's private
# profile on 2026-10-01. So when the governing repository is known, the segments
# are read from the path BELOW it, in the physical spelling richos_governing_root
# answers in.
DIALECT_GOV_ROOT="$DG_GOV"
DIALECT_FILE_PHYS=""
if [ -n "$DG_GOV" ] && [ -n "$FILE_PATH" ]; then
    DIALECT_FILE_PHYS="$(richos_physical "$FILE_PATH" 2>/dev/null || true)"
fi
export DIALECT_TARGET DIALECT_SCAN_ALLOWLIST DIALECT_EXEMPT_PATHS DICT DIALECT_GOV_ROOT DIALECT_FILE_PHYS

# A fixer that CRASHES refuses: a write it could not judge is not a clean write.
if ! RESULT="$(printf '%s' "$INPUT" | python3 "$FIXER" hook)"; then
    echo "ERROR: dialect-guard.sh: scripts/lib/dialect/dialect-fix.py failed on this payload — refusing (fail-closed). Its own error is above. (hook: scripts/hooks/dialect-guard.sh)" >&2
    exit 2
fi
VERDICT="${RESULT%%$'\n'*}"
BODY=""
case "$RESULT" in *$'\n'*) BODY="${RESULT#*$'\n'}" ;; esac

case "$VERDICT" in
  CLEAN|NOTFOUND|PARSEFAIL)
    exit 0
    ;;
  NODICT)
    echo "ERROR: dialect-guard.sh: $DICT is present but yielded no usable rules — refusing (fail-closed). A table that parses to nothing is an enforcement outage that looks like a clean run." >&2
    exit 2
    ;;
  REPORTONLY)
    # WHOSE PROSE IS THIS? Asked here rather than before the scan, so a clean
    # write never pays for it. A note about somebody else's document is noise.
    _dg_is_vendored && exit 0
    printf '%s\n' "$BODY"
    exit 0
    ;;
  FIX)
    # Somebody else's prose is never rewritten: the write goes through as typed.
    _dg_is_vendored && exit 0
    printf '%s\n' "$BODY"
    exit 0
    ;;
  REFUSE)
    _dg_is_vendored && exit 0
    printf '%s\n' "$BODY" >&2
    exit 2
    ;;
  *)
    echo "ERROR: dialect-guard.sh: unexpected fixer output — refusing (fail-closed)" >&2
    exit 2
    ;;
esac
