#!/usr/bin/env bash
#
# guard-host-sleep.test.sh: the Bash rule that refuses an agent command holding the Mac awake or
# changing its sleep or power settings (scripts/hooks/guard-host-sleep.sh, a module of
# dispatch-pretooluse.sh; the rule is scripts/lib/host_sleep.py).
#
# 2026-10-29: an agent held `caffeinate -i -w <pid>` on the CEO's Mac for about seven minutes, and
# nothing refused it: guard-host-display-power.sh refuses only caffeinate -d and -u. ceo-decisions
# §65: nothing an agent runs changes the host's sleep state.
#
#   P1   POSITIVE CONTROL, the 2026-10-29 command shape, refused, and the refusal says what to do
#   P2   every caffeinate form: -i -s -t -w, bare, by path, -is with a utility, -d, -u
#   P3   every position: after && ; | & newline, in ( ), { }, $( ), backticks, if/then, sh -c,
#        bash -lc, sudo, env, nohup ... &, exec, time, nice -n, timeout <d>, xargs, VAR=x prefix
#   P4   pmset writes: sleepnow, displaysleepnow, noidle, -a/-c settings, schedule, relative
#   P5   the other sleep-assertion changes: systemsetup -set...sleep, defaults write
#        com.apple.PowerManagement, open -a <keep-awake app>, inline python/osascript code
#   P6   review 2026-10-29: a heredoc fed to a shell (its body is commands) or an interpreter
#        (its body is code) that starts caffeinate
#   P7   review 2026-10-30: that heredoc is still read after a here-string (<<<), a comment or
#        quoted text naming `<<EOF` (one heredoc scan, host_sleep.heredocs, shared with toolchain.py)
#   N1   reading is allowed: pmset -g, pmset -g assertions, bare pmset
#   N2   a MENTION is not a run: echo, grep, which, man, a commit message, a heredoc body, a comment,
#        a shell heredoc inside a file a heredoc writes
#   N3   the test VM: ssh <vm> caffeinate, ssh <vm> 'sudo pmset ...', a testvm/ script
#   N4   an unreadable payload, another tool's payload and a command naming none of it pass
#   D1   through the real dispatcher (dispatch-pretooluse.sh Bash) the refusal is exit 2 with its
#        text, and when the dispatcher is given only this rule by name (the app's lease form)
#
# Usage: scripts/hooks/guard-host-sleep.test.sh

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$SCRIPT_DIR/guard-host-sleep.sh"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2" | head -8; FAIL=$((FAIL + 1)); return 0; }

[ -f "$HOOK" ] || { echo "FATAL: missing $HOOK" >&2; exit 2; }

payload() { # <command> [tool]
    python3 -c 'import json,sys; print(json.dumps({"tool_name":sys.argv[2],"tool_input":{"command":sys.argv[1],"content":sys.argv[1]},"cwd":"/tmp","session_id":"test"}))' \
        "$1" "${2:-Bash}"
}
check() { # <expected rc> <id> <description> <command> [tool]
    local out rc
    out="$(payload "$4" "${5:-Bash}" | bash "$HOOK" 2>&1)"; rc=$?
    if [ "$rc" = "$1" ]; then ok "$2 $3"; else bad "$2 $3" "rc=$rc (expected $1) $out"; fi
    LAST_OUT="$out"
}

echo "=== guard-host-sleep: refused ==="
# shellcheck disable=SC2016  # the $-words are the literal command text under test
{
check 2 P1 "the 2026-10-29 shape: caffeinate -i -w <pid>" 'caffeinate -i -w 48213'
for want in "Nothing ran" "run_in_background" "test VM" "pmset -g"; do
    if printf '%s' "$LAST_OUT" | grep -qF "$want"; then
        ok "P1 the refusal carries '$want'"
    else
        bad "P1 the refusal does not carry '$want'" "$LAST_OUT"
    fi
done
check 2 P2 'caffeinate -is with a utility' 'caffeinate -is cargo build --release'
check 2 P2 'caffeinate -s' 'caffeinate -s'
check 2 P2 'caffeinate -t 600' 'caffeinate -t 600'
check 2 P2 'bare caffeinate' 'caffeinate'
check 2 P2 'by absolute path' '/usr/bin/caffeinate -i make'
check 2 P2 'caffeinate -d (also the display guard)' 'caffeinate -d'
check 2 P2 'caffeinate -dimsu' 'caffeinate -dimsu python3 scripts/nightly-local.py build'
check 2 P3 'after &&' 'cd /tmp && caffeinate -i -w 1'
check 2 P3 'after ;' 'echo hi; caffeinate -i'
check 2 P3 'after |' 'echo hi | caffeinate -i'
check 2 P3 'after & (backgrounded)' 'caffeinate -i -w 1 & echo started'
check 2 P3 'after ||' 'false || caffeinate -i'
check 2 P3 'after a newline' 'echo hi
caffeinate -i -w 1'
check 2 P3 'in a subshell' '( caffeinate -i -t 5 )'
check 2 P3 'in a brace group' '{ caffeinate -i -t 5; }'
check 2 P3 'in $( )' 'x="$(caffeinate -i -t 1)"'
check 2 P3 'in backticks' 'echo `caffeinate -i -t 1`'
check 2 P3 'in if/then' 'if true; then caffeinate -i -t 1; fi'
check 2 P3 'sh -c' 'sh -c "caffeinate -i -w 1"'
check 2 P3 'bash -lc, single quoted' "bash -lc 'caffeinate -s -t 30'"
check 2 P3 'sudo' 'sudo caffeinate -i'
check 2 P3 'env with an assignment' 'env FOO=1 caffeinate -i make'
check 2 P3 'nohup ... &' 'nohup caffeinate -i -w 99 >/dev/null 2>&1 &'
check 2 P3 'exec' 'exec caffeinate -i'
check 2 P3 'time' 'time caffeinate -i -t 1'
check 2 P3 'nice -n 5' 'nice -n 5 caffeinate -i'
check 2 P3 'timeout 60' 'timeout 60 caffeinate -i'
check 2 P3 'xargs' 'echo 1 | xargs caffeinate -w'
check 2 P3 'an assignment prefix' 'LC_ALL=C caffeinate -i'
check 2 P4 'pmset sleepnow' 'pmset sleepnow'
check 2 P4 'pmset displaysleepnow' 'pmset displaysleepnow'
check 2 P4 'pmset noidle' 'pmset noidle'
check 2 P4 'sudo pmset -a displaysleep 0' 'sudo pmset -a displaysleep 0'
check 2 P4 'pmset -c sleep 0' 'pmset -c sleep 0'
check 2 P4 'pmset schedule' 'pmset schedule wake "10/30/26 08:00:00"'
check 2 P4 'pmset relative' 'pmset relative wake 60'
check 2 P5 'systemsetup -setcomputersleep' 'sudo systemsetup -setcomputersleep Never'
check 2 P5 'defaults write PowerManagement' 'defaults write com.apple.PowerManagement "Sleep On Power Button" -bool NO'
check 2 P5 'open -a Amphetamine' 'open -a Amphetamine'
check 2 P5 'open -ga (flags clustered)' 'open -ga Amphetamine'
check 2 P5 'open of a keep-awake app by path' 'open /Applications/KeepingYouAwake.app'
check 2 P5 'python -c running caffeinate' "python3 -c 'import subprocess; subprocess.run([\"caffeinate\", \"-i\", \"-t\", \"60\"])'"
check 2 P5 'osascript -e running caffeinate' "osascript -e 'do shell script \"caffeinate -i -t 60\"'"
check 2 P5 'python -c creating an IOKit assertion' "python3 -c 'from ctypes import cdll; iokit.IOPMAssertionCreateWithName(1)'"
check 2 P6 'a heredoc fed to python' "python3 - <<'EOF'
import subprocess; subprocess.run(['caffeinate', '-i', '-t', '5'])
EOF"
check 2 P6 'a heredoc fed to bash' "bash <<'EOF'
caffeinate -i -t 5
EOF"
check 2 P6 'a heredoc fed to sh behind nohup, unquoted delimiter' "nohup sh <<EOF
echo start
caffeinate -s -t 5
EOF"
check 2 P7 'a heredoc fed to bash after a here-string' "cat <<< hello
bash <<'EOF'
caffeinate -i -t 5
EOF"
check 2 P7 'a heredoc fed to bash after a comment naming <<EOF' "ls  # writes <<EOF somewhere
bash <<'EOF'
caffeinate -i -t 5
EOF"
check 2 P7 'a heredoc fed to bash after quoted <<EOF' "echo \"use <<EOF here\"
bash <<'X'
caffeinate -i -t 5
X"
}

echo "=== guard-host-sleep: allowed ==="
# shellcheck disable=SC2016
{
check 0 N1 'pmset -g' 'pmset -g'
check 0 N1 'pmset -g assertions' 'pmset -g assertions'
check 0 N1 'pmset -g batt in a pipeline' 'pmset -g batt | head -2'
check 0 N1 'bare pmset (usage)' 'pmset'
check 0 N1 'systemsetup -getcomputersleep' 'systemsetup -getcomputersleep'
check 0 N1 'defaults read PowerManagement' 'defaults read com.apple.PowerManagement'
check 0 N2 'echo naming caffeinate' 'echo "never run caffeinate -i on the host"'
check 0 N2 'grep for caffeinate' 'grep -rn "caffeinate -i" scripts'
check 0 N2 'which caffeinate' 'which caffeinate'
check 0 N2 'man pmset' 'man pmset'
check 0 N2 'a commit message' "git commit -m 'guard: refuse caffeinate and pmset sleepnow'"
check 0 N2 'a heredoc body' "cat > notes.sh <<'EOF'
caffeinate -i -w 1
EOF"
check 0 N2 'a heredoc fed to bash that only mentions it' "bash <<'EOF'
echo 'never run caffeinate on the host'
EOF"
check 0 N3 'a heredoc sent to the guest' "ssh vm bash <<'EOF'
caffeinate -i -t 60
EOF"
check 0 N2 'a shell heredoc inside a file written by a heredoc' "cat > notes.sh <<'SCRIPT'
bash <<'BUILD'
caffeinate -i -t 5
BUILD
SCRIPT"
check 0 N2 'a comment' 'ls # caffeinate -i'
check 0 N2 'a path that names it' 'cat scripts/lib/host-shield/caffeinate'
check 0 N2 'open of an ordinary file' 'open -R notes/caffeine-review.md'
check 0 N3 'ssh to the guest' 'ssh admin@vm caffeinate -i -t 60'
check 0 N3 'ssh to the guest, sudo pmset' "ssh vm 'sudo pmset -a displaysleep 0'"
check 0 N3 'a testvm script' 'richos/app/scripts/testvm/run.sh vm caffeinate -i'
check 0 N4 'a command naming none of it' 'git status --short'
check 0 N4 'a Write payload carrying the word' 'caffeinate -i -w 1' Write
}

echo "=== unreadable payload ==="
out="$(printf 'not json' | bash "$HOOK" 2>&1)"; rc=$?
if [ "$rc" = 0 ]; then ok "N4 an unreadable payload is not refused"; else bad "N4 an unreadable payload was refused" "rc=$rc $out"; fi

echo "=== through the dispatcher ==="
out="$(payload 'caffeinate -i -w 48213' | bash "$SCRIPT_DIR/dispatch-pretooluse.sh" Bash 2>&1)"; rc=$?
if [ "$rc" = 2 ] && printf '%s' "$out" | grep -q "guard-host-sleep"; then
    ok "D1 dispatch-pretooluse.sh Bash runs this module and returns its refusal"
else
    bad "D1 dispatch-pretooluse.sh Bash runs this module and returns its refusal" "rc=$rc $out"
fi
out="$(payload 'caffeinate -i -w 48213' | bash "$SCRIPT_DIR/dispatch-pretooluse.sh" Bash guard-host-sleep.sh 2>&1)"; rc=$?
if [ "$rc" = 2 ] && printf '%s' "$out" | grep -q "guard-host-sleep"; then
    ok "D1 the dispatcher given this rule by name (the app lease's form) refuses"
else
    bad "D1 the dispatcher given this rule by name (the app lease's form) refuses" "rc=$rc $out"
fi
out="$(payload 'pmset -g assertions' | bash "$SCRIPT_DIR/dispatch-pretooluse.sh" Bash guard-host-sleep.sh 2>&1)"; rc=$?
if [ "$rc" = 0 ]; then ok "D1 the dispatcher passes pmset -g"; else bad "D1 the dispatcher passes pmset -g" "rc=$rc $out"; fi

printf 'guard-host-sleep: %d passed, %d FAILED\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
