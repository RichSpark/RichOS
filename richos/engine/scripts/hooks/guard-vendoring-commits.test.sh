#!/usr/bin/env bash
#
# guard-vendoring-commits.test.sh — regression tests for
# scripts/hooks/guard-vendoring-commits.sh and its predicate,
# scripts/lib/vendored-material.sh.
#
# Five sections, and the second is the one that decides whether this guard
# survives its first week.
#
#   A. POSITIVES — it refuses an unrecorded vendoring, across the command
#      shapes people actually type (`git commit`, `cd x && git commit`,
#      `git -C x commit`, `--amend`, a MULTI-LINE message), and the refusal
#      NAMES the path, the registry and the line to add.
#   B. NEGATIVES — it does not fire on a recorded addition, a modification, a
#      deletion, an ungoverned path, a dry run, a merge, a push, or a
#      repository that declares nothing. A guard that cries wolf is switched
#      off within a day, and then the rule has no chokepoint again.
#   C. CONFIGURATION AND FAILURE MODES — a malformed registry is fail-closed
#      with a named reason, a malformed payload is fail-open, two copies of the
#      declaration are BROKEN rather than a choice.
#   D. THE SHIPPED REGISTRY ITSELF — it parses, everything it names exists,
#      everything under a governed path is covered, the 2026-09-04 audit's
#      fifteen third-party skills are all in it, and its two qualified
#      confidences are still qualified. A registry that drifted from the tree
#      would leave this guard green over a fiction.
#   E. REGISTRATION — wired on both surfaces, declared in the probe's spec, and
#      exactly ONE parser of the registry exists.
#
# Run directly: scripts/hooks/guard-vendoring-commits.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# The registration question has ONE answer in this engine, and it is not a
# grep: seventeen guards are wired by scripts/hooks/dispatch-pretooluse.manifest
# rather than by an entry of their own. hook_enforced_on_surface asks whether
# the surface causes the guard to RUN, which is the property this case is about.
# shellcheck source=../lib/registered-hooks.sh
. "$ENGINE_ROOT/scripts/lib/registered-hooks.sh"


# Declare the governed SEAT rather than inheriting the launching session's —
# same reasoning as scan-secrets.test.sh and dialect-guard.test.sh: run from a
# session seated elsewhere, the guard would resolve THAT repository, find no
# adoption marker, stand down, and every case below would pass by never running.
RICHOS_ENTITY_ROOT="$ENGINE_ROOT"
export RICHOS_ENTITY_ROOT
unset CLAUDE_PROJECT_DIR

HOOK="$SCRIPT_DIR/guard-vendoring-commits.sh"
LIB="$ENGINE_ROOT/scripts/lib/vendored-material.sh"

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d -t guard-vendoring-test.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s%s\n' "$1" "${2:+ — $2}"; FAIL=$((FAIL + 1)); }

# An empty hooks dir, so a machine-wide pre-commit hook (this one has an
# identity guard) cannot decide the outcome of a test about a different thing.
NOHOOKS="$SANDBOX/nohooks"
mkdir -p "$NOHOOKS"

# mkrepo <dir> — a git repository with one commit and no engine adoption.
mkrepo() {
    local d="$1"
    mkdir -p "$d"
    git -C "$d" init -q
    git -C "$d" config user.email webdevbooster@gmail.com
    git -C "$d" config user.name tester
    git -C "$d" config commit.gpgsign false
    git -C "$d" config core.hooksPath "$NOHOOKS"
    printf 'seed\n' >"$d/seed.txt"
    git -C "$d" add -A >/dev/null 2>&1
    git -C "$d" commit -qm seed >/dev/null 2>&1
}

# registry <dir> <body...> — write .richos/vendored-material and commit it.
registry() {
    local d="$1"; shift
    mkdir -p "$d/.richos"
    printf '%s\n' "$@" >"$d/.richos/vendored-material"
    git -C "$d" add -A >/dev/null 2>&1
    git -C "$d" commit -qm registry >/dev/null 2>&1
}

TAB=$'\t'
ENTRY_KNOWN="engine/skills/known${TAB}third-party${TAB}MIT${TAB}Someone${TAB}up/stream${TAB}abc123${TAB}2026-01-01 x${TAB}certain${TAB}verbatim${TAB}docs/x.md"
ENTRY_OURS="engine/skills/ours${TAB}richos${TAB}AGPL-3.0-only${TAB}RichOS${TAB}-${TAB}-${TAB}2026-01-01 x${TAB}high${TAB}verbatim${TAB}docs/x.md"
ENTRY_FILE="assets/one.bin${TAB}third-party${TAB}MIT${TAB}Someone${TAB}up/stream${TAB}abc123${TAB}2026-01-01 x${TAB}certain${TAB}verbatim${TAB}docs/x.md"

# payload <cwd> <command>
payload() {
    python3 -c '
import json, sys
print(json.dumps({"tool_name": "Bash", "cwd": sys.argv[1],
                  "tool_input": {"command": sys.argv[2]}}))' "$1" "$2"
}

# run <cwd> <command> -> exit code, stderr in RUN_ERR
RUN_ERR=""
run() {
    local rc=0
    RUN_ERR="$(payload "$1" "$2" | "$HOOK" 2>&1 >/dev/null)" || rc=$?
    return $rc
}

# case_exit <name> <want-rc> <cwd> <command>
case_exit() {
    local name="$1" want="$2" cwd="$3" cmd="$4" rc=0
    run "$cwd" "$cmd" || rc=$?
    [ "$rc" -eq "$want" ] && ok "$name" || bad "$name" "expected exit $want, got $rc"
}

# case_msg <name> <needle> <cwd> <command>
case_msg() {
    local name="$1" needle="$2" cwd="$3" cmd="$4"
    run "$cwd" "$cmd" || true
    printf '%s' "$RUN_ERR" | grep -qF "$needle" && ok "$name" \
        || bad "$name" "stderr did not mention \"$needle\""
}

# ===========================================================================
# The standard subject: a repository governing engine/skills and assets, with
# one third-party entry, one RichOS entry and one exact-FILE entry.
# ===========================================================================
R="$SANDBOX/subject"
mkrepo "$R"
registry "$R" \
    '# subject registry' \
    'REDISTRIBUTABLE_PATHS="engine/skills assets"' \
    "$ENTRY_KNOWN" "$ENTRY_OURS" "$ENTRY_FILE"

# stage_new <relpath> — the subject repository holds EXACTLY ONE new file.
#
# The `--hard` and the `clean` are not tidiness. Without them, every earlier
# case's untracked leftovers are re-staged by `git add -A`, so a case asserting
# "this passes" can be carrying an unrecorded file from four cases ago and a
# case asserting "this refuses" can be refusing for it. B17 did exactly that
# and its mutant proved it: the case was green and completely insensitive to
# the property it named. A negative test that passes for the wrong reason is
# the failure this whole harness exists to catch.
stage_new() { # <relpath>
    git -C "$R" reset -q --hard >/dev/null 2>&1
    git -C "$R" clean -qfdx >/dev/null 2>&1
    mkdir -p "$R/$(dirname "$1")"
    printf 'content\n' >"$R/$1"
    git -C "$R" add -A >/dev/null 2>&1
}

echo "=== A. POSITIVES — it refuses an unrecorded vendoring ==="

stage_new "engine/skills/newthing/SKILL.md"
case_exit "A1. an unrecorded skill is refused"                 2 "$R" 'git commit -m "add a skill"'
case_msg  "A2. the refusal NAMES the unit path"                "engine/skills/newthing" "$R" 'git commit -m "add a skill"'
case_msg  "A3. the refusal names the registry file"            ".richos/vendored-material" "$R" 'git commit -m "add a skill"'
case_msg  "A4. the refusal names the ten-field line to add"    "path<TAB>origin<TAB>license" "$R" 'git commit -m "add a skill"'
case_msg  "A5. the refusal names the governed paths"           "engine/skills assets" "$R" 'git commit -m "add a skill"'
case_msg  "A6. the refusal tells you to record your own work"  "RECORD YOUR OWN WORK TOO" "$R" 'git commit -m "add a skill"'

# THE MULTI-LINE MESSAGE. Not an exotic input: it is how every commit in this
# project is written, and the classifier this guard's sibling carries splits on
# a bare newline, so both halves fail to parse and NO `git commit` is seen at
# all. Caught on this guard's own first smoke run, where three "PASS" results
# were all passing for that reason.
case_exit "A7. a MULTI-LINE commit message is still classified" 2 "$R" 'git commit -m "add a skill

with a second paragraph"'

case_exit "A8. a bare 'vendoring-ack:' exempts nothing"        2 "$R" 'git commit -m "add a skill

vendoring-ack:"'
case_exit "A9. a two-word reason exempts nothing"              2 "$R" 'git commit -m "add a skill

vendoring-ack: it is fine"'
case_exit "A10. a PROMISE to record it later is refused"       2 "$R" 'git commit -m "add a skill

vendoring-ack: I am going to add the registry entry in a follow-up commit next week"'
case_msg  "A11. and the refusal names the promise as the problem" "PROMISE TO RECORD IT LATER" "$R" 'git commit -m "add a skill

vendoring-ack: I am going to add the registry entry in a follow-up commit next week"'

case_exit "A12. 'cd <repo> && git commit' is judged in <repo>" 2 "$SANDBOX" "cd $R && git commit -m 'add a skill'"
case_exit "A13. 'git -C <repo> commit' is judged in <repo>"    2 "$SANDBOX" "git -C $R commit -m 'add a skill'"

# --amend rewrites the commit that is already there, so its additions are
# additions again and the base widens to HEAD~1.
git -C "$R" reset -q >/dev/null 2>&1
mkdir -p "$R/engine/skills/amended"
printf 'x\n' >"$R/engine/skills/amended/SKILL.md"
git -C "$R" add -A >/dev/null 2>&1
git -C "$R" commit -qm "landed unrecorded" >/dev/null 2>&1
case_exit "A14. --amend re-checks the additions it rewrites"   2 "$R" 'git commit --amend -m "reworded"'
git -C "$R" reset -q --hard HEAD~1 >/dev/null 2>&1

echo
echo "=== B. NEGATIVES — it does not cry wolf ==="

stage_new "engine/skills/known/references/new.md"
case_exit "B1. an addition inside a RECORDED skill passes"     0 "$R" 'git commit -m "extend a recorded skill"'

stage_new "engine/skills/ours/SKILL.md"
case_exit "B2. an addition recorded as origin=richos passes"   0 "$R" 'git commit -m "our own skill"'

stage_new "assets/one.bin"
case_exit "B3. an exact-FILE entry covers its file"            0 "$R" 'git commit -m "the recorded asset"'

stage_new "docs/notes.md"
case_exit "B4. an addition outside every governed path passes" 0 "$R" 'git commit -m "unrelated"'

# PATH-BOUNDARY, not string-prefix. `engine/skills-archive` starts with
# `engine/skills` and is a different directory.
stage_new "engine/skills-archive/thing.md"
case_exit "B5. a sibling path that merely SHARES A PREFIX passes" 0 "$R" 'git commit -m "not under skills"'

# A MODIFICATION is not an addition. Whoever added the file answered the
# question then; re-asking here would refuse every edit to vendored material.
git -C "$R" reset -q --hard >/dev/null 2>&1
git -C "$R" reset -q >/dev/null 2>&1
mkdir -p "$R/engine/skills/mystery"
printf 'v1\n' >"$R/engine/skills/mystery/SKILL.md"
git -C "$R" add -A >/dev/null 2>&1
git -C "$R" -c core.hooksPath="$NOHOOKS" commit -qm "pre-existing, unrecorded" >/dev/null 2>&1
printf 'v2\n' >"$R/engine/skills/mystery/SKILL.md"
git -C "$R" add -A >/dev/null 2>&1
case_exit "B6. MODIFYING an unrecorded governed file passes"   0 "$R" 'git commit -m "edit"'

git -C "$R" reset -q --hard >/dev/null 2>&1
git -C "$R" rm -q -r engine/skills/mystery >/dev/null 2>&1
case_exit "B7. DELETING under a governed path passes"          0 "$R" 'git commit -m "remove"'
git -C "$R" reset -q --hard >/dev/null 2>&1

stage_new "engine/skills/newthing/SKILL.md"
case_exit "B8. --dry-run is not a commit"                      0 "$R" 'git commit --dry-run -m "add a skill"'
case_exit "B9. 'git add' is not a commit"                      0 "$R" 'git add -A'
case_exit "B10. 'git push' is not a commit"                    0 "$R" 'git push origin main'
case_exit "B11. 'git merge' is out of scope by design"         0 "$R" 'git merge feature-branch'
case_exit "B12. a non-git command passes"                      0 "$R" 'ls -la engine/skills'

case_exit "B13. a WELL-ARGUED vendoring-ack passes"            0 "$R" 'git commit -m "add a skill

vendoring-ack: this directory holds throwaway fixtures generated by the packaging test and never leaves the build sandbox"'
ACKLOG="$R/.claude/state/vendoring-acks.log"
[ -s "$ACKLOG" ] && grep -q "vendoring-ack:" "$ACKLOG" \
    && ok "B14. an accepted ack is LOGGED, so a habit is visible as a habit" \
    || bad "B14. an accepted ack is logged" "nothing at $ACKLOG"

# A repository that declares nothing has made no claim, and inventing one for
# it would be a policy expansion rather than a guard. SILENT, not announced —
# an adopter's unrelated repository must not narrate at every commit.
U="$SANDBOX/undeclared"
mkrepo "$U"
mkdir -p "$U/engine/skills/whatever"
printf 'x\n' >"$U/engine/skills/whatever/SKILL.md"
git -C "$U" add -A >/dev/null 2>&1
case_exit "B15. a repository declaring no registry passes"     0 "$U" 'git commit -m "anything"'
run "$U" 'git commit -m "anything"' || true
[ -z "$RUN_ERR" ] && ok "B16. ...and passes SILENTLY, saying nothing at all" \
                  || bad "B16. the undeclared no-op is silent" "said: $RUN_ERR"

# THE OTHER HALF OF THE BOUNDARY RULE, and it belongs here rather than in A
# because its failure looks like a PASS: an entry for `engine/skills/known`
# must not quietly cover `engine/skills/known-extra`, which is a different
# skill and a different vendoring. B5 proves the governed-prefix side; this
# proves the covering-entry side, and a substring match would sink both.
stage_new "engine/skills/known-extra/SKILL.md"
case_exit "B17. an entry does NOT cover a name that merely extends it" 2 "$R" 'git commit -m "a different skill"'

echo
echo "=== C. CONFIGURATION AND FAILURE MODES ==="

broken_case() { # <name> <needle> <registry-line...>
    local name="$1" needle="$2"; shift 2
    local d="$SANDBOX/broken-$PASS-$FAIL"
    mkrepo "$d"
    registry "$d" "$@"
    mkdir -p "$d/engine/skills/x"
    printf 'x\n' >"$d/engine/skills/x/SKILL.md"
    git -C "$d" add -A >/dev/null 2>&1
    local rc=0
    run "$d" 'git commit -m "anything"' || rc=$?
    if [ "$rc" -ne 2 ]; then
        bad "$name" "expected exit 2, got $rc"
        return
    fi
    printf '%s' "$RUN_ERR" | grep -qF "$needle" && ok "$name" \
        || bad "$name" "refused, but did not say \"$needle\""
}

broken_case "C1. a row with nine fields is fail-closed, naming the count" \
    "has 9 tab-separated field(s)" \
    'REDISTRIBUTABLE_PATHS="engine/skills"' \
    "engine/skills/a${TAB}third-party${TAB}MIT${TAB}S${TAB}u${TAB}r${TAB}2026${TAB}certain${TAB}verbatim"

broken_case "C2. an unrecognized origin is fail-closed" \
    "declares origin 'maybe-ours'" \
    'REDISTRIBUTABLE_PATHS="engine/skills"' \
    "engine/skills/a${TAB}maybe-ours${TAB}MIT${TAB}S${TAB}u${TAB}r${TAB}2026${TAB}certain${TAB}verbatim${TAB}docs"

broken_case "C3. no REDISTRIBUTABLE_PATHS means nothing is governed — refused" \
    "declares no REDISTRIBUTABLE_PATHS" \
    "engine/skills/a${TAB}third-party${TAB}MIT${TAB}S${TAB}u${TAB}r${TAB}2026${TAB}certain${TAB}verbatim${TAB}docs"

broken_case "C4. a registry with no entries at all is refused" \
    "carries no entries at all" \
    'REDISTRIBUTABLE_PATHS="engine/skills"'

broken_case "C5. an unknown SETTING is refused rather than ignored" \
    "declares an unknown setting" \
    'REDISTRIBUTABLE_PATHS="engine/skills"' \
    'REDISTRIBUTABLE_PATH="engine/skills"' \
    "engine/skills/a${TAB}third-party${TAB}MIT${TAB}S${TAB}u${TAB}r${TAB}2026${TAB}certain${TAB}verbatim${TAB}docs"

# TWO COPIES ARE BROKEN, NEVER A CHOICE — choosing one quietly is how the wrong
# one stays live. The resolver's rule, exercised through this guard.
D="$SANDBOX/twocopies"
mkrepo "$D"
registry "$D" 'REDISTRIBUTABLE_PATHS="engine/skills"' "$ENTRY_KNOWN"
cp "$D/.richos/vendored-material" "$D/.vendored-material"
mkdir -p "$D/engine/skills/x"; printf 'x\n' >"$D/engine/skills/x/SKILL.md"
git -C "$D" add -A >/dev/null 2>&1
case_exit "C6. BOTH .richos/ and root forms present is BROKEN" 2 "$D" 'git commit -m "anything"'
case_msg  "C7. ...and it says which two files disagree"        "Two declarations are two answers" "$D" 'git commit -m "anything"'

# The LEGACY ROOT FORM alone still works, and that is not a courtesy: an
# adopter who vendored this engine before .richos/ existed keeps enforcement.
L="$SANDBOX/legacyform"
mkrepo "$L"
printf '%s\n' 'REDISTRIBUTABLE_PATHS="engine/skills"' "$ENTRY_KNOWN" >"$L/.vendored-material"
git -C "$L" add -A >/dev/null 2>&1
git -C "$L" commit -qm registry >/dev/null 2>&1
mkdir -p "$L/engine/skills/x"; printf 'x\n' >"$L/engine/skills/x/SKILL.md"
git -C "$L" add -A >/dev/null 2>&1
case_exit "C8. the legacy ROOT declaration still enforces"     2 "$L" 'git commit -m "anything"'

# FAIL-OPEN on a payload this guard cannot parse, matching its Bash-matcher
# siblings. A hook that refuses every command it cannot read is a hook that is
# removed the first time the harness changes shape.
# Seated on a sandbox COPY of the engine's adoption, never the engine itself:
# an unreadable payload appends a line to <entity>/.claude/state/
# unevaluated-payloads.log, and with the engine as the entity that line landed
# in the checkout under test, which the proof gate binds as the input of every
# engine check running beside this one (2026-10-01).
ENT_MALFORMED="$SANDBOX/ent-malformed"
mkdir -p "$ENT_MALFORMED"
cp "$ENGINE_ROOT/orchestration.config" "$ENT_MALFORMED/orchestration.config"
printf 'not json at all' | RICHOS_ENTITY_ROOT="$ENT_MALFORMED" "$HOOK" >/dev/null 2>&1
[ $? -eq 0 ] && ok "C9. an unparseable payload fails OPEN" \
             || bad "C9. an unparseable payload fails OPEN" "expected exit 0"

echo
echo "=== D. THE SHIPPED REGISTRY ITSELF ==="

# A MUTANT IS NOT THE SHIPPED TREE. This section asserts facts about the real
# repository's own registry, which lives ABOVE the engine root and is therefore
# absent from a mutation sandbox. Running it there would go red for a reason
# that has nothing to do with the mutated property, and a harness whose red is
# unrelated proves nothing.
if [ -n "${RICHOS_MUTATION_INNER:-}" ]; then
    echo "  (skipped inside a mutation sandbox — this section is about the SHIPPED tree)"
else
# shellcheck source=../lib/vendored-material.sh
. "$LIB"
VMRC=0
vm_load_upward "$ENGINE_ROOT" || VMRC=$?
if [ "$VMRC" -ne 0 ]; then
    bad "D0. the shipped registry loads" "vm_load_upward returned $VMRC: ${VM_BROKEN_REASON:-not found}"
else
    ok "D0. the shipped registry parses ($VM_ENTRY_COUNT entries)"
    REPO_ROOT="$VM_ROOT"

    MISSING=""
    while IFS=$'\t' read -r p _; do
        [ -n "$p" ] || continue
        [ -e "$REPO_ROOT/$p" ] || MISSING="$MISSING $p"
    done <<EOF
$VM_COVERED
EOF
    [ -z "$MISSING" ] && ok "D1. every path the registry names exists on disk" \
        || bad "D1. every path the registry names exists on disk" "absent:$MISSING"

    # THE OTHER DIRECTION, and it is the one that matters: a file under a
    # governed path with no entry is exactly what this whole mechanism exists
    # to make impossible, and today's tree must already satisfy it.
    UNCOVERED=""
    N=0
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        vm_governed "$f" || continue
        N=$((N + 1))
        vm_covering "$f" || UNCOVERED="$UNCOVERED $f"
    done < <(git -C "$REPO_ROOT" ls-files 2>/dev/null)
    if [ "$N" -eq 0 ]; then
        bad "D2. every governed file is covered" "found ZERO governed files — the check would pass by never running"
    elif [ -n "$UNCOVERED" ]; then
        bad "D2. every governed file is covered" "uncovered:$UNCOVERED"
    else
        ok "D2. all $N tracked files under a governed path are covered"
    fi

    # THE 2026-09-04 AUDIT'S FINDING, pinned. Fifteen third-party skills; if a
    # future edit quietly reclassified one as ours, this is where it shows.
    TP=0
    while IFS=$'\t' read -r p origin; do
        case "$p" in richos/engine/skills/*) [ "$origin" = "third-party" ] && TP=$((TP + 1)) ;; esac
    done <<EOF
$VM_COVERED
EOF
    [ "$TP" -eq 15 ] && ok "D3. all fifteen third-party skills are recorded" \
        || bad "D3. all fifteen third-party skills are recorded" "found $TP"

    for want in richos/engine/tools/gpt-exporter \
                richos/app/ui/fonts/Inter-Variable.woff2 \
                richos/app/ui/fonts/Newsreader-Regular.woff2 \
                richos/app/ui/fonts/NotoSansMath-subset.woff2 \
                richos/app/ui/fonts/NotoSansSymbols-subset.woff2; do
        vm_is_third_party "$want" \
            && ok "D4. recorded as third-party: $want" \
            || bad "D4. recorded as third-party: $want" "not covered by a third-party entry"
    done

    # CONFIDENCE IS NEVER ROUNDED UP. The audit qualified exactly two verdicts,
    # and a registry that quietly promoted either to `certain` would be
    # asserting something nobody established.
    REG="$VM_REGISTRY"
    grep -q "^richos/engine/skills/ui-ux-design${TAB}.*${TAB}medium-high${TAB}" "$REG" \
        && ok "D5. ui-ux-design keeps its medium-high confidence" \
        || bad "D5. ui-ux-design keeps its medium-high confidence" "the one row not closed by a byte comparison was rounded up"
    grep -q "^richos/engine/skills/marketing-strategy-pmm${TAB}.*${TAB}high${TAB}" "$REG" \
        && ok "D6. marketing-strategy-pmm keeps its 'high', not 'certain'" \
        || bad "D6. marketing-strategy-pmm keeps its 'high', not 'certain'"

    # THE FILE THE DIALECT GUARD DAMAGED. Its entry is what makes that guard
    # leave it alone, so its presence is load-bearing in a second mechanism.
    vm_is_third_party "richos/engine/skills/copywriting/references/natural-transitions.md" \
        && ok "D7. the file damaged on 2026-08-30 is recorded as third-party" \
        || bad "D7. the file damaged on 2026-08-30 is recorded as third-party"
fi
fi

echo
echo "=== E. REGISTRATION ==="

grep -q '\. "\$_RR_LIB"' "$HOOK" \
    && ok "E1. sources the root-resolution contract" \
    || bad "E1. sources the root-resolution contract"

hook_enforced_on_surface "$ENGINE_ROOT/hooks/hooks.json" guard-vendoring-commits.sh \
    && ok "E2. registered on the plugin surface (hooks/hooks.json)" \
    || bad "E2. registered on the plugin surface (hooks/hooks.json)"

hook_enforced_on_surface "$ENGINE_ROOT/.claude/settings.local.json" guard-vendoring-commits.sh \
    && ok "E3. registered on the seated surface (.claude/settings.local.json)" \
    || bad "E3. registered on the seated surface (.claude/settings.local.json)"

grep -q '^guard-vendoring-commits\.sh|PreToolUse$' "$ENGINE_ROOT/scripts/hooks/contract-integrity-probe.sh" \
    && ok "E4. declared in the probe's BR_EXPECTED specification" \
    || bad "E4. declared in the probe's BR_EXPECTED specification" "BR2 checks registration in BOTH directions; an undeclared wired guard is a probe failure"

# INVERTED 2026-09-14 WITH LAYER R: the layer derives the hooks it walks from
# hooks/hooks.json, so registration IS membership. What is still assertable is
# that nobody exempted this hook as rootless, which would silently drop its
# bootstrap out of R3's comparison.
sed -n '/^    R_ROOTLESS_HOOKS="/,/"$/p' "$ENGINE_ROOT/scripts/hooks/contract-integrity-probe.sh" \
    | grep -q '\bguard-vendoring-commits\b' \
    && bad "E5. not exempted from Layer R (R_ROOTLESS_HOOKS)" "it IS exempted, so Layer R skips it and its root bootstrap goes unchecked" \
    || ok "E5. not exempted from Layer R, so its bootstrap is derived from the registration and checked"

grep -q 'vendored-material\.sh' "$ENGINE_ROOT/scripts/hooks/install.sh" \
    && ok "E6. the predicate is sidecar-hashed by install.sh" \
    || bad "E6. the predicate is sidecar-hashed by install.sh" "hashing the guard and not the thing that decides checks the lock and ignores the key"

grep -q 'vendored-material' "$ENGINE_ROOT/scripts/lib/declaration-path.sh" \
    && ok "E7. the declaration stem is CLASSIFIED by the resolver" \
    || bad "E7. the declaration stem is CLASSIFIED by the resolver" "publication-completeness.py fails on a declaration in neither list"

# ONE PARSER. A second reader of the registry anywhere in the engine is the
# defect scripts/lib/registered-hooks.sh exists to describe, one domain over.
# The predicate is who RESOLVES the declaration — naming REDISTRIBUTABLE_PATHS
# in a refusal message or a test is a mention, and mentions are not parsers.
# The verification-inputs contract (lib/verification-dependencies.json) and its
# Python suite name the config key as DATA about the guard's inputs; they are
# mentions too, and E8 went red on them from the moment they named it.
DUP="$(grep -rl 'VENDORING_DECLARATION' "$ENGINE_ROOT/scripts" 2>/dev/null \
        | grep -v 'vendored-material' \
        | grep -vE '\.(test|mutation)\.(sh|py)$' \
        | grep -vxF "$ENGINE_ROOT/scripts/lib/verification-dependencies.json" || true)"
[ -z "$DUP" ] && ok "E8. the registry has exactly one parser" \
              || bad "E8. the registry has exactly one parser" "also resolved by: $DUP"

echo
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -x "$SCRIPT_DIR/guard-vendoring-commits.mutation.sh" ]; then
    # THE HARNESS RUNS FROM THE SUITE IT MUTATES, so the runner that discovers
    # *.test.sh runs it too. A harness nobody runs proves nothing about anything.
    echo "=== running the mutation harness ==="
    "$SCRIPT_DIR/guard-vendoring-commits.mutation.sh" || FAIL=$((FAIL + 1))
    echo
fi

if [ "$FAIL" -eq 0 ]; then
    printf '\n  %d/%d cases passed\n' "$PASS" "$((PASS + FAIL))"
    exit 0
fi
printf '\n  %d/%d cases passed, %d FAILED\n' "$PASS" "$((PASS + FAIL))" "$FAIL"
exit 1
