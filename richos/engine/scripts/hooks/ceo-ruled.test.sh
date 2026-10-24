#!/usr/bin/env bash
#
# ceo-ruled.test.sh — the CEO-RULED gate, both directions, every case red first.
#
# ===========================================================================
# WHAT THIS SUITE IS FOR
# ===========================================================================
# On 2026-09-01 the orchestrator put three questions to the CEO that the record
# already answered, each written down by the orchestrator itself hours or days
# earlier. guard-ceo-ruled-ask.sh refuses those; notice-ceo-ruled-prose.sh
# reports the prose ones. This suite proves BOTH DIRECTIONS, because a
# one-sided check is satisfied by a corpse:
#
#   REFUSED   the three real questions of that night, each citing the ruling it
#             found — reproduced from the session transcripts, not invented.
#   PASSED    a genuinely new question the record does not answer (which
#             monospace face to ship), and the ordinary machinery cases.
#
# AND EVERY ASSERTION IS RUN RED, ONE MUTATION AT A TIME. A green tick over a
# gate that refuses everything, or over a record that parsed to nothing, would
# be the same defect this engine has caught in itself repeatedly: Layer K green
# over a scanner that never ran. Section 3 is the negative controls, and each
# one changes exactly one thing.
#
# THE RECORD USED HERE IS A FIXTURE, not the live wiki, so the suite is
# deterministic on any machine. It is shaped to match the live register's
# statistics where those matter — "logo" appears in several rulings and is
# therefore vocabulary, "swoosh" appears repeatedly in one and is therefore a
# subject — because those statistics are what the predicate decides on. Live-record
# auditing is explicit through CEO_RULED_LIVE_DIR; mutable private prose is not a
# hidden input to the default suite.

set -uo pipefail

VERBOSE=0
[ "${1:-}" = "--verbose" ] && VERBOSE=1

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_SRC="$(cd "$SRC_DIR/../.." && pwd)"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '         %s\n' "$2"; FAIL=$((FAIL + 1)); }
say() { [ "$VERBOSE" -eq 1 ] && printf '\n----- %s -----\n%s\n' "$1" "$2"; return 0; }

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

SANDBOX="$(cd "$(mktemp -d -t ceo-ruled.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

ENGINE="$SANDBOX/engine"
mkdir -p "$ENGINE/scripts/hooks" "$ENGINE/scripts/lib"
for h in guard-ceo-ruled-ask.sh notice-ceo-ruled-prose.sh; do
    cp "$SRC_DIR/$h" "$ENGINE/scripts/hooks/$h"
    chmod +x "$ENGINE/scripts/hooks/$h"
done
for l in ceo-ruled.sh ceo-ruled.py ceo-asks.sh ceo-asks.py ceo-todos.sh \
         ceo-todos.py resolve-roots.sh resolve-main-checkout.sh \
         stop-hook-notice.sh premise-ask.sh premise-ask.py; do
    cp "$SRC_DIR/../lib/$l" "$ENGINE/scripts/lib/$l" 2>/dev/null || true
done
cp "$ENGINE_SRC/scripts/ceo-ruled-exempt.sh" "$ENGINE/scripts/"
chmod +x "$ENGINE/scripts/ceo-ruled-exempt.sh"

GATE="$ENGINE/scripts/hooks/guard-ceo-ruled-ask.sh"
PROSE="$ENGINE/scripts/hooks/notice-ceo-ruled-prose.sh"
EXEMPT="$ENGINE/scripts/ceo-ruled-exempt.sh"

# --- the governed repository and its record ---------------------------------
SEAT="$SANDBOX/seat"
HQ="$SANDBOX/hq"
mkdir -p "$SEAT" "$HQ/wiki"
git -C "$SANDBOX" init -q seat 2>/dev/null || true
cat > "$SEAT/orchestration.config" <<CONF
CEO_RULINGS_PATHS="../hq/wiki/ceo-decisions.md ../hq/wiki/open-items.md CLAUDE.md"
CONF

write_decisions() {
    cat > "$HQ/wiki/ceo-decisions.md" <<'REC'
# CEO decisions — the standing register

## What this page is

Prose about the register. Not a ruling.

## 14. The visual standard — `round-8.1/v0` is it, for dark (CEO, 2026-08-30)

**His words:** *"as far as the actual color palette for dark mode in general as
well as the overall design of the page elements including the background etc,
this becomes the current standard."*

The palette is the standard. It says nothing about the logo.

## 15. Light mode, theming, and two components ported from deeply (CEO, 2026-08-30)

The light standard is Daybreak. The wordmark and the logo both re-ink per theme.

### The wordmark replaces "My Company" (CEO, 2026-08-30)

The wordmark, not the logo, carries the product name in the rail.

## 16. Delete the ACP adapter — RichOS drives Claude directly (CEO, 2026-08-31)

**His words:** *"Delete it."* The adapter goes.

## 19. The payload — SHIP NOTHING EXTRA, 8.8 MB (CEO, 2026-09-01)

**His words:** *"Go ahead with 8.8 MB."*

## 21. The v1 start screen — `round-6.4/v5`, finished in `round-11.1` (CEO, 2026-09-01)

He named Constellation as the v1 start screen.

### Typeface — Newsreader and Inter, APPROVED (CEO, 2026-09-01)

Both faces are approved for the product. A monospaced face is mentioned once
here and nowhere else, and nobody has ruled on which one.

### The splash screens — TWO, and the order is DETERMINISTIC (CEO, 2026-09-01)

**His words:** *"I have never approved more than 2 splash screens. The other
MOCKUP DESIGNS ARE NOT READY FOR USE IN SPLASH SCREENS YET."*

First start goes to number one, second to number two, and every start after
that back to number one.

### The logo — APPROVED (CEO, 2026-09-01)

**His words, on opening the wordmark sheet: "OK, go!"** The dark-mode and
light-mode marks are approved and are the assets the product uses.

The defect this closed was one bug in three places: the swoosh was a knock-out,
a hole rather than a color. A hole shows whatever is behind it, which is why
the mark rendered monochrome everywhere at once. The swoosh is painted now, so
each mark carries its own two tones on any ground, and the swoosh measures
7.68:1 dark and 3.49:1 light.

### A DESIGN SYSTEM — ruled (CEO, 2026-09-01)

Ten rules want a monospaced face and none is vendored. The logo lives in it.

## 22. Type — the app ships its own fonts, and relies on NO system font

The app vendors every face it uses.
REC
}

write_items() {
    cat > "$HQ/wiki/open-items.md" <<'REC'
# Open items — what is actually outstanding

## 1. Waiting on the CEO — a decision

**1.7** (the start screen) is CLOSED 2026-09-01 — he named Constellation.

## 3. Blocked on Rich

| # | Item | State |
| 3.13 | **Delete the ACP adapter — CLOSED 2026-09-01.** The adapter is gone. | `CLOSED` |
| 3.14 | **RULED 2026-09-01 — ship nothing extra, 8.8 MB.** His words: *"Go ahead with 8.8 MB."* What ships is four files. **His option D, 2026-08-31: detect at install or first run whether Claude Code is present, and download and install it if not.** It is not a new idea in this project — it is his own standing instruction (*"automatically download and install whatever the user needs"*), quoted at the head of the payload design. | `CLOSED` |
REC
}

write_claude() {
    cat > "$SEAT/CLAUDE.md" <<'REC'
# Rich — orchestrator

## Hard Rules

### No Pagination

No page numbers, no paginated tables, no pagination controls, anywhere.

### American English

American English is the language of every string a person reads.
REC
}

write_decisions
write_items
write_claude

# Export the same controlled register for the standalone drift harness.
if [ "${1:-}" = --export-register ]; then
    mkdir -p "$2"
    cp "$HQ/wiki/ceo-decisions.md" "$HQ/wiki/open-items.md" "$2/"
    exit 0
fi

# --- payloads ---------------------------------------------------------------
# The three questions are RECONSTRUCTED FROM THE SESSION TRANSCRIPTS of
# 2026-09-01, rendered as the AskUserQuestion calls they should have been. The
# wording is his and the orchestrator's, not this suite's.
ask_payload() { # <case> [session] [agent_id]
    CASE="$1" SID="${2:-sess-0001}" AID="${3:-}" python3 <<'PY'
import json, os
C = {
 "f1": {"header": "Install",
        "question": ("What the customer installs themselves. You ruled 8.8 MB, ship nothing "
                     "extra. That means the customer must already have Claude Code and the "
                     "engine directory."),
        "options": [{"label": "Build your Option D",
                     "description": "RichOS detects and installs what is missing on first run."},
                    {"label": "Ship the engine inside the app",
                     "description": "The engine directory travels in the payload."}]},
 "f2": {"header": "Logo",
        "question": ("The question that matters now is yours, and I am not going to guess it "
                     "again: is your mark one color, or does the swoosh get the gold?"),
        "options": [{"label": "One color", "description": "Your source file says one color."},
                    {"label": "Two tones", "description": "A mockup said two."}]},
 "f3": {"header": "Splash screens",
        "question": ("The app currently ships seven splash variations extracted from round 8.1. "
                     "Which splash screens should ship in v1?"),
        "options": [{"label": "Keep the seven", "description": "All seven stay in the array."},
                    {"label": "Your two only", "description": "The other seven come out."}]},
 "pos": {"header": "Monospace",
         "question": "No monospaced family is vendored yet. Which one should ship?",
         "options": [{"label": "JetBrains Mono", "description": "Open licensed, wide coverage."},
                     {"label": "IBM Plex Mono", "description": "Open licensed, closer to Inter."}]},
}
p = {"hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion",
     "session_id": os.environ["SID"], "cwd": os.environ.get("SEAT", ""),
     "tool_input": {"questions": [C[os.environ["CASE"]]]}}
if os.environ.get("AID"):
    p["agent_id"] = os.environ["AID"]
print(json.dumps(p))
PY
}

run_gate() { # <case> [session] [agent_id] -> RC, OUT
    local payload
    payload="$(SEAT="$SEAT" ask_payload "$1" "${2:-sess-0001}" "${3:-}")"
    OUT="$(cd "$SEAT" && CLAUDE_PROJECT_DIR="$SEAT" bash "$GATE" <<<"$payload" 2>&1)"
    RC=$?
    say "gate $1" "rc=$RC
$OUT"
    return 0
}

refused() { # <case> <label> <must-name>
    run_gate "$1"
    if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -qF "$3"; then
        ok "$2"
    else
        bad "$2" "rc=$RC; expected a refusal naming '$3'. Got: $(printf '%s' "$OUT" | head -4 | tr '\n' ' ')"
    fi
}

permitted() { # <case> <label>
    run_gate "$1"
    if [ "$RC" -eq 0 ]; then
        ok "$2"
    else
        bad "$2" "rc=$RC — the gate refused a question it should let through: $(printf '%s' "$OUT" | head -3 | tr '\n' ' ')"
    fi
}

echo ""
echo "=== 1. THE THREE FAILURES OF 2026-09-01, EACH REFUSED AND CITED ==="

refused f1 "1a  the Option D / what-the-customer-installs question is refused, citing row 3.14" \
    "row 3.14"
run_gate f1
if printf '%s' "$OUT" | grep -qF "automatically download and install whatever the user needs"; then
    ok "1b  ...and the refusal quotes his STANDING INSTRUCTION, the sentence that already answered it"
else
    bad "1b  the refusal quotes his standing instruction" \
        "the row's own words were not surfaced, so the refusal sends him hunting"
fi

refused f2 "1c  the logo one-tone-or-two question is refused, citing the logo ruling" \
    "The logo"
run_gate f2
if printf '%s' "$OUT" | grep -qF 'logo / swoosh'; then
    ok "1d  ...and it matched by TITLE PLUS CORROBORATION, not by the word 'logo' alone"
else
    bad "1d  the logo match is corroborated" \
        "expected the anchor 'logo / swoosh'; a bare 'logo' match would fire on any question mentioning a logo"
fi

refused f3 "1e  the splash-screens question is refused, citing the splash ruling" \
    "The splash screens"
run_gate f3
if printf '%s' "$OUT" | grep -qF "I have never approved more than 2 splash screens"; then
    ok "1f  ...and the refusal quotes him saying it, in capitals, already"
else
    bad "1f  the splash refusal quotes his own words" "his sentence was not surfaced"
fi

echo ""
echo "=== 2. THE POSITIVE CONTROL — a genuinely new question PASSES ==="
# The record MENTIONS a monospaced face exactly once, as a finding about
# vendored fonts, and rules nothing about it. If this is ever refused, the gate
# has stopped distinguishing "the record talks about this" from "the record
# ruled this", which is the whole predicate.
permitted pos "2a  'which monospace family should ship' is not refused — nobody ever ruled it"

echo ""
echo "=== 3. NEGATIVE CONTROLS — every case above, run RED, one mutation each ==="

# 3a. Take the corroborating word out of the logo ruling. The word "logo" is
# still there, in the title and in three other rulings. If case 1c still fires,
# it was never the corroboration doing the work.
write_decisions
python3 - "$HQ/wiki/ceo-decisions.md" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read().replace("swoosh", "curve")
open(p, "w").write(s)
PY
run_gate f2
if [ "$RC" -eq 0 ]; then
    ok "3a  NEGATIVE CONTROL: with 'swoosh' gone the logo question PASSES — case 1c was the corroboration, not the word 'logo'"
else
    bad "3a  NEGATIVE CONTROL" "the logo question is still refused with no corroborating word, so a bare one-word title is firing on its own"
fi
write_decisions

# 3b. Rename the splash heading. The body still says "splash screens" many
# times. If case 1e still fires, the anchor is reading bodies, which is the
# broad matcher that was measured and deleted.
python3 - "$HQ/wiki/ceo-decisions.md" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read().replace(
    "### The splash screens — TWO, and the order is DETERMINISTIC (CEO, 2026-09-01)",
    "### The opening sequence — TWO, and the order is DETERMINISTIC (CEO, 2026-09-01)")
open(p, "w").write(s)
PY
run_gate f3
if [ "$RC" -eq 0 ]; then
    ok "3b  NEGATIVE CONTROL: renaming the heading makes the splash question PASS — the anchor is the TITLE, never the body"
else
    bad "3b  NEGATIVE CONTROL" "still refused after the title changed, so something other than the title is matching"
fi
write_decisions

# 3c. Strip the settled marker off row 3.14. An open item is not a ruling, and
# refusing questions about open work is the false-positive class that would get
# this gate waived.
python3 - "$HQ/wiki/open-items.md" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read().replace("**RULED 2026-09-01 — ship nothing extra",
                           "**Proposed — ship nothing extra")
s = s.replace("| `CLOSED` |\n| 3.14", "| `CLOSED` |\n| 3.14")
s = s.replace("standing instruction", "note")
s = s.replace("His words: *\"Go ahead with 8.8 MB.\"* What ships is four files.",
              "What ships is four files.")
s = s.replace("| `CLOSED` |", "| `OPEN` |")
open(p, "w").write(s)
PY
run_gate f1
if [ "$RC" -eq 0 ]; then
    ok "3c  NEGATIVE CONTROL: an UNSETTLED row 3.14 lets the question through — only rulings are indexed"
else
    bad "3c  NEGATIVE CONTROL" "an unsettled row still refused the question, so open work is being treated as ruled"
fi
write_items

# 3d. Make the record actually rule the monospace question, three times over.
# If the positive control still passes here, it is passing because the gate is
# dead rather than because the record is silent — which is a corpse, and the
# thing that bit three times on 2026-09-01.
python3 - "$HQ/wiki/ceo-decisions.md" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
s += ("\n### The monospace family — RULED (CEO, 2026-09-01)\n\n"
      "**His words:** *\"JetBrains Mono, ship it.\"* The monospace family is\n"
      "JetBrains Mono, vendored like every other monospace face the app uses,\n"
      "and no monospace fallback to a system stack is permitted.\n")
open(p, "w").write(s)
PY
run_gate pos
if [ "$RC" -eq 2 ]; then
    ok "3d  NEGATIVE CONTROL: once the record RULES the monospace family the question is refused — case 2a passes on silence, not on a dead gate"
else
    bad "3d  NEGATIVE CONTROL" "the gate let through a question the record now explicitly rules, so case 2a proves nothing"
fi
write_decisions

# 3e. A one-word title must not vouch for itself. Found by the LIVE register
# moving underneath this predicate while it was being measured: a ruling
# appeared titled "The door", the word "door" corroborated the word "door", and
# a question from July about recording video in the app was refused. The
# corroborating signal has to be INDEPENDENT of the title, or it is a spelling
# of the title.
python3 - "$HQ/wiki/ceo-decisions.md" <<'DOORPY'
import sys
p = sys.argv[1]
s = open(p).read()
s += ("\n### The door — RULED (CEO, 2026-09-01)\n\n"
      "The door opens on launch. The door is the only door, and a second door\n"
      "is not a door.\n")
open(p, "w").write(s)
DOORPY
DOOR_PAYLOAD="$(python3 -c '
import json
print(json.dumps({"hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion",
  "session_id": "sess-0001",
  "tool_input": {"questions": [{"header": "Capture",
    "question": "Should the in-app recording door be extended to the men now, or wait for the native apps?",
    "options": [{"label": "Now", "description": "The door already exists for the intros."},
                {"label": "Wait", "description": "Hold the door until the native apps are online."}]}]}}))
')"
OUT="$(cd "$SEAT" && CLAUDE_PROJECT_DIR="$SEAT" bash "$GATE" <<<"$DOOR_PAYLOAD" 2>&1)"; RC=$?
say "door" "rc=$RC
$OUT"
if [ "$RC" -eq 0 ]; then
    ok "3e  REGRESSION: a one-word title cannot corroborate itself — 'The door' does not refuse a question about a recording door"
else
    bad "3e  REGRESSION: a one-word title cannot corroborate itself" \
        "rc=$RC — the subject word is counting as its own second signal, which is one coincidence wearing two hats"
fi
write_decisions

echo ""
echo "=== 4. FAIL OPEN — every plumbing failure lets the ask through, loudly ==="

mv "$HQ/wiki/ceo-decisions.md" "$HQ/wiki/ceo-decisions.md.away"
run_gate f2
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "CEO-RULED GATE IS OFF"; then
    ok "4a  a DECLARED record that is missing fails OPEN and says the gate is off"
else
    bad "4a  missing declared record fails open and loud" "rc=$RC out=$(printf '%s' "$OUT" | head -2 | tr '\n' ' ')"
fi
mv "$HQ/wiki/ceo-decisions.md.away" "$HQ/wiki/ceo-decisions.md"

mv "$ENGINE/scripts/lib/ceo-ruled.py" "$ENGINE/scripts/lib/ceo-ruled.py.away"
run_gate f2
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "CEO-RULED GATE IS OFF"; then
    ok "4b  a missing predicate fails OPEN and says so — an absent gate and a clean run never look the same"
else
    bad "4b  missing predicate fails open and loud" "rc=$RC out=$(printf '%s' "$OUT" | head -2 | tr '\n' ' ')"
fi
mv "$ENGINE/scripts/lib/ceo-ruled.py.away" "$ENGINE/scripts/lib/ceo-ruled.py"

OUT="$(cd "$SEAT" && CLAUDE_PROJECT_DIR="$SEAT" bash "$GATE" <<<'not json at all' 2>&1)"; RC=$?
if [ "$RC" -eq 0 ]; then
    ok "4c  an unparseable payload is NOT blocked — there is no question text to compare, and a refusal nobody could permit is worse than none"
else
    bad "4c  unparseable payload fails open" "rc=$RC"
fi

# A repository that declares nothing stands down SILENTLY. The engine loads at
# user scope in every directory on this machine; a notice in each would be the
# noise this engine already decided not to make.
BARE="$SANDBOX/bare"
mkdir -p "$BARE"
OUT="$(cd "$BARE" && CLAUDE_PROJECT_DIR="$BARE" bash "$GATE" <<<"$(SEAT="$BARE" ask_payload f2)" 2>&1)"; RC=$?
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "4d  a repository with no CEO record stands down SILENTLY, not loudly"
else
    bad "4d  no-record repository is silent" "rc=$RC out=$(printf '%s' "$OUT" | head -2 | tr '\n' ' ')"
fi

echo ""
echo "=== 5. THE ESCAPE HATCH IS A DECLARATION — a bare marker exempts nothing ==="

EOUT="$(cd "$SEAT" && CLAUDE_PROJECT_DIR="$SEAT" bash "$EXEMPT" sess-0001 "§21 › The logo" "no" 2>&1)"; ERC=$?
if [ "$ERC" -ne 0 ] && printf '%s' "$EOUT" | grep -q "A bare marker exempts nothing"; then
    ok "5a  a two-character reason is REFUSED — the reason is the whole mechanism"
else
    bad "5a  short reason refused" "rc=$ERC out=$(printf '%s' "$EOUT" | head -2 | tr '\n' ' ')"
fi

refused f2 "5b  before any declaration, the logo question is still refused" "The logo"

EOUT="$(cd "$SEAT" && CLAUDE_PROJECT_DIR="$SEAT" bash "$EXEMPT" sess-0001 "§21 › The logo" \
    "The logo ruling approves the marks as drawn; it never addresses how many tones the mark carries." 2>&1)"; ERC=$?
if [ "$ERC" -eq 0 ] && [ -f "$SEAT/.claude/state/ceo-ruled-exempts.log" ]; then
    ok "5c  a real reason is recorded, where a reviewer sees it"
else
    bad "5c  a real reason is recorded" "rc=$ERC out=$(printf '%s' "$EOUT" | head -2 | tr '\n' ' ')"
fi

permitted f2 "5d  ...and the declared citation lets THAT question through"

run_gate f2 sess-OTHER
if [ "$RC" -eq 2 ]; then
    ok "5e  the exemption does NOT carry to another session — it clears a habit, not a rule"
else
    bad "5e  exemption is per-session" "rc=$RC — a declaration in one session cleared another"
fi

refused f3 "5f  the exemption does NOT carry to another CITATION" "The splash screens"
rm -f "$SEAT/.claude/state/ceo-ruled-exempts.log"

echo ""
echo "=== 6. ATTRIBUTION — a worker's own question is not the orchestrator's ==="
run_gate f2 sess-0001 "a1234567890abcdef"
if [ "$RC" -eq 0 ]; then
    ok "6a  a call carrying an agent_id passes — a teammate's clarifying question is not a ruling being re-litigated"
else
    bad "6a  worker calls pass" "rc=$RC"
fi

echo ""
echo "=== 7. THE PROSE NOTICE — the half no gate can block ==="

prose_payload() { # <message>
    MSG="$1" SID="${2:-sess-0001}" SEATP="$SEAT" python3 <<'PY'
import json, os
print(json.dumps({"hook_event_name": "Stop", "session_id": os.environ["SID"],
                  "cwd": os.environ["SEATP"], "stop_hook_active": False,
                  "transcript_path": "", "last_assistant_message": os.environ["MSG"]}))
PY
}

# The 22:01 message of 2026-09-01, in its own words, cut to the paragraph that
# asked. It carries no question mark at all in that paragraph — it asked with
# **Options:** — which is why the window is the asking PARAGRAPH.
PROSE_MSG='Three, in the order they block.

**3. What the customer installs themselves.**
You ruled 8.8 MB, ship nothing extra. That means the customer must already have **Claude Code** *and* **the engine directory**. **Options: build your Option D (RichOS detects and installs what is missing), ship the engine inside the app, or v1 is explicitly dogfood-only.**'

POUT="$(cd "$SEAT" && CLAUDE_PROJECT_DIR="$SEAT" bash "$PROSE" <<<"$(prose_payload "$PROSE_MSG")" 2>&1)"; PRC=$?
say "prose" "rc=$PRC
$POUT"
if [ "$PRC" -eq 0 ] && printf '%s' "$POUT" | grep -q "ALREADY RULES" && printf '%s' "$POUT" | grep -q "row 3.14"; then
    ok "7a  the real prose Option D turn is reported, citing row 3.14 — the case the blocking gate cannot see"
else
    bad "7a  prose notice fires on the Option D turn" "rc=$PRC out=$(printf '%s' "$POUT" | head -2 | tr '\n' ' ')"
fi
if [ "$PRC" -eq 0 ]; then
    ok "7b  ...and it NEVER blocks — a Stop hook that refuses a turn strands the session"
else
    bad "7b  prose notice never blocks" "rc=$PRC"
fi

POUT="$(cd "$SEAT" && CLAUDE_PROJECT_DIR="$SEAT" bash "$PROSE" <<<"$(prose_payload 'Landed and pushed. Suites green.')" 2>&1)"; PRC=$?
if [ "$PRC" -eq 0 ] && ! printf '%s' "$POUT" | grep -q "ALREADY RULES"; then
    ok "7c  NEGATIVE CONTROL: a turn that asks nothing produces no notice"
else
    bad "7c  a turn with no question is quiet" "rc=$PRC out=$(printf '%s' "$POUT" | head -2 | tr '\n' ' ')"
fi

POUT="$(cd "$SEAT" && CLAUDE_PROJECT_DIR="$SEAT" bash "$PROSE" <<<"$(prose_payload 'Which monospace family should the app vendor? No monospaced face is vendored yet.')" 2>&1)"; PRC=$?
if [ "$PRC" -eq 0 ] && ! printf '%s' "$POUT" | grep -q "ALREADY RULES"; then
    ok "7d  ...and a genuinely new question asked in prose is quiet too"
else
    bad "7d  a new prose question is quiet" "rc=$PRC out=$(printf '%s' "$POUT" | head -2 | tr '\n' ' ')"
fi

echo ""
echo "=== 8. THE LIVE RECORD ON THIS MACHINE ==="
# ===========================================================================
# WHAT THIS SECTION IS FOR — AND THE PIN THAT WAS TAKEN OUT OF IT
# ===========================================================================
# Sections 1-7 run against a fixture that was SHAPED so the answers come out
# right: sixty lines, arranged so that "logo" reads as vocabulary and "swoosh"
# reads as a subject. That proves the MECHANISM, and it cannot prove the
# mechanism survives a real register — 67 rulings, roughly 1,400 lines, real
# term frequencies. That is what this section is for, and it is the only place
# in the suite that touches anything real:
#
#   NO FALSE NEGATIVE AT SCALE   the three questions that were the actual
#                                failure of 2026-09-01 are still refused when
#                                the haystack is the whole register, and the
#                                refusal is USABLE — it points at a line that
#                                exists and quotes what is written there.
#   NO FALSE POSITIVE AT SCALE   the one question nobody had ruled on still
#                                passes, against 1,400 lines of vocabulary.
#
# UNTIL 2026-09-04 CASE 8a ALSO ASSERTED **WHICH** RULING DID THE REFUSING: it
# grepped the refusal for the string "row 3.14". It went red — and not because
# anything broke. The gate refused, with rc=2, citing §19, the payload ruling,
# which is a better citation than the one the case demanded. What moved was the
# record: open-items.md retired row 3.14 on 2026-09-02 in "eight finished rows
# leave the page", which is ordinary maintenance of the kind
# guard-row-currency-commits.sh exists to DEMAND. A suite that goes red because
# the CEO's record was correctly maintained is a suite that gets ignored on the
# day it is right.
#
# WHICH ruling answers a question is a fact about the record's contents at one
# moment. It is not a property of the predicate. So the citation is no longer
# pinned by name, and two mechanical properties that cannot drift with content
# took its place — both of them things the old case never checked at all:
#
#   THE LOCATOR RESOLVES        every "in <file>, line <N>" the refusal prints
#                               has to land on a line that really carries the
#                               title printed above it. A refusal is worth
#                               something only as a pointer, and nothing until
#                               now checked that it pointed anywhere.
#   THE QUOTE IS THE RECORD'S   every quoted line has to be findable in the
#                               file it was attributed to — six consecutive
#                               words after normalization, because the renderer
#                               unwraps paragraphs, strips markdown and
#                               truncates at 400 characters. A refusal that
#                               invents his words is worse than no refusal.
#
# THIS IS WEAKER IN EXACTLY ONE PLACE AND THE PLACE IS NAMED: nothing here
# asserts any longer that the cited ruling is ABOUT the question. That claim is
# made against the fixture, where the record is fixed and the claim therefore
# means something — cases 1a-1f make it, and the negative controls of section 3
# prove each one load-bearing by taking the matching signal away. Case 8d is
# what stops a gate that refuses everything from passing this section, and
# scripts/hooks/ceo-ruled.mutation.sh turns every case here red by breaking the
# gate, one property at a time.
#
# When the live record is not on this machine, THAT IS SAID, not passed over —
# a suite that reported green for a check it never ran is the failure mode this
# whole mechanism exists for.
#
# CEO_RULED_LIVE_DIR redirects the search at a copy of the register. It exists
# for the drift demonstration in the mutation harness, which renumbers and
# retitles the record and requires this section to stay green, so that "this no
# longer depends on which row answers" is measured rather than asserted.
# Unit verification is reproducible across hosts and private-record maintenance.
# Live audits remain explicit through CEO_RULED_LIVE_DIR. The title-based predicate
# cannot promise to match an old question after the record's matching title is retired.
LIVE_DIR="${CEO_RULED_LIVE_DIR:-$HQ/wiki}"
LIVE_REGISTER_KIND="fixture"
[ -z "${CEO_RULED_LIVE_DIR:-}" ] || LIVE_REGISTER_KIND="explicit register"
if true; then
    LIVE_DEC="$LIVE_DIR/ceo-decisions.md"
    LIVE_ITEMS="$LIVE_DIR/open-items.md"
    LIVESEAT="$SANDBOX/liveseat"
    mkdir -p "$LIVESEAT"
    {
        printf 'CEO_RULINGS_PATHS="%s' "$LIVE_DEC"
        [ -f "$LIVE_ITEMS" ] && printf ' %s' "$LIVE_ITEMS"
        printf '"\n'
    } > "$LIVESEAT/orchestration.config"

    # Reads a refusal on stdin and answers the only two questions that survive
    # a maintained record: does every locator it printed land on the title it
    # printed, and is every sentence it quoted actually in the file it named.
    # Prints nothing when the refusal is sound; prints the reason otherwise.
    # It parses the REFUSAL TEXT, not the predicate's output, deliberately —
    # what the operator is shown is the thing that has to be true.
    cat > "$SANDBOX/live-citation.py" <<'CITEPY'
import os
import re
import sys

LIVE_DIR = os.environ["LIVE_DIR"]
WINDOW = 6  # consecutive words that must be found; see the section comment

LOC_RE = re.compile(r"^ {6}in (.+?), line (\d+)\s{3}\(matched: ")
QUOTE_RE = re.compile(r"^ {6}> (.*)$")
TITLE_RE = re.compile(r"^ {4}(\S.*?)  +(\S.*)$")


def norm(s):
    return re.sub(r"[^a-z0-9]+", " ", s.lower()).strip()


text = sys.stdin.read().split("\n")
blocks = []          # (cite, title, label, lineno, [quotes])
for i, line in enumerate(text):
    m = LOC_RE.match(line)
    if not m:
        continue
    label, lineno = m.group(1), int(m.group(2))
    t = TITLE_RE.match(text[i - 1]) if i else None
    cite, title = (t.group(1), t.group(2)) if t else ("?", "")
    quotes = []
    for nxt in text[i + 1:]:
        q = QUOTE_RE.match(nxt)
        if not q:
            break
        quotes.append(q.group(1))
    blocks.append((cite, title, label, lineno, quotes))

if not blocks:
    print("the refusal named no ruling at all — 'already decided' sends him hunting")
    sys.exit(0)

cache = {}
for cite, title, label, lineno, quotes in blocks:
    path = os.path.join(LIVE_DIR, label)
    if not os.path.isfile(path):
        print("%s cites %s, which is not one of the live record's files" % (cite, label))
        sys.exit(0)
    if path not in cache:
        with open(path, encoding="utf-8") as fh:
            cache[path] = fh.read().split("\n")
    lines = cache[path]
    if not 1 <= lineno <= len(lines):
        print("%s points at %s line %d; the file has %d lines"
              % (cite, label, lineno, len(lines)))
        sys.exit(0)
    if norm(title) not in norm(lines[lineno - 1]):
        print("%s prints title %r but %s line %d reads %r — the locator does not "
              "point at the ruling it names"
              % (cite, title[:60], label, lineno, lines[lineno - 1][:60]))
        sys.exit(0)
    if not quotes:
        print("%s is named but not quoted — a citation without his words is a "
              "citation the reader has to go and check" % cite)
        sys.exit(0)
    hay = norm("\n".join(lines))
    for q in quotes:
        words = norm(q).split()
        if not words:
            print("%s quoted an empty line" % cite)
            sys.exit(0)
        w = min(WINDOW, len(words))
        if not any(" ".join(words[i:i + w]) in hay
                   for i in range(len(words) - w + 1)):
            print("%s quotes %r, and no %d consecutive words of it appear in %s — "
                  "the refusal is not quoting the record" % (cite, q[:70], w, label))
            sys.exit(0)
CITEPY

    live_gate() {
        local payload
        payload="$(SEAT="$LIVESEAT" ask_payload "$1")"
        OUT="$(cd "$LIVESEAT" && CLAUDE_PROJECT_DIR="$LIVESEAT" bash "$GATE" <<<"$payload" 2>&1)"
        RC=$?
        say "live $1" "rc=$RC
$OUT"
        return 0
    }

    live_refused() { # <case> <label>
        live_gate "$1"
        if [ "$RC" -ne 2 ]; then
            bad "$2" "rc=$RC — the ${LIVE_REGISTER_KIND} register no longer refuses a question the record already answers: $(printf '%s' "$OUT" | head -3 | tr '\n' ' ')"
            return
        fi
        local why
        why="$(printf '%s' "$OUT" | LIVE_DIR="$LIVE_DIR" python3 "$SANDBOX/live-citation.py" 2>&1)"
        if [ -z "$why" ]; then
            ok "$2"
        else
            bad "$2" "$why"
        fi
    }

    live_refused f1 "8a  ${LIVE_REGISTER_KIND} register: the install / Option D question is refused, and every ruling it cites resolves and is quoted from the file"
    live_refused f2 "8b  ${LIVE_REGISTER_KIND} register: the logo one-tone-or-two question is refused, and every ruling it cites resolves and is quoted from the file"
    live_refused f3 "8c  ${LIVE_REGISTER_KIND} register: the splash-screens question is refused, and every ruling it cites resolves and is quoted from the file"

    live_gate pos
    if [ "$RC" -eq 0 ]; then
        ok "8d  ${LIVE_REGISTER_KIND} register: the monospace question PASSES — the positive control is measured, not assumed"
    else
        bad "8d  ${LIVE_REGISTER_KIND} register: the monospace question passes" "rc=$RC out=$(printf '%s' "$OUT" | head -3 | tr '\n' ' ')"
    fi
fi
echo ""
echo "=== 9. WIRING — registered, hashed, and carried into the sandboxes ==="

HOOKS_JSON="$ENGINE_SRC/hooks/hooks.json"
if grep -q 'guard-ceo-ruled-ask.sh' "$HOOKS_JSON" && \
   python3 -c "
import json,sys
d=json.load(open('$HOOKS_JSON'))
ms=[m for m in d['hooks']['PreToolUse'] if m.get('matcher')=='AskUserQuestion']
sys.exit(0 if ms and any('guard-ceo-ruled-ask.sh' in h['command'] for h in ms[0]['hooks']) else 1)"; then
    ok "9a  guard-ceo-ruled-ask.sh is registered on PreToolUse[AskUserQuestion]"
else
    bad "9a  gate registered on AskUserQuestion" "not in hooks/hooks.json under that matcher"
fi
if grep -q 'notice-ceo-ruled-prose.sh' "$HOOKS_JSON"; then
    ok "9b  notice-ceo-ruled-prose.sh is registered on Stop"
else
    bad "9b  prose notice registered on Stop" "not in hooks/hooks.json"
fi
for f in scripts/lib/ceo-ruled.sh scripts/lib/ceo-ruled.py; do
    if grep -q "$(basename "$f")" "$SRC_DIR/install.sh" 2>/dev/null; then
        ok "9c  $f is sidecar-hashed by install.sh"
    else
        bad "9c  $f is sidecar-hashed by install.sh" \
            "install.sh does not name it, so the integrity probe cannot notice it changing"
    fi
done
for f in scripts/lib/ceo-ruled.sh scripts/lib/ceo-ruled.py scripts/ceo-ruled-exempt.sh; do
    if grep -q "$f" "$SRC_DIR/contract-integrity.test.sh" 2>/dev/null; then
        ok "9d  $f is carried into the contract-integrity sandboxes"
    else
        bad "9d  $f is carried into the contract-integrity sandboxes" \
            "a sandbox missing it models an engine whose newest gate cannot start"
    fi
done
if grep -q 'ceo-ruled' "$ENGINE_SRC/scripts/demo.sh" 2>/dev/null; then
    ok "9e  the demo's sample engine carries the predicate"
else
    bad "9e  demo.sh carries the predicate" "demo.sh does not name ceo-ruled"
fi

# The root-resolution bootstrap is byte-identical across every rooted hook, and
# probe Layer R asserts it. Checked here too so a divergence is caught by the
# suite that owns these files rather than by a 14-minute probe.
REF="$(sed -n '/^SCRIPT_DIR=/,/^ENGINE_ROOT=/p' "$SRC_DIR/guard-stale-staging.sh" | grep -v '^  echo "  hook:')"
for h in guard-ceo-ruled-ask.sh notice-ceo-ruled-prose.sh; do
    MINE="$(sed -n '/^SCRIPT_DIR=/,/^ENGINE_ROOT=/p' "$SRC_DIR/$h" | grep -v '^  echo "  hook:')"
    if [ "$(printf '%s' "$REF" | grep -vc 'hook: scripts')" = "$(printf '%s' "$MINE" | grep -vc 'hook: scripts')" ]; then
        ok "9f  $h carries the shared root-resolution bootstrap"
    else
        bad "9f  $h carries the shared root-resolution bootstrap" "diverged"
    fi
done

# --- THE MUTATION HARNESS RUNS FROM THE SUITE IT MUTATES -------------------
# Until 2026-09-05 it ran from NOTHING. run-all-tests.sh discovers *.test.sh;
# contract-integrity.test.sh names a hand-typed set of harnesses that never
# included this one; and the only other mention of the file anywhere in the
# repository was a comment. So every property it proves load-bearing was
# proven exactly when somebody remembered to type its path. Recorded as row
# 3.31 of wiki/open-items.md.
#
# ITS FAILURE IS THIS SUITE'S FAILURE, not a warning. A harness whose result is
# advisory is a harness nobody has to fix.
#
# RICHOS_MUTATION_INNER is the only thing standing between this and an infinite
# regress: the harness EXPORTS it before running any copy of this suite, so the
# inner run skips this block. Never remove one half without the other.
# A MISSING HARNESS IS A FAILURE, NOT A SKIP, and that sentence was earned
# forty minutes ago rather than reasoned. The first draft of this block wrote
# the directory variable with one dollar sign too many, so the path expanded to
# the shell PID followed by the variable name, the test was false, the harness was
# skipped in silence and the suite printed a full green tally in 3.7 seconds.
# That is the exact defect this whole row is about, rebuilt inside the fix for
# it, and only the WALL CLOCK gave it away. So the guard now says so out loud.
if [ -z "${RICHOS_MUTATION_INNER:-}" ]; then
    echo ""
    echo "=== running the mutation harness: ceo-ruled.mutation.sh ==="
    if [ -x "$SRC_DIR/ceo-ruled.mutation.sh" ]; then
        CEO_RULED_LIVE_DIR="$LIVE_DIR" "$SRC_DIR/ceo-ruled.mutation.sh" || FAIL=$((FAIL + 1))
    else
        echo "  FAIL  MUT. ceo-ruled.mutation.sh is missing or not executable — IT DID NOT RUN"
        FAIL=$((FAIL + 1))
    fi
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "  $PASS/$PASS cases passed"
    exit 0
fi
echo "  $PASS passed, $FAIL FAILED"
exit 1
