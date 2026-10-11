#!/usr/bin/env bash
#
# guard-host-display-power.test.sh — regression tests for
# scripts/hooks/guard-host-display-power.sh.
#
# THE FIXTURES ARE THE REAL STRINGS OF 2026-09-19, not paraphrases. That is the
# whole design of this suite: the guard exists because of specific commands and
# specific sentences, some of which must be refused and some of which must
# pass, and a synthetic fixture cannot tell you which side of that line a
# change lands on. Every quoted block below is copied from the transcript,
# brief or record named above it.
#
# Covered here:
#
#   (a) REFUSES the two `pmset displaysleepnow` calls zach-opus-testvm1 made at
#       18:51:27Z and 18:51:31Z, verbatim, including the multi-line one whose
#       OTHER pmset — `pmset -g powerstate` — must not be what fires;
#   (b) REFUSES the Command-Q that reached the CEO's Terminal at 18:10Z, in the
#       brief's short form AND in the real form addressed at a unix id;
#   (c) REFUSES `sudo pmset sleepnow` and `caffeinate -dimsu`, and PASSES
#       `pmset -g`, `caffeinate -is <cmd>`, and `kill -TERM <pid>` — the last
#       because it is the PRESCRIBED remedy and a guard that catches the remedy
#       gets waived;
#   (d) GUEST BY CONSTRUCTION: `ssh <host> '<host-changing command>'` and
#       `testvm/ax.sh <vm> '<applescript>'` pass;
#   (e) NEGATIVE CONTROLS — six host commands wrapped to LOOK guest-bound. A
#       command after an ssh segment, inside `bash -c`, behind an ssh-shaped
#       environment assignment, in a command substitution, behind a launcher
#       merely NAMED like ssh, and in an `ssh -o ProxyCommand=` (which runs
#       LOCALLY). All refused;
#   (f) PROSE: the real 18:38:49Z SendMessage and the real 2026-09-18 brief
#       line are REFUSED; ceo-decisions §65 PASSES ON MERIT, with no path
#       exemption, because a guard that forbids the record of what it forbids
#       is the shape that cost this engine fourteen vendored lines in 2026-08;
#   (g) HEREDOCS ARE DOCUMENTS: `cat > note.md <<EOF` carrying the memory note
#       that BANS the Command-Q passes, and `cat > run.sh <<EOF` carrying a
#       script that RUNS it does not;
#   (h) THE HATCH IS A CITATION: a resolving `ceo-ruled-host-power:` line
#       permits and is LOGGED; a bare marker, a section with no quotation, and
#       a citation of a section NOT in the record each exempt nothing;
#   (i) FAIL OPEN: unparseable payload, another tool, an unadopted repository
#       and a declared off switch all allow — the first loudly, the rest silent;
#   (j) the self-exemption is by BASENAME and covers exactly four files;
#   (k) the one hard refusal: a missing scripts/lib/resolve-roots.sh prints the
#       shared BROKEN INSTALL banner and exits 2.
#
# Run directly: scripts/hooks/guard-host-display-power.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOK="$SCRIPT_DIR/guard-host-display-power.sh"

# DECLARED, never inherited from the launching session: run from a session
# seated elsewhere, every case below would pass by standing down.
unset CLAUDE_PROJECT_DIR

PASS=0
FAIL=0

SB="$(cd "$(mktemp -d -t guard-host-display-power.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SB"' EXIT

ENTITY="$SB/entity"
HQ="$SB/hq"
mkdir -p "$ENTITY/.claude/state" "$HQ/wiki"
printf 'PROTECTED_PATHS=""\nCEO_TODOS_REPOS="%s"\n' "$HQ" >"$ENTITY/orchestration.config"
: >"$HQ/.ceo-todos"

# ===========================================================================
# THE FIXTURES — real text, from the sources named
# ===========================================================================

# richos-hq/wiki/ceo-decisions.md §65, verbatim. The record the hatch resolves
# against AND the document case (f) drives through the guard: it quotes
# `pmset displaysleepnow`, `caffeinate -d`/`-u` and `killall loginwindow`, and
# it must be writeable. §99 is deliberately absent; case H4 cites it.
cat >"$HQ/wiki/ceo-decisions.md" <<'REC'
## §64 — Urban's re-walk is off fix-only nightly rounds (2026-09-19)

Body.

## §65 — No agent touches this Mac's display, sleep, lock or session state (2026-09-19)

**What happened.** To prove the new test VM survives a locked host, Rich wrote `pmset displaysleepnow` into a teammate's instructions as an option. The teammate ran it on the CEO's Mac, at least three times, while he was at the machine. His words, 19:10Z: *"So, this is why all of the monitors went black and there was no way for me to switch them back on? I'm lucky that locking the screen with the keyboard button worked and that unlocking brought back the screens on. But a horrible fucking experience that! I was already considering killing the Mac with the power button!"* The same day a teammate's Command-Q keystroke through System Events landed on his Terminal and offered to terminate the session.

**The rule, Rich's, recorded here because it reached him physically.** No agent, and no brief, message or script Rich writes, changes this Mac's display, sleep, lock, session or input state: no `pmset` other than `pmset -g`, no `caffeinate -d`/`-u`, no lock or sleep through osascript, no keystroke with a modifier to the host, no `shutdown`/`reboot`/`killall loginwindow`. Such state changes only inside a test VM over ssh. A "locked host" proof is done by asking him to lock it himself, or not done.

**Mechanism.** `guard-host-display-power.sh` in the engine (PreToolUse on Bash, Agent and brief writes), tested by defeat with the real strings from the day, enforcing from the next session. The only exemption is a `ceo-ruled-host-power:` line quoting a section of this file.
REC

# The two executions, verbatim, from
# .claude/projects/-Users-alex-ab-femcboost/<session>/subagents/
#   agent-acb0a69e78f52de0b.jsonl, lines 499 and 503.
# The FIRST is the interesting one: it carries `pmset -g powerstate` two lines
# below the offending call, so a rule that fired on the word rather than on the
# invocation would be indistinguishable from this one on it.
read -r -d '' TESTVM_499 <<'FIX' || true
D=/Users/alex/ab/richos-wt/zach-opus-testvm1/richos/app/scripts/testvm
pmset displaysleepnow
sleep 4
echo "=== host display state ==="
pmset -g powerstate IODisplayWrangler 2>/dev/null | tail -2 || echo "(no wrangler info)"
echo "=== capture while host display is asleep ==="
$D/shot.sh richos-test-1 /Users/alex/.richos-testvm/log/proof-engine-offer.png 2>&1
FIX

TESTVM_503='pmset displaysleepnow; echo "host display slept at $(date -u +%H:%M:%SZ)"'

# agent-a3661f802205dc81e.jsonl, 2026-09-19T18:10:28Z — the one that hit the
# CEO's Terminal. NOTE it is addressed at a specific unix id and still landed
# on him: System Events delivers to whatever is FRONTMOST, and the target had
# already exited.
CMDQ_REAL='osascript -e '"'"'tell application "System Events" to tell (first process whose unix id is 46360) to keystroke "q" using command down'"'"' 2>&1 | head -2; sleep 4; pgrep -x richos-tauri || echo "gone after Cmd-Q"'

# The short form the commissioning brief names.
CMDQ_SHORT='osascript -e '"'"'tell application "System Events" to keystroke "q" using command down'"'"''

# The 18:38:49Z SendMessage — the ACTUAL vector. Not a brief, not a spawn
# prompt: the mailbox. Both halves are here, the pmset and the caffeinate.
read -r -d '' MSG_1838 <<'FIX' || true
Two acceptance items added, from the CEO's question "so the Mac screen won't need to be kept unlocked after that?": (1) the completion proof (run.sh -> frame -> AX read -> stop.sh) must be captured while the HOST's screen is locked (`pmset displaysleepnow` or the lock shortcut via the CEO - if you cannot lock it yourself without a keystroke, say so and I will ask him to lock it for the test); the guest's own screensaver/lock is off and stays off. (2) The VM host process runs under `caffeinate -dimsu` (or equivalent) so a locked-but-awake Mac never pauses the VM; document in `docs/testvm.md` that locked is fine and sleep pauses everything.
FIX

# richos-hq/docs/briefs/echo-brief-wait-for-screen-unlock-2026-09-18.md, the
# block containing line 47. Written the day BEFORE §65 — the rule is right and
# the document predates it. Note the block also says "only if no candidate is
# on screen", about something else: without the permissive-construction veto
# that `no` exempts the whole item.
read -r -d '' BRIEF_0918 <<'FIX' || true
   `cargo test -p richos-core`, `cargo check` in `src-tauri`, the UI runner if a timeline line or
   status chip changes (registry row + affordance check), `gui-boot.test.sh` only if no candidate is
   on screen. Paste the tails. Dated record under `docs/verification/` with a real lock/unlock walk on
   this Mac (you may lock the screen with `pmset displaysleepnow` only when Rich says the screen is
   free - ask first; a walk is on it now).
FIX

# .claude/projects/.../memory/feedback_never_quit_by_keystroke.md, written at
# 2026-09-19T18:12:35Z through `cat > "$M/..." <<'EOF'`. The body BANS the
# Command-Q and quotes it to do so. Read as a command chain it is refused —
# the "a guard that forbids the record of what it forbids" shape, arriving
# through the Bash door.
read -r -d '' HEREDOC_NOTE <<'FIX' || true
M=/Users/alex/.claude/projects/-Users-alex-ab-femcboost/memory; cat > "$M/feedback_never_quit_by_keystroke.md" <<'EOF'
---
name: never-quit-by-keystroke
---

On 2026-09-19 ~18:10Z echo-opus-offer2 quit its on-screen proof instance with `osascript ... keystroke "q" using command down`. The app was not frontmost; the keystroke went to the CEO's Terminal.

**How to apply:** quit by process id only (`kill -TERM <pid>`); never any key with `command down` through System Events.
EOF
FIX

# The same shape writing a SCRIPT rather than a document. The redirect target
# picks the scope, so this one is judged per line and refused.
read -r -d '' HEREDOC_SCRIPT <<'FIX' || true
cat > /tmp/sleepit.sh <<'EOF'
echo "locking the host"
pmset displaysleepnow
EOF
FIX

# ===========================================================================
# HARNESS
# ===========================================================================

# payload <tool> <file_path> <text>
payload() {
    python3 - "$1" "$2" "$3" <<'PY'
import json, sys
tool, path, text = sys.argv[1:4]
if tool == "Bash":
    ti = {"command": text}
elif tool == "Agent":
    ti = {"subagent_type": "zach", "name": "zach-opus-t1", "prompt": text,
          "isolation": "worktree"}
elif tool == "SendMessage":
    ti = {"to": "zach-opus-t1", "message": text, "summary": "acceptance added"}
elif tool == "Write":
    ti = {"file_path": path, "content": text}
elif tool == "Edit":
    ti = {"file_path": path, "old_string": "PLACEHOLDER", "new_string": text}
elif tool == "MultiEdit":
    ti = {"file_path": path, "edits": [{"old_string": "P", "new_string": text}]}
else:
    ti = {"command": text}
print(json.dumps({"tool_name": tool, "tool_input": ti,
                  "session_id": "hp000000-0000-4000-8000-000000000000",
                  "tool_use_id": "toolu_hp_test"}))
PY
}

RC=0
OUT=""
run() {   # <json> [extra env assignments...]
    local json="$1"; shift
    set +e
    OUT="$(printf '%s' "$json" | env RICHOS_ENTITY_ROOT="$ENTITY" "$@" "$HOOK" 2>&1)"
    RC=$?
    set -e
}

ok()  { PASS=$((PASS + 1)); printf '  PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); printf '  FAIL  %s%s\n' "$1" "${2:+ — $2}"; }

expect_rc() {   # <name> <rc> <json>
    run "$3"
    if [ "$RC" -eq "$2" ]; then ok "$1"; else bad "$1" "expected exit $2, got $RC: ${OUT:0:240}"; fi
}

expect_silent() {   # <name> <json>
    run "$2"
    if [ "$RC" -ne 0 ]; then bad "$1" "expected exit 0, got $RC: ${OUT:0:240}"
    elif [ -n "$OUT" ]; then bad "$1" "expected SILENCE, got: ${OUT:0:240}"
    else ok "$1"; fi
}

expect_says() {   # <name> <needle> <json>
    run "$3"
    if printf '%s' "$OUT" | grep -qF "$2"; then ok "$1"; else bad "$1" "output did not carry '$2': ${OUT:0:300}"; fi
}

printf '=== guard-host-display-power.sh ===\n'

# --- (z) THE FILE IS SHELL, AND ITS PYTHON BLOCK IS INSIDE A SINGLE QUOTE ---
# Z1 is a plain syntax check and Z2 is the invariant that keeps breaking it.
# The classifier is embedded as `python3 -c '<block>'`, so ONE apostrophe in a
# python comment closes the bash quote and everything after it is reparsed as
# shell. It happened three times while this guard was being written — "the
# CEO's own setting", "the login session or the CEO's terminal", "those
# scripts' own headers" — and each time the whole file died with an EOF error
# a hundred lines away from the cause. Without Z2 the suite still catches it,
# but as 90 unrelated failures rather than one line naming the apostrophe.
if bash -n "$HOOK" 2>/dev/null; then ok "Z1  the hook parses as bash"
else bad "Z1  the hook parses as bash" "$(bash -n "$HOOK" 2>&1 | head -2)"; fi

Z_BAD="$(awk "/FILE_PATH=\"\\\$FILE_PATH\" python3 -c '/{f=1;next} /^' 2>\\/dev\\/null/{f=0} f && /'/{print NR\": \"\$0}" "$HOOK")"
if [ -z "$Z_BAD" ]; then ok "Z2  ... and its embedded python block contains no apostrophe"
else bad "Z2  ... and its embedded python block contains no apostrophe" "$(printf '%s' "$Z_BAD" | head -3)"; fi

# Z3 IS HERE BECAUSE THE HARNESS RAN A COMMAND ON THE CEO'S MAC.
# The mutation harness passes each mutant's rationale as a double-quoted bash
# argument, and a backtick inside double quotes is COMMAND SUBSTITUTION. A
# rationale reading "...makes <backtick>brew install tesseract
# displayplacer<backtick> an invocation..." installed 59 Homebrew formulae at
# 20:56 on 2026-09-19 — including displayplacer, a display-reconfiguration
# tool, which is the exact class of thing §65 exists to keep off this machine.
# A guard against changing the host, whose own test harness changed the host.
# The invariant is therefore blunt and total: NO BACKTICK ANYWHERE in that
# file. It costs nothing — prose does not need them — and it cannot be got
# wrong the way "remember to escape it" can.
Z_MUT="$SCRIPT_DIR/host-display-power.mutation.sh"
if [ ! -f "$Z_MUT" ] && [ -n "${RICHOS_MUTATION_INNER:-}" ]; then
    # Running AS a mutant: the harness deliberately copies only the guard and
    # this suite into the sandbox, so its own absence is expected there and is
    # not a finding. Outside that, an absent harness IS one.
    :
elif [ ! -f "$Z_MUT" ]; then bad "Z3  the mutation harness contains no backtick" "harness missing at $Z_MUT"
elif ! grep -q '`' "$Z_MUT"; then ok "Z3  the mutation harness contains no backtick (it once ran brew install)"
else bad "Z3  the mutation harness contains no backtick (it once ran brew install)" \
        "$(grep -n '`' "$Z_MUT" | head -3)"; fi

# --- (a) the two executions of 2026-09-19 ----------------------------------
expect_rc     "A1  the 18:51:27Z call is REFUSED" 2 "$(payload Bash "" "$TESTVM_499")"
expect_rc     "A2  the 18:51:31Z call is REFUSED" 2 "$(payload Bash "" "$TESTVM_503")"
expect_says   "A3  ... and it is the displaysleepnow that fires, not the -g two lines below" \
              "pmset displaysleepnow" "$(payload Bash "" "$TESTVM_499")"
expect_says   "A4  the refusal carries his words, not a pointer to them" \
              "killing the Mac with the power button" "$(payload Bash "" "$TESTVM_503")"
expect_says   "A5  the refusal carries the REPLACEMENT, so it is not just a no" \
              "kill -TERM <pid>" "$(payload Bash "" "$TESTVM_503")"
expect_says   "A6  ... including the safe caffeinate form" \
              "caffeinate -is <cmd>" "$(payload Bash "" "$TESTVM_503")"
expect_says   "A7  the refusal gives the citation line" \
              "ceo-ruled-host-power:" "$(payload Bash "" "$TESTVM_503")"

# --- (b) the Command-Q that reached his Terminal ---------------------------
expect_rc     "B1  the real 18:10Z Command-Q, addressed at a unix id, is REFUSED" 2 \
              "$(payload Bash "" "$CMDQ_REAL")"
expect_rc     "B2  the short form is REFUSED" 2 "$(payload Bash "" "$CMDQ_SHORT")"
expect_says   "B3  ... and the refusal says WHY a pid is different from a keystroke" \
              "FRONTMOST" "$(payload Bash "" "$CMDQ_SHORT")"
expect_silent "B4  an UNMODIFIED key code to a frontmost app PASSES" \
              "$(payload Bash "" "osascript -e 'tell application \"System Events\" to key code 53'")"
expect_rc     "B5  a Control-Command-Q (the lock shortcut) is REFUSED" 2 \
              "$(payload Bash "" "osascript -e 'tell application \"System Events\" to keystroke \"q\" using {command down, control down}'")"
expect_rc     "B6  an osascript that tells the host to sleep is REFUSED" 2 \
              "$(payload Bash "" "osascript -e 'tell application \"Finder\" to sleep'")"

# --- (c) the rest of the surface, read off the man pages -------------------
expect_rc     "C1  sudo pmset sleepnow is REFUSED (the wrapper is stripped)" 2 \
              "$(payload Bash "" "sudo pmset sleepnow")"
expect_rc     "C2  sudo -u alex pmset sleepnow is REFUSED (the flag takes an argument)" 2 \
              "$(payload Bash "" "sudo -u alex pmset sleepnow")"
expect_rc     "C3  pmset -a displaysleep 10 is REFUSED (a source flag WRITES)" 2 \
              "$(payload Bash "" "pmset -a displaysleep 10")"
expect_rc     "C4  pmset relative wake 60 is REFUSED (in the synopsis, in no list given)" 2 \
              "$(payload Bash "" "pmset relative wake 60")"
expect_rc     "C5  pmset schedule sleep is REFUSED" 2 \
              "$(payload Bash "" "pmset schedule sleep \"07/04/26 20:00:00\"")"
expect_silent "C6  pmset -g PASSES" "$(payload Bash "" "pmset -g")"
expect_silent "C7  pmset -g powerstate PASSES" \
              "$(payload Bash "" "pmset -g powerstate IODisplayWrangler")"
expect_silent "C8  a bare pmset (usage) PASSES" "$(payload Bash "" "pmset")"
expect_rc     "C9  caffeinate -dimsu is REFUSED (the 18:38Z message form)" 2 \
              "$(payload Bash "" "caffeinate -dimsu python3 scripts/nightly-local.py build")"
expect_rc     "C10 caffeinate -u alone is REFUSED (it turns the display ON)" 2 \
              "$(payload Bash "" "caffeinate -u -t 3600")"
expect_rc     "C10b P3-23: caffeinate -t 60 -d is REFUSED (the -t value must not hide -d)" 2 \
              "$(payload Bash "" "caffeinate -t 60 -d")"
expect_silent "C11 caffeinate -is <cmd> PASSES — the shipped form stays shipped" \
              "$(payload Bash "" "caffeinate -is cargo build --release")"
expect_silent "C12 kill -TERM <pid> PASSES — a guard that catches the remedy gets waived" \
              "$(payload Bash "" "kill -TERM 46360; sleep 2; pgrep -x richos-tauri || echo gone")"
expect_rc     "C13 killall loginwindow is REFUSED" 2 "$(payload Bash "" "killall loginwindow")"
expect_rc     "C14 killall -HUP WindowServer is REFUSED (the flag is not the target)" 2 \
              "$(payload Bash "" "killall -HUP WindowServer")"
expect_silent "C15 pkill -x richos-tauri PASSES — it is not a protected process" \
              "$(payload Bash "" "pkill -x richos-tauri")"
expect_rc     "C16 sudo shutdown -r now is REFUSED" 2 "$(payload Bash "" "sudo shutdown -r now")"
expect_rc     "C17 open -a ScreenSaverEngine is REFUSED" 2 \
              "$(payload Bash "" "open -a ScreenSaverEngine")"
expect_rc     "C18 defaults write com.apple.screensaver is REFUSED" 2 \
              "$(payload Bash "" "defaults -currentHost write com.apple.screensaver idleTime 0")"
expect_silent "C19 defaults read com.apple.screensaver PASSES" \
              "$(payload Bash "" "defaults -currentHost read com.apple.screensaver idleTime")"
expect_rc     "C20 systemsetup -setdisplaysleep is REFUSED" 2 \
              "$(payload Bash "" "sudo systemsetup -setdisplaysleep 1")"
expect_silent "C21 systemsetup -getdisplaysleep PASSES" \
              "$(payload Bash "" "systemsetup -getdisplaysleep")"
expect_rc     "C22 launchctl reboot userspace is REFUSED" 2 \
              "$(payload Bash "" "sudo launchctl reboot userspace")"
expect_rc     "C23 nvram writing a variable is REFUSED" 2 \
              "$(payload Bash "" "sudo nvram boot-args=-v")"
expect_silent "C24 nvram -p PASSES" "$(payload Bash "" "nvram -p")"
expect_silent "C25 a grep FOR the command PASSES — the word is not the invocation" \
              "$(payload Bash "" "grep -rn 'pmset displaysleepnow' /Users/alex/ab/richos-hq | head")"
expect_silent "C26 git log naming it in a message PASSES" \
              "$(payload Bash "" "git log --oneline --grep='pmset displaysleepnow' | head -5")"

# --- (d) guest by construction ---------------------------------------------
expect_silent "D1  ssh <host> 'pmset displaysleepnow' PASSES" \
              "$(payload Bash "" "ssh admin@192.168.64.7 'pmset displaysleepnow'")"
expect_silent "D2  testvm/ax.sh <vm> '<applescript>' PASSES" \
              "$(payload Bash "" "testvm/ax.sh vm1 'tell application \"System Events\" to key code 53'")"
expect_silent "D3  ... and an absolute ax.sh with a MODIFIER keystroke PASSES too" \
              "$(payload Bash "" "/Users/alex/ab/richos/richos/app/scripts/testvm/ax.sh vm1 'tell application \"System Events\" to keystroke \"q\" using command down'")"
expect_silent "D4  scp to the guest PASSES" \
              "$(payload Bash "" "scp /tmp/x admin@192.168.64.7:/tmp/pmset-notes")"

# --- (e) NEGATIVE CONTROLS: wrapped to LOOK guest-bound --------------------
# Every one of these contains an ssh-shaped token and runs on the HOST.
expect_rc     "E1  a host command AFTER an ssh segment is REFUSED" 2 \
              "$(payload Bash "" "ssh admin@192.168.64.7 'true'; pmset displaysleepnow")"
expect_rc     "E2  a host command inside bash -c is REFUSED" 2 \
              "$(payload Bash "" "bash -c 'pmset displaysleepnow'")"
expect_rc     "E3  a host command behind an ssh-SHAPED assignment is REFUSED" 2 \
              "$(payload Bash "" 'SSH="ssh admin@vm" pmset displaysleepnow')"
expect_rc     "E4  a host command in a command substitution is REFUSED" 2 \
              "$(payload Bash "" 'echo "$(pmset displaysleepnow)"')"
expect_rc     "E5  a launcher merely NAMED like ssh exempts nothing" 2 \
              "$(payload Bash "" "notssh admin@vm 'true' && pmset displaysleepnow")"
expect_rc     "E6  ssh -o ProxyCommand= runs LOCALLY and is REFUSED" 2 \
              "$(payload Bash "" "ssh -o ProxyCommand='pmset displaysleepnow' vm true")"
expect_rc     "E7  a backtick substitution is REFUSED" 2 \
              "$(payload Bash "" 'X=`pmset displaysleepnow`; echo "$X"')"

# --- (f) the prose surfaces ------------------------------------------------
expect_rc     "F1  the REAL 18:38:49Z SendMessage is REFUSED" 2 \
              "$(payload SendMessage "" "$MSG_1838")"
expect_says   "F2  ... and it names BOTH halves, the pmset and the caffeinate" \
              "caffeinate -d" "$(payload SendMessage "" "$MSG_1838")"
expect_rc     "F3  the REAL 2026-09-18 brief line is REFUSED as a Write" 2 \
              "$(payload Write "$SB/docs/briefs/echo-brief-wait-for-screen-unlock-2026-09-18.md" "$BRIEF_0918")"
expect_rc     "F4  ... and as an Agent spawn prompt" 2 "$(payload Agent "" "$BRIEF_0918")"
expect_silent "F5  ceo-decisions §65 PASSES — on MERIT, and it quotes every term" \
              "$(payload Write "$HQ/wiki/ceo-decisions.md" "$(cat "$HQ/wiki/ceo-decisions.md")")"
# Proof it is MERIT and not the path: the same file with an INSTRUCTION in it.
expect_rc     "F6  ceo-decisions.md is NOT path-exempt — the 09-18 line there is REFUSED" 2 \
              "$(payload Write "$HQ/wiki/ceo-decisions.md" "$BRIEF_0918")"
expect_silent "F7  a brief RECORDING the ban PASSES" \
              "$(payload Agent "" 'The guard refuses `pmset displaysleepnow`, `caffeinate -d` and `killall loginwindow`. No agent changes the host display.')"
expect_silent "F8  'never run pmset displaysleepnow' in a code comment PASSES" \
              "$(payload Edit "$SB/scripts/a.sh" '# never run pmset displaysleepnow on the host; quit by pid instead')"
# F8 PASSES ON ITS CUE — "never" — and does so with the comment stripper
# disabled, which the mutation harness proved and this note records because an
# earlier draft of the guard's header credited the stripper for it. What fixed
# F8 was deleting a permissive clause that matched `run <trigger>`; the
# stripper was added in the same edit and took the credit. F13 is the case
# that actually isolates it.
#
# MEASURED, on richos/app/scripts/testvm/provision-guest.sh as it landed at
# 321f1076: a comment with no negation cue, and a MENTION of a trigger word in
# argument position, were both refused by a word-scan that took every trigger
# to the end of its line. The code surface requires COMMAND POSITION now, the
# same test the Bash surface always applied.
expect_silent "F13 a comment with NO cue still PASSES (a comment cannot run)" \
              "$(payload Edit "$SB/scripts/a.sh" '# handy: pmset displaysleepnow blanks the screen')"
expect_silent "F14 ... and a // comment in non-shell source PASSES" \
              "$(payload Edit "$SB/src/a.js" '// handy: pmset displaysleepnow blanks the screen')"
expect_silent "F15 a MENTION in argument position is not an invocation" \
              "$(payload Edit "$SB/scripts/a.sh" 'brew install tesseract displayplacer >/tmp/brew.log 2>&1')"
# A LINE CONTINUATION IS ONE COMMAND. testvm/setup.sh reboots the GUEST across
# two lines; per physical line the continuation loses its `ssh`.
#
# THE FIXTURE IS UNQUOTED ON PURPOSE, and the first version of it was not — it
# used setup.sh's own `'sudo shutdown -r now'`, which shlex reads as ONE token
# whose basename is the whole string, so it passed with the joining disabled
# and proved nothing. The mutation harness caught that by refusing to go red.
# F16b keeps the real line anyway: it is the text this property was written
# for, and a case that passes both ways is still worth having as a fixture.
expect_silent "F16 a backslash-continued ssh keeps its command word" \
              "$(payload Edit "$SB/scripts/a.sh" 'ssh "${OPTS[@]}" -i "$KEY" "$USER@$IP" \
  sudo shutdown -r now')"
expect_silent "F16b the real testvm/setup.sh guest reboot, verbatim" \
              "$(payload Edit "$SB/scripts/a.sh" 'ssh "${TESTVM_SSH_OPTS[@]}" -i "$TESTVM_SSH_KEY" "$TESTVM_GUEST_USER@$IP" \
  '"'"'sudo shutdown -r now'"'"' >/dev/null 2>&1 || true')"
# The negative control for the joining: it must not SWALLOW a host command
# that happens to be continued. Here the command word after the join is the
# dangerous one, so the join changes nothing and the refusal stands.
expect_rc     "F17 ... and joining does not swallow a continued HOST command" 2 \
              "$(payload Edit "$SB/scripts/a.sh" 'sudo \
  shutdown -r now')"
# An unresolvable $VAR in command position is stepped over, never trusted:
# `$SUDO pmset ...` is the sudo-or-nothing idiom and testvm/provision-guest.sh
# uses it. Treating $SUDO as the command word would hide the line completely.
expect_rc     "F18 a \$VAR in command position does not hide what follows" 2 \
              "$(payload Edit "$SB/scripts/a.sh" '$SUDO pmset -a displaysleep 0 sleep 0')"
# THE GUEST BOUNDARY ON THE FILE SURFACE. Same path segment the shell surface
# uses. provision-guest.sh runs pmset and three `defaults write` lines INSIDE
# the guest, and nothing in its text says so.
expect_silent "F19 a file under testvm/ is guest-side by declaration" \
              "$(payload Edit "$SB/app/scripts/testvm/provision-guest.sh" '$SUDO pmset -a displaysleep 0 sleep 0 disksleep 0 powernap 0
defaults -currentHost write com.apple.screensaver idleTime -int 0')"
expect_rc     "F20 ... and the SAME content outside testvm/ is REFUSED" 2 \
              "$(payload Edit "$SB/app/scripts/provision-guest.sh" '$SUDO pmset -a displaysleep 0 sleep 0 disksleep 0 powernap 0
defaults -currentHost write com.apple.screensaver idleTime -int 0')"
expect_rc     "F9  ... and the same line WITHOUT the comment marker is REFUSED" 2 \
              "$(payload Edit "$SB/scripts/a.sh" 'echo locking
pmset displaysleepnow')"
expect_silent "F10 a brief with no trigger word anywhere PASSES" \
              "$(payload Agent "" 'Build the offer screen. Run the UI suite. Report SHAs oldest-first.')"
expect_rc     "F11 a bare permission with no negation is REFUSED" 2 \
              "$(payload Agent "" 'The walk. You may use `pmset displaysleepnow` here.')"
# The lookahead on the permissive clause. "You may NOT" is a PROHIBITION
# wearing a permission verb; without the lookahead the veto fires on it and
# the sentence banning the command is refused for banning it.
expect_silent "F12 ... and 'you may NOT run it' is a prohibition, not a permission" \
              "$(payload Agent "" 'The walk. You may not run `pmset displaysleepnow` on the host.')"

# --- (g) heredocs are documents --------------------------------------------
expect_silent "G1  a heredoc writing the note that BANS the Command-Q PASSES" \
              "$(payload Bash "" "$HEREDOC_NOTE")"
expect_rc     "G2  a heredoc writing a SCRIPT that RUNS it is REFUSED" 2 \
              "$(payload Bash "" "$HEREDOC_SCRIPT")"
expect_rc     "G3  a heredoc brief INSTRUCTING it is REFUSED" 2 \
              "$(payload Bash "" "cat > /tmp/b.md <<'EOF'
Then run the build under \`caffeinate -dimsu\` and quote the directory.
EOF")"

# --- (h) the hatch is a citation -------------------------------------------
ACKLOG="$ENTITY/.claude/state/host-power-acks.log"
: >"$ACKLOG"
GOOD_ACK='pmset displaysleepnow  # ceo-ruled-host-power: §65 — "No agent, and no brief, message or script Rich writes, changes this Mac"'
expect_silent "H1  a resolving citation with his words PERMITS" "$(payload Bash "" "$GOOD_ACK")"
if grep -q 'cite=§65' "$ACKLOG" 2>/dev/null; then ok "H2  ... and is LOGGED to host-power-acks.log"
else bad "H2  ... and is LOGGED to host-power-acks.log" "nothing in $ACKLOG"; fi
if grep -q 'section=verified' "$ACKLOG" 2>/dev/null; then ok "H3  ... recorded as verified against the record"
else bad "H3  ... recorded as verified against the record" "$(head -1 "$ACKLOG" 2>/dev/null)"; fi

expect_rc     "H4  a citation of a section NOT in the record is REFUSED" 2 \
              "$(payload Bash "" 'pmset displaysleepnow  # ceo-ruled-host-power: §99 — "a sentence he never said about the display"')"
expect_says   "H5  ... and the refusal names the section and the file" "§99 is NOT in" \
              "$(payload Bash "" 'pmset displaysleepnow  # ceo-ruled-host-power: §99 — "a sentence he never said about the display"')"
expect_rc     "H6  a BARE marker exempts nothing" 2 \
              "$(payload Bash "" "pmset displaysleepnow  # ceo-ruled-host-power: yes")"
expect_rc     "H7  a section with no quotation exempts nothing" 2 \
              "$(payload Bash "" "pmset displaysleepnow  # ceo-ruled-host-power: §65 covers this")"
expect_rc     "H8  a quotation too short to be a sentence exempts nothing" 2 \
              "$(payload Bash "" 'pmset displaysleepnow  # ceo-ruled-host-power: §65 — "it is fine"')"
expect_rc     "H8b P3-31: an invented quotation next to a real section is REFUSED" 2 \
              "$(payload Bash "" 'pmset displaysleepnow  # ceo-ruled-host-power: §65 — "the CEO said any agent may sleep the display"')"
: >"$ACKLOG"
run "$(payload Bash "" "pmset displaysleepnow  # ceo-ruled-host-power: yes")"
if [ ! -s "$ACKLOG" ]; then ok "H9  a REFUSED citation is not logged"
else bad "H9  a REFUSED citation is not logged" "$(head -1 "$ACKLOG")"; fi
# A citation on a CLEAN call is never logged: the hatch is read only when there
# is something to exempt, so a stale marker in a brief cannot fill the ledger.
: >"$ACKLOG"
expect_silent "H10 a citation on a clean call is silent" \
              "$(payload Bash "" "pmset -g  # ceo-ruled-host-power: §65 — \"No agent, and no brief, message or script Rich writes\"")"
if [ ! -s "$ACKLOG" ]; then ok "H11 ... and is NOT logged"
else bad "H11 ... and is NOT logged" "$(head -1 "$ACKLOG")"; fi

# --- (i) fail open ---------------------------------------------------------
printf 'not json at all' >"$SB/garbage.json"
set +e
OUT="$(env RICHOS_ENTITY_ROOT="$ENTITY" "$HOOK" <"$SB/garbage.json" 2>&1)"; RC=$?
set -e
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q 'NOT checked'; then ok "I1  an unparseable payload ALLOWS and says so"
else bad "I1  an unparseable payload ALLOWS and says so" "rc=$RC: ${OUT:0:200}"; fi

expect_silent "I2  a call for another tool is silent" "$(payload Read "" "/tmp/x")"

NOADOPT="$SB/noadopt"
mkdir -p "$NOADOPT"
payload Bash "" "$TESTVM_503" >"$SB/noadopt.json"
set +e
OUT="$(cd "$NOADOPT" && env -u RICHOS_ENTITY_ROOT -u CLAUDE_PROJECT_DIR "$HOOK" <"$SB/noadopt.json" 2>&1)"; RC=$?
set -e
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then ok "I3  an unadopted repository is silent"
else bad "I3  an unadopted repository is silent" "rc=$RC: ${OUT:0:200}"; fi

set +e
OUT="$(env RICHOS_ENTITY_ROOT="$NOADOPT" "$HOOK" <"$SB/noadopt.json" 2>&1)"; RC=$?
set -e
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q 'GUARD IS OFF'; then ok "I4  an unresolvable declared root ALLOWS and says so"
else bad "I4  an unresolvable declared root ALLOWS and says so" "rc=$RC: ${OUT:0:200}"; fi

printf 'PROTECTED_PATHS=""\nCEO_TODOS_REPOS="%s"\nHOST_DISPLAY_POWER_GUARD="off"\n' "$HQ" >"$ENTITY/orchestration.config"
expect_silent "I5  HOST_DISPLAY_POWER_GUARD=off stands down silently" "$(payload Bash "" "$TESTVM_503")"
printf 'PROTECTED_PATHS=""\nCEO_TODOS_REPOS="%s"\n' "$HQ" >"$ENTITY/orchestration.config"
expect_rc     "I6  ... and removing the switch restores the refusal" 2 "$(payload Bash "" "$TESTVM_503")"

# --- (j) the self-exemption ------------------------------------------------
expect_silent "J1  the guard's own source is exempt" \
              "$(payload Write "$SB/scripts/hooks/guard-host-display-power.sh" "$BRIEF_0918")"
expect_silent "J2  its suite is exempt" \
              "$(payload Write "$SB/scripts/hooks/guard-host-display-power.test.sh" "$BRIEF_0918")"
expect_silent "J3  its mutation harness is exempt" \
              "$(payload Write "$SB/scripts/hooks/host-display-power.mutation.sh" "$BRIEF_0918")"
expect_silent "J4  its corpus is exempt" \
              "$(payload Write "$SB/scripts/hooks/host-display-power.corpus.md" "$BRIEF_0918")"
expect_rc     "J5  a file merely NAMED like one, elsewhere, is not exempt by directory" 2 \
              "$(payload Write "$SB/scripts/hooks/guard-host-display-powers.sh" "$BRIEF_0918")"

# --- (k) the one hard refusal ----------------------------------------------
BROKEN="$SB/broken"
mkdir -p "$BROKEN/scripts/hooks" "$BROKEN/scripts/lib"
cp "$HOOK" "$BROKEN/scripts/hooks/"
payload Bash "" "$TESTVM_503" >"$SB/t503.json"
set +e
OUT="$(env RICHOS_ENTITY_ROOT="$ENTITY" bash "$BROKEN/scripts/hooks/guard-host-display-power.sh" <"$SB/t503.json" 2>&1)"; RC=$?
set -e
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q 'BROKEN INSTALL'; then ok "K1  a missing resolve-roots.sh REFUSES with the shared banner"
else bad "K1  a missing resolve-roots.sh REFUSES with the shared banner" "rc=$RC: ${OUT:0:200}"; fi

# --- the mutation harness --------------------------------------------------
# A suite that cannot fail proves nothing. The harness breaks the guard in ways
# a real edit could and requires this suite to go red for each.
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -x "$SCRIPT_DIR/host-display-power.mutation.sh" ]; then
    printf '\n--- mutation harness ---\n'
    if "$SCRIPT_DIR/host-display-power.mutation.sh"; then
        PASS=$((PASS + 1)); printf '  PASS  M1  the suite FAILS on every planted defect\n'
    else
        FAIL=$((FAIL + 1)); printf '  FAIL  M1  the suite FAILS on every planted defect\n'
    fi
fi

printf '\n%s passed, %s failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
