#!/usr/bin/env bash
#
# host_shield.test.sh — no suite started through host_shield.py can reach this Mac's power,
# sleep, lock or display commands (ceo-decisions §65). The runners' own suites (run-tests.test.sh,
# proof-run.test.py, ci-shard.test.sh, run-all-tests.test.sh) prove each runner starts its suites
# through it; this file proves what the shield does once it is in the way.
#
# EVERY CASE RUNS UNDER hs_path: a tripwire directory first and NEITHER sbin on PATH, so a stand-in
# that wrongly handed a call on reaches a tripwire that records it, never the host. Every command
# name a case needs is assembled at run time (see host-shield-fixture.sh); no line of this file is
# a call to one.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PYTHONDONTWRITEBYTECODE=1
# shellcheck source=host-shield-fixture.sh
. "$HERE/host-shield-fixture.sh"
unset RICHOS_HOST_SHIELD_LOG RICHOS_HOST_SHIELD_RUNS

TMP="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/host-shield-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "$TMP"' EXIT
# Every run below keeps its log in a TMPDIR of its own: in the shared one, another agent's
# shielded suite running at the same moment leaves a live log that H10 read as this file's
# (2026-10-11, three logs of concurrent runs, each still in use when H10 looked).
export TMPDIR="$TMP/tmpdir"
mkdir -p "$TMPDIR"
PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '%s\n' "$2" | sed 's/^/          /'; FAIL=$((FAIL + 1)); }

TRIP="$TMP/trip"; SUITES="$TMP/suites"
hs_fixture "$TRIP" "$SUITES"
CASE_PATH="$(hs_path "$TRIP")"
PM="pm"; PM="${PM}set"; CAF="caff"; CAF="${CAF}einate"; OSA="osa"; OSA="${OSA}script"
KA="kill"; KA="${KA}all"; DOCK="Do"; DOCK="${DOCK}ck"; SLEEPNOW="sleep"; SLEEPNOW="${SLEEPNOW}now"
SLEEP="sle"; SLEEP="${SLEEP}ep"
logs_in_tmpdir() { find "${TMPDIR:-/tmp}" -maxdepth 1 -name 'richos-host-shield.*.log' 2>/dev/null | LC_ALL=C sort; }
logs_in_tmpdir > "$TMP/logs-before"

# shielded <suite-file> [args...] -> output in $TMP/out, rc echoed
shielded() {
    PATH="$CASE_PATH" python3 -S "$HS_TOOL" run -- bash "$@" > "$TMP/out" 2>&1
    printf '%s' "$?"
}
trip_clean() { : > "$TRIP/trip.log"; }
suite() {  # <name> <body lines...>: a suite file under $SUITES
    local f="$SUITES/$1"; shift
    { printf '#!/usr/bin/env bash\n'; printf '%s\n' "$@"; } > "$f"
    printf '%s' "$f"
}

echo "=== host_shield ==="

# H1 — the stand-ins are exactly the commands the guard judges, read from the guard itself. By
# name, so the engine's selector maps every stand-in file to this suite (it matches basenames):
# (the screen-saver one is named in guard-host-display-power.test.sh; here it is assembled, as a
# literal would read to the guard's code surface as a call)
SSE="ScreenSaver"; SSE="${SSE}Engine"
STANDIN_FILES="betterdisplaycli brightness caffeinate cgsession defaults displayplacer halt killall launchctl nvram open osascript pkill pmset reboot $SSE shutdown systemsetup"
lower_sorted() { tr ' ' '\n' | tr '[:upper:]' '[:lower:]' | LC_ALL=C sort | tr '\n' ' ' | sed 's/ $//'; }
if [ "$(ls "$HS_STANDINS" | tr '\n' ' ' | lower_sorted)" = "$(printf '%s' "$STANDIN_FILES" | lower_sorted)" ]; then
    ok "H1a host-shield/ holds exactly the eighteen stand-ins this suite names"
else
    bad "H1a host-shield/ changed" "$(LC_ALL=C ls "$HS_STANDINS" | tr '\n' ' ')"
fi
# The heredoc writes to a file and is never inside $(...): bash 3.2 mis-parses that shape, and
# that mis-parse is the incident this whole file exists for.
python3 -S - "$HS_TOOL" > "$TMP/h1" 2>&1 <<'PY'
import importlib.util, re, sys
spec = importlib.util.spec_from_file_location("host_shield", sys.argv[1])
hs = importlib.util.module_from_spec(spec); spec.loader.exec_module(hs)
G = hs.guard()
src = open(hs.GUARD, encoding="utf-8").read()
body = src[src.index("def judge("):src.index("def analyze_command(")]
names = {k.lower() for k in G["ALWAYS_BAD"]}
names |= set(re.findall(r'name == "([a-z]+)"', body))
for group in re.findall(r'name in \(([^)]*)\)', body):
    names |= set(re.findall(r'"([a-z]+)"', group))
# The screen-saver branch tests the command word itself, not `name ==`: read it too.
names |= set(re.findall(r'"([a-z]+)" in a0\.lower\(\)', body))
want, have = sorted(names), sorted(n.lower() for n in hs.standin_names())
print("same" if want == have else "guard judges %s; stand-ins are %s" % (want, have))
PY
GOT="$(cat "$TMP/h1")"
if [ "$GOT" = same ]; then
    ok "H1  host-shield/ holds a stand-in for every command the guard's judge() can refuse, and no other"
else
    bad "H1  the stand-in set drifted from the guard" "$GOT"
fi

# H2 — the throwaway suite calls the halt command by bare name: it fails with the logged line.
trip_clean
RC="$(shielded "$SUITES/hs-bare.test.sh")"
if [ "$RC" = 1 ] && grep -q "^  FAIL  host-shield: refused \`$HS_CMD -h +60\`" "$TMP/out" && [ ! -s "$TRIP/trip.log" ]; then
    ok "H2  a suite calling $HS_CMD -h +60 by bare name FAILS (rc 1) with the stand-in's logged line; the tripwire behind it saw nothing"
else
    bad "H2  rc=$RC" "$(cat "$TMP/out"; echo '--- tripwire:'; cat "$TRIP/trip.log")"
fi
if grep -q "carried on after the call" "$TMP/out"; then
    ok "H2b the suite itself exited 0 after the refused call, and the shield still counts it FAILED"
else
    bad "H2b the suite did not carry on as written" "$(cat "$TMP/out")"
fi

# H3 — its source names the command by path: refused before it runs.
trip_clean
RC="$(shielded "$SUITES/hs-path.test.sh")"
if [ "$RC" = 1 ] && grep -q "REFUSED before it ran" "$TMP/out" && grep -q "hs-path.test.sh:6: /sbin/$HS_CMD -h +60" "$TMP/out" \
        && [ ! -e "$TRIP/path-suite-ran" ]; then
    ok "H3  a suite whose source names /sbin/$HS_CMD is refused before it starts (its first line never ran)"
else
    bad "H3  rc=$RC" "$(cat "$TMP/out")"
fi

# H4 — the harmless forms are handed on unchanged; the harmful ones are refused.
F="$(suite h4.test.sh "$PM -g batt" "$OSA -e 'return 1'" "$KA -0 no-such-process-here")"
trip_clean
RC="$(shielded "$F")"
if [ "$RC" = 0 ] && grep -q "TRIPWIRE $PM -g batt" "$TRIP/trip.log" \
        && grep -q "TRIPWIRE $OSA -e return 1" "$TRIP/trip.log" && grep -q "TRIPWIRE $KA -0 no-such-process-here" "$TRIP/trip.log"; then
    ok "H4  $PM -g, a harmless $OSA -e and $KA -0 reach the next command on PATH unchanged (rc 0)"
else
    bad "H4  rc=$RC" "$(cat "$TMP/out"; echo '--- tripwire:'; cat "$TRIP/trip.log")"
fi
# The guard's refusals, then the TEST-RUN rules (forms the guard allows an operator and a test
# never needs: any caffeinate, any osascript keystroke, an app brought to the front).
SE='tell application "System Events" to'
for form in "$PM $SLEEPNOW" "$CAF -d true" "$CAF -t 5 -d true" "$KA $DOCK" "$CAF -is true" \
        "$OSA -e '$SE key code 53'" "$OSA -e '$SE keystroke \"a\"'" "$OSA -e 'tell application \"Finder\" to set frontmost to true'"; do
    F="$(suite h4b.test.sh "$form || true" "exit 0")"
    trip_clean
    RC="$(shielded "$F")"
    if [ "$RC" = 1 ] && grep -q "^  FAIL  host-shield: refused \`${form%% *} " "$TMP/out" && [ ! -s "$TRIP/trip.log" ]; then
        ok "H4b $form is refused and logged; nothing reached the tripwire"
    else
        bad "H4b $form: rc=$RC" "$(cat "$TMP/out"; echo '--- tripwire:'; cat "$TRIP/trip.log")"
    fi
done

# H4c — the test-run rules refuse what cannot be shown harmless: any open (it launches or
# activates an app, a file's app or a URL's browser), and an osascript that activates an app,
# sends a key code through JXA, runs a shell from inside itself, reaches the Objective-C bridge
# or puts a dialog up (reviews rv-20261011T141442Z-0009fe5a-a898 #2, rv-20261011T173659Z-e08ee8a2-2c75 #2).
OPEN="op"; OPEN="${OPEN}en"
for form in "$OPEN -a TextEdit" "$OPEN https://example.com" "$OPEN -g -b com.apple.TextEdit" \
        "$OSA -e 'tell application \"Finder\" to activate'" \
        "$OSA -l JavaScript -e 'Application(\"System Events\").keyCode(53)'" \
        "$OSA -e 'do shell script \"/sbin/$HS_CMD -h +60\"'" \
        "$OSA -l JavaScript -e 'ObjC.import(\"stdlib\"); \$.system(\"true\")'" \
        "$OSA -e 'display dialog \"hello\"'"; do
    F="$(suite h4c.test.sh "$form || true" "exit 0")"
    trip_clean
    RC="$(shielded "$F")"
    if [ "$RC" = 1 ] && grep -q "^  FAIL  host-shield: refused \`${form%% *} " "$TMP/out" && [ ! -s "$TRIP/trip.log" ]; then
        ok "H4c $form is refused and logged; nothing reached the tripwire"
    else
        bad "H4c $form: rc=$RC" "$(cat "$TMP/out"; echo '--- tripwire:'; cat "$TRIP/trip.log")"
    fi
done
# A compiled script cannot be read, so it cannot be judged harmless.
printf 'FasdUAS 1.101.10\000\000' > "$TMP/compiled.scpt"
F="$(suite h4d.test.sh "$OSA '$TMP/compiled.scpt' || true" "exit 0")"
trip_clean
RC="$(shielded "$F")"
if [ "$RC" = 1 ] && grep -q "compiled script cannot be read" "$TMP/out" && [ ! -s "$TRIP/trip.log" ]; then
    ok "H4d a compiled $OSA script is refused: it cannot be read, so it cannot be judged harmless"
else
    bad "H4d rc=$RC" "$(cat "$TMP/out"; echo '--- tripwire:'; cat "$TRIP/trip.log")"
fi

# H5 — osascript reads its script from stdin: the stand-in judges that text too, and hands a
# harmless one on with its stdin intact.
TELL='tell application "System Events" to'
F="$(suite h5.test.sh "printf '%s\n' '$TELL $SLEEP' | $OSA || true" "exit 0")"
trip_clean
RC="$(shielded "$F")"
if [ "$RC" = 1 ] && grep -q "^  FAIL  host-shield: refused \`$OSA\` -- $OSA" "$TMP/out" && [ ! -s "$TRIP/trip.log" ]; then
    ok "H5  an $OSA script piped on stdin that tells the host to sleep is refused; nothing reached the tripwire"
else
    bad "H5  rc=$RC" "$(cat "$TMP/out"; echo '--- tripwire:'; cat "$TRIP/trip.log")"
fi
F="$(suite h5b.test.sh "printf 'return 2' | $OSA")"
trip_clean
RC="$(shielded "$F")"
if [ "$RC" = 0 ] && grep -q "STDIN return 2" "$TRIP/trip.log"; then
    ok "H5b a harmless $OSA script on stdin reaches the next $OSA with the same stdin"
else
    bad "H5b rc=$RC" "$(cat "$TMP/out"; echo '--- tripwire:'; cat "$TRIP/trip.log")"
fi

# H6 — the source check: sudo resets PATH, so a bare name under it is refused; a harmless read by
# path is not.
F="$(suite h6.test.sh "exit 0" "sudo $HS_CMD -h +60")"
RC="$(shielded "$F")"
if [ "$RC" = 1 ] && grep -q "under sudo, which resets PATH" "$TMP/out"; then
    ok "H6  sudo before a bare name is refused before the suite runs (sudo resets PATH past the stand-ins)"
else
    bad "H6  rc=$RC" "$(cat "$TMP/out")"
fi
F="$(suite h6b.test.sh "exit 0" "/usr/bin/$PM -g batt")"
RC="$(shielded "$F")"
if [ "$RC" = 0 ]; then
    ok "H6b /usr/bin/$PM -g by path is a harmless read and is not refused"
else
    bad "H6b rc=$RC" "$(cat "$TMP/out")"
fi
F="$(suite h6c.test.sh "exit 0" "TOOL=/sbin/$HS_CMD")"
RC="$(shielded "$F")"
if [ "$RC" = 1 ] && grep -q "names /sbin/$HS_CMD by path" "$TMP/out"; then
    ok "H6c a path to an always-refused command is refused wherever it is named (an assignment too)"
else
    bad "H6c rc=$RC" "$(cat "$TMP/out")"
fi

# H6d — a path inside shell control syntax, and a path to ANY stood-in command kept in a
# variable, are refused too (each sits after `exit 0`: the source check reads it, nothing runs it).
for line in "if /usr/bin/$PM $SLEEPNOW; then :; fi" \
            "while ! /usr/bin/$OSA -e '$SE $SLEEP'; do :; done" \
            "POWER=/usr/bin/$PM" "local power=/usr/bin/$CAF"; do
    F="$(suite h6d.test.sh "exit 0" "$line")"
    RC="$(shielded "$F")"
    if [ "$RC" = 1 ] && grep -q "REFUSED before it ran" "$TMP/out"; then
        ok "H6d refused before it runs: $line"
    else
        bad "H6d not refused: $line (rc=$RC)" "$(cat "$TMP/out")"
    fi
done

# H6f — a path never reaches a stand-in, so the source check applies the TEST-RUN rules to it
# too (review rv-20261011T141442Z-0009fe5a-a898 #1), and refuses an osascript by path whose
# script it cannot read. H6g — a backslash-continued command is read joined (#3).
for line in "/usr/bin/$CAF -is true" "/usr/bin/$OSA -e '$SE key code 53'" "/usr/bin/$OPEN -a TextEdit" \
            "/usr/bin/$OSA ./a-script.applescript"; do
    F="$(suite h6f.test.sh "exit 0" "$line")"
    RC="$(shielded "$F")"
    if [ "$RC" = 1 ] && grep -q "REFUSED before it ran" "$TMP/out"; then
        ok "H6f refused before it runs: $line"
    else
        bad "H6f not refused: $line (rc=$RC)" "$(cat "$TMP/out")"
    fi
done
F="$(suite h6g.test.sh "exit 0" "/usr/bin/$OSA \\" "  -e '$SE $SLEEP'")"
RC="$(shielded "$F")"
if [ "$RC" = 1 ] && grep -q "h6g.test.sh:3: .*$SLEEP" "$TMP/out"; then
    ok "H6g a host command continued across lines is read as one command and refused before it runs"
else
    bad "H6g a continued command was not read joined (rc=$RC)" "$(cat "$TMP/out")"
fi
F="$(suite h6h.test.sh "exit 0" "/usr/bin/$OSA -e 'return 1' >/dev/null" "/usr/bin/$PM -g batt >/dev/null || true")"
RC="$(shielded "$F")"
if [ "$RC" = 0 ]; then
    ok "H6h an inert $OSA -e and $PM -g by path still pass the source check (scan only: they sit after exit 0)"
else
    bad "H6h rc=$RC" "$(cat "$TMP/out")"
fi

# H6i — the source check reads every form a runner starts, not only a shell suite (review
# rv-20261011T141442Z-0009fe5a-a898 #4): a Python suite, a Node suite, Python's inline code, a
# shell's inline string, and a host command started directly by path. Each payload exits (or
# is a recording tripwire) before its host call: on a shield that let it start, nothing reaches
# the host.
shielded_argv() {  # <argv...> -> output in $TMP/out, rc echoed
    PATH="$CASE_PATH" python3 -S "$HS_TOOL" run -- "$@" > "$TMP/out" 2>&1
    printf '%s' "$?"
}
printf '%s\n' 'print("PYTHON SUITE STARTED", flush=True)' 'raise SystemExit(0)' 'import subprocess' \
    "subprocess.run([\"/sbin/$HS_CMD\", \"-h\", \"+60\"])" > "$SUITES/h6i.test.py"
printf '%s\n' 'console.log("NODE SUITE STARTED");' 'process.exit(0);' \
    "require(\"child_process\").execFileSync(\"/sbin/$HS_CMD\", [\"-h\", \"+60\"]);" > "$SUITES/h6i.test.js"
h6i() {  # <label> <argv...>
    local label="$1" rc; shift
    trip_clean
    rc="$(shielded_argv "$@")"
    if [ "$rc" = 1 ] && grep -q "REFUSED before it ran" "$TMP/out" && ! grep -q "SUITE STARTED" "$TMP/out" \
            && [ ! -s "$TRIP/trip.log" ]; then
        ok "H6i refused before it starts: $label"
    else
        bad "H6i not refused before it started: $label (rc=$rc)" "$(cat "$TMP/out"; echo '--- tripwire:'; cat "$TRIP/trip.log")"
    fi
}
h6i "a Python suite naming /sbin/$HS_CMD" python3 "$SUITES/h6i.test.py"
h6i "a Node suite naming /sbin/$HS_CMD" node "$SUITES/h6i.test.js"
h6i "python3 -c code naming /sbin/$HS_CMD" python3 -c "raise SystemExit(0); import subprocess; subprocess.run(['/sbin/$HS_CMD', '-h'])"
h6i "a shell's inline string calling $HS_CMD by path" bash -c "exit 0; $TRIP/$HS_CMD -h +60"
h6i "$HS_CMD started directly by path (here a recording tripwire)" "$TRIP/$HS_CMD" -h +60

# H6j — a backtick substitution inside double quotes is read as a command, as $(...) there is
# (review rv-20261011T154414Z-d0066287-e6a8). Scan only: the line sits after `exit 0`.
H6J_LINE="result=\"\`/sbin/$HS_CMD\`\""
F="$(suite h6j.test.sh "exit 0" "$H6J_LINE")"
RC="$(shielded "$F")"
if [ "$RC" = 1 ] && grep -q "REFUSED before it ran" "$TMP/out"; then
    ok "H6j refused before it runs: $H6J_LINE"
else
    bad "H6j not refused: $H6J_LINE (rc=$RC)" "$(cat "$TMP/out")"
fi

# H6e — the screen-saver binary has a stand-in of its own: a bare-name call is refused.
F="$(suite h6e.test.sh "$SSE || true" "exit 0")"
trip_clean
RC="$(shielded "$F")"
if [ "$RC" = 1 ] && grep -q "^  FAIL  host-shield: refused \`$(printf '%s' "$SSE" | tr '[:upper:]' '[:lower:]')\`" "$TMP/out" \
        && [ ! -s "$TRIP/trip.log" ]; then
    ok "H6e a bare-name call of the screen-saver binary is refused; nothing reached the tripwire"
else
    bad "H6e rc=$RC" "$(cat "$TMP/out"; echo '--- tripwire:'; cat "$TRIP/trip.log")"
fi

# H7 — declarations. `names` lets the source carry it as data; a bare-name call is still refused.
F="$(suite h7.test.sh "# host-shield: names $HS_CMD -- a fixture this suite only prints" \
        "$HS_CMD -h +60 || true" "printf '%s\n' /sbin/$HS_CMD" "exit 0")"
trip_clean
RC="$(shielded "$F")"
if [ "$RC" = 1 ] && ! grep -q "REFUSED before it ran" "$TMP/out" && grep -q "^  FAIL  host-shield: refused" "$TMP/out" \
        && [ ! -s "$TRIP/trip.log" ]; then
    ok "H7  a 'names' declaration lets the source name it as data, and the stand-in still refuses the call"
else
    bad "H7  rc=$RC" "$(cat "$TMP/out")"
fi
# H7d — `names` never exempts a call that would run it past the stand-in: by path, or as an
# argument something else runs (review rv-20261011T141442Z-0009fe5a-a898 #6). Each sits after
# `exit 0`: the source check reads it, nothing runs it.
for line in "/sbin/$HS_CMD -h +60" "xargs /sbin/$HS_CMD -h" "sudo $HS_CMD -h +60"; do
    F="$(suite h7d.test.sh "# host-shield: names $HS_CMD -- a fixture this suite only prints" "exit 0" "$line")"
    RC="$(shielded "$F")"
    if [ "$RC" = 1 ] && grep -q "REFUSED before it ran" "$TMP/out"; then
        ok "H7d a 'names' declaration does not exempt a call: $line"
    else
        bad "H7d a 'names' declaration exempted a call: $line (rc=$RC)" "$(cat "$TMP/out")"
    fi
done
F="$(suite h7b.test.sh "# host-shield: names $HS_CMD" "exit 0")"
RC="$(shielded "$F")"
if [ "$RC" = 1 ] && grep -q "declares nothing" "$TMP/out"; then
    ok "H7b a declaration without a reason declares nothing and refuses the suite"
else
    bad "H7b rc=$RC" "$(cat "$TMP/out")"
fi
F="$(suite h7c.test.sh "# host-shield: runs $HS_CMD -- this case proves the declared form is handed on" \
        "$HS_CMD -h +60")"
trip_clean
RC="$(shielded "$F")"
if [ "$RC" = 0 ] && grep -q "TRIPWIRE $HS_CMD -h +60" "$TRIP/trip.log"; then
    ok "H7c a 'runs' declaration hands the call to the next $HS_CMD on PATH (here the tripwire)"
else
    bad "H7c rc=$RC" "$(cat "$TMP/out"; echo '--- tripwire:'; cat "$TRIP/trip.log")"
fi

# H8 — a shielded run inside a shielded run: the inner suite is the one charged, once.
F="$(suite h8.test.sh "python3 -S '$HS_TOOL' run -- bash '$SUITES/hs-bare.test.sh'; exit 0")"
trip_clean
RC="$(shielded "$F")"
N="$(grep -c "^  FAIL  host-shield: refused" "$TMP/out")"
if [ "$RC" = 0 ] && [ "$N" = 1 ] && [ ! -s "$TRIP/trip.log" ]; then
    ok "H8  nested: the inner run reports the call once and the outer log stays empty (the outer suite exited 0)"
else
    bad "H8  rc=$RC, $N refusal line(s)" "$(cat "$TMP/out")"
fi

# H9 — a stand-in that cannot load the guard's rules refuses even a harmless form.
mkdir -p "$TMP/noguard/lib"
cp "$HS_TOOL" "$TMP/noguard/lib/"
cp -R "$HS_STANDINS" "$TMP/noguard/lib/"
: > "$TMP/h9.log"
trip_clean
RICHOS_HOST_SHIELD_LOG="$TMP/h9.log" PATH="$TMP/noguard/lib/host-shield:$CASE_PATH" "$TMP/noguard/lib/host-shield/$PM" -g batt \
    > "$TMP/out" 2>&1; RC=$?
if [ "$RC" = 1 ] && grep -q "could not be judged harmless" "$TMP/h9.log" && [ ! -s "$TRIP/trip.log" ]; then
    ok "H9  without the guard's rules a stand-in refuses even $PM -g: a call it cannot judge is never assumed harmless"
else
    bad "H9  rc=$RC" "$(cat "$TMP/out" "$TMP/h9.log")"
fi

# H10 — the shield leaves nothing behind: its log file is removed when the suite ends.
logs_in_tmpdir > "$TMP/logs-after"
LEFT="$(comm -13 "$TMP/logs-before" "$TMP/logs-after" | head -3)"
if [ -z "$LEFT" ]; then
    ok "H10 no richos-host-shield.*.log is left in TMPDIR by the runs above"
else
    bad "H10 left behind: $LEFT"
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "=== host_shield tests: all $PASS passed ==="
    exit 0
fi
echo "=== host_shield tests: $PASS passed, $FAIL FAILED ==="
exit 1
