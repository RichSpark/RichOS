#!/usr/bin/env bash
#
# dispatch-pretooluse.test.sh — THE RUNNER SHAPE, PROVEN.
#
# scripts/hooks/dispatch-pretooluse.sh runs N rule modules inside ONE process
# where the host used to run N processes. That trade buys a large latency win
# and takes on one new risk in exchange: twelve rules now share an address
# space, a set of shell options, a stdin and an exit code. Every case below is
# about one of those four.
#
# THE CASES THAT MATTER MOST ARE THE ISOLATION ONES, NOT THE REFUSALS.
# A dispatcher that forwards a refusal is easy. A dispatcher that keeps running
# after one module dies, that does not let module 3's `set -u` reach module 4,
# and that never reports a rule which DID NOT RUN as a rule that passed — that
# is the whole reason this shape is allowed to exist. A rule silently skipped
# is indistinguishable from a rule that found nothing, and this engine's own
# unevaluated-payload library exists because that confusion already happened
# once here, to 17 of 25 PreToolUse guards.
#
# Cases D1-D16 run against a SANDBOX engine with synthetic modules, so a module
# that blocks, errors, hangs on options or writes to stdout can be summoned on
# demand. Cases D17-D22 run against the SHIPPED tree and assert the properties
# the real manifest and the real registration must hold.
#
# The mutation harness proving each assertion load-bearing is
# scripts/hooks/dispatch-pretooluse.mutation.sh, invoked at the end — a harness
# nobody runs proves nothing about anything.
#
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DISPATCHER="$SCRIPT_DIR/dispatch-pretooluse.sh"
MANIFEST="$SCRIPT_DIR/dispatch-pretooluse.manifest"
HOOKS_JSON="$ENGINE_ROOT/hooks/hooks.json"

unset CLAUDE_PROJECT_DIR RICHOS_ENTITY_ROOT RICHOS_ENGINE_ROOT 2>/dev/null || true

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

[ -f "$DISPATCHER" ] || { echo "FATAL: $DISPATCHER missing" >&2; exit 1; }
[ -f "$MANIFEST" ]   || { echo "FATAL: $MANIFEST missing" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

SANDBOX="$(cd "$(mktemp -d -t dispatch-pretooluse-test.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

# --- the sandbox engine ----------------------------------------------------
# A real engine subtree, so the dispatcher's own bootstrap block resolves a real
# root out of a real adoption marker rather than a stub. The rule modules are
# synthetic: each one does exactly one thing, so a case can name the behavior it
# is about without a real guard's reasons in the way.
SBE="$SANDBOX/engine"
mkdir -p "$SBE/scripts/hooks" "$SBE/scripts/lib" "$SBE/hooks"
cp "$DISPATCHER" "$SBE/scripts/hooks/"
cp "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" \
   "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" \
   "$ENGINE_ROOT/scripts/lib/unevaluated-notice.sh" "$SBE/scripts/lib/"
cp "$ENGINE_ROOT/orchestration.config" "$SBE/orchestration.config"
SBD="$SBE/scripts/hooks"

w() { printf '%s\n' "$2" > "$SBD/$1"; }

w t-pass.sh          'INPUT="$(cat)"; exit 0'
w t-block.sh         'INPUT="$(cat)"; echo "BLOCK-ALPHA refused this call" >&2; exit 2'
w t-block2.sh        'INPUT="$(cat)"; echo "BLOCK-BETA refused this call" >&2; exit 2'
w t-noise.sh         'INPUT="$(cat)"; echo "NOTICE-GAMMA saw this call" >&2; exit 0'
w t-rc1.sh           'INPUT="$(cat)"; echo "DELTA could not start" >&2; exit 1'
w t-rc127.sh         'INPUT="$(cat)"; exit 127'
w t-stdout.sh        'INPUT="$(cat)"; printf "{\"systemMessage\":\"EPSILON\"}\n"; exit 0'
w t-stdout2.sh       'INPUT="$(cat)"; printf "{\"systemMessage\":\"ZETA\"}\n"; exit 0'
w t-echo-payload.sh  'INPUT="$(cat)"; printf "%s" "$INPUT" | cksum | tr -d " /" >&2; exit 0'
# Sets a shell option and a variable, then exits. If either escapes its subshell
# the module after it changes behavior.
w t-leak.sh          'INPUT="$(cat)"; set -u; LEAKED=yes; export LEAKED; exit 0'
# Relies on bash SCRIPT DEFAULTS. The unset variable is read BARE, not with
# `${x:-}`: the default form is safe under `set -u` and would have made this
# fixture prove nothing — which is exactly what the mutation harness caught on
# its first run, with `options-leak` surviving.
w t-needs-defaults.sh 'INPUT="$(cat)"; echo "UNSET=[$NOT_SET_ANYWHERE]" >&2; false; echo "ETA survived" >&2; exit 0'

mkmanifest() { printf '%s\n' "$@" > "$SBD/dispatch-pretooluse.manifest"; }

# A payload whose cwd is the sandbox engine, so the dispatcher's root resolution
# has a real adopted root to find.
PAYLOAD="$SANDBOX/payload.json"
python3 - "$SBE" > "$PAYLOAD" <<'PY'
import json, sys
print(json.dumps({"session_id": "t", "cwd": sys.argv[1],
                  "hook_event_name": "PreToolUse", "tool_name": "Bash",
                  "tool_input": {"command": "echo hello", "description": "d"}}))
PY

run() { # <chain-key> -> sets RC / OUT / ERR
    OUT="$(bash "$SBD/dispatch-pretooluse.sh" "$1" < "$PAYLOAD" 2>"$SANDBOX/err")"
    RC=$?
    ERR="$(cat "$SANDBOX/err")"
}

echo "=== dispatch-pretooluse.sh — the runner shape ==="
echo
echo "--- A. the chain runs, and its verdict is the set's verdict ---"

mkmanifest 'Bash|t-pass.sh' 'Bash|t-pass.sh'
run Bash
if [ "$RC" = 0 ] && [ -z "$ERR" ] && [ -z "$OUT" ]; then
    ok "D1  all rules allow -> exit 0, and the chain is SILENT"
else
    bad "D1  all rules allow -> exit 0, and the chain is SILENT (rc=$RC err=$ERR out=$OUT)"
fi

mkmanifest 'Bash|t-pass.sh' 'Bash|t-block.sh' 'Bash|t-pass.sh'
run Bash
if [ "$RC" = 2 ] && printf '%s' "$ERR" | grep -q 'BLOCK-ALPHA refused this call'; then
    ok "D2  one rule blocks -> exit 2, and its refusal reaches stderr verbatim"
else
    bad "D2  one rule blocks -> exit 2, and its refusal reaches stderr verbatim (rc=$RC err=$ERR)"
fi

# The host ran all twelve registrations regardless of what the first decided, so
# a call refused by two guards showed BOTH reasons. Reporting only the first
# would send the operator round the loop twice.
mkmanifest 'Bash|t-block.sh' 'Bash|t-pass.sh' 'Bash|t-block2.sh'
run Bash
if [ "$RC" = 2 ] \
   && printf '%s' "$ERR" | grep -q 'BLOCK-ALPHA' \
   && printf '%s' "$ERR" | grep -q 'BLOCK-BETA'; then
    ok "D3  TWO rules block -> BOTH refusals are reported, not just the first"
else
    bad "D3  TWO rules block -> BOTH refusals are reported, not just the first (rc=$RC err=$ERR)"
fi

mkmanifest 'Bash|t-block2.sh' 'Bash|t-block.sh'
run Bash
FIRST_LINE="$(printf '%s' "$ERR" | grep -n 'BLOCK-' | head -1)"
if printf '%s' "$FIRST_LINE" | grep -q 'BLOCK-BETA'; then
    ok "D4  refusals are reported in MANIFEST ORDER, which is registration order"
else
    bad "D4  refusals are reported in MANIFEST ORDER, which is registration order (first=$FIRST_LINE)"
fi

mkmanifest 'Bash|t-noise.sh' 'Bash|t-pass.sh'
run Bash
if [ "$RC" = 0 ] && printf '%s' "$ERR" | grep -q 'NOTICE-GAMMA saw this call'; then
    ok "D5  a NON-blocking rule's notice still reaches stderr, and does not block"
else
    bad "D5  a NON-blocking rule's notice still reaches stderr, and does not block (rc=$RC err=$ERR)"
fi

echo
echo "--- B. one rule failing must not take the others down ---"

# THE CASE THIS WHOLE FILE EXISTS FOR. A module that is not on disk is a rule
# that did not evaluate the call. Counting it as a pass is how a defense becomes
# a rumour; the other rules must still run and their verdicts must still stand.
mkmanifest 'Bash|t-absent-module.sh' 'Bash|t-block.sh' 'Bash|t-noise.sh'
run Bash
if [ "$RC" = 2 ] \
   && printf '%s' "$ERR" | grep -q 't-absent-module.sh' \
   && printf '%s' "$ERR" | grep -q 'NOT PRESENT' \
   && printf '%s' "$ERR" | grep -q 'BLOCK-ALPHA' \
   && printf '%s' "$ERR" | grep -q 'NOTICE-GAMMA'; then
    ok "D6  a MISSING rule module is named as not-evaluated, and every sibling still runs"
else
    bad "D6  a MISSING rule module is named as not-evaluated, and every sibling still runs (rc=$RC err=$ERR)"
fi

mkmanifest 'Bash|t-absent-module.sh'
run Bash
if [ "$RC" = 0 ] && printf '%s' "$ERR" | grep -q 'NOT PRESENT'; then
    ok "D6b a MISSING rule does not block by itself — it has no verdict, only an announcement"
else
    bad "D6b a MISSING rule does not block by itself — it has no verdict, only an announcement (rc=$RC err=$ERR)"
fi

mkmanifest 'Bash|t-rc1.sh' 'Bash|t-noise.sh'
run Bash
if [ "$RC" = 0 ] \
   && printf '%s' "$ERR" | grep -q 't-rc1.sh' \
   && printf '%s' "$ERR" | grep -q 'DID NOT EVALUATE' \
   && printf '%s' "$ERR" | grep -q 'NOTICE-GAMMA'; then
    ok "D7  a rule exiting 1 is ANNOUNCED as not-evaluated, is NOT a block, and siblings run"
else
    bad "D7  a rule exiting 1 is ANNOUNCED as not-evaluated, is NOT a block, and siblings run (rc=$RC err=$ERR)"
fi

mkmanifest 'Bash|t-rc127.sh' 'Bash|t-block.sh'
run Bash
if [ "$RC" = 2 ] \
   && printf '%s' "$ERR" | grep -q 't-rc127.sh' \
   && printf '%s' "$ERR" | grep -q 'exited 127' \
   && printf '%s' "$ERR" | grep -q 'BLOCK-ALPHA'; then
    ok "D8  a rule exiting 127 is named with its code, and a sibling's block still stands"
else
    bad "D8  a rule exiting 127 is named with its code, and a sibling's block still stands (rc=$RC err=$ERR)"
fi

echo
# Exit-zero stderr is hidden by the host. Require one visible JSON envelope.
for broken in t-absent-module.sh t-rc1.sh t-rc127.sh; do
    mkmanifest "Bash|$broken"
    run Bash
    if [ "$RC" = 0 ] && printf '%s' "$OUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert sys.argv[1] in d["systemMessage"]; assert "DID NOT EVALUATE" in d["systemMessage"]; assert "hookSpecificOutput" not in d' "$broken" 2>/dev/null; then
        ok "D7b $broken is visible in the host systemMessage without inventing a verdict"
    else
        bad "D7b $broken has no valid visible failure notice (rc=$RC out=$OUT)"
    fi
done

w t-envelope.sh 'cat >/dev/null; echo '\''{"systemMessage":"KEEP-ME","hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","additionalContext":"KEEP-CONTEXT"}}'\''; exit 0'
mkmanifest 'Bash|t-absent-module.sh' 'Bash|t-rc1.sh' 'Bash|t-envelope.sh'
run Bash
if [ "$RC" = 0 ] && printf '%s' "$OUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert all(x in d["systemMessage"] for x in ["KEEP-ME", "t-absent-module.sh", "t-rc1.sh"]); assert d["hookSpecificOutput"] == {"hookEventName":"PreToolUse", "permissionDecision":"ask", "additionalContext":"KEEP-CONTEXT"}' 2>/dev/null; then
    ok "D7c failures merge into one envelope preserving sibling control and context"
else
    bad "D7c failures lost sibling output (rc=$RC out=$OUT)"
fi

# Remove the pid record to exercise the no-start report branch deterministically.
cp "$SBD/dispatch-pretooluse.sh" "$SANDBOX/original-dispatcher"
python3 - "$SBD/dispatch-pretooluse.sh" <<'PYINNER'
import pathlib, sys
p=pathlib.Path(sys.argv[1]); s=p.read_text()
s=s.replace('printf \'%s\\n\' "$!" > "$_dsp_slot.pid"', ': omitted-pid')
p.write_text(s)
PYINNER
mkmanifest 'Bash|t-pass.sh'
run Bash
if [ "$RC" = 0 ] && printf '%s' "$OUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "t-pass.sh" in d["systemMessage"]; assert "could not be started" in d["systemMessage"]' 2>/dev/null; then
    ok "D7d a rule without a process status gets a visible failure notice"
else
    bad "D7d a rule without a process status is invisible (rc=$RC out=$OUT)"
fi
cp "$SANDBOX/original-dispatcher" "$SBD/dispatch-pretooluse.sh"

mkmanifest 'Bash|t-absent-module.sh' 'Bash|t-stdout.sh' 'Bash|t-stdout2.sh'
run Bash
if [ "$RC" = 0 ] && printf '%s' "$OUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "2 rules" in d["systemMessage"]; assert "t-absent-module.sh" in d["systemMessage"]' 2>/dev/null; then
    ok "D7e a stdout collision does not swallow a failure notice"
else
    bad "D7e a stdout collision swallowed a failure notice (out=$OUT)"
fi

w t-invalid.sh 'cat >/dev/null; echo not-json; exit 0'
mkmanifest 'Bash|t-rc1.sh' 'Bash|t-invalid.sh'
run Bash
if [ "$RC" = 0 ] && printf '%s' "$OUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "invalid hook output" in d["systemMessage"]; assert "t-rc1.sh" in d["systemMessage"]' 2>/dev/null; then
    ok "D7f malformed sibling stdout cannot suppress the failure notice"
else
    bad "D7f malformed sibling stdout suppressed the failure notice (out=$OUT)"
fi

python3 - "$SBD/dispatch-pretooluse.sh" <<'PYINNER'
import pathlib, sys
p=pathlib.Path(sys.argv[1]); s=p.read_text()
s=s.replace('if python3 - "$_DSP_WORK"', 'if false && python3 - "$_DSP_WORK"')
p.write_text(s)
PYINNER
mkmanifest 'Bash|t-absent-module.sh' 'Bash|t-rc1.sh'
run Bash
if [ "$RC" = 0 ] && printf '%s' "$OUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "t-absent-module.sh" in d["systemMessage"]; assert "t-rc1.sh" in d["systemMessage"]; assert "\n" in d["systemMessage"]' 2>/dev/null; then
    ok "D7g a broken JSON encoder still produces a valid visible notice"
else
    bad "D7g a broken JSON encoder silenced the failure notice (out=$OUT)"
fi
cp "$SANDBOX/original-dispatcher" "$SBD/dispatch-pretooluse.sh"

echo "--- C. the shared address space does not leak ---"

# t-leak.sh turns on `set -u` and exports a variable. t-needs-defaults.sh then
# reads an unset variable and runs a failing command, both of which are fatal
# under the options t-leak.sh set. If the options or the variable escaped their
# subshell, ETA never survives.
mkmanifest 'Bash|t-leak.sh' 'Bash|t-needs-defaults.sh'
run Bash
if [ "$RC" = 0 ] && printf '%s' "$ERR" | grep -q 'ETA survived'; then
    ok "D9  a rule's SHELL OPTIONS do not reach the next rule"
else
    bad "D9  a rule's SHELL OPTIONS do not reach the next rule (rc=$RC err=$ERR)"
fi

w t-reads-leak.sh 'INPUT="$(cat)"; echo "LEAKED=[${LEAKED:-none}]" >&2; exit 0'
mkmanifest 'Bash|t-leak.sh' 'Bash|t-reads-leak.sh'
run Bash
if printf '%s' "$ERR" | grep -q 'LEAKED=\[none\]'; then
    ok "D10 a rule's VARIABLES do not reach the next rule"
else
    bad "D10 a rule's VARIABLES do not reach the next rule (err=$ERR)"
fi

# A module that exits must exit ITSELF, never the dispatcher: if `exit 2` ended
# the dispatcher, every rule after it would be silently skipped — which is the
# D6/D7 failure again, arriving by a different door.
mkmanifest 'Bash|t-block.sh' 'Bash|t-noise.sh' 'Bash|t-block2.sh'
run Bash
if printf '%s' "$ERR" | grep -q 'NOTICE-GAMMA' && printf '%s' "$ERR" | grep -q 'BLOCK-BETA'; then
    ok "D11 a rule's exit ends the RULE, never the chain — later rules still run"
else
    bad "D11 a rule's exit ends the RULE, never the chain — later rules still run (err=$ERR)"
fi

echo
echo "--- D. every rule gets the same payload, on its own stdin ---"

mkmanifest 'Bash|t-echo-payload.sh' 'Bash|t-echo-payload.sh' 'Bash|t-echo-payload.sh'
run Bash
SUMS="$(printf '%s\n' "$ERR" | sort -u | grep -c . || true)"
EXPECT_SUM="$(tr -d '\n' < "$PAYLOAD" | cksum | tr -d ' /')"
if [ "$SUMS" = 1 ] && printf '%s' "$ERR" | grep -qx "$EXPECT_SUM"; then
    ok "D12 every rule reads the SAME payload, byte-for-byte, on its own stdin"
else
    bad "D12 every rule reads the SAME payload, byte-for-byte, on its own stdin (distinct=$SUMS err=$ERR)"
fi

echo
echo "--- E. stdout is an envelope, and two envelopes are not one envelope ---"

mkmanifest 'Bash|t-pass.sh' 'Bash|t-stdout.sh'
run Bash
if [ "$OUT" = '{"systemMessage":"EPSILON"}' ]; then
    ok "D13 ONE rule writing stdout has its envelope forwarded verbatim"
else
    bad "D13 ONE rule writing stdout has its envelope forwarded verbatim (out=$OUT)"
fi

# Concatenating two JSON objects produces text that is not JSON. The host drops
# it silently, so both rules' envelopes vanish and nothing says so.
mkmanifest 'Bash|t-stdout.sh' 'Bash|t-stdout2.sh'
run Bash
if printf '%s' "$OUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if "2 rules" in d.get("systemMessage","") else 1)' 2>/dev/null; then
    ok "D14 TWO rules writing stdout -> one VALID envelope naming the collision, never invalid JSON"
else
    bad "D14 TWO rules writing stdout -> one VALID envelope naming the collision, never invalid JSON (out=$OUT)"
fi
# D14b (hunt part 3, 30): the merged envelope CARRIES both rules' messages.
if printf '%s' "$OUT" | python3 -c 'import json,sys; m=json.load(sys.stdin)["systemMessage"]; sys.exit(0 if "EPSILON" in m and "ZETA" in m else 1)' 2>/dev/null; then
    ok "D14b TWO rules writing stdout -> the one envelope keeps BOTH rules' messages"
else
    bad "D14b TWO rules writing stdout -> the one envelope keeps BOTH rules' messages (out=$OUT)"
fi

echo
echo "--- F. the dispatcher refuses rather than guesses ---"

OUT="$(bash "$SBD/dispatch-pretooluse.sh" < "$PAYLOAD" 2>"$SANDBOX/err")"; RC=$?
ERR="$(cat "$SANDBOX/err")"
if [ "$RC" = 2 ] && printf '%s' "$ERR" | grep -q 'no chain key'; then
    ok "D15 NO chain key -> exit 2 naming the problem, never a silent pass"
else
    bad "D15 NO chain key -> exit 2 naming the problem, never a silent pass (rc=$RC err=$ERR)"
fi

mkmanifest 'Bash|t-pass.sh'
run Agent
if [ "$RC" = 2 ] && printf '%s' "$ERR" | grep -q "NO rules for chain 'Agent'"; then
    ok "D16 a chain key the manifest does not name -> exit 2, never an empty silent chain"
else
    bad "D16 a chain key the manifest does not name -> exit 2, never an empty silent chain (rc=$RC err=$ERR)"
fi

mv "$SBD/dispatch-pretooluse.manifest" "$SBD/manifest.parked"
run Bash
if [ "$RC" = 2 ] && printf '%s' "$ERR" | grep -q 'UNEVALUATED'; then
    ok "D17 a MISSING manifest -> exit 2 saying every rule is unevaluated"
else
    bad "D17 a MISSING manifest -> exit 2 saying every rule is unevaluated (rc=$RC err=$ERR)"
fi
mv "$SBD/manifest.parked" "$SBD/dispatch-pretooluse.manifest"

echo
echo "--- F2. a caller's own selection (the desktop app's lease) ---"

mkmanifest 'Bash|t-block.sh' 'Bash|t-pass.sh' 'Bash|t-block2.sh'
OUT="$(bash "$SBD/dispatch-pretooluse.sh" Bash t-pass.sh < "$PAYLOAD" 2>"$SANDBOX/err")"; RC=$?
ERR="$(cat "$SANDBOX/err")"
if [ "$RC" = 0 ] && [ -z "$ERR" ]; then
    ok "D17a named rules run and the chain's other rules do not"
else
    bad "D17a named rules run and the chain's other rules do not (rc=$RC err=$ERR)"
fi
OUT="$(bash "$SBD/dispatch-pretooluse.sh" Bash t-block2.sh t-pass.sh < "$PAYLOAD" 2>"$SANDBOX/err")"; RC=$?
ERR="$(cat "$SANDBOX/err")"
if [ "$RC" = 2 ] && printf '%s' "$ERR" | grep -q 'BLOCK-BETA' && ! printf '%s' "$ERR" | grep -q 'BLOCK-ALPHA'; then
    ok "D17b a named rule's refusal stands; an unnamed one never runs"
else
    bad "D17b a named rule's refusal stands; an unnamed one never runs (rc=$RC err=$ERR)"
fi
OUT="$(bash "$SBD/dispatch-pretooluse.sh" Bash t-pass.sh t-not-listed.sh < "$PAYLOAD" 2>"$SANDBOX/err")"; RC=$?
ERR="$(cat "$SANDBOX/err")"
if [ "$RC" = 0 ] && printf '%s' "$ERR" | grep -q "rule 't-not-listed.sh' was named" \
   && printf '%s' "$OUT" | grep -q 't-not-listed.sh'; then
    ok "D17c a named rule the chain does not list is announced to the person and never blocks"
else
    bad "D17c a named rule the chain does not list is announced to the person and never blocks (rc=$RC out=$OUT err=$ERR)"
fi

echo
echo "--- G. the SHIPPED manifest and the SHIPPED registration agree ---"

SHIPPED_MODULES="$(grep -E '^(Bash|Write)\|' "$MANIFEST" | cut -d'|' -f2)"
MISSING=""
for m in $SHIPPED_MODULES; do
    [ -f "$SCRIPT_DIR/$m" ] || MISSING="$MISSING $m"
done
if [ -z "$MISSING" ]; then
    ok "D18 every rule the shipped manifest names is present on disk ($(printf '%s\n' $SHIPPED_MODULES | grep -c .) rules)"
else
    bad "D18 every rule the shipped manifest names is present on disk — ABSENT:$MISSING"
fi

# THE DOUBLE-EXECUTION REGRESSION, NAMED. install.sh's own header records the
# settings.json era, when two surfaces both registered the same hook and "every
# hook fired TWICE per matching tool event". A rule that is BOTH a dispatcher
# module AND its own registration rebuilds that bug inside one surface.
#
# THE TEST IS ABOUT TOOL EVENTS, NOT ABOUT NAMES — CORRECTED 2026-09-19.
# It used to flag any module whose basename also appeared anywhere in
# hooks.json, which was EXACT for as long as every separately-registered hook
# sat on `Bash`, on the Write matcher, or on the empty one. It is not exact in
# general: guard-no-home-network-phone.sh is a Write-chain module and is also
# registered on `Agent`, because the ruling it carries (§61) names "any brief,
# code write or spawn" — a brief and code are Writes, a dispatch is an Agent
# call. Those two matchers are DISJOINT, so no tool event reaches it twice and
# there is no double execution to find.
#
# So the comparison is between MATCHER SETS. A module is flagged when one of
# its own registrations covers a tool its chain already covers — including the
# EMPTY matcher, which governs every tool and therefore overlaps everything.
# That is the property the regression was about, and it is now checked rather
# than approximated. A same-matcher double still fails: the negative control
# below plants one.
overlap_report() {   # <hooks.json> <manifest> -> one line per offending module
    python3 - "$1" "$2" <<'PY'
import json, re, sys
hooks_json, manifest = sys.argv[1:3]

CHAIN_TOOLS = {
    "Bash":  {"Bash"},
    "Write": {"Write", "Edit", "MultiEdit", "NotebookEdit"},
}
UNIVERSAL = object()

modules = {}   # basename -> set of chain keys it is a module of
for line in open(manifest, encoding="utf-8"):
    line = line.strip()
    if not line or line.startswith("#") or "|" not in line:
        continue
    key, name = line.split("|", 1)
    key, name = key.strip(), name.strip()
    if key in CHAIN_TOOLS:
        modules.setdefault(name, set()).add(key)

d = json.load(open(hooks_json, encoding="utf-8"))
own = {}   # basename -> list of matcher tool-sets it is registered under
for entries in (d.get("hooks") or {}).values():
    for entry in entries:
        matcher = entry.get("matcher", "")
        tools = UNIVERSAL if not str(matcher).strip() else {
            t.strip() for t in str(matcher).split("|") if t.strip()}
        for h in entry.get("hooks", []) or []:
            for m in re.findall(r"scripts/hooks/([A-Za-z0-9._+-]+\.sh)",
                                h.get("command", "") or ""):
                own.setdefault(m, []).append(tools)

for name in sorted(modules):
    chain_tools = set()
    for key in modules[name]:
        chain_tools |= CHAIN_TOOLS[key]
    for tools in own.get(name, []):
        if tools is UNIVERSAL:
            print("%s (registered on the EMPTY matcher, which governs every tool, "
                  "while also a %s-chain module)"
                  % (name, "/".join(sorted(modules[name]))))
            break
        clash = tools & chain_tools
        if clash:
            print("%s (registered on %s, which its own chain already covers)"
                  % (name, "|".join(sorted(clash))))
            break
PY
}

DOUBLE="$(overlap_report "$HOOKS_JSON" "$MANIFEST" | tr '\n' ' ')"
if [ -z "${DOUBLE// /}" ]; then
    ok "D19 no rule is registered on a tool its own dispatcher chain already covers (no double execution)"
else
    bad "D19 no rule is registered on a tool its own dispatcher chain already covers — DOUBLED: $DOUBLE"
fi

# D19z NEGATIVE CONTROL. D19 can only be trusted if it still fails on the thing
# it was written for, and the correction above widened what it accepts — so the
# same-matcher double is planted here and must be caught.
D19Z_DIR="$(mktemp -d -t d19z.XXXXXX)"
python3 - "$HOOKS_JSON" "$D19Z_DIR/hooks.json" <<'PY'
import json, sys
src, dst = sys.argv[1:3]
d = json.load(open(src, encoding="utf-8"))
d.setdefault("hooks", {}).setdefault("PreToolUse", []).append({
    "matcher": "Write|Edit|MultiEdit|NotebookEdit",
    "hooks": [{"type": "command",
               "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/scan-secrets.sh",
               "timeout": 20}]})
json.dump(d, open(dst, "w", encoding="utf-8"))
PY
if overlap_report "$D19Z_DIR/hooks.json" "$MANIFEST" | grep -q 'scan-secrets.sh'; then
    ok "D19z NEGATIVE CONTROL: a module re-registered on its OWN chain's matcher is still caught"
else
    bad "D19z NEGATIVE CONTROL: a module re-registered on its OWN chain's matcher was NOT caught"
fi
rm -rf "$D19Z_DIR"

WIRED_KEYS="$(python3 - "$HOOKS_JSON" <<'PY'
import json, re, sys
d = json.load(open(sys.argv[1]))
for entry in d.get("hooks", {}).get("PreToolUse", []):
    for h in entry.get("hooks", []) or []:
        m = re.search(r"dispatch-pretooluse\.sh\s+(\S+)", h.get("command", "") or "")
        if m:
            print(f"{entry.get('matcher','')}\t{m.group(1)}")
PY
)"
BADKEY=""
while IFS="$(printf '\t')" read -r matcher key; do
    [ -n "${key:-}" ] || continue
    grep -qE "^${key}\|" "$MANIFEST" || BADKEY="$BADKEY ${matcher}=>${key}"
done <<WK
$WIRED_KEYS
WK
NWIRED="$(printf '%s\n' "$WIRED_KEYS" | grep -c . || true)"
if [ -z "$BADKEY" ] && [ "$NWIRED" -ge 1 ]; then
    ok "D20 every chain key hooks.json passes the dispatcher exists in the manifest ($NWIRED registrations)"
else
    bad "D20 every chain key hooks.json passes the dispatcher exists in the manifest (wired=$NWIRED unknown:$BADKEY)"
fi

# Each rule is still a standalone script. That is not decoration: it is what
# keeps every rule's own test suite meaningful, and what makes a rule debuggable
# without the dispatcher in the way.
# THE MANIFEST DECIDES WHICH GUARDS RUN, so it is DATA the install has to hash —
# the same argument scripts/lib/dialect-en-US.dict carries in install.sh, at a
# wider blast radius. A hashed dispatcher over an unhashed manifest is the lock
# checked and the key ignored: delete a line and the dispatcher stays wired,
# hashed and executable while running sixteen rules where it ran seventeen, and
# the sixteen that do run all pass.
# A clean checkout has no generated sidecars. Exercise the installer in a
# disposable copy, removing any copied sidecar so stale output cannot pass.
INSTALL_ENGINE="$SANDBOX/install-engine"
mkdir -p "$INSTALL_ENGINE" "$SANDBOX/install-home"
cp -R "$ENGINE_ROOT/." "$INSTALL_ENGINE/"
INSTALLED_MANIFEST="$INSTALL_ENGINE/scripts/hooks/dispatch-pretooluse.manifest"
rm -f "$INSTALLED_MANIFEST.sha256"
if HOME="$SANDBOX/install-home" CLAUDE_CONFIG_DIR="$SANDBOX/install-home/.claude" \
   RICHOS_ENTITY_ROOT="$INSTALL_ENGINE" bash "$INSTALL_ENGINE/scripts/hooks/install.sh" \
   >"$SANDBOX/install.log" 2>&1 \
   && [ -f "$INSTALLED_MANIFEST.sha256" ] \
   && [ "$(cut -d' ' -f1 < "$INSTALLED_MANIFEST.sha256")" = "$(shasum -a 256 "$INSTALLED_MANIFEST" | cut -d' ' -f1)" ]; then
    ok "D21b the manifest is sidecar-hashed by install.sh and matches"
else
    bad "D21b the installer did not produce a matching manifest sidecar"
    sed 's/^/       /' "$SANDBOX/install.log"
fi

STANDALONE_BAD=""
for m in $SHIPPED_MODULES; do
    bash "$SCRIPT_DIR/$m" < "$PAYLOAD" >/dev/null 2>&1
    rc=$?
    case "$rc" in 0|2) : ;; *) STANDALONE_BAD="$STANDALONE_BAD $m(rc=$rc)" ;; esac
done
if [ -z "$STANDALONE_BAD" ]; then
    ok "D21 every rule is still independently runnable as its own process (rc 0 or 2)"
else
    bad "D21 every rule is still independently runnable as its own process —$STANDALONE_BAD"
fi

echo
echo "--- H. the chassis memos: inert by default, exact when enabled ---"

# The memos are the ONLY reason one process is cheaper than N. They are also the
# only part of this change that touches a shared library every other hook uses,
# so the property that matters most is that they are INERT unless asked for.
MEMO_OUT="$(bash -c '
set -uo pipefail
. "'"$ENGINE_ROOT"'/scripts/lib/resolve-roots.sh"
A="'"$SANDBOX"'/ra"; B="'"$SANDBOX"'/rb"
mkdir -p "$A" "$B"
cp "'"$ENGINE_ROOT"'/orchestration.config" "$A/orchestration.config"
cp "'"$ENGINE_ROOT"'/orchestration.config" "$B/orchestration.config"
PA="{\"cwd\":\"$A\"}"; PB="{\"cwd\":\"$B\"}"
resolve_entity_root "$PA" >/dev/null 2>&1; echo "uncached-1=$RICHOS_ENTITY_ROOT_RESOLVED"
resolve_entity_root "$PB" >/dev/null 2>&1; echo "uncached-2=$RICHOS_ENTITY_ROOT_RESOLVED"
_RR_MEMO_ENABLE=1
resolve_entity_root "$PA" >/dev/null 2>&1; echo "memo-1=$RICHOS_ENTITY_ROOT_RESOLVED status=$RICHOS_ROOT_STATUS src=$RICHOS_ROOT_SOURCE"
resolve_entity_root "$PA" >/dev/null 2>&1; echo "memo-hit=$RICHOS_ENTITY_ROOT_RESOLVED status=$RICHOS_ROOT_STATUS src=$RICHOS_ROOT_SOURCE"
resolve_entity_root "$PB" >/dev/null 2>&1; echo "memo-2=$RICHOS_ENTITY_ROOT_RESOLVED"
' 2>/dev/null)"

U1="$(printf '%s\n' "$MEMO_OUT" | sed -n 's/^uncached-1=//p')"
U2="$(printf '%s\n' "$MEMO_OUT" | sed -n 's/^uncached-2=//p')"
if [ -n "$U1" ] && [ -n "$U2" ] && [ "$U1" != "$U2" ]; then
    ok "D22 with the memo OFF (the default) two different payloads get two different answers"
else
    bad "D22 with the memo OFF (the default) two different payloads get two different answers (1=$U1 2=$U2)"
fi

M1="$(printf '%s\n' "$MEMO_OUT" | sed -n 's/^memo-1=//p')"
MH="$(printf '%s\n' "$MEMO_OUT" | sed -n 's/^memo-hit=//p')"
if [ -n "$M1" ] && [ "$M1" = "$MH" ] && [ "${M1%% *}" = "$U1" ]; then
    ok "D23 with the memo ON a repeat gives the SAME root, status and source as the uncached answer"
else
    bad "D23 with the memo ON a repeat gives the SAME root, status and source as the uncached answer (first=$M1 hit=$MH uncached=$U1)"
fi

M2="$(printf '%s\n' "$MEMO_OUT" | sed -n 's/^memo-2=//p')"
if [ "$M2" = "$U2" ]; then
    ok "D24 the memo is KEYED: a different payload is recomputed, never served the previous answer"
else
    bad "D24 the memo is KEYED: a different payload is recomputed, never served the previous answer (got=$M2 want=$U2)"
fi

# THE HAZARD THE MEMO KEY CANNOT SEE, and therefore the case that decides
# whether being off by default is load-bearing. The key carries the payload, the
# working directory and the environment — everything EXCEPT the filesystem. Ask
# the same question twice with the adoption marker removed in between and the
# honest answer changes; a memo cannot know that, which is precisely why no
# caller gets one unless it has asked for it and knows its call is momentary.
FS_OUT="$(bash -c '
set -uo pipefail
unset CLAUDE_PROJECT_DIR RICHOS_ENTITY_ROOT
cd "'"$SANDBOX"'"
. "'"$ENGINE_ROOT"'/scripts/lib/resolve-roots.sh"
C="'"$SANDBOX"'/rc"; mkdir -p "$C"
cp "'"$ENGINE_ROOT"'/orchestration.config" "$C/orchestration.config"
P="{\"cwd\":\"$C\"}"
resolve_entity_root "$P" >/dev/null 2>&1; echo "before=$RICHOS_ROOT_STATUS"
rm -f "$C/orchestration.config"
resolve_entity_root "$P" >/dev/null 2>&1; echo "after=$RICHOS_ROOT_STATUS"
' 2>/dev/null)"
FS_B="$(printf '%s\n' "$FS_OUT" | sed -n 's/^before=//p')"
FS_A="$(printf '%s\n' "$FS_OUT" | sed -n 's/^after=//p')"
if [ "$FS_B" = "governed" ] && [ -n "$FS_A" ] && [ "$FS_A" != "$FS_B" ]; then
    ok "D26 OFF by default means the FILESYSTEM is still consulted every time, which no memo key can cover"
else
    bad "D26 OFF by default means the FILESYSTEM is still consulted every time (before=$FS_B after=$FS_A; want 'governed' then anything else)"
fi

# THE CAPTURE IS A REDIRECTION, NEVER `$(...)`. A command substitution runs in
# a subshell, so the memo this case exists to exercise would be written into a
# copy and discarded — every call would be a MISS and the case would pass over a
# memo it never reached. The mutation harness caught exactly that: its
# `ue-memo-loses-reason` mutant survived because no case had ever taken the hit
# branch.
UE_OUT="$(bash -c '
set -uo pipefail
. "'"$ENGINE_ROOT"'/scripts/lib/unevaluated-notice.sh"
T="'"$SANDBOX"'/ue"; mkdir -p "$T"
probe() { # <label> <payload>
    local rc
    richos_payload_unreadable "$2" > "$T/out"
    rc=$?
    echo "$1=[$(cat "$T/out")] rc=$rc"
}
probe good-off "{\"a\":1}"
_UE_MEMO_ENABLE=1
probe good-on  "{\"a\":1}"
probe good-hit "{\"a\":1}"
probe bad-on   "not json at all"
probe bad-hit  "not json at all"
' 2>/dev/null)"
G_OFF="$(printf '%s\n' "$UE_OUT" | sed -n 's/^good-off=//p')"
G_ON="$(printf '%s\n' "$UE_OUT"  | sed -n 's/^good-on=//p')"
G_HIT="$(printf '%s\n' "$UE_OUT" | sed -n 's/^good-hit=//p')"
B_ON="$(printf '%s\n' "$UE_OUT"  | sed -n 's/^bad-on=//p')"
B_HIT="$(printf '%s\n' "$UE_OUT" | sed -n 's/^bad-hit=//p')"
if [ "$G_OFF" = "$G_ON" ] && [ "$G_ON" = "$G_HIT" ] && [ "$B_ON" = "$B_HIT" ] \
   && [ "$G_ON" != "$B_ON" ] && [ -n "$B_ON" ]; then
    ok "D25 the payload-readability memo returns the same answer AND the same rc, cached or not"
else
    bad "D25 the payload-readability memo returns the same answer AND the same rc, cached or not (off=$G_OFF on=$G_ON hit=$G_HIT bad=$B_ON badhit=$B_HIT)"
fi

echo
echo "--- K. a rule's own words decide whether it is forked ---"

# A rule that names its words is forked only for a call that contains one. Its
# body records that it ran and refuses, so a skip and a run cannot look alike.
w t-words.sh '# dispatch-words: zzbanana zzcherry
INPUT="$(cat)"; : > "'"$SANDBOX"'/words-ran"; echo "WORDS-RULE refused this call" >&2; exit 2'
mkmanifest 'Bash|t-words.sh' 'Bash|t-pass.sh'
words_payload() { # <command> -> $PAYLOAD
    python3 - "$SBE" "$1" > "$PAYLOAD" <<'PY'
import json, sys
print(json.dumps({"session_id": "t", "cwd": sys.argv[1], "hook_event_name": "PreToolUse",
                  "tool_name": "Bash", "tool_input": {"command": sys.argv[2], "description": "d"}}))
PY
}
words_case() { # <id> <command> <want: ran|skipped> <label>
    rm -f "$SANDBOX/words-ran"
    words_payload "$2"
    run Bash
    if [ "$3" = ran ] && [ "$RC" = 2 ] && [ -f "$SANDBOX/words-ran" ] \
       && printf '%s' "$ERR" | grep -q 'WORDS-RULE refused'; then
        ok "$1  $4"
    elif [ "$3" = skipped ] && [ "$RC" = 0 ] && [ ! -f "$SANDBOX/words-ran" ] && [ -z "$ERR$OUT" ]; then
        ok "$1  $4"
    else
        bad "$1  $4 (want $3; rc=$RC err=$ERR out=$OUT)"
    fi
}
words_case K1 'echo hello'                skipped "a call naming none of the rule's words never forks it, and the chain is silent"
words_case K2 'git add . && zzcherry pie' ran     "a call naming one of the words runs the rule, and its refusal stands"
words_case K3 "zz'ban'ana split by quotes" ran    "a word spelled in quoted pieces still runs the rule (quotes are stripped first)"
words_case K4 $'zzban\\\nana'              ran    "a word broken by a line continuation still runs the rule"
# Nothing is skipped on a payload the dispatcher cannot read: every rule runs.
rm -f "$SANDBOX/words-ran"
printf 'not json at all' > "$PAYLOAD"
run Bash
if [ -f "$SANDBOX/words-ran" ] && [ "$RC" = 2 ]; then
    ok "K5  an unreadable payload skips NOTHING: the rule ran on it"
else
    bad "K5  an unreadable payload skips NOTHING (rc=$RC ran=$([ -f "$SANDBOX/words-ran" ] && echo yes || echo no))"
fi
# A rule with no words line runs on every call, exactly as before.
w t-nowords.sh 'INPUT="$(cat)"; : > "'"$SANDBOX"'/nowords-ran"; exit 0'
mkmanifest 'Bash|t-nowords.sh'
rm -f "$SANDBOX/nowords-ran"
words_payload 'echo hello'
run Bash
if [ -f "$SANDBOX/nowords-ran" ] && [ "$RC" = 0 ]; then
    ok "K6  a rule that names no words runs on every call"
else
    bad "K6  a rule that names no words runs on every call (rc=$RC)"
fi
# The shipped words: each must be a word that rule's own filter tests, so the
# word list cannot quietly drift from the filter it stands for.
K7_BAD=""
for m in $(grep '^Bash|' "$MANIFEST" | cut -d'|' -f2); do
    wl="$(sed -n 's/^# dispatch-words://p' "$SCRIPT_DIR/$m" | head -1)"
    for word in $wl; do
        n="$(grep -v '^#' "$SCRIPT_DIR/$m" | grep -c -- "$word" || true)"
        [ "$n" -gt 0 ] || K7_BAD="$K7_BAD $m:$word"
    done
done
if [ -z "$K7_BAD" ]; then
    ok "K7  every shipped dispatch word appears in its rule's own code"
else
    bad "K7  every shipped dispatch word appears in its rule's own code (missing:$K7_BAD)"
fi
words_payload 'echo hello'

echo
echo "=== $PASS passed, $FAIL failed ==="

MUT="$SCRIPT_DIR/dispatch-pretooluse.mutation.sh"
if [ "${RICHOS_MUTATION_INNER:-0}" = "1" ]; then
    echo "(mutation harness skipped: this run IS a mutant — one level is the proof)"
elif [ -f "$MUT" ]; then
    echo
    bash "$MUT" || FAIL=$((FAIL + 1))
else
    echo
    echo "  FAIL  dispatch-pretooluse.mutation.sh is missing — every case above is unproven"
    FAIL=$((FAIL + 1))
fi

[ "$FAIL" -eq 0 ] || exit 1
exit 0
