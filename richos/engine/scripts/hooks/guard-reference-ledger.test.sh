#!/usr/bin/env bash
#
# guard-reference-ledger.test.sh — regression tests for
# scripts/hooks/guard-reference-ledger.sh.
#
# THE FIXTURES ARE THE REAL TEXTS OF 2026-09-19, not paraphrases of them. The
# guard exists because of four specific documents — two that must be refused
# and two that must pass — and a synthetic fixture cannot tell you which side
# of that line a change lands on. Every quoted block below is copied from the
# file named above it.
#
# Covered here:
#
#   (a) REFUSES the two phone briefs the CEO's §66 is about, as committed:
#       echo-brief-phone-delivery-30s-paired-wording-keychain-2026-09-19.md,
#       whose Scope names richos/web/web-app/lib/link.js, and
#       echo-brief-phone-to-mac-same-paint-and-pairing-sheet-fold-2026-09-19.md,
#       whose Scope names app/ui/phone.js and whose text carries the transport
#       word that makes a weak path count;
#   (b) PASSES both of them the moment a live `reference:` line is added;
#   (c) PASSES ray-brief-phone-path-in-the-vm-nightly-7-2026-09-19.md — a QA
#       walk over the same surface, which reads it rather than building on it;
#   (d) PASSES reed-brief-t3code-tooling-adoption-read-2026-09-19.md — the
#       research read that COMMISSIONED the ledger, and the densest
#       signal-carrying document in the whole corpus;
#   (e) THE HATCH IS A CITATION OR AN ARGUED ABSENCE: a bare `reference:`
#       exempts nothing; a line naming no section, or no ledger, or a section
#       that is NOT in the ledger, each exempt nothing and are refused BY NAME;
#       a `reference: none — <40+ characters>` passes and is LOGGED to
#       .claude/state/reference-none.log; an accepted citation is logged to
#       reference-ledger-acks.log with section=verified or section=unverified;
#   (f) THE WEAK-PATH RULE, both directions: a brief whose Scope names
#       app/ui/phone.js and whose text carries NO transport word passes, and
#       the same text with one carries a refusal. This is the narrowing that
#       took the measured fires from 32 to 11;
#   (g) THE READ VETO IS QUANTIFIED, both directions: "you do not touch the
#       host keychain" does NOT exempt a build brief, and "do not modify
#       anything under richos/app" does;
#   (h) FAIL OPEN: an unparseable payload, a call for another tool, a
#       repository that never adopted the engine, a declared off switch, and a
#       missing surfaces file all allow — the first and last loudly;
#   (i) the one hard refusal: a missing scripts/lib/resolve-roots.sh prints the
#       shared BROKEN INSTALL banner and exits 2.
#
# ===========================================================================
# THE MEASUREMENT, REPRODUCIBLE
# ===========================================================================
# Before this guard was wired, every brief in richos-hq/docs/briefs and every
# spawn prompt in the ordering session's scratchpad — 302 documents — were
# driven through THIS FILE'S SUBJECT as real Agent payloads. ELEVEN fired.
#
#   echo-brief-phone-delivery-30s-paired-wording-keychain-2026-09-19.md  §2.6
#   echo-brief-phone-to-mac-same-paint-and-pairing-sheet-fold-2026-09-19.md §2.6
#   echo-brief-the-chosen-route-reaches-the-backend-2026-09-19.md        §2.6
#   echo-brief-nightly-gate-env-isolation-2026-09-17.md                  §2.1
#   zach-brief-candidate-build-time-2026-09-19.md                        §2.1
#   zach-brief-guard-reference-ledger-2026-09-19.md            §2.1 §2.4 §2.6
#   spawn-pl2.prompt.md, spawn-pl3.prompt.md, spawn-echo-opus-route1.prompt.md,
#   spawn-zb.prompt.md, spawn-rg.prompt.md      (the spawn prompts of the above)
#
# NINE of the eleven are build dispatches onto a ledger surface — the two phone
# briefs, the route brief that changes the phone backend, and two briefs that
# change what the nightly's signing and notarization steps receive. The other
# TWO are this guard's own brief and its spawn prompt, which enumerate the
# signal words inside a `## What to build` section; they carry the argued
# `reference: none` that brief itself prescribes, and that is the designed
# answer rather than a miss.
#
# THE NARROWINGS THAT PRODUCED THAT NUMBER, each measured and each reversible
# by a mutant in reference-ledger.mutation.sh:
#
#   32 -> 11   weak paths. `ui/phone.js` as a STRONG path fired on 13 phone
#              briefs of 2026-09-19; twelve were about button focus, scroll
#              position and splash placement, and carried no transport word
#              anywhere. Exactly one did, and it is the one §66 is about.
#   a refusal recovered   the read veto needs a universal quantifier. A bare
#              "do not touch" passed the delivery brief, which says "you do not
#              touch the host keychain" about one file while commissioning
#              changes to six others.
#   a refusal recovered   a bare `read-only` is not a read. The same brief says
#              "(Rich read 6 attribute lines for that service, read-only;
#              nothing deleted)".
#
# Reproduce before changing any pattern: build an Agent payload per document
# and pipe it into the guard. The classifier is the guard's own python3 block,
# so the measurement is of the shipped rule and not of a model of it.
#
# Run directly: scripts/hooks/guard-reference-ledger.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOK="$SCRIPT_DIR/guard-reference-ledger.sh"

# DECLARED, never inherited from the launching session: run from a session
# seated elsewhere, every case below would pass by standing down.
unset CLAUDE_PROJECT_DIR

PASS=0
FAIL=0

SB="$(cd "$(mktemp -d -t guard-reference-ledger.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SB"' EXIT

ENTITY="$SB/entity"
HQ="$SB/richos-hq"
mkdir -p "$ENTITY/.claude/state" "$HQ/docs/research"
printf 'PROTECTED_PATHS=""\n' >"$ENTITY/orchestration.config"

# The ledger the citations must resolve against. §2.6 is here; §9.9
# deliberately is not, and case E5 cites §9.9.
LEDGER="$HQ/docs/research/t3code-tooling-what-to-adopt-2026-09-19.md"
{
    printf '# T3 Code tooling — what RichOS can adopt instead of build\n\n'
    printf '## 2. Per-area verdicts\n\n'
    printf '### 2.1 Build, sign, notarize, publish\n\nBody.\n\n'
    printf '### 2.5 Developer loop\n\nBody.\n\n'
    printf '### 2.6 Crash and reconnect\n\nBody.\n\n'
    printf '## 3. The honest answer\n\n'
    printf '### 3.2 What prevents most bugs\n\nBody.\n'
} >"$LEDGER"

# ===========================================================================
# THE FIXTURES — real text, from the files named
# ===========================================================================

# richos-hq/docs/briefs/echo-brief-phone-delivery-30s-paired-wording-keychain-2026-09-19.md
# Title, the item that names the transport, the two sentences that defeated an
# unquantified read veto, and the Scope section verbatim.
read -r -d '' DELIVERY <<'FIX' || true
# Brief: a phone message takes 30–55 s to reach the Mac and one in four never arrives; the Mac says "It is paired" before the person confirms the six words

1. **BLOCKING — delivery.** Four phone sends; send → Mac intake (from the Mac's own intake log): 30.67 s, 54.47 s, 48.79 s, and one **never arrived**. Every delay is ≥ 30 s with a backoff's spread; `richos/web/web-app/lib/link.js:66` `MAX_RETRY_MS = 30000`, `:83` `opts.maxRetryMs`.
4. **MEDIUM (harness) — the CEO's real keychain.** Candidates .14–.16 ran on the host with those homes (Rich read 6 attribute lines for that service, read-only; nothing deleted). Make the service name derive from the app-data directory so a scratch `HOME` can never collide with the real app's item; Rich decides whether to remove them; you do not touch the host keychain.

## Scope

`richos/web/web-app/lib/link.js` and the phone page's send path, `richos/app/src-tauri/src/phone/{listen.rs,secrets.rs,ca.rs,mod.rs}`, `richos/app/ui/phone.js` (+ `style.css` if wording needs it), tests: `second-mouth.js`, `phone.js`, `settings-fit.js`.

## Completion criterion

- Item 1's cause named (file:function), red/green in the harness.
FIX

# richos-hq/docs/briefs/echo-brief-phone-to-mac-same-paint-and-pairing-sheet-fold-2026-09-19.md
# The Scope section names app/ui/phone.js — a WEAK path — and item 1 carries
# the transport word "intake" that makes it count.
read -r -d '' SAMEPAINT <<'FIX' || true
# Brief: a phone-typed message still reaches the Mac's open thread in the same paint as its reply (7.3 s)

1. **R3, phone → Mac, STILL THERE and worse than .13 (5.68 s): 7,288 ms.** Find where the thread view drops or defers the intake-borne `CeoMessage` — the open-thread render path, a "same thread already open" guard, a batch-until-turn-settles rule, or the event never carrying the open thread's id — and fix it at the cause.

## Scope and files

`app/ui/phone.js`, `app/ui/phone.css` (or wherever the pairing sheet's styles live — find them), the thread-render path in `app/ui/main.js` OUTSIDE offer2's region, `app/crates/richos-core/src/spine.rs` / `live.rs` only if the event is the cause, `app/ui/tests/settings-fit.js`.

## Completion criterion

- Each of the three reproduced red on `f247c535` in the harness, green on your branch.
FIX

# THE SAME BRIEF WITH ITS TRANSPORT WORD REMOVED — the twelve phone briefs of
# 2026-09-19 that name app/ui/phone.js and are about focus, scroll and splash
# placement all look like this, and all twelve must pass.
read -r -d '' PHONEUI <<'FIX' || true
# Brief: the composer's buttons sit off the baseline and the Settings button loses focus on Escape

1. **The composer's two buttons are 3 px below the input's baseline at every window width.** Fix it in the stylesheet, not with a margin on one button.

## Scope and files

`app/ui/phone.js`, `app/ui/phone.css`, `app/ui/style.css`, `app/ui/tests/settings-fit.js`, `app/ui/tests/lib/state-registry.js`.

## Completion criterion

- Red on `f247c535` in the harness, green on your branch; both themes, AA computed.
FIX

# richos-hq/docs/briefs/ray-brief-phone-path-in-the-vm-nightly-7-2026-09-19.md
# A QA walk over exactly the surface the delivery brief builds on. No build
# heading, and a "Do not" section that says it fixes nothing.
read -r -d '' RAYWALK <<'FIX' || true
# Brief: walk the phone path on nightly .7 inside the test VM — pairing, both sides, phone → Mac in one paint or not

## The walk, in order

4. **R3, phone → Mac.** In the guest, with the same thread open in the app: type a message in the phone page and send; capture the app window at ~45 ms intervals from the send; report the ms until the user bubble appears in the open thread. `richos/web/web-app/lib/link.js` is the phone page's send path.

## Deliverable

`docs/verification/2026-09-19-nightly-7-phone-path-in-the-vm-audit.md` in richos — verdict first, then a row per item with FIXED / STILL THERE / NOT REACHED and the measurement.

## Do not

Do not launch anything on the host. Do not fix anything; record it. No new dependency.
FIX

# richos-hq/docs/briefs/reed-brief-t3code-tooling-adoption-read-2026-09-19.md
# The read that COMMISSIONED the ledger. It names every area the ledger has,
# under a heading that is scope-shaped, and it must obviously pass.
read -r -d '' REEDREAD <<'FIX' || true
# Brief: read T3 Code's build, test, release, update and harness tooling in full, and say what RichOS can adopt instead of build

Our stack: Rust + Tauri 2 desktop app (`richos/app`), a WebKit-Playwright ui harness, `richos/app/scripts/nightly-local.py` for build/sign/notarize/publish to GitHub releases, `richos-user-update` crate for updates.

## What to deliver — a committed brief in your worktree, `docs/research/t3code-tooling-what-to-adopt-2026-09-19.md`

1. **Per area, one of three verdicts with the file:line evidence:** ADOPT AS-IS, COPY THE APPROACH, NOT APPLICABLE. Areas: build/sign/notarize/publish; update channel and rollback; test harness and screenshot stability; release gating; developer loop; crash/reconnect; anything else you find that we hand-built (compare against `richos/app/scripts/` and `richos/app/ui/tests/lib/`).

Read in full; every claim carries a `t3code:<path>:<line>`. Commit, SHA in the handoff. Delete the clone. Do not modify anything under `richos/app`.
FIX

# A BUILD BRIEF ON A SURFACE THE LEDGER DOES NOT COVER. The positive probe for
# every "passes" case above: these assertions can fail.
read -r -d '' UNRELATED <<'FIX' || true
# Brief: the sidebar's Running row does not clear its activity dot when a turn ends

## Scope and files

`app/ui/sidebar.js`, `app/ui/style.css`, `app/ui/tests/sidebar.js`.

## Completion criterion

- Red on `f247c535`, green on your branch.
FIX

# ===========================================================================
# HARNESS
# ===========================================================================

# payload <tool> <text> — a real PreToolUse payload on stdout.
payload() {
    TOOL="$1" TEXT="$2" python3 -c '
import json, os
tool = os.environ["TOOL"]
text = os.environ["TEXT"]
ti = {"prompt": text, "description": "spawn"} if tool == "Agent" else {"file_path": "/tmp/x.md", "content": text}
print(json.dumps({"session_id": "s1", "cwd": os.environ.get("CWD", ""),
                  "tool_name": tool, "tool_input": ti}))
'
}

# run_case <id> <want-rc> <description> <tool> <text> [env assignments...]
run_case() {
    local id="$1" want="$2" desc="$3" tool="$4" text="$5"; shift 5
    local out rc
    # THE CASE'S OWN ASSIGNMENTS COME LAST, so a case can override the default
    # entity root. With the default last, G3 silently tested the governed root
    # and passed for the wrong reason.
    out="$(payload "$tool" "$text" | env RICHOS_ENTITY_ROOT="$ENTITY" "$@" "$HOOK" 2>&1)"
    rc=$?
    if [ "$rc" -eq "$want" ]; then
        PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' "$id" "$desc"
        LAST_OUT="$out"
        return 0
    fi
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s (rc=%s want=%s)\n' "$id" "$desc" "$rc" "$want"
    printf '%s\n' "$out" | sed 's/^/          /' | head -12
    LAST_OUT="$out"
    return 1
}

# saw <id> <needle> <description> — assert the last refusal named something.
saw() {
    local id="$1" needle="$2" desc="$3"
    if printf '%s' "$LAST_OUT" | grep -qF -- "$needle"; then
        PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' "$id" "$desc"
    else
        FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s — never said %s\n' "$id" "$desc" "$needle"
    fi
}

withref() { printf '%s\n\n%s\n' "$1" "$2"; }

echo "=== guard-reference-ledger.test.sh ==="
echo ""
echo "A. THE TWO BRIEFS THE RULING IS ABOUT — refused as committed"

run_case A1 2 "the delivery brief (Scope names web-app/lib/link.js) is REFUSED" \
    Agent "$DELIVERY" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
saw A1a "web-app/lib/link.js" "the refusal names the signal it matched"
saw A1b "2.6" "the refusal names the section to cite"
saw A1c "REINVENTING" "the refusal carries the CEO's own sentence"

run_case A2 2 "the same-paint brief (Scope names ui/phone.js, text says intake) is REFUSED" \
    Agent "$SAMEPAINT" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
saw A2a "ui/phone.js" "the refusal names the weak path that counted"

echo ""
echo "B. THE SAME TWO, WITH A reference: LINE — allowed"

run_case B1 0 "the delivery brief with a live citation passes" \
    Agent "$(withref "$DELIVERY" 'reference: adoption ledger §2.6 — COPY THE APPROACH; a reconnect alone cannot distinguish success from rollback')" \
    RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
run_case B2 0 "the same-paint brief with a live citation passes" \
    Agent "$(withref "$SAMEPAINT" 'reference: adoption ledger §2.6 — COPY THE APPROACH; the connection runtime and the outbox')" \
    RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
if grep -q "section=verified" "$ENTITY/.claude/state/reference-ledger-acks.log" 2>/dev/null; then
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' B3 "an accepted citation is logged, and the section was VERIFIED against the ledger"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s\n' B3 "no verified citation in reference-ledger-acks.log"
fi
run_case B4 0 "an unreachable ledger accepts the citation on its shape" \
    Agent "$(withref "$DELIVERY" 'reference: adoption ledger §2.6 — COPY THE APPROACH')" \
    RICHOS_REFERENCE_LEDGER_DOC="$SB/no-such-ledger.md"
if grep -q "section=unverified" "$ENTITY/.claude/state/reference-ledger-acks.log" 2>/dev/null; then
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' B5 "and it is logged UNVERIFIED — 'I cannot check' is never 'forbidden'"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s\n' B5 "no unverified citation in reference-ledger-acks.log"
fi

echo ""
echo "C. A QA WALK AND A RESEARCH READ OVER THE SAME SURFACE — allowed"

run_case C1 0 "Ray's VM walk over the phone path passes (it reads, it does not build)" \
    Agent "$RAYWALK" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
run_case C2 0 "Reed's adoption read passes — it COMMISSIONED the ledger" \
    Agent "$REEDREAD" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
run_case C3 0 "POSITIVE PROBE: a build brief on a surface no row covers passes" \
    Agent "$UNRELATED" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"

# NARRATIVE IS NOT AN INSTRUCTION. This one names a ledger path, carries no
# read veto at all, and has no heading that names files to edit — so nothing
# in it says anybody is going to change that file. It is the case the
# BUILD-SECTION conjunct exists for, isolated from the read veto.
read -r -d '' NARRATIVE <<'FIX' || true
# Brief: tell me how long the phone path has been slow, and since which commit

Ray measured 7,288 ms on candidate .16 and 30-55 s on nightly .7. The send path
is `richos/web/web-app/lib/link.js` and the intake log is on the Mac side. Walk
the history and say when each number changed; the answer goes in your handoff.
FIX
run_case C4 0 "a narrative naming a ledger path, with no build section, passes" \
    Agent "$NARRATIVE" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"

echo ""
echo "D. THE WEAK-PATH RULE, BOTH DIRECTIONS"

run_case D1 0 "a phone-UI brief naming ui/phone.js with no transport word passes" \
    Agent "$PHONEUI" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
run_case D2 2 "NEGATIVE CONTROL: the same brief with one transport word is refused" \
    Agent "$(printf '%s\n\n%s\n' "$PHONEUI" 'The intake path is not in scope.')" \
    RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"

echo ""
echo "E. THE HATCH"

run_case E1 2 "a BARE 'reference:' exempts nothing" \
    Agent "$(withref "$DELIVERY" 'reference:')" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
saw E1a "BARE 'reference:'" "and the refusal says so by name"

run_case E2 2 "a citation naming no section is refused" \
    Agent "$(withref "$DELIVERY" 'reference: adoption ledger — it says copy the approach')" \
    RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
saw E2a "no section" "and the refusal says which half is missing"

run_case E3 2 "a citation naming no ledger is refused" \
    Agent "$(withref "$DELIVERY" 'reference: §2.6')" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
saw E3a "does not name the reference" "and the refusal says which half is missing"

run_case E4 2 "'reference: none' with no argument is refused" \
    Agent "$(withref "$DELIVERY" 'reference: none')" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
run_case E4b 2 "'reference: none — <short>' is refused" \
    Agent "$(withref "$DELIVERY" 'reference: none — not covered')" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
saw E4c "at least 40 characters" "and the refusal says how long a reason must be"

run_case E5 2 "a citation of a section that is NOT in the ledger is refused BY NAME" \
    Agent "$(withref "$DELIVERY" 'reference: adoption ledger §9.9 — it says so somewhere')" \
    RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
saw E5a "9.9 is NOT in" "and the refusal names the file it opened"

run_case E6 0 "'reference: none — <40+ characters>' passes" \
    Agent "$(withref "$DELIVERY" 'reference: none — engine guard tooling; no T3 Code ledger row covers the RichOS engine hook system')" \
    RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
if grep -q "engine guard tooling" "$ENTITY/.claude/state/reference-none.log" 2>/dev/null; then
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' E7 "and it is logged to .claude/state/reference-none.log"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s\n' E7 "nothing in reference-none.log"
fi

echo ""
echo "F. THE READ VETO IS QUANTIFIED"

run_case F1 2 "'you do not touch the host keychain' does NOT exempt a build brief" \
    Agent "$DELIVERY" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
run_case F2 0 "'Do not modify anything under richos/app' DOES — it is a read" \
    Agent "$(printf '%s\n\n%s\n' "$DELIVERY" 'Do not modify anything under richos/app.')" \
    RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"

echo ""
echo "G. FAIL OPEN"

run_case G1 0 "a call for another tool is silent" Write "$DELIVERY" \
    RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"
if [ -z "$LAST_OUT" ]; then
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' G1a "and says nothing at all"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s\n' G1a "a Write call produced output"
fi

G2_OUT="$(printf 'not json at all' | RICHOS_ENTITY_ROOT="$ENTITY" "$HOOK" 2>&1)"; G2_RC=$?
if [ "$G2_RC" -eq 0 ]; then
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' G2 "an unparseable payload ALLOWS"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s (rc=%s)\n' G2 "an unparseable payload must allow" "$G2_RC"
fi

# A REAL DIRECTORY WITH NO orchestration.config — adoption is DECLARED, so a
# path that does not exist would fall through to $PWD and be judged against
# whatever repository the suite happens to run in.
mkdir -p "$SB/not-adopted"
run_case G3 0 "a repository that never adopted the engine stands down" \
    Agent "$DELIVERY" RICHOS_ENTITY_ROOT="$SB/not-adopted" RICHOS_REFERENCE_LEDGER_DOC="$LEDGER"

G4_OUT="$(payload Agent "$DELIVERY" | env RICHOS_ENTITY_ROOT="$SB/off" "$HOOK" 2>&1)"; G4_RC=$?
mkdir -p "$SB/off"
printf 'PROTECTED_PATHS=""\nREFERENCE_LEDGER_GUARD="off"\n' >"$SB/off/orchestration.config"
G4_OUT="$(payload Agent "$DELIVERY" | env RICHOS_ENTITY_ROOT="$SB/off" "$HOOK" 2>&1)"; G4_RC=$?
if [ "$G4_RC" -eq 0 ] && [ -z "$G4_OUT" ]; then
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' G4 "a declared off switch stands down in silence"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s (rc=%s)\n' G4 "the off switch must stand down silently" "$G4_RC"
fi

G5_OUT="$(payload Agent "$DELIVERY" | env RICHOS_ENTITY_ROOT="$ENTITY" \
    RICHOS_REFERENCE_LEDGER_SURFACES="$SB/no-such-file.surfaces" "$HOOK" 2>&1)"; G5_RC=$?
# An explicit surfaces path that does not exist still falls through to the
# adopter's and then the engine's copy — so this asserts the SHIPPED file is
# found and the guard still works, which is the honest property.
if [ "$G5_RC" -eq 2 ]; then
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' G5 "a bad explicit surfaces path falls through to the shipped one"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s (rc=%s)\n' G5 "the shipped surfaces file must still be found" "$G5_RC"
fi

echo ""
echo "H. THE ONE HARD REFUSAL"

BROKEN="$SB/broken"
mkdir -p "$BROKEN/scripts/hooks"
cp "$HOOK" "$BROKEN/scripts/hooks/"
H1_OUT="$(payload Agent "$DELIVERY" | env RICHOS_ENTITY_ROOT="$ENTITY" \
    "$BROKEN/scripts/hooks/guard-reference-ledger.sh" 2>&1)"; H1_RC=$?
if [ "$H1_RC" -eq 2 ] && printf '%s' "$H1_OUT" | grep -q "BROKEN INSTALL"; then
    PASS=$((PASS + 1)); printf '  PASS  %-4s %s\n' H1 "a missing scripts/lib/resolve-roots.sh prints the shared banner and exits 2"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  %-4s %s (rc=%s)\n' H1 "a broken install must refuse loudly" "$H1_RC"
fi

echo ""
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -x "$SCRIPT_DIR/reference-ledger.mutation.sh" ]; then
    # THE HARNESS RUNS FROM THE SUITE IT MUTATES, as dialect-guard.test.sh
    # does. Until 2026-09-22 nothing ran it (open items 3.42/3.43, and
    # mutation-inventory.test.sh named it an orphan). The harness sets
    # RICHOS_MUTATION_INNER for every copy of this suite it runs, so this
    # block does not recurse. A failing harness counts as one failure.
    echo "=== running the mutation harness ==="
    "$SCRIPT_DIR/reference-ledger.mutation.sh" || FAIL=$((FAIL + 1))
fi
echo ""
echo "guard-reference-ledger: $PASS/$((PASS + FAIL)) cases pass"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
