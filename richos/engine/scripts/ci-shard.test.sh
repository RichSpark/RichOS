#!/usr/bin/env bash
#
# ci-shard.test.sh — the verdict rules, and the coverage proof, by execution.
#
# EVERY CASE HERE RUNS AGAINST A SYNTHETIC ENGINE built in a sandbox, not
# against this tree. That is deliberate: the properties being proven are about
# a shard that runs FEWER units than it was given, a section whose scoping
# stopped applying, a known-red entry that outlived its defect. None of those
# can be arranged in the real tree without breaking it, and a property that can
# only be proven by breaking production is a property nobody proves.
#
# WHAT IS PROVEN:
#
#   S1   A suite unit is green at exit 0.
#   S2   A SECTION unit is green at exit 3, which is that suite's deliberate
#        scoped-green code.
#   S3   A section unit that exits 0 is a FAILURE, reported as SCOPE-LOST. Exit
#        0 from a scoped invocation means the --only selector did not apply, so
#        the unit ran something other than what its id says — the one thing a
#        scoped run must never be able to claim.
#   S4   An empty selection is REFUSED (exit 2) unless --allow-empty is given.
#        A shard that verifies nothing must never exit 0.
#   S5   A unit id that is not in the inventory is fatal, not skipped.
#   S6   KNOWN-RED: a declared failing unit does not fail the job, and its
#        expiry and reason are printed.
#   S7   THE NEGATIVE CONTROL: a declared unit that PASSES fails the job. Without
#        this the table outlives its defects and becomes a permanent skip list.
#   S8   An entry past its expiry fails the job even while still failing.
#   S9   A receipt is written per unit, carrying the unit id, the verdict, the
#        duration and the COMMIT.
#   S10  --verify-receipts passes when the receipts union to the plan.
#   S11  --verify-receipts FAILS and NAMES the unit when one is missing — the
#        dropped-shard case, which is the whole reason receipts exist.
#   S12  --verify-receipts FAILS when the receipts carry two different commits.
#   S13  --verify-receipts FAILS when a unit has two receipts.
#   S14  --verify-receipts FAILS on an empty receipt set rather than certifying
#        a plan against nothing.
#   S18  A CONDITIONAL known-red row applies only where its predicate holds. In
#        force it behaves as S6; dormant, the unit is judged NORMALLY — rule 2
#        included, so a host without the defect still gets a real verdict rather
#        than a tolerated one. A predicate that cannot be evaluated is treated as
#        APPLYING and says so, because failing open would rebuild the skip list
#        one broken expression at a time.
#   S16  --units-file WITH --shard packs that set and runs shard i of it. This
#        is the affected gate on a large diff: this work's own first push
#        selected 54 units costing ~100 minutes serial, which would have been
#        killed at the job's 60-minute timeout.
#   S17  --verify-receipts --units-file certifies the RESTRICTED plan, and the
#        same receipts checked against the WHOLE inventory correctly FAIL. Both
#        directions, because a coverage check that passes against any plan
#        certifies nothing.
#   S15  A unit that writes outside its sandbox is caught and named, so the
#        leak canary is not lost by sharding. This is the 2026-09-05
#        escalations.test.sh finding, which run-all-tests.sh catches per suite;
#        splitting the pass must not silently drop it.
#   S15b-S15h  THE RECORD CANARY COUNTS ONLY THE UNIT'S OWN WRITES. Each unit
#        runs with HOME in a throwaway home and every variable pointing into
#        the operator's record removed, and the canary watches the record in
#        that home. Positive controls: a write
#        through CLAUDE_CONFIG_DIR, through $HOME alone and through the
#        workspace registry are each RED and named (S15b-d), and none reaches
#        the operator's record (S15b'). The 2026-09-27 defect: a clean unit
#        PASSES while another process writes the live record during it (S15e).
#        The unit gets no road back (S15f), keeps a git identity (S15g), and a
#        unit that builds a world of its own keeps the resolution it chose (S15h).
#        Codex's reproduction 337: a unit whose only act is scratch_new/release
#        creates its first scratch ledger in its own home, never in the
#        operator's config/state (S15i).
#   S24  A STOPPED RUN NEVER DELETES ITS OWN FOLDER UNDER ITS UNIT (2026-10-02).
#        The merge gate's stop reached ci-shard.sh and worker_tokens.py at the
#        same moment; the shell's EXIT trap removed the folder while
#        worker_tokens.py was still stopping the unit, its timing write died with
#        FileNotFoundError, and the unit's own log went with the folder. Here the
#        run is started under the gate's own supervisor (proc_tree.py run) and
#        stopped the way the gate stops it (TERM to the supervisor); the folder
#        must outlive the unit's own TERM cleanup, the unit's last output must
#        be printed, no receipt may be written, and the folder must come from
#        the scratch allocator with this run's pid as its owner and be released
#        afterward.
#   S25  THE ENGINE'S OWN RUNTIME STATE IS NOT A UNIT'S TO WRITE (2026-10-05). A
#        unit that appends to <engine>/.claude/state/ fails as STATE-WRITTEN,
#        named with the line it added; the same unit seated on
#        lib/entity-sandbox.sh passes and leaves that folder byte-for-byte as it
#        was; an untouched state folder is no finding.
#
# Exit 0 = all cases pass; exit 1 = at least one failure. A scoped run
# (--contamination-only, --stop-only, --state-only) exits 3 when its cases pass.

set -uo pipefail
case "${1:-}" in
    "") ;;
    --contamination-only|--stop-only|--state-only) [ "$#" -eq 1 ] || exit 2 ;;
    *) echo "usage: ci-shard.test.sh [--contamination-only | --stop-only | --state-only]" >&2; exit 2 ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

PASS=0; FAIL=0
SANDBOX="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/ci-shard-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

for f in ci-shard.sh ci-units.sh lib/ci-receipts.py lib/leak-canary.sh lib/record-canary.sh lib/tree-witness.sh lib/proc_tree.py lib/operator_fences.py lib/worker_tokens.py lib/engine_pass.py lib/machine_settings.py lib/scratch.sh; do
    [ -f "$ENGINE_ROOT/scripts/$f" ] || { echo "FATAL: missing scripts/$f" >&2; exit 1; }
done
command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }
# The shard runner's RECORD canary (round 15) watches ${CLAUDE_CONFIG_DIR:-$HOME/.claude};
# every invocation below points it at a throwaway config directory.
export CLAUDE_CONFIG_DIR="$SANDBOX/cfg"
export RICHOS_MACHINE_WORKERS="$SANDBOX/machine" RICHOS_ENGINE_PASS_DIR="$SANDBOX/slot"
unset RICHOS_WORKER_TOKENS RICHOS_WORKER_SLOT_HELD RICHOS_WORKER_BORROW_LOCK
# The cases below contaminate their synthetic shards ON PURPOSE (S15's leaky unit, the
# record-canary cases). Under proof-run.py this variable names the REAL run's
# contamination directory, so an inherited value made S15's fixture report itself there
# and cancel every check of the run (2026-09-28). S23 sets its own below.
unset RICHOS_VERIFICATION_CONTAMINATION
mkdir -p "$CLAUDE_CONFIG_DIR/state"
# Every synthetic engine below has no host shield of its own, and ci-shard.sh refuses to start a
# unit without one: each run gets this checkout's shield, so every fixture unit runs behind the
# stand-ins (S26c proves the refusal with it withheld).
export RICHOS_HOST_SHIELD_TOOL="$ENGINE_ROOT/scripts/lib/host_shield.py"

echo "=== ci-shard tests ==="

# ---------------------------------------------------------------------------
# a synthetic engine: the minimum ci-shard.sh + ci-units.sh recognize
# ---------------------------------------------------------------------------
mk_engine() { # <root>
    local r="$1"
    mkdir -p "$r/scripts/lib" "$r/scripts/hooks"
    printf '1.0.0-test\n' > "$r/VERSION"
    for f in ci-shard.sh ci-units.sh; do
        cp "$ENGINE_ROOT/scripts/$f" "$r/scripts/$f"; chmod +x "$r/scripts/$f"
    done
    for f in ci-receipts.py leak-canary.sh record-canary.sh tree-witness.sh proc_tree.py operator_fences.py worker_tokens.py engine_pass.py machine_settings.py scratch.sh; do
        cp "$ENGINE_ROOT/scripts/lib/$f" "$r/scripts/lib/$f"
    done
    # The sectioned suite, with two real `if _section` markers, so the section
    # units are discovered the same way they are in the real tree. `--only`
    # exits 3 when green and 1 when its selected section fails, mirroring the
    # contract that matters here.
    cat > "$r/scripts/hooks/contract-integrity.test.sh" <<'SECTIONED'
#!/usr/bin/env bash
set -uo pipefail
# The markers have to be REAL: `ci-units.sh` reads section ids with
#   awk '/^if _section [A-Za-z0-9_.-]+; then$/ ...'
# so the `then` must end the line, exactly as it does in the shipped suite. A
# one-line `if _section x; then :; fi` shape parses as bash and is invisible to
# that awk, which is a fixture that silently tests nothing.
_section() { return 1; }
SEL=""
if [ "${1:-}" = "--list" ]; then printf 'alpha\nbeta\n'; exit 0; fi
[ "${1:-}" = "--only" ] && SEL="${2:-}"
if _section alpha; then
    :
fi  # _section
if _section beta; then
    :
fi  # _section
[ -n "$SEL" ] || exit 0
[ "${SECTION_FAILS:-}" = "$SEL" ] && exit 1
[ "${SECTION_EXITS_ZERO:-}" = "$SEL" ] && exit 0
exit 3
SECTIONED
    chmod +x "$r/scripts/hooks/contract-integrity.test.sh"
}
mk_suite() { printf '#!/usr/bin/env bash\nexit %s\n' "${2:-0}" > "$1"; chmod +x "$1"; }

E="$SANDBOX/engine"; mk_engine "$E"
mk_suite "$E/scripts/lib/green.test.sh" 0
mk_suite "$E/scripts/lib/red.test.sh" 1
SH="$E/scripts/ci-shard.sh"

run_shard() { # captures stdout+stderr to $SANDBOX/out, echoes the rc
    local out="$SANDBOX/out"
    bash "$SH" "$@" > "$out" 2>&1
    printf '%s' "$?"
}

# --- S1 / S2 ---------------------------------------------------------------
if [ -z "${1:-}" ]; then
RC="$(run_shard --only-units scripts/lib/green.test.sh)"
if [ "$RC" = "0" ] && grep -q 'PASS' "$SANDBOX/out"; then
    ok "S1   a suite unit is green at exit 0"
else
    bad "S1   rc=$RC"; sed 's/^/          /' "$SANDBOX/out"
fi
RC="$(run_shard --only-units 'scripts/hooks/contract-integrity.test.sh:alpha')"
if [ "$RC" = "0" ] && grep -q 'PASS' "$SANDBOX/out"; then
    ok "S2   a section unit is green at exit 3, that suite's scoped-green code"
else
    bad "S2   rc=$RC"; sed 's/^/          /' "$SANDBOX/out"
fi

# --- S3: the scoping stopped applying --------------------------------------
SECTION_EXITS_ZERO=alpha
export SECTION_EXITS_ZERO
RC="$(run_shard --only-units 'scripts/hooks/contract-integrity.test.sh:alpha')"
unset SECTION_EXITS_ZERO
if [ "$RC" = "1" ] && grep -q 'SCOPE-LOST\|scoped run exited 0' "$SANDBOX/out"; then
    ok "S3   a scoped section that exits 0 FAILS as SCOPE-LOST — it ran something other than its id"
else
    bad "S3   rc=$RC — exit 0 from a scoped run was accepted, so a selector that stopped matching reads as green"
    sed 's/^/          /' "$SANDBOX/out"
fi

# --- S4: emptiness ---------------------------------------------------------
: > "$SANDBOX/empty-units.txt"
RC="$(run_shard --units-file "$SANDBOX/empty-units.txt")"
if [ "$RC" = "2" ] && grep -q 'no units selected' "$SANDBOX/out"; then
    ok "S4a  an empty selection exits 2 — a shard that verifies nothing never exits 0"
else
    bad "S4a  rc=$RC on an empty selection"
fi
RC="$(run_shard --units-file "$SANDBOX/empty-units.txt" --allow-empty)"
if [ "$RC" = "0" ] && grep -q 'Nothing was verified' "$SANDBOX/out"; then
    ok "S4b  --allow-empty exits 0 but SAYS nothing was verified"
else
    bad "S4b  rc=$RC with --allow-empty"
fi

# --- S5: an unknown unit is fatal -----------------------------------------
RC="$(run_shard --only-units scripts/lib/does-not-exist.test.sh)"
if [ "$RC" = "2" ] && grep -q 'is not a unit' "$SANDBOX/out"; then
    ok "S5   a unit id absent from the inventory is fatal, never silently skipped"
else
    bad "S5   rc=$RC for an unknown unit id"
fi

# S5b (P5-78): --only-units together with an unreadable --units-file is fatal. `die`
# used to run in the pipeline's child, after the inline ids were emitted, and the run
# went on with that PARTIAL selection and exit 0.
RC="$(run_shard --list --only-units scripts/lib/green.test.sh --units-file "$SANDBOX/no-such-units-file.txt")"
if [ "$RC" != "0" ] && grep -q 'no such file' "$SANDBOX/out" && ! grep -q '^scripts/lib/green.test.sh$' "$SANDBOX/out"; then
    ok "S5b  --only-units plus a missing --units-file is fatal (rc=$RC) and lists no partial selection"
else
    bad "S5b  rc=$RC — a missing units file still produced a partial plan"; sed 's/^/          /' "$SANDBOX/out"
fi

# --- S6 / S7 / S8: the known-red table ------------------------------------
KR="$E/scripts/lib/ci-known-red.tsv"
FUTURE="$(python3 -c 'import datetime; print((datetime.date.today() + datetime.timedelta(days=30)).isoformat())')"
PAST="$(python3 -c 'import datetime; print((datetime.date.today() - datetime.timedelta(days=1)).isoformat())')"

printf 'scripts/lib/red.test.sh\t2026-01-01\t%s\tdeadbeef\tC1 C2\tbecause the contract moved\n' "$FUTURE" > "$KR"
RC="$(run_shard --only-units scripts/lib/red.test.sh)"
if [ "$RC" = "0" ] && grep -q 'KNOWN-RED' "$SANDBOX/out" && grep -q 'because the contract moved' "$SANDBOX/out"; then
    ok "S6   a declared failing unit is KNOWN-RED, does not fail the job, and prints its reason and expiry"
else
    bad "S6   rc=$RC"; sed 's/^/          /' "$SANDBOX/out"
fi

printf 'scripts/lib/green.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\tstale entry\n' "$FUTURE" > "$KR"
RC="$(run_shard --only-units scripts/lib/green.test.sh)"
if [ "$RC" = "1" ] && grep -q 'declares this unit red and it PASSED' "$SANDBOX/out"; then
    ok "S7   NEGATIVE CONTROL: a declared unit that PASSES fails the job, so the table cannot outlive its defect"
else
    bad "S7   rc=$RC — a stale known-red row was tolerated, which is how a skip list becomes permanent"
    sed 's/^/          /' "$SANDBOX/out"
fi

printf 'scripts/lib/red.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\texpired tolerance\n' "$PAST" > "$KR"
RC="$(run_shard --only-units scripts/lib/red.test.sh)"
if [ "$RC" = "1" ] && grep -q 'EXPIRED' "$SANDBOX/out"; then
    ok "S8   an entry past its expiry fails the job — the tolerance was time-boxed when it was granted"
else
    bad "S8   rc=$RC on an expired entry"; sed 's/^/          /' "$SANDBOX/out"
fi

# S8b: the expiry is read on the OPERATOR'S calendar (local), never UTC. Pinned with two
# zones at the extremes (UTC+14 and UTC-12): at every hour of the day at least one of them is
# on a different calendar date from UTC, which is what made S8 fail 00:00-01:00 BST. In each
# zone, an expiry of local yesterday is expired and an expiry of local today is still live.
for TZNAME in Pacific/Kiritimati Etc/GMT+12; do
    LTODAY="$(TZ=$TZNAME python3 -c 'import datetime; print(datetime.date.today().isoformat())')"
    LYEST="$(TZ=$TZNAME python3 -c 'import datetime; print((datetime.date.today() - datetime.timedelta(days=1)).isoformat())')"
    printf 'scripts/lib/red.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\texpired tolerance\n' "$LYEST" > "$KR"
    RC="$(TZ=$TZNAME run_shard --only-units scripts/lib/red.test.sh)"
    if [ "$RC" = "1" ] && grep -q 'EXPIRED' "$SANDBOX/out"; then
        ok "S8b  $TZNAME: local yesterday is expired"
    else
        bad "S8b  $TZNAME: local yesterday ($LYEST) not expired, rc=$RC"
    fi
    printf 'scripts/lib/red.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\tlive tolerance\n' "$LTODAY" > "$KR"
    RC="$(TZ=$TZNAME run_shard --only-units scripts/lib/red.test.sh)"
    if [ "$RC" = "0" ] && grep -q 'KNOWN-RED' "$SANDBOX/out" && ! grep -q 'EXPIRED' "$SANDBOX/out"; then
        ok "S8b  $TZNAME: an expiry of local today is still live"
    else
        bad "S8b  $TZNAME: local today ($LTODAY) treated as expired, rc=$RC"
    fi
done
# S8c (P5-77): an expiry that is not a real calendar date is not a time boundary; it
# used to compare as a string and keep the excuse alive indefinitely.
for BADEXP in tomorrow 2999-99-99 2026-02-30 20991231; do
    printf 'scripts/lib/red.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\tbad expiry\n' "$BADEXP" > "$KR"
    RC="$(run_shard --only-units scripts/lib/red.test.sh)"
    if [ "$RC" = "1" ] && grep -q 'EXPIRED' "$SANDBOX/out"; then
        ok "S8c  an entry whose expiry is '$BADEXP' (not a valid YYYY-MM-DD date) fails the job as expired"
    else
        bad "S8c  expiry '$BADEXP' kept a red unit excused (rc=$RC)"; sed 's/^/          /' "$SANDBOX/out"
    fi
done
rm -f "$KR"

# --- S9: receipts ---------------------------------------------------------
R="$SANDBOX/receipts"
mkdir -p "$R"
bash "$SH" --only-units scripts/lib/green.test.sh --receipt "$R/a.jsonl" >/dev/null 2>&1
if [ -s "$R/a.jsonl" ] && python3 - "$R/a.jsonl" <<'PY'
import json, sys
rec = json.loads(open(sys.argv[1]).readline())
need = {"unit", "rc", "expected_rc", "verdict", "seconds", "shard", "shards", "sha"}
sys.exit(0 if need <= set(rec) and rec["unit"] == "scripts/lib/green.test.sh" else 1)
PY
then
    ok "S9   a receipt carries the unit, the verdict, the duration and the commit"
else
    bad "S9   the receipt is missing or malformed"; cat "$R/a.jsonl" 2>/dev/null | sed 's/^/          /'
fi

# --- S10..S14: the coverage proof -----------------------------------------
# The full plan for this synthetic engine, run for real, then mutilated.
rm -rf "$R"; mkdir -p "$R"
bash "$SH" --receipt "$R/all.jsonl" >/dev/null 2>&1 || true
bash "$SH" --verify-receipts "$R" > "$SANDBOX/out" 2>&1
RC=$?
if [ "$RC" != "0" ]; then
    # red.test.sh is genuinely red here, so declare it and re-take the baseline
    printf 'scripts/lib/red.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\tred by design in this fixture\n' "$FUTURE" > "$KR"
    rm -rf "$R"; mkdir -p "$R"
    bash "$SH" --receipt "$R/all.jsonl" >/dev/null 2>&1
    bash "$SH" --verify-receipts "$R" > "$SANDBOX/out" 2>&1
    RC=$?
fi
if [ "$RC" = "0" ] && grep -q 'planned unit(s) ran, all green' "$SANDBOX/out"; then
    ok "S10  --verify-receipts passes when the receipts union to the whole plan at one commit"
else
    bad "S10  rc=$RC"; sed 's/^/          /' "$SANDBOX/out"
fi

# S11 — the dropped shard: drop one unit's receipt
DROPPED="$(head -1 "$R/all.jsonl" | python3 -c 'import json,sys; print(json.loads(sys.stdin.readline())["unit"])')"
python3 - "$R/all.jsonl" "$DROPPED" <<'PY'
import json, sys
path, drop = sys.argv[1], sys.argv[2]
keep = [l for l in open(path) if l.strip() and json.loads(l)["unit"] != drop]
open(path, "w").writelines(keep)
PY
bash "$SH" --verify-receipts "$R" > "$SANDBOX/out" 2>&1
RC=$?
if [ "$RC" = "1" ] && grep -qF "$DROPPED" "$SANDBOX/out" && grep -q 'have NO receipt' "$SANDBOX/out"; then
    ok "S11  a missing receipt FAILS and NAMES the unit ($DROPPED) — the dropped-shard case"
else
    bad "S11  rc=$RC — a unit nothing ran was not detected, so twelve green jobs would certify a partial pass"
    sed 's/^/          /' "$SANDBOX/out"
fi

# S12 — two commits
rm -rf "$R"; mkdir -p "$R"
bash "$SH" --receipt "$R/all.jsonl" >/dev/null 2>&1
python3 - "$R/all.jsonl" <<'PY'
import json, sys
path = sys.argv[1]
lines = [json.loads(l) for l in open(path) if l.strip()]
lines[0]["sha"] = "0000000000000000000000000000000000000000"
with open(path, "w") as fh:
    for rec in lines:
        fh.write(json.dumps(rec, sort_keys=True, separators=(",", ":")) + "\n")
PY
bash "$SH" --verify-receipts "$R" > "$SANDBOX/out" 2>&1
RC=$?
if [ "$RC" = "1" ] && grep -q 'did not all verify the SAME commit' "$SANDBOX/out"; then
    ok "S12  receipts from two different commits FAIL — a union across two trees certifies neither"
else
    bad "S12  rc=$RC on mixed commits"; sed 's/^/          /' "$SANDBOX/out"
fi

# S13 — a duplicated unit
rm -rf "$R"; mkdir -p "$R"
bash "$SH" --receipt "$R/all.jsonl" >/dev/null 2>&1
head -1 "$R/all.jsonl" > "$R/dupe.jsonl"
bash "$SH" --verify-receipts "$R" > "$SANDBOX/out" 2>&1
RC=$?
if [ "$RC" = "1" ] && grep -q 'more than one receipt' "$SANDBOX/out"; then
    ok "S13  a unit with two receipts FAILS — either the packing overlapped or a receipt was collected twice"
else
    bad "S13  rc=$RC on a duplicated receipt"; sed 's/^/          /' "$SANDBOX/out"
fi

# S14 — nothing at all
rm -rf "$R"; mkdir -p "$R"
bash "$SH" --verify-receipts "$R" > "$SANDBOX/out" 2>&1
RC=$?
if [ "$RC" = "1" ] && grep -q 'NO receipts at all' "$SANDBOX/out"; then
    ok "S14  an empty receipt set FAILS rather than certifying the plan against nothing"
else
    bad "S14  rc=$RC on an empty receipt directory"; sed 's/^/          /' "$SANDBOX/out"
fi
rm -f "$KR"

# --- S15: the leak canary survives sharding -------------------------------
# A suite that writes into the directory the runner was started from is exactly
# the 2026-09-05 escalations.test.sh finding. run-all-tests.sh catches it per
# suite; sharding must not lose that.
LEAKDIR="$SANDBOX/leak-cwd"
mkdir -p "$LEAKDIR"
cat > "$E/scripts/lib/leaky.test.sh" <<'LEAKY'
#!/usr/bin/env bash
printf 'residue a stranger will have to explain\n' > "./leaked-fixture.txt"
exit 0
LEAKY
chmod +x "$E/scripts/lib/leaky.test.sh"
( cd "$LEAKDIR" && bash "$SH" --only-units scripts/lib/leaky.test.sh > "$SANDBOX/out" 2>&1 )
RC=$?
if [ "$RC" = "1" ] && grep -q 'outside its sandbox' "$SANDBOX/out"; then
    ok "S15  a unit that writes outside its sandbox is caught and named — the canary is not lost by sharding"
else
    bad "S15  rc=$RC — the per-unit leak canary did not fire"; sed 's/^/          /' "$SANDBOX/out"
fi
rm -f "$E/scripts/lib/leaky.test.sh"

# --- S15b-S15h: the RECORD canary counts only the unit's OWN writes --------
# Every case below runs the shard under an "operator" of its own: HOME and
# CLAUDE_CONFIG_DIR point at $OPHOME, which stands for the real ~/.claude. So a
# case that goes wrong writes into this sandbox, never into the operator's
# real record.
#
# THE DEFECT (2026-09-27): the canary compared the LIVE record before and after
# each unit, so anything else on the machine that wrote there during the unit
# (a spawn, a land, a message) failed the unit as RECORD-TOUCHED, and engine
# proofs could only pass on an idle session. Each unit now runs with HOME in a
# throwaway home and every variable pointing into the operator's record removed
# (CLAUDE_CONFIG_DIR and RICHOS_WORKSPACES_DIR included, so their defaults
# follow HOME), and the canary watches THAT: a unit that would have written the operator's
# record writes the throwaway one and is still red (S15b-S15d, the positive
# controls), and a concurrent writer to the live record is no longer charged to
# the unit (S15e).
OPHOME="$SANDBOX/ophome"
OPCFG="$OPHOME/.claude"
FLAGS="$SANDBOX/flags"
mkdir -p "$OPCFG/state" "$OPCFG/teams/session-aaaaaaaa" "$FLAGS"
# the operator's git configuration, with a marker S15g asserts the unit cannot see
printf '[user]\n\tname = operator\n\temail = operator@example.invalid\n[richos]\n\tmarker = operator-gitconfig\n' > "$OPHOME/.gitconfig"
op_shard() { # <argv...> — run the shard as that operator; output to $SANDBOX/out, echoes the rc
    (
        export HOME="$OPHOME"
        export CLAUDE_CONFIG_DIR="$OPCFG"
        export RICHOS_WORKSPACES_DIR="$OPCFG/state/workspaces"
        export RICHOS_TEST_DEVICES_DIR="$OPCFG/state/test-devices"
        cd "$LEAKDIR" && bash "$SH" "$@"
    ) > "$SANDBOX/out" 2>&1
    printf '%s' "$?"
}
mk_unit() { # <name> — the unit's body on stdin
    cat > "$E/scripts/lib/$1"; chmod +x "$E/scripts/lib/$1"
}

# S15b — POSITIVE CONTROL: the 2026-09-11 shape, a `terminated` row appended to
# the ledger the unit resolves from its own environment, exactly as the shipped
# reaper resolved it. Red, named, the row printed.
mk_unit toucher.test.sh <<'TOUCHER'
#!/usr/bin/env bash
cfg="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
mkdir -p "$cfg/state"
printf '{"event": "terminated", "agent_id": "x", "teammate": "fixture", "witness": "platform-terminal-record", "ts": "t"}\n' >> "$cfg/state/worktree-ledger.jsonl"
exit 0
TOUCHER
RC="$(op_shard --only-units scripts/lib/toucher.test.sh)"
if [ "$RC" = "1" ] && grep -q "RECORD-TOUCHED\|touched the operator" "$SANDBOX/out" && grep -q 'event=terminated' "$SANDBOX/out"; then
    ok "S15b POSITIVE CONTROL: a unit that appends a terminated row to the ledger under its CLAUDE_CONFIG_DIR is caught, named, and the row printed"
else
    bad "S15b rc=$RC — the per-unit record canary did not fire on a ledger write"; sed 's/^/          /' "$SANDBOX/out"
fi
rm -f "$E/scripts/lib/toucher.test.sh"

# S15c — POSITIVE CONTROL through $HOME ALONE: worktree-ledger.py resolves the
# ledger from expanduser("~"), never CLAUDE_CONFIG_DIR. A sandbox that moved only
# the config directory would let this write through to the operator's ledger.
mk_unit hometoucher.test.sh <<'HOMETOUCHER'
#!/usr/bin/env bash
unset CLAUDE_CONFIG_DIR
python3 - <<'PYW'
import os
p = os.path.join(os.path.expanduser("~"), ".claude", "state", "worktree-ledger.jsonl")
os.makedirs(os.path.dirname(p), exist_ok=True)
open(p, "a").write('{"event": "registered", "agent_id": "y", "teammate": "home-route", "ts": "t"}\n')
PYW
exit 0
HOMETOUCHER
RC="$(op_shard --only-units scripts/lib/hometoucher.test.sh)"
if [ "$RC" = "1" ] && grep -q 'teammate=home-route' "$SANDBOX/out"; then
    ok "S15c POSITIVE CONTROL: a ledger row written through expanduser(\"~\") alone is caught and named"
else
    bad "S15c rc=$RC — a HOME-route ledger write was not caught"; sed 's/^/          /' "$SANDBOX/out"
fi
rm -f "$E/scripts/lib/hometoucher.test.sh"

# S15d — POSITIVE CONTROL through RICHOS_WORKSPACES_DIR: the workspace registry.
mk_unit registrar.test.sh <<'REGISTRAR'
#!/usr/bin/env bash
ws="${RICHOS_WORKSPACES_DIR:-${CLAUDE_CONFIG_DIR:-$HOME/.claude}/state/workspaces}"
mkdir -p "$ws/agents"
printf '{"key": "deadbeef--dev-sonnet-q1"}\n' > "$ws/agents/deadbeef--dev-sonnet-q1.json"
printf '{"event": "registered-spawn", "key": "deadbeef--dev-sonnet-q1"}\n' >> "$ws/events.jsonl"
exit 0
REGISTRAR
RC="$(op_shard --only-units scripts/lib/registrar.test.sh)"
if [ "$RC" = "1" ] && grep -q 'deadbeef--dev-sonnet-q1' "$SANDBOX/out"; then
    ok "S15d POSITIVE CONTROL: a registration written into the workspace registry is caught and named"
else
    bad "S15d rc=$RC — a registry write was not caught"; sed 's/^/          /' "$SANDBOX/out"
fi
rm -f "$E/scripts/lib/registrar.test.sh"
# ...and none of the three reached the operator's record itself
LEAKED_TO_OP=""
[ -e "$OPCFG/state/worktree-ledger.jsonl" ] && LEAKED_TO_OP="$LEAKED_TO_OP $OPCFG/state/worktree-ledger.jsonl"
[ -e "$OPCFG/state/workspaces" ] && LEAKED_TO_OP="$LEAKED_TO_OP $OPCFG/state/workspaces"
if [ -z "$LEAKED_TO_OP" ]; then
    ok "S15b' and none of those writes reached the operator's record: they landed in the unit's throwaway home"
else
    bad "S15b' a unit's write reached the operator's record:$LEAKED_TO_OP"
    rm -rf "$OPCFG/state/worktree-ledger.jsonl" "$OPCFG/state/workspaces"
fi

# S15e — THE DEFECT: a CLEAN unit while something else on the machine writes the
# live record — a spawn's `registered` row, a new team directory, a fallback
# event. The writer is not the unit's descendant, and it writes WHILE the unit
# runs (a handshake on flag files, not a sleep). The unit must PASS.
rm -f "$FLAGS/unit-started" "$FLAGS/writer-done"
cat > "$E/scripts/lib/quiet.test.sh" <<QUIET
#!/usr/bin/env bash
touch "$FLAGS/unit-started"
n=0
while [ ! -e "$FLAGS/writer-done" ] && [ "\$n" -lt 300 ]; do sleep 0.1; n=\$((n + 1)); done
[ -e "$FLAGS/writer-done" ] || { echo "the concurrent writer never wrote; this case proves nothing" >&2; exit 7; }
exit 0
QUIET
chmod +x "$E/scripts/lib/quiet.test.sh"
(
    n=0
    while [ ! -e "$FLAGS/unit-started" ] && [ "$n" -lt 600 ]; do sleep 0.1; n=$((n + 1)); done
    [ -e "$FLAGS/unit-started" ] || exit 0
    printf '{"event": "registered", "agent_id": "live1", "teammate": "echo-opus-live1", "source": "detect-nonnative-worktree.sh", "ts": "t"}\n' >> "$OPCFG/state/worktree-ledger.jsonl"
    mkdir -p "$OPCFG/teams/session-cafebabe"
    printf '{"event": "WorkerRunEnded", "agent_id": "live1", "session_id": "cafebabe-0000", "timestamp": "t"}\n' >> "$OPCFG/worker-events.jsonl"
    # A REAL LAND, as workspaces.py writes it into the live registry: the
    # agent's record leaves agents/ for done/ and the landed/deleted events
    # are appended (2026-10-04: lands were held back during the nightly's
    # workspace-mutants gate, which runs its units through this runner, on
    # the belief that the canary would charge them to the gate's unit).
    mkdir -p "$OPCFG/state/workspaces/agents" "$OPCFG/state/workspaces/done"
    printf '{"key": "cafebabe--echo-opus-live1", "name": "echo-opus-live1"}\n' \
        > "$OPCFG/state/workspaces/agents/cafebabe--echo-opus-live1.json"
    printf '{"event": "landed", "key": "cafebabe--echo-opus-live1", "ts": "t"}\n{"event": "deleted", "key": "cafebabe--echo-opus-live1", "ts": "t"}\n' \
        >> "$OPCFG/state/workspaces/events.jsonl"
    mv "$OPCFG/state/workspaces/agents/cafebabe--echo-opus-live1.json" "$OPCFG/state/workspaces/done/"
    touch "$FLAGS/writer-done"
) &
WRITER=$!
RC="$(op_shard --only-units scripts/lib/quiet.test.sh)"
wait "$WRITER" 2>/dev/null
if [ "$RC" = "0" ] && grep -q 'PASS' "$SANDBOX/out" && [ -e "$FLAGS/writer-done" ] \
   && grep -q 'echo-opus-live1' "$OPCFG/state/worktree-ledger.jsonl" 2>/dev/null \
   && [ -f "$OPCFG/state/workspaces/done/cafebabe--echo-opus-live1.json" ]; then
    ok "S15e a clean unit PASSES while another process writes the live record during it, a land in the live workspace registry included — the canary counts only the unit's own writes"
else
    bad "S15e rc=$RC — a concurrent writer to the live record failed a clean unit"; sed 's/^/          /' "$SANDBOX/out"
fi
rm -f "$E/scripts/lib/quiet.test.sh"
rm -rf "$OPCFG/teams/session-cafebabe" "$OPCFG/worker-events.jsonl" "$OPCFG/state/worktree-ledger.jsonl" \
       "$OPCFG/state/workspaces"

# S15f — the unit CANNOT reach the live record: HOME is outside the operator's
# home, and every variable the operator exported INTO the record is removed —
# CLAUDE_CONFIG_DIR, RICHOS_WORKSPACES_DIR and RICHOS_TEST_DEVICES_DIR here.
cat > "$E/scripts/lib/envprobe.test.sh" <<ENVPROBE
#!/usr/bin/env bash
{
  printf 'HOME=%s\n' "\$HOME"
  printf 'CLAUDE_CONFIG_DIR=%s\n' "\${CLAUDE_CONFIG_DIR:-}"
  printf 'RICHOS_WORKSPACES_DIR=%s\n' "\${RICHOS_WORKSPACES_DIR:-}"
  printf 'RICHOS_TEST_DEVICES_DIR=%s\n' "\${RICHOS_TEST_DEVICES_DIR:-}"
} > "$FLAGS/env.txt"
exit 0
ENVPROBE
chmod +x "$E/scripts/lib/envprobe.test.sh"
RC="$(op_shard --only-units scripts/lib/envprobe.test.sh)"
ENVDUMP="$(cat "$FLAGS/env.txt" 2>/dev/null || true)"
if [ "$RC" = "0" ] && [ -n "$ENVDUMP" ] \
   && ! printf '%s\n' "$ENVDUMP" | grep -q "$OPHOME" \
   && printf '%s\n' "$ENVDUMP" | grep -q '^HOME=/' \
   && printf '%s\n' "$ENVDUMP" | grep -qx 'CLAUDE_CONFIG_DIR=' \
   && printf '%s\n' "$ENVDUMP" | grep -qx 'RICHOS_WORKSPACES_DIR=' \
   && printf '%s\n' "$ENVDUMP" | grep -qx 'RICHOS_TEST_DEVICES_DIR='; then
    ok "S15f the unit runs with HOME outside the operator's home, and every variable pointing into the record (CLAUDE_CONFIG_DIR, RICHOS_WORKSPACES_DIR, RICHOS_TEST_DEVICES_DIR) is removed"
else
    bad "S15f rc=$RC — the unit could reach the live record:"; printf '%s\n' "$ENVDUMP" | sed 's/^/          /'
fi
rm -f "$E/scripts/lib/envprobe.test.sh"

# S15g — the throwaway home is CI's shape, not an empty one: a git identity (the
# engine-self-verify workflow declares one, and fixtures commit), and NOTHING of
# the operator's own git configuration.
mk_unit gitid.test.sh <<'GITID'
#!/usr/bin/env bash
set -e
d="$(mktemp -d "${TMPDIR:-/tmp}/gitid.XXXXXX")"
trap 'rm -rf "$d"' EXIT
git -C "$d" init -q
git -C "$d" commit -q --allow-empty -m fixture
[ -z "$(git config --global --get richos.marker || true)" ] || { echo "the operator's git config is visible" >&2; exit 5; }
exit 0
GITID
RC="$(op_shard --only-units scripts/lib/gitid.test.sh)"
if [ "$RC" = "0" ]; then
    ok "S15g a fixture can commit in the unit's home (a git identity is declared), and the operator's git configuration is not visible"
else
    bad "S15g rc=$RC — git in the unit's home"; sed 's/^/          /' "$SANDBOX/out"
fi
rm -f "$E/scripts/lib/gitid.test.sh"

# S15h — a unit that builds a world of its own KEEPS the resolution it chose.
# The shape scratch-reaper.test.sh S30 has: it sets CLAUDE_CONFIG_DIR to its
# world and leaves RICHOS_WORKSPACES_DIR unset, so the registry resolves to
# <its config>/state/workspaces. This change's first version EXPORTED
# RICHOS_WORKSPACES_DIR into the throwaway home, overrode that, and S30's
# session records went unread. The runner removes; it never sets.
mk_unit ownworld.test.sh <<'OWNWORLD'
#!/usr/bin/env bash
w="$(mktemp -d "${TMPDIR:-/tmp}/ownworld.XXXXXX")"
trap 'rm -rf "$w"' EXIT
got="$(CLAUDE_CONFIG_DIR="$w" python3 -c 'import os; print((os.environ.get("RICHOS_WORKSPACES_DIR") or "").strip() or os.path.join(os.environ["CLAUDE_CONFIG_DIR"], "state", "workspaces"))')"
[ "$got" = "$w/state/workspaces" ] || { echo "the registry resolved to $got, not the unit's own world" >&2; exit 6; }
exit 0
OWNWORLD
RC="$(op_shard --only-units scripts/lib/ownworld.test.sh)"
if [ "$RC" = "0" ]; then
    ok "S15h a unit that points CLAUDE_CONFIG_DIR at a world of its own still resolves the registry inside that world — the runner removes names, it never sets them"
else
    bad "S15h rc=$RC — the runner overrode a unit's own world"; sed 's/^/          /' "$SANDBOX/out"
fi
rm -f "$E/scripts/lib/ownworld.test.sh"

# S15i — Codex's reproduction 337 (2026-09-27): a unit that does nothing but
# allocate and release engine scratch. The FIRST scratch_new creates
# <config>/state/scratch-ledger.jsonl, because the allocator resolves its ledger
# from ${CLAUDE_CONFIG_DIR:-$HOME/.claude} before a fixture (ofx_init) has
# redirected anything. Run as the operator, that created a file in the
# operator's config/state, which a stricter witness of the whole directory
# (Codex's wrapper, run 336) reported as a change to an isolated record. The
# per-unit home must absorb it: the unit PASSES, and the operator's config is
# byte for byte what it was, with no row of the UNIT's allocation in it. The
# positive control beside it is S15b: a worktree-ledger row is still red.
#
# The one row the operator's scratch ledger may gain is the RUNNER's own
# (2026-10-02, cc/zach-opus-tmpvanish1): ci-shard.sh takes its run folder from
# the registered allocator (lib/scratch.sh, working rule 5), so the reaper can
# remove that folder if ci-shard.sh is SIGKILLed, and the allocator records it
# in the ledger of whoever runs ci-shard.sh, here the operator. That is the
# runner's registration, not a unit's write, so this case checks it instead of
# forbidding the file: exactly one `ci-shard` allocation, released when the run
# ended, and nothing else. A unit's allocation reaching that ledger is still red.
cp "$ENGINE_ROOT/scripts/lib/scratch.sh" "$E/scripts/lib/scratch.sh"
mkdir -p "$SANDBOX/unit-tmp"
cat > "$E/scripts/lib/scratchonly.test.sh" <<SCRATCHONLY
#!/usr/bin/env bash
export TMPDIR="$SANDBOX/unit-tmp"
. "$E/scripts/lib/scratch.sh"
d="\$(scratch_new record-canary-337)" || { echo "scratch_new failed" >&2; exit 8; }
scratch_release "\$d" || { echo "scratch_release failed" >&2; exit 9; }
printf '%s\n' "\$(scratch_ledger)" > "$FLAGS/scratch-ledger-path.txt"
exit 0
SCRATCHONLY
chmod +x "$E/scripts/lib/scratchonly.test.sh"
op_record_state() { # every path under the operator's config, and every file's hash, but the scratch ledger (judged below)
    ( cd "$OPCFG" && find . -print | grep -vxF ./state/scratch-ledger.jsonl | LC_ALL=C sort \
      && find . -type f ! -path ./state/scratch-ledger.jsonl -exec shasum -a 256 {} + | LC_ALL=C sort )
}
op_ledger_only_the_runner() { # exactly one `ci-shard` allocation, released, and no other row
    python3 - "$OPCFG/state/scratch-ledger.jsonl" <<'PY'
import json, os, sys
try:
    rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
except OSError as e:
    print(f"no scratch ledger: {e}"); sys.exit(1)
real = lambda p: os.path.realpath(p or "")  # scratch_new records TMPDIR's spelling, scratch_release the real path
new = [r for r in rows if r.get("event") == "new"]
released = {real(r.get("path")) for r in rows if r.get("event") == "release"}
other = [r for r in rows if r.get("event") not in ("new", "release")]
if len(new) != 1 or new[0].get("label") != "ci-shard" or real(new[0].get("path")) not in released or other \
        or len(rows) != 2:
    print("unexpected rows: " + "; ".join(json.dumps(r) for r in rows)); sys.exit(1)
PY
}
rm -f "$OPCFG/state/scratch-ledger.jsonl"
OP_BEFORE="$(op_record_state)"
RC="$(op_shard --only-units scripts/lib/scratchonly.test.sh)"
OP_AFTER="$(op_record_state)"
UNIT_LEDGER="$(cat "$FLAGS/scratch-ledger-path.txt" 2>/dev/null || true)"
if LEDGER_WHY="$(op_ledger_only_the_runner)"; then LEDGER_OK=1; else LEDGER_OK=0; fi
if [ "$RC" = "0" ] && [ -n "$UNIT_LEDGER" ] && [ "$OP_BEFORE" = "$OP_AFTER" ] && [ "$LEDGER_OK" = 1 ] \
   && case "$UNIT_LEDGER" in "$OPHOME"/*) false ;; *) true ;; esac; then
    ok "S15i Codex 337: a unit that only allocates and releases scratch PASSES, its first scratch ledger is created in its own home ($UNIT_LEDGER), and the operator's config is unchanged but for the runner's own released ci-shard allocation"
else
    bad "S15i rc=$RC — the first scratch_new reached the operator's config (unit's ledger: ${UNIT_LEDGER:-none}; operator's scratch ledger: ${LEDGER_WHY:-only the runner allocation})"
    diff <(printf '%s\n' "$OP_BEFORE") <(printf '%s\n' "$OP_AFTER") | sed 's/^/          /'
    sed 's/^/          /' "$SANDBOX/out"
fi
rm -f "$E/scripts/lib/scratchonly.test.sh" "$OPCFG/state/scratch-ledger.jsonl"

# --- S16 / S17: the restricted plan ---------------------------------------
SUBSET="$SANDBOX/subset.txt"
printf 'scripts/lib/green.test.sh\n' > "$SUBSET"
printf 'scripts/hooks/contract-integrity.test.sh:alpha\n' >> "$SUBSET"
RSUB="$SANDBOX/rsub"; rm -rf "$RSUB"; mkdir -p "$RSUB"
RC1=0; RC2=0
bash "$SH" --units-file "$SUBSET" --shard 1/2 --receipt "$RSUB/1.jsonl" >/dev/null 2>&1 || RC1=$?
bash "$SH" --units-file "$SUBSET" --shard 2/2 --receipt "$RSUB/2.jsonl" >/dev/null 2>&1 || RC2=$?
RAN="$(cat "$RSUB"/*.jsonl 2>/dev/null | python3 -c '
import json, sys
print(" ".join(sorted(json.loads(l)["unit"] for l in sys.stdin if l.strip())))')"
if [ "$RC1" = "0" ] && [ "$RC2" = "0" ] \
   && [ "$RAN" = "scripts/hooks/contract-integrity.test.sh:alpha scripts/lib/green.test.sh" ]; then
    ok "S16  --units-file with --shard packs that set across shards and runs each unit exactly once"
else
    bad "S16  rc=$RC1/$RC2 ran: $RAN"
fi

bash "$SH" --verify-receipts "$RSUB" --units-file "$SUBSET" > "$SANDBOX/out" 2>&1
RC=$?
if [ "$RC" = "0" ] && grep -q '2/2 planned unit(s) ran' "$SANDBOX/out"; then
    ok "S17a --verify-receipts --units-file certifies the restricted plan"
else
    bad "S17a rc=$RC"; sed 's/^/          /' "$SANDBOX/out"
fi
bash "$SH" --verify-receipts "$RSUB" > "$SANDBOX/out" 2>&1
RC=$?
if [ "$RC" = "1" ] && grep -q 'have NO receipt' "$SANDBOX/out"; then
    ok "S17b the SAME receipts checked against the whole inventory FAIL — the restriction is a real claim, not a waiver"
else
    bad "S17b rc=$RC — a restricted run's receipts certified the whole inventory, which certifies nothing"
    sed 's/^/          /' "$SANDBOX/out"
fi

# --- S18: conditional rows -------------------------------------------------
KR="$E/scripts/lib/ci-known-red.tsv"
FUT="$(python3 -c 'import datetime; print((datetime.date.today() + datetime.timedelta(days=30)).isoformat())')"

# in force: the predicate holds, the failing unit is tolerated
printf 'scripts/lib/red.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\tconditional\ttrue\n' "$FUT" > "$KR"
RC="$(run_shard --only-units scripts/lib/red.test.sh)"
if [ "$RC" = "0" ] && grep -q 'KNOWN-RED' "$SANDBOX/out" && grep -q 'applies only where' "$SANDBOX/out"; then
    ok "S18a a conditional row IN FORCE behaves as an unconditional one, and prints its condition"
else
    bad "S18a rc=$RC"; sed 's/^/          /' "$SANDBOX/out"
fi

# dormant: the predicate is false, so the failing unit FAILS normally
printf 'scripts/lib/red.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\tconditional\tfalse\n' "$FUT" > "$KR"
RC="$(run_shard --only-units scripts/lib/red.test.sh)"
if [ "$RC" = "1" ] && ! grep -q 'KNOWN-RED' "$SANDBOX/out"; then
    ok "S18b a DORMANT row does not tolerate anything — the unit fails normally"
else
    bad "S18b rc=$RC — a row whose condition is false still suppressed a failure"
    sed 's/^/          /' "$SANDBOX/out"
fi

# dormant + the unit PASSES: rule 2 must NOT fire, or the developer's machine
# goes red over a defect that only exists on the runner. This is the case the
# whole seventh column exists for.
printf 'scripts/lib/green.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\tconditional\tfalse\n' "$FUT" > "$KR"
RC="$(run_shard --only-units scripts/lib/green.test.sh)"
if [ "$RC" = "0" ] && ! grep -q 'declares this unit red' "$SANDBOX/out"; then
    ok "S18c a DORMANT row over a PASSING unit is silent — rule 2 does not fire where the row does not apply"
else
    bad "S18c rc=$RC — rule 2 fired on a host the row does not describe, which is why the column exists"
    sed 's/^/          /' "$SANDBOX/out"
fi

# an unevaluable predicate must NOT fail open
printf 'scripts/lib/red.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\tconditional\tif [ ; then\n' "$FUT" > "$KR"
RC="$(run_shard --only-units scripts/lib/red.test.sh)"
if [ "$RC" = "0" ] && grep -q 'not valid shell' "$SANDBOX/out"; then
    ok "S18d an unevaluable condition is treated as APPLYING and says so, rather than failing open"
else
    bad "S18d rc=$RC — a broken condition changed the verdict silently"
    sed 's/^/          /' "$SANDBOX/out"
fi

# and a six-column row still means what it always did
printf 'scripts/lib/red.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\tno condition at all\n' "$FUT" > "$KR"
RC="$(run_shard --only-units scripts/lib/red.test.sh)"
if [ "$RC" = "0" ] && grep -q 'KNOWN-RED' "$SANDBOX/out"; then
    ok "S18e a six-column row is unconditional, exactly as before the column existed"
else
    bad "S18e rc=$RC — adding the column changed the meaning of existing rows"
fi
rm -f "$KR"

# ---------------------------------------------------------------------------
# S19 — THE PER-UNIT DEADLINE. A unit that never finishes is NAMED and FAILS.
# ---------------------------------------------------------------------------
# Before 2026-09-15 a hung unit had no ceiling at all: it took the shard down
# at the workflow's `timeout-minutes` with `The operation was canceled.` and
# nothing about WHICH unit was stuck.
#
# THE FIXTURE HANGS ON PURPOSE AND THE DEADLINE IS ONE SECOND, so this case
# cannot pass because the unit happened to be fast. The first attempt at this
# test used a real 2.1 s unit with a 1 s deadline and reported PASS — the unit
# finished in 1.0 s on a warm cache and the deadline never fired. An
# indeterminate negative test is worse than none: it reports the feature works.
printf '#!/usr/bin/env bash\nsleep 120\n' > "$E/scripts/lib/hangs.test.sh"
chmod +x "$E/scripts/lib/hangs.test.sh"

T0="$(python3 -c 'import time; print(time.time())')"
RC="$(CI_SHARD_UNIT_TIMEOUT=1 run_shard --only-units scripts/lib/hangs.test.sh \
        --receipt "$SANDBOX/hang.jsonl")"
T1="$(python3 -c 'import time; print(time.time())')"
ELAPSED="$(python3 -c "print(int($T1 - $T0))")"

if [ "$RC" = "1" ] && grep -q 'DEADLINE' "$SANDBOX/out"; then
    ok "S19  a unit that never finishes is KILLED at its deadline and FAILS the shard"
else
    bad "S19  rc=$RC — a hung unit did not fail the shard; the job would have died anonymously instead"
    sed 's/^/          /' "$SANDBOX/out"
fi

# The unit sleeps 120 s. If the deadline did not actually kill it, this case
# could only have finished by waiting for it — so the clock is the proof that
# the kill happened, independent of anything the script printed about itself.
if [ "$ELAPSED" -lt 30 ]; then
    ok "S19b the kill is real: the 120 s unit was reaped in ${ELAPSED}s, not waited out"
else
    bad "S19b the shard took ${ELAPSED}s on a 120 s unit with a 1 s deadline — nothing was killed"
fi

if grep -q '"verdict":"TIMED-OUT"' "$SANDBOX/hang.jsonl" 2>/dev/null; then
    ok "S19c the receipt says TIMED-OUT, not FAIL — 'never finished' and 'finished wrong' are different reports"
else
    bad "S19c the receipt does not carry the TIMED-OUT verdict: $(cat "$SANDBOX/hang.jsonl" 2>/dev/null)"
fi

# S19e — NOTHING A TIMED-OUT UNIT STARTED SURVIVES IT (2026-09-23). The kill used
# to reach the unit's own shell only: workspace-spec-fourteen.test.sh was killed at
# 3600 s and its mutation harness ran on under init for about an hour and a half.
# The fixture reproduces each way a process escaped: a child that IGNORES SIGTERM,
# a background child whose parent already exited (re-parented to init, reachable
# only through its process group), and a grandchild in a SESSION OF ITS OWN whose
# parent waits on it (the shape of stop-at-line.py and reserve.py). Each writes
# its pid; after the shard returns, every one of them must be gone. NOT covered,
# and said so: a process that puts itself in a new session AND whose parent has
# already exited is nobody's descendant and in no group of the unit's; nothing
# selected by parentage or group can reach it (proc_tree.py's header).
PIDS="$SANDBOX/s19e-pids"; mkdir -p "$PIDS"
cat > "$E/scripts/lib/escapes.test.sh" <<ESCAPES
#!/usr/bin/env bash
bash -c 'trap "" TERM; echo \$\$ > "$PIDS/ignores-term"; while :; do sleep 1; done' &
sh -c 'sleep 300 >/dev/null 2>&1 & echo \$! > "$PIDS/orphaned"'
python3 -c 'import subprocess; p = subprocess.Popen(["sleep", "300"], start_new_session=True); open("$PIDS/own-session", "w").write(str(p.pid)); p.wait()' &
sleep 300
ESCAPES
chmod +x "$E/scripts/lib/escapes.test.sh"
RC="$(CI_SHARD_UNIT_TIMEOUT=2 run_shard --only-units scripts/lib/escapes.test.sh)"
S19E_ALIVE=""; S19E_SEEN=0
for f in ignores-term orphaned own-session; do
    p="$(cat "$PIDS/$f" 2>/dev/null)"
    [ -n "$p" ] || continue
    S19E_SEEN=$((S19E_SEEN + 1))
    if kill -0 "$p" 2>/dev/null; then S19E_ALIVE="$S19E_ALIVE $f=$p"; kill -KILL "$p" 2>/dev/null; fi
done
if [ "$RC" = "1" ] && [ "$S19E_SEEN" -eq 3 ] && [ -z "$S19E_ALIVE" ]; then
    ok "S19e a timed-out unit takes its whole tree: a TERM-ignoring child, an orphan re-parented to init and a grandchild in its own session are all gone"
else
    bad "S19e rc=$RC, $S19E_SEEN of 3 fixture processes started, still alive after the shard returned:${S19E_ALIVE:- none}"
    sed 's/^/          /' "$SANDBOX/out"
fi

# A hung unit must NOT be excusable by the known-red table. A declaration says
# "this unit fails in a known way", which is a claim about a verdict it
# REACHED; a hang reaches none, and letting the table swallow it would turn a
# declared entry into an unbounded permit to block the pipeline.
printf 'scripts/lib/hangs.test.sh\t2026-01-01\t%s\tdeadbeef\tC1\thangs\n' "$FUT" > "$KR"
RC="$(CI_SHARD_UNIT_TIMEOUT=1 run_shard --only-units scripts/lib/hangs.test.sh)"
if [ "$RC" = "1" ]; then
    ok "S19d a KNOWN-RED declaration does not excuse a HANG — a hang reaches no verdict to declare"
else
    bad "S19d rc=$RC — the known-red table swallowed a hang, making it an unbounded permit"
    sed 's/^/          /' "$SANDBOX/out"
fi
rm -f "$KR"

# ---------------------------------------------------------------------------
# S20 — WEIGHT DRIFT reports itself, with the replacement row.
# ---------------------------------------------------------------------------
# The packing data went stale silently and cost more wall clock than anything
# else in this system: 32 of 137 units carried no weight row, one of them
# actually costing 2811.7 s against the 60 s the packer assumed.
cat > "$E/scripts/lib/ci-unit-weights.tsv" <<'W'
# test fixture
scripts/lib/slow.test.sh	1.0	fixture
W
printf '#!/usr/bin/env bash\nsleep 4\nexit 0\n' > "$E/scripts/lib/slow.test.sh"
chmod +x "$E/scripts/lib/slow.test.sh"
# CI_SHARD_DRIFT_FLOOR is lowered from its declared 60 s so this case costs
# four seconds instead of seventy. The FACTOR is left at its real value, so
# what is being tested is the live comparison and the live output, not a
# special path. The default floor gets its own case immediately below.
RC="$(CI_SHARD_DRIFT_FLOOR=1 run_shard --only-units scripts/lib/slow.test.sh)"
if [ "$RC" = "0" ] && grep -q 'WEIGHT-DRIFT' "$SANDBOX/out" \
   && grep -q 'Replacement row' "$SANDBOX/out" \
   && python3 -c 'import re,sys; rows=re.findall(r"scripts/lib/slow[.]test[.]sh\t([0-9.]+)",open(sys.argv[1]).read());sys.exit(0 if rows and float(rows[-1]) >= 4 else 1)' "$SANDBOX/out"; then
    ok "S20  a unit that costs far more than its recorded weight PRINTS the replacement row, measured"
else
    bad "S20  rc=$RC — drift went unreported, so the plan can rot silently again"
    sed 's/^/          /' "$SANDBOX/out"
fi

# AND IT IS A REPORT, NOT A FAILURE. A weight decides which machine runs a
# unit, never whether it passed. A coverage job that went red over packing
# balance is a coverage job that gets waived, which is how the last three
# guards in this project died.
#
# The `grep` is repeated rather than inherited from the case above on purpose:
# asserting only `rc=0` would also pass on a build where drift is never
# detected at all, which is the exact shape of "a negative test that passes for
# the wrong reason" this project keeps a note about.
if [ "$RC" = "0" ] && grep -q 'WEIGHT-DRIFT' "$SANDBOX/out"; then
    ok "S20b drift is REPORTED and does not fail the shard — a wrong weight cannot make a green wrong"
else
    bad "S20b drift either failed the shard (rc=$RC) or was not detected; balance is not correctness"
fi

# THE DEFAULT FLOOR IS REAL. The case above lowers it, so without this one a
# build whose declared floor had drifted to 6000 s would still show S20 green.
RC="$(run_shard --only-units scripts/lib/slow.test.sh)"
if [ "$RC" = "0" ] && ! grep -q 'WEIGHT-DRIFT' "$SANDBOX/out"; then
    ok "S20a at the DECLARED floor the same 4x drift is silent — 3 s of imbalance is not worth a line"
else
    bad "S20a the declared 60 s floor did not suppress a 3 s drift; the report will be noise"
    sed 's/^/          /' "$SANDBOX/out"
fi

# The quiet clause: a unit within its weight must print nothing at all, or a
# hundred lines of noise bury the one that matters.
printf '#!/usr/bin/env bash\nexit 0\n' > "$E/scripts/lib/ontime.test.sh"
chmod +x "$E/scripts/lib/ontime.test.sh"
printf 'scripts/lib/ontime.test.sh\t120.0\tfixture\n' >> "$E/scripts/lib/ci-unit-weights.tsv"
RC="$(run_shard --only-units scripts/lib/ontime.test.sh)"
if [ "$RC" = "0" ] && ! grep -q 'WEIGHT-DRIFT' "$SANDBOX/out"; then
    ok "S20c a unit inside its weight says nothing — the report stays readable"
else
    bad "S20c a well-behaved unit printed a drift line; the report will be read as noise"
    sed 's/^/          /' "$SANDBOX/out"
fi
rm -f "$E/scripts/lib/hangs.test.sh" "$E/scripts/lib/slow.test.sh" \
      "$E/scripts/lib/ontime.test.sh" "$E/scripts/lib/ci-unit-weights.tsv"

# A large inventory must not turn an early match into an upstream SIGPIPE.
BIG="$SANDBOX/large-engine"; mk_engine "$BIG"
python3 - "$BIG" <<'PY'
import pathlib, sys
root = pathlib.Path(sys.argv[1]) / 'scripts/lib'
for i in range(2000):
    (root / ('inventory-%04d-padding-for-a-realistic-unit-path.test.sh' % i)).write_text('#!/usr/bin/env bash\nexit 0\n')
PY
if bash "$BIG/scripts/ci-shard.sh" --only-units scripts/hooks/contract-integrity.test.sh:alpha --list > "$SANDBOX/out" 2>&1; then
    ok "S21 a valid early unit survives an inventory larger than a pipe buffer"
else
    bad "S21 a valid unit was rejected from a large inventory"
    sed 's/^/          /' "$SANDBOX/out"
fi

# A failure must reach the outer runner without running the remainder of this shard.
mk_suite "$E/scripts/lib/00-stop-red.test.sh" 1
mk_suite "$E/scripts/lib/01-stop-green.test.sh" 0
FAST_UNITS='scripts/lib/00-stop-red.test.sh,scripts/lib/01-stop-green.test.sh'
RC="$(run_shard --fail-fast --only-units "$FAST_UNITS" --receipt "$SANDBOX/fast.jsonl")"
if [ "$RC" = 1 ] && [ "$(wc -l < "$SANDBOX/fast.jsonl" | tr -d ' ')" = 1 ] && ! grep -q '01-stop-green' "$SANDBOX/fast.jsonl"; then
    ok "S22 fail-fast stops after the failed unit and cannot receipt an unfinished unit"
else
    bad "S22 fail-fast continued or wrote an unearned receipt"
fi
RC="$(run_shard --only-units "$FAST_UNITS" --receipt "$SANDBOX/continue.jsonl")"
if [ "$RC" = 1 ] && [ "$(wc -l < "$SANDBOX/continue.jsonl" | tr -d ' ')" = 2 ]; then
    ok "S22b the explicit full-inventory mode still runs both units"
else
    bad "S22b full-inventory mode lost a unit"
fi

fi

# A contamination finding stops the shard even with default continuation and
# signals the outer runner without inventing receipts for units never executed.
if [ "${1:-}" != --stop-only ]; then
mk_suite "$E/scripts/lib/01-stop-green.test.sh" 0
cat > "$E/scripts/lib/00-contaminate.test.sh" <<'CONTAMINATE'
#!/usr/bin/env bash
cfg="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
mkdir -p "$cfg/state"
printf '{"event":"terminated","agent_id":"contamination-fixture"}\n' >> "$cfg/state/worktree-ledger.jsonl"
CONTAMINATE
export RICHOS_VERIFICATION_CONTAMINATION="$SANDBOX/contamination"
RC="$(run_shard --only-units 'scripts/lib/00-contaminate.test.sh,scripts/lib/01-stop-green.test.sh' --receipt "$SANDBOX/unsafe.jsonl")"
if [ "$RC" = 1 ] && [ "$(wc -l < "$SANDBOX/unsafe.jsonl" | tr -d ' ')" = 1 ] \
   && grep -q 'RECORD-TOUCHED' "$SANDBOX/unsafe.jsonl" \
   && [ "$(find "$RICHOS_VERIFICATION_CONTAMINATION" -name '*.json' | wc -l | tr -d ' ')" = 1 ]; then
    ok "S23 contamination stops the domain and signals siblings in default continuation mode"
else
    bad "S23 contamination was continued or lost its signal"
    sed 's/^/          /' "$SANDBOX/out"
fi
unset RICHOS_VERIFICATION_CONTAMINATION
fi

# ---------------------------------------------------------------------------
# S24 — A STOPPED RUN NEVER DELETES ITS FOLDER UNDER ITS OWN UNIT (2026-10-02)
# ---------------------------------------------------------------------------
# The reproduction of the merge gate's refusals of cc/zach-sonnet-vinputs1:
# the run is started under the supervisor the gate uses (proc_tree.py run) and
# stopped the way the gate stops a check at its cap (TERM to that supervisor,
# whose finish_scope then sends TERM to every process of the run at once). The
# unit TRAPS TERM and spends a second on its own cleanup before it prints its
# last line, so a folder removed at the moment of the signal is caught three
# ways: worker_tokens.py's timing write fails (the gate's FileNotFoundError),
# the unit's last line is never printed, and nothing names where it was.
if [ "${1:-}" != --contamination-only ]; then
S24T="$SANDBOX/s24-tmp"; S24PID="$SANDBOX/s24-unit.pid"
mkdir -p "$S24T"
cat > "$E/scripts/lib/stopped.test.sh" <<STOPPED
#!/usr/bin/env bash
trap 'trap "" TERM; sleep 1; echo "S24-UNIT-CLEANUP-FINISHED"; exit 143' TERM
echo "S24-UNIT-REACHED-ITS-LONG-STEP"
echo \$\$ > "$S24PID"
sleep 300 &
wait
STOPPED
chmod +x "$E/scripts/lib/stopped.test.sh"
: > "$SANDBOX/s24.jsonl"
( cd "$E" && TMPDIR="$S24T" exec python3 "$E/scripts/lib/proc_tree.py" run "$$" -- \
    bash "$SH" --only-units scripts/lib/stopped.test.sh --receipt "$SANDBOX/s24.jsonl" \
    > "$SANDBOX/s24.out" 2>&1 ) &
S24SUP=$!
S24DIR=""
for _ in $(seq 1 300); do
    if [ -s "$S24PID" ]; then
        S24LOG="$(find "$S24T" -name 1.log -type f 2>/dev/null | head -1)"
        [ -n "$S24LOG" ] && S24DIR="$(dirname "$S24LOG")" && break
    fi
    sleep 0.1
done
S24ROOT="$(cd "$S24T" && pwd -P)/richos-scratch"
S24ALLOC=0
S24OWNER=""
if [ -n "$S24DIR" ]; then
    # The run's folder, while the unit runs: allocated, and its owner on record.
    case "$S24DIR" in
        "$S24ROOT"/*-ci-shard-*)
            S24OWNER="${S24DIR##*/}"; S24OWNER="${S24OWNER%%-*}"
            if kill -0 "$S24OWNER" 2>/dev/null \
               && grep -F "\"path\":\"$S24DIR\"" "$CLAUDE_CONFIG_DIR/state/scratch-ledger.jsonl" 2>/dev/null \
                  | grep -q "\"pid\":$S24OWNER,.*\"event\":\"new\""; then
                S24ALLOC=1
            fi ;;
    esac
fi
kill -TERM "$S24SUP" 2>/dev/null
wait "$S24SUP" 2>/dev/null
S24PROCLEFT="$(cat "$S24PID" 2>/dev/null)"
if [ -n "$S24PROCLEFT" ] && kill -0 "$S24PROCLEFT" 2>/dev/null; then kill -KILL "$S24PROCLEFT" 2>/dev/null; fi
if [ -z "$S24DIR" ]; then
    bad "S24  the stopped-run fixture never started its unit"
    sed 's/^/          /' "$SANDBOX/s24.out"
elif grep -q 'FileNotFoundError\|No such file or directory' "$SANDBOX/s24.out"; then
    bad "S24  the run's folder was removed while its unit was still stopping (the merge gate's FileNotFoundError)"
    sed 's/^/          /' "$SANDBOX/s24.out"
elif ! grep -q 'S24-UNIT-CLEANUP-FINISHED' "$SANDBOX/s24.out" \
     || ! grep -q 'S24-UNIT-REACHED-ITS-LONG-STEP' "$SANDBOX/s24.out"; then
    bad "S24  a stopped run did not print its unit's own last output, so a stop at the cap leaves nothing to say where the unit was"
    sed 's/^/          /' "$SANDBOX/s24.out"
else
    ok "S24  a stopped run keeps its folder until its unit has finished stopping, and prints the unit's last output"
fi
if [ "$S24ALLOC" -eq 1 ]; then
    ok "S24b the run's folder comes from the scratch allocator, its live owner (pid $S24OWNER) on record in the ledger and the name"
else
    bad "S24b the run's folder is not an allocation with a live recorded owner: ${S24DIR:-none} (allocator root $S24ROOT)"
fi
if [ -n "$S24DIR" ] && [ ! -e "$S24DIR" ] && ! grep -q . "$SANDBOX/s24.jsonl"; then
    ok "S24c the stopped run still removes its folder afterward, and writes no receipt for the unfinished unit"
else
    bad "S24c after the stop: folder ${S24DIR:-none} exists=$([ -e "${S24DIR:-/nonexistent}" ] && echo yes || echo no), receipt rows=$(grep -c . "$SANDBOX/s24.jsonl")"
fi
rm -f "$E/scripts/lib/stopped.test.sh"
fi

# --- S25: THE ENGINE'S OWN RUNTIME STATE IS NOT A UNIT'S TO WRITE (2026-10-05) ---
# Hook suites seated their hooks on the engine checkout (RICHOS_ENTITY_ROOT=<engine>), so their
# acks and unparseable payloads were appended to <engine>/.claude/state/: in the merge gate the
# live plugin's audit record, and an input of every engine check beside them (501 results thrown
# away as `invalid`). The state folder is gitignored, so the leak canary is blind there by design.
# A unit after which it changed is STATE-WRITTEN, named with the line it added (S25a); the same
# unit seated on lib/entity-sandbox.sh passes and leaves the engine's state as it was (S25b); and
# a state folder that already holds files is no finding by itself (S25c).
if [ -z "${1:-}" ] || [ "${1:-}" = --state-only ]; then
# The engine is a checkout whose .gitignore holds /.claude/state/, as the real one does: that is
# what blinds the leak canary there, and the case is about exactly that blindness.
git -C "$E" init -q
printf '/.claude/state/\n' > "$E/.gitignore"
mkdir -p "$E/.claude/state"
printf 'an older line\n' > "$E/.claude/state/unevaluated-payloads.log"
cp "$ENGINE_ROOT/scripts/lib/entity-sandbox.sh" "$E/scripts/lib/entity-sandbox.sh"
state_digest() { find "$E/.claude/state" -type f -exec shasum -a 256 {} + 2>/dev/null | LC_ALL=C sort; }
cat > "$E/scripts/lib/stateful.test.sh" <<'STATEFUL'
#!/usr/bin/env bash
# The writer's shape: a hook seated on the engine appends an unparseable payload's line.
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
entity="$here"
if [ "${SEAT_ON_SANDBOX:-}" = 1 ]; then
    . "$here/scripts/lib/entity-sandbox.sh"
    entity="$(entity_sandbox "$here")"
fi
mkdir -p "$entity/.claude/state"
printf '2026-10-05T02:13:38Z\tunevaluated\thook=fixture-guard.sh\treason=not-json\n' >> "$entity/.claude/state/unevaluated-payloads.log"
[ "$entity" = "$here" ] || rm -rf "$entity"
exit 0
STATEFUL
chmod +x "$E/scripts/lib/stateful.test.sh"
S25_BEFORE="$(state_digest)"
RC="$(run_shard --only-units scripts/lib/stateful.test.sh)"
if [ "$RC" = "1" ] && grep -q 'STATE-WRITTEN' "$SANDBOX/out" \
   && grep -q 'unevaluated-payloads.log  appended 1 line(s); first: .*hook=fixture-guard.sh' "$SANDBOX/out" \
   && grep -q 'scripts/lib/stateful.test.sh — STATE-WRITTEN' "$SANDBOX/out"; then
    ok "S25a a unit that appends to the engine's own .claude/state fails by name, with the line it added"
else
    bad "S25a rc=$RC — a write into the engine's runtime state was not named"; sed 's/^/          /' "$SANDBOX/out"
fi
printf 'an older line\n' > "$E/.claude/state/unevaluated-payloads.log"
export SEAT_ON_SANDBOX=1
RC="$(run_shard --only-units scripts/lib/stateful.test.sh)"
unset SEAT_ON_SANDBOX
if [ "$RC" = "0" ] && grep -q 'PASS' "$SANDBOX/out" && [ "$(state_digest)" = "$S25_BEFORE" ]; then
    ok "S25b the same unit seated on lib/entity-sandbox.sh passes, and the engine's state is byte-for-byte unchanged"
else
    bad "S25b rc=$RC — the sandboxed unit did not pass, or the engine's state changed"; sed 's/^/          /' "$SANDBOX/out"
fi
RC="$(run_shard --only-units scripts/lib/green.test.sh)"
if [ "$RC" = "0" ] && ! grep -q 'STATE-WRITTEN' "$SANDBOX/out"; then
    ok "S25c a unit that leaves an existing state folder alone is no finding"
else
    bad "S25c rc=$RC — an untouched state folder was reported"; sed 's/^/          /' "$SANDBOX/out"
fi
rm -rf "$E/.claude" "$E/.git" "$E/.gitignore" "$E/scripts/lib/stateful.test.sh" "$E/scripts/lib/entity-sandbox.sh"
fi

# --- S26: the host shield (ceo-decisions §65; lib/host_shield.py) ----------
# A unit runs through the shield: one calling the host halt command by bare name fails with the
# stand-in's logged line, one whose source names it by path is refused before it runs. The two
# throwaway suites and their walls are lib/host-shield-fixture.sh's; PATH is the tripwire first
# and neither sbin. The fixture engine has no shield of its own and uses the exported one.
if [ -z "${1:-}" ]; then
# shellcheck source=lib/host-shield-fixture.sh
. "$ENGINE_ROOT/scripts/lib/host-shield-fixture.sh"
EHS="$SANDBOX/engine-hs"; mk_engine "$EHS"
hs_fixture "$SANDBOX/hstrip" "$EHS/scripts/lib"
hs_shard() { # <unit> -> rc; output in $SANDBOX/out
    PATH="$(hs_path "$SANDBOX/hstrip")" RICHOS_HOST_SHIELD_TOOL="$HS_TOOL" \
        bash "$EHS/scripts/ci-shard.sh" --only-units "$1" > "$SANDBOX/out" 2>&1
    printf '%s' "$?"
}
RC="$(hs_shard scripts/lib/hs-bare.test.sh)"
if [ "$RC" != 0 ] && grep -q "FAIL  host-shield: refused \`$HS_CMD -h +60\`" "$SANDBOX/out" && [ ! -s "$SANDBOX/hstrip/trip.log" ]; then
    ok "S26a a unit calling $HS_CMD -h +60 by bare name fails with the stand-in's logged line; nothing reached the tripwire"
else
    bad "S26a rc=$RC — a bare-name host call was not stood in for"; sed 's/^/          /' "$SANDBOX/out"
fi
RC="$(hs_shard scripts/lib/hs-path.test.sh)"
if [ "$RC" != 0 ] && grep -q "REFUSED before it ran" "$SANDBOX/out" && [ ! -e "$SANDBOX/hstrip/path-suite-ran" ]; then
    ok "S26b a unit whose source names /sbin/$HS_CMD is refused before it runs"
else
    bad "S26b rc=$RC — a by-path host call was not refused before the unit ran"; sed 's/^/          /' "$SANDBOX/out"
fi
# S26c — with no shield beside it and none exported, the runner refuses (rc 2) and starts no unit.
printf '%s\n' '#!/usr/bin/env bash' "touch \"$SANDBOX/hstrip/unit-started\"" 'echo "  PASS  started"' \
    > "$EHS/scripts/lib/hs-started.test.sh"
RC="$(PATH="$(hs_path "$SANDBOX/hstrip")" env -u RICHOS_HOST_SHIELD_TOOL \
        bash "$EHS/scripts/ci-shard.sh" --only-units scripts/lib/hs-started.test.sh > "$SANDBOX/out" 2>&1; printf '%s' "$?")"
if [ "$RC" = 2 ] && grep -q "REFUSED: NO HOST SHIELD" "$SANDBOX/out" && [ ! -e "$SANDBOX/hstrip/unit-started" ]; then
    ok "S26c with no shield beside it and none exported, ci-shard.sh refuses (rc 2) and the unit never starts"
else
    bad "S26c rc=$RC — a unit started without the host shield"; sed 's/^/          /' "$SANDBOX/out"
fi
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    if [ "${1:-}" = --state-only ]; then
        echo "=== ci-shard tests: scoped S25 passed; other cases were not run ==="
        exit 3
    fi
    if [ "${1:-}" = --contamination-only ]; then
        echo "=== ci-shard tests: scoped S23 passed; other cases were not run ==="
        exit 3
    fi
    if [ "${1:-}" = --stop-only ]; then
        echo "=== ci-shard tests: scoped S24 passed; other cases were not run ==="
        exit 3
    fi
    echo "=== ci-shard tests: all $PASS passed ==="
    exit 0
fi
echo "=== ci-shard tests: $PASS passed, $FAIL FAILED ===" >&2
exit 1
