#!/usr/bin/env bash
#
# by-reference-2.test.sh — the negative controls for contract-integrity-probe.sh's
# BY-REFERENCE layer set, part 2 of 4: the rest of BR2, then BR7, BR8 and BR9.
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
# Run directly:  scripts/hooks/by-reference-2.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/by-reference-fixture.sh
. "$SCRIPT_DIR/../lib/by-reference-fixture.sh"

echo "=== by-reference-2.test.sh ==="
echo ""

# Right guard, wrong event: registered, present, hash-matched — and wired to an
# event it will never see. Nothing else in the probe would notice.
SB="$(make_sandbox)"
python3 - "$SB/engine/hooks/hooks.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
# Move a NAMED guard, never "whichever happens to be last". PostToolUse[Agent]
# is a chain and it has already grown once (the worker-lifecycle creation
# emitter was appended to it); a positional pop silently retargets this case at
# whatever guard was added most recently, and the assertion below then fails
# for a reason that has nothing to do with the defect under test.
moved = None
for entry in d["hooks"]["PostToolUse"]:
    if entry.get("matcher") != "Agent":
        continue
    for h in list(entry["hooks"]):
        if "detect-nonnative-worktree.sh" in h.get("command", ""):
            entry["hooks"].remove(h)
            moved = h
if moved is None:
    raise SystemExit("mutation target detect-nonnative-worktree.sh not found in PostToolUse[Agent]")
d["hooks"].setdefault("Stop", []).append({"hooks": [moved]})
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
# The wording says "not exactly once on <event>" rather than "not on <event>"
# because BR2's uniqueness invariant is now PER EVENT: a hook wired twice on
# one event double-fires, while a hook wired once on each of two different
# events (terminalize-agent-worktrees.sh, on SubagentStop and WorktreeRemove)
# does not, and the old per-script count called the second one a defect.
expect_only_layer_failed "2e.BR2-guard-on-the-wrong-event" "BR2" "detect-nonnative-worktree.sh(registered, but not exactly once on PostToolUse)"
rm -rf "$SB"

# THE DRIFT THAT ACTUALLY HAPPENED, in the direction nothing used to look.
#
# Cases 2b-2e all walk the managed set and ask "is it wired?". For two days
# nothing asked the reverse, and the reverse is where the defect lived:
# guard-worktree-removal.sh was wired at 79d6958/084eed3, every forward check
# stayed green, and the engine ran a guard that no inventory in the system knew
# about — uncounted by the probe and uncounted by the session banner. This is
# that move, replayed with a seventeenth guard.
SB="$(make_sandbox)"
printf '#!/usr/bin/env bash\nexit 0\n' >"$SB/engine/scripts/hooks/guard-brand-new.sh"
chmod +x "$SB/engine/scripts/hooks/guard-brand-new.sh"
python3 - "$SB/engine/hooks/hooks.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["hooks"]["PreToolUse"].append({
    "matcher": "Bash",
    "hooks": [{"type": "command",
               "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/guard-brand-new.sh",
               "timeout": 10}],
})
json.dump(d, open(p, "w"), indent=2)
PY
run_probe "$SB"
expect_only_layer_failed "2f.BR2-guard-wired-but-not-declared-in-the-managed-set" "BR2" \
    "the managed set above does not name: guard-brand-new.sh"
rm -rf "$SB"

# ---------------------------------------------------------------------------
# 2f2 — A TENTH PreToolUse[Agent] HOOK, REGISTERED AND DECLARED, IS GREEN.
# ---------------------------------------------------------------------------
# THIS IS THE CASE THAT DID NOT EXIST ON 2026-09-14, and its absence is the
# whole reason that day cost a red `main`. guard-brief-scope.sh was registered
# as the NINTH Agent hook, correctly, and the engine went red — not because
# anything was wrong with it, but because two consumers inside the probe held a
# TYPED copy of the chain that still said eight. Every existing case here
# mutates the engine and asserts a REFUSAL; not one of them asked the opposite
# question, which is the one an engine that expects to grow has to answer:
#
#     DOES A CORRECTLY ADDED HOOK PASS?
#
# So this case adds a tenth, exactly the way the tenth will really be added —
# appended to the Agent chain in hooks/hooks.json, declared in the managed set,
# on disk, executable, hash-matched — and asserts the probe is GREEN.
#
# It fails on the pre-fix code, which is what makes it worth keeping: there,
# Layer C's typed CANONICAL_AGENT_CHAIN and BR2's typed BR_AGENT_WANT would both
# report the new hook as the defect. It is the negative control for the
# derivation itself — re-type either list and this case goes red.
#
# WHAT IT DELIBERATELY STILL COSTS: one line in BR_EXPECTED. That table is the
# probe's INDEPENDENT oracle and deriving it would make BR2 tautological (its
# own header says so), so a tenth hook still has to be DECLARED. The difference
# is that the probe names that one line when it is missing — case 2f above —
# whereas the two typed chains named nothing and failed somewhere else entirely.
#
# HOW "IT WAS ACTUALLY COUNTED" IS ASSERTED. A green BR2 prints a COUNT, not a
# roster — "all N managed guards registered exactly once …" — so grepping the
# output for the new hook's name proves nothing either way (the first draft of
# this case did exactly that and failed against a working engine). The
# observable that does carry the claim is the count itself: it must be exactly
# one higher than the same sandbox reports unmutated. A layer that skipped the
# tenth hook would print the baseline number and still be green, which is the
# 14/14-guards defect this engine has already paid for twice.
SB="$(make_sandbox)"
run_probe "$SB"
BR2_BASE_N="$(printf '%s\n' "$OUT" | sed -n 's/.*BR2\. all \([0-9][0-9]*\) managed guards.*/\1/p' | head -1)"
rm -rf "$SB"

SB="$(make_sandbox)"
# THE FIXTURE CARRIES A REAL ROOT BOOTSTRAP, copied from a real rooted hook
# rather than retyped. It was a bare `exit 0`, which was fine while Layer R
# walked a TYPED list this synthetic name was never in. Layer R derives the
# hooks it walks from hooks/hooks.json since 2026-09-14, so a registered hook
# that resolves its root any other way is named — correctly, and by this fixture
# too. Copying rather than retyping matters because R3 compares the block byte
# for byte.
{
    echo '#!/usr/bin/env bash'
    echo '# A synthetic TENTH PreToolUse[Agent] gate. It decides nothing: this case is'
    echo '# about whether the engine'"'"'s own inventories tolerate its existence.'
    echo 'set -euo pipefail'
    echo ''
    sed -n '/^# --- ROOT RESOLUTION ---/,/^ENGINE_ROOT="\$(resolve_engine_root/p' \
        "$SB/engine/scripts/hooks/notice-unlanded-branches.sh" \
        | sed 's|scripts/hooks/notice-unlanded-branches\.sh|scripts/hooks/guard-tenth-example.sh|'
    echo 'exit 0'
} >"$SB/engine/scripts/hooks/guard-tenth-example.sh"
chmod +x "$SB/engine/scripts/hooks/guard-tenth-example.sh"
# The sidecar, minted the way install.sh mints one, so BR4 hash-matches it
# rather than reporting it as the one script whose tamper check did not run.
shasum -a 256 "$SB/engine/scripts/hooks/guard-tenth-example.sh" \
    | awk '{print $1}' >"$SB/engine/scripts/hooks/guard-tenth-example.sh.sha256"
# Registered at the TAIL of the chain, which is where a policy gate goes and is
# the position the order rule deliberately leaves unconstrained.
python3 - "$SB/engine/hooks/hooks.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "Agent":
        entry["hooks"].append({
            "type": "command",
            "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/guard-tenth-example.sh",
            "timeout": 10,
        })
        break
else:
    sys.stderr.write("2f2: no Agent matcher in the sandbox hook table\n")
    sys.exit(3)
json.dump(d, open(p, "w"), indent=2)
PY
# Declared in the managed set — the one place a tenth hook still owes a line.
python3 - "$SB/engine/scripts/hooks/contract-integrity-probe.sh" <<'PY'
import sys
p = sys.argv[1]
src = open(p, encoding="utf-8").read()
anchor = "guard-brief-scope.sh|PreToolUse\n"
if anchor not in src:
    sys.stderr.write("2f2: BR_EXPECTED anchor not found in the sandbox probe\n")
    sys.exit(3)
open(p, "w", encoding="utf-8").write(
    src.replace(anchor, anchor + "guard-tenth-example.sh|PreToolUse\n", 1))
PY
run_probe "$SB"
if [ "$RC" -eq 0 ]; then
    ok "2f2.a-tenth-agent-hook-registered-and-declared-is-GREEN"
else
    bad "2f2.a-tenth-agent-hook-registered-and-declared-is-GREEN" \
        "probe exit was $RC, expected 0 — a correctly added tenth hook must not be a defect: $(printf '%s\n' "$OUT" | grep '✗' | head -3)"
fi
# And it was actually COUNTED, not merely tolerated by a layer that skipped it.
BR2_N="$(printf '%s\n' "$OUT" | sed -n 's/.*BR2\. all \([0-9][0-9]*\) managed guards.*/\1/p' | head -1)"
if [ -n "$BR2_BASE_N" ] && [ -n "$BR2_N" ] && [ "$BR2_N" -eq "$((BR2_BASE_N + 1))" ]; then
    ok "2f2b.the-tenth-hook-is-COUNTED-by-BR2-not-silently-skipped"
else
    bad "2f2b.the-tenth-hook-is-COUNTED-by-BR2-not-silently-skipped" \
        "BR2 counted [${BR2_N:-none}] managed guards, expected one more than the unmutated [${BR2_BASE_N:-none}]"
fi
rm -rf "$SB"

# The SESSION BANNER's copy of the inventory. engine-status.sh sizes its
# "N/M guards" line from scripts/lib/registered-hooks.sh, and that line is the
# first thing an operator reads every session — twice now it has been a full
# fraction over a stale inventory. The derivation removed the typed list; this
# case removes the last way it could still drift, by proving the probe notices
# when the banner's reading of hooks.json stops matching its own.
#
# The mutation is the historical defect itself: a hand-typed inventory, the
# same fourteen names the banner carried before the fix.
SB="$(make_sandbox)"
cat >"$SB/engine/scripts/lib/registered-hooks.sh" <<'STALE'
#!/usr/bin/env bash
registered_hook_scripts() {
    printf '%s\n' \
        guard-worktree-isolation.sh guard-definition-drift.sh guard-ceo-ask-first.sh \
        verify-agent-prompt.sh guard-main-checkout-writes.sh scan-secrets.sh \
        guard-resume-isolation.sh guard-bash-main-writes.sh guard-workflow-ban.sh \
        detect-nonnative-worktree.sh session-start-reap-worktrees.sh \
        snapshot-agent-definitions.sh teammate-idle-handoff.sh task-completed-handoff.sh
}
STALE
run_probe "$SB"
expect_only_layer_failed "2g.BR2-session-banner-inventory-disagrees-with-the-hook-table" "BR2" \
    "the session banner's guard inventory DISAGREES"
rm -rf "$SB"

# And if the library is gone entirely, the banner cannot count at all. An
# engine that opens every session with an unknown guard count is not a working
# engine, and "the file was missing" must not be something only the operator
# discovers.
SB="$(make_sandbox)"
rm -f "$SB/engine/scripts/lib/registered-hooks.sh"
run_probe "$SB"
expect_only_layer_failed "2h.BR2-guard-inventory-library-missing" "BR2" \
    "guard-inventory library MISSING"
rm -rf "$SB"


# ---------------------------------------------------------------------------
# BR7 — does the registration reach the next clone?
# ---------------------------------------------------------------------------
SB="$(make_sandbox)"
git -C "$SB" rm -q --cached ".claude-plugin/marketplace.json" >/dev/null 2>&1
printf '.claude-plugin/marketplace.json\n' >>"$SB/.gitignore"
git -C "$SB" add .gitignore >/dev/null 2>&1
git -C "$SB" commit -q -m "untrack" >/dev/null 2>&1
run_probe "$SB"
expect_only_layer_failed "7a.BR7-marketplace-manifest-untracked-and-ignored" "BR7" "reaches nobody else"
rm -rf "$SB"

SB="$(make_sandbox)"
rm -f "$SB/.claude-plugin/marketplace.json"
run_probe "$SB"
expect_only_layer_failed "7b.BR7-no-marketplace-manifest-at-all" "BR7" "no marketplace to add"
rm -rf "$SB"

# ---------------------------------------------------------------------------
# BR8 — the announcement, on both channels
# ---------------------------------------------------------------------------
# The operator channel specifically. On 2026-08-28 the announcement was firing
# perfectly into additionalContext, where only the model could see it, and an
# operator reading stdout and stderr concluded the engine was not loaded.
SB="$(make_sandbox)"
cat >"$SB/engine/scripts/hooks/engine-status.sh" <<'EOS'
#!/usr/bin/env bash
printf '%s\n' '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"ENFORCEMENT ACTIVE '"$RICHOS_ENTITY_ROOT"'"}}'
exit 0
EOS
chmod +x "$SB/engine/scripts/hooks/engine-status.sh"
run_probe "$SB"
if layer_failed "BR8" && out_has "no systemMessage"; then
    ok "8a.BR8-announcement-that-reaches-only-the-model-is-a-failure"
else
    bad "8a.BR8-announcement-that-reaches-only-the-model-is-a-failure" "BR8 accepted a model-only announcement"
fi
rm -rf "$SB"

# The negative arm's own negative control. A status hook that hardcodes ACTIVE
# satisfies every positive assertion while deciding nothing — mutation M9's
# failure, which is why BR8 also drives an UNADOPTED directory.
SB="$(make_sandbox)"
cat >"$SB/engine/scripts/hooks/engine-status.sh" <<'EOS'
#!/usr/bin/env bash
printf '%s\n' '{"systemMessage":"ENFORCEMENT ACTIVE '"$RICHOS_ENTITY_ROOT"'","hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"ENFORCEMENT ACTIVE '"$RICHOS_ENTITY_ROOT"'"}}'
exit 0
EOS
chmod +x "$SB/engine/scripts/hooks/engine-status.sh"
run_probe "$SB"
if layer_failed "BR8" && out_has "did not get STOOD DOWN"; then
    ok "8b.BR8-status-hook-that-always-says-ACTIVE-is-caught"
else
    bad "8b.BR8-status-hook-that-always-says-ACTIVE-is-caught" "BR8 accepted a hook that decides nothing"
fi
rm -rf "$SB"

# ---------------------------------------------------------------------------
# BR9 — a guard that still has teeth
# ---------------------------------------------------------------------------
SB="$(make_sandbox)"
cat >"$SB/engine/scripts/hooks/guard-worktree-isolation.sh" <<'EOS'
#!/usr/bin/env bash
exit 0
EOS
chmod +x "$SB/engine/scripts/hooks/guard-worktree-isolation.sh"
run_probe "$SB"
if layer_failed "BR9" && out_has "was ALLOWED"; then
    ok "9a.BR9-guard-gutted-into-a-no-op-is-caught"
else
    bad "9a.BR9-guard-gutted-into-a-no-op-is-caught" "BR9 accepted a guard that blocks nothing"
fi
rm -rf "$SB"

# The other direction, and the reason BR9 has a positive arm at all: a guard
# that refuses everything satisfies "does it block?" while being useless, and
# would take a working session down with it.
SB="$(make_sandbox)"
cat >"$SB/engine/scripts/hooks/guard-worktree-isolation.sh" <<'EOS'
#!/usr/bin/env bash
exit 2
EOS
chmod +x "$SB/engine/scripts/hooks/guard-worktree-isolation.sh"
run_probe "$SB"
if layer_failed "BR9" && out_has "was REFUSED"; then
    ok "9b.BR9-guard-that-blocks-everything-is-caught"
else
    bad "9b.BR9-guard-that-blocks-everything-is-caught" "BR9 accepted a guard that refuses every spawn"
fi
rm -rf "$SB"

# The roster read. A guard that compares two strings it was handed would pass
# 9a and 9b; only the model-truthfulness arm requires it to have actually opened
# the ENTITY's own agent definition.
SB="$(make_sandbox)"
cat >"$SB/engine/scripts/hooks/guard-worktree-isolation.sh" <<'EOS'
#!/usr/bin/env bash
PAYLOAD="$(cat)"
case "$PAYLOAD" in
    *'"isolation":"worktree"'*) exit 0 ;;
    *) exit 2 ;;
esac
EOS
chmod +x "$SB/engine/scripts/hooks/guard-worktree-isolation.sh"
run_probe "$SB"
if layer_failed "BR9" && out_has "did not read the entity's roster"; then
    ok "9c.BR9-guard-that-never-reads-the-roster-is-caught"
else
    bad "9c.BR9-guard-that-never-reads-the-roster-is-caught" "BR9 accepted a guard that only pattern-matches its payload"
fi
rm -rf "$SB"

by_reference_finish "by-reference-2.test.sh"
