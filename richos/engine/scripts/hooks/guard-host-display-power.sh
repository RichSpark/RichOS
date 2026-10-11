#!/usr/bin/env bash
#
# guard-host-display-power.sh — BLOCKING PreToolUse guard on FOUR surfaces:
# Bash and Write|Edit|MultiEdit|NotebookEdit (through dispatch-pretooluse.sh),
# Agent, and SendMessage.
#
# REFUSES A COMMAND — OR A BRIEF, A MESSAGE, A SPAWN OR A SCRIPT THAT CARRIES
# ONE — THAT CHANGES THIS MAC'S DISPLAY, SLEEP, LOCK, SESSION OR INPUT STATE.
#
# ===========================================================================
# THE RULING, AND THE FAILURE IT WAS GIVEN FOR
# ===========================================================================
# ceo-decisions.md §65 (2026-09-19). His words at 19:10Z:
#
#     "So, this is why all of the monitors went black and there was no way for
#      me to switch them back on? I'm lucky that locking the screen with the
#      keyboard button worked and that unlocking brought back the screens on.
#      But a horrible fucking experience that! I was already considering
#      killing the Mac with the power button!"
#
# WHAT HAPPENED, re-derived from the transcripts rather than recalled, because
# the shape of the failure decides this file's surfaces:
#
#   18:38:49Z  Rich SENDS zach-opus-testvm1 a mid-task correction:
#              "the completion proof ... must be captured while the HOST's
#               screen is locked (`pmset displaysleepnow` or the lock shortcut
#               via the CEO — if you cannot lock it yourself without a
#               keystroke, say so and I will ask him to lock it for the test)"
#              and, in the same message, "The VM host process runs under
#              `caffeinate -dimsu` (or equivalent)".
#   18:45:03Z  The teammate CORRECTS the caffeinate half by hand — "-d (prevent
#              DISPLAY sleep) and -u (declare user active) ... would keep the
#              CEO's screen lit and awake permanently" — and ships `-is`.
#   18:51:27Z  It runs `pmset displaysleepnow` as the first line of a capture.
#   18:51:31Z  It runs `pmset displaysleepnow` again, four seconds later.
#   19:10Z     The CEO, at the machine, asks the question above.
#
# TWO EXECUTIONS, not three. §65's own account says "at least three times" and
# the brief that ordered this file says "7 mentions ... at least 3 executions";
# the transcript holds exactly two Bash tool calls carrying that command
# (agent-acb0a69e78f52de0b.jsonl, lines 499 and 503). The seven were JSONL
# lines, which count tool results and echoes. The correction is recorded here
# because a number in the record is a claim with a date on it, and nothing in
# this guard's design rests on which of two and three it was.
#
# THE SURFACE THE INCIDENT ACTUALLY USED WAS `SendMessage`, AND THAT IS WHY
# THERE ARE FOUR OF THEM. zach-opus-testvm1's spawn prompt does not contain the
# word pmset, or sleep, or lock, or display — checked, all 5,845 characters of
# it. The command reached the teammate through the mailbox. A guard wired only
# on Bash, Agent and Write — which is what this file was commissioned as —
# would not have seen the instruction at all; it would have caught only the
# teammate's own Bash call, which is the last line of defense and not the
# first. §65's rule text names the vector in as many words: "no brief, MESSAGE
# or script Rich writes". So SendMessage is a surface here.
#
# ===========================================================================
# WHAT IT REFUSES — ENUMERATED FROM THE MAN PAGES ON THIS MACHINE
# ===========================================================================
# Not from memory, and the difference showed up twice.
#
#   pmset(1)      ANY invocation whose first argument is not `-g`. The synopsis
#                 is `pmset [-a|-b|-c|-u] [setting value]`, `pmset -g [option]`,
#                 `pmset schedule ...`, `pmset repeat ...`, `pmset relative ...`,
#                 `pmset [touch | sleepnow | displaysleepnow | boot]`. EVERY
#                 form except `-g` writes: the four source flags set timers,
#                 `schedule`/`repeat`/`relative` arm power events, and the bare
#                 verbs act immediately. `relative` is in the synopsis and in
#                 none of the lists this guard was commissioned with — it
#                 schedules a wake or a poweron, and the man page says the
#                 event "cannot be canceled". So the rule is stated as the man
#                 page states it: `-g` reads, everything else does not. `pmset`
#                 with no arguments prints usage and is allowed.
#
#   caffeinate(8) `-u` and `-d`, and ONLY those two.
#                   -u  "If the display is off, this option turns the display
#                        ON and prevents the display from going into idle
#                        sleep." That is a display state change, full stop.
#                   -d  "prevent the display from sleeping" — it cannot black a
#                        screen out, and refusing it therefore does not follow
#                        from the incident. It follows from the OTHER half of
#                        the same message: held with no utility it persists
#                        until caffeinate exits, which for a backgrounded agent
#                        is forever, and the CEO's display-sleep setting stops
#                        working on his own machine. The teammate reached that
#                        conclusion by hand at 18:45:03Z and swapped `-dimsu`
#                        for `-is`. This encodes what it worked out.
#                   -i -m -s -t -w  ALLOWED, with or without a utility. None of
#                        them touches the display; `caffeinate -is <cmd>` is the
#                        shipped form and stays shipped. A guard that made the
#                        safe alternative harder would be waived by lunchtime.
#
#   osascript(1)  a script text that tells the host to sleep, shut down,
#                 restart, log out, lock the screen, start the screen saver, or
#                 that sends a MODIFIER KEYSTROKE — `keystroke`/`key code` with
#                 a `using ... down` clause.
#
#   killall/pkill of loginwindow, WindowServer, Dock, Finder, SystemUIServer,
#                 ScreenSaverEngine, Terminal, iTerm2. NOTE `SystemUIServer` —
#                 the commissioning list spelled it `SystemUIService`, which is
#                 not a process on this machine; both spellings are matched and
#                 only one of them exists.
#
#   `kill` IS NEVER A TRIGGER, and that is a design decision rather than an
#                 omission. `kill -TERM <pid>` is the PRESCRIBED way to quit a
#                 test instance (see the Command-Q paragraph below). A guard
#                 that caught the remedy along with the defect teaches its
#                 reader to route around it.
#
#   shutdown, reboot, halt          always.
#   launchctl reboot | bootout gui/…  always.
#   defaults write|delete com.apple.{screensaver,loginwindow,PowerManagement,dock}
#   nvram <name>=<value> | -d | -c  (`nvram -p` and a bare read are allowed).
#   systemsetup -set{displaysleep,computersleep,harddisksleep,sleep,restart…,
#                 wakeonnetworkaccess,allowpowerbutton…}  (`-get…` allowed).
#   open -a ScreenSaverEngine, or executing any path naming ScreenSaverEngine.
#   displayplacer, brightness, betterdisplaycli, CGSession -suspend.
#
# NOT INSTALLED ON THIS MACHINE, stated rather than implied: `displayplacer`,
# `brightness`, and the `CGSession` binary under Menu Extras — checked with
# `command -v` and `ls`. They are matched anyway because they are one `brew
# install` away and the pattern costs nothing, and because a guard that only
# knows the binaries that happen to be present today is a guard with a shelf
# life. No test can execute them, so no test pretends to.
#
# THE COMMAND-Q CASE, AND THE 42 COMMANDS IT MAKES RED
# ----------------------------------------------------
# The same day, an agent's `osascript … keystroke "q" using command down` went
# to the CEO's Terminal — it had been addressed at a specific unix id, and
# System Events delivers to whatever is FRONTMOST, so when the target had
# already exited the key landed on him and offered to terminate the session.
#
# MEASURED, because this is the one pattern with a real incumbent population:
# 42 Bash calls across this project's transcripts send a modifier keystroke
# through System Events, and every one of them is an agent quitting its own
# on-screen test instance — the procedure §54 addendum 4 requires. They are not
# collateral damage; they are the practice §65 ends. The refusal below
# therefore CARRIES THE REPLACEMENT rather than pointing at a rule: quit by pid
# with `kill -TERM`, confirm with `pgrep`. A guard that only says no to the
# only method somebody knows gets waived, and habitual waiving is how a guard
# dies.
#
# WHAT IS DELIBERATELY NOT HERE
# -----------------------------
#   * An UNMODIFIED `keystroke`/`key code`. `key code 53` (Escape) addressed at
#     a frontmost app is the walk idiom and is not a session-state change; §65
#     says "no keystroke with a MODIFIER to the host" and this matches that.
#   * `osascript -` / `osascript <<EOF` reading a script from STDIN. The script
#     text is not in the payload and this guard will not pretend to have read
#     it. testvm/ax.sh uses exactly that form, deliberately, and is exempt for
#     a better reason (below).
#   * Volume, wallpaper, notification state. Real annoyances; not display,
#     sleep, lock, session or input, which is what §65 names.
#   * `xargs`, `find -exec`, `watch`, and every other indirect executor except
#     `sh|bash|zsh|dash|ksh -c`, which IS followed. Named so the gap is a known
#     one rather than a discovered one.
#
# ===========================================================================
# THE GUEST EXEMPTION IS STRUCTURAL, NOT TEXTUAL
# ===========================================================================
# §65 permits these state changes "only inside a test VM over ssh", so the
# exemption has to be real — and "the string appears after an `ssh`" is not it.
# `ssh vm 'true'; pmset displaysleepnow` satisfies that test and blacks out the
# CEO's screen.
#
# So the command is TOKENIZED instead. The text is split into shell segments at
# unquoted `;  && || | & newline ( ) { }`, command substitutions are pulled out
# and analyzed as segments of their own, and each segment is resolved to its
# COMMAND WORD after stripping `VAR=value` assignments and the wrappers `sudo
# doas env nohup time nice stdbuf setsid command builtin exec`. A segment is
# guest-bound when THAT WORD is `ssh`/`scp`/`sftp`, or a script under a
# `testvm/` path segment. Everything the segment carries after that runs on the
# far side of the connection, and nothing before it does — because there is
# nothing before it, the command word is the first word.
#
# TWO CONSEQUENCES WORTH STATING:
#
#   * `testvm/ax.sh <vm> …` is exempt BY CONSTRUCTION, verified by reading it
#     rather than by trusting its name: every path that reaches a SCREEN ends at
#     `guest_ssh`, which is `ssh "${TESTVM_SSH_OPTS[@]}" … "$user@$ip"`
#     (lib.sh) — the AppleScript form at `osascript -`, and the `tree`/`find`/
#     `click` forms at `osascript -l JavaScript -`.
#     CORRECTED 2026-09-20, when those three subcommands were added: it is no
#     longer true that ax.sh runs NOTHING locally. They build their parameter
#     block and render the guest's answer with `python3 -c` on this Mac, and
#     take one `mktemp` scratch file. None of that touches display, power,
#     session or input, so the exemption still holds — but it holds because the
#     local branches are pure text, not because there are none. If a local
#     branch in there ever grows teeth, this is the sentence that changes with
#     it.
#   * `ssh -o ProxyCommand='…'` and `-o LocalCommand='…'` DO run locally, so an
#     ssh segment is still scanned for those two option values and they are
#     analyzed as commands. Without this, the word `ssh` would be a bypass with
#     an option flag on it.
#
# The suite's negative controls are the wrapped forms: a host command after an
# ssh segment, a host command inside `bash -c`, a host command behind an
# `ssh`-shaped environment assignment, and a ProxyCommand. All refused.
#
# ===========================================================================
# PROSE: THE RECORD OF THE RULE MUST BE WRITEABLE
# ===========================================================================
# On Agent, SendMessage and Write|Edit the payload is text, not a command, and
# the failure mode to avoid is the one that cost this engine fourteen lines in
# two vendored skills on 2026-08-30: a guard that forbids the record of what it
# forbids. §65 itself quotes `pmset displaysleepnow`, `caffeinate -d`/`-u` and
# `killall loginwindow`, and it MUST be writeable.
#
#   (a) A CANDIDATE COMMAND is every occurrence of a trigger word, taken to the
#       end of its line or to the closing backtick, and then judged by the same
#       shell rules the Bash surface uses. So "no `pmset` other than `pmset -g`"
#       yields two candidates and neither is refused, while "`pmset
#       displaysleepnow` or the lock shortcut" yields one that is.
#
#       IN PROSE, `shutdown reboot halt restart defaults open brightness` are
#       triggers ONLY INSIDE A CODE SPAN OR FENCE. They are ordinary English
#       and this record is full of them — "shutdown_request", "teammate
#       shutdown", "the defaults", "open the file". The other triggers (pmset,
#       caffeinate, osascript, killall, pkill, launchctl, nvram, systemsetup,
#       displayplacer, CGSession, ScreenSaverEngine) are not words anybody
#       writes by accident and fire anywhere.
#
#   (b) SCOPE IS THE BLOCK in prose and THE LINE in code, for the reason the
#       sibling guard measured: hard-wrapping is a formatting choice and must
#       not change a verdict, while joining adjacent SOURCE lines lets one
#       line's comment exempt the next line's command. A fenced code block is
#       one unit, and the paragraph immediately above it contributes its cues —
#       "the guard refuses:" followed by a fence is how every such passage in
#       this record is actually written.
#
#   (c) A BLOCK THAT IS RECORDING RATHER THAN INSTRUCTING PASSES. The cue list
#       is the negation/refusal vocabulary: never, no, not, n't, nothing, none,
#       refuses, banned, forbidden, blocked, prevented, impossible, instead,
#       zero, went black, and the words beside them.
#
#   (d) AND A PERMISSIVE CONSTRUCTION VETOES (c). This is not decoration; it is
#       the half that makes the difference on the two real texts:
#
#         echo-brief-wait-for-screen-unlock-2026-09-18.md, line 47 —
#           "(YOU MAY lock the screen with `pmset displaysleepnow` only when
#            Rich says the screen is free — ask first...)"
#         the block it sits in also says "only if NO candidate is on screen",
#         about something else entirely. Cue-only, it exempts itself.
#
#         the 18:38:49Z SendMessage —
#           "...(`pmset displaysleepnow` or the lock shortcut via the CEO — IF
#            YOU CANNOT lock it yourself without a keystroke, say so...)"
#         "cannot" and "without" are both cues, in a sentence handing over the
#         command. Cue-only, it exempts itself too.
#
#       So `you may|can|could|should`, `feel free`, `go ahead`, `it is fine/ok/
#       safe`, `if you can|cannot`, `or equivalent`, `runs under`, and `use|run|
#       using|with <trigger>` cancel the exemption. Every entry on that list was
#       taken from a sentence that exists, and the two above are in the suite.
#
# NOTE what (d) means for the 2026-09-18 brief: it is REFUSED, and it was
# correct when it was written — §65 was ruled on the 19th. The rule is right
# and the document predates it, which is the same thing its sibling found and
# the same reason to say it out loud rather than quietly re-date the finding.
#
# ===========================================================================
# THE ESCAPE HATCH IS A QUOTATION, NOT A MARKER
# ===========================================================================
#     ceo-ruled-host-power: §<N> — "<his verbatim sentence>"
#
# on its own line. It needs a section token, a quoted sentence of at least 25
# characters, AND THE SECTION MUST EXIST in the CEO's record — the guard opens
# wiki/ceo-decisions.md through cr_resolve(), the engine's single declaration
# of where his rulings live. A bare marker exempts nothing. Where the record
# cannot be reached at all the citation is accepted on its shape and logged
# UNVERIFIED: "I cannot check" is never "forbidden".
#
# Accepted uses append to <entity root>/.claude/state/host-power-acks.log,
# beside the other opt-out ledgers, so a habit of waiving is visible.
#
# WHY A QUOTATION HERE TOO. The instruction that caused this was not careless —
# it was careful, hedged, and conditional ("only when Rich says the screen is
# free", "if you cannot lock it yourself, say so"). A free-text reason field
# would have been filled in exactly that well. A citation that must resolve
# against the file cannot be.
#
# ===========================================================================
# THE SELF-EXEMPTION, DECLARED RATHER THAN HIDDEN
# ===========================================================================
# This file, its suite, its mutation harness and its corpus are exempt BY
# BASENAME — four names, checked against the basename and never a directory,
# listed here rather than in a config key so that adding to it is an edit
# somebody reviews. Without it this guard could not be authored, tested or
# repaired, and a guard that cannot be repaired is one somebody deletes.
#
# ceo-decisions.md IS NOT ON THAT LIST, deliberately. §65 passes on MERIT, and
# the suite proves it by driving §65 through this guard.
#
# ===========================================================================
# FAIL OPEN, AND LOUDLY
# ===========================================================================
#   NOT ADOPTED (no orchestration.config)     -> STAND DOWN, silent.
#   HOST_DISPLAY_POWER_GUARD="off"            -> STAND DOWN, silent.
#   NO TRIGGER WORD ANYWHERE                  -> SILENT, and this is the large
#     majority of the 1,097 Bash calls measured beside this file.
#   PAYLOAD UNPARSEABLE, python3 MISSING      -> ALLOW + ANNOUNCE. This guard
#     can read neither the command nor the hatch from such a call.
#   THE CEO RECORD UNREADABLE                 -> the hatch is accepted on its
#     shape and logged UNVERIFIED. It never turns a pass into a refusal.
#
# The one thing that is NOT fail-open: scripts/lib/resolve-roots.sh missing.
#
# ===========================================================================
# MEASURED, ON REAL TEXT, BEFORE IT WAS WIRED
# ===========================================================================
# 1,632 real payloads, driven through THIS FILE as Bash and Write calls — so
# the measurement is of the shipped rule and not of a model of it. The full
# listing, with the reason each finding is right, is in
# host-display-power.corpus.md beside this file.
#
#   1,097 Bash commands  — every `Bash` tool call in the session that ordered
#                          this guard.  19 REFUSED, 1,078 silent.
#     10  `caffeinate -dimsu python3 scripts/nightly-local.py build …` — the
#         nightly candidate build, twenty minutes at a time, holding `-u` (turn
#         the display ON, declare the user active) on the CEO's own Mac. Right,
#         and the highest-value catch in the whole measurement: nothing
#         anywhere was watching for it, and `-is` does the job.
#      9  `osascript … keystroke "q" using command down` — the Command-Q quit,
#         including three of Rich's own. Right; §65 bans it by name.
#      0  anything else.
#
#     535 documents — all 248 committed briefs, the whole of ceo-decisions.md,
#                     CLAUDE.md, and every memory note.  2 REFUSED.
#         echo-brief-wait-for-screen-unlock-2026-09-18.md:47 — "you may lock
#         the screen with `pmset displaysleepnow`", the sentence that put the
#         command in a teammate's hands. Written the day BEFORE §65: the rule
#         is right and the document predates it.
#         zach-brief-candidate-build-time-2026-09-19.md:5 — `caffeinate -dimsu`
#         in a brief. Right.
#         NOTHING ELSE. §65 itself passes, on merit, with no path exemption.
#
# THE ONE MISS, STATED RATHER THAN DISCOVERED LATER. In
# echo-brief-front-door-suite-in-the-build-2026-09-18.md two blocks carry
# `caffeinate -dimsu` as an instruction and are EXEMPTED, because each block
# also quotes a test runner — "REFUSED: --bundle must name a…", "the runner
# must refuse". The cue test cannot tell a refusal ABOUT this command from a
# refusal about something else, and the fix is not available: `refus…` is the
# central verb of this record, and dropping it would refuse §65 itself.
#
# THAT MISS IS AFFORDABLE, AND SAYING WHY IS THE DESIGN. The prose layer is
# DEFENSE IN DEPTH and is tuned toward permissiveness on purpose — its failure
# mode is forbidding the record, which is unrecoverable. The SHELL layer is the
# airtight one: it applies no cue test to a real command, and it refused that
# same brief's `caffeinate -dimsu` four separate times when somebody ran it.
#
# MEASURED AND NOT LOAD-BEARING: removing `\bfail(?:s|ed|ure)?\b` from the cue
# list changes NO verdict on any of the 1,632 payloads. It is kept as
# vocabulary, not as a proven property, and it is named here so a later reader
# does not mistake it for one.
#
# NOTE: hooks are snapshotted at session start. This one is INERT until the
# next session — it assumes nothing about being live in the session that adds
# it, which is the session that wrote the brief it refuses.

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
  hook: scripts/hooks/guard-host-display-power.sh
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
    announce_off "HOST DISPLAY/POWER GUARD IS OFF: python3 is not on PATH, so this call was NOT checked for a command that changes this Mac's display, sleep, lock or session state (ceo-decisions §65)."
    exit 0
fi

if ! resolve_entity_root "$INPUT"; then
    if [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
        exit 0
    fi
    announce_off "HOST DISPLAY/POWER GUARD IS OFF: it cannot tell which repository it governs (${RICHOS_ROOT_REASON:-root resolution failed}). Nothing is checking whether this call changes this Mac's display, sleep, lock or session state."
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
    unevaluated_or_continue "guard-host-display-power.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this call changes this Mac's display, sleep, lock or session state"
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"

# --- THE OFF SWITCH, AND WHY THERE IS NO ON SWITCH -------------------------
# Unset means ON, for the reason §65 was written: the CEO's Mac reaches him
# physically and a contract nobody has switched on yet is a contract
# remembered. The trigger set is narrow enough to ship that way — the large
# majority of the 1,097 measured Bash calls never reach a rule at all.
: "${HOST_DISPLAY_POWER_GUARD:=on}"
case "$HOST_DISPLAY_POWER_GUARD" in
    off|OFF|0|no|NO) exit 0 ;;
esac
# The section the refusal cites. Data, not prose, so a repository whose ruling
# is numbered differently cites its own.
: "${HOST_DISPLAY_POWER_RULING:=§65}"

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
case "$TOOL_NAME" in
    Bash|Write|Edit|MultiEdit|NotebookEdit|Agent|SendMessage) ;;
    *) exit 0 ;;
esac

# --- THE SELF-EXEMPTION ----------------------------------------------------
# Declared in the header. BASENAME only — never a directory, so it cannot be
# widened by moving a file into a folder.
FILE_PATH="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); ti=d.get("tool_input",{}) or {}; print(ti.get("file_path") or ti.get("notebook_path") or "")' 2>/dev/null || true)"
if [ -n "$FILE_PATH" ]; then
    richos_assert_jurisdiction "guard-host-display-power.sh" "$ENTITY_ROOT" "$FILE_PATH" "file" "proceeds" || true
fi
case "$(basename "${FILE_PATH:-}" 2>/dev/null || true)" in
    guard-host-display-power.sh|guard-host-display-power.test.sh|host-display-power.mutation.sh|host-display-power.corpus.md)
        exit 0 ;;
esac

# --- THE CLASSIFIER --------------------------------------------------------
# Prints one of:
#   CLEAN
#   ACK<TAB><the ceo-ruled-host-power: line's argument>
#   FIND<TAB><n>|<rule>|<command>|<why>   (one per line, at most 5)
# then, for FIND runs, a trailing SURFACE<TAB>shell|prose|code line.
VERDICT="$(printf '%s' "$INPUT" | FILE_PATH="$FILE_PATH" python3 -c '
import json, os, re, shlex, sys

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
if tool == "Bash":
    texts.append(str(ti.get("command", "") or ""))
elif tool == "Agent":
    texts.append(str(ti.get("prompt", "") or ""))
    texts.append(str(ti.get("description", "") or ""))
elif tool == "SendMessage":
    m = ti.get("message")
    # A protocol message is a dict; a real one is a string. json.dumps on the
    # dict keeps any text inside it readable without a second code path.
    texts.append(m if isinstance(m, str) else json.dumps(m, ensure_ascii=False))
    texts.append(str(ti.get("summary", "") or ""))
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

# ---- the hatch, read from the RAW text ------------------------------------
# Before anything is stripped or split: on the Bash surface the citation is a
# shell comment, and comment-stripping runs later.
#
# TWO PLACEMENTS, both deliberate. At the START of a line (after any comment
# punctuation) is the form every sibling hatch in this engine uses. AFTER A
# `#` anywhere on the line is accepted too, because on the Bash surface the
# natural way to write it is a trailing comment on the command itself, and a
# hatch that exists but does not work where an operator would reach for it is
# a hatch that gets reported as a broken guard.
ACK_RE = re.compile(r"(?:^[ \t#/*>-]*|#[ \t]*)ceo-ruled-host-power:[ \t]*([^\n]*\S)",
                    re.MULTILINE)
_ack = ACK_RE.search(blob)

# ===========================================================================
# THE SHELL ANALYZER
# ===========================================================================
# Everything below decides ONE question: does this segment RUN a command that
# changes the host state? Not "does this text contain the word".

WRAPPERS = {"sudo", "doas", "env", "nohup", "time", "nice", "stdbuf",
            "setsid", "command", "builtin", "exec"}
# Wrapper flags that CONSUME the next token. `sudo -u alex pmset sleepnow`
# would otherwise resolve to `alex`.
WRAPPER_FLAG_ARG = {"-u", "-g", "-p", "-C", "-r", "-t", "-U", "-n", "-i", "-o"}
SHELLS = {"sh", "bash", "zsh", "dash", "ksh"}
REMOTE = {"ssh", "scp", "sftp"}

def strip_comments(text):
    """Remove unquoted `#`-to-end-of-line. A `#` only starts a comment at the
    start of a word, which is what the shell itself does."""
    out, i, n = [], 0, len(text)
    q = None
    prev = "\n"
    while i < n:
        c = text[i]
        if q:
            if c == "\\" and q == "\x22" and i + 1 < n:
                out.append(c); out.append(text[i+1]); i += 2; prev = "x"; continue
            if c == q:
                q = None
            out.append(c); i += 1; prev = c; continue
        if c in ("\x27", "\x22"):
            q = c; out.append(c); i += 1; prev = c; continue
        if c == "\\" and i + 1 < n:
            out.append(c); out.append(text[i+1]); i += 2; prev = "x"; continue
        if c == "#" and (prev in " \t\n;|&(" or prev == ""):
            while i < n and text[i] != "\n":
                i += 1
            prev = "\n"
            continue
        out.append(c); i += 1; prev = c
    return "".join(out)

SEG_BREAK = set(";|&\n()\x7b\x7d")

def split_segments(text, depth=0):
    """Top-level shell segments, plus command substitutions as segments of
    their own. Quoting is tracked so an operator inside quotes does not split."""
    if depth > 6:
        return [text]
    segs, cur, subs = [], [], []
    i, n, q = 0, len(text), None
    while i < n:
        c = text[i]
        if q:
            if c == "\\" and q == "\x22" and i + 1 < n:
                cur.append(c); cur.append(text[i+1]); i += 2; continue
            if c == q:
                q = None
                cur.append(c); i += 1; continue
            if q == "\x22" and c == "$" and i + 1 < n and text[i+1] == "(":
                j, d2 = i + 2, 1
                while j < n and d2:
                    if text[j] == "(":
                        d2 += 1
                    elif text[j] == ")":
                        d2 -= 1
                    j += 1
                subs.extend(split_segments(text[i+2:j-1], depth + 1))
                cur.append(" "); i = j; continue
            cur.append(c); i += 1; continue
        if c == "\\" and i + 1 < n:
            cur.append(c); cur.append(text[i+1]); i += 2; continue
        if c in ("\x27", "\x22"):
            q = c; cur.append(c); i += 1; continue
        if c == "$" and i + 1 < n and text[i+1] == "(":
            j, d2 = i + 2, 1
            while j < n and d2:
                if text[j] == "(":
                    d2 += 1
                elif text[j] == ")":
                    d2 -= 1
                j += 1
            subs.extend(split_segments(text[i+2:j-1], depth + 1))
            cur.append(" "); i = j; continue
        if c == "`":
            j = text.find("`", i + 1)
            if j < 0:
                j = n
            subs.extend(split_segments(text[i+1:j], depth + 1))
            cur.append(" "); i = j + 1; continue
        if c in SEG_BREAK:
            segs.append("".join(cur)); cur = []; i += 1; continue
        cur.append(c); i += 1
    segs.append("".join(cur))
    return [s for s in segs + subs if s.strip()]

def words(seg):
    try:
        return shlex.split(seg, posix=True)
    except ValueError:
        try:
            return shlex.split(seg + "\x27", posix=True)
        except ValueError:
            return seg.split()

ASSIGN_RE = re.compile(r"^[A-Za-z_][A-Za-z_0-9]*=")

VAR_RE = re.compile(r"^\$\{?[A-Za-z_]")

def resolve(argv):
    """Strip assignments and wrappers; return (argv from the command word, sudoed)."""
    i, sudoed = 0, False
    while i < len(argv):
        w = argv[i]
        if ASSIGN_RE.match(w):
            i += 1; continue
        # AN UNRESOLVABLE $VAR IN COMMAND POSITION IS SKIPPED, NOT TRUSTED.
        # `$SUDO pmset -a displaysleep 0` is the sudo-or-nothing idiom and it
        # is what testvm/provision-guest.sh uses. Treating `$SUDO` as the
        # command word would make every such line invisible to this guard — a
        # hole opened by an idiom, which is the worst kind. We cannot know what
        # it expands to, so we step over it and judge what follows.
        if VAR_RE.match(w):
            i += 1; continue
        base = os.path.basename(w)
        if base in WRAPPERS:
            if base in ("sudo", "doas"):
                sudoed = True
            i += 1
            while i < len(argv):
                t = argv[i]
                if ASSIGN_RE.match(t):
                    i += 1; continue
                if t == "--":
                    i += 1; break
                if t.startswith("-"):
                    if t in WRAPPER_FLAG_ARG and base != "env":
                        i += 2
                    else:
                        i += 1
                    continue
                break
            continue
        break
    return argv[i:], sudoed

PROTECTED_PROCS = {"loginwindow", "windowserver", "dock", "finder",
                   "systemuiserver", "systemuiservice", "screensaverengine",
                   "terminal", "iterm2", "iterm"}
DEFAULTS_DOMAIN = re.compile(
    r"com\.apple\.(?:screensaver|loginwindow|PowerManagement|dock)", re.IGNORECASE)
SYSTEMSETUP_SET = re.compile(
    r"^-set(?:displaysleep|computersleep|harddisksleep|sleep|restart|wakeon|"
    r"allowpowerbutton|remotelogin|usingnetworktime)", re.IGNORECASE)
ALWAYS_BAD = {"shutdown": "halts or restarts this Mac",
              "reboot": "restarts this Mac",
              "halt": "halts this Mac",
              "displayplacer": "reconfigures the attached displays",
              "brightness": "changes display brightness",
              "betterdisplaycli": "reconfigures the attached displays",
              "cgsession": "suspends the login session (fast user switch / lock)"}

OSA_RULES = [
    (re.compile(r"\b(?:to\s+)?sleep\b", re.IGNORECASE), "tells the host to sleep"),
    (re.compile(r"\bshut\s+down\b", re.IGNORECASE), "tells the host to shut down"),
    (re.compile(r"\brestart\b", re.IGNORECASE), "tells the host to restart"),
    (re.compile(r"\blog\s+out\b", re.IGNORECASE), "logs the session out"),
    (re.compile(r"\block\s+screen\b", re.IGNORECASE), "locks the screen"),
    (re.compile(r"\bstart\s+current\s+screen\s+saver\b", re.IGNORECASE),
     "starts the screen saver"),
    (re.compile(r"\bScreenSaverEngine\b", re.IGNORECASE), "starts the screen saver"),
    (re.compile(r"(?:keystroke|key\s+code)\b[\s\S]{0,80}?\busing\b[\s\S]{0,60}?\bdown\b",
                re.IGNORECASE),
     "sends a MODIFIER KEYSTROKE to whatever is frontmost on the host"),
]

def judge(argv, sudoed):
    """(rule, why) for a resolved argv, or None."""
    if not argv:
        return None
    a0 = argv[0]
    name = os.path.basename(a0).lower()
    args = argv[1:]

    if "screensaverengine" in a0.lower() and name != "open":
        return ("ScreenSaverEngine", "runs the screen saver on the host")

    if name == "pmset":
        if not args:
            return None
        if args[0] == "-g":
            return None
        return ("pmset", "every pmset form except `pmset -g` WRITES power state; "
                         "`-g` is the only read")

    if name == "caffeinate":
        _skip = False
        for w in args:
            if _skip:   # P3-23: the value of -t/-w is not the end of the options
                _skip = False
                continue
            if w == "--":
                break
            if not w.startswith("-") or w == "-":
                break
            if w in ("-t", "-w"):
                _skip = True
                continue
            flags = set(w[1:].lower())
            if "u" in flags:
                return ("caffeinate -u", "-u turns the display ON and declares "
                                         "the user active")
            if "d" in flags:
                return ("caffeinate -d", "-d holds the host display awake, "
                                         "overriding the display-sleep setting "
                                         "the CEO chose on his own machine")
        return None

    if name == "osascript":
        script = " \n ".join(args)
        for rx, why in OSA_RULES:
            if rx.search(script):
                return ("osascript", why)
        return None

    if name == "open":
        for w in args:
            if "screensaverengine" in w.lower():
                return ("open", "starts the screen saver on the host")
        return None

    if name in ("killall", "pkill"):
        for w in args:
            if w.startswith("-"):
                continue
            for part in re.split(r"[^A-Za-z0-9_.-]+", w):
                if part and part.lower() in PROTECTED_PROCS:
                    return (name, "kills %s — the window server, the Dock, the "
                                  "login session or the CEO terminal" % part)
        return None

    if name in ALWAYS_BAD:
        return (name, ALWAYS_BAD[name])

    if name == "launchctl":
        if args and args[0].lower() == "reboot":
            return ("launchctl reboot", "restarts the machine or the user session")
        if args and args[0].lower() in ("bootout", "unload"):
            for w in args[1:]:
                if w.lower().startswith("gui/") or "loginwindow" in w.lower():
                    return ("launchctl " + args[0], "tears down the GUI login session")
        return None

    if name == "defaults":
        if any(w.lower() in ("write", "delete", "rename") for w in args) and \
           any(DEFAULTS_DOMAIN.search(w) for w in args):
            return ("defaults", "rewrites the screen-saver, login-window, power "
                                "or Dock preferences on the host")
        return None

    if name == "nvram":
        for w in args:
            if "=" in w or w in ("-d", "-c"):
                return ("nvram", "writes firmware variables — boot and display "
                                 "state survive a restart")
        return None

    if name == "systemsetup":
        for w in args:
            if SYSTEMSETUP_SET.match(w):
                return ("systemsetup", "sets the host sleep, display-sleep, "
                                       "restart or wake policy")
        return None

    return None

def analyze_command(text, depth=0):
    """Findings for a command string: [(rule, snippet, why)]."""
    if depth > 4:
        return []
    found = []
    for seg in split_segments(strip_comments(text)):
        argv = words(seg)
        if not argv:
            continue
        argv, sudoed = resolve(argv)
        if not argv:
            continue
        a0 = argv[0]
        base = os.path.basename(a0)

        # A shell -c argument is a command, and is followed.
        if base in SHELLS:
            for k, w in enumerate(argv[1:], 1):
                if w == "-c" and k + 1 < len(argv):
                    found.extend(analyze_command(argv[k+1], depth + 1))
                    break
            continue

        # GUEST BY CONSTRUCTION. Everything after an ssh/scp/sftp, or anything
        # under a testvm/ path, runs on the far side. The two ssh options that
        # run LOCALLY are still analyzed.
        if base in REMOTE or re.search(r"(?:^|/)testvm/", a0):
            for idx, w in enumerate(argv[1:], 1):
                m = re.match(r"^(?:-o)?\s*(?:ProxyCommand|LocalCommand)=(.*)$", w,
                             re.IGNORECASE)
                if m and m.group(1).strip():
                    found.extend(analyze_command(m.group(1), depth + 1))
                elif re.match(r"^(?:ProxyCommand|LocalCommand)$", w, re.IGNORECASE) \
                        and idx + 1 < len(argv):
                    found.extend(analyze_command(argv[idx+1], depth + 1))
            continue

        v = judge(argv, sudoed)
        if v:
            snip = " ".join(argv)[:110]
            found.append((v[0], snip, v[1]))
    return found

# ===========================================================================
# THE PROSE / CODE SURFACES
# ===========================================================================
# A trigger word starts a CANDIDATE COMMAND, taken to end of line or to the
# closing backtick, and judged by the analyzer above.
HARD_TRIGGERS = r"pmset|caffeinate|osascript|killall|pkill|launchctl|nvram|systemsetup|displayplacer|betterdisplaycli|CGSession|ScreenSaverEngine"
SOFT_TRIGGERS = r"shutdown|reboot|halt|defaults|brightness"
HARD_RE = re.compile(r"\b(?:%s)\b" % HARD_TRIGGERS)
SOFT_RE = re.compile(r"\b(?:%s)\b" % SOFT_TRIGGERS)
CODESPAN_RE = re.compile(r"`([^`\n]+)`")

NEG_RE = re.compile("|".join([
    r"\bnever\b", r"\bnot\b", r"\bno\b", r"\bnone\b", r"\bnothing\b",
    r"n\x27t\b", r"\bcannot\b", r"\bwithout\b", r"\bzero\b",
    r"\brefus(?:e|es|ed|al|als)\b", r"\bban(?:s|ned)?\b", r"\bforbid(?:s|den)?\b",
    r"\bblock(?:s|ed|ing)?\b", r"\bprohibit(?:s|ed)?\b", r"\bprevent(?:s|ed)?\b",
    r"\bimpossible\b", r"\binstead\b", r"\bremoved?\b", r"\bstopped?\b",
    r"\bwent black\b", r"\bincident\b", r"\bfail(?:s|ed|ure)?\b",
    r"\bstale\b", r"\bobsolete\b", r"\bsuperseded\b",
]), re.IGNORECASE)

# PERMISSIVE CONSTRUCTIONS THAT VETO THE EXEMPTION. Every entry is taken from
# a sentence that exists, and the list is SHORT ON PURPOSE.
#
# A SIXTH ENTRY WAS REMOVED BEFORE THIS SHIPPED, and the removal is the useful
# part. It read `(?:use|run|using|with|via)\s+<trigger>` — meant to catch "lock
# the screen WITH `pmset displaysleepnow`". It also catches "NEVER RUN `pmset
# displaysleepnow`", "do not USE `caffeinate -d`", and "INSTEAD OF RUNNING
# `pmset`" — which is to say it vetoed the exemption on precisely the sentences
# the exemption exists for. A code comment reading `# never run pmset
# displaysleepnow on the host; quit by pid instead` was refused by it.
#
# It also earned nothing: both real permissive sentences are caught by entries
# that remain — "YOU MAY lock the screen with ..." by the first, "IF YOU CANNOT
# lock it yourself ..." by the second, "RUNS UNDER `caffeinate -dimsu` (OR
# EQUIVALENT)" by the fifth and sixth. A veto that fires on the record and
# never on the instruction is a false positive with no upside.
#
# The lookahead on the first entry is the same lesson one clause down: "you may
# NOT run it" is a prohibition wearing a permission.
PERMIT_RE = re.compile("|".join([
    r"\byou (?:may|can|could|should)\b(?!\s*(?:not|never))",
    r"\bif you (?:can|cannot|can\x27t)\b",
    r"\bfeel free\b", r"\bgo ahead\b",
    r"\bit(?:\x27s| is) (?:fine|ok|okay|safe)\b",
    r"\bor equivalent\b", r"\bruns? under\b",
]), re.IGNORECASE)

def candidates(line, in_code):
    """Candidate command strings out of one line.

    THE TWO SURFACES ASK DIFFERENT QUESTIONS AND THE SPLIT IS THE DESIGN.

    CODE is already a command. The whole line goes to the shell analyzer, which
    requires the dangerous binary to be in COMMAND POSITION — the same test the
    Bash surface applies, for the same reason. So a comment is not a command
    (`strip_comments` eats `#`, and `// handy: pmset …` resolves to the command
    word `//`), and a MENTION is not an invocation: `brew install tesseract
    displayplacer` has the command word `brew`.

    WHY THAT MATTERS, measured rather than argued: with a word-scan here — take
    every trigger word to end of line and judge that — the newly landed
    `testvm/provision-guest.sh` was refused for the `displayplacer` in its
    `brew install` line. An engineer editing that file would have been blocked
    on every save, which is how a guard gets switched off.

    A NAMED GAP, so it is a known one: command position cannot see a host
    command embedded in a non-shell source as data — `Command::new("pmset")
    .arg("displaysleepnow")` in Rust resolves to no command word this
    understands. The word-scan did not catch it either (it produced
    `pmset").arg("displaysleepnow")`, whose basename is not `pmset`), so
    nothing was lost here; it is simply not covered, and the Bash surface
    catches the process when it actually runs.

    PROSE has no command position. "run pmset displaysleepnow" is the shape, so
    every trigger word starts a candidate that runs to the end of the line or
    the closing backtick.
    """
    if in_code:
        return [line]
    out = []
    for m in CODESPAN_RE.finditer(line):
        inner = m.group(1)
        if HARD_RE.search(inner) or SOFT_RE.search(inner):
            out.append(inner)
    scan = CODESPAN_RE.sub(lambda mo: " " * len(mo.group(0)), line)
    for m in HARD_RE.finditer(scan):
        out.append(scan[m.start():])
    return out

# A TRAILING BACKSLASH IS ONE COMMAND, NOT TWO LINES.
# ------------------------------------------------------------------
# Code is scored per line, and that is right — joining ADJACENT source lines is
# the mistake the sibling guard measured, where the negation on one line exempts
# the code on the next. A line continuation is the opposite case: it is an EXPLICIT,
# unambiguous shell operator saying "this is the same command", not a
# formatting choice anybody could have made differently.
#
# MEASURED. `testvm/setup.sh` reboots the GUEST:
#     ssh "${TESTVM_SSH_OPTS[@]}" -i "$TESTVM_SSH_KEY" "$USER@$IP" \
#       "sudo shutdown -r now" >/dev/null 2>&1 or true
# Per physical line the continuation loses its `ssh`, and a guest reboot is
# read as a shutdown of HIS Mac. Joined, the command word is
# `ssh` and it is guest-bound by construction, which is what it is.
def join_continuations(lines):
    """[(line-number, logical line)] — a backslash-continued run becomes ONE
    entry carrying the number of the line it STARTED on, so a reported line
    number still points where the reader is looking."""
    out, buf, at = [], None, 0
    for n, raw in enumerate(lines, 1):
        if buf is None:
            buf, at = raw, n
        else:
            buf = buf + " " + raw.lstrip()
        if buf.rstrip().endswith("\\"):
            buf = buf.rstrip()[:-1]
            continue
        out.append((at, buf))
        buf = None
    if buf is not None:
        out.append((at, buf))
    return out

FENCE_RE = re.compile(r"^\s*(?:```|~~~)")
NEW_BLOCK_RE = re.compile(r"^\s*(?:#{1,6}\s|[-*+]\s|\d+[.)]\s|\|)")

def text_findings(text, prose, base=0):
    """Findings in a DOCUMENT: a brief, a message, a source file, a heredoc.

    BLOCKS. In prose a block is what one author wrote as one thought, so
    wrapped lines join and a heading, a list item, a table row or a blank line
    starts a new one. In code the block is the LINE. A fenced code block is one
    unit and inherits the cue text of the paragraph above it.
    """
    # CODE IS ITS OWN PATH, and short. A source file has no headings, no list
    # items and no hard-wrapping to undo — the only thing it has that a line
    # split gets wrong is the backslash continuation, which is an explicit
    # operator rather than a formatting choice. So: join continuations, then
    # every logical line is its own block and its own candidate.
    #
    # THIS USED TO SHARE THE PROSE WALKER AND THE JOINING RAN INSIDE THE
    # PER-BLOCK LOOP — where, in code, each block is exactly one line, so there
    # was never anything to join and the joining did nothing at all. F16 caught
    # it: the guest reboot in testvm/setup.sh was still refused.
    if not prose:
        out, seen = [], set()
        for n, line in join_continuations(text.split("\n")):
            if NEG_RE.search(line) and not PERMIT_RE.search(line):
                continue
            # Through candidates() rather than straight to analyze_command, so
            # that BOTH surfaces have one candidate-extraction entry point and
            # the whole-line-is-the-command rule of the code surface lives in
            # one place where a mutant can remove it.
            for cand in candidates(line, True):
                for rule, snip, why in analyze_command(cand):
                    key = (base + n, rule, snip)
                    if key in seen:
                        continue
                    seen.add(key)
                    out.append((base + n, rule, snip, why))
        return out

    raws = text.split("\n")
    blocks, cur, start, in_fence = [], [], 0, False
    fence_cue = [""]

    def flush(extra=""):
        if cur:
            blocks.append((start, list(cur), " ".join(cur) + " " + extra))

    for n, raw in enumerate(raws, 1):
        if FENCE_RE.match(raw):
            if not in_fence:
                fence_cue[0] = blocks[-1][2] if blocks else ""
                flush(); del cur[:]
            else:
                flush(fence_cue[0]); del cur[:]
                fence_cue[0] = ""
            in_fence = not in_fence
            continue
        if in_fence:
            if not cur:
                start = n
            cur.append(raw)
            continue
        if not raw.strip():
            flush(); del cur[:]
            continue
        if cur and (not prose or NEW_BLOCK_RE.match(raw)):
            flush(); del cur[:]
        if not cur:
            start = n
        cur.append(raw)
    flush(fence_cue[0] if in_fence else "")

    out, seen = [], set()
    for start, lines, joined in blocks:
        if NEG_RE.search(joined) and not PERMIT_RE.search(joined):
            continue
        for off, line in enumerate(lines):
            for cand in candidates(line, False):
                for rule, snip, why in analyze_command(cand):
                    key = (base + start + off, rule, snip)
                    if key in seen:
                        continue
                    seen.add(key)
                    out.append((base + start + off, rule, snip, why))
    return out

# A HEREDOC BODY IS A DOCUMENT, NOT A COMMAND CHAIN.
# ------------------------------------------------------------------
# `cat > brief.md <<\x27EOF\x27 … EOF` is how briefs, memory notes and commit
# messages are written in this project, and on the Bash surface the body used
# to be split on `;` and read as shell. MEASURED CONSEQUENCE: the memory note
# that BANS the Command-Q — whose body contains the sentence "never any key
# with `command down` through System Events" — was refused, along with the
# brief that ordered this guard. That is the "a guard that forbids the record
# of what it forbids" shape, arriving through the Bash door rather than the
# Write door, and the block-and-cue machinery above is exactly the thing that
# already solves it. So the body is lifted out, judged as a document, and the
# rest of the command line is judged as shell. Nothing stops being checked: a
# heredoc that writes a SCRIPT still has every line judged, with the cue
# required on that line, because the redirect target picks the scope.
HEREDOC_RE = re.compile(r"<<-?\s*([\x27\x22]?)([A-Za-z_][A-Za-z_0-9]*)\1")
CODE_TARGET_RE = re.compile(r"\.(?:sh|bash|zsh|py|rb|pl|rs|js|ts|mjs|go|c|h|swift|kt)\b")

def split_heredocs(text):
    """(text with heredoc bodies blanked, [(body, starting line, prose?)])."""
    lines = text.split("\n")
    bodies, out, i = [], [], 0
    while i < len(lines):
        line = lines[i]
        out.append(line)
        m = HEREDOC_RE.search(line)
        if not m:
            i += 1
            continue
        delim = m.group(2)
        prose = not CODE_TARGET_RE.search(line)
        body, j = [], i + 1
        while j < len(lines) and lines[j].strip() != delim:
            body.append(lines[j])
            j += 1
        if j >= len(lines):
            # No terminator: not a heredoc this guard can trust. Leave the text
            # alone rather than swallow the rest of the command.
            i += 1
            continue
        bodies.append(("\n".join(body), i + 1, prose))
        out.extend("" for _ in body)
        out.append(lines[j])
        i = j + 1
    return "\n".join(out), bodies

# THE GUEST BOUNDARY, ON THE FILE SURFACE TOO — DECLARED, NOT DERIVED.
# ------------------------------------------------------------------
# A script under a `testvm/` path segment is guest-side. The shell surface
# already treats one as guest-bound when it is INVOKED; this is the same
# boundary when it is EDITED, and it is needed for a reason no amount of
# text analysis could reach: `testvm/provision-guest.sh` runs `pmset -a
# displaysleep 0` and three `defaults write com.apple.screensaver` lines to
# stop the GUEST screen going dark for screenshots, and it does so because the
# whole file is copied into the guest and executed there. NOTHING IN THE TEXT
# SAYS THAT. Without this, an engineer editing that file is refused on every
# save, and a guard that blocks the work gets switched off.
#
# WHAT IT COSTS, stated rather than buried: a host-changing command added to a
# `testvm/` script is not caught here. `testvm/setup.sh` genuinely does run on
# the host, so this boundary is DECLARED — one path segment, reviewed as a
# unit, with the headers of those scripts carrying the discipline — and is not
# derived from anything the guard can check. It is the same trade as the
# self-exemption four names above, made for the same reason and written down
# in the same place.
if re.search(r"(?:^|/)testvm/", path):
    print("CLEAN"); sys.exit(0)

PROSE_EXT = {".md", ".markdown", ".txt", ".text", ".rst", ".org", ".adoc", ".mdx"}
if tool in ("Agent", "SendMessage"):
    surface = "prose"
elif tool == "Bash":
    surface = "shell"
else:
    base = os.path.basename(path)
    ext = os.path.splitext(base)[1].lower()
    surface = "prose" if (ext in PROSE_EXT or (ext == "" and base != "")) else "code"

FINDINGS = []

if surface == "shell":
    shell_text, heredocs = split_heredocs(blob)
    if HARD_RE.search(shell_text) or SOFT_RE.search(shell_text):
        for rule, snip, why in analyze_command(shell_text):
            FINDINGS.append((1, rule, snip, why))
    for body, at, prose in heredocs:
        FINDINGS.extend(text_findings(body, prose, base=at))
else:
    FINDINGS.extend(text_findings(blob, surface == "prose"))

if not FINDINGS:
    print("CLEAN"); sys.exit(0)

# The hatch is honored only when there is something to exempt, so a citation in
# a clean call is never logged and never has to be right.
if _ack:
    print("ACK\t" + _ack.group(1).strip().replace("\t", " "))
    sys.exit(0)

FINDINGS.sort(key=lambda f: f[0])
for n, rule, snip, why in FINDINGS[:5]:
    print("FIND\t%d|%s|%s|%s" % (n, rule, snip.replace("|", "/"), why.replace("|", "/")))
print("SURFACE\t" + surface)
' 2>/dev/null || printf 'PARSEFAIL')"

case "$VERDICT" in
    ""|PARSEFAIL*)
        announce_off "HOST DISPLAY/POWER GUARD: this ${TOOL_NAME} call could not be read, so it was NOT checked against ceo-decisions ${HOST_DISPLAY_POWER_RULING}, and a 'ceo-ruled-host-power:' citation could not be read from it either. This ONE call is UNCHECKED."
        exit 0 ;;
    CLEAN) exit 0 ;;
esac

SESSION_ID="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("session_id","") or "")' 2>/dev/null || true)"
LOG_DIR="$ENTITY_ROOT/.claude/state"
LOG="$LOG_DIR/host-power-acks.log"

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
        ACK_WHY="it names no section. Cite the ruling that permits this: 'ceo-ruled-host-power: §<N> — \"<his sentence>\"'."
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
                        # P3-31: the quotation must be IN that section's body,
                        # not merely next to a real section number.
                        if ! python3 - "$_cr_path" "$ACK_CITE" "$ACK_QUOTE" <<'PYQ' 2>/dev/null
import re, sys
path, cite, quote = sys.argv[1:4]
norm = lambda t: re.sub(r"\s+", " ", t.replace("\u201c", '"').replace("\u201d", '"').replace("\u2019", "'")).strip()
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
            ACK_WHY="§${ACK_CITE} is NOT in ${ACK_SOURCE}. Open the file and cite a section that is in it — a citation nobody checked is exactly how a confident instruction gets past a careful person."
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
        echo "=== THE HOST-POWER CITATION WAS NOT ACCEPTED ==="
        echo "  call     : ${TOOL_NAME}${FILE_PATH:+ -> $FILE_PATH}"
        echo "  you gave : ceo-ruled-host-power: ${ACK_ARG}"
        echo "  problem  : ${ACK_WHY}"
        echo ""
        echo "  A BARE MARKER EXEMPTS NOTHING, and neither does a citation that does"
        echo "  not resolve. The line needs a section that EXISTS in the CEO's record"
        echo "  and at least 25 characters of his own words in quotes:"
        echo ""
        echo "      ceo-ruled-host-power: §<N> — \"<his verbatim sentence>\""
        echo ""
        echo "(hook: scripts/hooks/guard-host-display-power.sh)"
    } >&2
    exit 2
fi

# --- REFUSE — and CARRY the rule and the replacement, never point at them ---
SURFACE="$(printf '%s' "$VERDICT" | sed -n 's/^SURFACE	//p' | sed -n 1p)"
{
    echo "=== THIS WOULD CHANGE THE CEO'S OWN MAC — DISPLAY, SLEEP, LOCK OR SESSION (ceo-decisions ${HOST_DISPLAY_POWER_RULING}) ==="
    echo "  call : ${TOOL_NAME}${FILE_PATH:+ -> $FILE_PATH}${SURFACE:+   (read as ${SURFACE})}"
    echo ""
    echo "  WHAT THIS CALL WOULD RUN, OR TELL SOMEBODY TO RUN:"
    printf '%s\n' "$VERDICT" | sed -n 's/^FIND	//p' | while IFS='|' read -r _n _rule _cmd _why; do
        printf '    line %-5s %s\n' "$_n" "$_cmd"
        printf '              ^ %s — %s\n' "$_rule" "$_why"
    done
    echo ""
    echo "  WHY: his Mac is the machine he is working on. A change to its display,"
    echo "  sleep, lock, session or input state reaches him physically, and he"
    echo "  cannot tell an agent's action from a hardware failure. On 2026-09-19"
    echo "  two \`pmset displaysleepnow\` calls blacked out every monitor while he"
    echo "  was at the keyboard: \"there was no way for me to switch them back on"
    echo "  ... I was already considering killing the Mac with the power button!\""
    echo ""
    echo "  THE RULE (${HOST_DISPLAY_POWER_RULING}): no agent, and no brief, message or script,"
    echo "  changes this Mac's display, sleep, lock, session or input state. Such"
    echo "  state changes happen ONLY inside a test VM, over ssh."
    echo ""
    echo "  WHAT TO DO INSTEAD — the replacement, not a rule to go and read:"
    echo "    * quitting a test instance : kill -TERM <pid>, then pgrep -x <name>"
    echo "                                 to confirm it is gone. NEVER a Command"
    echo "                                 keystroke: System Events delivers to"
    echo "                                 whatever is FRONTMOST, and on 2026-09-19"
    echo "                                 that was his Terminal."
    echo "    * a locked-host proof      : ask him to lock it, or do not do it."
    echo "    * keeping a long job alive : caffeinate -is <cmd>  (never -d, never -u)."
    echo "    * reading power state      : pmset -g ...  (every other pmset writes)."
    echo "    * changing display/sleep   : inside the guest — ssh <vm> '...' or"
    echo "                                 testvm/ax.sh <vm> '...', both exempt here."
    echo ""
    echo "  OR CITE A RULING THAT PERMITS IT — one line, and the section must be"
    echo "  in the record with his words quoted from it:"
    echo ""
    echo "      ceo-ruled-host-power: §<N> — \"<his verbatim sentence>\""
    echo ""
    echo "  Accepted citations are appended to .claude/state/host-power-acks.log,"
    echo "  so a habit of waiving is visible rather than invisible."
    echo "(hook: scripts/hooks/guard-host-display-power.sh)"
} >&2
exit 2
