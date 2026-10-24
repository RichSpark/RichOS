#!/usr/bin/env bash
#
# dialect-guard.sh — PreToolUse guard (Write|Edit|MultiEdit|NotebookEdit).
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# On 2026-08-29 the CEO ruled, verbatim: *"American English must be the language
# for UI as well as things like 'CEO queue'."* On 2026-08-30 a sweep fixed 654
# sites across 205 files. WITHIN HOURS, on 2026-08-31, ~20 fresh violations were
# written into the same record — including into the decision page that carries
# the ruling.
#
# The cause was structural and it is the whole design of this file. The sweep
# CLEANED what existed and CONSTRAINED NOTHING THAT CAME AFTER; its instrument
# was not even kept. Every rule that survives in this project has a write-time
# chokepoint — main-checkout writes, secrets, publication boundary, row
# currency, worktree isolation, the CEO-ask gate. This ruling had none, so it
# decayed in hours. A second sweep would have decayed on the same schedule.
#
# So: a guard, modelled on scan-secrets.sh — the closest existing shape. Same
# event, same matcher, same "inspect the content this call is about to
# introduce, and refuse with the findings NAMED" behavior.
#
# ===========================================================================
# WHAT IT CHECKS
# ===========================================================================
#   1. SPELLING. Every word in scripts/lib/dialect-en-US.dict — the vocabulary
#      lives there, as data, in ONE place, and nowhere else in this engine.
#      Case-preserving: the suggestion is re-cased to the form actually written.
#      BLOCKING (exit 2).
#
#   2. THE CEO'S LIST IS NOT A "QUEUE", and this one is NOT a spelling — which
#      is why it has its own rule and its own precision argument:
#
#      `queue` is a perfectly good word for a data structure and stays legal in
#      code and in technical prose about queues. What the ruling bans is `queue`
#      as the NAME OF THE CEO'S LIST. So the detector matches a COLLOCATION,
#      never the token:
#
#        BLOCKED   `CEO queue`, `CEO's queue`, `CEO queues` (any case, any run
#                  of whitespace between). Precision here is high enough to
#                  block on: the words "CEO" and "queue" adjacent are the name,
#                  not a data structure. Nobody has a job queue called the CEO.
#
#        REPORTED, NEVER BLOCKED   `his queue`, `her queue`, `my queue`,
#                  `your queue`, `the queue` on a line that also mentions
#                  CEO/TODO. In THIS record those nearly always mean his list;
#                  in an adopter's repository "the queue" on a line about a
#                  CEO-facing job runner is an ordinary sentence. I cannot
#                  separate the two from one line of text, so per the brief's
#                  own instruction I REPORT rather than block, say so here, and
#                  say so in the message. A guard that blocks a sentence it
#                  cannot actually adjudicate is the "cries wolf" failure this
#                  file's exemption list exists to avoid.
#
#      HYPHENATED FORMS ARE NOT MATCHED AT ALL — `.ceo-queue` is a real legacy
#      FILE NAME this engine still reads for backward compatibility
#      (UPGRADING.md), and `ceo_queue` would be an identifier. Renaming those
#      would break adopters. Collocation, not token, one more time.
#
#      RENAME NARRATION IS EXEMPT. You cannot document a rename without naming
#      the old name, so `called the CEO queue`, `renamed`, `formerly`,
#      `pre-rename`, `legacy`, `used to be`, `old name` within 60 characters
#      before the match exempts it. Without this the guard would forbid the
#      sentence that explains the guard.
#
# ===========================================================================
# THE EXEMPTIONS — SCOPED AS CAREFULLY AS THE 2026-08-30 SWEEP SCOPED ITSELF
# ===========================================================================
# That pass's REFUSALS were its most valuable output. Getting these right
# matters more than catching every last word: a guard that cries wolf is
# switched off within a day, and then the rule has no chokepoint again.
#
#   PATH-LEVEL (whole write skipped)
#     * VENDORED THIRD-PARTY MATERIAL, by REGISTRY LOOKUP rather than by path
#       name — `.richos/vendored-material` records who wrote every piece of
#       redistributable material in a tree, and an `origin=third-party` entry
#       covering the file exempts it. This is the only exemption here that
#       comes from a recorded fact, and it is the one that was missing on
#       2026-08-30 when this guard Americanized four lines of Corey Haines's
#       MIT-licensed prose inside `engine/skills/copywriting/`. A path-name
#       convention could not have caught that: a vendored skill sits at
#       `engine/skills/<name>/`, exactly where the skills we wrote sit.
#       Material recorded as `origin=richos` is STILL CHECKED — a registry that
#       exempted everything it named would be an off switch with an inventory
#       attached.
#     * vendor legal text: LICENSE / LICENCE / NOTICE / COPYING / COPYRIGHT /
#       THIRD-PARTY-NOTICES, in any casing, with or without an extension. NOT
#       OURS TO EDIT — and note the direction of the `license` trap: American
#       `license` is the CORRECT spelling, so `LICENSE.md` and the `license`
#       field in package.json are never flagged by a British-form dictionary in
#       the first place. The exemption here is for a vendor file that spells it
#       the other way.
#     * lockfiles and generated data: *.lock, package-lock.json, yarn.lock,
#       Cargo.lock, go.sum, *.min.js, *.min.css, *.patch, *.diff, *.po, *.pot
#     * CAPTURED EVIDENCE, by path segment: raw/, cold-open/, transcripts/,
#       transcript/, fixtures/, fixture/, corpora/, corpus/, snapshots/,
#       snapshot/, logs/, node_modules/, vendor/, third_party/, third-party/ —
#       read BELOW the governing repository when it is known, never from the
#       directories the repository itself lives in (see DIALECT_GOV_ROOT)
#     * run output: *.log, *.jsonl
#     * THIS GUARD'S OWN SIX FILES — dialect-en-US.dict, dialect-guard.sh,
#       dialect-guard.test.sh, and the three vocabulary files of the generated
#       American spelling table in scripts/lib/dialect/ (added 2026-09-27):
#       dialect-en-US.generated.dict, dialect-en-US.overrides.dict,
#       dialect-en-US.leave.tsv. Stated plainly rather than buried, because it
#       is self-serving on its face: a dictionary of British spellings is made
#       of British spellings, and a guard that cannot have its own vocabulary
#       edited is a guard nobody can maintain. The mitigation is that the
#       exemption is six named basenames, not a pattern anyone can slip into.
#       The table's generator, its test and its README carry no vocabulary
#       word at all (dialect-table.test.py proves it), so they need no entry.
#     * anything matching DIALECT_EXEMPT_PATHS in orchestration.config
#
#   CONTENT-LEVEL (per match)
#     * FENCED CODE BLOCKS (``` / ~~~), tracked across the new content
#     * INLINE CODE SPANS (odd number of backticks before the match on its line)
#     * BLOCKQUOTE lines (^\s*>) — quoted external material, and the CEO's own
#       verbatim words, neither of which anyone may "correct"
#     * a line carrying `dialect-exempt: <reason>` — a DECLARED exemption, in
#       the same discipline the contrast floor uses: calling something exempt is
#       a claim, and every claim is made where a reviewer sees it. A bare
#       `dialect-exempt:` with nothing after it does NOT exempt anything.
#     * URLS AND PATHS — the match's surrounding token contains `://` or `/`
#     * CODE IDENTIFIERS — mostly free: `\b` word boundaries already exclude
#       camelCase (`colourPicker`) and snake_case (`text_colour`), because the
#       adjacent character is a word character. What `\b` does NOT exclude is
#       kebab-case and CSS custom properties, so the enclosing
#       `[A-Za-z0-9_$./:-]` run is additionally checked for `_`, `$`, `::`,
#       `--` or a dotted member path, any of which mean identifier, not prose.
#     * per-word ext-exempt from the dictionary — `grey` is a LEGAL CSS named
#       color, so it is not flagged in .css/.scss/.sass/.less
#     * DIALECT_SCAN_ALLOWLIST in orchestration.config (space-separated literal
#       substrings), for an adopter's own known-safe strings
#
#   THE RESIDUAL, NAMED RATHER THAN GLOSSED. A bare one-word quoted value in a
#   source file — `"colour"` as a third-party wire value — IS flagged. I
#   considered exempting quoted literals with no whitespace and rejected it:
#   that shape is ALSO the shape of a one-word UI label, which is precisely the
#   string the CEO's ruling is about. Exempting it would put a hole straight
#   through the rule to close a rare annoyance. The allowlist and the
#   `dialect-exempt:` marker are the answer for the rare case; a silent hole
#   would not be.
#
# ===========================================================================
# JURISDICTION — AND WHY IT DIFFERS FROM scan-secrets.sh ON PURPOSE
# ===========================================================================
# scan-secrets.sh scans a file in ANOTHER repository anyway, because a leaked
# credential is still leaked wherever it lands. A dialect is not like that: it
# is a POLICY RELATIVE TO A DECLARED LOCALE, and a British company's repository
# is not committing a defect by writing `colour`. So:
#
#   * no `DIALECT_TARGET` in the governing orchestration.config -> SILENT
#     stand-down. Nothing declared, nothing to enforce.
#   * a `DIALECT_TARGET` other than `en-US` -> announced no-op. The only
#     dictionary shipped is en-GB -> en-US; the guard says so rather than
#     pretending to enforce a locale it has no data for.
#   * repository never adopted the engine -> LOUD stand-down, same as every
#     other guard.
#
# A cross-repository write takes the target repository's own DIALECT_TARGET
# when that repository has adopted, and otherwise the SEAT's — because the
# operator's declared dialect follows the operator across the repositories they
# write in during a session, and that cross-repository write is EXACTLY the
# shape of the 2026-08-31 regression (a session seated in one repo writing
# British prose into another).
#
# FAIL-CLOSED on a missing python3. FAILS OPEN (exit 0) on a malformed payload,
# matching its Write/Edit siblings' convention.

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

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
case "$TOOL_NAME" in
  Write|Edit|MultiEdit|NotebookEdit) ;;
  *) exit 0 ;;
esac

FILE_PATH="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); ti=d.get("tool_input",{}) or {}; print(ti.get("file_path") or ti.get("notebook_path") or "")' 2>/dev/null || true)"

# The dialect that governs THIS FILE, which is not necessarily the seat's — see
# the jurisdiction section. A target repository that has adopted the engine
# speaks for itself; one that has not inherits the seat's declaration.
DG_GOV=""
DG_GOV="$(richos_governing_root "$FILE_PATH" "${ENTITY_ROOT}" 2>/dev/null || true)"
# BOTH SIDES IN THE SAME SPELLING before the comparison — same reason as
# scan-secrets.sh: richos_governing_root answers physically, the seat arrives in
# whatever spelling the payload carried, and comparing the two raw makes the seat
# look foreign to itself.
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

# NOTHING DECLARED, NOTHING ENFORCED — silently. This is not a stand-down
# wearing a costume: a repository with no DIALECT_TARGET has made no claim
# about what dialect its words are in, and inventing one for it would be the
# policy expansion scan-secrets.sh's own header refuses.
[ -n "$DIALECT_TARGET" ] || exit 0

if [ "$DIALECT_TARGET" != "en-US" ]; then
    echo "NOTE: dialect-guard.sh: DIALECT_TARGET=\"$DIALECT_TARGET\" but the only dictionary shipped is scripts/lib/dialect-en-US.dict (en-GB -> en-US). Nothing was checked. Set DIALECT_TARGET=\"en-US\" or add a dictionary for your locale — this guard will not pretend to enforce a locale it has no data for. (hook: scripts/hooks/dialect-guard.sh)" >&2
    exit 0
fi

DICT="$ENGINE_ROOT/scripts/lib/dialect-en-US.dict"
if [ ! -f "$DICT" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — DIALECT ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/dialect-guard.sh"
        echo "  the vocabulary is missing at: $DICT"
        echo "  DIALECT_TARGET is declared, so this repository believes it is"
        echo "  governed. Refusing rather than passing every write through."
    } >&2
    exit 2
fi

# --- JURISDICTION: ANNOUNCED, NEVER SILENT --------------------------------
# This guard does NOT decline an artifact outside its seat — the 2026-08-31
# regression WAS a cross-repository write, a session seated in one repository
# putting British prose into another, so declining would aim the guard away
# from the case it was built for. But it must not judge another repository's
# file without saying so: what follows may apply the SEAT's declared dialect,
# allowlist and exempt paths to a repository that never declared any of them.
#
# AND IT SITS HERE, BELOW EVERY STAND-DOWN, NOT ABOVE THEM. An announcement is
# a claim that something is being enforced on somebody else's artifact. Placed
# earlier it fired for a repository with a BLANK DIALECT_TARGET — announcing
# jurisdiction over a decision that was never going to be made, which is noise
# with a serious face on. Caught by case D1 asserting the silent no-op is
# SILENT, which is why that case checks stderr rather than just the exit code.
richos_assert_jurisdiction "scripts/hooks/dialect-guard.sh" "$ENTITY_ROOT" "$FILE_PATH" "file" "proceeds" || true

# --- IS THIS SOMEBODY ELSE'S PROSE? ---------------------------------------
# THE PATH EXEMPTION THIS GUARD DID NOT HAVE, AND THE INCIDENT IT COST.
#
# On 2026-08-30, commit 06f4a8221a61, this project's dialect sweep rewrote
# FOURTEEN lines across TWO vendored skills: four in
# `engine/skills/copywriting/references/natural-transitions.md` (Corey Haines's,
# MIT, from `coreyhaines31/marketingskills`) and ten in
# `engine/skills/landing-page-taste/SKILL.md` (Leonxlnx, MIT). Nothing was
# hazardous and nothing leaked — but neither is ours to correct, and a future
# byte comparison against upstream would have shown fourteen unexplained
# differences with no record of who made them. Found by an audit on 2026-09-04,
# six weeks later, by accident.
#
# AND THE DAMAGE WAS A ONE-WAY DOOR. This guard REFUSES a write whose new
# content carries a foreign spelling, so re-vendoring either file verbatim from
# upstream was blocked too — and so was the attempt to QUOTE the two spellings
# into a notice explaining the divergence, which was refused twice before
# somebody phrased the prose around it. One missing fact, three failures: the
# guard caused the divergence, forbade the repair, and forbade the record of it.
#
# THE GUARD WAS NOT WRONG. It had no way to be right: it exempts `vendor/`,
# `third_party/` and `node_modules/` by NAME, and this engine vendors third-
# party work into `engine/skills/<name>/`, which looks exactly like the skills
# we wrote. A path-name heuristic cannot answer a provenance question.
#
# So the answer comes from the fact itself. `.richos/vendored-material` records
# who wrote every piece of redistributable material in a tree, and this guard
# asks it one question: is the file I am about to rewrite recorded as somebody
# else's? Only `origin=third-party` exempts. Material recorded as `richos` is
# still checked — and that asymmetry is the whole design, because a registry
# that exempted everything it named would be an off switch with an inventory
# attached.
#
# It is a LOOKUP, never an inference. This guard does not decide whose work a
# file is; it reads what somebody wrote down when they knew.
# AND IT IS DEFERRED UNTIL THERE IS SOMETHING TO SUPPRESS. The lookup was
# first written above the scan, as a path exemption -- correct, and it cost
# 250ms of pure bash on EVERY Write and Edit in every repository on this
# machine, doubling the guard's measured 231ms baseline. The registry can only
# ever change the answer when the scan has actually FOUND something, and the
# scan finds nothing on the overwhelming majority of writes. So the clean path
# pays nothing and the finding path pays once, below, where FOUND and
# REPORTONLY are handled. A guard that adds half a second to every edit is a
# guard somebody removes by the end of the week.
_DG_VM_LIB="$SCRIPT_DIR/../lib/vendored-material.sh"
if [ ! -f "$_DG_VM_LIB" ]; then
    # FAIL CLOSED, like every other missing predicate in this engine, and
    # checked UP HERE rather than at the point of use: a hook that starts fine
    # and only discovers at decision time that it cannot answer the ownership
    # question is a hook that ran blind. contract-integrity.test.sh's SC1 starts
    # every registered hook in a sandbox and reads exactly this refusal.
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
# ELSE'S. Sets DG_VENDOR_MATCH to the entry that covered it. Exits 2 itself on a
# declared-but-unreadable registry, for seat-jurisdiction.sh's rule: a guard
# that cannot tell whose bytes these are must not answer, and carrying on is
# precisely how the 2026-08-30 edit happened.
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

# EVIDENCE IS A PLACE INSIDE A REPOSITORY, NOT ANYWHERE ABOVE IT. The captured-
# evidence segments below (logs/, fixtures/, raw/ ...) describe directories a
# repository keeps its evidence in. Matched against the WHOLE absolute path,
# they also matched the directories a repository happens to LIVE in: a checkout
# under ~/work/logs/, or a test repository under a verification run's
# <run>/logs/fixtures/<id>/tmp/, had every file in it exempted as "evidence",
# and the guard passed British prose in complete silence. That is how eight
# contract-integrity sections went red only inside the merge gate's private
# profile on 2026-10-01: the probe's Layer T canary wrote a British word to
# <fixture repo>/__dialect_canary__.md, and the guard waved it through
# because the fixture repository's ancestors included logs/ and fixtures/.
# So when the governing repository is known, the segments are read from the
# path BELOW it, in the same physical spelling richos_governing_root answers
# in. A file in no known repository keeps the whole-path reading, as before.
DIALECT_GOV_ROOT="$DG_GOV"
DIALECT_FILE_PHYS=""
if [ -n "$DG_GOV" ] && [ -n "$FILE_PATH" ]; then
    DIALECT_FILE_PHYS="$(richos_physical "$FILE_PATH" 2>/dev/null || true)"
fi
export DIALECT_SCAN_ALLOWLIST DIALECT_EXEMPT_PATHS DICT DIALECT_GOV_ROOT DIALECT_FILE_PHYS

SCAN_PY="$(mktemp -t dialect-guard.XXXXXX.py)"
trap 'rm -f "$SCAN_PY"' EXIT
cat >"$SCAN_PY" <<'PY'
import json, os, re, sys

ALLOWLIST = [a for a in os.environ.get("DIALECT_SCAN_ALLOWLIST", "").split() if a]
EXTRA_EXEMPT_PATHS = [p for p in os.environ.get("DIALECT_EXEMPT_PATHS", "").split() if p]
DICT_PATH = os.environ.get("DICT", "")

try:
    payload = json.loads(sys.stdin.read())
except Exception:
    print("PARSEFAIL"); sys.exit(0)
if not isinstance(payload, dict):
    print("PARSEFAIL"); sys.exit(0)

tool_name = payload.get("tool_name", "")
ti = payload.get("tool_input", {})
if not isinstance(ti, dict):
    ti = {}
file_path = ti.get("file_path") or ti.get("notebook_path") or ""
if not isinstance(file_path, str):
    file_path = ""

# --- PATH-LEVEL EXEMPTIONS -------------------------------------------------
base = os.path.basename(file_path)
low_base = base.lower()
low_path = file_path.lower().replace("\\", "/")
# The path the evidence segments are read from: below the governing repository
# when it is known (see DIALECT_GOV_ROOT above), the whole path otherwise.
GOV_ROOT = os.environ.get("DIALECT_GOV_ROOT", "").rstrip("/")
FILE_PHYS = os.environ.get("DIALECT_FILE_PHYS", "")
segment_path = low_path
if GOV_ROOT and FILE_PHYS.startswith(GOV_ROOT + "/"):
    segment_path = FILE_PHYS[len(GOV_ROOT) + 1:].lower().replace("\\", "/")

VENDOR_LEGAL = ("license", "licence", "notice", "copying", "copyright",
                "third-party-notices", "third_party_notices", "authors",
                "patents")
GENERATED = ("package-lock.json", "yarn.lock", "pnpm-lock.yaml", "cargo.lock",
             "go.sum", "composer.lock", "gemfile.lock")
EVIDENCE_SEGMENTS = ("raw", "cold-open", "transcripts", "transcript",
                     "fixtures", "fixture", "corpora", "corpus", "snapshots",
                     "snapshot", "logs", "node_modules", "vendor",
                     "third_party", "third-party", "__pycache__", ".git")
EVIDENCE_EXTS = (".log", ".jsonl", ".lock", ".min.js", ".min.css", ".patch",
                 ".diff", ".po", ".pot", ".map", ".sha256")
# This guard's own six files. Self-serving on its face, so it is named in the
# header and enumerated here rather than expressed as a pattern.
OWN_FILES = ("dialect-en-us.dict", "dialect-guard.sh", "dialect-guard.test.sh",
             "dialect-en-us.generated.dict", "dialect-en-us.overrides.dict",
             "dialect-en-us.leave.tsv")

def path_exempt():
    if not file_path:
        return ""
    if low_base in OWN_FILES:
        return "this guard's own vocabulary/implementation/test file"
    stem = low_base.split(".")[0]
    if stem in VENDOR_LEGAL:
        return "vendor legal text (%s) — not ours to edit" % base
    if low_base in GENERATED:
        return "generated lockfile"
    for ext in EVIDENCE_EXTS:
        if low_base.endswith(ext):
            return "generated/captured file type (%s)" % ext
    for seg in segment_path.split("/"):
        if seg in EVIDENCE_SEGMENTS:
            return "captured evidence — path segment '%s/'" % seg
        # ...and a QUALIFIED one. Real evidence directories get qualified by
        # what they hold: `license-snapshots/`, `call-transcripts/`,
        # `interview-corpus/`. Found the hard way — the first captured page this
        # guard met lived in `licence-snapshots/`, whose exact segment matched
        # nothing, so a vendor's own page would have been "corrected". Only the
        # LAST hyphen component is consulted, so `raw-notes/` is still prose.
        if "-" in seg and seg.rsplit("-", 1)[1] in EVIDENCE_SEGMENTS:
            return "captured evidence — path segment '%s/'" % seg
    for p in EXTRA_EXEMPT_PATHS:
        if p and p.lower() in low_path:
            return "DIALECT_EXEMPT_PATHS entry '%s'" % p
    return ""

skip_reason = path_exempt()
if skip_reason:
    print("CLEAN")
    sys.exit(0)

ext = ""
if "." in low_base:
    ext = low_base.rsplit(".", 1)[1]

# --- THE VOCABULARY --------------------------------------------------------
RULES = {}          # lowercase british form -> (american form, set(ext-exempt))
try:
    with open(DICT_PATH, "r", encoding="utf-8") as fh:
        for raw in fh:
            line = raw.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 2:
                continue
            bad = parts[0].strip().lower()
            good = parts[1].strip()
            exts = set()
            if len(parts) > 2:
                for opt in parts[2].strip().split(","):
                    opt = opt.strip()
                    if opt.startswith("ext-exempt="):
                        exts.update(e.strip().lower() for e in opt[len("ext-exempt="):].split(",") if e.strip())
                    elif opt and "=" not in opt and exts:
                        exts.add(opt.lower())
            if bad and good:
                RULES[bad] = (good, exts)
except Exception:
    print("NODICT"); sys.exit(0)

if not RULES:
    print("NODICT"); sys.exit(0)

# --- THE NEW TEXT THIS CALL WOULD INTRODUCE --------------------------------
texts = []
if tool_name == "Write":
    texts.append(ti.get("content") or "")
elif tool_name == "Edit":
    texts.append(ti.get("new_string") or "")
elif tool_name == "MultiEdit":
    for e in (ti.get("edits") or []):
        if isinstance(e, dict):
            texts.append(e.get("new_string") or "")
elif tool_name == "NotebookEdit":
    texts.append(ti.get("new_source") or "")
blob = "\n".join(t for t in texts if isinstance(t, str))

WORD_RE = re.compile(r"\b(%s)\b" % "|".join(sorted(RULES, key=len, reverse=True)), re.IGNORECASE)
CEO_QUEUE_RE = re.compile(r"\bCEO(?:'s|s)?\s+queues?\b", re.IGNORECASE)
SOFT_QUEUE_RE = re.compile(r"\b(?:his|her|my|your|the)\s+queues?\b", re.IGNORECASE)
SOFT_CONTEXT_RE = re.compile(r"\b(CEO|TODO|TODOs)\b")
RENAME_CUE_RE = re.compile(
    r"(called|renamed|rename|formerly|former|pre-rename|legacy|used to be|old name|was the|previously|instead of|no longer)",
    re.IGNORECASE)
TOKEN_CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_$./:\\-"
FENCE_RE = re.compile(r"^\s*(```|~~~)")
# The reason must START WITH A WORD CHARACTER, not merely be "something". A
# bare marker inside an HTML comment (`<!-- dialect-exempt: -->`) otherwise
# reads `-->` as its own justification, which is a bare marker wearing a
# reason — exactly the shape the CEO-ask gate's own escape hatch refuses.
DECLARED_EXEMPT_RE = re.compile(r"dialect-exempt:\s*[A-Za-z0-9]")

def recase(observed, target):
    if observed.isupper() and len(observed) > 1:
        return target.upper()
    if observed[:1].isupper():
        return target[:1].upper() + target[1:]
    return target

def enclosing_token(line, start, end):
    i = start
    while i > 0 and line[i - 1] in TOKEN_CHARS:
        i -= 1
    j = end
    while j < len(line) and line[j] in TOKEN_CHARS:
        j += 1
    return line[i:j]

def looks_like_identifier_or_path(tok):
    if "/" in tok or "://" in tok or "\\" in tok:
        return True
    if "_" in tok or "$" in tok or "::" in tok or "--" in tok:
        return True
    if re.search(r"\.[A-Za-z_]", tok):
        return True
    return False

def inside_inline_code(line, col):
    return line.count("`", 0, col) % 2 == 1

FINDINGS = []          # (label, site)
SOFT = []              # (label, site) — reported, never blocking

in_fence = False
for line in blob.split("\n"):
    if FENCE_RE.match(line):
        in_fence = not in_fence
        continue
    if in_fence:
        continue
    stripped = line.lstrip()
    if stripped.startswith(">"):
        continue
    if DECLARED_EXEMPT_RE.search(line):
        continue

    for m in WORD_RE.finditer(line):
        observed = m.group(1)
        good, exempt_exts = RULES[observed.lower()]
        if ext and ext in exempt_exts:
            continue
        if inside_inline_code(line, m.start()):
            continue
        tok = enclosing_token(line, m.start(), m.end())
        if tok != observed and looks_like_identifier_or_path(tok):
            continue
        if any(a in line for a in ALLOWLIST):
            continue
        FINDINGS.append(("%s -> %s" % (observed, recase(observed, good)), line.strip()[:110]))

    for m in CEO_QUEUE_RE.finditer(line):
        if inside_inline_code(line, m.start()):
            continue
        if RENAME_CUE_RE.search(line[max(0, m.start() - 60):m.start()]):
            continue
        if any(a in line for a in ALLOWLIST):
            continue
        FINDINGS.append(("%s -> CEO-TODOs" % m.group(0), line.strip()[:110]))

    for m in SOFT_QUEUE_RE.finditer(line):
        if inside_inline_code(line, m.start()):
            continue
        if not SOFT_CONTEXT_RE.search(line):
            continue
        if RENAME_CUE_RE.search(line[max(0, m.start() - 60):m.start()]):
            continue
        if any(a in line for a in ALLOWLIST):
            continue
        SOFT.append((m.group(0).strip(), line.strip()[:110]))

if FINDINGS:
    print("FOUND")
    for label, site in FINDINGS:
        print("BLOCK\t%s\t%s" % (label, site))
    for label, site in SOFT:
        print("REPORT\t%s\t%s" % (label, site))
elif SOFT:
    print("REPORTONLY")
    for label, site in SOFT:
        print("REPORT\t%s\t%s" % (label, site))
else:
    print("CLEAN")
PY

RESULT="$(printf '%s' "$INPUT" | python3 "$SCAN_PY" 2>/dev/null || printf 'PARSEFAIL')"

case "$(printf '%s' "$RESULT" | head -1)" in
  CLEAN|PARSEFAIL)
    exit 0
    ;;
  NODICT)
    echo "ERROR: dialect-guard.sh: $DICT is present but yielded no usable rules — refusing (fail-closed). A vocabulary that parses to nothing is an enforcement outage that looks like a clean run." >&2
    exit 2
    ;;
  REPORTONLY)
    # WHOSE PROSE IS THIS? Asked here rather than before the scan, so a clean
    # write never pays for it. See _dg_is_vendored's header.
    _dg_is_vendored && exit 0
    {
      echo "=== Dialect NOTE (not blocked) ==="
      echo "  '${FILE_PATH:-<unknown file>}' uses 'queue' in a way that MIGHT mean the CEO's list,"
      echo "  which is CEO-TODOs (CEO ruling 2026-08-29). Reported rather than blocked because"
      echo "  this collocation cannot be adjudicated from one line — 'the queue' is also an"
      echo "  ordinary sentence about a data structure:"
      printf '%s\n' "$RESULT" | tail -n +2 | while IFS=$'\t' read -r kind label site; do
        [ "$kind" = "REPORT" ] && echo "    ? $label   — $site"
      done
      echo "(hook: scripts/hooks/dialect-guard.sh)"
    } >&2
    exit 0
    ;;
  FOUND)
    _dg_is_vendored && exit 0
    {
      echo "=== Dialect check BLOCKED ==="
      echo "  Refusing to write '${FILE_PATH:-<unknown file>}' — this content introduces wording"
      echo "  that is not $DIALECT_TARGET (CEO ruling 2026-08-29: American English is the language"
      echo "  of every string a customer reads, and of the record):"
      printf '%s\n' "$RESULT" | tail -n +2 | while IFS=$'\t' read -r kind label site; do
        if [ "$kind" = "BLOCK" ]; then
          echo "    - $label"
          echo "        $site"
        else
          echo "    ? $label (reported, not the reason for this block)"
          echo "        $site"
        fi
      done
      echo "  FIX THE WORD. If the text is genuinely exempt — quoted external material, a"
      echo "  protocol value, a name you do not own — declare it where a reviewer will see it:"
      echo "    * put it in a code span or fenced block, or a > blockquote if it is a quotation;"
      echo "    * or add 'dialect-exempt: <reason>' to the line (a bare marker does nothing);"
      echo "    * or add the literal substring to DIALECT_SCAN_ALLOWLIST in orchestration.config."
      echo "  IF THIS FILE IS SOMEBODY ELSE'S WORK, that is not an exemption to declare — it"
      echo "  is a fact to record. Add an entry with origin=third-party to the repository's"
      echo "  .vendored-material registry and this guard will leave the whole path alone,"
      echo "  including a verbatim re-vendoring from upstream. On 2026-08-30 this guard"
      echo "  rewrote 14 lines across two vendored skills for want of that entry, and then"
      echo "  refused the commit that tried to quote the difference back."
      echo "  To turn this off for a repository whose product does not speak $DIALECT_TARGET,"
      echo "  blank DIALECT_TARGET in orchestration.config — never weaken the dictionary."
      echo "(hook: scripts/hooks/dialect-guard.sh)"
    } >&2
    exit 2
    ;;
  *)
    echo "ERROR: dialect-guard.sh: unexpected scanner output — refusing (fail-closed)" >&2
    exit 2
    ;;
esac
