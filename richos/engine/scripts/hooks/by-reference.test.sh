#!/usr/bin/env bash
#
# by-reference.test.sh — the negative controls for contract-integrity-probe.sh's
# BY-REFERENCE layer set, part 1 of 4: the baseline (case 0), BR1, and BR2's first four cases.
# The four suites were one until 2026-10-05; why they are four, the sandbox and the
# assertions are in scripts/lib/by-reference-fixture.sh, which each sources.
#
# Every engine file the undivided suite named, named here too, so a change to any of them
# selects this suite exactly as it selected the whole (the selector's basename rule):
#   .gitignore clark.md contract-integrity-probe.sh detect-nonnative-worktree.sh dispatch-pretooluse.manifest engine-status.sh
#   global-state-witness.sh guard-bash-main-writes.sh guard-brief-scope.sh guard-definition-drift.sh guard-model-ceiling.sh guard-interactive-prompt.sh
#   guard-main-checkout-writes.sh guard-named-persons-writes.sh guard-resume-isolation.sh guard-vendoring-commits.sh guard-workflow-ban.sh guard-worktree-isolation.sh
#   guard-worktree-removal.sh hooks.json install.sh notice-unlanded-branches.sh orchestration.config plugin.json
#   registered-hooks.sh scan-secrets.sh settings.local.json shell-evidence.sh snapshot-agent-definitions.sh task-completed-handoff.sh
#   teammate-idle-handoff.sh verify-agent-prompt.sh
#
# Run directly:  scripts/hooks/by-reference.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/by-reference-fixture.sh
. "$SCRIPT_DIR/../lib/by-reference-fixture.sh"

echo "=== by-reference.test.sh ==="
echo ""

# ---------------------------------------------------------------------------
# 0 — the baseline. Everything below is only meaningful if a correct
#     installation is genuinely green; a suite whose baseline is already red
#     proves its mutations changed nothing.
# ---------------------------------------------------------------------------
SB="$(make_sandbox)"
run_probe "$SB"
if [ "$RC" -eq 0 ]; then
    ok "0a.baseline-correct-install-is-green"
else
    bad "0a.baseline-correct-install-is-green" "exit $RC; output: $(printf '%s' "$OUT" | grep '✗' | head -3)"
fi
if layer_passed "BR1" && layer_passed "BR2" && layer_passed "BR3" && layer_passed "BR4" \
   && layer_passed "BR5" && layer_passed "BR6" && layer_passed "BR6b" \
   && layer_passed "BR10" && layer_passed "BR8" && layer_passed "BR9"; then
    ok "0b.baseline-every-BR-layer-actually-ran"
else
    bad "0b.baseline-every-BR-layer-actually-ran" "a layer neither passed nor was reached: $(printf '%s' "$OUT" | grep -c '✓') ✓ lines"
fi
# The mode branch itself: a by-reference install must NOT be audited with the
# seated layers, and vice versa. Both directions, because picking the wrong set
# is precisely the defect that made this probe refuse a verdict for a month.
if out_has "ENGINE LOADED BY REFERENCE" && ! layer_passed "A" && ! layer_failed "A"; then
    ok "0c.by-reference-install-does-not-run-the-seated-layers"
else
    bad "0c.by-reference-install-does-not-run-the-seated-layers" "seated Layer A ran under a by-reference engine"
fi
set +e
SEATED_OUT="$(HOME="$SB/home" CLAUDE_CONFIG_DIR="$SB/home/.claude" RICHOS_ENTITY_ROOT="$SB/engine" \
    "$SB/engine/scripts/hooks/contract-integrity-probe.sh" 2>&1)"
SEATED_RC=$?
set -e
if printf '%s\n' "$SEATED_OUT" | grep >/dev/null "✓.*A\." && ! printf '%s\n' "$SEATED_OUT" | grep >/dev/null "BR1\."; then
    ok "0d.seated-install-runs-the-seated-layers-not-the-BR-set"
else
    bad "0d.seated-install-runs-the-seated-layers-not-the-BR-set" "exit $SEATED_RC"
fi
# The regression this suite was written alongside: R4 asserts engine-status.sh
# is registered in the ENTITY's settings.local.json, which is only true when
# SEATED. By reference the plugin registers it and the entity's settings file
# correctly never mentions it — asserting the seated location there produced a
# hard failure whose only cause was the probe looking in the wrong file.
mkdir -p "$SB/entity/.claude"
printf '{\n  "hooks": {}\n}\n' >"$SB/entity/.claude/settings.local.json"
run_probe "$SB"
if ! out_has "engine-status.sh is NOT registered in"; then
    ok "0e.R4-does-not-demand-the-seated-registration-from-a-by-reference-entity"
else
    bad "0e.R4-does-not-demand-the-seated-registration-from-a-by-reference-entity" "R4 fired on the entity's settings file"
fi
rm -rf "$SB"

# ---------------------------------------------------------------------------
# BR1 — the plugin manifest
# ---------------------------------------------------------------------------
SB="$(make_sandbox)"
rm -f "$SB/engine/.claude-plugin/plugin.json"
run_probe "$SB"
expect_only_layer_failed "1a.BR1-missing-plugin-manifest" "BR1" "plugin manifest MISSING"
rm -rf "$SB"

SB="$(make_sandbox)"
printf '{ "displayName": "no name here" }\n' >"$SB/engine/.claude-plugin/plugin.json"
run_probe "$SB"
expect_only_layer_failed "1b.BR1-manifest-without-a-name" "BR1" 'has no "name"'
rm -rf "$SB"

# ---------------------------------------------------------------------------
# BR2 — the plugin hook table
# ---------------------------------------------------------------------------
SB="$(make_sandbox)"
rm -f "$SB/engine/hooks/hooks.json"
run_probe "$SB"
expect_only_layer_failed "2a.BR2-missing-hook-table" "BR2" "plugin hook table MISSING"
rm -rf "$SB"

# A guard silently dropped from the table. Every byte of it is still on disk and
# still correct — and it never runs. This is the shape of the regression.
SB="$(make_sandbox)"
python3 - "$SB/engine/hooks/hooks.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "SendMessage":
        entry["hooks"] = []
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "2b.BR2-guard-not-registered" "BR2" "guard-resume-isolation.sh(NOT registered)"
rm -rf "$SB"

# The additive-merge double-fire, arriving through the plugin door.
#
# THE GUARD IS DUPLICATED BY NAME, NOT BY POSITION. This case used to take
# entry["hooks"][0] and assert BR2 named guard-bash-main-writes.sh — which
# quietly meant "whatever happens to be first in the Bash chain". On 2026-09-01
# guard-interactive-prompt.sh was wired ahead of it and the case went red while
# testing something it had never meant to test. An index is a second, invisible
# inventory of the chain order; a name is the property the case is actually
# about.
SB="$(make_sandbox)"
python3 - "$SB/engine/hooks/hooks.json" <<'PY'
import copy, json, sys
p = sys.argv[1]
d = json.load(open(p))
TARGET = "shell-evidence.sh"
found = False
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") != "Bash":
        continue
    for h in list(entry["hooks"]):
        if TARGET in h.get("command", ""):
            entry["hooks"].append(copy.deepcopy(h))
            found = True
            break
if not found:
    sys.stderr.write("2c: %s is not wired on the Bash matcher\n" % TARGET)
    sys.exit(3)
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "2c.BR2-guard-registered-twice" "BR2" "shell-evidence.sh(registered 2x"
rm -rf "$SB"

# THE SAME DOUBLE-FIRE, ARRIVING THROUGH THE DISPATCHER'S MANIFEST.
#
# Since 2026-09-15 seventeen guards are wired by one line each in
# scripts/hooks/dispatch-pretooluse.manifest rather than by an entry in
# hooks.json. That file is therefore a REGISTRATION SURFACE, and every property
# BR2 holds hooks.json to has to hold there too — otherwise the manifest is the
# unwatched second surface this whole library exists to prevent. Duplicating a
# rule line runs that rule twice per tool call, which is the additive-merge
# double-fire wearing a different hat.
SB="$(make_sandbox)"
python3 - "$SB/engine/scripts/hooks/dispatch-pretooluse.manifest" <<'PY'
import sys
p = sys.argv[1]
lines = open(p, encoding="utf-8").read().splitlines()
TARGET = "Bash|guard-vendoring-commits.sh"
if TARGET not in lines:
    sys.stderr.write("2c2: %s is not in the dispatcher manifest\n" % TARGET)
    sys.exit(3)
out = []
for line in lines:
    out.append(line)
    if line == TARGET:
        out.append(line)
open(p, "w", encoding="utf-8").write("\n".join(out) + "\n")
PY
run_probe "$SB"
expect_only_layer_failed "2c2.BR2-manifest-rule-listed-twice" "BR2" "guard-vendoring-commits.sh(registered 2x"
rm -rf "$SB"

# ...and the other direction: a rule DELETED from the manifest is a guard that
# stops running, and it must arrive here as NOT registered, exactly as a guard
# deleted from hooks.json used to. This is the case that makes the manifest
# safe to be a registration surface at all.
SB="$(make_sandbox)"
python3 - "$SB/engine/scripts/hooks/dispatch-pretooluse.manifest" <<'PY'
import sys
p = sys.argv[1]
lines = open(p, encoding="utf-8").read().splitlines()
TARGET = "Write|guard-named-persons-writes.sh"
if TARGET not in lines:
    sys.stderr.write("2c3: %s is not in the dispatcher manifest\n" % TARGET)
    sys.exit(3)
open(p, "w", encoding="utf-8").write(
    "\n".join(l for l in lines if l != TARGET) + "\n")
PY
run_probe "$SB"
expect_only_layer_failed "2c3.BR2-manifest-rule-deleted" "BR2" "guard-named-persons-writes.sh(NOT registered)"
rm -rf "$SB"

# Chain order. The isolation guard must refuse a bad spawn before the later
# hooks start reasoning about a spawn that should never have been considered.
#
# The rule BR2 applies is no longer a typed sequence of every registered name —
# that list went stale the day a ninth hook was registered and reported the new
# hook as the defect. It is a PREFIX rule (AGENT_CHAIN_STRUCTURAL_PREFIX in
# scripts/lib/registered-hooks.sh): the four structural gates come first, in
# order, and the policy tail is deliberately unconstrained. A reversed chain
# violates it at position 1, which is what this case asserts by name.
SB="$(make_sandbox)"
python3 - "$SB/engine/hooks/hooks.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "Agent":
        entry["hooks"].reverse()
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "2d.BR2-agent-chain-out-of-order" "BR2" "chain ORDER: position 1 of the PreToolUse[Agent] chain must be guard-worktree-isolation.sh"
rm -rf "$SB"


by_reference_finish "by-reference.test.sh"
