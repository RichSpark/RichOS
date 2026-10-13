#!/usr/bin/env bash
#
# audit-report-watch.test.sh - a fixture audit report gains a finding in a new
# commit; the lead must get a notice naming that finding, through the Stop hook
# the lead actually runs (notice-escalations.sh).
#
# Red on main: the hook raises nothing about audit reports, so the delivered
# context never names the finding.

set -uo pipefail
SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SRC_DIR/../.." && pwd)"
PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '         %s\n' "$2"; FAIL=$((FAIL + 1)); }

unset GIT_AUTHOR_EMAIL GIT_COMMITTER_EMAIL GIT_AUTHOR_NAME GIT_COMMITTER_NAME EMAIL
SANDBOX="$(cd "$(mktemp -d -t auditwatch.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

REPO="$SANDBOX/record"
mkdir -p "$REPO/docs/audits"
git -C "$REPO" init -q
REPORT="$REPO/docs/audits/part-1.md"
printf '# Part 1\n\n## 01. First finding\n\nBody one.\n\n## 02. Second finding\n\nBody two.\n' > "$REPORT"
git -C "$REPO" add -A
git -C "$REPO" commit -q -m "report v1"

export RICHOS_ESCALATION_LEDGER="$SANDBOX/ledger.jsonl"
export RICHOS_AUDIT_RECORD_REPO="$REPO"
export RICHOS_AUDIT_WATCH_STATE="$SANDBOX/state"
# A shipped engine has no Git metadata. Explicit CLI/environment selections still
# work there, and the absent implicit repository must not create watcher state.
ROOT_OUT="$(python3 -B - "$ENGINE_ROOT" "$SANDBOX" "$REPO" <<'PY'
from pathlib import Path
import json, os, shutil, subprocess, sys
engine, sandbox, record = map(Path, sys.argv[1:])
source = sandbox / 'extracted engine/scripts/lib/audit-report-watch.py'
source.parent.mkdir(parents=True)
shutil.copy2(engine / 'scripts/lib/audit-report-watch.py', source)
env = dict(os.environ)
env.pop('RICHOS_AUDIT_RECORD_REPO', None)
def run(script, state, args=(), environment=env):
    # Finite hang guard only; correctness is the exit, state and recorded finding.
    return subprocess.run([sys.executable, '-B', str(script), '--state-dir', str(state), *args],
                          env=environment, capture_output=True, text=True, timeout=30)
state = sandbox / 'extracted-no-record'
r = run(source, state)
assert r.returncode == 0 and not r.stdout and not r.stderr and not state.exists(), r
for mode, args, selected in (
        ('cli', ('--repo', str(record)), env),
        ('environment', (), {**env, 'RICHOS_AUDIT_RECORD_REPO': str(record)})):
    state = sandbox / ('extracted-' + mode)
    r = run(source, state, args, selected)
    assert r.returncode == 0, r.stderr
    assert set(json.loads((state / 'seen.json').read_text())['docs/audits/part-1.md']['findings']) == {'01', '02'}
checkout = sandbox / 'checkouts/richos'
checkout.mkdir(parents=True)
subprocess.run(['git', '-C', str(checkout), 'init', '-q'], check=True, env=env)
(checkout.parent / 'richos-hq').symlink_to(record, target_is_directory=True)
tracked = checkout / 'richos/engine/scripts/lib/audit-report-watch.py'
tracked.parent.mkdir(parents=True)
shutil.copy2(source, tracked)
state = sandbox / 'checkout-default'
r = run(tracked, state)
assert r.returncode == 0 and (state / 'seen.json').is_file(), r.stderr
shutil.rmtree(checkout / '.git')
(checkout / '.git').write_text('gitdir: missing-git-directory\n')
state = sandbox / 'broken-checkout'
r = run(tracked, state)
assert r.returncode == 2 and 'cannot resolve record repository' in r.stderr and not state.exists(), r
PY
)"; ROOT_RC=$?
if [ "$ROOT_RC" = 0 ]; then
    ok "extracted engines honor explicit repositories or stay quiet; checkout discovery still works and refuses broken Git"
else
    bad "record repository discovery works outside a checkout" "$ROOT_OUT"
fi
# THE HOOK GOVERNS A SANDBOX ENTITY, NEVER THE CHECKOUT IT RUNS FROM. Run with
# its cwd in the engine and no entity of its own, notice-escalations.sh wrote
# its per-session notice state (and escalations_deliver its delivered record)
# into the checkout's own .claude/state/stop-hook-notices/. The proof gate
# binds the whole engine directory as the input of every engine check, so
# those writes made every check running beside this one "invalid: execution
# inputs changed during the check" (merge of cc/zach-sonnet-escclaim1,
# 2026-10-01).
unset CLAUDE_PROJECT_DIR RICHOS_ENGINE_ROOT
export RICHOS_ENTITY_ROOT="$SANDBOX/entity"
mkdir -p "$RICHOS_ENTITY_ROOT"
printf 'PROTECTED_PATHS=""\n' >"$RICHOS_ENTITY_ROOT/orchestration.config"
PAYLOAD='{"session_id":"auditwatch-test","transcript_path":""}'
cd "$RICHOS_ENTITY_ROOT" || exit 1

stop() { printf '%s' "$PAYLOAD" | bash "$SRC_DIR/notice-escalations.sh" 2>&1; }

OUT1="$(stop)"
case "$OUT1" in
    *"40"*|*"udit"*) bad "first sighting is a silent baseline" "$OUT1" ;;
    *) ok "first sighting is a silent baseline" ;;
esac

printf '\n## 40. A late finding\n\nBody forty.\n' >> "$REPORT"
sed -i.bak 's/Body two\./Body two, revised./' "$REPORT"
rm -f "$REPORT.bak"
git -C "$REPO" add -A
git -C "$REPO" commit -q -m "report v2"
SHA="$(git -C "$REPO" rev-parse --short HEAD)"

OUT2="$(stop)"
if printf '%s' "$OUT2" | grep -q 'part-1.md' && printf '%s' "$OUT2" | grep -q 'new: 40' \
   && printf '%s' "$OUT2" | grep -q 'changed: 02' && printf '%s' "$OUT2" | grep -q "$SHA"; then
    ok "the lead's turn end names the report, the commit, new finding 40 and changed finding 02"
else
    bad "the lead's turn end names the report, commit and IDs" "$OUT2"
fi

OUT3="$(stop)"
if printf '%s' "$OUT3" | grep -q 'new: 40'; then
    bad "the same change is delivered once, not at every turn end" "$OUT3"
else
    ok "the same change is delivered once, not at every turn end"
fi
ROWS="$(grep -c 'audit-report-watch' "$RICHOS_ESCALATION_LEDGER" 2>/dev/null || true)"
if [ "$ROWS" = "1" ]; then ok "exactly one ledger row was raised"; else bad "exactly one ledger row was raised" "rows=$ROWS"; fi

sed -i.bak 's/^# Part 1$/# Part 1 (edited preamble)/' "$REPORT"
rm -f "$REPORT.bak"
git -C "$REPO" add -A
git -C "$REPO" commit -q -m "prose only"
stop >/dev/null
ROWS="$(grep -c 'audit-report-watch' "$RICHOS_ESCALATION_LEDGER" 2>/dev/null || true)"
if [ "$ROWS" = "1" ]; then ok "an edit that touches no finding raises nothing"; else bad "an edit that touches no finding raises nothing" "rows=$ROWS"; fi

# P5-75: a column-zero "# comment" inside a fenced code quotation is quotation, not a
# report heading. It used to close the finding, so edits below it (the consequence,
# the evidence) left the digest unchanged and raised nothing.
FENCE_OUT="$(python3 - "$ENGINE_ROOT/scripts/lib/audit-report-watch.py" <<'PYEOF'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("arw", sys.argv[1])
arw = importlib.util.module_from_spec(spec); spec.loader.exec_module(arw)
TICKS = chr(96) * 3  # no literal backticks: macOS bash 3.2 misparses them in $( <<heredoc )
def report(consequence, fence=TICKS):
    return "\n".join([
        "# Part", "", "### P5-75: Quoted comment", "", "Intro.", "",
        fence + "bash", "# a shell comment at column zero", "echo hi", fence, "",
        "**Consequence.** " + consequence, "", "### P5-76: Next", "", "Other."])
a = arw.parse_findings(report("A"))
b = arw.parse_findings(report("B"))
t = arw.parse_findings(report("A", "~~~"))
bt = arw.parse_findings(report("B", "~~~"))
if a["P5-75"][1] == b["P5-75"][1]:
    print("a change below a fenced # comment left the digest unchanged")
if t["P5-75"][1] == bt["P5-75"][1]:
    print("same for a ~~~ fence")
if a["P5-76"][1] != b["P5-76"][1]:
    print("the next finding's digest moved with an unrelated change")
if sorted(a) != ["P5-75", "P5-76"]:
    print("finding ids wrong: %r" % sorted(a))
PYEOF
)"
if [ -z "$FENCE_OUT" ]; then
    ok "a # comment inside a fenced code quotation does not close the finding; the next real heading still does"
else
    bad "fenced comment closes the finding" "$FENCE_OUT"
fi

printf '\n%s passed, %s failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
