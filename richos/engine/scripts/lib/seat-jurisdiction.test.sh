#!/usr/bin/env bash
#
# seat-jurisdiction.test.sh — THE TEST THAT OUTLIVES THE FIX.
#
# ===========================================================================
# WHAT THIS SUITE IS FOR
# ===========================================================================
# On 2026-08-30 it was established that every guard decides WHETHER TO RUN from
# the session's seat and WHAT TO INSPECT from the command's repository, and that
# in a by-reference installation those are never the same repository. Seven of
# the twenty seat-resolving guards derived their inspection target with no
# reference to their seat at all, and when the two diverged the guard exited 0 —
# the same byte as a pass.
#
# Fixing those seven is worth little on its own. The eighth is written next
# week, by someone who never read this file, and it will reintroduce the defect
# unless something mechanical refuses. So the deliverable is not the fix, it is
# this suite:
#
#   STRUCTURAL (cases 1.x)  Derives the guard set from hooks/hooks.json and
#                           FAILS if any registered guard can let its seat
#                           decision and its inspection target diverge. A new
#                           guard with the old shape turns this red on the day
#                           it is written.
#
#   BEHAVIORAL (cases 2.x) Drives the real guards against real sandbox repos
#                           and asserts that divergence and stand-down are LOUD.
#                           Silence and success must not look the same.
#
#   NEGATIVE CONTROLS       Both halves are mutated to prove they can fail. Two
#   (cases 3.x)             mechanisms on 2026-08-30 reported green because they
#                           read nothing; a suite that cannot be shown to go red
#                           is one of them waiting to happen.
#
# ===========================================================================
# WHY THE STRUCTURAL CHECK IS DERIVED, NEVER TYPED
# ===========================================================================
# A typed list of 14 over a registration of 15 is the drift that opened this
# whole sequence. The guard inventory here comes from the same library the
# session banner and the integrity probe use — scripts/lib/registered-hooks.sh,
# reading hooks/hooks.json, the file that actually determines what the host
# loads. If this suite ever scans zero guards it FAILS rather than passing
# vacuously (case 1a).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOKS_DIR="$ENGINE_ROOT/scripts/hooks"

PASS=0
FAIL=0
ok()  { PASS=$((PASS + 1)); echo "  PASS  $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL  $1"; [ -n "${2:-}" ] && echo "        $2"; }

SANDBOX="$(mktemp -d -t seatjur.XXXXXX)"
cleanup() { rm -rf "$SANDBOX"; }
trap cleanup EXIT

echo "seat-jurisdiction: engine at $ENGINE_ROOT"

# ===========================================================================
# THE CLASSIFIER — one definition of "diverges", used by the real check and by
# its own negative control, so the control cannot pass against a weaker rule.
# ===========================================================================
#
#   seat    the guard asks resolve_entity_root which repository governs it
#   target  the guard derives an artifact from the PAYLOAD (a tool_input path,
#           or a repo root off REPO_HINT / the payload cwd) — i.e. from
#           something the seat has no say over
#   bound   the guard routes that artifact through the jurisdiction contract
#
# A guard with a seat AND an independently-derived target AND no jurisdiction
# call can silently judge, or silently decline to judge, an artifact in a
# repository it does not govern. That is the defect, and it is decidable from
# the source text.
classify() { # <file> -> prints: seat target bound
    local f="$1" seat=no target=no bound=no
    grep -q 'resolve_entity_root' "$f" 2>/dev/null && seat=yes
    grep -Eq 'file_path|notebook_path|ct_repo_root|pb_repo_root|REPO_HINT|PAYLOAD_CWD' "$f" 2>/dev/null && target=yes
    grep -q 'richos_assert_jurisdiction\|richos_in_jurisdiction' "$f" 2>/dev/null && bound=yes
    printf '%s %s %s' "$seat" "$target" "$bound"
}

diverges() { # <file> -> rc 0 if this guard CAN diverge
    local c; c="$(classify "$1")"
    [ "$c" = "yes yes no" ]
}

# ===========================================================================
# 1. STRUCTURAL — derived across every registered guard
# ===========================================================================
echo
echo "1. structural: seat and inspection target, across the registered set"

REG_LIB="$ENGINE_ROOT/scripts/lib/registered-hooks.sh"
if [ ! -f "$REG_LIB" ]; then
    bad "1a  guard inventory" "scripts/lib/registered-hooks.sh missing — the inventory cannot be derived, and this suite will not fall back to a typed list"
else
    # shellcheck source=./registered-hooks.sh
    . "$REG_LIB"
    ROWS="$(registered_hook_scripts "$ENGINE_ROOT/hooks/hooks.json")" || ROWS=""

    SCANNED=0
    WITH_SEAT=0
    DIVERGENT=""
    while IFS= read -r g; do
        [ -n "$g" ] || continue
        f="$HOOKS_DIR/$g"
        [ -f "$f" ] || continue
        SCANNED=$((SCANNED + 1))
        case "$(classify "$f")" in yes\ *) WITH_SEAT=$((WITH_SEAT + 1)) ;; esac
        diverges "$f" && DIVERGENT="$DIVERGENT $g"
    done <<EOF
$ROWS
EOF

    # --- THE NEGATIVE CONTROL, and it comes FIRST -------------------------
    # A scan that examined nothing reports no divergence and looks identical to
    # a clean engine. Two mechanisms were caught doing exactly that on the day
    # this was written, so the count is asserted before the result it produces.
    if [ "$SCANNED" -gt 0 ]; then
        ok "1a  NEGATIVE CONTROL: the scan examined $SCANNED registered guards (a scan of zero cannot report clean)"
    else
        bad "1a  NEGATIVE CONTROL" "the scan examined ZERO guards — every result below would be vacuous"
    fi

    if [ "$WITH_SEAT" -gt 0 ]; then
        ok "1b  $WITH_SEAT of them resolve a seat, so the property under test is actually present"
    else
        bad "1b  seat-resolving guards" "none found — hooks.json or the classifier is wrong"
    fi

    if [ -z "$DIVERGENT" ]; then
        ok "1c  NO registered guard can let its seat decision and its inspection target diverge"
    else
        bad "1c  seat/target divergence" "these guards resolve a seat, derive an inspection target from the payload, and never compare the two — so an artifact in another repository is judged, or silently not judged, with no announcement:$DIVERGENT. Route the target through richos_assert_jurisdiction (scripts/lib/seat-jurisdiction.sh)."
    fi
fi

# ===========================================================================
# 2. BEHAVIORAL — divergence and stand-down are LOUD, against real guards
# ===========================================================================
echo
echo "2. behavioral: silence and success must not look the same"

# Two sandbox repos. ADOPTED carries orchestration.config and protects src/;
# STRANGER carries nothing. Both are real git repositories, because every path
# the guards take asks git what repository a path belongs to.
ADOPTED="$SANDBOX/adopted"
STRANGER="$SANDBOX/stranger"
#
# core.hooksPath is neutralized and --no-verify is passed because this machine
# carries a GLOBAL pre-commit identity guard. Without both, the sandbox commit
# fails, the repo has no commits, `git worktree add` checks out an empty tree,
# and case 2f fails for a reason that has nothing to do with jurisdiction. It
# did exactly that on first run — a test whose SETUP fails silently reports on
# something other than its subject.
for r in "$ADOPTED" "$STRANGER"; do
    mkdir -p "$r/src"
    git -C "$r" init -q 2>/dev/null
    git -C "$r" config user.email t@t.invalid
    git -C "$r" config user.name t
    git -C "$r" config core.hooksPath /dev/null
    : >"$r/src/keep.txt"
    git -C "$r" add -A 2>/dev/null
    git -C "$r" commit -qm init --no-verify 2>/dev/null
    if [ -z "$(git -C "$r" rev-parse --verify HEAD 2>/dev/null)" ]; then
        bad "0.  SANDBOX SETUP" "could not create a commit in $r — every case below would test the wrong thing"
    fi
done
printf 'PROTECTED_PATHS="src"\n' >"$ADOPTED/orchestration.config"

GUARD="$HOOKS_DIR/guard-main-checkout-writes.sh"

# run_guard <seat> <target> -> sets RC and OUT
run_guard() {
    local seat="$1" target="$2" tmpout
    tmpout="$(mktemp)"
    (
        unset RICHOS_ENTITY_ROOT CLAUDE_PLUGIN_ROOT
        export CLAUDE_PROJECT_DIR="$seat"
        export TMPDIR="$SANDBOX/notices"
        mkdir -p "$TMPDIR"
        cd "$seat" 2>/dev/null || cd /
        printf '{"tool_name":"Write","cwd":"%s","tool_input":{"file_path":"%s"}}' "$seat" "$target" \
            | bash "$GUARD"
    ) >"$tmpout" 2>&1
    RC=$?
    OUT="$(cat "$tmpout")"
    rm -f "$tmpout"
}

# --- POSITIVE CONTROL FIRST. If an in-jurisdiction protected write does not
# block, every "allowed" result below is meaningless — the guard would be inert
# for reasons that have nothing to do with jurisdiction.
rm -rf "$SANDBOX/notices"
run_guard "$ADOPTED" "$ADOPTED/src/f.txt"
if [ "$RC" = 2 ]; then
    ok "2a  POSITIVE CONTROL: an in-jurisdiction write to a protected tree still BLOCKS (rc 2)"
else
    bad "2a  POSITIVE CONTROL" "expected rc 2, got rc $RC — the guard is inert, so nothing below proves anything: $OUT"
fi

# --- the divergence itself: seated in one repo, handed a file in another.
rm -rf "$SANDBOX/notices"
run_guard "$ADOPTED" "$STRANGER/src/f.txt"
if [ "$RC" = 0 ] && printf '%s' "$OUT" | grep -q 'OUT OF JURISDICTION'; then
    ok "2b  a target in ANOTHER repository is announced, not silently allowed"
elif [ "$RC" = 0 ]; then
    bad "2b  out-of-jurisdiction announcement" "the guard exited 0 with NO announcement — divergence and success are the same byte again: [${OUT}]"
else
    bad "2b  out-of-jurisdiction announcement" "unexpected rc $RC: $OUT"
fi

# --- the announcement must NAME both repositories. A banner that says only
# "not enforced" leaves the reader unable to tell WHICH tree is unguarded,
# which is the failure mode of the engine-self banner this work also fixed.
if printf '%s' "$OUT" | grep -q "$STRANGER" && printf '%s' "$OUT" | grep -q "$ADOPTED"; then
    ok "2c  the announcement names BOTH the seat and the artifact's own repository"
else
    bad "2c  announcement names both repos" "missing one of them: $OUT"
fi

# --- stand-down must be loud at the moment of the decision.
rm -rf "$SANDBOX/notices"
run_guard "$STRANGER" "$STRANGER/src/f.txt"
if [ "$RC" = 0 ] && printf '%s' "$OUT" | grep -q 'STOOD DOWN'; then
    ok "2d  an unadopted repository gets a LOUD stand-down, naming the repo, at the moment the guard declines"
elif [ "$RC" = 0 ]; then
    bad "2d  loud stand-down" "the guard exited 0 in silence — an unadopted repository cannot tell it is unguarded: [${OUT}]"
else
    bad "2d  loud stand-down" "unexpected rc $RC: $OUT"
fi

# --- once, not every call. A notice on every tool call is filtered out by the
# reader within minutes, and a signal nobody reads is the silent skip again.
rm -rf "$SANDBOX/notices"
run_guard "$STRANGER" "$STRANGER/src/a.txt"; FIRST="$OUT"
run_guard "$STRANGER" "$STRANGER/src/b.txt"; SECOND="$OUT"
if [ -n "$FIRST" ] && [ -z "$SECOND" ]; then
    ok "2e  the stand-down is said ONCE per repository per session, not on every call"
else
    bad "2e  dedupe" "first=[${FIRST:0:60}] second=[${SECOND:0:60}] — expected a notice then silence"
fi

# --- a linked worktree of the seat IS the seat. If this were wrong the notice
# would fire on every legitimate isolated-worktree edit and be tuned out.
WT="$SANDBOX/adopted-wt"
if git -C "$ADOPTED" worktree add -q -b jur-test "$WT" 2>/dev/null; then
    rm -rf "$SANDBOX/notices"
    run_guard "$ADOPTED" "$WT/src/f.txt"
    if ! printf '%s' "$OUT" | grep -q 'OUT OF JURISDICTION'; then
        ok "2f  a linked worktree of the seat is IN jurisdiction (no false notice on isolated work)"
    else
        bad "2f  worktree normalization" "a worktree of the seat was called out-of-jurisdiction: $OUT"
    fi
    git -C "$ADOPTED" worktree remove --force "$WT" 2>/dev/null
else
    ok "2f  SKIPPED: git worktree unavailable in this sandbox"
fi

# --- 2g/2h: THE SEAT ARM, IN THE SPELLING IT ACTUALLY ARRIVES IN.
#
# Every case above seats the guard in a git repository, so richos_repo_of
# answers first and the seat arm of richos_governing_root never runs. The seat
# arm is what serves an adopted directory that is NOT a git checkout — which is
# also, exactly, what every contract-integrity sandbox is.
#
# It returned the seat verbatim while richos_repo_of returned it physicalized,
# so the same function answered in two spellings. On macOS TMPDIR lives under
# /var, a symlink to /private/var, so the guard compared a logical governing
# root against a physicalized seat, concluded a FOREIGN repository governed the
# file, and then matched PROTECTED_PATHS with the wrong spelling: it ran every
# branch, printed nothing, blocked nothing, and exited 0. Contract-integrity
# Layer D caught it only once seat-jurisdiction.sh was added to the sandbox file
# lists — before that the guard was missing its library and exited 2, which is
# the number Layer D wanted, for the opposite reason.
#
# 2g is the POSITIVE control (blocks), 2h is its companion (an unprotected tree
# in the same seat is still allowed) — a guard that blocked everything would
# satisfy 2g alone.
NOGIT="$SANDBOX/adopted-nogit"
mkdir -p "$NOGIT/src" "$NOGIT/docs"
printf 'PROTECTED_PATHS="src"\n' >"$NOGIT/orchestration.config"
rm -rf "$SANDBOX/notices"
run_guard "$NOGIT" "$NOGIT/src/f.txt"
if [ "$RC" = 2 ]; then
    ok "2g  a non-git adopted seat (the seat arm) still BLOCKS a protected write — governing root and seat compare in one spelling"
else
    bad "2g  seat-arm spelling" "expected rc 2, got rc $RC — the guard went inert on a seat it governs: [${OUT}]"
fi

rm -rf "$SANDBOX/notices"
run_guard "$NOGIT" "$NOGIT/docs/f.txt"
if [ "$RC" = 0 ]; then
    ok "2h  the same non-git seat still ALLOWS an unprotected tree (2g is not a block-everything guard)"
else
    bad "2h  seat-arm over-blocking" "expected rc 0, got rc $RC — the guard blocks outside PROTECTED_PATHS: [${OUT}]"
fi

# ===========================================================================
# 4. THE BANNER TELLS THE TRUTH ABOUT WHAT THE GUARD DID
# ===========================================================================
# rc 1 from richos_assert_jurisdiction does NOT mean the guard stood down. Ten
# call sites route through it and NINE proceed to judge the artifact anyway, out
# of the target repository's own declaration. The banner said "the guard did NOT
# judge it / THIS IS NOT A PASS" to all ten, and on 2026-09-04 an engineer read
# it, reported that dialect-guard.sh does not judge files in cross-repository
# worktrees, and that reached the CEO as a finding. A second engineer replayed
# the configuration and measured exit 2 with a full refusal. The guard worked;
# the message lied, and it cost a day.
echo
echo "4. the banner: a guard that judged must not be reported as having declined"

# --- 4a STRUCTURAL: every call site says which of the two happened. The
# default is the conservative wording, so a forgotten parameter under-claims
# rather than manufacturing a pass -- but an under-claim is still a wrong
# sentence, and this is the check that stops one being added.
MISSING=""
SITES=0
while IFS= read -r hit; do
    f="${hit%%:*}"
    line="${hit#*:}"
    case "$line" in
        *richos_assert_jurisdiction*) : ;;
        *) continue ;;
    esac
    SITES=$((SITES + 1))
    # The disposition may sit on the call line or on its continuation, so the
    # two lines following the match are read with it.
    ctx="$(grep -A2 -F "$line" "$f" 2>/dev/null | head -3)"
    case "$ctx" in
        *'"proceeds"'*|*'"declines"'*) : ;;
        *) MISSING="$MISSING $(basename "$f")" ;;
    esac
done <<EOF
$(grep -rn 'richos_assert_jurisdiction "' "$HOOKS_DIR" --include='*.sh' 2>/dev/null | grep -v '\.test\.sh:' | grep -v '\.mutation\.sh:' | sed 's/^\([^:]*\):[0-9]*:/\1:/')
EOF

if [ "$SITES" -eq 0 ]; then
    bad "4a  NEGATIVE CONTROL" "the scan found ZERO call sites — the check below would be vacuous"
elif [ -z "$MISSING" ]; then
    ok "4a  all $SITES call sites declare a disposition, so none of them can be described wrongly"
else
    bad "4a  implicit disposition" "these call sites leave it to the default, so the banner may describe them wrongly:$MISSING"
fi

# --- 4b/4c BEHAVIORAL, against the two real wordings.
sj_banner() { # <disposition> -> OUT
    (
        unset RICHOS_ENTITY_ROOT CLAUDE_PLUGIN_ROOT
        export TMPDIR="$SANDBOX/notices-$1"
        mkdir -p "$TMPDIR"
        # shellcheck source=./seat-jurisdiction.sh
        . "$ENGINE_ROOT/scripts/lib/seat-jurisdiction.sh"
        richos_assert_jurisdiction "scripts/hooks/probe.sh" "$ADOPTED" "$STRANGER/src/f.txt" "file" "$1"
    ) 2>&1
}

OUT="$(sj_banner proceeds)"
if printf '%s' "$OUT" | grep -q 'JUDGED ANYWAY' \
   && ! printf '%s' "$OUT" | grep -q 'THIS IS NOT A PASS' \
   && ! printf '%s' "$OUT" | grep -q 'did NOT judge it'; then
    ok "4b  a guard that PROCEEDS is reported as having judged it, not as having declined"
else
    bad "4b  proceeds wording" "the banner still says the guard did not judge it: $OUT"
fi

OUT="$(sj_banner declines)"
if printf '%s' "$OUT" | grep -q 'THIS IS NOT A PASS' \
   && printf '%s' "$OUT" | grep -q 'did NOT judge it'; then
    ok "4c  a guard that DECLINES still gets the wording that was always right"
else
    bad "4c  declines wording" "the stand-down wording was lost: $OUT"
fi

# --- 4d the default is the CONSERVATIVE one. An over-claim ("this was judged")
# is a false pass; an under-claim prompts a check. Getting this backwards is the
# one way this parameter could make things worse than the hardcoded banner.
OUT="$(sj_banner "")"
if printf '%s' "$OUT" | grep -q 'THIS IS NOT A PASS'; then
    ok "4d  the DEFAULT is the conservative wording, so a forgotten parameter under-claims"
else
    bad "4d  default disposition" "an omitted disposition claims the artifact was judged: $OUT"
fi

# ===========================================================================
# 3. NEGATIVE CONTROLS — prove each half can fail
# ===========================================================================
echo
echo "3. negative controls: break it on purpose"

# --- 3a: the structural check must go RED when a guard's jurisdiction call is
# removed. Copy a real wired guard, strip the call, and re-classify it. If the
# classifier still says "fine", case 1c is decorative.
MUT="$SANDBOX/mutant.sh"
sed '/richos_assert_jurisdiction/d; /richos_in_jurisdiction/d' \
    "$HOOKS_DIR/guard-main-checkout-writes.sh" >"$MUT" 2>/dev/null
if diverges "$MUT"; then
    ok "3a  NEGATIVE CONTROL: stripping the jurisdiction call from a real guard makes case 1c report it as divergent"
else
    bad "3a  NEGATIVE CONTROL" "a guard with its jurisdiction call removed was still classified clean — case 1c cannot detect the defect it exists for, so its green means nothing. classify=[$(classify "$MUT")]"
fi

# --- 3b: and the wired original must classify clean, or 3a passed for the
# wrong reason (e.g. the classifier calling everything divergent).
if ! diverges "$HOOKS_DIR/guard-main-checkout-writes.sh"; then
    ok "3b  NEGATIVE CONTROL: the unmutated guard classifies clean, so 3a discriminates rather than always firing"
else
    bad "3b  NEGATIVE CONTROL" "the real guard classifies as divergent — the classifier fires on everything and 3a proved nothing"
fi

# --- 3c: the behavioral half must go RED without the library. With
# seat-jurisdiction.sh unavailable a guard must refuse loudly, never carry on
# quietly — a missing predicate is the BROKEN case, not the not-applicable one.
BROKEN_ENGINE="$SANDBOX/broken-engine"
mkdir -p "$BROKEN_ENGINE/scripts"
cp -R "$ENGINE_ROOT/scripts/hooks" "$BROKEN_ENGINE/scripts/hooks" 2>/dev/null
cp -R "$ENGINE_ROOT/scripts/lib" "$BROKEN_ENGINE/scripts/lib" 2>/dev/null
rm -f "$BROKEN_ENGINE/scripts/lib/seat-jurisdiction.sh"
BROKEN_OUT="$(
    unset RICHOS_ENTITY_ROOT CLAUDE_PLUGIN_ROOT
    export CLAUDE_PROJECT_DIR="$ADOPTED"
    cd "$ADOPTED" || exit
    printf '{"tool_name":"Write","cwd":"%s","tool_input":{"file_path":"%s"}}' "$ADOPTED" "$ADOPTED/src/f.txt" \
        | bash "$BROKEN_ENGINE/scripts/hooks/guard-main-checkout-writes.sh" 2>&1
)"
BROKEN_RC=$?
if [ "$BROKEN_RC" = 2 ] && printf '%s' "$BROKEN_OUT" | grep -q 'BROKEN INSTALL'; then
    ok "3c  NEGATIVE CONTROL: with seat-jurisdiction.sh deleted the guard refuses LOUDLY (rc 2), it does not carry on quietly"
else
    bad "3c  NEGATIVE CONTROL" "expected rc 2 + 'BROKEN INSTALL', got rc $BROKEN_RC: ${BROKEN_OUT:0:300}"
fi

echo
TOTAL=$((PASS + FAIL))
echo "seat-jurisdiction: $PASS/$TOTAL cases pass"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
