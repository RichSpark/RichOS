#!/usr/bin/env bash
#
# guard-no-shell-c.test.sh: the Bash rule that refuses, before Claude Code asks the user, an inline
# `bash -c '<script>'` (scripts/hooks/guard-no-shell-c.sh, a module of dispatch-pretooluse.sh; the
# rule is scripts/lib/shell_c.py).
#
# The CEO, 2026-10-09, saw "This shell -c script runs rm and could not be checked. Do you want to
# proceed?" and hit yes.
#
#   P1   POSITIVE CONTROL: the shape that prompted him, refused with the message and the fix
#   P2   every spelling: bash/sh/zsh/dash, a path, -c, -lc, -ec, -xc, -o pipefail -c
#   P3   every position: after && ; | newline, in ( ), $( ), backticks, sudo/env/nohup/xargs, find -exec
#   N1   a script file (bash file.sh, bash -x file.sh, sh ./x.sh, bash < file) and ordinary commands pass
#   N2   a MENTION is not a call: echo, a commit message, a heredoc body
#   N3   an unreadable payload and a non-Bash tool pass
#   N5   a hyphen inside a path (the session scratch folder /-Users-<user>-...) is not an option word
#   P9   with that fixed, bash -c, sh -lc and a later sh -c are still refused
#   D1   through the real dispatcher (dispatch-pretooluse.sh Bash) the refusal is exit 2 with its text
#
# Usage: scripts/hooks/guard-no-shell-c.test.sh

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$SCRIPT_DIR/guard-no-shell-c.sh"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2" | head -6; FAIL=$((FAIL + 1)); return 0; }

[ -f "$HOOK" ] || { echo "FATAL: missing $HOOK" >&2; exit 2; }

payload() { # <command>
    python3 -c 'import json,sys; print(json.dumps({"tool_name":"Bash","tool_input":{"command":sys.argv[1]},"session_id":"test"}))' "$1"
}
check() { # <expected rc> <id> <description> <command>
    local out rc
    out="$(payload "$4" | bash "$HOOK" 2>&1)"; rc=$?
    if [ "$rc" = "$1" ]; then ok "$2 $3"; else bad "$2 $3" "rc=$rc (expected $1) $out"; fi
    LAST_OUT="$out"
}

echo "=== guard-no-shell-c: refused ==="
check 2 P1 "the 2026-10-09 shape" "bash -c 'rm -rf /tmp/x/old && echo done'"
if printf '%s' "$LAST_OUT" | grep -qF 'write the script to a file' && printf '%s' "$LAST_OUT" | grep -qF 'bash <file>'; then
    ok "P1 the refusal says what to do instead"
else
    bad "P1 the refusal does not say what to do instead" "$LAST_OUT"
fi
check 2 P2 "sh -c" "sh -c 'echo hi'"
check 2 P2 "zsh -c" "zsh -c 'echo hi'"
check 2 P2 "dash -c" "dash -c 'echo hi'"
check 2 P2 "a path" "/bin/bash -c 'echo hi'"
check 2 P2 "-lc" "bash -lc 'echo hi'"
check 2 P2 "-ec" "bash -ec 'echo hi'"
check 2 P2 "-xc" "sh -xc 'echo hi'"
check 2 P2 "-o pipefail -c" "bash -o pipefail -c 'echo hi'"
check 2 P2 "double quotes" 'bash -c "echo hi"'
check 2 P3 "after &&" "cd /tmp && bash -c 'echo hi'"
check 2 P3 "after ;" "cd /tmp; bash -c 'echo hi'"
check 2 P3 "after a pipe" "echo hi | bash -c 'cat'"
check 2 P3 "after a newline" $'cd /tmp\nbash -c \'echo hi\''
check 2 P3 "in a subshell" "(bash -c 'echo hi')"
check 2 P3 "in a substitution" 'x=$(bash -c "echo hi")'
check 2 P3 "after sudo" "sudo bash -c 'echo hi'"
check 2 P3 "after env" "env FOO=1 bash -c 'echo hi'"
check 2 P3 "after a variable assignment" "FOO=1 bash -c 'echo hi'"
check 2 P3 "after nohup" "nohup bash -c 'sleep 1' &"
check 2 P3 "xargs" "ls | xargs -I{} bash -c 'echo {}'"
check 2 P3 "find -exec" "find . -name '*.tmp' -exec sh -c 'rm \"\$1\"' _ {} \\;"

echo "=== guard-no-shell-c: second-review cases (plain match over the whole text) ==="
check 2 R1 "quoted shell name" "'bash' -c 'echo hi'"
check 2 R1 "double-quoted shell name" '"sh" -c "echo hi"'
check 2 R1 "shell inside a quoted substitution" 'echo "$(bash -c "echo hi")"'
check 2 R2 "--noprofile before -c" "bash --noprofile -c 'echo hi'"
check 2 R2 "xargs -I ITEM operand" "ls | xargs -I ITEM bash -c 'echo ITEM'"
check 2 R3 "a tab instead of a space" $'bash\t-c\t\'echo hi\''
check 2 R3 "a tab before the option" $'x=1\tbash\t-lc\t\'echo hi\''
check 2 R4 "after a quoted heredoc mention" $'cat <<\'EOF\'\nbash -c is banned\nEOF\nbash -c \'echo hi\''
check 2 R5 "after a ; inside a comment" $'echo a # note; x\nbash -c \'echo hi\''
check 2 R6 "mention: echo (accepted false positive)" "echo 'run bash -c here'"
check 2 R6 "mention: commit message (accepted false positive)" 'git commit -m "refuse bash -c wrappers"'
if printf '%s' "$LAST_OUT" | grep -qF 'git commit -F <file>'; then
    ok "R6 the mention refusal says how to avoid it"
else
    bad "R6 the mention refusal does not say how to avoid it" "$LAST_OUT"
fi

echo "=== guard-no-shell-c: passes ==="
check 0 N1 "bash file.sh" "bash /Volumes/E1TB/tmp/claude/zach/run.sh"
check 0 N1 "bash -x file.sh" "bash -x ./run.sh arg"
check 0 N1 "sh ./x.sh" "sh ./x.sh"
check 2 P4 "accepted false positive: script with its own -c" "bash ./run.sh -c config"
check 0 N1 "bash reading a file" "bash < ./run.sh"
check 0 N1 "an ordinary command" "ls -la /tmp && git -C /tmp status"
check 0 N1 "a tool with -c" "python3 -c 'print(1)'"
check 0 N1 "git -c" "git -c core.editor=true commit -m x"
check 2 P1 "adjacent redirection bash -c</dev/null" "bash -c</dev/null 'printf INLINE'"
check 2 P2 "mention with attached semicolon" "echo 'bash -c'; printf ORDINARY"
check 2 P3 "bash -o pipefail -c still refused" "bash -o pipefail -c 'x'"
check 0 N1 "bash script then git diff --cached" "bash test.sh && git diff --cached"
check 2 P5 "redirection before -c" "bash</dev/null -c 'printf INLINE'"
check 2 P5 "spaced redirection before -c" "bash </dev/null -c 'printf INLINE'"
check 2 P5 "-o operand then redirection" "bash -o pipefail</dev/null -c 'printf INLINE'"
check 2 P5 "-O extglob -c" "bash -O extglob -c 'printf INLINE'"
check 2 P5 "--rcfile f -c" "bash --rcfile /dev/null -c 'printf INLINE'"
check 2 P5 "-eo pipefail -c" "bash -eo pipefail -c 'printf INLINE'"
check 2 P6 "zsh -f -c0 (digit in the group)" "zsh -f -c0 'printf INLINE'"
check 2 P6 "ksh -c- (dash in the group)" "ksh -c- 'printf INLINE'"
check 2 P7 "line continuation before -c" $'bash \\\n-c \'printf INLINE\''
check 2 P7 "&> redirection before -c" "bash &>/dev/null -c 'printf INLINE'"
check 2 P7 "pipe to grep -c after a shell word (accepted false positive)" "bash x.sh | grep -c y"
check 2 P8 "&> attached to the shell name" "bash&>/dev/null -c 'printf INLINE'"
check 2 P8 "line continuation attached to the shell name" $'bash\\\n-c \'printf INLINE\''
check 2 P8 "pipe attached to the shell name" "bash|grep -c y"
check 2 P8 "semicolon attached to the shell name" "bash; grep -c y"
check 0 N4 "script name test.sh then -c word" "test.sh -c x"
check 0 N4 "bashrc is not a shell word" "bashrc -c x"
check 0 N4 "ssh -C host" "ssh -C host"

echo "=== guard-no-shell-c: a hyphen inside a path is not an option (2026-10-24) ==="
# A Claude session's scratch folder has the shape /private/tmp/claude-<uid>/-Users-<user>-<project>/...;
# the hyphen run after the / was read as an option group containing c. The three command shapes it
# refused, with a fictional user and project:
SESSION=/private/tmp/claude-501/-Users-sample-ab-demoproject/0a1b2c3d-0000-4000-8000-000000000000/scratchpad
check 0 N5 "install.sh with a log in the session scratch folder" "cd /Users/sample/demo/engine && bash scripts/hooks/install.sh > $SESSION/install.log 2>&1"
check 0 N5 "a script file in the session scratch folder" "bash $SESSION/mutant-rerun.sh"
check 0 N5 "sh a script, log in the session scratch folder" "sh /Users/sample/demo-state/mutation-proof/run-1/rerun/01-shell-c-mutant.sh > $SESSION/mutant-rerun.log 2>&1"
check 0 N5 "a hyphen word after a path separator" "bash /tmp/x/-c/run.sh"
check 2 P9 "bash -c 'x' still refused" "bash -c 'x'"
check 2 P9 "sh -lc x still refused" "sh -lc x"
check 2 P9 "a script file, then sh -c" "bash /tmp/a.sh; sh -c y"
check 2 P9 "a session-path script, then bash -c" "bash $SESSION/a.sh && bash -c 'x'"
check 2 P9 "a quoted option word" "bash '-c' 'x'"
check 0 N3 "unreadable payload" "x"
out="$(printf 'not json' | bash "$HOOK" 2>&1)"; rc=$?
[ "$rc" = 0 ] && ok "N3 unreadable payload passes" || bad "N3 unreadable payload" "rc=$rc $out"
out="$(printf '%s' '{"tool_name":"Read","tool_input":{"command":"bash -c x"}}' | bash "$HOOK" 2>&1)"; rc=$?
[ "$rc" = 0 ] && ok "N3 a non-Bash tool passes" || bad "N3 a non-Bash tool" "rc=$rc $out"

echo "=== guard-no-shell-c: through the dispatcher ==="
out="$(payload "bash -c 'rm -rf /tmp/x'" | bash "$SCRIPT_DIR/dispatch-pretooluse.sh" Bash 2>&1)"; rc=$?
if [ "$rc" = 2 ] && printf '%s' "$out" | grep -qF 'guard-no-shell-c: BLOCKED'; then
    ok "D1 dispatcher refuses bash -c with the message"
else
    bad "D1 dispatcher" "rc=$rc $out"
fi
out="$(payload "bash /tmp/x.sh" | bash "$SCRIPT_DIR/dispatch-pretooluse.sh" Bash 2>&1)"; rc=$?
if printf '%s' "$out" | grep -qF 'guard-no-shell-c'; then bad "D1 dispatcher passes a script file" "$out"; else ok "D1 dispatcher lets a script file past this rule"; fi

echo ""
echo "passed=$PASS failed=$FAIL"
[ "$FAIL" = 0 ]
