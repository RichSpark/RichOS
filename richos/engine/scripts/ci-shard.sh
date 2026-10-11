#!/usr/bin/env bash
#
# ci-shard.sh — run one shard of the engine's verification units, and leave a
# RECEIPT that says which ones actually ran.
#
# ===========================================================================
# WHY A RECEIPT, AND NOT JUST AN EXIT CODE
# ===========================================================================
# Sharding a verification pass introduces a failure mode the serial runner does
# not have: a shard that runs FEWER units than it was given still exits 0. A
# section selector that stopped matching, a matrix entry that never started, a
# job stopped by a concurrency rule — each of those turns into a green tick on
# a commit nothing verified. That is the same defect as "all 18 suites green"
# when there were 23, which is the defect `run-all-tests.sh` was written to end,
# and it comes straight back the moment the pass is split across machines.
#
# So every unit this script runs is written to a receipt, one JSON object per
# line, carrying the unit id, the exit code, the verdict, the wall clock, the
# shard it belonged to and THE COMMIT IT RAN AT. `--verify-receipts` then reads
# every shard's receipt back and refuses unless:
#
#   * the union of units run equals the planned inventory EXACTLY — a missing
#     unit is named, an unplanned one is named,
#   * every receipt carries the SAME commit, so a shard that somehow checked
#     out a different tree cannot be averaged in with the others,
#   * and every unit reached a verdict this script is willing to call green.
#
# The exit code of the whole workflow is then a claim about a set somebody can
# read, rather than about twelve jobs nobody counted.
#
# ===========================================================================
# EXIT 3 IS A SCOPED SECTION'S GREEN, AND EXIT 0 IS ITS FAILURE
# ===========================================================================
# `contract-integrity.test.sh --only <section>` exits 3 when it is green,
# deliberately, so that nothing can read a partial run as a full pass. A shard
# therefore expects 3 from a section unit — and expects NEVER to see 0, because
# 0 from a scoped invocation would mean the scoping did not apply and the unit
# silently ran something other than what was asked. Both directions are
# checked; `ci-units.sh` supplies the expected code per unit as data.
#
# ===========================================================================
# THE LEAK CANARY IS NOT LOST BY SHARDING
# ===========================================================================
# `run-all-tests.sh` takes a per-suite leak baseline so a suite that writes
# outside its sandbox — or mutates a tracked engine file mid-run — is NAMED
# rather than bisected. Splitting the pass must not quietly drop that
# guarantee, so this script takes the same per-unit baseline through the same
# `lib/leak-canary.sh`, and refuses to report a pass if the canary could not
# witness one of its roots.
#
# ===========================================================================
# KNOWN-RED IS DECLARED, DATED, EXPIRING, AND ITS OWN NEGATIVE CONTROL
# ===========================================================================
# A suite can be red on `main` for a reason that is neither this gate's business
# nor its author's: on 2026-09-09 two suites were red on BOTH macOS and Linux
# because two commits changed a contract and did not update the suites that
# assert it. Leaving them red makes every check red forever, which is how a gate
# gets ignored; deleting or skipping their assertions hides a real defect, which
# is worse. So they are DECLARED, in `lib/ci-known-red.tsv`, with the date, the
# failing cases, the commit that broke them and an EXPIRY.
#
# Three rules keep that table from becoming a place defects go to die:
#
#   1. a declared unit that fails is reported KNOWN-RED and does not fail the
#      job — but it is printed, every run, with its expiry;
#   0. and a row may carry a CONDITION saying where it applies at all, because
#      the first entry that needed one was red on git >= 2.55 and green on 2.43
#      and 2.52 — an unconditional row would have been right on the runner and a
#      lie on the developer's machine, where rule 2 would then fire on every
#      local pass. Where the condition is false the unit is judged normally,
#      rule 2 included;
#   2. a declared unit that PASSES fails the job. The defect is fixed and the
#      entry is now a lie; delete it. This is the negative control, and without
#      it the table would silently outlive everything in it;
#   3. an entry past its expiry fails the job. The tolerance was time-boxed
#      when it was granted, and a decision — fix it, or re-declare it with a
#      reason — is forced rather than deferred indefinitely.
#
# Usage:
#   ci-shard.sh --shard <i>/<n>          run shard i of n of the full plan
#   ci-shard.sh --only-units <id[,id…]>  run exactly these units
#   ci-shard.sh --units-file <path>      run the units named in a file
#   ci-shard.sh --list                   print what would run, run nothing
#   ci-shard.sh                          no selector: the WHOLE inventory, except in a teammate
#                                        workspace (a cc/ branch in a linked worktree,
#                                        lib/workspace_scope.py), where it is the units
#                                        ci-affected-units.sh maps the branch to, said in one
#                                        line; a branch that maps to none is refused (exit 2)
#   ci-shard.sh --full                   the whole inventory, wherever it runs
#   ci-shard.sh --verify-receipts <dir>  the coverage proof over collected
#                                        receipts; runs no tests. Add
#                                        --units-file to certify a RESTRICTED
#                                        plan (the affected gate's set) rather
#                                        than the whole inventory.
#   ci-shard.sh --units-file <f> --shard i/N   pack THAT set into N shards and
#                                        run shard i — the affected gate on a
#                                        large diff, through the same planner
#                                        the full pass uses.
#   options: --receipt <path>  --verbose  --allow-empty  --shards <n>
#            --emit-weights <path>  (with --verify-receipts) write the measured
#                                   cost of every unit in the format
#                                   lib/ci-unit-weights.tsv expects
#
# Exit codes:
#   0  every unit reached its expected verdict (KNOWN-RED counts as reached)
#   1  a unit failed, leaked, TIMED OUT, was declared-red-but-passed, or is past
#      expiry; or --verify-receipts found the union incomplete
#   2  usage, discovery, or a precondition (no units, unreadable engine)
#
# ===========================================================================
# EVERY UNIT RUNS UNDER A DEADLINE, AND THE PLAN REPORTS ITS OWN STALENESS
# ===========================================================================
# Two things were missing here until 2026-09-15 and they were the same
# omission wearing two hats: nothing measured a unit against what the plan
# expected of it.
#
#   * no unit had a time limit, so a hang became an anonymous job cancellation
#     forty minutes later. See `run_with_deadline` for the ceiling and the
#     arithmetic behind it. A unit that overruns is `TIMED-OUT` — its own
#     verdict, because "never finished" and "finished wrong" send the reader to
#     different files.
#   * nothing noticed when a unit's real cost left its recorded weight behind,
#     so the packer kept believing a 2811.7 s suite cost 60 s and that suite was
#     the wall clock of every push for days. See `report_weight_drift`.
# ===========================================================================

set -uo pipefail
ORIG_ARGS=("$@")
export PYTHONDONTWRITEBYTECODE=1

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
export RICHOS_VERIFICATION_CHECKOUT="${RICHOS_VERIFICATION_CHECKOUT:-$ENGINE_ROOT}"
UNITS_SH="$SCRIPT_DIR/ci-units.sh"
# The host shield (lib/host_shield.py, ceo-decisions §65). A fixture engine without it uses the
# one the shielded run around it exports; with neither it says so and runs unshielded.
SHIELD="$SCRIPT_DIR/lib/host_shield.py"
[ -f "$SHIELD" ] || SHIELD="${RICHOS_HOST_SHIELD_TOOL:-}"
SHIELDED=()
if [ -n "$SHIELD" ] && [ -f "$SHIELD" ]; then
    SHIELDED=(python3 -S "$SHIELD" run --)
else
    echo "ci-shard.sh: NO HOST SHIELD beside this copy and none exported (RICHOS_HOST_SHIELD_TOOL): units run WITHOUT the power/sleep/lock/display stand-ins." >&2
fi
KNOWN_RED="$ENGINE_ROOT/scripts/lib/ci-known-red.tsv"
WEIGHTS_TSV="$ENGINE_ROOT/scripts/lib/ci-unit-weights.tsv"

# ---------------------------------------------------------------------------
# THE PER-UNIT DEADLINE — so a hung unit is NAMED, not waited out
# ---------------------------------------------------------------------------
# Until 2026-09-15 this script ran every unit with no ceiling of any kind. The
# only bound was the workflow's `timeout-minutes` on the whole job, so a unit
# that hung took the entire shard down forty or sixty minutes later with
# `##[error]The operation was canceled.` and nothing at all about WHICH unit
# was stuck. That is the shape of red that gets waived rather than read.
#
# THE CEILING IS DERIVED FROM THE UNIT'S OWN WEIGHT, NOT TYPED AS A CONSTANT.
# Measured on run 34945072758, the inventory spans 0.2 s
# (`ledger-prune-sandbox.test.sh`) to 2811.7 s (`workspace-spec-fourteen.
# test.sh`) — a factor of fourteen thousand. One global number is either
# useless for the small units or fires constantly on the big one. So the
# deadline is `max(FLOOR, weight x FACTOR)`, capped at MAX, and
# `lib/ci-unit-weights.tsv` already carries the weight.
#
# THE THREE NUMBERS, and the arithmetic rather than a preference:
#
#   FACTOR 3   a weight is a measurement with a date, and the same harness has
#              honestly measured 381 s and 1500 s on different hardware under
#              different load (see the weights file's own header). Anything
#              tighter than 3x would fire on a loaded runner, and a deadline
#              that fires on healthy runs is a deadline somebody disables.
#   FLOOR 900  fifteen minutes. Below this the multiple is meaningless: 3x a
#              0.2 s unit is 0.6 s, which would kill it for being scheduled.
#   MAX 3600   sixty minutes, and this one is a CONSEQUENCE, not a taste. The
#              deadline is only useful if it fires BEFORE the job's own
#              `timeout-minutes`, because the whole point is to name the unit
#              instead of losing the job anonymously. engine-self-verify's
#              `shards` job is 75 minutes, so 60 leaves the shard time to
#              report, write its receipt and upload it.
#
# A TIMED-OUT UNIT IS A FAILURE WITH ITS OWN VERDICT, never a skip and never
# folded into FAIL. `TIMED-OUT` says the unit did not reach a verdict; `FAIL`
# says it reached one and it was wrong. Collapsing them would let a hang be
# diagnosed as a broken assertion, which is the wrong file to go and read.
#
# OVERRIDE for a deliberate local run: CI_SHARD_UNIT_TIMEOUT=<seconds>, or 0 to
# disable the deadline entirely. Never set in the workflow.
UNIT_TIMEOUT_FACTOR="${CI_SHARD_TIMEOUT_FACTOR:-3}"
UNIT_TIMEOUT_FLOOR="${CI_SHARD_TIMEOUT_FLOOR:-900}"
UNIT_TIMEOUT_MAX="${CI_SHARD_TIMEOUT_MAX:-3600}"

deadline_for() { # <weight-seconds> -> seconds
    python3 - "$1" "$UNIT_TIMEOUT_FACTOR" "$UNIT_TIMEOUT_FLOOR" "$UNIT_TIMEOUT_MAX" <<'PY'
import sys
try:
    w = float(sys.argv[1] or 0)
except ValueError:
    w = 0.0
factor, floor, cap = float(sys.argv[2]), float(sys.argv[3]), float(sys.argv[4])
print(int(min(cap, max(floor, w * factor))))
PY
}

# Run argv with a wall-clock deadline, portably. Returns the command's exit
# status, or 124 when the deadline was reached — the same code GNU `timeout`
# uses, so a reader who knows that tool is not surprised by this one.
#
# WHY THIS IS NOT `timeout(1)`. It is GNU coreutils. It is on `ubuntu-latest`
# and it is NOT on this project's development machines — verified here on
# 2026-09-15: `which timeout gtimeout` found neither on darwin 24.6.0. A
# deadline that exists only on the runner is a behavior the engineer cannot
# reproduce before pushing, which is the environment-parity failure this
# project has a standing rule about.
#
# STDIN IS /dev/null, DELIBERATELY AND ON BOTH SIDES. A background job in a
# non-interactive shell already gets /dev/null on most shells, so making it
# explicit is what keeps a local run and a runner run the same execution
# rather than two. A unit that wants to read stdin would hang in CI today.
#
# THE KILL TAKES THE WHOLE TREE (2026-09-23). It used to be `kill -TERM <pid>`
# then `kill -KILL <pid>`: the unit's shell died and everything it had started
# lived on. workspace-spec-fourteen.test.sh was killed at 3600 s while its
# mutation harness continued under init, spawning suites and leaving `sleep 3600`s
# while ignoring SIGTERM. Its total elapsed time did not establish how long it was orphaned. So the unit is started
# in a PROCESS GROUP OF ITS OWN (`set -m` for that one launch), and the kill is
# scripts/lib/proc_tree.py: every descendant, every group any of them is in
# (which is how a background child re-parented to init is still reached), TERM,
# three seconds for EXIT traps, then KILL. ci-shard.test.sh S19e proves no
# process of a timed-out unit survives, including one that ignores TERM, one
# re-parented to init and one in a session of its own.
run_with_deadline() { # <seconds> <logfile> <argv...>
    local _limit="$1" _log="$2"; shift 2
    if [ "${_limit:-0}" -le 0 ]; then
        "$@" >"$_log" 2>&1 </dev/null
        return $?
    fi
    # The existing process supervisor already polls and owns tree cleanup.
    # Giving it the deadline avoids a second shell watcher rounding every
    # short unit up to a one-second sleep.
    python3 "$SCRIPT_DIR/lib/proc_tree.py" run "$$" --deadline "$_limit" \
        --timeout-marker "$_log.deadline" -- "$@" >"$_log" 2>&1 </dev/null
    return $?

}

# ---------------------------------------------------------------------------
# WEIGHT DRIFT — the packing data reports its own staleness
# ---------------------------------------------------------------------------
# `lib/ci-unit-weights.tsv` went stale silently and stayed stale, and it cost
# more wall clock than anything else in this system. Measured: 32 of the
# inventory's 137 units carried no row and were packed at DEFAULT_WEIGHT = 60 s;
# one of them, `mega-lander/tests/workspace-spec-fourteen.test.sh`, actually takes
# 2811.7 s. Forty-seven times. The packer put it in a shard with eleven other
# units and that shard was the wall clock of every push — 3146 s on run #216,
# 3262 s on #212, against 194-1015 s for every other shard.
#
# NOBODY WAS NEGLIGENT. The file's own header says how to re-measure, and the
# coverage job's closing line said `re-pack against measured cost with: ...
# (durations above)` — where there were no durations above. Re-measuring meant
# a person downloading twelve job logs and doing arithmetic, and a maintenance
# task that needs a human eye is a maintenance task that does not happen. That
# is a defect in the design, not in the maintainer.
#
# SO THE MEASUREMENT REPORTS ITSELF, at the moment it is taken, in the line the
# reader is already looking at, carrying the exact replacement row. There is no
# arithmetic left to do and nothing to remember.
#
# IT IS A REPORT AND NOT A FAILURE, deliberately. A wrong weight costs
# wall-clock balance and cannot make a green wrong — nothing it changes is what
# is verified. Failing the build on it would make every legitimately slower run
# a red build, and a gate that cries about balance is a gate that gets waived,
# which is how the last three guards in this project died.
#
# THE THRESHOLD is `measured > 2x weight AND measured > weight + 60s`, and BOTH
# clauses are load-bearing. The multiple alone would print a line for every
# 0.3 s unit that took 0.7 s once; the absolute alone would stay silent while a
# 200 s unit doubled. Together they name only a drift that could actually move
# a unit between shards. Sixty seconds is the smallest imbalance worth a
# reader's attention when the whole serial pass is 7723 s across 12 shards.
#
# Declared as overridable constants for the same reason the deadline's are:
# a number that decides what gets printed should be readable and testable
# without editing the script that uses it.
DRIFT_FACTOR="${CI_SHARD_DRIFT_FACTOR:-2}"
DRIFT_FLOOR="${CI_SHARD_DRIFT_FLOOR:-60}"

report_weight_drift() { # <unit-id> <measured-seconds> <planned-weight>
    python3 - "$1" "$2" "$3" "$DRIFT_FACTOR" "$DRIFT_FLOOR" <<'PY'
import sys
uid, measured, weight = sys.argv[1], float(sys.argv[2] or 0), float(sys.argv[3] or 0)
factor, floor = float(sys.argv[4]), float(sys.argv[5])
if weight <= 0:
    sys.exit(0)
if measured > weight * factor and measured > weight + floor:
    print("        WEIGHT-DRIFT  planned %.1fs, measured %.1fs (%.1fx). The shard plan is built "
          "from the planned number, so this unit is in the wrong shard."
          % (weight, measured, measured / weight))
    print("        Replacement row for lib/ci-unit-weights.tsv:")
    print("          %s\t%.1f\t<this-run-id>" % (uid, measured))
PY
}

# ---------------------------------------------------------------------------
# THE ENGINE'S OWN RUNTIME STATE IS NOT A UNIT'S TO WRITE (2026-10-05)
# ---------------------------------------------------------------------------
# What happened: hook suites seated their hooks on the engine checkout itself
# (`RICHOS_ENTITY_ROOT=<engine>`), so every fixture spawn, ack and unparseable
# payload appended a line to <engine>/.claude/state/ (main-checkout-runs.log,
# model-downgrade-acks.log, hand-roll-acks.log, unevaluated-payloads.log,
# left-off.nosession.state). In the merge gate that engine is the main checkout,
# which is also the live plugin, so test lines landed in the operator's real
# audit logs, and the proof gate bound that folder as an input of every engine
# check running beside the writer: 501 finished results thrown away as
# `invalid`, 26,512 s of check work in 34 of 211 attempts (richos-hq
# docs/operations/2026-10-04-merge-check-speed.md, section 2).
#
# Why no check caught it: the leak canary witnesses `git status`, and the state
# folder is gitignored, so the canary is blind there BY DESIGN (lib/leak-canary.sh).
# The one fix before this was per suite (guard-vendoring-commits C9), one
# incident at a time.
#
# So the folder is witnessed here, per unit, by content: every file's bytes
# before and after. A unit after which it changed is STATE-WRITTEN, a failure of
# its own, named with the files and the lines it added. A suite that needs the
# engine's adoption seats its hooks on a sandbox copy (lib/entity-sandbox.sh)
# instead. ATTRIBUTION: under proof-run.py's parallel lanes another lane's unit
# writing at the same moment is charged here too, exactly as the leak canary's
# note says; the lines printed say whose fixture they were.
STATE_WITNESS_DIR="$ENGINE_ROOT/.claude/state"
state_witness() { # <out-file> — every file under the state folder: size and sha256
    python3 - "$STATE_WITNESS_DIR" "$1" <<'PY'
import hashlib, json, os, sys
root, out = sys.argv[1], sys.argv[2]
seen = {}
if os.path.isdir(root):
    for base, dirs, files in os.walk(root):
        dirs.sort()
        for name in sorted(files):
            path = os.path.join(base, name)
            rel = os.path.relpath(path, root)
            try:
                if os.path.islink(path):
                    seen[rel] = {"link": os.readlink(path)}
                    continue
                with open(path, "rb") as stream:
                    data = stream.read()
                seen[rel] = {"size": len(data), "sha256": hashlib.sha256(data).hexdigest()}
            except OSError as exc:
                seen[rel] = {"unreadable": str(exc)}
with open(out, "w") as stream:
    json.dump(seen, stream)
PY
}
state_written() { # <before-file> — prints one line per changed file, nothing when unchanged
    python3 - "$STATE_WITNESS_DIR" "$1" <<'PY'
import hashlib, json, os, sys
root, before_file = sys.argv[1], sys.argv[2]
try:
    before = json.load(open(before_file))
except (OSError, ValueError):
    print("the state witness taken before the unit could not be read")
    sys.exit(0)
after = {}
if os.path.isdir(root):
    for base, dirs, files in os.walk(root):
        for name in files:
            path = os.path.join(base, name)
            after[os.path.relpath(path, root)] = path
for rel in sorted(set(before) | set(after)):
    old, path = before.get(rel), after.get(rel)
    if path is None:
        print("%s  removed" % rel)
        continue
    try:
        if os.path.islink(path):
            if old != {"link": os.readlink(path)}:
                print("%s  link changed" % rel)
            continue
        data = open(path, "rb").read()
    except OSError as exc:
        print("%s  unreadable after the unit (%s)" % (rel, exc))
        continue
    if old and old.get("sha256") == hashlib.sha256(data).hexdigest():
        continue
    if old and "size" in old and len(data) > old["size"] \
            and hashlib.sha256(data[:old["size"]]).hexdigest() == old["sha256"]:
        added = data[old["size"]:].decode("utf-8", "replace").splitlines()
        print("%s  appended %d line(s); first: %s" % (rel, len(added), (added[0] if added else "")[:200]))
    elif old:
        print("%s  rewritten" % rel)
    else:
        first = data.decode("utf-8", "replace").splitlines()[:1]
        print("%s  created; first line: %s" % (rel, (first[0] if first else "")[:200]))
PY
}

C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_YEL=$'\033[33m'; C_BOLD=$'\033[1m'; C_RESET=$'\033[0m'

die() { printf '%sERROR: ci-shard.sh: %s%s\n' "$C_RED" "$1" "$C_RESET" >&2; exit "${2:-2}"; }

[ -f "$UNITS_SH" ] || die "missing $UNITS_SH — the unit inventory is not optional."

SHARD=""
SHARDS=""
ONLY_UNITS=""
UNITS_FILE=""
RECEIPT=""
VERIFY_DIR=""
EMIT_WEIGHTS=""
LIST_ONLY=0
VERBOSE=0
ALLOW_EMPTY=0
FAIL_FAST=0
FULL=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --shard)  [ "$#" -ge 2 ] || die "--shard needs <i>/<n> or <i>"
                  case "$2" in
                      */*) SHARD="${2%%/*}"; SHARDS="${2##*/}" ;;
                      *)   SHARD="$2" ;;
                  esac
                  shift 2 ;;
        --shards) [ "$#" -ge 2 ] || die "--shards needs a count"; SHARDS="$2"; shift 2 ;;
        --only-units) [ "$#" -ge 2 ] || die "--only-units needs a comma-separated list"
                  ONLY_UNITS="$ONLY_UNITS,$2"; shift 2 ;;
        --units-file) [ "$#" -ge 2 ] || die "--units-file needs a path"; UNITS_FILE="$2"; shift 2 ;;
        --receipt) [ "$#" -ge 2 ] || die "--receipt needs a path"; RECEIPT="$2"; shift 2 ;;
        --verify-receipts) [ "$#" -ge 2 ] || die "--verify-receipts needs a directory"; VERIFY_DIR="$2"; shift 2 ;;
        --emit-weights) [ "$#" -ge 2 ] || die "--emit-weights needs a path"; EMIT_WEIGHTS="$2"; shift 2 ;;
        --list) LIST_ONLY=1; shift ;;
        --verbose|-v) VERBOSE=1; shift ;;
        --allow-empty) ALLOW_EMPTY=1; shift ;;
        --fail-fast) FAIL_FAST=1; shift ;;
        --full) FULL=1; shift ;;
        -h|--help) sed -n '/^# Usage:/,/^# =\{10,\}$/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unrecognized argument '$1' — see --help" ;;
    esac
done

[ -z "$SHARDS" ] && SHARDS="$(bash "$UNITS_SH" shard-count)"

# The commit under test. Baked into every receipt so the coverage proof can
# refuse a set of shards that did not all verify the same tree — identity, not
# a timestamp and not "the checkout said success".
sha_now() {
    git -C "$ENGINE_ROOT" rev-parse HEAD 2>/dev/null || printf 'unknown'
}

# ---------------------------------------------------------------------------
# the known-red table
# ---------------------------------------------------------------------------
# id <TAB> classified <TAB> expires <TAB> broken-by <TAB> failing <TAB> why [<TAB> when]
kr_field() { # <unit-id> <column>
    [ -f "$KNOWN_RED" ] || return 1
    awk -F'\t' -v want="$1" -v col="$2" '
        /^#/ { next } NF < 6 { next }
        $1 == want { print $col; found=1; exit }
        END { exit(found ? 0 : 1) }' "$KNOWN_RED"
}

# THE OPTIONAL SEVENTH COLUMN: a shell predicate saying WHERE the row applies.
#
# Added 2026-09-10, because the first entry that needed it proved the table
# could not describe a real defect. `scripts/lib/worktree-ledger.test.sh` is red
# on git >= 2.55 — which is what `ubuntu-latest` ships — and GREEN on 2.43 and
# on 2.52. An unconditional row would therefore be correct on the runner and a
# LIE on the machine the engine is developed on, where rule 2 would fire on
# every local full pass with "declared red, but it PASSED". A mechanism that can
# only say "red everywhere" describes a defect shape that is not the common one:
# this tree already carries launchd-only and darwin-only cases.
#
# Empty or absent means the row always applies, so every existing row keeps its
# meaning. A predicate that exits NON-ZERO means the row does not apply here and
# the unit is judged normally — including rule 2, so a host where the defect
# does not exist still gets a real verdict rather than a tolerated one.
#
# AN UNRUNNABLE PREDICATE IS NOT A PASS. If the condition cannot be evaluated,
# the row is treated as APPLYING and the reason is printed: a condition that
# silently fails open would turn the whole table back into a skip list, one
# broken shell expression at a time.
kr_applies() { # <unit-id> — 0 when the row is in force here
    local cond
    cond="$(kr_field "$1" 7 2>/dev/null || true)"
    [ -n "$cond" ] || return 0
    if bash -c "$cond" >/dev/null 2>&1; then
        return 0
    fi
    # distinguish "predicate said no" from "predicate could not run"
    if ! bash -n -c "$cond" >/dev/null 2>&1; then
        printf '%sWARNING%s ci-shard.sh: the known-red condition for %s is not valid shell (%s);\n' \
            "$C_YEL" "$C_RESET" "$1" "$cond" >&2
        printf '        treating the row as APPLYING rather than failing open.\n' >&2
        return 0
    fi
    return 1
}

kr_declared() { kr_field "$1" 1 >/dev/null 2>&1 && kr_applies "$1"; }
kr_expired()  { # <unit-id> — 0 when the entry's expiry has passed or is not a valid date
    local exp rc=0; exp="$(kr_field "$1" 3 2>/dev/null)" || return 1
    # P5-77: only a real calendar date YYYY-MM-DD that is today or later keeps an entry
    # live (python exits 7 for that, and for nothing else). A blank or malformed expiry
    # (`tomorrow`, `2999-99-99`) sorted after every date as a string and excused a red
    # unit forever; and a python that cannot run must not fail open either, so anything
    # other than the one live answer counts as expired.
    # LOCAL date: the expiry is a date the operator writes, so it is read on the operator's
    # calendar. ci-receipts.py (lib/) reads the same column the same way; a UTC "today" here
    # made S8 fail 00:00-01:00 BST, when local yesterday equals the UTC date. An unreadable
    # local date is not a date either, so it fails closed like a malformed expiry.
    python3 -c 'import datetime, re, sys
e = sys.argv[1]
try:
    live = bool(re.fullmatch(r"\d{4}-\d{2}-\d{2}", e)) and \
        datetime.date.fromisoformat(e) >= datetime.date.fromisoformat(sys.argv[2])
except ValueError:
    live = False
sys.exit(7 if live else 0)' "$exp" "$(date +%Y-%m-%d)" 2>/dev/null || rc=$?
    [ "$rc" -ne 7 ]
}

# ---------------------------------------------------------------------------
# --verify-receipts: the coverage proof. Runs nothing.
# ---------------------------------------------------------------------------
if [ -n "$VERIFY_DIR" ]; then
    [ -d "$VERIFY_DIR" ] || die "--verify-receipts: no such directory: $VERIFY_DIR"
    PLAN_FILE="$(mktemp)"; trap 'rm -f "$PLAN_FILE"' EXIT
    # With --units-file the plan being certified is that SET, not the whole
    # inventory: the affected gate covers what the diff selected, and checking
    # its receipts against all 148 units would report 94 false absences.
    if [ -n "$UNITS_FILE" ]; then
        [ -f "$UNITS_FILE" ] || die "--units-file: no such file: $UNITS_FILE"
        bash "$UNITS_SH" units --units-file "$UNITS_FILE" | cut -f1 | LC_ALL=C sort > "$PLAN_FILE" \
            || die "could not read the restricted inventory"
    else
        bash "$UNITS_SH" units | cut -f1 | LC_ALL=C sort > "$PLAN_FILE" || die "could not read the planned inventory"
    fi
    # --weights and --emit-weights are passed UNCONDITIONALLY, not behind a
    # flag the workflow has to remember. The whole finding they answer is that
    # an opt-in maintenance step does not get opted into.
    RW_ARGS=(--plan "$PLAN_FILE")
    [ -z "${RICHOS_PROOF_RUN:-}" ] || RW_ARGS+=(--proof-run "$RICHOS_PROOF_RUN")
    [ -f "$WEIGHTS_TSV" ] && RW_ARGS+=(--weights "$WEIGHTS_TSV")
    [ -n "$EMIT_WEIGHTS" ] && RW_ARGS+=(--emit-weights "$EMIT_WEIGHTS")
    find "$VERIFY_DIR" -type f -name '*.jsonl' -print0 2>/dev/null | xargs -0 cat 2>/dev/null \
        | python3 "$SCRIPT_DIR/lib/ci-receipts.py" verify "${RW_ARGS[@]}"
    exit $?
fi

# ---------------------------------------------------------------------------
# a teammate workspace runs what its branch touches (CEO, 2026-10-01)
# ---------------------------------------------------------------------------
# No selector by hand in a teammate workspace was the two-hour whole inventory. There it
# is now the units the engine's own diff map (ci-affected-units.sh) names for the branch,
# committed or not. `--full`, any selector, the main checkout and RICHOS_TEST_SCOPE=full
# (proof-run.py sets it for every check) keep the behavior above, untouched.
SCOPE_TOOL="$SCRIPT_DIR/lib/workspace_scope.py"
if [ "$FULL" -eq 1 ]; then
    export RICHOS_TEST_SCOPE=full
elif [ -z "$SHARD" ] && [ -z "$ONLY_UNITS" ] && [ -z "$UNITS_FILE" ] && [ -f "$SCOPE_TOOL" ]; then
    # Read as fields, never eval'd: the engine's input qualifier refuses dynamic evaluation.
    decided="$(python3 "$SCOPE_TOOL" fields "$ENGINE_ROOT")" \
        || die "could not decide whether this is a workspace run; nothing ran" 2
    scope=""; base=""; branch=""
    IFS=: read -r scope base branch <<<"$decided"
    if [ "$scope" = narrow ]; then
        where="workspace run on ${branch:-this checkout}, compared with main at ${base:0:12}"
        units="$(bash "$SCRIPT_DIR/ci-affected-units.sh" --working --base "$base")" \
            || die "REFUSED: $where: ci-affected-units.sh could not map the branch. Everything: bash scripts/ci-shard.sh --full" 2
        units="$(printf '%s\n' "$units" | grep -v '^$' | LC_ALL=C sort -u | tr '\n' ',' | sed 's/,$//' || true)"
        [ -n "$units" ] || die "REFUSED: $where: ci-affected-units.sh maps the branch to no engine unit, so nothing would run. Everything: bash scripts/ci-shard.sh --full ; chosen: --only-units <id>" 2
        ONLY_UNITS=",$units"
        printf 'ci-shard.sh: %s: the unit(s) ci-affected-units.sh maps it to: %s. Everything: bash scripts/ci-shard.sh --full\n' \
            "$where" "$(printf '%s' "$units" | sed 's/,/, /g')"
    fi
fi

# ---------------------------------------------------------------------------
# which units this invocation owns
# ---------------------------------------------------------------------------
ALL_UNITS="$(mktemp)"; SELECTED="$(mktemp)"
trap 'rm -f "$ALL_UNITS" "$SELECTED"' EXIT
bash "$UNITS_SH" units > "$ALL_UNITS" || die "ci-units.sh failed — the inventory could not be built."

resolve_selection() {
    # --units-file AND --shard together: pack THAT set and take shard i of it.
    # The affected gate uses this, so a large diff is spread across machines by
    # exactly the planner the full pass uses rather than by a second mechanism
    # that has to be kept in step with it.
    if [ -n "$UNITS_FILE" ] && [ -n "$SHARD" ]; then
        [ -f "$UNITS_FILE" ] || die "--units-file: no such file: $UNITS_FILE"
        # The plan is read into a variable first: in a pipeline its failure (a
        # requested unit the inventory rejects) was swallowed and only the
        # recognized units carried on. A failed plan is fatal.
        local plan
        plan="$(bash "$UNITS_SH" shards "$SHARDS" --units-file "$UNITS_FILE")" \
            || die "the restricted shard plan could not be built (see the error above); refusing to run a partial selection" 2
        printf '%s\n' "$plan" | awk -F'\t' -v i="$SHARD" '$1 == i { print $2 }' | LC_ALL=C sort
        return 0
    fi
    if [ -n "$ONLY_UNITS" ] || [ -n "$UNITS_FILE" ]; then
        # The file is checked HERE, in this shell: `die` inside the pipeline's left
        # side ran in a child, printed its error and let the inline --only-units
        # through as a successful PARTIAL plan (P5-78).
        if [ -n "$UNITS_FILE" ]; then
            [ -f "$UNITS_FILE" ] || die "--units-file: no such file: $UNITS_FILE"
            [ -r "$UNITS_FILE" ] || die "--units-file: not readable: $UNITS_FILE"
        fi
        {
            printf '%s' "${ONLY_UNITS#,}" | tr ',' '\n'
            [ -z "$UNITS_FILE" ] || cat "$UNITS_FILE"
        } | sed 's/^[[:space:]]*//; s/[[:space:]]*$//' | grep -v '^$' | grep -v '^#' | LC_ALL=C sort -u
        return 0
    fi
    if [ -n "$SHARD" ]; then
        local plan
        plan="$(bash "$UNITS_SH" shards "$SHARDS")" \
            || die "the shard plan could not be built (see the error above); refusing to run a partial selection" 2
        printf '%s\n' "$plan" | awk -F'\t' -v i="$SHARD" '$1 == i { print $2 }' | LC_ALL=C sort
        return 0
    fi
    # No selector at all is the WHOLE inventory in one process. Honest, and
    # the two-hour run this exists to replace, so it is allowed but named.
    cut -f1 "$ALL_UNITS" | LC_ALL=C sort
}

resolve_selection > "$SELECTED"

# Every selected id must be a real unit. An id that matches nothing is fatal:
# a shard silently selecting nothing is the whole failure this file is about.
UNKNOWN=0
while IFS= read -r id; do
    [ -n "$id" ] || continue
    # Read the inventory directly. An early-exiting grep in a pipeline can
    # SIGPIPE cut under pipefail and misreport a present unit as missing.
    if ! awk -F'\t' -v want="$id" '$1 == want { found=1 } END { exit !found }' "$ALL_UNITS"; then
        printf '%sERROR%s ci-shard.sh: %s is not a unit in the current inventory.\n' "$C_RED" "$C_RESET" "$id" >&2
        UNKNOWN=1
    fi
done < "$SELECTED"
[ "$UNKNOWN" -eq 0 ] || die "one or more requested units do not exist — run 'ci-units.sh units' to see the inventory." 2

N_SEL="$(grep -c . "$SELECTED" || true)"
if [ "${N_SEL:-0}" -eq 0 ]; then
    if [ "$ALLOW_EMPTY" -eq 1 ]; then
        printf '%s— ci-shard.sh: no units selected, and --allow-empty was given. Nothing was verified.%s\n' "$C_YEL" "$C_RESET"
        [ -n "$RECEIPT" ] && : > "$RECEIPT"
        exit 0
    fi
    die "no units selected. A shard that verifies nothing must never exit 0 — pass --allow-empty only where an empty selection is a real answer." 2
fi

if [ "$LIST_ONLY" -eq 1 ]; then
    cat "$SELECTED"
    printf '%s unit(s) selected of %s in the inventory\n' "$N_SEL" "$(grep -c . "$ALL_UNITS")" >&2
    exit 0
fi

# Acquire the large-plan slot before any worker permit. Unit admission below
# releases the worker between units, including direct shard invocations.
python3 "$SCRIPT_DIR/lib/engine_pass.py" needed "$N_SEL"; PASS_RC=$?
PASS_ARGS=()
case "$PASS_RC" in
    0) ;;
    10) [ "${RICHOS_WORKER_SLOT_HELD:-}" != 1 ] || die "large engine plan must acquire its slot before a worker; use proof-run's unit dispatch" 75
        PASS_ARGS=(python3 "$SCRIPT_DIR/lib/engine_pass.py" hold --count "$N_SEL"
                   --label "ci-shard.sh" --checkout "$ENGINE_ROOT" --) ;;
    *) die "could not establish large-plan admission" 2 ;;
esac

# ---------------------------------------------------------------------------
# the leak canary — same library, same contract as run-all-tests.sh
# ---------------------------------------------------------------------------
for lib in tree-witness leak-canary record-canary; do
    [ -f "$ENGINE_ROOT/scripts/lib/$lib.sh" ] \
        || die "scripts/lib/$lib.sh is missing; refusing to report a pass with the sandbox check absent." 2
done
# shellcheck source=lib/tree-witness.sh
. "$ENGINE_ROOT/scripts/lib/tree-witness.sh"
# shellcheck source=lib/leak-canary.sh
. "$ENGINE_ROOT/scripts/lib/leak-canary.sh"
# the operator's record (ledger, fallback event log, team directories), per
# unit — the same library and contract as run-all-tests.sh (round 15)
# shellcheck source=lib/record-canary.sh
. "$ENGINE_ROOT/scripts/lib/record-canary.sh"

# THE RUN'S FOLDER, AND WHO MAY REMOVE IT (2026-10-02).
#
# What happened: the merge gate of cc/zach-sonnet-vinputs1 failed three
# attempts in a row on by-reference.test.sh and operator-fences.test.sh with
#   FileNotFoundError: .../T//ci-shard.cH28fX/1.timing.json.new
# and it read as "something deleted the folder while the unit ran". It did
# not. Each traceback was the LAST write to its log, at 601 s, the gate's
# 600 s cap, and no verdict line followed it. The deleter was THIS SCRIPT: the
# gate's stop (proof-run.py stop_item, then proc_tree.finish_scope) sends TERM
# to every process of the check at once. Bash, with TERM untrapped, ran the EXIT
# trap below at once and removed the folder while worker_tokens.py, its own
# child, was still stopping the unit and had yet to write its timing file into
# it. The unit's own output (1.log) went with the folder, so the one record of
# where the unit was when it was stopped was destroyed by the stop itself.
#
# Two rules, so that cannot happen again:
#   1. The folder is never removed while its unit is running. TERM, INT and HUP
#      are TRAPPED (ci_shard_stopped). Bash defers a trapped signal until the
#      foreground child returns, so the handler, and the EXIT trap after it,
#      run only once the unit and worker_tokens.py have finished. The handler
#      prints the unit's own last output before the folder goes.
#   2. The folder comes from the engine's scratch allocator (lib/scratch.sh,
#      CLAUDE.md working rule 5), with this script's pid recorded in the ledger
#      and in the name. If this script is SIGKILLed, nothing removes the folder
#      here; the scratch reaper's allocator arm removes it once that pid has
#      ended, and keeps it while the pid is alive, whether or not any file in it
#      is open.
# A fixture that copies only this script and no lib/scratch.sh keeps the old
# bare mktemp; rule 1 holds either way.
LOG_DIR=""
LOG_DIR_ALLOCATED=0
CI_SHARD_UNIT_RUNNING=0
ci_shard_cleanup() {
    rm -f "$ALL_UNITS" "$SELECTED"
    [ -n "$LOG_DIR" ] || return 0
    if [ "$LOG_DIR_ALLOCATED" -eq 1 ]; then
        scratch_release "$LOG_DIR" || true
    else
        rm -rf "$LOG_DIR"
    fi
}
ci_shard_stopped() { # <signal-number>
    trap - TERM INT HUP
    if [ "$CI_SHARD_UNIT_RUNNING" -eq 1 ] && [ -f "${LOG:-}" ]; then
        printf '\n        STOPPED by signal %s while %s ran, after it finished stopping. NO VERDICT, NO RECEIPT.\n' \
            "$1" "${id:-its unit}"
        printf '        Its final output, kept before its folder is removed:\n'
        tail -40 "$LOG" | sed 's/^/          /'
    fi
    exit $((128 + $1))
}
trap ci_shard_cleanup EXIT
trap 'ci_shard_stopped 15' TERM
trap 'ci_shard_stopped 2' INT
trap 'ci_shard_stopped 1' HUP
if [ -f "$SCRIPT_DIR/lib/scratch.sh" ]; then
    # shellcheck source=lib/scratch.sh
    . "$SCRIPT_DIR/lib/scratch.sh"
    LOG_DIR="$(scratch_new ci-shard)" || die "the scratch allocator (lib/scratch.sh) could not allocate this run's folder." 2
    LOG_DIR_ALLOCATED=1
else
    LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ci-shard.XXXXXX")"
fi
tw_pick_mtime "$LOG_DIR"
lc_add_root "$PWD"
lc_add_root "$ENGINE_ROOT"
[ "$(lc_count)" -gt 0 ] || die "the leak canary resolved NO watched root; it would report 'nothing leaked' without looking at anything." 2

SHA="$(sha_now)"
LABEL="${SHARD:+shard $SHARD/$SHARDS}"
printf '%s=== ci-shard: %s unit(s)%s at %s ===%s\n' \
    "$C_BOLD" "$N_SEL" "${LABEL:+, $LABEL}" "$SHA" "$C_RESET"

[ -n "$RECEIPT" ] && { mkdir -p "$(dirname "$RECEIPT")" 2>/dev/null; : > "$RECEIPT"; }

PASSED=0; FAILED=0; KNOWNRED=0
FAIL_LINES=()
i=0
while IFS= read -r id; do
    [ -n "$id" ] || continue
    i=$((i + 1))
    EXPECT_RC="$(awk -F'\t' -v want="$id" '$1 == want { print $3; exit }' "$ALL_UNITS")"
    # Column 4 is the weight the PACKER used for this unit. It is read here so
    # the deadline and the drift report are both stated against the same number
    # the plan was built from, rather than against a second copy of it.
    WEIGHT="$(awk -F'\t' -v want="$id" '$1 == want { print $4; exit }' "$ALL_UNITS")"
    DEADLINE="${CI_SHARD_UNIT_TIMEOUT:-$(deadline_for "${WEIGHT:-0}")}"
    LOG="$LOG_DIR/$i.log"
    printf '  [%3s/%3s] %-72s ' "$i" "$N_SEL" "$id"

    CANARY_DIR="$LOG_DIR/canary.$i"
    LC_HEALTHY=1
    lc_baseline "$CANARY_DIR"
    CANARY_BASE_HEALTHY="$LC_HEALTHY"
    mkdir -p "$CANARY_DIR" 2>/dev/null || true
    state_witness "$CANARY_DIR/state.json" 2>/dev/null || CANARY_BASE_HEALTHY=0
    # The unit gets a record of its own (lib/record-canary.sh, "AMENDED
    # 2026-09-27"): HOME in a throwaway home, every variable pointing into the
    # operator's record removed, and the canary watches the record in that
    # home. Nothing else writes
    # there, so a spawn or a land elsewhere on the machine is never charged to
    # this unit, and a unit that would have written the operator's record is
    # still red.
    UNIT_HOME="$LOG_DIR/home.$i"
    if rc_sandbox "$UNIT_HOME"; then
        rc_baseline "$CANARY_DIR/record.txt"
        RECORD_BASE_HEALTHY="$RC_HEALTHY"
    else
        RECORD_BASE_HEALTHY=0
    fi

    # The argv comes from ci-units.sh, so "how is a unit invoked" has exactly
    # one definition and a section's --only cannot drift from its id.
    ARGV=()
    while IFS= read -r tok; do ARGV+=("$tok"); done < <(bash "$UNITS_SH" cmd "$id" | tr '\t' '\n')
    # THE HOST SHIELD (ceo-decisions §65, lib/host_shield.py): the unit's source is checked
    # first, then it runs with stand-ins for this Mac's power, sleep, lock and display commands
    # first on PATH, and a unit that called one fails with the stand-in's logged line.
    ARGV=(${SHIELDED[@]+"${SHIELDED[@]}"} "${ARGV[@]}")
    if [ "$RECORD_BASE_HEALTHY" -eq 1 ]; then
        ARGV=(env "${RC_SANDBOX_ENV[@]}" "${ARGV[@]}")
    fi

    export -f run_with_deadline
    export SCRIPT_DIR
    TIMING="$LOG_DIR/$i.timing.json"
    export RICHOS_UNIT_DEADLINE_MARKER="$LOG.deadline"
    if [ "$RECORD_BASE_HEALTHY" -eq 1 ]; then
        # A stop signal that lands now is handled when this returns (see
        # ci_shard_stopped), so the folder outlives the unit and its timing file.
        CI_SHARD_UNIT_RUNNING=1
        ${PASS_ARGS[@]+"${PASS_ARGS[@]}"} python3 "$SCRIPT_DIR/lib/worker_tokens.py" machine --timing "$TIMING" -- \
            bash -c 'run_with_deadline "$@"' bash "$DEADLINE" "$LOG" "${ARGV[@]}"
        RC=$?
        CI_SHARD_UNIT_RUNNING=0
    else
        printf "ci-shard.sh: NOT RUN: private home unavailable\n" >"$LOG"
        RC=1
    fi
    SLOT_REFUSED=0
    [ "$RC" -ne 75 ] || [ -f "$TIMING" ] || SLOT_REFUSED=1
    # Admission waits do not spend the unit's execution deadline or inflate
    # the planner's execution weight. Missing timing cannot earn a pass.
    EXECUTION="infrastructure-error"; SECS=0; QUEUED="unknown"
    read -r EXECUTION SECS QUEUED < <(python3 - "$TIMING" "$RC" <<'PY'
import json, math, sys
try:
    row = json.load(open(sys.argv[1]))
    elapsed, queue = row["execution_seconds"], row["admission_seconds"]
    assert all(type(v) in (int, float) and math.isfinite(v) and v >= 0 for v in (elapsed, queue))
    assert type(row["admitted"]) is bool and row["exit"] == int(sys.argv[2])
    outcome = row["outcome"]
    assert outcome in ("completed", "not-admitted", "infrastructure-error", "timed-out")
    assert row["admitted"] or outcome in ("not-admitted", "infrastructure-error")
    print(outcome, round(elapsed, 1), round(queue, 1))
except (OSError, ValueError, KeyError, TypeError, AssertionError):
    print("infrastructure-error 0 unknown")
PY
)
    if [ "$SLOT_REFUSED" -eq 1 ]; then EXECUTION="not-admitted"; fi
    printf '(admission %ss) ' "${QUEUED:-unknown}"

    ESCAPED="$(lc_escaped "$CANARY_DIR" "$LOG_DIR")"
    TOUCHED=""
    [ "$RECORD_BASE_HEALTHY" -eq 1 ] && TOUCHED="$(rc_escaped "$CANARY_DIR/record.txt")"
    STATE_WRITTEN="$(state_written "$CANARY_DIR/state.json" 2>/dev/null || true)"
    rm -rf "$UNIT_HOME" 2>/dev/null || true

    VERDICT=""
    if [ "$EXECUTION" = "not-admitted" ]; then
        VERDICT="NOT-ADMITTED"
    elif [ "$EXECUTION" = "infrastructure-error" ] || [ -z "$EXECUTION" ]; then
        VERDICT="INFRASTRUCTURE-ERROR"
    elif [ "$EXECUTION" = "timed-out" ]; then
        # FIRST, and ahead of the canary: a unit killed mid-flight has almost
        # certainly left residue, so the leak canary would fire too and the
        # report would name the wrong defect. The hang is the finding.
        VERDICT="TIMED-OUT"
    elif [ "$CANARY_BASE_HEALTHY" -ne 1 ] || [ "$RECORD_BASE_HEALTHY" -ne 1 ]; then
        VERDICT="CANARY-BLIND"
    elif [ -n "$TOUCHED" ]; then
        VERDICT="RECORD-TOUCHED"
    elif [ -n "$ESCAPED" ]; then
        VERDICT="LEAKED"
    elif [ -n "$STATE_WRITTEN" ]; then
        VERDICT="STATE-WRITTEN"
    elif [ "$EXPECT_RC" = "3" ] && [ "$RC" -eq 0 ]; then
        # Named as its own verdict rather than folded into FAIL: this one does
        # not mean the assertions failed, it means the SCOPING did not apply
        # and the unit ran something other than what its id says.
        VERDICT="SCOPE-LOST"
    elif [ "$RC" -eq "${EXPECT_RC:-0}" ]; then
        VERDICT="PASS"
    else
        VERDICT="FAIL"
    fi

    # known-red only ever RE-LABELS a failure, and only when the entry is live
    if kr_declared "$id"; then
        if kr_expired "$id"; then
            VERDICT="KNOWN-RED-EXPIRED"
        elif [ "$VERDICT" = "PASS" ]; then
            VERDICT="KNOWN-RED-BUT-PASSED"
        elif [ "$VERDICT" = "FAIL" ]; then
            VERDICT="KNOWN-RED"
        fi
    fi

    case "$VERDICT" in
        NOT-ADMITTED|INFRASTRUCTURE-ERROR)
            printf '%s%s%s %ss (rc=%s)\n' "$C_RED" "$VERDICT" "$C_RESET" "$SECS" "$RC"
            FAILED=$((FAILED + 1))
            FAIL_LINES+=("$id — $VERDICT; no completed test verdict, not accepted coverage")
            ;;
        PASS)
            printf '%sPASS%s %ss\n' "$C_GREEN" "$C_RESET" "$SECS"
            PASSED=$((PASSED + 1))
            report_weight_drift "$id" "$SECS" "${WEIGHT:-0}"
            [ "$VERBOSE" -eq 1 ] && sed 's/^/        /' "$LOG"
            ;;
        TIMED-OUT)
            printf '%sFAIL%s %ss — DEADLINE (%ss) reached, killed\n' "$C_RED" "$C_RESET" "$SECS" "$DEADLINE"
            FAILED=$((FAILED + 1))
            FAIL_LINES+=("$id — did not finish within its ${DEADLINE}s deadline and was killed. Its planned weight is ${WEIGHT:-unknown}s (lib/ci-unit-weights.tsv); either it hung, or it has grown past 3x its measurement and the weight is now wrong. The last lines of its output are above.")
            printf '        NO VERDICT WAS REACHED — this unit did not fail an assertion, it never finished.\n'
            printf '        Its final output before the kill:\n'
            tail -25 "$LOG" | sed 's/^/          /'
            ;;
        KNOWN-RED)
            printf '%sKNOWN-RED%s %ss (expires %s)\n' "$C_YEL" "$C_RESET" "$SECS" "$(kr_field "$id" 3)"
            KNOWNRED=$((KNOWNRED + 1))
            printf '        declared %s, broken by %s: %s\n' \
                "$(kr_field "$id" 2)" "$(kr_field "$id" 4)" "$(kr_field "$id" 6)"
            CONDW="$(kr_field "$id" 7 2>/dev/null || true)"
            [ -n "$CONDW" ] && printf '        applies only where: %s\n' "$CONDW"
            printf '        failing: %s\n' "$(kr_field "$id" 5)"
            ;;
        KNOWN-RED-EXPIRED)
            printf '%sFAIL%s %ss — known-red entry EXPIRED\n' "$C_RED" "$C_RESET" "$SECS"
            FAILED=$((FAILED + 1))
            FAIL_LINES+=("$id — its known-red entry expired on $(kr_field "$id" 3). The tolerance was time-boxed when it was granted: either fix the suite, or re-declare the entry with a new expiry and a reason. Broken by $(kr_field "$id" 4).")
            ;;
        KNOWN-RED-BUT-PASSED)
            printf '%sFAIL%s %ss — declared red, but it PASSED\n' "$C_RED" "$C_RESET" "$SECS"
            FAILED=$((FAILED + 1))
            FAIL_LINES+=("$id — lib/ci-known-red.tsv declares this unit red and it PASSED. The defect is fixed; DELETE the entry. A known-red table that outlives its defects is how a skip becomes permanent.")
            ;;
        SCOPE-LOST)
            printf '%sFAIL%s %ss — scoped run exited 0\n' "$C_RED" "$C_RESET" "$SECS"
            FAILED=$((FAILED + 1))
            FAIL_LINES+=("$id — a scoped section exited 0, which it must never do: 3 is its green. The --only selector did not apply, so this unit did not run what its id says.")
            sed 's/^/        /' "$LOG"
            ;;
        LEAKED)
            printf '%sFAIL%s %ss — wrote outside its sandbox\n' "$C_RED" "$C_RESET" "$SECS"
            FAILED=$((FAILED + 1))
            FAIL_LINES+=("$id — wrote outside its sandbox")
            printf '%s\n' "$ESCAPED" | while IFS="$(printf '\t')" read -r croot centry; do
                if lc_is_tracked_change "$centry"; then
                    printf '        TRACKED FILE CHANGED DURING THE RUN — every unit after this one tested different code:\n'
                else
                    printf '        WROTE OUTSIDE ITS SANDBOX — residue a stranger will have to explain:\n'
                fi
                printf '          %s  (under %s)\n' "$centry" "$croot"
            done
            ;;
        STATE-WRITTEN)
            printf '%sFAIL%s %ss — STATE-WRITTEN: wrote the engine'"'"'s own runtime state\n' "$C_RED" "$C_RESET" "$SECS"
            FAILED=$((FAILED + 1))
            FAIL_LINES+=("$id — STATE-WRITTEN: wrote into $STATE_WITNESS_DIR, the live plugin's audit record and an input of every engine check beside it. Seat its hooks on a sandbox entity (scripts/lib/entity-sandbox.sh).")
            printf '        WROTE INTO %s WHILE IT RAN (a unit in another lane at the same moment would show here too):\n' "$STATE_WITNESS_DIR"
            printf '%s\n' "$STATE_WRITTEN" | sed 's/^/          /'
            ;;
        RECORD-TOUCHED)
            printf '%sFAIL%s %ss — RECORD-TOUCHED: wrote to the record it was given, which outside this runner is the operator'"'"'s\n' "$C_RED" "$C_RESET" "$SECS"
            FAILED=$((FAILED + 1))
            FAIL_LINES+=("$id — RECORD-TOUCHED: wrote to its HOME/CLAUDE_CONFIG_DIR record (run by hand, that is $RC_LIVE_CFG)")
            printf '        WROTE TO THE RECORD UNDER ITS OWN HOME — run by hand it would have written the operator'"'"'s, the class that wrote a false termination for a running agent on 2026-09-11:\n'
            printf '%s\n' "$TOUCHED" | sed 's/^/          /'
            ;;
        CANARY-BLIND)
            printf '%sFAIL%s %ss — canary blind\n' "$C_RED" "$C_RESET" "$SECS"
            FAILED=$((FAILED + 1))
            FAIL_LINES+=("$id — a canary could not witness one of its roots (leak: $CANARY_BASE_HEALTHY, record: $RECORD_BASE_HEALTHY), so this is NOT reported as a pass")
            ;;
        *)
            printf '%sFAIL%s %ss (rc=%s, expected %s)\n' "$C_RED" "$C_RESET" "$SECS" "$RC" "$EXPECT_RC"
            FAILED=$((FAILED + 1))
            FAIL_LINES+=("$id (rc=$RC, expected $EXPECT_RC)")
            sed 's/^/        /' "$LOG"
            ;;
    esac

    if [ -n "$RECEIPT" ]; then
        UNIT_ID="$id" UNIT_RC="$RC" UNIT_EXP="${EXPECT_RC:-0}" UNIT_VERDICT="$VERDICT" \
        UNIT_SECS="$SECS" UNIT_ADMISSION="${QUEUED:-}" UNIT_EXECUTION="$EXECUTION" \
        UNIT_SHARD="${SHARD:-0}" UNIT_SHARDS="$SHARDS" UNIT_SHA="$SHA" \
        python3 "$SCRIPT_DIR/lib/ci-receipts.py" emit >> "$RECEIPT"
    fi
    if [ "$SLOT_REFUSED" -eq 1 ]; then
        printf 'ci-shard: slot admission refused; %s remaining unit(s) were not admitted and remain unrun.\n' "$((N_SEL - i))"
        break
    fi
    # Contamination is independent of the assertion verdict (including a timeout).
    # Stop this domain even in keep-going mode and notify concurrent sibling shards.
    if [ "$CANARY_BASE_HEALTHY" -ne 1 ] || [ "$RECORD_BASE_HEALTHY" -ne 1 ] \
       || [ -n "$TOUCHED" ] || [ -n "$ESCAPED" ]; then
        if [ -n "${RICHOS_VERIFICATION_CONTAMINATION:-}" ]; then
            mkdir -p "$RICHOS_VERIFICATION_CONTAMINATION"
            UNIT_ID="$id" UNIT_VERDICT="$VERDICT" python3 - "$RICHOS_VERIFICATION_CONTAMINATION" <<'PY'
import json, os, sys, tempfile
fd, path = tempfile.mkstemp(prefix="unit-", suffix=".json", dir=sys.argv[1])
with os.fdopen(fd, "w") as out:
    json.dump({"unit": os.environ["UNIT_ID"], "verdict": os.environ["UNIT_VERDICT"],
               "reason": "canary reported contamination or could not establish safety"}, out)
PY
        fi
        printf 'ci-shard: contaminated execution domain; remaining units are unrun.\n'
        break
    fi
    if [ "$FAIL_FAST" -eq 1 ] && [ "$FAILED" -gt 0 ]; then
        printf 'ci-shard: stopping after the first failed unit; unfinished units have no passing receipt.\n'
        break
    fi
done < "$SELECTED"

echo ""
if [ "$FAILED" -eq 0 ]; then
    printf '%s✓ ci-shard%s: %s/%s unit(s) passed' "$C_GREEN" "${LABEL:+ $LABEL}" "$PASSED" "$N_SEL"
    [ "$KNOWNRED" -gt 0 ] && printf ', %s declared KNOWN-RED (see lib/ci-known-red.tsv)' "$KNOWNRED"
    printf '.%s\n' "$C_RESET"
    exit 0
fi
printf '%s✗ ci-shard%s: %s/%s unit(s) passed, %s FAILED.%s\n' \
    "$C_RED" "${LABEL:+ $LABEL}" "$PASSED" "$N_SEL" "$FAILED" "$C_RESET" >&2
for l in "${FAIL_LINES[@]}"; do printf '    - %s\n' "$l" >&2; done
exit 1
