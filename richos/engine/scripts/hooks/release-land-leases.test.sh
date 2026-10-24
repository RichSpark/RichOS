#!/usr/bin/env bash
#
# release-land-leases.test.sh: the operator fence's turn-end release
# (scripts/hooks/release-land-leases.sh; spec r3 e6, Frank F3 and G5 point 3).
#
#   T1  this session's lease on a repository at rest is released at its turn end,
#       silently
#   T2  a commit not pushed keeps the lease, and one systemMessage says so
#   T3  dirty paths keep it, and the message names them (G5 point 3)
#   T4  a merge in progress keeps it, and the message says so
#   T5  another session's lease is never touched, and nothing is said
#   T6  WITH THE SWITCH OFF a lease file is ignored: no output, nothing touched
#   T7  no leases at all: no output; the hook always exits 0 and its stdout, when
#       there is any, is one JSON object
#   T8  a lease whose land command is still running (the session's background
#       script took it and has not merged yet) is kept at the turn end, however
#       the repository looks, and the message says so; the turn end after that
#       command has ended releases it (2026-10-11, twice in richos)
#
# Usage: scripts/hooks/release-land-leases.test.sh

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOK="$SCRIPT_DIR/release-land-leases.sh"
# shellcheck source=../lib/operator-fences-fixture.sh
. "$ENGINE_ROOT/scripts/lib/operator-fences-fixture.sh"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2" | head -8; FAIL=$((FAIL + 1)); return 0; }
lease_count() { ls "$OFX/locks/"*.lease 2>/dev/null | grep -c . ; }
turn_end() { # <session>: run the Stop hook inside that session; OFX_OUT = its stdout
    ofx_in "$1" "printf '{}' | bash '$HOOK' 2>/dev/null"
}
is_json_or_empty() { # the notice is for the lead: Stop additionalContext (R6)
    [ -z "$1" ] && return 0
    printf '%s' "$1" | python3 -c 'import json,sys; v=json.load(sys.stdin); sys.exit(0 if isinstance(v, dict) and (v.get("hookSpecificOutput") or {}).get("additionalContext") and not v.get("systemMessage") else 1)'
}
turn_end_in() { # <session> <session id>: the Stop hook with a session id, so its notice ledger is in use
    ofx_in "$1" "printf '{\"session_id\":\"$2\"}' | bash '$HOOK' 2>/dev/null"
}

ofx_init || exit 1
trap ofx_cleanup EXIT
ofx_repo r; R="$OFX_R"
ofx_install "$R" >/dev/null 2>&1 && ofx_on "$R" >/dev/null 2>&1
ofx_session A; ofx_session B

echo "=== release-land-leases: the turn-end release ==="
turn_end A; rc=$?
{ [ $rc = 0 ] && [ -z "$OFX_OUT" ]; } && ok "T7 no leases: silent, exit 0" || bad "T7 no leases: silent, exit 0" "rc=$rc $OFX_OUT"

ofx_in A "$(ofx_lease acquire --repo "$R")"
turn_end A; rc=$?
{ [ $rc = 0 ] && [ -z "$OFX_OUT" ] && [ "$(lease_count)" = 0 ]; } \
    && ok "T1 at rest: this session's lease is released at its turn end, silently" \
    || bad "T1 at rest: this session's lease is released at its turn end, silently" "rc=$rc leases=$(lease_count) $OFX_OUT"

# T8: the land runs in the background of the session (a script that took the
# lease and has not reached its merge yet) while the session's turn ends.
ofx_in A "( $(ofx_lease acquire --repo "$R") >/dev/null; : > '$OFX/t8-taken'; while [ ! -f '$OFX/t8-go' ]; do sleep 0.05; done ) & while [ ! -f '$OFX/t8-taken' ]; do sleep 0.05; done; printf '{}' | bash '$HOOK' > '$OFX/t8-said' 2>/dev/null; ls '$OFX/locks/' | grep -c '[.]lease\$' > '$OFX/t8-count'; : > '$OFX/t8-go'; wait"
{ [ "$(cat "$OFX/t8-count" 2>/dev/null)" = 1 ] && grep -q "still running" "$OFX/t8-said" && is_json_or_empty "$(cat "$OFX/t8-said")"; } \
    && ok "T8 a lease whose land command still runs is kept at the turn end, and the message says so" \
    || bad "T8 a lease whose land command still runs is kept at the turn end, and the message says so" \
           "count=$(cat "$OFX/t8-count" 2>/dev/null) said=$(cat "$OFX/t8-said" 2>/dev/null) $OFX_OUT"
turn_end A
[ "$(lease_count)" = 0 ] && ok "T8 the turn end after that land has ended releases the lease" \
    || bad "T8 the turn end after that land has ended releases the lease" "$OFX_OUT"

ofx_in A "$(ofx_lease acquire --repo "$R") && cd '$R' && printf 'n\n' > n.txt && git add n.txt && git commit -q -m n"
turn_end A; rc=$?
{ [ $rc = 0 ] && [ "$(lease_count)" = 1 ] && printf '%s' "$OFX_OUT" | grep -q "1 commit not pushed" \
  && printf '%s' "$OFX_OUT" | grep -q "still holding the lock" && is_json_or_empty "$OFX_OUT"; } \
    && ok "T2 a commit not pushed keeps the lease and says so, once, as JSON" \
    || bad "T2 a commit not pushed keeps the lease and says so, once, as JSON" "rc=$rc $OFX_OUT"

printf 'dirty\n' >> "$R/f.txt"
ofx_in A "git -C '$R' push -q origin main"
turn_end A
{ [ "$(lease_count)" = 1 ] && printf '%s' "$OFX_OUT" | grep -q "uncommitted changes (f.txt)"; } \
    && ok "T3 a dirty path keeps the lease and the message names it" \
    || bad "T3 a dirty path keeps the lease and the message names it" "$OFX_OUT"
# T9 (orchestration reliability R6): the same lease kept for the same reason is said
# once to the lead, not at every turn end; the next, older turn end says nothing.
turn_end_in A t9-session-0001; first="$OFX_OUT"
turn_end_in A t9-session-0001; second="$OFX_OUT"
{ printf '%s' "$first" | grep -q "still holding the lock" && is_json_or_empty "$first" && [ -z "$second" ] \
  && [ "$(lease_count)" = 1 ]; } \
    && ok "T9 an unchanged held lease is told to the lead once, not at every turn end" \
    || bad "T9 an unchanged held lease is told to the lead once, not at every turn end" "first=$first second=$second"
git -C "$R" checkout -- f.txt

( cd "$R" && git -c core.hooksPath=/dev/null switch -q -c side && printf 'side\n' > f.txt && git -c core.hooksPath=/dev/null commit -q -am side \
  && git -c core.hooksPath=/dev/null switch -q main ) >/dev/null 2>&1
ofx_in A "cd '$R' && printf 'main\n' > f.txt && git commit -q -am m && git push -q origin main && git merge -q side -m x"
if [ -f "$R/.git/MERGE_HEAD" ]; then
    turn_end A
    { [ "$(lease_count)" = 1 ] && printf '%s' "$OFX_OUT" | grep -q "merge in progress"; } \
        && ok "T4 a merge in progress keeps the lease and says so" || bad "T4 a merge in progress keeps the lease and says so" "$OFX_OUT"
    ofx_in A "git -C '$R' merge --abort"
else
    bad "T4 the fixture's merge did not conflict, so the case proves nothing" "$OFX_OUT"
fi

turn_end B; rc=$?
{ [ $rc = 0 ] && [ -z "$OFX_OUT" ] && [ "$(lease_count)" = 1 ]; } \
    && ok "T5 another session's lease is never touched, and nothing is said" \
    || bad "T5 another session's lease is never touched, and nothing is said" "rc=$rc $OFX_OUT"

printf 'dirty\n' >> "$R/f.txt"
ofx_off "$R" >/dev/null 2>&1
turn_end A; rc=$?
{ [ $rc = 0 ] && [ -z "$OFX_OUT" ] && [ "$(lease_count)" = 1 ]; } \
    && ok "T6 OFF: a lease file is ignored, nothing said, nothing touched" \
    || bad "T6 OFF: a lease file is ignored, nothing said, nothing touched" "rc=$rc $OFX_OUT"
git -C "$R" checkout -- f.txt
ofx_on "$R" >/dev/null 2>&1
turn_end A
[ "$(lease_count)" = 0 ] && ok "T1 back on and at rest: the lease is released" || bad "T1 back on and at rest: the lease is released" "$OFX_OUT"

ofx_end A; ofx_end B
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -f "$SCRIPT_DIR/release-land-leases.mutation.sh" ]; then
    echo "=== running the mutation harness ==="
    if bash "$SCRIPT_DIR/release-land-leases.mutation.sh"; then
        ok "M. every rule above has been watched fail"
    else
        bad "M. the mutation harness found a property this suite does not actually prove"
    fi
fi

printf 'release-land-leases: %d passed, %d FAILED\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
