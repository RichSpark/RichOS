#!/usr/bin/env bash
#
# guard-reference-ledger.sh — BLOCKING PreToolUse guard on the Agent tool.
#
# REFUSES A DISPATCH THAT BUILDS ON A SURFACE A NAMED REFERENCE HAS ALREADY
# ANSWERED, UNLESS THE PROMPT CITES THE ROW.
#
# ===========================================================================
# THE RULING, AND THE FAILURE IT WAS GIVEN FOR
# ===========================================================================
# CEO, 2026-09-19 23:15-23:25Z, verbatim:
#
#     "In other words, you ARE FUCKING REINVENTING THE FUCKING WHEEL FROM
#      SCRATCH"
#
#     "WHEN WILL ALL THIS WRITTEN DOWN SO THAT I DON'T GET MY FUCKING TIME
#      WASTED WITH THIS RETARDED-FUCK REINVENTION OF THE FUCKING WHEEL???"
#
# Written down the same night as femcboost CLAUDE.md's hard rule "Never
# Reinvent What a Named Reference Already Solved" (commit 321dbc263) and
# richos-hq wiki/ceo-decisions.md §66. This file is the mechanism that rule
# promises, and the answer to the second sentence is that it is enforced here
# rather than remembered.
#
# WHAT HAPPENED, because it decides this file's shape. On 2026-09-18 Reed read
# T3 Code's phone stack in full and named the connection runtime and the outbox
# as the things to take for our phone path
# (richos-hq/docs/research/t3code-mobile-vs-richos-phone-2026-09-18.md §1
# item 6). Three phone briefs followed on 2026-09-19. Not one of them cited it;
# all three patched richos/web/web-app/lib/link.js from first principles. The
# read existed, it was correct, it was in the record, and nobody opened it at
# the moment of writing a brief.
#
# A rule enforced by remembering lasts exactly as long as the remembering. That
# is the same finding guard-no-home-network-phone.sh was built from one day
# earlier, and it is why this is a hook and not a paragraph.
#
# ===========================================================================
# THE SURFACES ARE DATA, NOT CODE
# ===========================================================================
# scripts/hooks/adoption-ledger.surfaces is one record per ledger area: its
# section, its verdict, the PATH FRAGMENTS of our code it covers, and the
# PHRASES that name it in prose. Adding an area is an edit to that file and
# never an edit to this one. The file's own header explains the format; the
# guard reads it and nothing else.
#
# The file is searched for in three places, first hit wins:
#
#   $RICHOS_REFERENCE_LEDGER_SURFACES        explicit; also the test affordance
#   <entity root>/.richos/adoption-ledger.surfaces   an adopter's own rows
#   <engine root>/scripts/hooks/adoption-ledger.surfaces   the shipped default
#
# THE DATA FILE DOES NOT LIVE IN THE REPOSITORY THAT HOLDS THE LEDGER, and
# that is a deviation from the brief that ordered this guard, stated here
# rather than buried. The brief prescribed richos-hq/docs/research/. Two facts
# decided otherwise. (1) This guard fires in a femcboost session, for a
# dispatch into richos, against a document in richos-hq: three repositories,
# and nothing in this engine locates the third deterministically. A guard whose
# DATA lives somewhere it may not find stands down silently on the machine
# where the checkout is missing, which is the exact silent-skip failure
# scripts/lib/resolve-roots.sh exists to end. (2) Every other guard datum in
# this engine ships with the guard — scripts/lib/ci-known-red.tsv,
# ci-unit-weights.tsv, dialect-en-US.dict, scripts/hooks/dispatch-pretooluse.manifest.
# The LEDGER stays where it is and is named by the data file; only the SIGNALS
# ship with the code that reads them.
#
# ===========================================================================
# THE TEST: THREE CONJUNCTS, AND THE MIDDLE ONE IS WHY IT DOES NOT FIRE ON
# EVERY BRIEF THAT SAYS THE WORD "PHONE"
# ===========================================================================
#   (a) A BUILD SECTION EXISTS. A markdown heading naming the files this
#       teammate will EDIT — Scope, Scope and files, What to build, What to
#       change, What to fix, Files, Implementation, The work. The section runs
#       to the next heading at the same level or higher. TEXT OUTSIDE EVERY
#       BUILD SECTION IS NOT READ FOR SIGNALS AT ALL, which is what keeps a
#       narrative paragraph ("Ray measured 7,288 ms on the phone page") from
#       being treated as an instruction to build on the phone page.
#
#   (b) A LEDGER SIGNAL INSIDE THAT SECTION. A path fragment or a phrase from
#       adoption-ledger.surfaces. Inside a Scope section a backticked path IS
#       the signal, so — unlike guard-no-home-network-phone.sh — inline code
#       spans are deliberately NOT stripped here. Opposite decision, opposite
#       reason: there a backtick marks somebody else's words, here it marks the
#       file about to be edited.
#
#   (c) THE DISPATCH IS NOT A READ. A prompt that tells its teammate not to
#       change the code — "Do not fix anything; record it", "Do not modify
#       anything under richos/app", "read-only", "do not write code" — is a
#       research or QA dispatch and is never refused, whatever it names. This
#       conjunct is not decoration: it is the one that passes Reed's own
#       adoption-read brief, which is DENSER in ledger signals than any build
#       brief in the corpus (it commissions the ledger) and must obviously pass.
#
# WHY (a) AND (c) BOTH, when either alone nearly works: measured, each alone
# has a failure the other covers. Ray's VM walk has no build heading but names
# scripts; Reed's read has "## What to deliver" and a scope-shaped list of
# areas. The conjunction is what makes "builds on it" a fact about the text
# rather than a judgment about the author.
#
# ===========================================================================
# MEASURED, ON REAL TEXT, BEFORE IT WAS WIRED
# ===========================================================================
# Every brief in richos-hq/docs/briefs (288 .md files) plus every spawn prompt
# of the session that ordered this guard (37), driven through THIS FILE as real
# Agent payloads. The numbers, the fires and the judgment on each one are in
# guard-reference-ledger.test.sh's header, where a reader can re-run them.
#
# Reproduce before changing any pattern here: build an Agent payload per
# document and pipe it in. The classifier is this file's own python3 block, so
# the measurement is of the shipped rule and not of a model of it.
#
# ===========================================================================
# THE HATCH IS A CITATION OR AN ARGUED "NONE" — NEVER A MARKER
# ===========================================================================
#     reference: adoption ledger §2.5 — ADOPT AS-IS, derive the port from the
#                worktree path
#
# needs a LEDGER NAME (the document's file name, or the name the data file
# declares) AND A SECTION, and the section MUST EXIST in the ledger. The guard
# opens the document and looks for the heading; a citation of a section that is
# not there is refused BY NAME. Half the value of the hatch is being made to
# open the file — which is precisely the step the three phone briefs skipped.
#
#     reference: none — <at least 40 characters saying why no row covers this>
#
# is the honest answer when the surface genuinely is not in the ledger, and it
# is logged to .claude/state/reference-none.log so a habit of waiving is
# visible rather than invisible. A BARE `reference:` exempts nothing and is
# refused by name.
#
# Where the ledger cannot be located at all, a citation is accepted on its
# shape and the log says UNVERIFIED. "I cannot check" is never "forbidden".
#
# ===========================================================================
# FAIL OPEN, AND LOUDLY
# ===========================================================================
#   NOT ADOPTED (no orchestration.config)  -> STAND DOWN, silent.
#   REFERENCE_LEDGER_GUARD="off"           -> STAND DOWN, silent.
#   NOT AN Agent CALL                      -> silent.
#   NO BUILD SECTION / NO SIGNAL / A READ  -> SILENT. This is the overwhelming
#                                             majority of all dispatches.
#   THE SURFACES FILE IS MISSING           -> ALLOW + ANNOUNCE. The engine ships
#                                             one, so its absence is a broken
#                                             install, not a policy.
#   PAYLOAD UNPARSEABLE, python3 MISSING   -> ALLOW + ANNOUNCE.
#   THE LEDGER DOCUMENT UNREADABLE         -> the citation is accepted on its
#                                             shape, logged UNVERIFIED. It NEVER
#                                             turns a pass into a refusal.
#
# The one thing that is NOT fail-open: scripts/lib/resolve-roots.sh missing.
#
# NOTE: hooks are snapshotted at session start. This one is INERT until the
# next session — it assumes nothing about being live in the session that adds
# it, and the session that added it is the session that wrote the three briefs.

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
  hook: scripts/hooks/guard-reference-ledger.sh
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

# announce_off <one-line> — the best-effort loud channel for a fail-open. BOTH
# stderr and systemMessage, because neither is proven for this event and a
# condition announced on nothing is the defect the banner above is about.
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
    announce_off "REFERENCE-LEDGER GUARD IS OFF: python3 is not on PATH, so this dispatch was NOT checked against the adoption ledger (ceo-decisions §66)."
    exit 0
fi

if ! resolve_entity_root "$INPUT"; then
    if [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
        exit 0
    fi
    announce_off "REFERENCE-LEDGER GUARD IS OFF: it cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether this dispatch rebuilds something the adoption ledger already answered."
    exit 0
fi
ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"

# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this guard takes the SAME silent exit 0 that a
# well-formed payload for a DIFFERENT tool takes. This separates the two. NO
# VERDICT CHANGES — only the silence. See scripts/lib/unevaluated-notice.sh.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-reference-ledger.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this dispatch builds on a surface the adoption ledger already answered"
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"

# --- THE OFF SWITCH, AND WHY THERE IS NO ON SWITCH -------------------------
# Unset means ON, for the ruling's own reason: the CEO asked when it would be
# "WRITTEN DOWN" so his time stops being wasted, and a contract nobody has
# switched on yet is a contract remembered. The three conjuncts are what make
# that safe to ship — a repository with no adoption ledger has no rows, and a
# guard with no rows is silent on every dispatch by construction.
: "${REFERENCE_LEDGER_GUARD:=on}"
case "$REFERENCE_LEDGER_GUARD" in
    off|OFF|0|no|NO) exit 0 ;;
esac
: "${REFERENCE_LEDGER_RULING:=§66}"

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
case "$TOOL_NAME" in
    Agent) ;;
    *) exit 0 ;;
esac

# --- THE SURFACES FILE -----------------------------------------------------
SURFACES=""
for _cand in \
    "${RICHOS_REFERENCE_LEDGER_SURFACES:-}" \
    "$ENTITY_ROOT/.richos/adoption-ledger.surfaces" \
    "$ENGINE_ROOT/scripts/hooks/adoption-ledger.surfaces"
do
    [ -n "$_cand" ] || continue
    if [ -f "$_cand" ]; then SURFACES="$_cand"; break; fi
done
if [ -z "$SURFACES" ]; then
    announce_off "REFERENCE-LEDGER GUARD IS OFF: adoption-ledger.surfaces was not found (looked in \$RICHOS_REFERENCE_LEDGER_SURFACES, $ENTITY_ROOT/.richos/ and $ENGINE_ROOT/scripts/hooks/). The engine ships one, so this is a broken install and not a policy. This dispatch was NOT checked."
    exit 0
fi

# --- LOCATE THE LEDGER DOCUMENT --------------------------------------------
# Declared by the data file as <ledger-repo>/<ledger>. Looked for beside the
# entity root and beside the engine's own checkout, which is where a sibling
# repository is on every machine this has ever run on. NOT FOUND is never a
# refusal: it downgrades a citation to UNVERIFIED and nothing else.
LEDGER_REL="$(sed -n 's/^ledger:[[:space:]]*//p' "$SURFACES" | sed -n 1p)"
LEDGER_REPO="$(sed -n 's/^ledger-repo:[[:space:]]*//p' "$SURFACES" | sed -n 1p)"
LEDGER_DOC=""
if [ -n "${RICHOS_REFERENCE_LEDGER_DOC:-}" ]; then
    # DECLARED, and therefore the ONLY candidate — including when it does not
    # exist. An explicit pointer that silently falls back to a search is a
    # pointer nobody can use to say "the ledger is unreachable here", which is
    # the state the UNVERIFIED path exists for and the state a suite must be
    # able to construct.
    [ -f "${RICHOS_REFERENCE_LEDGER_DOC}" ] && LEDGER_DOC="$RICHOS_REFERENCE_LEDGER_DOC"
elif [ -n "$LEDGER_REL" ] && [ -n "$LEDGER_REPO" ]; then
    _ENGINE_MAIN_DIR=""
    if command -v resolve_main_checkout >/dev/null 2>&1; then
        _ENGINE_MAIN_DIR="$(resolve_main_checkout "$ENGINE_ROOT" 2>/dev/null || true)"
    fi
    for _base in "$(dirname "$ENTITY_ROOT")" "$(dirname "${_ENGINE_MAIN_DIR:-$ENGINE_ROOT}")" "$(dirname "$(dirname "${_ENGINE_MAIN_DIR:-$ENGINE_ROOT}")")"; do
        [ -n "$_base" ] || continue
        if [ -f "$_base/$LEDGER_REPO/$LEDGER_REL" ]; then
            LEDGER_DOC="$_base/$LEDGER_REPO/$LEDGER_REL"
            break
        fi
    done
fi

# --- THE CLASSIFIER --------------------------------------------------------
# Prints one of:
#   CLEAN
#   REF<TAB><the reference: line's argument>          (a citation was given)
#   BARE                                              (a `reference:` with no argument)
#   FIND<TAB><area>|<section>|<signal>|<excerpt>      (one per row, at most 5)
VERDICT="$(printf '%s' "$INPUT" | SURFACES="$SURFACES" python3 -c '
import json, os, re, sys

try:
    d = json.load(sys.stdin)
except Exception:
    print("PARSEFAIL"); sys.exit(0)

ti = d.get("tool_input") or {}
if not isinstance(ti, dict):
    print("PARSEFAIL"); sys.exit(0)

blob = "\n".join(str(ti.get(k, "") or "") for k in ("prompt", "description"))
if not blob.strip():
    print("CLEAN"); sys.exit(0)

# ---- the data file --------------------------------------------------------
rows, cur = [], {}
try:
    with open(os.environ["SURFACES"], encoding="utf-8") as fh:
        text = fh.read()
except OSError:
    print("PARSEFAIL"); sys.exit(0)
for raw in text.splitlines():
    line = raw.rstrip()
    if line.startswith("#"):
        continue
    if not line.strip():
        if cur:
            rows.append(cur); cur = {}
        continue
    if ":" not in line:
        continue
    k, v = line.split(":", 1)
    cur[k.strip()] = v.strip()
if cur:
    rows.append(cur)
AREAS = [r for r in rows if r.get("area")]
if not AREAS:
    print("CLEAN"); sys.exit(0)

# ---- the hatch, read before anything else so a cited brief costs nothing ---
BARE_RE = re.compile(r"^[ \t]*reference:[ \t]*$", re.MULTILINE)
REF_RE = re.compile(r"^[ \t]*reference:[ \t]*(\S.*)$", re.MULTILINE)

# ---- (c) IS THIS A READ RATHER THAN A BUILD? ------------------------------
# Document level. A dispatch that forbids its teammate to change the code is
# not building on anything, however many surfaces it names. Reed’s adoption
# read ("Do not modify anything under `richos/app`") and Ray’s VM walk ("Do
# not fix anything; record it") are the two measured proofs, and they are the
# densest signal-carrying documents in the whole corpus.
#
# THE QUANTIFIER IS LOAD-BEARING AND IT WAS MEASURED, NOT CHOSEN. A first
# draft matched a bare "do not touch", and it PASSED the delivery brief this
# guard was written to refuse: that brief says "you do not touch the host
# keychain" about one file it must leave alone, while commissioning changes
# to six others. A build brief names things NOT to touch; a read brief says
# the teammate changes NOTHING. Only the universal form is a read.
READ_RE = re.compile("|".join([
    r"\bdo(?:es)? not (?:modify|change|edit|fix|touch|write|patch|repair)\b"
    r"[^.\n]{0,40}\b(?:anything|nothing|any (?:file|code)|a single)\b",
    r"\bdon.t (?:modify|change|edit|fix|touch|write|patch|repair)\b"
    r"[^.\n]{0,40}\b(?:anything|nothing|any (?:file|code)|a single)\b",
    r"\b(?:fix|change|modify|edit) nothing\b",
    # `read-only` ALONE IS NOT HERE, and that is the second thing the
    # measurement decided. The same delivery brief contains "(Rich read 6
    # attribute lines for that service, read-only; nothing deleted)" —
    # a parenthesis about what the LEAD did, not about what the teammate may
    # do — and a bare `read-only` exempted the brief on it.
    r"\bread-only (?:read|task|dispatch|brief|audit|walk|pass|job)\b",
    r"\bno code changes?\b",
    r"\bdo not (?:write|produce|ship) (?:any )?code\b",
    r"\bdo not fix(?: it)?[;,.] record\b",
]), re.IGNORECASE)
IS_READ = bool(READ_RE.search(blob))

# ---- (a) THE BUILD SECTIONS ----------------------------------------------
# A heading that names the files this teammate will EDIT. The section runs to
# the next heading at the same level or higher; a deeper heading stays inside
# it. Text before the first build heading is not a build section at all.
HEAD_RE = re.compile(r"^(#{1,6})[ \t]*(.*)$")
BUILD_RE = re.compile(
    r"^(?:the\s+)?(?:"
    r"scope"
    r"|files?"
    r"|what to build"
    r"|what to change"
    r"|what to fix"
    r"|what to implement"
    r"|implementation"
    r"|the work"
    r"|the fix(?:es)?"
    r"|the change(?:s)?"
    r")\b", re.IGNORECASE)

lines = blob.split("\n")
sections = []          # (start_line_no, [lines])
cur_level, cur_lines, cur_start = 0, None, 0
for n, raw in enumerate(lines, 1):
    m = HEAD_RE.match(raw)
    if m:
        level = len(m.group(1))
        title = m.group(2).strip().lstrip("*_ ").strip()
        if cur_lines is not None and level <= cur_level:
            sections.append((cur_start, cur_lines))
            cur_lines = None
        if BUILD_RE.match(title):
            cur_level, cur_start, cur_lines = level, n, []
            continue
    if cur_lines is not None:
        cur_lines.append((n, raw))
if cur_lines is not None:
    sections.append((cur_start, cur_lines))

if not sections:
    print("CLEAN"); sys.exit(0)

# ---- (b) A LEDGER SIGNAL INSIDE A BUILD SECTION ---------------------------
# INLINE CODE SPANS ARE NOT STRIPPED. In a Scope section a backticked path is
# the file about to be edited, which is the opposite of what a backtick means
# in the prose guards. Stated because it is a deliberate divergence.
def split_csv(s):
    return [x.strip() for x in (s or "").split(",") if x.strip()]

FINDINGS = []
for row in AREAS:
    paths = split_csv(row.get("paths"))
    weak = split_csv(row.get("weak-paths"))
    terms = split_csv(row.get("terms"))
    term_res = [re.compile(r"(?<![A-Za-z0-9])" + re.escape(t) + r"(?![A-Za-z0-9])",
                           re.IGNORECASE) for t in terms]
    # A WEAK PATH IS A FILE THE ROW SHARES WITH OTHER WORK, so it counts only
    # where the row is also the SUBJECT: one of its own terms, anywhere in the
    # prompt. Twelve of the thirteen briefs that named app/ui/phone.js on
    # 2026-09-19 were about focus, scroll and splash placement and carry no
    # transport word at all; the thirteenth is the one this guard exists for.
    topical = any(rx.search(blob) for rx in term_res) if weak else False
    hit = None
    for start, body in sections:
        for n, raw in body:
            low = raw.lower()
            for p in paths:
                if p.lower() in low:
                    hit = (n, p, raw.strip()); break
            if hit:
                break
            if topical:
                for p in weak:
                    if p.lower() in low:
                        hit = (n, p, raw.strip()); break
                if hit:
                    break
            for rx, t in zip(term_res, terms):
                if rx.search(raw):
                    hit = (n, t, raw.strip()); break
            if hit:
                break
        if hit:
            break
    if hit:
        n, sig, excerpt = hit
        if len(excerpt) > 150:
            excerpt = excerpt[:150] + "..."
        FINDINGS.append("%s|%s|%s|%s" % (
            row.get("area", "?"), row.get("section", "?"), sig,
            excerpt.replace("|", "/")))
    if len(FINDINGS) >= 5:
        break

if not FINDINGS:
    print("CLEAN"); sys.exit(0)
if IS_READ:
    print("CLEAN"); sys.exit(0)

m = REF_RE.search(blob)
if m:
    print("REF\t" + m.group(1).strip().replace("\t", " "))
    sys.exit(0)
if BARE_RE.search(blob):
    print("BARE")
for f in FINDINGS:
    print("FIND\t" + f)
' 2>/dev/null || printf 'PARSEFAIL')"

case "$VERDICT" in
    ""|PARSEFAIL*)
        announce_off "REFERENCE-LEDGER GUARD: this Agent call could not be read, so it was NOT checked against the adoption ledger (ceo-decisions ${REFERENCE_LEDGER_RULING}), and a 'reference:' line could not be read from it either. This ONE dispatch is UNCHECKED."
        exit 0 ;;
    CLEAN) exit 0 ;;
esac

SESSION_ID="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("session_id","") or "")' 2>/dev/null || true)"
LOG_DIR="$ENTITY_ROOT/.claude/state"
ACK_LOG="$LOG_DIR/reference-ledger-acks.log"
NONE_LOG="$LOG_DIR/reference-none.log"

# --- THE HATCH -------------------------------------------------------------
REF_ARG=""
case "$VERDICT" in
    REF*) REF_ARG="${VERDICT#REF	}" ;;
esac

if [ -n "$REF_ARG" ]; then
    REF_WHY=""
    case "$(printf '%s' "$REF_ARG" | tr '[:upper:]' '[:lower:]')" in
        none|none[!a-z0-9]*)
            # `reference: none` — an argued absence. The reason is the whole
            # value of it, so it is measured rather than trusted.
            REF_REASON="$(printf '%s' "$REF_ARG" | sed -E 's/^[Nn][Oo][Nn][Ee][[:space:]]*[-—:–]*[[:space:]]*//')"
            REF_LEN="$(printf '%s' "$REF_REASON" | wc -c | tr -d ' ')"
            if [ "${REF_LEN:-0}" -lt 41 ]; then
                REF_WHY="'reference: none' needs a REASON — at least 40 characters saying why no ledger row covers this surface. You gave ${REF_LEN} character(s). An unargued 'none' is the assertion this gate exists to stop."
            else
                mkdir -p "$LOG_DIR" 2>/dev/null || true
                printf '%s\tsession=%s\ttool=Agent\t%s\n' \
                    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "${SESSION_ID:-<unset>}" "$REF_ARG" \
                    >>"$NONE_LOG" 2>/dev/null || true
                exit 0
            fi
            ;;
        *)
            LEDGER_NAME="$(sed -n 's/^ledger-name:[[:space:]]*//p' "$SURFACES" | sed -n 1p)"
            LEDGER_BASE="$(basename "${LEDGER_REL:-}" 2>/dev/null || true)"
            REF_NAMES_DOC=0
            if [ -n "$LEDGER_BASE" ] && printf '%s' "$REF_ARG" | grep -qF "$LEDGER_BASE"; then
                REF_NAMES_DOC=1
            fi
            if [ "$REF_NAMES_DOC" -eq 0 ] && [ -n "$LEDGER_BASE" ]; then
                if printf '%s' "$REF_ARG" | grep -qF "${LEDGER_BASE%.md}"; then REF_NAMES_DOC=1; fi
            fi
            if [ "$REF_NAMES_DOC" -eq 0 ] && [ -n "$LEDGER_NAME" ]; then
                if printf '%s' "$REF_ARG" | tr '[:upper:]' '[:lower:]' \
                   | grep -qF "$(printf '%s' "$LEDGER_NAME" | tr '[:upper:]' '[:lower:]')"; then
                    REF_NAMES_DOC=1
                fi
            fi
            REF_CITE="$(printf '%s' "$REF_ARG" | sed -nE 's/.*(§|[Ss]ection )([0-9]+(\.[0-9]+)*).*/\2/p' | sed -n 1p)"
            if [ "$REF_NAMES_DOC" -eq 0 ]; then
                REF_WHY="it does not name the reference. Say which document answered this surface — '${LEDGER_NAME:-the ledger}' or ${LEDGER_BASE:-its file name} — so the next reader can open the same page you did."
            elif [ -z "$REF_CITE" ]; then
                REF_WHY="it names the reference but no section. A document is not an answer; a row is. Cite the section: 'reference: ${LEDGER_NAME:-adoption ledger} §<N.N> — <the verdict>'."
            fi

            REF_STATE="unverified"
            if [ -z "$REF_WHY" ] && [ -n "$LEDGER_DOC" ]; then
                if grep -qE "^#{1,6}[[:space:]]+(§|[Ss]ection[[:space:]]+)?${REF_CITE}([^0-9]|$)" "$LEDGER_DOC" 2>/dev/null; then
                    REF_STATE="verified"
                else
                    REF_STATE="absent"
                    REF_WHY="§${REF_CITE} is NOT in ${LEDGER_DOC}. That is the exact failure this gate exists for: three briefs on 2026-09-19 asserted what a read said without opening it. Open the file and cite a section that is in it."
                fi
            fi

            if [ -z "$REF_WHY" ]; then
                mkdir -p "$LOG_DIR" 2>/dev/null || true
                printf '%s\tsession=%s\ttool=Agent\tcite=§%s\tsection=%s\t%s\n' \
                    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "${SESSION_ID:-<unset>}" \
                    "$REF_CITE" "$REF_STATE" "$REF_ARG" \
                    >>"$ACK_LOG" 2>/dev/null || true
                exit 0
            fi
            ;;
    esac

    {
        echo "=== THE reference: LINE WAS NOT ACCEPTED ==="
        echo "  you gave : reference: ${REF_ARG}"
        echo "  problem  : ${REF_WHY}"
        echo ""
        echo "  A BARE MARKER EXEMPTS NOTHING, and neither does a line that does not"
        echo "  resolve. One of these two shapes, on its own line in the prompt:"
        echo ""
        echo "      reference: ${LEDGER_NAME:-adoption ledger} §<N.N> — <the row's verdict>"
        echo "      reference: none — <40+ characters on why no row covers this surface>"
        echo ""
        echo "(hook: scripts/hooks/guard-reference-ledger.sh)"
    } >&2
    exit 2
fi

# --- REFUSE — and CARRY the answer, never point at it ----------------------
LEDGER_NAME="$(sed -n 's/^ledger-name:[[:space:]]*//p' "$SURFACES" | sed -n 1p)"
BARE_SEEN=0
case "$VERDICT" in
    BARE*) BARE_SEEN=1 ;;
esac
{
    echo "=== THIS DISPATCH BUILDS ON A SURFACE THE ${LEDGER_NAME:-ADOPTION LEDGER} ALREADY ANSWERED (ceo-decisions ${REFERENCE_LEDGER_RULING}) ==="
    echo "  and it cites no row. The CEO's words, 2026-09-19:"
    echo "      \"In other words, you ARE FUCKING REINVENTING THE FUCKING WHEEL FROM SCRATCH\""
    echo ""
    if [ "$BARE_SEEN" -eq 1 ]; then
        echo "  YOU WROTE A BARE 'reference:' WITH NOTHING AFTER IT. A marker is not a"
        echo "  citation and exempts nothing."
        echo ""
    fi
    echo "  WHERE, inside this prompt's own build section(s):"
    printf '%s\n' "$VERDICT" | sed -n 's/^FIND	//p' | while IFS='|' read -r _area _sec _sig _ex; do
        printf '    %-34s -> §%-5s  signal: %s\n' "$_area" "$_sec" "$_sig"
        printf '        %s\n' "$_ex"
    done
    echo ""
    echo "  THE ROW(S) TO READ FIRST — ${LEDGER_REPO:-the record}/${LEDGER_REL:-the ledger}:"
    printf '%s\n' "$VERDICT" | sed -n 's/^FIND	//p' | cut -d'|' -f2 | while read -r _sec; do
        _v="$(awk -v want="$_sec" '
            /^area:/ { a=$0 }
            /^section:/ { s=$0; sub(/^section:[ \t]*/, "", s) }
            /^verdict:/ { v=$0; sub(/^verdict:[ \t]*/, "", v); if (s == want) { print v; exit } }
        ' "$SURFACES" 2>/dev/null || true)"
        printf '    §%-6s %s\n' "$_sec" "${_v:-see the ledger}"
    done
    echo ""
    echo "  WHY THIS IS NOT NOISE: a prompt with no build section is never read for"
    echo "  signals, and a prompt that tells its teammate not to change the code — a"
    echo "  research read, a QA walk — is never refused, whatever it names. Reed's own"
    echo "  adoption read is the densest signal-carrying document in the corpus and it"
    echo "  passes. The section above is this prompt telling somebody which files to"
    echo "  edit."
    echo ""
    echo "  THE USUAL ANSWER: open the row, and put one line in the prompt —"
    echo ""
    echo "      reference: ${LEDGER_NAME:-adoption ledger} §<N.N> — <the row's verdict>"
    echo ""
    echo "  If no row really covers this surface, say so and why, in 40+ characters:"
    echo ""
    echo "      reference: none — <why no ledger row covers this surface>"
    echo ""
    echo "  Both are logged (.claude/state/reference-ledger-acks.log and"
    echo "  reference-none.log), so a habit of waiving is visible rather than invisible."
    echo "(hook: scripts/hooks/guard-reference-ledger.sh)"
} >&2
exit 2
