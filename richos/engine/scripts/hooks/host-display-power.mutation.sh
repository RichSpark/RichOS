#!/usr/bin/env bash
#
# host-display-power.mutation.sh — PROVES THE HOST-POWER SUITE CAN FAIL.
#
# guard-host-display-power.test.sh is 88 green ticks, and a suite that cannot
# go red is a suite that proves nothing. Each mutant below removes ONE property
# of the guard — a property somebody could plausibly delete while "simplifying"
# the rules — and requires:
#
#   1. guard-host-display-power.test.sh FAILS, and
#   2. it fails AT THE NAMED CASE, so the red is caused by the removal and not
#      by some unrelated breakage the mutation happened to cause.
#
# THE MUTANTS ARE THE DESIGN DECISIONS, not the code's surface. Every one of
# them leaves the guard still blocking something obvious, still passing its
# easy cases, and quietly broken in the direction §65 cares about — which is
# the only kind of regression worth a harness.
#
# THREE OF THEM ARE DEFECTS THIS GUARD ACTUALLY HAD, caught by measurement
# before it shipped and named as such below: heredoc-as-shell, no-permit-veto
# and code-comments-not-stripped. They are not hypotheses.
#
# Run directly: scripts/hooks/host-display-power.mutation.sh
# Exit 0 = every property is load-bearing; exit 1 = at least one is not.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

PASS=0
FAIL=0
# The sandbox comes from the engine's allocator, never a bare mktemp -d:
# every mutant copies scripts/lib into it, which is the class of directory
# that left 105.3 GB under a name the reaper had never been told about on
# 2026-09-17. Allocated scratch is swept without anybody naming it.
# shellcheck source=../lib/scratch.sh
. "$ENGINE_ROOT/scripts/lib/scratch.sh"
SANDBOX="$(scratch_new host-display-power-mutation)" || {
    echo "FATAL: could not allocate a sandbox" >&2; exit 2; }
trap 'scratch_release "$SANDBOX"' EXIT

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

cat >"$SANDBOX/mutate.py" <<'PYEOF'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
old = old.replace("\\n", "\n")
new = new.replace("\\n", "\n")
with open(path, encoding="utf-8") as fh:
    src = fh.read()
if old not in src:
    sys.stderr.write("MUTATION TARGET ABSENT — the source has drifted:\n  %s\n" % old)
    sys.exit(3)
with open(path, "w", encoding="utf-8") as fh:
    fh.write(src.replace(old, new, 1))
PYEOF

# shellcheck source=../lib/stopwatch.sh
. "$ENGINE_ROOT/scripts/lib/stopwatch.sh"
# shellcheck source=../lib/mutation-pool.sh
. "$ENGINE_ROOT/scripts/lib/mutation-pool.sh"
# shellcheck source=../lib/mutant-suite-run.sh
. "$ENGINE_ROOT/scripts/lib/mutant-suite-run.sh"
mut_pool_init
MUT_WALL_T0="$(sw_now_ms)"

_mutant_body() {
    local name="$1" want="$2" rel="$3" old="$4" new="$5" why="$6"
    local dir="$SANDBOX/$name"
    mkdir -p "$dir/scripts/hooks" "$dir/hooks" "$dir/.claude"
    cp "$ENGINE_ROOT/scripts/hooks/guard-host-display-power.sh" \
       "$ENGINE_ROOT/scripts/hooks/guard-host-display-power.test.sh" \
       "$dir/scripts/hooks/"
    # THE WHOLE lib/, not a named list. This guard reaches resolve-roots.sh,
    # unevaluated-notice.sh, ceo-ruled.sh and whatever those reach in turn; a
    # typed list of libraries is the stale inventory this engine keeps finding
    # in itself, and a missing one here would make every mutant "fail" for the
    # wrong reason.
    cp -R "$ENGINE_ROOT/scripts/lib" "$dir/scripts/lib"
    cp "$ENGINE_ROOT/hooks/hooks.json" "$dir/hooks/" 2>/dev/null || true
    cp "$ENGINE_ROOT/.claude/settings.local.json" "$dir/.claude/" 2>/dev/null || true
    cp "$ENGINE_ROOT/orchestration.config" "$dir/"
    chmod +x "$dir/scripts/hooks/"*.sh

    if ! python3 "$SANDBOX/mutate.py" "$dir/$rel" "$old" "$new" 2>"$dir/mutate.err"; then
        printf '  FAIL  %s — the mutation did not apply\n' "$name"
        sed 's/^/          /' "$dir/mutate.err"
        return 1
    fi

    # The run stops at the mutant's named FAIL line (mutant-suite-run.sh): in this
    # suite a printed FAIL line always ends the run red, and the rest of the run
    # buys nothing the verdict reads.
    mutant_suite_run "$dir/out.txt" "$want" env RICHOS_MUTATION_INNER=1 bash "$dir/scripts/hooks/guard-host-display-power.test.sh"
    if [ "$MUTANT_STOPPED" = 1 ]; then
        printf '  PASS  %s — removing it turns %s red (stopped at that line)\n' "$name" "$want"
        return 0
    fi
    local rc=$MUTANT_RC
    if [ "$rc" -eq 0 ]; then
        printf '  FAIL  %s — the suite still PASSED without this property.\n' "$name"
        printf '          %s\n' "$why"
        return 1
    fi
    if ! grep -qF "FAIL  $want" "$dir/out.txt"; then
        printf '  FAIL  %s — the suite went red, but NOT at %s (so the red is unrelated).\n' "$name" "$want"
        grep '  FAIL' "$dir/out.txt" | sed 's/^/          /'
        return 1
    fi
    printf '  PASS  %s — removing it turns %s red\n' "$name" "$want"
    return 0
}

mutant() {
    mut_pool_submit "$1" _mutant_body "$@"
}

echo "=== the host display/power guard: every property, proven load-bearing by removing it ==="

G="scripts/hooks/guard-host-display-power.sh"

# --- 1. IT BLOCKS AT ALL ---------------------------------------------------
mutant refuses-to-refuse "A1 " "$G" \
    '    echo "(hook: scripts/hooks/guard-host-display-power.sh)"\n} >&2\nexit 2' \
    '    echo "(hook: scripts/hooks/guard-host-display-power.sh)"\n} >&2\nexit 0' \
    "the guard would find the call, print the whole ruling and the replacement, and let the monitors go black anyway — a warning wearing a guard's clothes."

# --- 2. pmset: -g IS THE ONLY READ ----------------------------------------
mutant pmset-allowlist-widened "A1 " "$G" \
    '        if args[0] == "-g":\n            return None' \
    '        if True:\n            return None' \
    "every pmset form would be allowed, including the two calls that blacked out his monitors. The man page is explicit that -g is the only one that does not write."

# --- 3. caffeinate: -d AND -u ARE THE DISPLAY FLAGS -----------------------
mutant caffeinate-flags-ignored "C9 " "$G" \
    '            flags = set(w[1:].lower())' \
    '            flags = set()' \
    "caffeinate -dimsu would run for twenty minutes at a time on his Mac, holding the display on and declaring him active — the measurement's single largest finding, ten live calls in one session."

# --- 4. THE MODIFIER KEYSTROKE --------------------------------------------
mutant modifier-keystroke-allowed "B1 " "$G" \
    '    (re.compile(r"(?:keystroke|key\s+code)\b[\s\S]{0,80}?\busing\b[\s\S]{0,60}?\bdown\b",' \
    '    (re.compile(r"(?!x)x",' \
    "the Command-Q that landed on the CEO's Terminal and offered to terminate the session would pass, and so would the thirty-nine others like it."

# --- 5. THE GUEST EXEMPTION IS STRUCTURAL, NOT TEXTUAL --------------------
# The whole point of tokenizing. A substring test for "ssh" is the obvious
# shortcut and it is defeated by a variable named SSH.
mutant guest-exemption-textual "E3 " "$G" \
    '        if base in REMOTE or re.search(r"(?:^|/)testvm/", a0):' \
    '        if "ssh" in seg or re.search(r"(?:^|/)testvm/", a0):' \
    "any command with the word ssh anywhere in it would be exempt, so SSH=\"ssh vm\" pmset displaysleepnow blacks out his screen with the guard watching."

# --- 6. ssh OPTIONS THAT RUN LOCALLY --------------------------------------
mutant proxycommand-unchecked "E6 " "$G" \
    '                m = re.match(r"^(?:-o)?\s*(?:ProxyCommand|LocalCommand)=(.*)$", w,' \
    '                m = re.match(r"^(?!x)x(.*)$", w,' \
    "ssh -o ProxyCommand='<anything>' runs that anything on the HOST, so the word ssh plus one option flag would be a complete bypass."

# --- 7. A SHELL -c ARGUMENT IS A COMMAND ----------------------------------
mutant shell-c-not-followed "E2 " "$G" \
    "                if w == \"-c\" and k + 1 < len(argv):" \
    "                if False and k + 1 < len(argv):" \
    "bash -c 'pmset displaysleepnow' would pass, and wrapping a command in a shell is the first thing anybody tries."

# --- 8. A HEREDOC BODY IS A DOCUMENT --------------------------------------
# A DEFECT THIS GUARD HAD. Measured on 1,097 real Bash calls: read as a command
# chain, the heredoc that writes the memory note BANNING the Command-Q was
# refused, and so was the brief that ordered this guard.
mutant heredoc-as-shell "G1 " "$G" \
    '    shell_text, heredocs = split_heredocs(blob)' \
    '    shell_text, heredocs = blob, []' \
    "the record could not be written: a heredoc writing note.md, carrying 'never any key with command down' is read as a command chain and refused. That is the shape that cost this engine fourteen vendored lines in 2026-08, arriving through the Bash door."

# --- 9. THE CUE TEST LETS THE RULING BE WRITTEN DOWN ----------------------
mutant no-cue-test "F5 " "$G" \
    '        if NEG_RE.search(joined) and not PERMIT_RE.search(joined):\n            continue' \
    '        if False:\n            continue' \
    "ceo-decisions §65 itself would be refused — it quotes pmset displaysleepnow, caffeinate -d/-u and killall loginwindow, because a ruling has to name what it rules on."

# --- 10. THE PERMISSIVE VETO ----------------------------------------------
# A DEFECT THIS GUARD HAD. Both real permissive sentences carry a negation cue
# about something else, so the cue test alone exempts the exact two texts that
# put the command in a teammate's hands.
mutant no-permit-veto "F3 " "$G" \
    ' and not PERMIT_RE.search(joined):' \
    ':' \
    "'you may lock the screen with pmset displaysleepnow' sits in a block that also says 'only if no candidate is on screen' — so the cue test exempts it, and the sentence that caused all of this is permitted by the guard written to stop it."

# --- 11. ...AND ITS LOOKAHEAD ---------------------------------------------
mutant permit-lookahead-removed "F12" "$G" \
    'r"\byou (?:may|can|could|should)\b(?!\s*(?:not|never))",' \
    'r"\byou (?:may|can|could|should)\b",' \
    "'you may NOT run it' is a prohibition wearing a permission verb; the veto would fire on it and the sentence banning the command would be refused for banning it."

# --- 12-15. THE CODE SURFACE ----------------------------------------------
# ALL FOUR OF THESE ARE DEFECTS THIS GUARD SHIPPED INTERNALLY AND MEASUREMENT
# CAUGHT, on the fifteen build and testvm scripts that landed at 321f1076 while
# this file was being written. Before them, two of those scripts were refused
# on every save. That is the failure mode this whole harness exists for: not a
# guard that misses something, but a guard that blocks the work and gets
# switched off by lunchtime.
#
# A NOTE ON WHAT WAS HERE BEFORE, because the correction is worth more than the
# mutant. This slot held code-comments-not-stripped, aimed at F8 — the comment
# line reading "never run pmset displaysleepnow" — on the belief that a stripper was
# what let that line through. IT WAS NOT: F8 passes on its own "never" cue, and
# this harness said so by going green where it should have gone red. What
# actually fixed F8 was deleting a permissive clause matching run-plus-trigger,
# made in the same edit, which then took the credit for a fix it did not make.
# The stripper is gone entirely now — command position does its job and more.
# Same error the sibling guard records against itself: a plausible claim
# written from memory of the fix rather than from a measurement of it.

# 12. A MENTION IS NOT AN INVOCATION.
mutant code-scored-by-word-scan "F15" "$G" \
    '    if in_code:\n        return [line]' \
    '    if False:\n        return [line]' \
    "taking every trigger word to the end of its line makes a brew-install line naming displayplacer look like an invocation of it. Measured: it refused testvm/provision-guest.sh, so editing that file was blocked on every save."

# 13. A TRAILING BACKSLASH IS ONE COMMAND.
mutant continuations-not-joined "F16" "$G" \
    '        for n, line in join_continuations(text.split(' \
    '        for n, line in enumerate(text.split(' \
    "testvm/setup.sh reboots the GUEST with an ssh whose argument sits on the next line; per physical line the continuation loses its command word and a guest reboot reads as a shutdown of his Mac."

# 14. AN UNRESOLVABLE $VAR IS STEPPED OVER, NOT TRUSTED.
mutant var-command-word-trusted "F18" "$G" \
    '        if VAR_RE.match(w):\n            i += 1; continue' \
    '        if False:\n            i += 1; continue' \
    "a line whose first word is the SUDO variable resolves to that variable as the command word, and the pmset after it vanishes. The sudo-or-nothing idiom would be a hole anybody could open by accident, which is the worst kind."

# 15. THE GUEST BOUNDARY ON THE FILE SURFACE.
mutant testvm-files-not-guest "F19" "$G" \
    'if re.search(r"(?:^|/)testvm/", path):\n    print("CLEAN"); sys.exit(0)' \
    'if False:\n    print("CLEAN"); sys.exit(0)' \
    "testvm/provision-guest.sh disables sleep and the screensaver INSIDE the guest, and nothing in its text says so — every save of it would be refused, with no honest hatch available since no ruling permits it."

# --- 13. THE WRAPPER FLAG THAT TAKES AN ARGUMENT --------------------------
mutant wrapper-flag-arg-ignored "C2 " "$G" \
    'WRAPPER_FLAG_ARG = {"-u", "-g", "-p", "-C", "-r", "-t", "-U", "-n", "-i", "-o"}' \
    'WRAPPER_FLAG_ARG = set()' \
    "sudo -u alex pmset sleepnow resolves to the command word 'alex' and passes — one flag with a value is all it takes."

# --- 14. THE HATCH IS NOT A MARKER ----------------------------------------
# The target is the ADMISSION, not either half of the validation: a citation
# needs a section AND a quotation, so disabling one alone leaves the other
# refusing and proves nothing.
mutant bare-marker-exempts "H6 " "$G" \
    '    if [ -z "$ACK_WHY" ]; then\n        mkdir -p "$LOG_DIR" 2>/dev/null || true' \
    '    if true; then\n        mkdir -p "$LOG_DIR" 2>/dev/null || true' \
    "'ceo-ruled-host-power: yes' would exempt anything, so the escape hatch becomes an off switch anyone can type on the line above the command."

# --- 15. THE CITATION MUST RESOLVE AGAINST THE RECORD ---------------------
mutant citation-unchecked "H4 " "$G" \
    'if [ "$ACK_STATE" = "absent" ]; then' \
    'if false; then' \
    "a citation of a section that does not exist would be accepted. The instruction that caused this was careful and hedged, not careless — a free-text reason would have been written just as well, and only a citation that must open the file cannot be."

# --- 16. THE SELF-EXEMPTION -----------------------------------------------
mutant no-self-exemption "J1 " "$G" \
    'guard-host-display-power.sh|guard-host-display-power.test.sh|host-display-power.mutation.sh|host-display-power.corpus.md)\n        exit 0 ;;' \
    'guard-host-display-power-NOTHING.sh)\n        exit 0 ;;' \
    "this guard could not be authored, tested or repaired, and a guard that cannot be repaired is a guard somebody deletes."

mut_pool_drain
mut_pool_require_submissions "$(basename "$0")"
PASS=$(( PASS + MUT_POOL_PASS ))
FAIL=$(( FAIL + MUT_POOL_FAIL ))
mut_pool_report_line "$(( $(sw_now_ms) - MUT_WALL_T0 ))"
mut_pool_cleanup

echo
if [ "$FAIL" -eq 0 ]; then
    printf '\n  %d/%d properties proven load-bearing\n' "$PASS" "$((PASS + FAIL))"
    exit 0
fi
printf '\n  %d/%d properties proven load-bearing, %d NOT\n' "$PASS" "$((PASS + FAIL))" "$FAIL"
exit 1
