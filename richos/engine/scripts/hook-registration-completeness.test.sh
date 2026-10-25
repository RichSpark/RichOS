#!/usr/bin/env bash
#
# hook-registration-completeness.test.sh — the suite for the predicate
# scripts/hook-registration-completeness.sh and its chokepoint
# scripts/hooks/guard-hook-registration-commits.sh.
#
# ===========================================================================
# WHAT THIS HAS TO PROVE, AND WHY EACH CASE EXISTS
# ===========================================================================
# The thing under test refuses commits, so the expensive failure is not a miss
# — it is a FALSE REFUSAL that makes somebody route around the guard. So the
# cases split three ways and the middle group is the largest:
#
#   IT FIRES      on an incomplete registration, naming every missing place.
#   IT IS SILENT  on a complete one, on an unrelated commit, on a repository
#                 with no engine, and — the one that decides whether this guard
#                 is worse than nothing — on a hook that DOES resolve a root,
#                 which must never be asked for R_ROOTLESS_HOOKS.
#   IT IS LOAD-BEARING: the negative cases are re-run against a MUTATED copy of
#                 the predicate, so a case that would pass for the wrong reason
#                 is caught. A negative test with no positive probe passes when
#                 the mechanism is deleted.
#
# EVERY CASE RUNS AGAINST A SANDBOX CLONE, never the live tree: the predicate
# reads a git baseline and a worktree, and a suite that mutated the real one
# would be a suite that can lose somebody's work.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CHECKER="$ENGINE_ROOT/scripts/hook-registration-completeness.sh"
GUARD="$ENGINE_ROOT/scripts/hooks/guard-hook-registration-commits.sh"

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf '  \033[32mok\033[0m   %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf '  \033[31mFAIL\033[0m %s\n' "$1"; [ "${2:-}" = "" ] || printf '%s\n' "$2" | sed 's/^/         /'; }

echo "=== hook-registration-completeness ==="

command -v git >/dev/null 2>&1 || { echo "git is required"; exit 2; }
[ -f "$CHECKER" ] || { bad "the predicate is missing at $CHECKER"; exit 1; }
[ -f "$GUARD" ]   || { bad "the guard is missing at $GUARD"; exit 1; }

SANDBOX="$(mktemp -d -t hookreg.XXXXXX)"
trap 'rm -rf "$SANDBOX"' EXIT

# --- THE FIXTURE -----------------------------------------------------------
# A minimal engine with everything the predicate derives from: a registration
# surface, a second registration surface, a probe carrying the three blocks it
# reads, an acknowledgement suite, and enough of ci-units.sh to answer "which
# files are suites". Small on purpose — a fixture that copied the real engine
# would pass or fail for reasons that have nothing to do with this predicate.
make_fixture() {
    local repo="$1"
    local peers=("hook-a.sh" "hook-b.sh" "hook-c.sh")
    mkdir -p "$repo/engine/hooks" "$repo/engine/.claude" \
             "$repo/engine/scripts/hooks" "$repo/engine/scripts/lib" "$repo/engine/docs"

    : > "$repo/engine/scripts/lib/registered-hooks.sh"

    {
        echo '{'
        echo '  "hooks": {'
        echo '    "PreToolUse": ['
        echo '      { "matcher": "Bash", "hooks": ['
        local first=1 p
        for p in "${peers[@]}"; do
            [ "$first" = 1 ] || echo '        ,'
            first=0
            echo "        { \"type\": \"command\", \"command\": \"bash \${CLAUDE_PLUGIN_ROOT}/scripts/hooks/$p\", \"timeout\": 10 }"
        done
        echo '      ] }'
        echo '    ]'
        echo '  }'
        echo '}'
    } > "$repo/engine/hooks/hooks.json"

    {
        echo '{'
        echo '  "hooks": {'
        echo '    "PreToolUse": ['
        echo '      { "matcher": "Bash", "hooks": ['
        local first=1 p
        for p in "${peers[@]}"; do
            [ "$first" = 1 ] || echo '        ,'
            first=0
            echo "        { \"type\": \"command\", \"command\": \"\$CLAUDE_PROJECT_DIR/scripts/hooks/$p\", \"timeout\": 10 }"
        done
        echo '      ] }'
        echo '    ]'
        echo '  }'
        echo '}'
    } > "$repo/engine/.claude/settings.local.json"

    {
        echo '#!/usr/bin/env bash'
        echo '# a fixture probe'
        echo '    R_ROOTLESS_HOOKS="hook-already-rootless"'
        echo ''
        echo '    BR_EXPECTED="\'
        local p
        for p in "${peers[@]}"; do echo "$p|PreToolUse"; done
        echo '"'
        echo ''
        echo 'CANONICAL_AGENT_CHAIN=('
        echo '    "$REPO_ROOT/scripts/hooks/hook-a.sh"'
        echo ')'
    } > "$repo/engine/scripts/hooks/contract-integrity-probe.sh"

    {
        echo '#!/usr/bin/env bash'
        echo '# a fixture acknowledgement suite'
        local p
        for p in "${peers[@]}"; do echo "# $p — what it is and why"; done
        echo "ACKNOWLEDGED_SCRIPTS=\"\$(LC_ALL=C sort <<'ACK'"
        for p in "${peers[@]}"; do echo "$p"; done
        echo 'ACK'
        echo ')"'
    } > "$repo/engine/scripts/hooks/engine-status.test.sh"

    local p
    for p in "${peers[@]}"; do
        {
            echo '#!/usr/bin/env bash'
            echo 'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"'
            echo '_RR_LIB="$SCRIPT_DIR/../lib/resolve-roots.sh"'
            echo '. "$_RR_LIB"'
        } > "$repo/engine/scripts/hooks/$p"
        chmod +x "$repo/engine/scripts/hooks/$p"
    done

    # The suite inventory: ci-units.sh `suites` is what A5 walks.
    {
        echo '#!/usr/bin/env bash'
        echo 'cd "$(dirname "$0")/.." || exit 2'
        echo 'find scripts -name "*.test.sh" | sed "s|^\./||" | LC_ALL=C sort'
    } > "$repo/engine/scripts/ci-units.sh"
    chmod +x "$repo/engine/scripts/ci-units.sh"

    # DELIBERATELY SHORT OF UNANIMITY — it names two of the three peers. A
    # suite that named ALL of them would itself be derived as an inventory (the
    # derivation is right to do that), and the fixture would then owe six
    # places where the live engine owes five. No suite in the real engine names
    # all 68 registered hooks, so the fixture should not either.
    {
        echo '#!/usr/bin/env bash'
        echo '# the fixture suite that names some of the peers'
        echo "# covers ${peers[0]}"
        echo "# covers ${peers[1]}"
    } > "$repo/engine/scripts/hooks/peers.test.sh"

    # NO user.email / user.name override. This machine runs a global
    # core.hooksPath identity guard that REFUSES any commit not authored by the
    # operator's address, so a helpfully-set test identity makes every fixture
    # commit fail and the whole suite then runs against EMPTY repositories —
    # where the predicate correctly answers NOT APPLICABLE and every negative
    # case passes for the wrong reason. Found by writing the suite and reading
    # the failures rather than the exit code; completeness-commits.test.sh
    # carries the same note for the same reason.
    git -C "$repo" init -q
    git -C "$repo" add -A
    # `-f` FOR THE SEATED SURFACE. This machine's global gitignore excludes
    # `.claude/`, so `git add -A` tracked eight of the fixture's nine files and
    # the ninth was the one inventory the predicate reads through `git grep`.
    # That is the same stranding trap probe Layer N exists for, reproduced by
    # accident in a fixture — and it is why the predicate now also looks at its
    # typed reader paths directly.
    git -C "$repo" add -f engine/.claude/settings.local.json
    git -C "$repo" commit -qm "fixture engine" >/dev/null
}

# register <repo> <name> [rooted]
# Adds a hook to hooks.json ONLY — the incomplete registration under test.
register() {
    local repo="$1" name="$2" rooted="${3:-1}"
    python3 - "$repo/engine/hooks/hooks.json" "$name" <<'PY'
import json, sys
p, name = sys.argv[1], sys.argv[2]
d = json.load(open(p))
d["hooks"]["PreToolUse"][0]["hooks"].append(
    {"type": "command", "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/" + name, "timeout": 10})
json.dump(d, open(p, "w"), indent=2)
open(p, "a").write("\n")
PY
    {
        echo '#!/usr/bin/env bash'
        if [ "$rooted" = "1" ]; then
            echo 'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"'
            echo '_RR_LIB="$SCRIPT_DIR/../lib/resolve-roots.sh"'
            echo '. "$_RR_LIB"'
        fi
        echo 'exit 0'
    } > "$repo/engine/scripts/hooks/$name"
    chmod +x "$repo/engine/scripts/hooks/$name"
}

# complete <repo> <name> [rooted] — pay every place the guard asks for.
complete() {
    local repo="$1" name="$2" rooted="${3:-1}" stem="${2%.sh}"
    python3 - "$repo/engine/.claude/settings.local.json" "$name" <<'PY'
import json, sys
p, name = sys.argv[1], sys.argv[2]
d = json.load(open(p))
d["hooks"]["PreToolUse"][0]["hooks"].append(
    {"type": "command", "command": "$CLAUDE_PROJECT_DIR/scripts/hooks/" + name, "timeout": 10})
json.dump(d, open(p, "w"), indent=2)
open(p, "a").write("\n")
PY
    python3 - "$repo/engine/scripts/hooks/contract-integrity-probe.sh" "$name" "$stem" "$rooted" <<'PY'
import sys
p, name, stem, rooted = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4] == "1"
t = open(p).read()
t = t.replace('hook-c.sh|PreToolUse\n', 'hook-c.sh|PreToolUse\n%s|PreToolUse\n' % name)
# INVERTED 2026-09-14 with Layer R. A ROOTED hook owes this inventory nothing —
# Layer R derives the hooks it walks from hooks.json, so registering it is the
# entry. It is the ROOTLESS hook that must now declare itself.
if not rooted:
    t = t.replace('R_ROOTLESS_HOOKS="hook-already-rootless"',
                  'R_ROOTLESS_HOOKS="hook-already-rootless %s"' % stem)
open(p, "w").write(t)
PY
    python3 - "$repo/engine/scripts/hooks/engine-status.test.sh" "$name" <<'PY'
import sys
p, name = sys.argv[1], sys.argv[2]
t = open(p).read()
t = t.replace('ACKNOWLEDGED_SCRIPTS=', '# %s — what it is and why it exists\nACKNOWLEDGED_SCRIPTS=' % name)
t = t.replace("<<'ACK'\n", "<<'ACK'\n%s\n" % name)
open(p, "w").write(t)
PY
    printf '# covers %s\n' "$name" >> "$repo/engine/scripts/hooks/peers.test.sh"
}

run() { bash "$CHECKER" --root "$1" "${@:2}" 2>&1; }

# ===========================================================================
# A — THE REPRODUCTION: an incomplete registration is refused by name
# ===========================================================================
R1="$SANDBOX/a"; make_fixture "$R1"; register "$R1" "guard-new-thing.sh" 1
OUT="$(run "$R1")"; RC=$?

if [ "$RC" = "1" ]; then ok "A1  an incomplete registration exits 1"
else bad "A1  expected rc 1, got $RC" "$OUT"; fi

# FOUR PLACES, NOT FIVE, SINCE 2026-09-14. guard-new-thing.sh is ROOTED, and a
# rooted hook no longer owes Layer R an entry: the layer derives the hooks it
# walks from hooks.json, so the registration IS the entry. The fifth place moved
# to the rootless case, which is C3.
for place in ".claude/settings.local.json" "contract-integrity-probe.sh" \
             "engine-status.test.sh" "A5"; do
    if printf '%s' "$OUT" | grep -qF -- "$place"; then
        ok "A2  the refusal names $place"
    else
        bad "A2  the refusal does NOT name $place" "$OUT"
    fi
done

# FOUR, AND THE NUMBER IS THE MEASUREMENT. This is positions-per-rule for a
# ROOTED hook, which is 57 of the 70 registered today, and it was 5 until Layer R
# stopped keeping a typed membership list on 2026-09-14. The four that remain are
# not removable by deriving anything: the seated surface is a second registration
# (a different problem), BR_EXPECTED and ACKNOWLEDGED_SCRIPTS are ORACLES that go
# tautological the moment they are derived from the file they are checked
# against, and A5 asks for a SUITE, which nothing can write for you. If this
# number changes, change it here deliberately and say why — it is the headline
# the whole verification-layer design is measured against.
if printf '%s' "$OUT" | grep -q "MISSING from 4 place"; then
    ok "A3  all four owed places are counted in one refusal"
else
    bad "A3  expected four owed places" "$OUT"
fi

# A REFUSAL THAT SAYS "INCOMPLETE" AND NOT "ADD X TO Y" IS HOW A GUARD BECOMES
# A THING PEOPLE FIGHT. Each place must carry an instruction, not a verdict.
if printf '%s' "$OUT" | grep -qi "add guard-new-thing.sh to ACKNOWLEDGED_SCRIPTS"; then
    ok "A4  the refusal says what to ADD, not merely that something is missing"
else
    bad "A4  no actionable instruction found" "$OUT"
fi

# ===========================================================================
# B — THE COMPLETE REGISTRATION PASSES SILENTLY
# ===========================================================================
R2="$SANDBOX/b"; make_fixture "$R2"; register "$R2" "guard-new-thing.sh" 1; complete "$R2" "guard-new-thing.sh" 1
OUT="$(run "$R2")"; RC=$?
if [ "$RC" = "0" ]; then ok "B1  a complete registration exits 0"
else bad "B1  expected rc 0, got $RC" "$OUT"; fi
if printf '%s' "$OUT" | grep -q "COMPLETE"; then ok "B2  and says so"
else bad "B2  no COMPLETE line" "$OUT"; fi

# ===========================================================================
# C — THE CONDITIONAL INVENTORIES STAY CONDITIONAL
# ===========================================================================
# THE CASE THAT DECIDES WHETHER THIS GUARD IS WORSE THAN NOTHING, INVERTED WITH
# LAYER R ON 2026-09-14. It used to be: a hook that resolves no root must never
# be asked for R_ROOTED_HOOKS, because Layer R asserted every hook it NAMED
# carried the bootstrap, so an obedient engineer adding it would make the probe
# assert something false.
#
# Layer R now derives the hooks it walks from hooks/hooks.json, so the false
# assertion is no longer available and the direction of the demand flips:
#
#   a ROOTED hook owes this inventory NOTHING — registering it is the entry, and
#     that is the whole point of the derivation. C1/C2.
#   a ROOTLESS hook must DECLARE itself, or Layer R names it for not sourcing a
#     library it has no reason to source. C3.
#
# handoff-facts-annotate.sh is a live instance of the rootless shape.
# notice-protected-ref-moves.sh was named here as a second one and that was
# WRONG: measured 2026-09-14 it both sources resolve-roots.sh and assigns
# ENGINE_ROOT, and it has been in the rooted set the whole time.
R3="$SANDBOX/c"; make_fixture "$R3"; register "$R3" "guard-rooted-thing.sh" 1; complete "$R3" "guard-rooted-thing.sh" 1
OUT="$(run "$R3" --explain)"; RC=$?
if [ "$RC" = "0" ]; then ok "C1  a rooted hook, complete everywhere else, exits 0"
else bad "C1  expected rc 0, got $RC" "$OUT"; fi
if printf '%s' "$OUT" | grep -q "R_ROOTLESS_HOOKS correctly does not name it"; then
    ok "C2  R_ROOTLESS_HOOKS is NOT demanded of a hook that DOES resolve a root"
else
    bad "C2  the rootless condition was not evaluated as a pass" "$OUT"
fi

# The direction Layer R actually goes red on now.
R4="$SANDBOX/c2"; make_fixture "$R4"; register "$R4" "notice-rootless.sh" 0; complete "$R4" "notice-rootless.sh" 1
OUT="$(run "$R4")"; RC=$?
# Matched on two fragments rather than one phrase: the refusal wraps its prose
# at 66 columns, so the instruction can be split across a line break and a
# single-phrase grep would report a working check as broken.
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q "add" \
   && printf '%s' "$OUT" | grep -q "notice-rootless' to R_ROOTLESS_HOOKS"; then
    ok "C3  a rootless hook NOT declared in R_ROOTLESS_HOOKS is told to add it"
else
    bad "C3  expected an add instruction, rc=$RC" "$OUT"
fi

# The Agent chain, conditional on the matcher and not on the event.
R5="$SANDBOX/c3"; make_fixture "$R5"; register "$R5" "guard-spawny.sh" 1; complete "$R5" "guard-spawny.sh" 1
OUT="$(run "$R5" --explain)"; RC=$?
if [ "$RC" = "0" ] && ! printf '%s' "$OUT" | grep -q "CANONICAL_AGENT_CHAIN"; then
    ok "C4  a Bash-matcher hook is never asked for CANONICAL_AGENT_CHAIN"
else
    bad "C4  the Agent chain was demanded of a Bash hook, rc=$RC" "$OUT"
fi

# ===========================================================================
# D — A NEW HOOK SCRIPT THAT IS NOT REGISTERED AT ALL
# ===========================================================================
# The sharpest instance in the failure record: ref-transaction-forensics.sh is
# a git reference-transaction hook, absent from hooks.json entirely, and it
# landed named by NO suite — "the thing watching the refs had nothing watching
# it". A trigger that only watched hooks.json would miss it.
R6="$SANDBOX/d"; make_fixture "$R6"
printf '#!/usr/bin/env bash\nexit 0\n' > "$R6/engine/scripts/hooks/ref-recorder.sh"
chmod +x "$R6/engine/scripts/hooks/ref-recorder.sh"
OUT="$(run "$R6")"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q "A5"; then
    ok "D1  an UNREGISTERED new hook script is caught by the suite inventory"
else
    bad "D1  expected an A5 failure for an unregistered hook, rc=$RC" "$OUT"
fi
if printf '%s' "$OUT" | grep -q "unregistered script"; then
    ok "D2  and is marked unregistered rather than reported as a registration"
else
    bad "D2  the unregistered subject was not marked as such" "$OUT"
fi
# AND IT IS ASKED FOR NOTHING ELSE: an unregistered hook is in no registration
# inventory by design, so demanding one would be the false-positive class.
if [ "$(printf '%s' "$OUT" | grep -c "MISSING from 1 place")" = "1" ]; then
    ok "D3  an unregistered hook owes A5 and nothing else"
else
    bad "D3  an unregistered hook was asked for a registration inventory" "$OUT"
fi

# ===========================================================================
# E — SILENCE WHERE THERE IS NOTHING TO SAY
# ===========================================================================
R7="$SANDBOX/e"; make_fixture "$R7"
printf 'hello\n' > "$R7/engine/docs/note.md"
OUT="$(run "$R7")"; RC=$?
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "E1  an unrelated change is passed in complete silence"
else
    bad "E1  expected a silent rc 0, got rc=$RC" "$OUT"
fi

R8="$SANDBOX/e2"; mkdir -p "$R8"; git -C "$R8" init -q
OUT="$(run "$R8")"; RC=$?
if [ "$RC" = "2" ] && printf '%s' "$OUT" | grep -q "NOT APPLICABLE"; then
    ok "E2  a repository with no engine is NOT APPLICABLE, not an error"
else
    bad "E2  expected NOT APPLICABLE, rc=$RC" "$OUT"
fi

# ===========================================================================
# F — FAIL LOUD, NEVER SKIP
# ===========================================================================
# A predicate that cannot find the block it reads must refuse, not pass. The
# alternative is a membership test against a block it can no longer locate,
# which passes everything — a green tick over the failure it exists to catch.
R9="$SANDBOX/f"; make_fixture "$R9"; register "$R9" "guard-new-thing.sh" 1
python3 - "$R9/engine/scripts/hooks/contract-integrity-probe.sh" <<'PY'
import sys
p = sys.argv[1]
t = open(p).read().replace('BR_EXPECTED="\\', 'BR_RENAMED="\\')
open(p, "w").write(t)
PY
OUT="$(run "$R9")"; RC=$?
if [ "$RC" = "2" ] && printf '%s' "$OUT" | grep -q "BROKEN"; then
    ok "F1  a probe whose BR_EXPECTED table cannot be located is BROKEN, not a pass"
else
    bad "F1  expected rc 2 BROKEN, got rc=$RC" "$OUT"
fi

# A typed structural reader whose file stopped being an inventory.
R10="$SANDBOX/f2"; make_fixture "$R10"; register "$R10" "guard-new-thing.sh" 1
printf '#!/usr/bin/env bash\n# names nothing\n' > "$R10/engine/scripts/hooks/engine-status.test.sh"
OUT="$(run "$R10")"; RC=$?
if [ "$RC" = "2" ] && printf '%s' "$OUT" | grep -q "stopped being an inventory"; then
    ok "F2  a typed reader that lost unanimity is reported, never silently skipped"
else
    bad "F2  expected the stale-reader refusal, rc=$RC" "$OUT"
fi

# ===========================================================================
# G — THE DERIVATION IS A DERIVATION
# ===========================================================================
# A file that names every peer becomes an inventory with nothing told to the
# predicate. This is the claim the whole design rests on, so it is executed
# rather than asserted.
R11="$SANDBOX/g"; make_fixture "$R11"
printf '# a NEW inventory nobody told the predicate about\nhook-a.sh\nhook-b.sh\nhook-c.sh\n' \
    > "$R11/engine/docs/sixth-inventory.md"
git -C "$R11" add -A >/dev/null 2>&1
git -C "$R11" commit -qm "a sixth inventory" >/dev/null 2>&1
register "$R11" "guard-new-thing.sh" 1; complete "$R11" "guard-new-thing.sh" 1
OUT="$(run "$R11")"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q "sixth-inventory.md"; then
    ok "G1  a file that reaches unanimity is demanded with NO typed list naming it"
else
    bad "G1  the derived sixth inventory was not demanded, rc=$RC" "$OUT"
fi

# And a file well short of unanimity is never demanded — the README case,
# which a hand audit had to argue about and this settles by measurement.
R12="$SANDBOX/g2"; make_fixture "$R12"
printf '# a per-system table\nhook-a.sh\n' > "$R12/engine/README.md"
git -C "$R12" add -A >/dev/null 2>&1
git -C "$R12" commit -qm "a partial mention" >/dev/null 2>&1
register "$R12" "guard-new-thing.sh" 1; complete "$R12" "guard-new-thing.sh" 1
OUT="$(run "$R12")"; RC=$?
if [ "$RC" = "0" ]; then
    ok "G2  a file naming SOME peers is not an inventory and is never demanded"
else
    bad "G2  a partial mention was treated as an inventory, rc=$RC" "$OUT"
fi

# ===========================================================================
# H — THE CHOKEPOINT: the guard classifies, and refuses, a real payload
# ===========================================================================
# ===========================================================================
# DR — A DATED RECORD IS NOT AN INVENTORY (2026-10-24)
# ===========================================================================
# An audit dated in its name that names every registered hook keeps the names
# of its day. A registration change must pass with NO hook-inventory-ack and
# without writing the new name into it (DR1); a real missing registration must
# still fail on the real places (DR2); and the same file WITHOUT a date is
# still an inventory, so the exclusion is the date rule and nothing else (DR3).
add_audit() { # <repo> <relative name>
    mkdir -p "$1/engine/docs/audits"
    {
        echo "# which hooks did what on the day"
        echo "| hook-a.sh | hook-b.sh | hook-c.sh |"
    } > "$1/engine/$2"
    git -C "$1" add -A
    git -C "$1" commit -qm "a dated audit" >/dev/null
}
RDR1="$SANDBOX/dr1"; make_fixture "$RDR1"; add_audit "$RDR1" "docs/audits/hooks-audit-2026-09-20.md"
register "$RDR1" "guard-new-thing.sh" 1; complete "$RDR1" "guard-new-thing.sh" 1
OUT="$(run "$RDR1")"; RC=$?
if [ "$RC" = "0" ] && ! printf '%s' "$OUT" | grep -qF "hooks-audit-2026-09-20.md"; then
    ok "DR1 a complete registration passes with no ack and leaves the dated audit alone"
else bad "DR1 expected rc 0 and no mention of the dated audit, got $RC" "$OUT"; fi
RDR2="$SANDBOX/dr2"; make_fixture "$RDR2"; add_audit "$RDR2" "docs/audits/hooks-audit-2026-09-20.md"
register "$RDR2" "guard-new-thing.sh" 1
OUT="$(run "$RDR2")"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -q "MISSING from 4 place" \
        && ! printf '%s' "$OUT" | grep -qF "hooks-audit-2026-09-20.md"; then
    ok "DR2 a real missing registration still fails on its four real places"
else bad "DR2 expected rc 1 naming four places and not the dated audit, got $RC" "$OUT"; fi
RDR3="$SANDBOX/dr3"; make_fixture "$RDR3"; add_audit "$RDR3" "docs/audits/hooks-audit.md"
register "$RDR3" "guard-new-thing.sh" 1; complete "$RDR3" "guard-new-thing.sh" 1
OUT="$(run "$RDR3")"; RC=$?
if [ "$RC" = "1" ] && printf '%s' "$OUT" | grep -qF "docs/audits/hooks-audit.md"; then
    ok "DR3 the same file without a date is still an inventory (control)"
else bad "DR3 expected rc 1 naming the undated file, got $RC" "$OUT"; fi

payload() {
    python3 -c 'import json,sys; print(json.dumps({"tool_name":"Bash","tool_input":{"command":sys.argv[1]}}))' "$1"
}
grun() {
    local repo="$1" cmd="$2"
    payload "$cmd" | (cd "$repo" && HOOK_INVENTORY_ACK_LOG="$SANDBOX/acks.log" bash "$GUARD" 2>&1)
}

R13="$SANDBOX/h"; make_fixture "$R13"; register "$R13" "guard-new-thing.sh" 1
OUT="$(grun "$R13" "git commit -m 'wire the new guard'")"; RC=$?
if [ "$RC" = "2" ] && printf '%s' "$OUT" | grep -q "REFUSING THIS commit"; then
    ok "H1  the guard REFUSES a commit carrying an incomplete registration"
else
    bad "H1  expected a blocking refusal, rc=$RC" "$OUT"
fi
if printf '%s' "$OUT" | grep -q "hook-inventory-ack:"; then
    ok "H2  and the refusal names its own escape hatch"
else
    bad "H2  the refusal hides the hatch — that is how a guard gets routed around" "$OUT"
fi

OUT="$(grun "$R13" "ls -la")"; RC=$?
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "H3  an ordinary shell command is passed in silence"
else
    bad "H3  a non-git command was not passed silently, rc=$RC" "$OUT"
fi

# The classifier must not read a commit MESSAGE as a verb.
OUT="$(grun "$R13" "echo 'nothing to commit here'")"; RC=$?
if [ "$RC" = "0" ]; then
    ok "H4  the word 'commit' inside a quoted string is not a commit"
else
    bad "H4  a quoted word was classified as a git verb, rc=$RC" "$OUT"
fi

# The hatch: a real reason passes and is logged; a bare marker does not.
OUT="$(grun "$R13" "git commit -m x # hook-inventory-ack: probe held by another agent on cc/x, paid at merge")"; RC=$?
if [ "$RC" = "0" ] && printf '%s' "$OUT" | grep -q "allowed on a declared ack"; then
    ok "H5  a reasoned ack is accepted"
else
    bad "H5  a reasoned ack was not accepted, rc=$RC" "$OUT"
fi
if [ -s "$SANDBOX/acks.log" ]; then
    ok "H6  and is recorded durably, so a hatch that becomes a habit can be seen"
else
    bad "H6  the ack was not logged"
fi
OUT="$(grun "$R13" "git commit -m x # hook-inventory-ack: later")"; RC=$?
if [ "$RC" = "2" ] && printf '%s' "$OUT" | grep -q "MARKER, NOT A REASON"; then
    ok "H7  a bare marker exempts nothing"
else
    bad "H7  a bare marker was accepted, rc=$RC" "$OUT"
fi

# A repository that carries no engine is never touched.
R14="$SANDBOX/h2"; mkdir -p "$R14"; git -C "$R14" init -q

printf 'x\n' > "$R14/f"; git -C "$R14" add -A; git -C "$R14" commit -qm x
OUT="$(grun "$R14" "git commit -m x")"; RC=$?
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "H8  a repository with no engine is passed in silence"
else
    bad "H8  an unrelated repository was not passed silently, rc=$RC" "$OUT"
fi

# ===========================================================================
# Grouped product checkouts must reach the same predicate through the guard.
# Commit the relocation first so the baseline has the same engine prefix.
R15="$SANDBOX/grouped"; make_fixture "$R15"
mkdir -p "$R15/richos"
mv "$R15/engine" "$R15/richos/engine"
git -C "$R15" add -A
git -C "$R15" add -f richos/engine/.claude/settings.local.json
git -C "$R15" commit -qm "group product sources"
register "$R15/richos" "guard-new-thing.sh" 1
OUT="$(grun "$R15" "git commit -m 'wire the new guard'")"; RC=$?
if [ "$RC" = "2" ] && printf '%s' "$OUT" | grep -q "REFUSING THIS commit"; then
    ok "H9  grouped checkout refuses an incomplete registration"
else
    bad "H9  grouped checkout bypassed enforcement, rc=$RC" "$OUT"
fi
complete "$R15/richos" "guard-new-thing.sh" 1
OUT="$(grun "$R15" "git commit -m 'wire the complete guard'")"; RC=$?
if [ "$RC" = "0" ] && [ -z "$OUT" ]; then
    ok "H10 grouped checkout accepts a complete registration"
else
    bad "H10 grouped checkout refused complete inventories, rc=$RC" "$OUT"
fi

# I — LOAD-BEARING: the negatives fail when the mechanism is removed
# ===========================================================================
# A negative test with no positive probe passes for the wrong reason. Each
# mutant deletes one decision and the case that covers it must go red.
mutate() {
    # THREE SEPARATE `local` STATEMENTS, not one. bash expands every word on a
    # `local` line BEFORE performing any of its assignments, so
    # `local a="$1" b="$SANDBOX/$a"` reads an unset `a` — which under `set -u`
    # kills the function and reports "the mutant could not be built" for all
    # three mutants at once. The suite would then have announced its own
    # load-bearing proof as missing.
    local name="$1"
    local expr="$2"
    local copy="$SANDBOX/mutant-$name.sh"
    cp "$CHECKER" "$copy"
    if python3 - "$copy" "$expr" <<'PY'
import sys
p, expr = sys.argv[1], sys.argv[2]
old, new = expr.split("\x1e")
t = open(p).read()
if old not in t:
    sys.stderr.write("MUTANT %s: the target text is not present\n" % p)
    raise SystemExit(3)
open(p, "w").write(t.replace(old, new, 1))
PY
    then printf '%s' "$copy"; else printf ''; fi
}

M1="$(mutate unanimity 'if all(p in t for p in PEERS):'$'\x1e''if False:')"
if [ -n "$M1" ]; then
    OUT="$(bash "$M1" --root "$SANDBOX/a" 2>&1)"; RC=$?
    if [ "$RC" != "1" ] || ! printf '%s' "$OUT" | grep -q "settings.local.json"; then
        ok "I1  deleting the unanimity derivation breaks case A — the derivation is load-bearing"
    else
        bad "I1  case A still passed with the derivation deleted: it proves nothing" "$OUT"
    fi
else
    bad "I1  the unanimity mutant could not be built"
fi

# I2 INVERTED WITH THE DEMAND. Hard-coding `rooted = True` means the mutant can
# never see a hook that resolves no root, so the R_ROOTLESS_HOOKS demand never
# fires — and C3's sandbox, which SHOULD be refused, comes back complete. The
# mutant is proved by the demand going MISSING, which is the direction a
# conditional inventory actually fails in: silently, on the case it was written
# for. Run against $SANDBOX/c2, which is case C3's tree.
M2="$(mutate rooted 'rooted = sources_roots_lib(s)'$'\x1e''rooted = True')"
if [ -n "$M2" ]; then
    OUT="$(bash "$M2" --root "$SANDBOX/c2" 2>&1)"; RC=$?
    if [ "$RC" != "1" ] || ! printf '%s' "$OUT" | grep -q "R_ROOTLESS_HOOKS"; then
        ok "I2  hard-coding the rooted test makes case C3 stop firing — the CONDITION is load-bearing"
    else
        bad "I2  case C3 still fired with the rooted condition hard-coded: it proves nothing, rc=$RC" "$OUT"
    fi
else
    bad "I2  the rooted mutant could not be built"
fi

M3="$(mutate a5 'hit = named_by_a_suite(s)'$'\x1e''hit = "stub"')"
if [ -n "$M3" ]; then
    OUT="$(bash "$M3" --root "$SANDBOX/d" 2>&1)"; RC=$?
    if [ "$RC" != "1" ]; then
        ok "I3  stubbing the suite lookup breaks case D — the A5 check is load-bearing"
    else
        bad "I3  case D still failed with the suite lookup stubbed" "$OUT"
    fi
else
    bad "I3  the A5 mutant could not be built"
fi

# ===========================================================================
# J — AN INVENTORY THAT COULD NOT BE READ IS NOT "COMPLETE" (hunt P5-08)
# ===========================================================================
# Suite discovery (ci-units.sh suites) failing used to be an advisory followed by
# "COMPLETE", exit 0. The control is case B's exact fixture (a complete
# registration that passes); the only difference is a ci-units.sh that fails.
R16="$SANDBOX/j"; make_fixture "$R16"; register "$R16" "guard-new-thing.sh" 1; complete "$R16" "guard-new-thing.sh" 1
OUT="$(run "$R16")"; RC=$?
if [ "$RC" = "0" ]; then ok "J1  (control) the complete registration passes while suite discovery works"
else bad "J1  the control fixture should pass, got rc $RC" "$OUT"; fi

printf '#!/usr/bin/env bash\nexit 2\n' > "$R16/engine/scripts/ci-units.sh"
OUT="$(run "$R16")"; RC=$?
if [ "$RC" = "2" ] && printf '%s' "$OUT" | grep -qF "suite inventory" \
   && ! printf '%s' "$OUT" | grep -qF "COMPLETE: every derived inventory"; then
    ok "J2  suite discovery failing is BROKEN (exit 2), never COMPLETE"
else
    bad "J2  expected rc 2 naming the suite inventory and no COMPLETE line, got rc $RC" "$OUT"
fi

# A real owing place keeps its own exit code: the fact outranks the gap.
register "$R16" "guard-other-thing.sh" 1
OUT="$(run "$R16")"; RC=$?
if [ "$RC" = "1" ]; then ok "J3  an owing place still exits 1 when suite discovery is also failing"
else bad "J3  expected rc 1 (the established fact), got $RC" "$OUT"; fi

# ===========================================================================
echo ""
echo "  $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
