#!/usr/bin/env bash
#
# by-reference-4.test.sh — the negative controls for contract-integrity-probe.sh's
# BY-REFERENCE layer set, part 4 of 4: BR6, BR6b and BR10.
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
# Run directly:  scripts/hooks/by-reference-4.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/by-reference-fixture.sh
. "$SCRIPT_DIR/../lib/by-reference-fixture.sh"

echo "=== by-reference-4.test.sh ==="
echo ""

# ---------------------------------------------------------------------------
# BR6 — will this operator actually load this engine?
# ---------------------------------------------------------------------------
# THE CASE THAT MATTERS MOST. Everything above can be perfect and the host can
# still load nothing, because the registration lives in ~/.claude and not in any
# repository. This is the shape of the reported regression: guards correct,
# guards present, guards never loaded — with no error anywhere.
SB="$(make_sandbox)"
printf '{\n  "enabledPlugins": {}\n}\n' >"$SB/home/.claude/settings.json"
printf '{}\n' >"$SB/home/.claude/plugins/known_marketplaces.json"
run_probe "$SB"
expect_only_layer_failed "6a.BR6-plugin-not-enabled-anywhere" "BR6" "UNGUARDED"
rm -rf "$SB"

SB="$(make_sandbox)"
python3 - "$SB/home/.claude/settings.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["enabledPlugins"] = {"richos-engine@sandbox-local": False}
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "6b.BR6-plugin-registered-but-disabled" "BR6" "UNGUARDED"
rm -rf "$SB"

# Enabled, and pointing at a different copy of the engine. "A plugin is enabled"
# is not the question; "is THIS engine the one that loads" is.
SB="$(make_sandbox)"
mkdir -p "$SB/other-engine"
python3 - "$SB/.claude-plugin/marketplace.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["plugins"][0]["source"] = "./other-engine"
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "6c.BR6-enabled-plugin-resolves-elsewhere" "BR6" "does NOT resolve to this engine"
rm -rf "$SB"

# ---------------------------------------------------------------------------
# BR6b — the entity-facing engine POINTER, which an entity's OWN scripts follow
# ---------------------------------------------------------------------------
# BR6 answers "will the HOST load this engine?". BR6b answers the other half:
# "will an ENTITY SCRIPT find it?" — the question that arises the moment an
# adopter's install-fresh pipeline or CI step calls an engine asset, since
# neither gets $CLAUDE_PLUGIN_ROOT and neither has a relative path to the engine
# any more. A pointer nobody audits is how such a script ends up running a moved
# engine's checks, or none at all, while every other layer stays green.
SB="$(make_sandbox)"
ln -sfn "$SB/entity" "$SB/home/.claude/richos-engine"     # exists, but is not an engine
run_probe "$SB"
expect_only_layer_failed "6d.BR6b-pointer-resolves-to-a-non-engine" "BR6b" "NOT an engine"
rm -rf "$SB"

SB="$(make_sandbox)"
mkdir -p "$SB/decoy-engine/scripts/hooks"
printf '0.0.0\n' >"$SB/decoy-engine/VERSION"
ln -sfn "$SB/decoy-engine" "$SB/home/.claude/richos-engine"
run_probe "$SB"
expect_only_layer_failed "6e.BR6b-pointer-disagrees-with-the-audited-engine" "BR6b" "DISAGREES"
rm -rf "$SB"

SB="$(make_sandbox)"
ln -sfn "$SB/engine-that-was-deleted" "$SB/home/.claude/richos-engine"
run_probe "$SB"
expect_only_layer_failed "6f.BR6b-dangling-pointer-is-not-reported-as-absent" "BR6b" "DANGLING"
rm -rf "$SB"

# ...and the positive arm: an ABSENT pointer is a NAMED WARNING, never a
# failure and never a green tick. An adopter cannot mint it (the engine root is
# read-only to the repository it governs), so failing them for the engine
# maintainer's step would be wrong; saying nothing would be the "green tick with
# the truth in a parenthesis" this probe has already shipped once.
SB="$(make_sandbox)"
rm -f "$SB/home/.claude/richos-engine"
run_probe "$SB"
if [ "$RC" -eq 0 ] && layer_warned "BR6b" && out_has "pointer is ABSENT"; then
    ok "6g.BR6b-absent-pointer-is-a-named-warning-not-a-failure"
else
    bad "6g.BR6b-absent-pointer-is-a-named-warning-not-a-failure" "rc=$RC; expected exit 0 with a warned BR6b naming it ABSENT"
fi
if layer_passed "BR6b"; then
    bad "6h.BR6b-absent-pointer-does-not-claim-agreement" "an absent pointer emitted a passing BR6b — a green tick for something that was never checked"
else
    ok "6h.BR6b-absent-pointer-does-not-claim-agreement"
fi
rm -rf "$SB"

# ---------------------------------------------------------------------------
# BR10 — the ENTITY's own critical config, which the plugin cannot supply
# ---------------------------------------------------------------------------
# Every other BR layer audits the ENGINE. These two keys are the entity's own,
# they work at project scope, and their absence is silent: no error, no banner,
# just an orchestrator that suddenly sees zero teammates. The seated layer set
# has checked them since that incident; the by-reference set did not, so an
# entity gained plugin verification and quietly lost config verification at the
# exact moment it adopted.
write_entity_settings() { # <sandbox> <teams-value-or-DELETE> <baseref-value-or-DELETE>
    python3 - "$1/entity/.claude/settings.local.json" "$2" "$3" <<'PY'
import json, os, sys
p, teams, ref = sys.argv[1:4]
d = {}
if os.path.exists(p):
    with open(p) as h:
        d = json.load(h)
d.setdefault("env", {})
d.setdefault("worktree", {})
if teams == "DELETE":
    d["env"].pop("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", None)
else:
    d["env"]["CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"] = teams
if ref == "DELETE":
    d["worktree"].pop("baseRef", None)
else:
    d["worktree"]["baseRef"] = ref
os.makedirs(os.path.dirname(p), exist_ok=True)
with open(p, "w") as h:
    json.dump(d, h, indent=2)
PY
}

SB="$(make_sandbox)"
write_entity_settings "$SB" DELETE head
run_probe "$SB"
expect_only_layer_failed "10a.BR10-AGENT_TEAMS-flag-missing" "BR10" "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS is None"
rm -rf "$SB"

SB="$(make_sandbox)"
write_entity_settings "$SB" 1 DELETE
run_probe "$SB"
expect_only_layer_failed "10b.BR10-worktree-baseRef-missing" "BR10" "worktree.baseRef is None"
rm -rf "$SB"

# Present but WRONG is not the same as missing, and both must fail: "0" is a
# perfectly valid-looking value that turns the flag off.
SB="$(make_sandbox)"
write_entity_settings "$SB" 0 head
run_probe "$SB"
expect_only_layer_failed "10c.BR10-AGENT_TEAMS-flag-present-but-off" "BR10" "expected \"1\""
rm -rf "$SB"

SB="$(make_sandbox)"
write_entity_settings "$SB" 1 main
run_probe "$SB"
expect_only_layer_failed "10d.BR10-baseRef-pointing-at-the-wrong-ref" "BR10" "expected \"head\""
rm -rf "$SB"

SB="$(make_sandbox)"
rm -rf "$SB/entity/.claude/settings.local.json"
run_probe "$SB"
expect_only_layer_failed "10e.BR10-entity-has-no-settings-file-at-all" "BR10" "no readable .claude/settings.local.json"
rm -rf "$SB"

# Positive arm, so 10a-10e mean something: a correct entity config PASSES and is
# not merely un-checked, and 0b asserts BR10 actually ran in the baseline.
SB="$(make_sandbox)"
write_entity_settings "$SB" 1 head
run_probe "$SB"
if [ "$RC" -eq 0 ] && layer_passed "BR10"; then
    ok "10f.BR10-a-correct-entity-config-passes"
else
    bad "10f.BR10-a-correct-entity-config-passes" "rc=$RC; $(printf '%s' "$OUT" | grep '✗' | head -2)"
fi
rm -rf "$SB"


by_reference_finish "by-reference-4.test.sh"
