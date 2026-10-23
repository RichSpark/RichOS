#!/usr/bin/env bash
#
# stop-hook-visibility.test.sh — EVERY Stop HOOK, DERIVED FROM THE REGISTRATION
#                                 SURFACE, CAN BE SEEN GOING QUIET.
#
# ===========================================================================
# WHAT THIS SUITE IS FOR
# ===========================================================================
# A Stop hook that stands down, fails open, or cannot run must say so where the
# operator will actually read it. Measured against Claude Code 2.1.251: on a
# zero exit the host files a Stop hook's stdout AND its stderr into the
# transcript as a `hook_success` attachment and renders neither in the
# operator's scroll. The only channel that reaches him is a stdout
# {"systemMessage": "..."}. The measurement, with its positive probe, is in
# scripts/lib/stop-hook-notice.sh.
#
# So this suite exists to stop the next Stop hook from being written the way
# guard-unresolved-claims.sh was: three separate ways to switch itself off,
# each announced on a channel nobody can see, under a header warning that an
# unseeable opt-out "decays into a rumour".
#
# ===========================================================================
# THE SET IS DERIVED, NEVER TYPED
# ===========================================================================
# The inventory of Stop hooks comes from hooks/hooks.json through the one
# shared parser (scripts/lib/registered-hooks.sh), for the reason that file
# documents at length: a TYPED list of 14 where the registration held 15 is the
# defect that opened this whole sequence, and a typed count went stale INSIDE
# the file arguing counts must be derived, hours later. A Stop hook added
# tomorrow is covered by this suite with no edit here.
#
# NEGATIVE CONTROL: the derivation is asserted to have found a NON-ZERO number
# of Stop hooks, and the number is printed. Two mechanisms this session were
# caught reporting green because they read nothing.
#
# ===========================================================================
# RELATIONSHIP TO engine-status.test.sh's COUNT TRIPWIRE
# ===========================================================================
# That suite carries a deliberately hand-typed ACKNOWLEDGED_SCRIPTS set beside
# its derived fraction, so that registering a hook is something a human has to
# acknowledge rather than absorb. It is quoted here BY NAME and with no number:
# this comment used to say `REGISTERED_N -eq 27` and was still saying it at 51,
# because a literal copied into a second file is a second inventory that nothing
# checks -- the exact defect both suites exist to kill, sitting in the sentence
# describing it. THIS CHANGE DOES NOT MOVE THAT SET, and that is the honest
# outcome rather than an omission: nothing here registers a new hook.
# The notice channel is a LIBRARY that existing Stop hooks source, so
# hooks.json is untouched and the tripwire is untouched. If a later change to
# this area does wire a hook, that set is the thing to add the name to — never
# to silence.
# It also already carries the caveat that the banner's noun is one wide, since
# turn-manifest.sh is counted there and renders rather than guards.
#
# THIS SUITE TYPES NO COUNT OF ITS OWN. It reports the derived number and
# asserts only that it is non-zero and that the two registration surfaces
# agree. A literal here would be a third hand-maintained inventory.
#
# Usage: scripts/hooks/stop-hook-visibility.test.sh
# Exit:  0 all checks passed, 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOKS_JSON="$ENGINE_ROOT/hooks/hooks.json"
SETTINGS_JSON="$ENGINE_ROOT/.claude/settings.local.json"
NOTICE_LIB="$ENGINE_ROOT/scripts/lib/stop-hook-notice.sh"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "$2"; FAIL=$((FAIL + 1)); }

SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/stop-visibility.XXXXXX")"
trap 'rm -rf "$SANDBOX"' EXIT

command -v python3 >/dev/null 2>&1 || {
    echo "ERROR: stop-hook-visibility.test.sh needs python3 to parse the registration surfaces." >&2
    exit 1
}

echo "=== every Stop hook can be seen going quiet ==="
echo ""

# ===========================================================================
# 1. THE DERIVATION
# ===========================================================================
. "$ENGINE_ROOT/scripts/lib/registered-hooks.sh"

STOP_HOOKS=""
if ! STOP_HOOKS="$(registered_hook_scripts "$HOOKS_JSON" Stop)"; then
    bad "1a. derive the Stop set from hooks/hooks.json" \
        "registered_hook_scripts returned $? — no inventory. Refusing to report green over a set of nothing."
    STOP_HOOKS=""
fi

STOP_N="$(printf '%s\n' "$STOP_HOOKS" | grep -c . || true)"

# THE NEGATIVE CONTROL, first, because everything below is vacuous without it.
if [ "$STOP_N" -gt 0 ]; then
    ok "1a. the Stop set is DERIVED from $HOOKS_JSON — $STOP_N hook(s): $(printf '%s' "$STOP_HOOKS" | tr '\n' ' ')"
else
    bad "1a. the Stop set is DERIVED" \
        "derivation found ZERO Stop hooks. Every case below would pass by examining nothing, which is how two mechanisms this session reported green."
fi

# 1b. THE SECOND SURFACE, parsed independently.
#
# Two registration surfaces exist and both are real: hooks/hooks.json is what a
# plugin-loaded (by-reference) engine registers, .claude/settings.local.json is
# what a seated engine registers. A hook present on one and missing from the
# other is enforcement that exists in one installation mode and not the other —
# the drift the probe's Layer R checks for engine-status.sh specifically. This
# parse is deliberately NOT the shared library's: two independent readings of
# the same fact must agree, which is the discipline BR2 already applies.
SET_A="$(printf '%s\n' "$STOP_HOOKS" | LC_ALL=C sort | grep -c . >/dev/null 2>&1; printf '%s\n' "$STOP_HOOKS" | LC_ALL=C sort)"
SET_B="$(python3 - "$SETTINGS_JSON" <<'PY'
import json, re, sys
try:
    with open(sys.argv[1], encoding="utf-8") as fh:
        doc = json.load(fh)
except Exception:
    sys.exit(1)
found = set()
for entry in (doc.get("hooks", {}) or {}).get("Stop", []) or []:
    if not isinstance(entry, dict):
        continue
    for h in entry.get("hooks", []) or []:
        if isinstance(h, dict) and isinstance(h.get("command"), str):
            found.update(re.findall(r"scripts/hooks/([A-Za-z0-9._+-]+\.sh)", h["command"]))
for n in sorted(found):
    print(n)
PY
)"
if [ -z "$SET_B" ]; then
    bad "1b. the two registration surfaces agree" "$SETTINGS_JSON registers no Stop script (or would not parse)"
elif [ "$SET_A" = "$SET_B" ]; then
    ok "1b. the two registration surfaces agree on the Stop set (plugin hooks.json == seated settings.local.json)"
else
    bad "1b. the two registration surfaces agree" \
        "hooks.json: $(printf '%s' "$SET_A" | tr '\n' ' ') vs settings.local.json: $(printf '%s' "$SET_B" | tr '\n' ' ')"
fi

# 1c. THE REGISTRATION TRAP, on both surfaces — EXACTLY ONCE.
#
# The trap is that two "command" KEYS in ONE hook entry is valid JSON and
# silently keeps only the last, which unregisters a guard with no error. The
# check is made against the PARSED structure, where a shadowed key has already
# vanished and shows up as the absence it is: every Stop script must appear
# exactly once, once shadowing has done its worst.
#
# AN EARLIER VERSION OF THIS CASE ALSO DEMANDED ONE SCRIPT PER HOOK OBJECT, AND
# THAT WAS WRONG. A single entry's "hooks" ARRAY holding several commands is a
# supported, working shape — PreToolUse[Agent] has used it for four guards all
# along, and it was confirmed live here: turn-manifest.sh and
# notice-hook-staleness.sh share one Stop entry and BOTH fired in a real
# session, each with its own hook_success attachment. Asserting otherwise would
# have flagged two engineers' correct work as a defect, which is a false
# positive in a suite whose whole claim is that it has none. The array is not
# the trap; the duplicate KEY is, and 1d owns that.
TRAP_OUT="$(python3 - "$HOOKS_JSON" "$SETTINGS_JSON" <<'PY'
import json, re, sys
problems = []
for path in sys.argv[1:]:
    try:
        with open(path, encoding="utf-8") as fh:
            doc = json.load(fh)
    except Exception as exc:
        problems.append("%s: unparseable (%s)" % (path, exc))
        continue
    counts = {}
    for entry in (doc.get("hooks", {}) or {}).get("Stop", []) or []:
        if not isinstance(entry, dict):
            continue
        for c in (entry.get("hooks") or []):
            if not isinstance(c, dict) or not isinstance(c.get("command"), str):
                continue
            m = re.search(r"scripts/hooks/([A-Za-z0-9._+-]+\.sh)", c["command"])
            if m:
                counts[m.group(1)] = counts.get(m.group(1), 0) + 1
    for n, k in sorted(counts.items()):
        if k != 1:
            problems.append("%s: %s appears %d times in the parsed Stop table (expected exactly 1)" % (path, n, k))
print("\n".join(problems))
PY
)"
if [ -z "$TRAP_OUT" ]; then
    ok "1c. on BOTH surfaces every Stop script appears exactly once in the PARSED table"
else
    bad "1c. registration trap" "$TRAP_OUT"
fi

# 1d. THE DUPLICATE-KEY TRAP ITSELF.
#
# json.load silently keeps the last of two identical keys, so 1c cannot see a
# shadowed "command" — it sees a hook object that was never there. This case
# re-parses both files rejecting duplicate keys outright, which is the only way
# the shadowing is observable at all.
DUP_OUT="$(python3 - "$HOOKS_JSON" "$SETTINGS_JSON" <<'PY'
import json, sys
def no_dupes(pairs):
    seen = set()
    for k, _ in pairs:
        if k in seen:
            raise ValueError("duplicate key %r in one object" % k)
        seen.add(k)
    return dict(pairs)
problems = []
for path in sys.argv[1:]:
    try:
        with open(path, encoding="utf-8") as fh:
            json.load(fh, object_pairs_hook=no_dupes)
    except ValueError as exc:
        problems.append("%s: %s" % (path, exc))
    except Exception as exc:
        problems.append("%s: unreadable (%s)" % (path, exc))
print("\n".join(problems))
PY
)"
if [ -z "$DUP_OUT" ]; then
    ok "1d. neither surface hides a shadowed duplicate key (two \"command\" keys in one object is valid JSON and keeps only the last)"
else
    bad "1d. duplicate-key trap" "$DUP_OUT"
fi

# 1e. THE DERIVATION IS A PARSE, NOT A GREP — proved by mutation.
#
# Counting occurrences of the string "command" over the file also matches the
# VALUE of "type": "command", so a naive count reads roughly double. This
# sandbox registers a Stop entry whose command names NO script: a grep for
# "command" finds two matches, a real parse finds zero registered scripts. The
# derivation must say zero.
mkdir -p "$SANDBOX/nogrep/hooks"
cat > "$SANDBOX/nogrep/hooks/hooks.json" <<'JSON'
{
  "hooks": {
    "Stop": [
      { "hooks": [ { "type": "command", "command": "echo hello" } ] }
    ]
  }
}
JSON
NAIVE="$(grep -o '"command"' "$SANDBOX/nogrep/hooks/hooks.json" | grep -c . || true)"
PARSED="$(registered_hook_scripts "$SANDBOX/nogrep/hooks/hooks.json" Stop 2>/dev/null | grep -c . || true)"
if [ "$NAIVE" -ge 2 ] && [ "$PARSED" -eq 0 ]; then
    ok "1e. the derivation parses rather than greps (naive \"command\" count $NAIVE, registered scripts $PARSED)"
else
    bad "1e. parse-not-grep" "naive count $NAIVE, derivation returned $PARSED registered scripts — a grep would have reported a hook here"
fi

# 1f. …and it does not filter without a real parser, rather than over-counting.
if registered_hook_scripts "$HOOKS_JSON" Stop >/dev/null 2>&1; then
    NOPY="$(mktemp -d "$SANDBOX/nopy.XXXXXX")"
    RC_NOPY=0
    PATH="$NOPY" registered_hook_scripts "$HOOKS_JSON" Stop >/dev/null 2>&1 || RC_NOPY=$?
    if [ "$RC_NOPY" -eq 3 ]; then
        ok "1f. without python3 the event filter REFUSES (rc 3) rather than returning every event's hooks under Stop's name"
    else
        bad "1f. event filter refuses without python3" "expected rc 3, got $RC_NOPY"
    fi
fi

# ===========================================================================
# 2. THE NOTICE CHANNEL BLOCK IS PRESENT AND UNIFORM
# ===========================================================================
# Layer R's argument, applied to the other shared block: a divergent copy is one
# hook disagreeing with its siblings about how it tells you it stopped working.
# Three hooks each hand-rolling their own visibility is three chances to get it
# wrong, and the wrong version is silent.
if [ -f "$NOTICE_LIB" ]; then
    ok "2a. the shared notice channel exists at scripts/lib/stop-hook-notice.sh"
else
    bad "2a. shared notice channel" "missing: $NOTICE_LIB"
fi

# WHAT IS ENFORCED HERE, AND WHAT DELIBERATELY IS NOT.
#
# The requirement is BEHAVIORAL — a Stop hook that cannot run must reach the
# operator — and section 3 enforces exactly that, for every derived hook, with
# no exceptions and no opt-out. Using this particular helper is NOT the
# requirement, and an earlier version of this section demanded it. That flagged
# notice-hook-staleness.sh, which reached the same design independently
# (`announce_once`, session-scoped, on systemMessage) and passes section 3
# cleanly. Failing a correct hook for not sharing an implementation is enforcing
# style, and a style rule dressed as a safety check is how a suite starts
# producing the false positives it advertises it has none of.
#
# So: the ADOPTERS are named and required to agree with each other, the
# hand-rollers are named so the choice is visible rather than silent, and
# whether either group actually speaks is settled in section 3.
BLOCK_REF=""
BLOCK_ADOPTERS=""
BLOCK_HANDROLLED=""
BLOCK_DIVERGENT=""
BLOCK_ABSENT=""
for h in $STOP_HOOKS; do
    f="$ENGINE_ROOT/scripts/hooks/$h"
    if [ ! -f "$f" ]; then
        BLOCK_ABSENT="$BLOCK_ABSENT $h"
        continue
    fi
    blk="$(sed -n '/^# --- NOTICE CHANNEL ---/,/^fi$/p' "$f" 2>/dev/null)"
    if [ -z "$blk" ]; then
        BLOCK_HANDROLLED="$BLOCK_HANDROLLED $h"
        continue
    fi
    BLOCK_ADOPTERS="$BLOCK_ADOPTERS $h"
    if [ -z "$BLOCK_REF" ]; then
        BLOCK_REF="$blk"
    elif [ "$blk" != "$BLOCK_REF" ]; then
        BLOCK_DIVERGENT="$BLOCK_DIVERGENT $h"
    fi
done

if [ -n "$BLOCK_ABSENT" ]; then
    bad "2b. every registered Stop hook is present on disk" \
        "registered but missing:$BLOCK_ABSENT — the host would load nothing for these."
elif [ -n "$BLOCK_ADOPTERS" ]; then
    ok "2b. the shared notice channel is used by:$BLOCK_ADOPTERS${BLOCK_HANDROLLED:+ (hand-rolled, and asserted by 3a instead:$BLOCK_HANDROLLED)}"
else
    bad "2b. the shared notice channel is used by something" \
        "no Stop hook sources scripts/lib/stop-hook-notice.sh — the helper is dead code shipping as protection."
fi

if [ -n "$BLOCK_DIVERGENT" ]; then
    bad "2c. the notice channel block is byte-identical" \
        "diverged in:$BLOCK_DIVERGENT — the same failure Layer R catches in the root bootstrap, one level over."
else
    ok "2c. the NOTICE CHANNEL block is byte-identical across every hook that carries it"
fi

# ===========================================================================
# 3. BEHAVIOR — driven down a cannot-run path, each one reaches the operator
# ===========================================================================
# The generic condition every rooted Stop hook has: an explicitly declared root
# that is not an adopted repository. The resolver calls that `broken` ("an
# explicitly declared root MUST be an adopted engine root; the resolver will NOT
# quietly substitute a different one"), which is a hard failure — the guard
# cannot tell which repository it governs, so it is not doing its job.
#
# This is a FACT about the hook's control flow, not a judgment about its output.
# There is no threshold here and nothing to tune.
UNADOPTED="$SANDBOX/unadopted"
mkdir -p "$UNADOPTED"
STOP_PAYLOAD="$SANDBOX/payload.json"
python3 - "$UNADOPTED" > "$STOP_PAYLOAD" <<'PY'
import json, sys
print(json.dumps({
    "hook_event_name": "Stop",
    "session_id": "feedface-0000-4000-8000-000000000000",
    "transcript_path": "/nonexistent/transcript.jsonl",
    "cwd": sys.argv[1],
    "prompt_id": "deadbeef-0000-4000-8000-000000000000",
    "permission_mode": "default",
    "stop_hook_active": False,
    "last_assistant_message": "Done.",
    "background_tasks": [],
    "session_crons": [],
}))
PY

# says_so <hook-path> — 0 if a cannot-run run of this hook puts a systemMessage
# on stdout and exits without blocking. Sets SAYS_WHY on failure.
says_so() {
    local hook="$1" out err rc
    out="$(mktemp "$SANDBOX/o.XXXXXX")"; err="$(mktemp "$SANDBOX/e.XXXXXX")"
    RICHOS_ENTITY_ROOT="$UNADOPTED" bash "$hook" <"$STOP_PAYLOAD" >"$out" 2>"$err"
    rc=$?
    SAYS_WHY=""
    if [ "$rc" -eq 2 ]; then
        SAYS_WHY="exited 2 — a Stop hook that BLOCKS on a broken install re-fires to the block cap and strands the session"
    elif [ ! -s "$out" ]; then
        SAYS_WHY="printed NOTHING to stdout. stderr said: $(head -c 200 "$err" 2>/dev/null)"
    elif ! python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if isinstance(d.get("systemMessage"),str) and d["systemMessage"] else 1)' "$out" 2>/dev/null; then
        SAYS_WHY="stdout is not JSON carrying a non-empty systemMessage: $(head -c 200 "$out")"
    fi
    rm -f "$out" "$err"
    [ -z "$SAYS_WHY" ]
}

for h in $STOP_HOOKS; do
    f="$ENGINE_ROOT/scripts/hooks/$h"
    [ -f "$f" ] || continue
    if says_so "$f"; then
        ok "3a. $h announces to the OPERATOR when it cannot resolve the repository it governs"
    else
        bad "3a. $h announces when it cannot run" "$SAYS_WHY"
    fi
done

# 3b. THE MUTATION — prove 3a can fail, AND FOR THE RIGHT REASON.
#
# A copy of a real Stop hook with its notice put back on stderr, which is what
# every one of them did before. If 3a still passes against this, 3a is checking
# nothing.
#
# THE MIRROR IS NOT INCIDENTAL. The first version of this case dropped the copy
# in a bare temp directory, where `$SCRIPT_DIR/../lib/resolve-roots.sh` does not
# exist — so the mutant died on the BROKEN INSTALL branch before it ever reached
# the notice, and the case went green having demonstrated nothing about the
# channel. That is a control passing for the wrong reason, which is worth less
# than no control. It now runs from a mirror whose scripts/lib is the real one,
# and the failure REASON is asserted rather than merely the failure.
#
# The subject is chosen by CONTENT, not by sort order. It was `head -1` of the
# derived set, which broke the moment a fourth Stop hook landed whose name sorts
# first and which carries no stop_notice_abnormal call to mutate: the "mutation"
# changed nothing and the case reported a control it had not performed.
# The subject must also be one the mutation below removes EVERY call from:
# ass-kicker.sh sorts first since its rename (2026-10-23) and makes one call
# inside $(...), which the line-start sed cannot reach.
FIRST_HOOK=""
for h in $STOP_HOOKS; do
    if grep -q 'stop_notice_abnormal ' "$ENGINE_ROOT/scripts/hooks/$h" 2>/dev/null \
        && ! sed 's/^\( *\)stop_notice_abnormal .*$/\1echo "MUTANT stood down" >\&2/' \
            "$ENGINE_ROOT/scripts/hooks/$h" | grep -q 'stop_notice_abnormal '; then
        FIRST_HOOK="$h"
        break
    fi
done
if [ -z "$FIRST_HOOK" ]; then
    bad "3b. negative control" "no derived Stop hook carries a stop_notice_abnormal call to mutate — 3a's results are unproven"
elif [ -f "$ENGINE_ROOT/scripts/hooks/$FIRST_HOOK" ]; then
    mkdir -p "$SANDBOX/mirror/scripts/hooks"
    ln -sfn "$ENGINE_ROOT/scripts/lib" "$SANDBOX/mirror/scripts/lib"
    MUT="$SANDBOX/mirror/scripts/hooks/mutant.sh"
    sed 's/^\( *\)stop_notice_abnormal .*$/\1echo "MUTANT stood down" >\&2/' \
        "$ENGINE_ROOT/scripts/hooks/$FIRST_HOOK" > "$MUT"

    MUT_OUT="$(mktemp "$SANDBOX/mo.XXXXXX")"; MUT_ERR="$(mktemp "$SANDBOX/me.XXXXXX")"
    RICHOS_ENTITY_ROOT="$UNADOPTED" bash "$MUT" <"$STOP_PAYLOAD" >"$MUT_OUT" 2>"$MUT_ERR"

    if grep -q 'stop_notice_abnormal ' "$MUT"; then
        bad "3b. negative control" "the mutation did not remove the notice calls from $FIRST_HOOK — 3a's result is unproven"
    elif grep -q 'BROKEN INSTALL' "$MUT_ERR"; then
        bad "3b. negative control" "the mutant died on the broken-install branch, so it never reached the notice — this control would be passing for the wrong reason"
    elif ! grep -q 'MUTANT stood down' "$MUT_ERR"; then
        bad "3b. negative control" "the mutant did not reach its stderr notice at all: $(head -c 200 "$MUT_ERR")"
    elif says_so "$MUT"; then
        bad "3b. negative control" "a copy of $FIRST_HOOK announcing on stderr STILL PASSED 3a — 3a is not checking the channel"
    elif [ -s "$MUT_OUT" ]; then
        bad "3b. negative control" "the mutant put something on stdout: $(head -c 200 "$MUT_OUT")"
    else
        ok "3b. negative control: the same hook, reaching the same branch but announcing on stderr, FAILS 3a"
    fi
    rm -f "$MUT_OUT" "$MUT_ERR"
fi

# 3d. A NOTICE NEVER BLOCKS, EVEN ON A BROKEN INSTALL (hunt part 3, finding 3).
# Every notice-*.sh carries the shared root bootstrap, whose missing-library
# branch announces "ENFORCEMENT IS NOT ACTIVE". For a guard that is a refusal;
# for a notice it must be the same announcement on the operator channel and
# exit 0, because a Stop hook that exits 2 re-fires to the block cap and strands
# the turn it was only meant to inform. Each real notice is copied where its
# ../lib/resolve-roots.sh does not exist, which is the broken install.
BARE="$SANDBOX/bare/scripts/hooks"
mkdir -p "$BARE"
N3D=0
for f in "$ENGINE_ROOT"/scripts/hooks/notice-*.sh; do
    h="$(basename "$f")"
    case "$h" in *.test.sh) continue ;; esac
    grep -q 'BROKEN INSTALL' "$f" 2>/dev/null || continue
    N3D=$((N3D + 1))
    cp "$f" "$BARE/$h"
    b_out="$(mktemp "$SANDBOX/bo.XXXXXX")"
    RICHOS_ENTITY_ROOT="$UNADOPTED" bash "$BARE/$h" <"$STOP_PAYLOAD" >"$b_out" 2>/dev/null
    b_rc=$?
    if [ "$b_rc" -eq 0 ] && python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if "BROKEN INSTALL" in d.get("systemMessage","") else 1)' "$b_out" 2>/dev/null; then
        ok "3d. $h on a broken install announces to the operator and does not block"
    else
        bad "3d. $h on a broken install must announce and exit 0" "exit $b_rc; stdout: $(head -c 160 "$b_out" 2>/dev/null)"
    fi
    rm -f "$b_out"
done
if [ "$N3D" -eq 0 ]; then
    bad "3d. notice hooks were found to check" "no notice-*.sh carries the broken-install banner — 3d checked nothing"
fi

# 3c. THE NOISE CONTROL. A directory that has simply not adopted the engine is
# NOT APPLICABLE, not broken, and every Stop hook must stay silent there —
# otherwise "announce when you cannot run" turns into nagging every session
# opened outside a governed repo, which is the muting trap.
# Machine disk warnings are independent of adoption. Feed the actual disk hook a healthy
# fixture rather than the operator's live warning state. Its own D2-D6 cover alert delivery.
NOISE_DISK="$SANDBOX/healthy-disk"
mkdir -p "$NOISE_DISK/state" "$NOISE_DISK/launch"
touch "$NOISE_DISK/launch/com.richos.disk-watchdog.plist"
printf 'DISK_RICH_ALERT_GB=1\nDISK_WATCHDOG_ENABLE=1\n' > "$NOISE_DISK/config"
printf '{"volumes":[],"alerts":[]}\n' > "$NOISE_DISK/state/disk-watchdog.json"
NOISE_FAIL=""
for h in $STOP_HOOKS; do
    f="$ENGINE_ROOT/scripts/hooks/$h"
    [ -f "$f" ] || continue
    out="$( (cd "$UNADOPTED" && env -u RICHOS_ENTITY_ROOT -u CLAUDE_PROJECT_DIR \
            DISK_WATCHDOG_CONFIG="$NOISE_DISK/config" CLAUDE_CONFIG_DIR="$NOISE_DISK" \
            RICHOS_LAUNCH_AGENTS_DIR="$NOISE_DISK/launch" \
            bash "$f" <"$STOP_PAYLOAD" 2>/dev/null) )"
    [ -n "$out" ] && NOISE_FAIL="$NOISE_FAIL $h"
done
if [ -z "$NOISE_FAIL" ]; then
    ok "3c. noise control: an unadopted directory is not-applicable, and every Stop hook stays silent there"
else
    bad "3c. noise control" "spoke in an unadopted directory:$NOISE_FAIL"
fi

# ===========================================================================
# 4. THE STATE MACHINE — silence and success must not look the same
# ===========================================================================
# The design decision, argued in the helper's header: announce on STATE CHANGE,
# counting "nothing recorded yet this session" as one, and announce the return
# to normal too. Every case below is a fact about the ledger, not a judgment
# about a message.
ENTITY="$SANDBOX/entity"
mkdir -p "$ENTITY"
: > "$ENTITY/orchestration.config"
SID='{"session_id":"aaaabbbb-0000-4000-8000-000000000000"}'
SID2='{"session_id":"ccccdddd-0000-4000-8000-000000000000"}'

# drive <script-body> — run a fragment against the real helper, print its stdout.
drive() {
    bash -c '
        set -eo pipefail
        . "$1"
        shift
        eval "$@"
    ' _ "$NOTICE_LIB" "$@" 2>/dev/null
}

OUT1="$(drive "stop_notice_init hookA.sh '$ENTITY' '$SID'; stop_notice_abnormal stood-down 'OFF'")"
if printf '%s' "$OUT1" | grep -qF '"systemMessage"'; then
    ok "4a. the first abnormal turn announces"
else
    bad "4a. first abnormal announces" "got: ${OUT1:-<empty>}"
fi

OUT2="$(drive "stop_notice_init hookA.sh '$ENTITY' '$SID'; stop_notice_abnormal stood-down 'OFF'")"
if [ -z "$OUT2" ]; then
    ok "4b. the same abnormal state, unchanged, says nothing again"
else
    bad "4b. unchanged state is silent" "repeated itself: $OUT2"
fi

OUT3="$(drive "stop_notice_init hookA.sh '$ENTITY' '$SID'; stop_notice_abnormal no-python3 'BROKEN'")"
if printf '%s' "$OUT3" | grep -qF 'BROKEN'; then
    ok "4c. a DIFFERENT abnormal state is news, and is announced"
else
    bad "4c. state change announces" "got: ${OUT3:-<empty>}"
fi

OUT4="$(drive "stop_notice_init hookA.sh '$ENTITY' '$SID'; stop_notice_normal 'BACK'")"
if printf '%s' "$OUT4" | grep -qF 'BACK'; then
    ok "4d. the return to normal is announced, so silence means 'unchanged' rather than 'probably fine'"
else
    bad "4d. recovery announces" "got: ${OUT4:-<empty>}"
fi

OUT5="$(drive "stop_notice_init hookA.sh '$ENTITY' '$SID'; stop_notice_normal 'BACK'")"
if [ -z "$OUT5" ]; then
    ok "4e. a healthy guard stays quiet — running normally is never announced"
else
    bad "4e. healthy is quiet" "spoke: $OUT5"
fi

OUT6="$(drive "stop_notice_init hookB.sh '$ENTITY' '$SID'; stop_notice_normal 'BACK'")"
if [ -z "$OUT6" ]; then
    ok "4f. a guard that was healthy from its very first turn announces nothing at all"
else
    bad "4f. healthy from the start is quiet" "spoke: $OUT6"
fi

# 4g. A NEW SESSION RE-ANNOUNCES. This is the half of "once per session" that is
# kept: a stand-down cannot decay into a rumour across days, because the next
# session says it again.
OUT7="$(drive "stop_notice_init hookA.sh '$ENTITY' '$SID'; stop_notice_abnormal stood-down 'OFF'")"
OUT8="$(drive "stop_notice_init hookA.sh '$ENTITY' '$SID2'; stop_notice_abnormal stood-down 'OFF'")"
if printf '%s' "$OUT7" | grep -qF 'OFF' && printf '%s' "$OUT8" | grep -qF 'OFF'; then
    ok "4g. a NEW session re-announces a stand-down that is still in force"
else
    bad "4g. new session re-announces" "same-session: ${OUT7:-<empty>} / new-session: ${OUT8:-<empty>}"
fi

# 4h. NO LEDGER, NO MEMORY, SO IT REPEATS. Degrading toward noise is
# recoverable; degrading toward silence rebuilds the defect.
OUT9="$(drive "stop_notice_init hookA.sh '' '$SID'; stop_notice_abnormal stood-down 'OFF'")"
OUT10="$(drive "stop_notice_init hookA.sh '' '$SID'; stop_notice_abnormal stood-down 'OFF'")"
if printf '%s' "$OUT9" | grep -qF 'OFF' && printf '%s' "$OUT10" | grep -qF 'OFF'; then
    ok "4h. with no entity root there is no ledger, so it announces every turn rather than assuming it already has"
else
    bad "4h. no ledger means repeat" "first: ${OUT9:-<empty>} / second: ${OUT10:-<empty>}"
fi

# 4i. THE ESCAPING, which has to work without python3 because one of the states
# it must report is "python3 is not on PATH".
NOPY2="$(mktemp -d "$SANDBOX/nopy2.XXXXXX")"
OUT11="$(PATH="$NOPY2:/usr/bin:/bin" drive "stop_notice_init hookC.sh '' '$SID'; stop_notice_abnormal x 'quote\" back\\\\slash tab'")"
if printf '%s' "$OUT11" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if "quote\"" in d["systemMessage"] else 1)' 2>/dev/null; then
    ok "4i. a message containing a quote and a backslash still produces valid JSON"
else
    bad "4i. escaping" "not valid JSON, or the quote was lost: ${OUT11:-<empty>}"
fi

# ---------------------------------------------------------------------------
# 4j-4l. THE RECURRING RUNG — a control must not get QUIETER as a failure
#        persists (Frank D9, 2026-09-10; failure type F).
# ---------------------------------------------------------------------------
# stop_notice_abnormal announces on ENTRY and then stays quiet. For a
# PERSISTING condition that produced a 23-hour silence: a land-disposition
# demand raised at 3h was announced at ~3h and ~4h and then not again until
# ~27h, because the persisting half was left to an escalation ladder whose next
# rung is 24h. stop_notice_abnormal_recurring keeps the de-duplication and adds
# one sparse rung.
SIDR="{\"session_id\":\"reeeeeee-0000-4000-8000-000000000000\"}"
OUTR1="$(drive "stop_notice_init hookR.sh '$ENTITY' '$SIDR'; stop_notice_abnormal_recurring owed 'OWED' 3600")"
if printf '%s' "$OUTR1" | grep -qF '"systemMessage"'; then
    ok "4j. the recurring form announces on entry, exactly like the plain one"
else
    bad "4j. recurring announces on entry" "got: ${OUTR1:-<empty>}"
fi

# ...and stays quiet inside the interval. This is the PROPERTY BEING KEPT: the
# reason for de-duplicating is real, so the rung must not become a per-turn line.
OUTR2="$(drive "stop_notice_init hookR.sh '$ENTITY' '$SIDR'; stop_notice_abnormal_recurring owed 'OWED' 3600")"
if [ -z "$OUTR2" ]; then
    ok "4k. the same state inside the interval still says nothing — the rung is not a per-turn repeat"
else
    bad "4k. quiet inside the interval" "spoke again: $OUTR2"
fi

# ...and speaks again once the interval has passed. Driven by rewriting the
# recorded emission time rather than by sleeping: the ledger line is
# `<state><TAB><epoch>`, so backdating it is the same fact an hour later.
LEDGERR="$ENTITY/.claude/state/stop-hook-notices/reeeeeee.hookR.state"
if [ -f "$LEDGERR" ]; then
    printf 'owed\t%s\n' "$(( $(date +%s) - 7200 ))" > "$LEDGERR"
    OUTR3="$(drive "stop_notice_init hookR.sh '$ENTITY' '$SIDR'; stop_notice_abnormal_recurring owed 'OWED' 3600")"
    if printf '%s' "$OUTR3" | grep -qF '"systemMessage"'; then
        ok "4l. once the interval has passed the persisting condition speaks again — it cannot go silent for a working day"
    else
        bad "4l. the rung fires after the interval" "stayed silent: ${OUTR3:-<empty>}"
    fi
else
    bad "4l. the rung fires after the interval" "no ledger at $LEDGERR — the state was never recorded"
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    printf '\033[32m✓ %s/%s stop-hook visibility checks passed (%s Stop hook(s) derived from hooks/hooks.json).\033[0m\n' \
        "$PASS" "$((PASS + FAIL))" "$STOP_N"
    exit 0
fi
printf '\033[31m✗ %s/%s passed, %s FAILED.\033[0m\n' "$PASS" "$((PASS + FAIL))" "$FAIL" >&2
exit 1
