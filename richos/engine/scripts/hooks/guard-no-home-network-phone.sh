#!/usr/bin/env bash
#
# guard-no-home-network-phone.sh — BLOCKING PreToolUse guard on TWO surfaces:
# Write|Edit|MultiEdit|NotebookEdit (through dispatch-pretooluse.sh) and Agent.
#
# REFUSES A BRIEF, A CODE WRITE OR A SPAWN THAT ASSOCIATES THE PHONE SURFACE
# WITH A HOME-NETWORK PATH.
#
# ===========================================================================
# THE RULING, AND THE FAILURE IT WAS GIVEN FOR
# ===========================================================================
# ceo-decisions.md §61 (2026-09-18 20:40Z), in his words:
#
#     "the new working definition for a mobile app or PWA is this: it's an app
#      that lets the user use RichOS (in some way) while being on the go and
#      away from office i.e. outside the home network. Because any mobile app
#      or PWA is utterly useless within the home network."
#
# And the addendum that ordered this file, 2026-09-19 ~15:05Z:
#
#     "How many more times will any 'home network' related shit be associated
#      with a phone or built for phone?"
#
# The answer is zero, "and it is enforced rather than remembered".
#
# WHAT HAPPENED, because it decides this file's shape. §61 was ruled on the
# 18th. On the 19th three briefs kept and repaired the phone flow's home path —
# the self-signed certificate, the trust page on 8444, the sixteen certificate
# taps, the .local origin, the route chooser — and one of them asserted a
# ruling that does not exist. Two engineers and a design walk were spent on it
# before he asked the question above. Nothing was broken: the rule was in the
# record, in his own words, and nobody read it at the moment of writing.
#
# A rule enforced by remembering lasts exactly as long as the remembering. This
# is the same finding the row-currency contract and the staging gate were built
# from, and it is why this is a hook and not a paragraph.
#
# ===========================================================================
# WHAT IT REFUSES AND WHAT IT PASSES — THE REAL SENTENCES OF 2026-09-19
# ===========================================================================
# The corpus below is not illustrative. These are the actual texts, and
# guard-no-home-network-phone.test.sh drives every one of them.
#
# REFUSED — echo-brief-the-chosen-route-reaches-the-backend-2026-09-19.md
# (the brief the addendum names), two sites:
#
#     "§61 (2026-09-18): two paths, the Tailscale path first and *"..."*; the
#      At-home path stays."
#
#     "... returns the home plan when the user said at-home, even on a Mac with
#      a valid tailnet certificate ... Do not remove the At-home option, and do
#      not fix it in the UI."
#
# The first is the false premise itself. The second is the instruction that
# came out of it. Both are prescriptions with no statement anywhere on the line
# that the home path is going away.
#
# PASSED — ceo-decisions.md §61 and its addendum. It is dense with every term
# this guard looks for — "home network", "At home only", self-signed, 8444,
# sixteen, .local — and it passes, because every one of those sites sits in a
# block that also says "utterly useless within the home network", "it is being
# removed", "an engine guard refuses". THE RULING MUST BE WRITEABLE. A guard
# that forbids the record of the thing it forbids is the exact shape that cost
# this engine fourteen unexplained lines in two vendored skills on 2026-08-30:
# the guard caused the divergence, forbade the repair, and forbade the note
# explaining it.
#
# PASSED — echo-brief-remove-the-at-home-route-2026-09-19.md, the removal brief
# that was running when this was written. It names the home path in nine
# separate blocks, quotes the offending sentence of the refused brief back at
# the engineer, and passes all nine times: every block carries "is removed",
# "leave the product", "no ... option", "is not a second path", or the quoted
# form. Quoting a refused sentence is how a mistake gets recorded, and it is
# never the mistake.
#
# ===========================================================================
# THE TEST: TWO CONJUNCTS, AND A THIRD THAT LETS THE RECORD THROUGH
# ===========================================================================
#   (a) PHONE SURFACE — document-level. Does this text name the phone at all:
#       phone, mobile, PWA, pairing, pair=. Or does the file PATH put it on
#       that surface: a phone/ or web-app/ segment, or a phone*.<ext> file.
#       Cheap, broad, and deliberately so: it is a jurisdiction question, not
#       the finding. The path test is by SEGMENT and is NOT the word-level
#       pattern pointed at a path — that version made every file in the tree a
#       phone file whenever an ancestor directory had "phone" in its name, and
#       case E1 caught it by living in a temp directory named after this guard.
#
#   (b) A HOME PATH — block-level, and this is the finding. Ten STRONG terms
#       (home network, at-home, "at home <path-noun>", AtHome, home plan, a
#       <host>.local origin, 8444, trust QR, certificate profile, Remove
#       Profile) and two WEAK ones (sixteen, self-signed) that refuse alone and
#       count only in pairs. `sixteen` is a number word and `self-signed` is
#       ordinary TLS vocabulary; one of either, by itself, is a sentence about
#       something else.
#
#   (c) NOT ALREADY SAYING SO. A block that carries a removal or negation —
#       removed, removal, no longer, nothing, useless, outside the home
#       network, is not, never, no, refuses, fails, zero, leaves the product —
#       is a block talking ABOUT the home path rather than building one.
#
# THREE THINGS THE MEASUREMENT DECIDED, none of them guessed:
#
#   1. "at home" SPACED needs a path noun after it; "at-home" HYPHENATED does
#      not. Without that, `urban-brief-tailscale-how-to-screens-2026-09-19.md`
#      was refused for "the name stops resolving when Tailscale is off,
#      including at home" — a statement of a Tailscale LIMIT — and
#      `sage-brief-phone-channel-on-the-users-device-2026-09-18.md` for "at
#      home, from the couch, on Wi-Fi". Neither builds anything.
#
#   2. IN PROSE, SCOPE IS THE BLOCK, NOT THE LINE OR THE SENTENCE, and neither
#      end of that is arbitrary. Per SENTENCE, the addendum's own quoted question —
#      "...be associated with a phone or built for phone?" — is refused, and
#      the ruling cannot be written down. Per LINE, route1's two sites are
#      exempted by a "NEVER" that belongs to a different clause about Tailscale
#      identities, and the brief the ruling names passes. A block is what one
#      author wrote as one thought; hard-wrapping is a formatting choice and
#      must not change a verdict, so wrapped lines join and a heading, a list
#      item or a blank line starts a new one.
#
#      IN CODE, SCOPE IS THE LINE. Source is already line-structured, and the
#      block rule exists only to undo hard-wrapping. Joining adjacent source
#      lines is actively wrong: in the real phone.js markup the two route
#      buttons are adjacent, the first reads "Anywhere, including away from
#      home" — which is a NEGATION cue — and the second is `>At home only<`.
#      Scored as one block, the option this whole ruling is about ships under
#      the exemption of the option above it.
#
#   3. QUOTED MATTER IS SOMEBODY ELSE'S — and ONE of the four strippers is
#      proven by real text, which is stated here rather than implied.
#
#      Bare "..." IS. Stripped in PROSE ONLY, and removing it turns three
#      cases red: the whole removal brief is refused, and so is a sentence
#      quoting the refused sentence. In CODE it is deliberately NOT stripped,
#      because there a double-quoted span is a string literal and is exactly
#      the copy at issue — `route = "at-home"` must stay reachable.
#
#      The CURLY pair, the record's *"..."* emphasis-quote idiom and inline
#      code spans are DEFENSE IN DEPTH. Disabling any of them changes no
#      verdict on any of the 269 documents or on any real fixture, because the
#      provenance apparatus that emits curly quotes also annotates every one
#      with "nothing in scope sources it" or "this run is NOT in that file" —
#      a negation, in the same block. They are kept because the *"..."* form is
#      how this record quotes the CEO and a future ruling need not come
#      wrapped in a negation, and their cases in the suite are marked
#      CONSTRUCTED so nobody later mistakes them for measured ones.
#
#      An earlier draft of this header said curly-stripping fixed the one
#      false positive of the first measured pass. It did not; BLOCK SCOPE did.
#      The claim was plausible, was written from memory of the fix rather than
#      from a measurement of it, and survived until a mutant could not kill it.
#
# ===========================================================================
# MEASURED, ON REAL TEXT, BEFORE IT WAS WIRED
# ===========================================================================
# Every brief in richos-hq/docs/briefs (248 .md files) plus the 20 spawn
# prompts of the session that ordered this, plus §61 itself: 269 documents
# driven through this file as real Write and Agent payloads. SEVEN refused.
#
#   echo-brief-the-chosen-route-reaches-the-backend-2026-09-19.md  + its prompt
#   ray-brief-candidate-14-mac-and-android-walk-2026-09-19.md      + its prompt
#       "walk candidate .14 — the At-home route now serves the home plan"
#   urban-brief-phone-flow-signoff-rewalk-2026-09-19.md            + its prompt
#       "your call whether the At-home route gets it too"
#   sage-brief-phone-reach-from-anywhere-bundled-2026-09-18.md
#       "Keep the home-network design of your revised plan as the floor"
#
# Six of the seven are the three home-path briefs of 2026-09-19 and their spawn
# prompts — the engineers, the walk, and the signoff re-walk that §61's
# addendum is about. The seventh is stated rather than glossed: it was written
# on 2026-09-18, BEFORE §61 was ruled at 20:40Z, when keeping the home-network
# design was the correct instruction. The rule is right and the document
# predates it.
#
# NOTHING ELSE FIRED. Not one of the other 262, and no non-phone document can
# ever reach conjunct (b). Two of those 262 were written by the live session
# WHILE this guard was being measured, and both passed.
#
# Reproduce it before changing any pattern here: build a Write or Agent payload
# per document and pipe it in. The classifier is this file's own python3 block,
# so the measurement is of the shipped rule and not of a model of it — which is
# the difference between this number and a plausible one.
#
# ===========================================================================
# THE ESCAPE HATCH IS A QUOTATION, NOT A MARKER
# ===========================================================================
#     ceo-ruled-home-network: §<N> — "<his verbatim sentence>"
#
# on its own line in the prompt or in the new content. It needs three things
# and a bare marker exempts nothing:
#
#   * a section token (§61, §61.1, "section 61"),
#   * a quoted sentence of at least 25 characters,
#   * and THE SECTION MUST EXIST in the CEO's record. The guard opens
#     wiki/ceo-decisions.md — located through cr_resolve(), the engine's single
#     declaration of where his rulings live — and looks for the heading. A
#     citation of a section that is not there is refused BY NAME, because that
#     is precisely what happened: a brief asserted "§61 keeps both" and §61
#     says the opposite. Half the value of this hatch is being made to open the
#     file.
#
# Where the record cannot be located at all the citation is accepted on its
# shape and the log says UNVERIFIED. "I cannot check" is never "forbidden".
# Accepted uses append to <entity root>/.claude/state/home-network-acks.log,
# beside stale-staging-acks.log and the other opt-out ledgers, so a habit of
# waiving is visible rather than invisible.
#
# WHY A QUOTATION AND NOT A REASON, unlike every sibling hatch in this engine.
# The failure was not a thin justification. It was a CONFIDENT one — "§61 keeps
# both" — invented in good faith by somebody who had not reopened §61. A reason
# field would have been filled in just as confidently. A citation that must
# resolve against the file cannot be.
#
# ===========================================================================
# THE SELF-EXEMPTION, DECLARED RATHER THAN HIDDEN
# ===========================================================================
# This file, its suite, its mutation harness and its corpus are exempt BY
# BASENAME. Everything above quotes the sentences the guard refuses, so without
# this the guard could not be authored, tested or corrected — and a guard that
# cannot be repaired is removed. It is four names, it is checked against the
# BASENAME and never a directory, and the list is here rather than in a config
# key so that adding to it is an edit somebody reviews.
#
# NOTE THAT ceo-decisions.md IS NOT ON THAT LIST, deliberately. The record
# passes on merit — proven, by driving §61 and its addendum through this guard
# in the suite — which is a much stronger claim than passing by path.
#
# ===========================================================================
# FAIL OPEN, AND LOUDLY
# ===========================================================================
#   NOT ADOPTED (no orchestration.config)     -> STAND DOWN, silent.
#   HOME_NETWORK_PHONE_GUARD="off"            -> STAND DOWN, silent. A
#     declaration is a choice somebody made and recorded; nagging about it is
#     how a switch gets set and forgotten.
#   NO PHONE SIGNAL / NO HOME PATH            -> SILENT. Always, and this is
#     262 of the 269 measured documents.
#   PAYLOAD UNPARSEABLE, python3 MISSING      -> ALLOW + ANNOUNCE. This guard
#     cannot read the content OR the hatch from such a call, so refusing would
#     block a write the operator has no way to permit.
#   THE CEO RECORD UNREADABLE                 -> the hatch is accepted on its
#     shape and logged UNVERIFIED. It NEVER turns a pass into a refusal.
#
# The one thing that is NOT fail-open: scripts/lib/resolve-roots.sh missing.
# That is the shared bootstrap's contract and probe Layer R asserts every
# rooted hook carries it byte-identically.
#
# ===========================================================================
# WHY BOTH SURFACES, AND WHY NOT A THIRD
# ===========================================================================
# The ruling names three things — "any brief, code write or spawn". A brief is
# a Write, code is a Write, a dispatch is an Agent call, so two registrations
# cover all three. PreToolUse[Bash] was REJECTED: a brief reaches disk through
# Write long before any `git commit` names it, and a Bash rule would have to
# match every shape of "write a file with a shell", which is an open class.
# A Stop-hook notice was REJECTED for the reason every notice in this engine
# gets rejected when the act is irreversible: it reports a brief that has
# already been dispatched from.
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
  hook: scripts/hooks/guard-no-home-network-phone.sh
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
    announce_off "HOME-NETWORK/PHONE GUARD IS OFF: python3 is not on PATH, so this call was NOT checked for a phone surface associated with a home-network path (ceo-decisions §61)."
    exit 0
fi

if ! resolve_entity_root "$INPUT"; then
    if [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
        exit 0
    fi
    announce_off "HOME-NETWORK/PHONE GUARD IS OFF: it cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether this call associates the phone with a home-network path."
    exit 0
fi
ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
_SJ_LIB="$SCRIPT_DIR/../lib/seat-jurisdiction.sh"
if [ ! -f "$_SJ_LIB" ]; then
    announce_off "BROKEN INSTALL: seat-jurisdiction.sh is missing; the target repository cannot be checked."
    exit 0
fi
. "$_SJ_LIB"

# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this guard takes the SAME silent exit 0 that a
# well-formed payload for a DIFFERENT tool takes. This separates the two. NO
# VERDICT CHANGES — only the silence. See scripts/lib/unevaluated-notice.sh.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-no-home-network-phone.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this call associates the phone surface with a home-network path"
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"

# --- THE OFF SWITCH, AND WHY THERE IS NO ON SWITCH -------------------------
# Unset means ON. Every sibling gate in this engine is adopted by declaring a
# key, and this one is not, for a reason that is the ruling's own: the answer
# is ZERO, enforced rather than remembered, and a contract nobody has switched
# on yet is a contract remembered. The conjunction is what makes that safe to
# ship — 262 of 269 measured documents never reach the second test, and no text
# without a phone signal can reach it at all. An adopter who genuinely has a
# home-network product says so once, here, and is never spoken to again.
: "${HOME_NETWORK_PHONE_GUARD:=on}"
case "$HOME_NETWORK_PHONE_GUARD" in
    off|OFF|0|no|NO) exit 0 ;;
esac
# The section the refusal cites. Data, not prose, so a repository whose ruling
# is numbered differently cites its own.
: "${HOME_NETWORK_PHONE_RULING:=§61}"

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
case "$TOOL_NAME" in
    Write|Edit|MultiEdit|NotebookEdit|Agent) ;;
    *) exit 0 ;;
esac

# --- THE SELF-EXEMPTION ----------------------------------------------------
# Declared in the header. BASENAME only — never a directory, so it cannot be
# widened by moving a file into a folder.
FILE_PATH="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); ti=d.get("tool_input",{}) or {}; print(ti.get("file_path") or ti.get("notebook_path") or "")' 2>/dev/null || true)"
if [ -n "$FILE_PATH" ]; then
    richos_assert_jurisdiction "guard-no-home-network-phone.sh" "$ENTITY_ROOT" "$FILE_PATH" "file" "proceeds" || true
fi
case "$(basename "${FILE_PATH:-}" 2>/dev/null || true)" in
    guard-no-home-network-phone.sh|guard-no-home-network-phone.test.sh|home-network-phone.mutation.sh|home-network-phone.corpus.md)
        exit 0 ;;
esac

# --- THE CLASSIFIER --------------------------------------------------------
# Prints one of:
#   CLEAN
#   ACK<TAB><the ceo-ruled-home-network: line's argument>
#   FIND<TAB><n>|<term>|<excerpt>   (one per line, at most 5)
# then, for FIND runs, a trailing SURFACE<TAB>prose|code line.
#
# `surface` decides ONE thing: whether a bare "..." span is attribution (prose)
# or a string literal (code). Nothing else in the classifier looks at it.
VERDICT="$(printf '%s' "$INPUT" | FILE_PATH="$FILE_PATH" python3 -c '
import json, os, re, sys

try:
    d = json.load(sys.stdin)
except Exception:
    print("PARSEFAIL"); sys.exit(0)

ti = d.get("tool_input") or {}
if not isinstance(ti, dict):
    print("PARSEFAIL"); sys.exit(0)

tool = str(d.get("tool_name", "") or "")
path = os.environ.get("FILE_PATH", "") or ""

texts = []
if tool == "Agent":
    texts.append(str(ti.get("prompt", "") or ""))
    texts.append(str(ti.get("description", "") or ""))
elif tool == "Write":
    texts.append(str(ti.get("content", "") or ""))
elif tool == "Edit":
    texts.append(str(ti.get("new_string", "") or ""))
elif tool == "MultiEdit":
    for e in (ti.get("edits") or []):
        if isinstance(e, dict):
            texts.append(str(e.get("new_string", "") or ""))
elif tool == "NotebookEdit":
    texts.append(str(ti.get("new_source", "") or ""))
blob = "\n".join(t for t in texts if isinstance(t, str))

if not blob.strip():
    print("CLEAN"); sys.exit(0)

# A spawn prompt is prose. A write is prose by EXTENSION, and code otherwise:
# the distinction decides only whether a bare double-quoted span is somebody
# being quoted or a string literal, and in a .js file it is the button copy.
PROSE_EXT = {".md", ".markdown", ".txt", ".text", ".rst", ".org", ".adoc", ".mdx"}
if tool == "Agent":
    prose = True
else:
    base = os.path.basename(path)
    ext = os.path.splitext(base)[1].lower()
    prose = (ext in PROSE_EXT) or (ext == "" and base != "")

# ---- (a) THE PHONE SURFACE, document level --------------------------------
# TWO PATTERNS, AND THE SECOND IS NOT THE FIRST APPLIED TO A PATH. A word-level
# test run over an absolute path makes every file in the tree a phone file the
# moment any ANCESTOR DIRECTORY has "phone" in its name — caught by case E1,
# whose sandbox lives in a temp directory named after this very guard, and
# which was refused for a brief written about a browser. The path establishes
# the SURFACE and nothing else, so it is matched on path SEGMENTS.
PHONE_RE = re.compile(
    r"\b(?:phones?|mobile|PWA)\b|\bpair(?:ing|ed)\b|\bpair=",
    re.IGNORECASE)
PHONE_PATH_RE = re.compile(
    r"(?:^|/)phone/|(?:^|/)web-app/|(?:^|/)phone[a-z0-9_-]*\.[a-z]+$",
    re.IGNORECASE)
if not (PHONE_RE.search(blob) or PHONE_PATH_RE.search(path)):
    print("CLEAN"); sys.exit(0)

# ---- the hatch ------------------------------------------------------------
ACK_RE = re.compile(r"^[ \t]*ceo-ruled-home-network:[ \t]*(\S.*)$", re.MULTILINE)
m = ACK_RE.search(blob)
if m:
    print("ACK\t" + m.group(1).strip().replace("\t", " "))
    sys.exit(0)

# ---- (b) A HOME PATH, block level -----------------------------------------
PATH_NOUN = r"(?:only|route|path|option|plan|flow|screens?|origin|design|mode|server|listener)"
STRONG = [
    r"\bhome[- ]network\b",
    r"\bat-home\b",
    r"\bat home " + PATH_NOUN + r"\b",
    r"\bAtHome\b",
    r"\bhome plan\b",
    # A .local HOST NAME (mDNS), not a settings file. Two file-name shapes are
    # not hosts: "settings.local.json", where more file-name dots follow, and
    # ".env.local", a DOTFILE: a name that begins with a dot is never a host
    # name (hunt part 3, finding 6, still partial in the v2 re-check, because
    # the old pattern began matching at the "v" of ".env"). So the name must
    # start at a boundary with a letter, digit, "_" or a "<placeholder>",
    # and may carry more labels: "host.local", "my-mac.local:8443",
    # "<name>.local", "_richos._tcp.local" and "host.local." at a sentence end
    # are hosts.
    r"(?<![\w.-])(?:[\w-]+\.)*(?:[\w-]*\w|<[^<>\s]+>)\.local(?::\d+)?\b(?!\.[A-Za-z0-9])",
    # Port 8444 as a PORT: ":8444", "port 8444" or "a listener on 8444", never
    # a bare number such as a width of 8444 pixels.
    r"(?::|\bports?\s+|\bon\s+)8444\b(?!\s*(?:px|pixels?)\b)",
    r"\btrust QR\b",
    r"\bcertificate profile\b",
    r"\bRemove Profile\b",
]
WEAK = [r"\bsixteen\b", r"\bself-signed\b"]
STRONG_RE = re.compile("|".join("(?:%s)" % p for p in STRONG), re.IGNORECASE)
WEAK_RE = re.compile("|".join("(?:%s)" % p for p in WEAK), re.IGNORECASE)

# ---- (c) already saying it is going ---------------------------------------
NEG_RE = re.compile("|".join([
    r"\bremoved\b", r"\bremoval\b", r"\bremoving\b", r"\bremoves\b", r"\bremove\b",
    r"\bdeletes?\b", r"\bdeleted\b", r"\bdropped\b", r"\bno longer\b",
    r"\bnothing\b", r"\bnone\b", r"\bnowhere\b", r"\bgone\b",
    r"\buseless\b", r"\boutside the home network\b", r"\baway from (?:the )?home\b",
    r"\bis not\b", r"\bare not\b", r"\bwas not\b", r"\bwere not\b",
    r"\bdoes not\b", r"\bdid not\b", r"\bnever\b", r"\bno\b", r"\bnot\b",
    r"\brefus(?:e|es|ed|al)\b", r"\bfails?\b", r"\bfailed\b", r"\bblocked?\b",
    r"\bbanned\b", r"\bforbid(?:s|den)?\b", r"\bzero\b",
    r"\bleaves? the product\b", r"\bstale\b", r"\bobsolete\b", r"\bsuperseded\b",
]), re.IGNORECASE)

# RETENTION vetoes (c). "Do not remove the At-home option" carries a removal
# word and is an instruction to KEEP — it is the second of the two sentences
# the ruling names, and without this veto it exempts itself.
RETAIN_RE = re.compile(
    r"\b(?:do not|don.t|never|must not|cannot|can.t|won.t|will not)\s+"
    r"(?:remove|delete|drop|touch|change|fix)\b"
    r"|\bkeeps? both\b",
    re.IGNORECASE)

# ---- quoted matter --------------------------------------------------------
SPANS = [
    re.compile(r"“.*?”", re.DOTALL),
    re.compile(r"\*+\".*?\"\*+", re.DOTALL),
    re.compile(r"`[^`\n]*`"),
]
if prose:
    SPANS.append(re.compile(r"\"[^\"\n]*\""))

def blank(mo):
    return re.sub(r"[^\n]", " ", mo.group(0))

stripped = blob
for rx in SPANS:
    stripped = rx.sub(blank, stripped)

FENCE_RE = re.compile(r"^\s*(?:```|~~~)")
NEW_BLOCK_RE = re.compile(r"^\s*(?:#{1,6}\s|[-*+]\s|\d+[.)]\s|\|)")

raws = blob.split("\n")
strs = stripped.split("\n")
out, cur, start, in_fence = [], [], 0, False

def flush():
    if cur:
        out.append((start, " ".join(cur), raws[start - 1]))

for n, raw in enumerate(raws, 1):
    if FENCE_RE.match(raw):
        flush(); del cur[:]
        in_fence = not in_fence
        continue
    if in_fence:
        continue
    if not raw.strip() or (prose and raw.lstrip().startswith(">")):
        flush(); del cur[:]
        continue
    if cur and (not prose or NEW_BLOCK_RE.match(raw)):
        # CODE IS ALREADY LINE-STRUCTURED, so a code surface is scored per
        # LINE. The block rule exists to undo HARD-WRAPPING, which is a prose
        # phenomenon: joining adjacent source lines would let one line say
        # "away from home" and the next ship `>At home only<` under its
        # exemption — measured on the real phone.js markup, where those two
        # buttons are adjacent.
        flush(); del cur[:]
    if not cur:
        start = n
    cur.append(strs[n - 1])
flush()

FINDINGS = []
for n, body, raw in out:
    hit = STRONG_RE.search(body)
    if hit is None:
        ms = list(WEAK_RE.finditer(body))
        if len(set(x.group(0).lower() for x in ms)) >= 2:
            hit = ms[0]
    if hit is None:
        continue
    if NEG_RE.search(body) and not RETAIN_RE.search(body):
        continue
    excerpt = raw.strip()
    if len(excerpt) > 150:
        excerpt = excerpt[:150] + "..."
    FINDINGS.append("%d|%s|%s" % (n, hit.group(0).strip(), excerpt.replace("|", "/")))
    if len(FINDINGS) >= 5:
        break

if not FINDINGS:
    print("CLEAN"); sys.exit(0)
for f in FINDINGS:
    print("FIND\t" + f)
print("SURFACE\t" + ("prose" if prose else "code"))
' 2>/dev/null || printf 'PARSEFAIL')"

case "$VERDICT" in
    ""|PARSEFAIL*)
        announce_off "HOME-NETWORK/PHONE GUARD: this ${TOOL_NAME} call could not be read, so it was NOT checked against ceo-decisions ${HOME_NETWORK_PHONE_RULING}, and a 'ceo-ruled-home-network:' citation could not be read from it either. This ONE call is UNCHECKED."
        exit 0 ;;
    CLEAN) exit 0 ;;
esac

SESSION_ID="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("session_id","") or "")' 2>/dev/null || true)"
LOG_DIR="$ENTITY_ROOT/.claude/state"
LOG="$LOG_DIR/home-network-acks.log"

# --- THE HATCH -------------------------------------------------------------
# Checked BEFORE the findings are reported, and its failures are reported with
# them: an operator who cited the wrong section must hear WHICH part of the
# citation failed, not just that it did not work.
ACK_ARG=""
case "$VERDICT" in
    ACK*) ACK_ARG="${VERDICT#ACK	}" ;;
esac

if [ -n "$ACK_ARG" ]; then
    ACK_WHY=""
    ACK_CITE="$(printf '%s' "$ACK_ARG" | sed -nE 's/.*(§|[Ss]ection )([0-9]+(\.[0-9]+)*).*/\2/p' | sed -n 1p)"
    ACK_QUOTE="$(printf '%s' "$ACK_ARG" | sed -nE 's/.*["“]([^"”]{25,})["”].*/\1/p' | sed -n 1p)"
    if [ -z "$ACK_CITE" ]; then
        ACK_WHY="it names no section. Cite the ruling that reinstates the home path: 'ceo-ruled-home-network: §<N> — \"<his sentence>\"'."
    elif [ -z "$ACK_QUOTE" ]; then
        ACK_WHY="it names a section but quotes nothing from it. A citation without his words is an assertion about the record; at least 25 characters in quotes, verbatim."
    fi

    # DOES THE SECTION EXIST? Located through the engine's single declaration
    # of where the CEO's record lives. Unreachable record -> UNVERIFIED, never
    # a refusal: "I cannot check" is not "forbidden".
    ACK_STATE="unverified"
    ACK_SOURCE=""
    if [ -z "$ACK_WHY" ]; then
        _CR_LIB="$SCRIPT_DIR/../lib/ceo-ruled.sh"
        if [ -f "$_CR_LIB" ]; then
            # shellcheck source=../lib/ceo-ruled.sh
            . "$_CR_LIB"
            if cr_resolve "$ENTITY_ROOT" >/dev/null 2>&1; then
                while IFS="$(printf '\t')" read -r _cr_path _cr_label; do
                    [ -n "$_cr_path" ] || continue
                    case "$_cr_label" in ceo-decisions.md) ;; *) continue ;; esac
                    ACK_SOURCE="$_cr_path"
                    if grep -qE "^#{1,6}[[:space:]]+(§|[Ss]ection[[:space:]]+)?${ACK_CITE}([^0-9]|$)" "$_cr_path" 2>/dev/null; then
                        ACK_STATE="verified"
                        # P3-31 (v3): the quotation must be IN that section's
                        # body, not merely next to a real section number. The
                        # same check guard-host-display-power.sh makes.
                        if ! python3 - "$_cr_path" "$ACK_CITE" "$ACK_QUOTE" <<'PYQ' 2>/dev/null
import re, sys
path, cite, quote = sys.argv[1:4]
norm = lambda t: re.sub(r"\s+", " ", t.replace("“", '"').replace("”", '"').replace("’", "'")).strip()
head = re.compile(r"^#{1,6}\s+(?:§|[Ss]ection\s+)?" + re.escape(cite) + r"(?:[^0-9]|$)")
body, on = [], False
for line in open(path, encoding="utf-8", errors="replace"):
    if head.match(line):
        on = True
        continue
    if on and re.match(r"^#{1,6}\s", line):
        break
    if on:
        body.append(line)
sys.exit(0 if norm(quote) in norm("".join(body)) else 1)
PYQ
                        then
                            ACK_STATE="quote-absent"
                        fi
                        break
                    fi
                    ACK_STATE="absent"
                done <<CR_EOF
$CR_SOURCES
CR_EOF
            fi
        fi
        if [ "$ACK_STATE" = "quote-absent" ]; then
            ACK_WHY="the quotation is NOT in the text of §${ACK_CITE} in ${ACK_SOURCE}. Quote his words from that section, verbatim."
        fi
        if [ "$ACK_STATE" = "absent" ]; then
            ACK_WHY="§${ACK_CITE} is NOT in ${ACK_SOURCE}. That is the exact failure this gate exists for: on 2026-09-19 a brief asserted '§61 keeps both' and §61 rules the opposite. Open the file and cite a section that is in it."
        fi
    fi

    if [ -z "$ACK_WHY" ]; then
        mkdir -p "$LOG_DIR" 2>/dev/null || true
        {
            printf '%s\tsession=%s\ttool=%s\tfile=%s\tcite=§%s\tsection=%s\t%s\n' \
                "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
                "${SESSION_ID:-<unset>}" "${TOOL_NAME:-<unset>}" \
                "${FILE_PATH:-<none>}" "$ACK_CITE" "$ACK_STATE" "$ACK_ARG"
        } >>"$LOG" 2>/dev/null || true
        exit 0
    fi

    {
        echo "=== THE HOME-NETWORK CITATION WAS NOT ACCEPTED ==="
        echo "  call     : ${TOOL_NAME}${FILE_PATH:+ -> $FILE_PATH}"
        echo "  you gave : ceo-ruled-home-network: ${ACK_ARG}"
        echo "  problem  : ${ACK_WHY}"
        echo ""
        echo "  A BARE MARKER EXEMPTS NOTHING, and neither does a citation that does"
        echo "  not resolve. The line needs a section that EXISTS in the CEO's record"
        echo "  and at least 25 characters of his own words in quotes:"
        echo ""
        echo "      ceo-ruled-home-network: §<N> — \"<his verbatim sentence>\""
        echo ""
        echo "(hook: scripts/hooks/guard-no-home-network-phone.sh)"
    } >&2
    exit 2
fi

# --- REFUSE — and CARRY the rule, never point at it ------------------------
SURFACE="$(printf '%s' "$VERDICT" | sed -n 's/^SURFACE	//p' | sed -n 1p)"
{
    echo "=== A PHONE SURFACE IS BEING ASSOCIATED WITH A HOME-NETWORK PATH (ceo-decisions ${HOME_NETWORK_PHONE_RULING}) ==="
    echo "  call : ${TOOL_NAME}${FILE_PATH:+ -> $FILE_PATH}${SURFACE:+   (read as ${SURFACE})}"
    echo ""
    echo "  WHERE, in the content this call would introduce:"
    printf '%s\n' "$VERDICT" | sed -n 's/^FIND	//p' | while IFS='|' read -r _n _term _ex; do
        printf '    line %-5s %-18s %s\n' "$_n" "$_term" "$_ex"
    done
    echo ""
    echo "  THE RULING, in his words (${HOME_NETWORK_PHONE_RULING}, 2026-09-18):"
    echo "      \"the new working definition for a mobile app or PWA is this: it's an"
    echo "       app that lets the user use RichOS (in some way) while being on the go"
    echo "       and away from office i.e. outside the home network. Because any mobile"
    echo "       app or PWA is utterly useless within the home network.\""
    echo ""
    echo "  AND THE ADDENDUM THAT PUT THIS GUARD HERE (2026-09-19):"
    echo "      \"How many more times will any 'home network' related shit be associated"
    echo "       with a phone or built for phone?\""
    echo "  The answer is zero. There is ONE phone path (Tailscale) and one later"
    echo "  (the paid relay); nothing else."
    echo ""
    echo "  WHY THIS IS NOT NOISE: a text that says the home path is going — removed,"
    echo "  no longer, useless, is not, leaves the product — is NEVER refused here, and"
    echo "  neither is a QUOTATION of one that keeps it. §61 itself, and the brief that"
    echo "  removed the At-home route, both pass. Measured over 269 real briefs and"
    echo "  spawn prompts, seven were refused and every one of them kept a home path."
    echo "  The sites above carry no such statement."
    echo ""
    echo "  THE USUAL ANSWER: say what is happening to the home path, or delete the"
    echo "  sentence. The phone reaches this Mac over Tailscale, from anywhere."
    echo ""
    echo "  OR CITE A RULING THAT REINSTATES IT — one line, and the section must be"
    echo "  in the record with his words quoted from it:"
    echo ""
    echo "      ceo-ruled-home-network: §<N> — \"<his verbatim sentence>\""
    echo ""
    echo "  Accepted citations are appended to .claude/state/home-network-acks.log,"
    echo "  so a habit of waiving is visible rather than invisible."
    echo "(hook: scripts/hooks/guard-no-home-network-phone.sh)"
} >&2
exit 2
