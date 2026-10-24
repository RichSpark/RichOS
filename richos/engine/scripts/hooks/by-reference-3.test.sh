#!/usr/bin/env bash
#
# by-reference-3.test.sh — the negative controls for contract-integrity-probe.sh's
# BY-REFERENCE layer set, part 3 of 4: BR3, BR4 and BR5.
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
# Run directly:  scripts/hooks/by-reference-3.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/by-reference-fixture.sh
. "$SCRIPT_DIR/../lib/by-reference-fixture.sh"

echo "=== by-reference-3.test.sh ==="
echo ""

# ---------------------------------------------------------------------------
# BR3 — confinement to ${CLAUDE_PLUGIN_ROOT}
# ---------------------------------------------------------------------------
SB="$(make_sandbox)"
python3 - "$SB/engine/hooks/hooks.json" "$SB" <<'PY'
import json, sys
p, sb = sys.argv[1], sys.argv[2]
d = json.load(open(p))
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "Bash":
        entry["hooks"][0]["command"] = f"bash {sb}/engine/scripts/hooks/guard-bash-main-writes.sh"
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "3a.BR3-absolute-path-instead-of-plugin-root" "BR3" "no \${CLAUDE_PLUGIN_ROOT}"
rm -rf "$SB"

# The ENTITY root standing in for an ENGINE asset — the exact conflation the
# whole two-root contract exists to prevent, expressed in the wiring.
SB="$(make_sandbox)"
python3 - "$SB/engine/hooks/hooks.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "SendMessage":
        entry["hooks"][0]["command"] = "bash $CLAUDE_PROJECT_DIR/scripts/hooks/guard-resume-isolation.sh"
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "3b.BR3-CLAUDE_PROJECT_DIR-for-an-engine-asset" "BR3" "uses \$CLAUDE_PROJECT_DIR"
rm -rf "$SB"

SB="$(make_sandbox)"
python3 - "$SB/engine/hooks/hooks.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "Bash":
        entry["hooks"][0]["command"] = "bash ${CLAUDE_PLUGIN_ROOT}/../scripts/hooks/guard-bash-main-writes.sh"
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "3c.BR3-escapes-the-plugin-root" "BR3" "escapes the plugin root"
rm -rf "$SB"

# ---------------------------------------------------------------------------
# BR4 — the scripts behind the registrations
# ---------------------------------------------------------------------------
SB="$(make_sandbox)"
rm -f "$SB/engine/scripts/hooks/scan-secrets.sh"
run_probe "$SB"
expect_only_layer_failed "4a.BR4-registered-script-not-on-disk" "BR4" "NOT ON DISK"
rm -rf "$SB"

SB="$(make_sandbox)"
chmod -x "$SB/engine/scripts/hooks/guard-model-ceiling.sh"
run_probe "$SB"
expect_only_layer_failed "4b.BR4-registered-script-not-executable" "BR4" "not executable"
rm -rf "$SB"

# Tampered AFTER the sidecar was minted — a guard gutted in place is invisible
# to every check except a hash.
SB="$(make_sandbox)"
printf '\n# tampered\n' >>"$SB/engine/scripts/hooks/guard-definition-drift.sh"
run_probe "$SB"
expect_only_layer_failed "4c.BR4-script-modified-since-install" "BR4" "MODIFIED since install"
rm -rf "$SB"

# A missing sidecar must NEVER be a green tick. It is not a failure either —
# a by-reference engine root is read-only to the repository it governs, so an
# adopter cannot mint one. What it must be is NAMED.
SB="$(make_sandbox)"
rm -f "$SB/engine/scripts/hooks/scan-secrets.sh.sha256"
run_probe "$SB"
if layer_warned "BR4" && out_has "TAMPER CHECK DID NOT RUN" && out_has "scan-secrets.sh"; then
    ok "4d.BR4-absent-sidecar-is-named-not-waved-through"
else
    bad "4d.BR4-absent-sidecar-is-named-not-waved-through" "no warning naming the unverified script"
fi
if printf '%s\n' "$OUT" | grep -E >/dev/null "✓.*BR4\. all [0-9]+ registered guard scripts present, executable and hash-matched"; then
    bad "4e.BR4-absent-sidecar-does-not-claim-hash-matched" "claimed hash-matched with a sidecar missing"
else
    ok "4e.BR4-absent-sidecar-does-not-claim-hash-matched"
fi
rm -rf "$SB"

# ---------------------------------------------------------------------------
# BR5 — the declared meta-roles
# ---------------------------------------------------------------------------
SB="$(make_sandbox)"
python3 - "$SB/engine/.claude-plugin/plugin.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["agents"] = ["./.claude/agents/does-not-exist.md"]
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "5a.BR5-declared-role-file-missing" "BR5" "NOT FOUND"
rm -rf "$SB"

SB="$(make_sandbox)"
python3 - "$SB/engine/.claude-plugin/plugin.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["agents"] = ["./../../etc/hosts.md"]
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "5b.BR5-declared-role-escapes-the-plugin-root" "BR5" "ESCAPES the plugin root"
rm -rf "$SB"

# The phantom-role trap, measured on this host 2026-08-28: the agent loader
# RECURSES into subdirectories of a declared agent directory and registers what
# it finds as <plugin>:<subdir>:<name>. engine/.claude/agents/ carries a
# templates/ subdirectory of non-live skeletons, so declaring that directory
# instead of the four files would ship seventeen roles nobody meant to ship.
SB="$(make_sandbox)"
python3 - "$SB/engine/.claude-plugin/plugin.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["agents"] = ["./.claude/agents"]
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "5c.BR5-declared-directory-with-subdirs-ships-phantom-roles" "BR5" "phantom"
rm -rf "$SB"

SB="$(make_sandbox)"
printf 'no frontmatter at all\n' >"$SB/engine/.claude/agents/clark.md"
run_probe "$SB"
expect_only_layer_failed "5d.BR5-declared-role-without-frontmatter" "BR5" "no YAML frontmatter"
rm -rf "$SB"


by_reference_finish "by-reference-3.test.sh"
