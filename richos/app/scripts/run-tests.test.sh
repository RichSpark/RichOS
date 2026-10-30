#!/usr/bin/env bash
#
# run-tests.test.sh — the harness's own allowance, held to account.
#
# =========================================================================================
# WHY A SUITE FOR THE THING THAT RUNS THE SUITES
# =========================================================================================
#
# `run-tests.sh` now tolerates one outcome it used to call a failure: a suite that exits 2,
# meaning "this host cannot answer". That allowance is the only way seven suites that pass
# perfectly on a public runner can run in CI at all, because the eighth needs a compiler that
# lives in a private repository and no runner can ever have it.
#
# IT IS ALSO THE MOST DANGEROUS LINE IN THIS DIRECTORY. An allowance is how a suite stops
# running and nobody finds out, and this repository has shipped that defect five times under
# a reassuring fraction — "13/13 guards", "18/18 suites", a `run.js` reporting "all 4 suites
# passed" while running none of `steering.js`'s 24 checks. The allowance is therefore
# conditional in four ways, and every one of those conditions is a claim that has to keep
# being true. A claim nothing executes is a comment.
#
# So: fake suites in a scratch directory with known exit codes, and a copy of the real
# `run-tests.sh` pointed at them. Nothing here touches the repository's own suites.
#
# CASES
#
#   H1  an UNDECLARED gap is refused                    <- the anti-silence case
#   H2  a gap DECLARED with a reason is green, and the summary names it
#   H3  a declaration with NO reason is refused         <- a bare marker declares nothing
#   H4  a STALE declaration — the suite answered — is refused
#   H5  a real FAILURE outranks a declared gap
#   H6  no gaps at all leaves the original summary untouched
#   H7  an empty inventory is still exit 2, never "all 0 suites passed"
#   H8  a DECLARED gap whose suite failed a case is a FAILURE   <- the concealment case
#   H9  ...and a declared gap that failed nothing is still green — H8 has not eaten H2
#   B1-B8  `--for`: the desktop build leaves out the phone apps' suites and keeps the Mac side
#
# THIS SUITE OPENS NO WINDOW, and it has to say so like every other file here — case S6
# below scans this directory for anything that could put one on the operator's Mac, and it
# scans ITSELF along with the rest. What it finds here is its OWN search pattern: the
# literal `/Contents/MacOS/`, `$GUI_BINARY` and `richos-tauri` inside the `grep -qE` it uses
# to do the scanning, plus the `. "$DIR/lib/gui-launch.sh"` that S1 writes into a FAKE suite
# in a scratch directory. Every suite this file runs is one it wrote itself, under mktemp,
# and not one of them is the app.
#
# That the scanner is caught by its own net is the mechanism working, not a flaw in it: a
# scanner exempt from its own rule is a rule with a hole exactly where somebody clever would
# put something.
# run-tests: no-host-screen: its matches are its own S6 search pattern and the fake suites S1 writes under mktemp
# run-tests: inputs richos/app/scripts/run-tests.test.sh richos/app/scripts/run-tests.sh richos/app/scripts/lib/worktree-resource.sh richos/app/scripts/lib/test_results.py richos/app/scripts/lib/proof_declarations.py richos/app/scripts/lib/gui-launch.sh richos/app/scripts/testvm richos/app/scripts/phone-app-suites.tsv richos/app/scripts/bin/cargo richos/app/scripts/lib/cargo_identity.py richos/app/scripts/lib/cargo-cache-env.sh richos/engine/scripts/lib/host_shield.py richos/engine/scripts/lib/machine_settings.py richos/engine/scripts/lib/machine-settings.sh richos/engine/scripts/lib/host-shield richos/engine/scripts/lib/host-shield-fixture.sh richos/engine/scripts/hooks/guard-host-display-power.sh
# run-tests: covers richos/app/scripts/run-tests.sh richos/app/scripts/lib/worktree-resource.sh richos/app/scripts/lib/gui-launch.sh richos/app/scripts/testvm/run-suite.sh richos/app/scripts/testvm/suite-in-guest.sh
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HARNESS="$DIR/run-tests.sh"
# A box copy of the runner has no engine beside it: every run of one gets this checkout's host
# shield through RICHOS_HOST_SHIELD_TOOL, which `env -i` would otherwise drop (without it the
# runner refuses; review rv-20261011T173659Z-e08ee8a2-2c75 #1).
SHIELD_TOOL="$(cd "$DIR/../../engine/scripts/lib" && pwd)/host_shield.py"

# A FIXTURE IS THE HARNESS AS IT SHIPS, not the harness minus its libraries. `run-tests.sh`
# derives every shared name — the proof store today — through `lib/worktree-resource.sh`,
# and refuses to run without it rather than falling back to a fixed path. So a scratch box
# gets both files, exactly as the real directory holds both.
install_harness() {  # install_harness <box directory>
  mkdir -p "$1/lib" "$1/bin"
  cp "$HARNESS" "$1/run-tests.sh"
  cp "$DIR/lib/worktree-resource.sh" "$1/lib/worktree-resource.sh"
  cp "$DIR/lib/test_results.py" "$1/lib/test_results.py"
  cp "$DIR/lib/proof_declarations.py" "$1/lib/proof_declarations.py"
  cp "$DIR/lib/cargo-cache-env.sh" "$DIR/lib/cargo_identity.py" "$1/lib/"
  cp "$DIR/bin/cargo" "$1/bin/cargo"
  mkdir -p "$1/../../engine/scripts/lib"
  cp "$DIR/../../engine/scripts/lib/machine_settings.py" \
     "$DIR/../../engine/scripts/lib/machine-settings.sh" "$1/../../engine/scripts/lib/"
}

TASK_TMP="$(mktemp -d "${TMPDIR:?}/run-tests-test.XXXXXX")"
trap 'rm -rf "$TASK_TMP"' EXIT
# Copied runners resolve engine helpers two levels above their scripts directory.
TMP="$TASK_TMP/fixtures"
mkdir -p "$TMP"
# A failed fake suite's results folder is kept in this scratch, never the operator's store; and a
# caller's results root (a proof run around this suite) is not the fixtures' to write into.
export RUN_TESTS_RESULTS_STATE="$TMP/kept-results"
unset RICHOS_TEST_RESULTS_ROOT RICHOS_TEST_RESULTS_DIR

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "${2:-}"; FAIL=$((FAIL + 1)); }

if [ "$(uname -s)" != "Darwin" ]; then
  echo "run-tests.test.sh: run-tests.sh refuses off macOS, so its allowances cannot be" >&2
  echo "                   exercised here. (uname -s reports $(uname -s).)" >&2
  exit 3
fi

# =========================================================================================
# TWO THINGS THAT MADE THIS FILE LIE, AND THE TWO HELPERS THAT END THEM
# =========================================================================================
#
# 2026-09-19. This suite was GREEN run by hand and RED inside `nightly-local.py build
# --no-host-screen` — the same commit, `62e5affd`, minutes apart, case H2, with the harness
# output in the build byte-for-byte identical to the output of a run that passed. A suite
# whose verdict depends on who started it is worth nothing, because the next disagreement
# is read as "the build is broken" or "the suite is flaky" and neither is true.
#
# Both causes are here, and neither is about what the harness does.
#
# ONE — THE FIXTURE INHERITED THE CALLER'S ENVIRONMENT. Inside a build this file is a suite
# run BY `run-tests.sh`, so the `run-tests.sh` copies it runs in its scratch box inherited
# whatever the outer run exported. Measured 2026-09-20 on `62e5affd` with one variable
# exported and nothing else changed: `RUN_TESTS_NO_HOST_SCREEN=1` — the documented env form
# of the flag EVERY build now passes — turns S2 and S4 RED on a tree where nothing is
# wrong. So the fixture gets `env -i` and an explicit list: PATH, HOME, TMPDIR, LC_ALL,
# a private RUN_TESTS_STATE, plus exactly what the case sets. Nothing else reaches it, in
# either direction: the fixture also stops writing proofs into the operator's real store.
#
# TWO — THE ASSERTION ITSELF COULD REPORT "NO MATCH" FOR TEXT THAT WAS THERE. Every check
# here read `printf '%s' "$OUT" | grep -Fq NEEDLE`. This file runs under `pipefail` and
# `grep -q` exits the instant it matches, so the writer on the left can die of SIGPIPE
# AFTER the match was found; the pipeline then reports 141 and the `!` reads it as absence.
# Measured on this Mac, 2026-09-20: 400 of 400 iterations reported "no match" for a 400 KB
# payload whose FIRST line was the needle, and the same idiom inside case S6's scanner
# dropped `gui-boot.test.sh` — the suite that opens a window on the operator's screen —
# from the scan in 14 of 200 runs. A here-string has no second process and no pipe, so it
# cannot do this. `says` also refuses to let a grep ERROR (exit 2 and up: a broken tool, a
# full disk) be reported as "the harness did not print it", which is how a machine problem
# arrives dressed as a product failure.
#
# Neither helper is a workaround for a flake. Each removes a way this file could be wrong
# about a tree that is right, which is the only thing it is for.

FIXTURE_HOME="$TMP/fixture-home"; mkdir -p "$FIXTURE_HOME"
# /usr/bin and /bin only: everything the harness shells out to is there (git, sed, awk,
# find, sort, date, shasum, basename), and an operator's PATH is exactly the kind of
# ambient state this file must not depend on. K0 below proves the fixture can still commit,
# so a machine where this is not enough SAYS so rather than passing while asserting nothing.
FIXTURE_PATH="/usr/bin:/bin:/usr/sbin:/sbin"

harness() {  # harness [VAR=VALUE]... -- <script> [args]   -> sets OUT and CODE
  local assigns=()
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --) shift; break ;;
      *)  assigns+=("$1"); shift ;;
    esac
  done
  OUT="$(env -i \
          PATH="$FIXTURE_PATH" \
          HOME="$FIXTURE_HOME" \
          TMPDIR="$TMP" \
          LC_ALL=C \
          RUN_TESTS_STATE="$TMP/fixture-state" \
          RICHOS_HOST_SHIELD_TOOL="$SHIELD_TOOL" \
          ${assigns[@]+"${assigns[@]}"} \
          bash "$@" 2>&1)"; CODE=$?
  return 0
}

says() {    # says <substring>  — 0 when $OUT contains it, 1 when it does not
  local rc
  grep -Fq -- "$1" <<<"$OUT"; rc=$?
  if [ "$rc" -gt 1 ]; then
    bad "the assertion machinery itself" \
        "grep exited $rc looking for '$1'. That is a TOOL failure and it must never be \
read as 'the harness did not print it' — the whole reason this helper exists."
  fi
  return "$rc"
}

saysre() {  # saysre <extended regex> — the same contract, for the two regex checks
  local rc
  grep -Eq -- "$1" <<<"$OUT"; rc=$?
  if [ "$rc" -gt 1 ]; then
    bad "the assertion machinery itself" "grep exited $rc looking for /$1/"
  fi
  return "$rc"
}

# A scratch inventory. `run-tests.sh` discovers suites next to ITSELF, so the copy goes in
# with them and the real directory is never read.
BOX="$TMP/box"; mkdir -p "$BOX"
install_harness "$BOX"
printf '%s\n' 'echo "=== aaa tests: all 3 passed ==="' 'exit 0' > "$BOX/aaa.test.sh"
printf '%s\n' 'echo "=== bbb tests: all 2 passed ==="' 'exit 0' > "$BOX/bbb.test.sh"
printf '%s\n' 'echo "gap.test.sh: cannot answer on this host." >&2' 'exit 2' > "$BOX/gap.test.sh"
printf '%s\n' 'echo "=== broken tests: 1 FAILED, 0 passed ==="' 'exit 1' > "$BOX/broken.hold"

# run <declaration> -> sets CODE and OUT
run() {
  harness "RUN_TESTS_DECLARED_GAPS=${1:-}" -- "$BOX/run-tests.sh"
}
# expect <name> <wanted-code> [substring]
expect() {
  local name="$1" want="$2" needle="${3:-}"
  if [ "$CODE" != "$want" ]; then
    bad "$name" "exit $CODE, wanted $want. Output: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"
  elif [ -n "$needle" ] && ! says "$needle"; then
    bad "$name" "exit $want as wanted, but the output never said '$needle'"
  else
    ok "$name"
  fi
}

echo ""
echo "=== H. the harness's host-gap allowance ==="

# H1 — the case the whole design exists for. Nothing declared; a suite says it cannot
# answer; the run must refuse rather than quietly drop it.
run ""
expect "H1 an undeclared gap is refused" 2 "nobody declared them: gap.test.sh"

# H2 — declared, with a reason. Green, and the summary must not read "all N suites passed".
run "gap.test.sh: no widget on this host"
if [ "$CODE" != 0 ]; then
  bad "H2 a declared gap with a reason is green" "exit $CODE, wanted 0"
elif says "all 3 suites passed"; then
  bad "H2 a declared gap with a reason is green" \
      "the summary claimed all three suites passed while one of them did not run"
elif ! says "2 of 3 suites passed"; then
  bad "H2 a declared gap with a reason is green" \
      "the summary does not say how many ran: $(printf '%s' "$OUT" | tail -2 | tr '\n' ' ')"
elif ! says "no widget on this host"; then
  bad "H2 a declared gap with a reason is green" "the reason is not printed with the result"
else
  ok "H2 a declared gap with a reason is green, and the summary names it and its reason"
fi

# H3 — a bare name. The same rule gui-boot.test.sh's own A4 case applies to the gaps IT
# accounts for: a marker with no reason declares nothing.
run "gap.test.sh"
expect "H3 a declaration with no reason is refused" 2 "A bare name declares nothing"

# H4 — the allowance outliving its reason. Without this, a declaration written for one cause
# sits there ready to swallow a different one years later.
mv "$BOX/gap.test.sh" "$BOX/gap.hold"
run "aaa.test.sh: a reason that is no longer true"
expect "H4 a declaration whose suite answered is refused" 2 "The allowance has outlived its reason"
mv "$BOX/gap.hold" "$BOX/gap.test.sh"

# H5 — a suite that RAN and lost must never be reported under a gap headline.
mv "$BOX/broken.hold" "$BOX/broken.test.sh"
run "gap.test.sh: no widget on this host"
expect "H5 a real failure outranks a declared gap" 1 "FAILED: broken.test.sh"
mv "$BOX/broken.test.sh" "$BOX/broken.hold"

# H6 — the ordinary path is untouched. This is the case that catches a refactor of the
# summary breaking the thing everyone actually reads.
rm -f "$BOX/gap.test.sh"
run ""
expect "H6 with no gap and no failure the original summary is unchanged" 0 "all 2 suites passed — 5 checks"

# H8 — THE CASE THIS FILE DID NOT HAVE, AND THE NINE DAYS IT COST.
#
# H1-H7 all ask whether the DECLARATION is honest. None of them asks whether the SUITE still
# works. `gui-boot.test.sh` was dead on every host from 2026-09-08 to 2026-09-10 — its boot
# fixture built a `.app` with no `Info.plist` and the app exited on its first line — and the
# declaration in `packaging-ci.yml`, which is TRUE (no public runner can hold the loro
# compiler), read exactly the same on the runner as it had every day before. A suite that
# cannot answer HERE and a suite that cannot answer ANYWHERE were indistinguishable.
#
# The fake suite below is that shape exactly: it prints a failed case in the standard form
# every suite in this directory uses, and then exits 2. Declared, it must still be a FAILURE.
rm -f "$BOX"/*.test.sh 2>/dev/null
printf '%s\n' 'echo "=== aaa tests: all 3 passed ==="' 'exit 0' > "$BOX/aaa.test.sh"
printf '%s\n' \
  'printf "  PASS  C1 the fixture is intact\n"' \
  'printf "  FAIL  C2 the fixture is intact\n         it is not\n"' \
  'echo "deadgap.test.sh: and now this host cannot answer either." >&2' \
  'exit 2' > "$BOX/deadgap.test.sh"
run "deadgap.test.sh: no widget on this host"
if [ "$CODE" != 1 ]; then
  bad "H8 a declared gap whose suite failed a case is a failure" \
      "exit $CODE, wanted 1. A gap is a claim about the HOST; this suite found something wrong. \
Output: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"
elif ! says "FAILED: deadgap.test.sh"; then
  bad "H8 a declared gap whose suite failed a case is a failure" \
      "exit 1 as wanted, but the summary does not name it as failed"
elif ! says "C2 the fixture is intact"; then
  bad "H8 a declared gap whose suite failed a case is a failure" \
      "the failed case is not quoted, so a reader has to reproduce the run to see what broke"
else
  ok "H8 a declared gap whose suite failed a case is a FAILURE, and the case is quoted"
fi

# H9 — and the allowance still works. H8 keys on a `FAIL` line rather than on the exit code,
# so the case that proves it did not swallow H2 has to be run right next to it: same
# declaration, same exit 2, no failed case, green.
printf '%s\n' \
  'printf "  PASS  C1 the fixture is intact\n"' \
  'echo "deadgap.test.sh: this host cannot answer." >&2' \
  'exit 2' > "$BOX/deadgap.test.sh"
run "deadgap.test.sh: no widget on this host"
expect "H9 a declared gap that failed nothing is still green" 0 "1 of 2 suites passed"

# The scratch inventory is put back the way H1-H6 left it, so H7 reads a directory it
# recognizes and anything added after this does too.
rm -f "$BOX/deadgap.test.sh"
printf '%s\n' 'echo "=== bbb tests: all 2 passed ==="' 'exit 0' > "$BOX/bbb.test.sh"

# H7 — the pre-existing refusal that must survive all of the above.
EMPTY="$TMP/empty"; mkdir -p "$EMPTY"
install_harness "$EMPTY"
harness -- "$EMPTY/run-tests.sh"
expect "H7 an empty inventory is refused, never 'all 0 suites passed'" 2 "refusing to report green over an empty inventory"

# =========================================================================================
# P. THE POOL — concurrency that cannot reorder, overrun or swallow anything
# =========================================================================================
#
# The harness runs its suites concurrently now. Three things about that are load-bearing
# and none of them is visible from a green summary, so each one is a case: the OUTPUT ORDER
# a reader depends on, the BOUND that keeps fourteen suites from thrashing one Mac, and the
# FAILURE that still has to stop the run when it happens in the middle of a pool.
echo ""
echo "=== P. the pool ==="
PBOX="$TMP/pool"; mkdir -p "$PBOX"
install_harness "$PBOX"
P2LOG="$TMP/concurrency.log"; export P2LOG

mk_timed() {  # mk_timed <name> <seconds>
  printf '%s\n' \
    'echo start >> "$P2LOG"' \
    "sleep $2" \
    'echo end >> "$P2LOG"' \
    "echo \"=== $1 tests: all 1 passed ===\"" \
    'exit 0' > "$PBOX/$1.test.sh"
}
# Deliberately DESCENDING durations against ASCENDING names: if anything printed in
# completion order instead of discovery order, P1 would read d,c,b,a.
mk_timed a 1.2
mk_timed b 0.9
mk_timed c 0.6
mk_timed d 0.1

prun() {  # prun <jobs>
  : > "$P2LOG"
  harness RUN_TESTS_DECLARED_GAPS= "P2LOG=$P2LOG" -- "$PBOX/run-tests.sh" --jobs "$1"
}

prun 4
ORDER="$(sed -n 's/^--- //p' <<<"$OUT" | tr '\n' ' ')"
if [ "$CODE" != 0 ]; then
  bad "P1 concurrent output is printed in discovery order" "exit $CODE, wanted 0"
elif [ "$ORDER" != "a.test.sh b.test.sh c.test.sh d.test.sh " ]; then
  bad "P1 concurrent output is printed in discovery order" \
      "printed '$ORDER' — four suites that finish in the opposite order must still read a,b,c,d"
elif ! says "all 4 suites passed — 4 checks"; then
  bad "P1 concurrent output is printed in discovery order" \
      "the summary lost a suite or a check: $(printf '%s' "$OUT" | tail -1)"
else
  ok "P1 four suites finishing in reverse still print a,b,c,d, and every check is counted"
fi

# P2 — THE BOUND. Without it, fourteen suites — three of which fan out to every core —
# start at once on a 10-core Mac. MEASURED 2026-09-19 on this machine, same tree, same
# twelve suites: pool 1 = 341 s, pool 2 = 223 s, pool 3 = 167 s, pool 4 = 183 s,
# pool 6 = 228 s, pool 10 = 236 s. MORE CONCURRENCY IS SLOWER PAST THREE, because two
# suites dominate the run and contend with each other, so a bound that is not enforced is
# not a faster run — it is a slower one.
prun 2
MAX="$(awk '/^start$/{n++; if (n>m) m=n} /^end$/{n--} END{print m+0}' "$P2LOG")"
if [ "$CODE" != 0 ]; then
  bad "P2 the pool bound is enforced" "exit $CODE, wanted 0"
elif [ "$MAX" -gt 2 ]; then
  bad "P2 the pool bound is enforced" \
      "--jobs 2 and $MAX suites ran at once. An unenforced bound is how a build thrashes."
elif [ "$MAX" -lt 2 ]; then
  bad "P2 the pool bound is enforced" \
      "--jobs 2 and never more than $MAX ran at once — the pool is not filling, so nothing is concurrent"
else
  ok "P2 --jobs 2 ran at most 2 suites at any instant, and did fill both slots"
fi

# P3 — a failure in the MIDDLE of a pool. The serial harness could not lose one; a pool can,
# by printing a summary before a straggler has been collected.
printf '%s\n' 'echo "  FAIL  c1 something is wrong"' 'echo "=== c tests: 1 FAILED, 0 passed ==="' 'exit 1' > "$PBOX/c.test.sh"
prun 4
expect "P3 a failure inside a full pool still stops the run" 1 "FAILED: c.test.sh"

# =========================================================================================
# K. SKIP-WHEN-UNCHANGED — an allowance keyed on content, never on a calendar
# =========================================================================================
#
# `make-release.test.sh` and `make-engine-asset.test.sh` are 286 s of every build (measured
# 2026-09-19, run 20260919T180454Z-ac11d13e) and neither reads anything an app-only commit
# touches. They may be skipped when nothing they read has changed since a run that proved
# them green.
#
# THIS IS THE ALLOWANCE THIS FILE'S SUBJECT IS MOST AFRAID OF — a suite that stops running
# and nobody finds out. Every case below is a way that could happen.
echo ""
echo "=== K. skip-when-unchanged ==="
KREPO="$TMP/krepo"
mkdir -p "$KREPO/scripts" "$KREPO/inputs" "$KREPO/.nohooks"
git -C "$KREPO" init -q >/dev/null 2>&1
git -C "$KREPO" config user.email nobody@example.invalid
git -C "$KREPO" config user.name "run-tests fixture"
# THIS MACHINE HAS A GLOBAL core.hooksPath, so a fixture repository inherits the operator's
# commit-identity guard and every `git commit` below is REFUSED — silently, since they were
# written with `>/dev/null 2>&1`. Measured 2026-09-19: the fixture never committed, so every
# input stayed staged-as-added, `suite_input_digest` correctly refused to skip a dirty tree,
# and K2-K5 all reported PASS while asserting nothing. A fixture must not depend on the
# machine's git configuration, so this one turns hooks off for itself and `kcommit` checks
# that the commit actually happened instead of trusting that it did.
git -C "$KREPO" config core.hooksPath "$KREPO/.nohooks"
install_harness "$KREPO/scripts"
printf 'one\n' > "$KREPO/inputs/a.txt"
printf '%s\n' \
  '# run-tests: inputs inputs' \
  'echo "=== heavy tests: all 7 passed ==="' \
  'exit 0' > "$KREPO/scripts/heavy.test.sh"
printf '%s\n' 'echo "=== light tests: all 1 passed ==="' 'exit 0' > "$KREPO/scripts/light.test.sh"

KFIXTURE_OK=1
kcommit() {  # commit everything, and REFUSE to continue quietly if it did not happen
  git -C "$KREPO" add -A >/dev/null 2>&1
  if ! git -C "$KREPO" commit -qm "$1" >"$TMP/kcommit.log" 2>&1; then
    KFIXTURE_OK=""
    bad "K0 the fixture repository can commit" \
        "git commit failed, so every K case below would assert nothing: $(head -3 "$TMP/kcommit.log" | tr '\n' ' ')"
    return 1
  fi
  if [ -n "$(git -C "$KREPO" status --porcelain --untracked-files=all)" ]; then
    KFIXTURE_OK=""
    bad "K0 the fixture repository is clean after committing" \
        "something is still uncommitted, so a skip could never be reached: $(git -C "$KREPO" status --porcelain | tr '\n' ' ')"
    return 1
  fi
  return 0
}
kcommit fixture && ok "K0 the fixture repository commits and is clean — the K cases can reach a skip"
KSTATE="$TMP/kstate"

krun() {  # krun <run-id>
  harness RUN_TESTS_DECLARED_GAPS= RUN_TESTS_SKIP_UNCHANGED=1 "RUN_TESTS_STATE=$KSTATE" \
          "RICHOS_NIGHTLY_RUN_ID=$1" RICHOS_RUNTIME_DIR= -- "$KREPO/scripts/run-tests.sh"
}

krun first
if [ "$CODE" != 0 ]; then
  bad "K1a a suite with no recorded proof runs" \
      "exit $CODE, wanted 0: $(printf '%s' "$OUT" | tail -3 | tr '\n' ' ')"
elif says "SKIPPED: heavy.test.sh"; then
  bad "K1a a suite with no recorded proof runs" \
      "it skipped a suite it had never seen pass — a skip must rest on a prior GREEN run"
else
  ok "K1a a suite with no recorded proof runs"
fi
krun second
if [ "$CODE" != 0 ]; then
  bad "K1b a proven suite is SKIPPED, naming its digest and its run" "exit $CODE, wanted 0"
elif ! says "SKIPPED: heavy.test.sh"; then
  bad "K1b a proven suite is SKIPPED, naming its digest and its run" "it ran again over identical inputs"
elif ! says "since run first"; then
  bad "K1b a proven suite is SKIPPED, naming its digest and its run" \
      "the skip does not name the run that proved it: $(grep SKIPPED <<<"$OUT" | head -1)"
elif ! saysre 'sha256 [0-9a-f]{64}'; then
  bad "K1b a proven suite is SKIPPED, naming its digest and its run" \
      "the skip does not name the digest it compared"
elif ! says "1 of 2 suites passed"; then
  bad "K1b a proven suite is SKIPPED, naming its digest and its run" \
      "the summary claims a suite that did not run: $(printf '%s' "$OUT" | tail -3 | tr '\n' ' ')"
else
  ok "K1b a proven suite is SKIPPED, naming the digest and the run that proved it"
fi

printf 'two\n' > "$KREPO/inputs/a.txt"
kcommit change
krun third
if says "SKIPPED: heavy.test.sh"; then
  bad "K2 a committed change to a declared input makes the suite run again" \
      "the input changed and the suite was skipped anyway"
else
  ok "K2 a committed change to a declared input makes the suite run again"
fi

krun fourth   # re-prove at the new content
printf 'three, and never committed\n' > "$KREPO/inputs/a.txt"
krun fifth
if says "SKIPPED: heavy.test.sh"; then
  bad "K3 an uncommitted edit to a declared input makes the suite run again" \
      "the working tree differs from the index and the suite was skipped over it. A digest \
taken from HEAD alone cannot see an edit nobody committed, which is most edits."
else
  ok "K3 an uncommitted edit to a declared input makes the suite run again"
fi
git -C "$KREPO" checkout -- inputs/a.txt >/dev/null 2>&1

if says "SKIPPED: light.test.sh"; then
  bad "K4 a suite that declares no inputs is never skipped" \
      "light.test.sh declares nothing and was skipped anyway — a silent skip is this file's subject"
else
  ok "K4 a suite that declares no inputs is never skipped, however often it passes"
fi

printf '%s\n' \
  '# run-tests: inputs inputs' \
  'echo "  FAIL  h1 the heavy suite found something"' \
  'echo "=== heavy tests: 1 FAILED, 0 passed ==="' \
  'exit 1' > "$KREPO/scripts/heavy.test.sh"
kcommit red
rm -f "$KSTATE/heavy.test.sh.proof"
krun sixth
krun seventh
if [ "$CODE" != 1 ]; then
  bad "K5 a suite that failed leaves no proof" "exit $CODE, wanted 1 — a red suite must stay red"
elif says "SKIPPED: heavy.test.sh"; then
  bad "K5 a suite that failed leaves no proof" \
      "a suite that FAILED was skipped on the next run. A proof is written only over a GREEN \
run, or a red suite goes green by being run twice."
else
  ok "K5 a suite that failed leaves no proof, so the next run still runs it and still fails"
fi

# A land receipt cannot suppress A8 in the run that asks for it (the iPhone app's release
# check), even at identical source.
cp "$KREPO/scripts/light.test.sh" "$KREPO/scripts/native-ios-app.test.sh"
printf '# run-tests: inputs inputs\n' >> "$KREPO/scripts/native-ios-app.test.sh"
kcommit nightly-mode
for mode in 0 1 1; do
  harness RUN_TESTS_DECLARED_GAPS= RUN_TESTS_SKIP_UNCHANGED=1 "RUN_TESTS_STATE=$KSTATE" \
    "RICHOS_NATIVE_IOS_APP_A8=$mode" RICHOS_RUNTIME_DIR= -- "$KREPO/scripts/run-tests.sh" --only native-ios-app.test.sh
  if [ "$CODE" = 0 ] && ! says "SKIPPED: native-ios-app.test.sh"; then
    ok "K6 native-ios-app executes with A8=$mode, including consecutive runs that ask for A8"
  else
    bad "K6 native-ios-app is never skipped when A8 is asked for" "$OUT"
  fi
done

# K7 (2026-09-28): a suite that checks reviewed reader pins is never skipped over a changed
# pinned reader, although its inputs line does not name the reader. `# run-tests: pins` adds
# every reader the pin file binds to the digest, read from the file (lib/proof_declarations.py).
mkdir -p "$KREPO/pins" "$KREPO/reader"
printf 'reader one\n' > "$KREPO/reader/r.sh"
printf '%s\n' '{"units": {"u": {"sources": {"reader/r.sh": "pinned"}}}}' > "$KREPO/pins/q.json"
printf '%s\n' \
  '# run-tests: inputs inputs' \
  '# run-tests: pins pins/q.json' \
  'echo "=== pinned tests: all 1 passed ==="' \
  'exit 0' > "$KREPO/scripts/pinned.test.sh"
kcommit pinned-suite
kpinned() {  # kpinned <run-id>
  harness RUN_TESTS_DECLARED_GAPS= RUN_TESTS_SKIP_UNCHANGED=1 "RUN_TESTS_STATE=$KSTATE" \
    "RICHOS_NIGHTLY_RUN_ID=$1" RICHOS_RUNTIME_DIR= -- "$KREPO/scripts/run-tests.sh" --only pinned.test.sh
}
kpinned pinned-first
kpinned pinned-second
if [ "$CODE" != 0 ] || ! says "SKIPPED: pinned.test.sh"; then
  bad "K7a a proven pin-checking suite is skipped over unchanged readers" \
      "exit $CODE; the skip was never reachable, so K7b would assert nothing: $(printf '%s' "$OUT" | tail -3 | tr '\n' ' ')"
else
  ok "K7a a proven pin-checking suite is skipped over unchanged readers"
  printf 'reader two\n' > "$KREPO/reader/r.sh"
  kcommit pinned-reader-change
  kpinned pinned-third
  if says "SKIPPED: pinned.test.sh"; then
    bad "K7b a changed pinned reader makes the pin-checking suite run again" \
        "reader/r.sh changed and the suite that checks its pin was skipped: a stale pin would pass the build"
  else
    ok "K7b a changed pinned reader makes the pin-checking suite run again"
  fi
fi

# =========================================================================================
# S. `--no-host-screen` — the promise that nothing reaches the operator's screen
# =========================================================================================
#
# The CEO, 2026-09-19: *"So, every engineer will keep opening the app making me unable to do
# anything here or WHAT???"*. `gui-boot.test.sh` boots the real app on the real screen for
# ~162 s of every build. The mode that stops it has to be ENFORCED rather than documented,
# and the enforcement has three separate ways to fail silently.
echo ""
echo "=== S. --no-host-screen ==="
SBOX="$TMP/screen"; mkdir -p "$SBOX"
install_harness "$SBOX"
printf '%s\n' 'echo "=== quiet tests: all 2 passed ==="' 'exit 0' > "$SBOX/quiet.test.sh"
# Classified by the ONE library in this repository that boots the shipped binary — a
# structural fact about what a suite calls, not a guess about what it might do.
printf '%s\n' \
  '. "$DIR/lib/gui-launch.sh"' \
  'echo "=== window tests: all 5 passed ==="' \
  'exit 0' > "$SBOX/window.test.sh"

srun() {  # srun <extra args> [gui host]
  harness RUN_TESTS_DECLARED_GAPS= "RICHOS_GUI_HOST=${2:-}" -- "$SBOX/run-tests.sh" $1
}

srun "--no-host-screen"
if [ "$CODE" != 0 ]; then
  bad "S1 --no-host-screen holds back the suite that boots the app" "exit $CODE, wanted 0"
elif ! says "NOT RUN (no screen): window.test.sh"; then
  bad "S1 --no-host-screen holds back the suite that boots the app" \
      "window.test.sh was not held back: $(printf '%s' "$OUT" | tail -2 | tr '\n' ' ')"
elif says "all 2 suites passed"; then
  bad "S1 --no-host-screen holds back the suite that boots the app" \
      "the summary claimed both suites passed while one of them never ran"
else
  ok "S1 --no-host-screen holds back the suite that boots the app, and the run is still green"
fi

srun "" ""
if [ "$CODE" != 0 ] || ! says "all 2 suites passed"; then
  bad "S2 without the flag nothing changes" \
      "both suites must run as before; exit $CODE, $(printf '%s' "$OUT" | tail -1)"
else
  ok "S2 without the flag nothing changes — both suites run, as they always did"
fi

# S3 — A NAMED GUEST IS NOT A SUGGESTION. The dangerous version of this feature falls back
# to the host's screen when the VM is missing, which is the exact interruption the mode
# exists to prevent; the second most dangerous records NOT RUN and lets a caller who asked
# for a proof believe they asked for nothing.
srun "--no-host-screen" "richos-test-1"
if [ "$CODE" != 2 ]; then
  bad "S3 a named guest with no runner is REFUSED" \
      "exit $CODE, wanted 2. It must neither fall back to this screen nor pretend the suite \
was skipped on purpose."
elif ! says "richos-test-1"; then
  bad "S3 a named guest with no runner is REFUSED" "the refusal does not name the guest that was asked for"
else
  ok "S3 RICHOS_GUI_HOST with no runner REFUSES — never the host's screen, never a silent NOT RUN"
fi

# S4 — the proof file `nightly-local.py publish` will demand of a screenless candidate.
PROOF="$TMP/gui.proof"
srun "--proof-out $PROOF"
if [ ! -f "$PROOF" ]; then
  bad "S4 a host-screen suite that RAN writes a proof" "no file at $PROOF"
elif ! grep -q '^commit=' "$PROOF"; then
  bad "S4 a host-screen suite that RAN writes a proof" \
      "the proof names no commit, so nothing can tell which tree it was taken against"
elif ! grep -q '^result=pass$' "$PROOF"; then
  bad "S4 a host-screen suite that RAN writes a proof" \
      "the proof does not record the verdict: $(tr '\n' ' ' < "$PROOF" | cut -c1-160)"
else
  ok "S4 a host-screen suite that ran writes a proof naming its commit and its verdict"
fi

# S5 — a silent no-op here would be a green run of nothing, which is this file's subject.
harness -- "$SBOX/run-tests.sh" --only nosuchsuite.test.sh
expect "S5 --only with a name that matches nothing is refused" 2 "names no suite under"

# S6 — THE REAL INVENTORY, not a fixture. A fixture proving the classifier works says
# nothing about whether THIS directory is classified correctly, and the cost of being wrong
# is a window on the operator's Mac. The same reason make-engine-asset.test.sh's L9-L18 run
# against the real tree after L1-L8 have run against a synthetic one.
SCAN_MISSING=""
for real in "$DIR"/*.test.sh; do
  # NOT `grep -v ... | grep -q ...`. That pipeline decides whether a file is scanned AT
  # ALL, and under `pipefail` the left side dies of SIGPIPE when the right side matches
  # early — measured 2026-09-20: 14 of 200 runs dropped `gui-boot.test.sh`, the one suite
  # that puts the app on the operator's screen, out of the scan entirely. A file skipped
  # by this loop is a file S6 silently declares clean.
  UNCOMMENTED="$(grep -vE '^[[:space:]]*#' "$real")"
  grep -qE '(^|[[:space:]]|\()(open|osascript)[[:space:]]|/Contents/MacOS/|\$GUI_BINARY|target/(debug|release)/richos-tauri' <<<"$UNCOMMENTED" \
    || continue
  grep -qE '^[[:space:]]*(\.|source)[[:space:]]+[^[:space:]]*lib/gui-launch\.sh' "$real" && continue
  grep -q '^# run-tests: host-screen' "$real" && continue
  if ! grep -qE '^# run-tests: no-host-screen:[[:space:]]*[^[:space:]]' "$real"; then
    SCAN_MISSING="$SCAN_MISSING $(basename "$real")"
  fi
done
if [ -n "$SCAN_MISSING" ]; then
  bad "S6 every real suite that could open a window is classified or declares itself inert" \
      "undeclared:$SCAN_MISSING — each must either source lib/gui-launch.sh (and so be held \
back by --no-host-screen) or carry '# run-tests: no-host-screen: <why its matches open \
nothing>'. A bare marker declares nothing, and neither does silence."
else
  ok "S6 every suite in the real inventory that could open a window is classified or declared inert"
fi

# S7 — THE OTHER DIRECTION, AND THE ONE THAT ACTUALLY BIT. S6 asks whether anything that
# could open a window was missed. It cannot ask the opposite: whether something that opens
# NOTHING was held back anyway. On the first full `--no-host-screen` run, 2026-09-19, this
# file itself was recorded `NOT RUN (no screen)` — the classifier matched the library's NAME
# anywhere in a file, and S1 above writes a fixture containing `. "$DIR/lib/gui-launch.sh"`.
# The harness's own self-test stopped running, under a reason that reads entirely
# legitimate, which is the exact shape of the defect this whole file exists for.
#
# So the classification of the REAL inventory is asserted as a set, both ways, by running
# the harness over it and reading back which suites it held aside.
CLASSIFIED=""
for real in "$DIR"/*.test.sh; do
  if grep -qE '^[[:space:]]*(\.|source)[[:space:]]+[^[:space:]]*lib/gui-launch\.sh' "$real" \
     || grep -q '^# run-tests: host-screen' "$real"; then
    CLASSIFIED="$CLASSIFIED $(basename "$real")"
  fi
done
CLASSIFIED="${CLASSIFIED# }"
S7_WHY=""
case " $CLASSIFIED " in
  *" gui-boot.test.sh "*)   ;;
  *) S7_WHY="gui-boot.test.sh is NOT classified, and it is the suite that boots the app on the screen" ;;
esac
case " $CLASSIFIED " in
  *" front-door.test.sh "*) ;;
  *) S7_WHY="$S7_WHY; front-door.test.sh is NOT classified, and it drives the shipped window" ;;
esac
case " $CLASSIFIED " in
  *" run-tests.test.sh "*)
    S7_WHY="$S7_WHY; run-tests.test.sh IS classified, and it opens nothing — it only writes \
the library's name into a fixture. A suite held back for a string in a quoted argument stops \
running under a reason that reads legitimate, which is this file's whole subject." ;;
esac
if [ -n "$S7_WHY" ]; then
  bad "S7 the real inventory's host-screen set is exactly the suites that source the library" \
      "classified: [$CLASSIFIED] — ${S7_WHY#; }"
else
  ok "S7 the host-screen set is exactly [$CLASSIFIED] — mentioning the library is not sourcing it"
fi

# =========================================================================================
# G. RICHOS_GUI_HOST: a host-screen suite really runs in a guest
# =========================================================================================
#
# Section 02 of the 2026-09-29 hunt (Codex, part 2): run-tests.sh routed every host-screen
# suite to `testvm/run-suite.sh` under RICHOS_GUI_HOST, and that runner had never been
# written, so the route refused every time while telling the caller to "provision the
# guest", which cannot create a missing script. S3 above holds the refusal; these cases
# hold the route itself: the REAL run-suite.sh, suite-walk.sh and suite-in-guest.sh,
# copied beside a copy of the harness, with two stand-ins and nothing else faked:
#
#   * run-walk.py -> a script that takes no slot and boots nothing: it records its
#     arguments, hands the command a VM name and writes the report run-walk.py writes;
#   * guest.sh    -> a directory under this scratch that plays the guest's $HOME, where
#     each command runs in a shell of its own (and `sudo` only runs what follows it).
#
# The executables are given (RICHOS_GUI_PREBUILT), so nothing is compiled here; the build
# half is gui_prebuild's, proved by the real run in a guest.
echo ""
echo "=== G. RICHOS_GUI_HOST: the guest route, against a stand-in guest ==="
GBOX="$TMP/guest-route"; GBIN="$TMP/guest-bin"; GSUDO="$TMP/guest-sudo"
GROOT="$TMP/guest-home"; GLOG="$TMP/guest-log"; GPRE="$TMP/guest-prebuilt"; GENG="$TMP/guest-engine"
mkdir -p "$GBOX/testvm" "$GBIN" "$GSUDO" "$GROOT" "$GLOG" "$GPRE" "$GENG"
# The guest half resolves its own paths with `pwd -P`; /var is a symlink on macOS.
GROOT="$(cd "$GROOT" && pwd -P)"
install_harness "$GBOX"
G_READY=1
for f in testvm/run-suite.sh testvm/suite-walk.sh testvm/suite-in-guest.sh lib/gui-launch.sh; do
  if [ -f "$DIR/$f" ]; then cp "$DIR/$f" "$GBOX/$f"; else G_READY=""; G_MISSING="${G_MISSING:-} $f"; fi
done
echo "the engine a suite provisions from" > "$GENG/VERSION"
mkdir -p "$GENG/scripts/lib"
cp "$DIR/../../engine/scripts/lib/machine_settings.py" \
   "$DIR/../../engine/scripts/lib/machine-settings.sh" "$GENG/scripts/lib/"
for n in richos-tauri gui_boot_machine; do
  printf '#!/bin/sh\necho "stand-in %s, built on the host"\n' "$n" > "$GPRE/$n"
  chmod +x "$GPRE/$n"
done
cat > "$GBIN/run-walk" <<'SH'
#!/bin/bash
# Stand-in for testvm/run-walk.py: no slot, no clone, no boot.
printf '%s\n' "$@" > "$GLOG/walk.args"
report=""
while [ $# -gt 0 ]; do
  case "$1" in --report) report="$2"; shift 2 ;; --) shift; break ;; *) shift ;; esac
done
cmd="$1"; shift
"$cmd" stand-in-vm "$@"; rc=$?
printf '{"vm": "stand-in-vm", "execution": "completed", "scenario_exit": %s, "cleanup_complete": true, "boot_seconds": 0, "scenario_seconds": 0, "cleanup_seconds": 0}\n' "$rc" > "$report"
exit "$rc"
SH
cat > "$GBIN/guest" <<'SH'
#!/bin/bash
# Stand-in for testvm/guest.sh <vm> '<command>': the guest's $HOME is a scratch directory.
shift
HOME="$GUEST_ROOT" PATH="$GUEST_SUDO:/usr/bin:/bin:/usr/sbin:/sbin" exec /bin/bash -c "$1"
SH
cat > "$GSUDO/sudo" <<'SH'
#!/bin/bash
# Stand-in for `sudo -n launchctl asuser <uid> sudo -n -u <user> /bin/bash ...`: runs the rest.
while [ $# -gt 0 ] && [ "$1" != /bin/bash ]; do shift; done
exec "$@"
SH
chmod +x "$GBIN/run-walk" "$GBIN/guest" "$GSUDO/sudo"
# A suite that boots the app, as gui-boot does: it sources the library, and it can only pass
# where the route put it — in prebuilt mode, with the host's executables beside it. Written
# with printf, never a here-document, for the reason S1's fixture is: a line of THIS file
# that begins by sourcing the library would make S7 classify this file as one that boots
# the app.
# shellcheck disable=SC2016  # fixture text: the $ expressions belong to the suite being written
G_HEAD='DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"'
# shellcheck disable=SC2016  # fixture text, as above
G_LIB='. "$DIR/lib/gui-launch.sh"'
# shellcheck disable=SC2016  # fixture text, as above
printf '%s\n' "$G_HEAD" "$G_LIB" \
  'gui_prebuilt_mode || { echo "  FAIL  G not in prebuilt mode"; exit 1; }' \
  'gui_prebuilt_check || { echo "  FAIL  G the prebuilt executables are missing"; exit 1; }' \
  '"$RICHOS_GUI_PREBUILT/richos-tauri"' \
  '[ -f "$RICHOS_GUI_ENGINE_SOURCE/VERSION" ] || { echo "  FAIL  G no engine"; exit 1; }' \
  'echo "  PASS  G ran in the guest'"'"'s payload at $(dirname "$DIR")"' \
  'echo "=== window tests: all 4 passed ==="' \
  'exit 0' > "$GBOX/window.test.sh"
printf '%s\n' "$G_HEAD" "$G_LIB" \
  'echo "  FAIL  R a case that failed in the guest"' \
  'echo "=== redwin tests: 1 FAILED, 3 passed ==="' \
  'exit 1' > "$GBOX/redwin.test.sh"
printf '%s\n' "$G_HEAD" \
  '# run-tests: host-only: it needs a published release on a person'"'"'s screen' \
  "$G_LIB" \
  'echo "  FAIL  D this suite must never have been started"' \
  'exit 1' > "$GBOX/door.test.sh"
grun() {  # grun <suite>  — with a named guest, and WITHOUT --no-host-screen
  rm -f "$GLOG/walk.args"
  harness RUN_TESTS_DECLARED_GAPS= RICHOS_GUI_HOST=richos-test-g \
    "TESTVM_RUN_WALK=$GBIN/run-walk" "TESTVM_GUEST_EXEC=$GBIN/guest" \
    "GUEST_ROOT=$GROOT" "GUEST_SUDO=$GSUDO" "GLOG=$GLOG" \
    "RICHOS_GUI_PREBUILT=$GPRE" "RICHOS_GUI_ENGINE_SOURCE=$GENG" \
    -- "$GBOX/run-tests.sh" --only "$1"
}

if [ -z "$G_READY" ]; then
  bad "G1 RICHOS_GUI_HOST runs a host-screen suite in a guest and reports its real result" \
      "this checkout has no${G_MISSING}: the route run-tests.sh takes under RICHOS_GUI_HOST \
has nothing to run the suite with, so it can only refuse (section 02 of the 2026-09-29 hunt)"
else
  grun window.test.sh
  if [ "$CODE" != 0 ]; then
    bad "G1 RICHOS_GUI_HOST runs a host-screen suite in a guest and reports its real result" \
        "exit $CODE, wanted 0: $(tail -6 <<<"$OUT" | tr '\n' ' ' | cut -c1-400)"
  elif ! says "in a guest: window.test.sh"; then
    bad "G1 RICHOS_GUI_HOST runs a host-screen suite in a guest and reports its real result" \
        "the plan did not route window.test.sh to a guest"
  elif ! says "stand-in richos-tauri, built on the host" || ! says "PASS  G ran in the guest's payload at $GROOT/run-suite/tree"; then
    bad "G1 RICHOS_GUI_HOST runs a host-screen suite in a guest and reports its real result" \
        "the suite did not run from the payload in the guest, in prebuilt mode: $(tr '\n' ' ' <<<"$OUT" | cut -c1-400)"
  elif ! grep -qx -- '--no-app' "$GLOG/walk.args" 2>/dev/null || grep -q -- 'tailnet' "$GLOG/walk.args"; then
    bad "G1 RICHOS_GUI_HOST runs a host-screen suite in a guest and reports its real result" \
        "run-walk.py was not asked for a guest with no app and no network flag: $(tr '\n' ' ' < "$GLOG/walk.args" 2>/dev/null)"
  elif ! says "window.test.sh exited 0 in guest stand-in-vm" || ! says "all 1 suites passed — 4 checks"; then
    bad "G1 RICHOS_GUI_HOST runs a host-screen suite in a guest and reports its real result" \
        "the suite's verdict and its checks did not come back as the run's: $(tail -3 <<<"$OUT" | tr '\n' ' ')"
  else
    ok "G1 RICHOS_GUI_HOST runs a host-screen suite in a guest from the host's executables, and its result is the run's"
  fi

  grun redwin.test.sh
  if [ "$CODE" != 1 ] || ! says "FAIL  R a case that failed in the guest" || ! says "suite(s) FAILED: redwin.test.sh"; then
    bad "G2 a suite that FAILS in the guest fails the run, with its failed case named" \
        "exit $CODE, wanted 1: $(tail -4 <<<"$OUT" | tr '\n' ' ' | cut -c1-300)"
  else
    ok "G2 a suite that fails in the guest fails the run, and its failed case is named"
  fi

  grun door.test.sh
  if [ "$CODE" != 0 ] || ! says "NOT RUN (no screen): door.test.sh" \
     || ! says "runs on this Mac's own screen only (no guest route): it needs a published release"; then
    bad "G3 a host-only suite is recorded NOT RUN with its reason, never promised a guest" \
        "exit $CODE: $(tail -4 <<<"$OUT" | tr '\n' ' ' | cut -c1-300)"
  elif [ -f "$GLOG/walk.args" ] || says "D this suite must never have been started"; then
    bad "G3 a host-only suite is recorded NOT RUN with its reason, never promised a guest" \
        "a guest was booted for it, or it was started anyway"
  else
    ok "G3 a host-only suite is recorded NOT RUN with its declared reason, and no guest is booted for it"
  fi

  # N07: the results folder a caller asked for is carried out of the guest before it is deleted.
  # Brought back, it is here; lost (the host side cannot take it), a passing suite cannot stand
  # as a pass and a failing one keeps its own exit code.
  GRES_OK="$TMP/guest-results-ok"; GRES_BAD="$TMP/guest-results-blocked/under-a-file"
  : > "$TMP/guest-results-blocked"
  gsuite() {  # gsuite <suite> <results dir> [runner args...]
    local suite="$1" results="$2"; shift 2
    harness RUN_TESTS_DECLARED_GAPS= RICHOS_GUI_HOST=richos-test-g \
      "TESTVM_RUN_WALK=$GBIN/run-walk" "TESTVM_GUEST_EXEC=$GBIN/guest" \
      "GUEST_ROOT=$GROOT" "GUEST_SUDO=$GSUDO" "GLOG=$GLOG" \
      "RICHOS_GUI_PREBUILT=$GPRE" "RICHOS_GUI_ENGINE_SOURCE=$GENG" "RICHOS_TEST_RESULTS_DIR=$results" \
      "PATH=$GBIN:$FIXTURE_PATH" TESTVM_SLOT=stand-in-slot \
      -- "$GBOX/testvm/run-suite.sh" richos-test-g "$GBOX/$suite" "$@"
  }
  mkdir -p "$GRES_OK"
  gsuite window.test.sh "$GRES_OK"
  if [ "$CODE" != 0 ] || [ ! -d "$GRES_OK" ]; then
    bad "G4 a results folder that comes back leaves the passing verdict as it was" "exit $CODE: $(tail -3 <<<"$OUT" | tr '\n' ' ' | cut -c1-300)"
  else
    ok "G4 a results folder that comes back leaves the passing verdict as it was"
  fi
  gsuite window.test.sh "$GRES_BAD"
  if [ "$CODE" != 2 ] || ! says "results folder could not be brought back"; then
    bad "G5 a passing suite whose requested results did not come back is not a pass" \
        "exit $CODE, wanted 2: $(tail -3 <<<"$OUT" | tr '\n' ' ' | cut -c1-300)"
  else
    ok "G5 a passing suite whose requested results did not come back is not a pass (exit 2, the loss named)"
  fi
  gsuite redwin.test.sh "$GRES_BAD"
  if [ "$CODE" != 1 ] || ! says "its results folder was lost"; then
    bad "G6 a failing suite whose results were lost still fails with its own exit code" \
        "exit $CODE, wanted 1: $(tail -3 <<<"$OUT" | tr '\n' ' ' | cut -c1-300)"
  else
    ok "G6 a failing suite whose results were lost still fails with its own exit code, and the loss is named"
  fi

  # The nightly already owns a booted guest. The real payload transport must attach to
  # that guest, carry the release and cliclick, run BOTH suites and bring both verdicts back.
  GRELEASE="$TMP/guest-release"; mkdir -p "$GRELEASE"
  printf '#!/bin/sh\necho guest-cliclick\n' > "$GBIN/cliclick"; chmod +x "$GBIN/cliclick"
  cat > "$GBOX/front-door.test.sh" <<'SH'
#!/bin/bash
[ "$1" = --release ] && [ -f "$2/RichOS.app.tar.gz" ] && [ "$3" = --evidence ] || exit 64
[ "$(cliclick)" = guest-cliclick ] || exit 64
mkdir -p "$4" || exit 2
echo front-door-evidence > "$4/observed"
echo "front-door ran in the guest with the transferred archive"
exit "$(cat "$2/RichOS.app.tar.gz")"
SH
  printf '%s\n' "$G_HEAD" "$G_LIB" 'echo "gui refusal in the guest"' 'exit 2' > "$GBOX/refusewin.test.sh"
  for gcase in green front-red front-refused gui-red gui-refused; do
    front=0; gui=0; suite=window.test.sh
    case "$gcase" in
      front-red) front=1 ;;
      front-refused) front=2 ;;
      gui-red) gui=1; suite=redwin.test.sh ;;
      gui-refused) gui=2; suite=refusewin.test.sh ;;
    esac
    expected="$gui"; [ "$front" = 0 ] || expected="$front"
    printf '%s\n' "$front" > "$GRELEASE/RichOS.app.tar.gz"
    result="$TMP/guest-pair-$gcase"
    rm -f "$GLOG/walk.args"
    gsuite "$suite" "$result" --existing-guest nightly-owned --release "$GRELEASE"
    if [ "$CODE" != "$expected" ] || [ -e "$GLOG/walk.args" ] \
       || [ "$(cat "$result/front-door.rc" 2>/dev/null)" != "$front" ] \
       || [ "$(cat "$result/$suite.rc" 2>/dev/null)" != "$gui" ] \
       || [ ! -s "$result/front-door.log" ] || [ ! -s "$result/$suite.log" ] \
       || [ ! -f "$result/front-door/observed" ] \
       || ! says "guest nightly-owned; caller must stop and delete the guest"; then
      bad "G7 $gcase: both guest screen verdicts survive without booting another guest" \
          "exit $CODE, wanted $expected: $(tail -4 <<<"$OUT" | tr '\n' ' ' | cut -c1-400)"
    else
      ok "G7 $gcase: both guest screen verdicts and logs survive; caller owns the single guest"
    fi
  done
  printf '0\n' > "$GRELEASE/RichOS.app.tar.gz"
  gsuite window.test.sh "$GRES_BAD" --existing-guest nightly-owned --release "$GRELEASE"
  if [ "$CODE" = 2 ] && says "results folder could not be brought back" \
     && says "caller retains guest cleanup ownership"; then
    ok "G8 an existing guest's missing results refuse instead of passing"
  else
    bad "G8 an existing guest's missing results refuse instead of passing" "exit $CODE: $OUT"
  fi

  # Exercise the real library after transfer, with a guest-owned setting. A copied
  # suite can otherwise pass in prebuilt mode after cargo-cache-env failed to load.
  mkdir -p "$GROOT/Library/Application Support/RichOS"
  printf 'RICHOS_BUILD_CACHE=%s\n' "$GROOT/selected-cache" \
    > "$GROOT/Library/Application Support/RichOS/.env"
  # shellcheck disable=SC2016  # these expressions run inside the guest fixture
  printf '%s\n' "$G_HEAD" "$G_LIB" \
    'unset RICHOS_MACHINE_SETTINGS_RESOLVED RICHOS_BUILD_CACHE' \
    '. "$DIR/lib/cargo-cache-env.sh" || exit 1' \
    '[ "$RICHOS_BUILD_CACHE" = "$HOME/selected-cache" ] || exit 1' \
    'echo "PASS guest library read the guest settings"' \
    > "$GBOX/settingswin.test.sh"
  gsuite settingswin.test.sh "$TMP/guest-settings" --existing-guest nightly-owned
  if [ "$CODE" = 0 ] && says "PASS guest library read the guest settings"; then
    ok "G9 transferred app libraries resolve the engine and load guest settings"
  else
    bad "G9 transferred app libraries resolve the engine and load guest settings" "exit $CODE: $OUT"
  fi
fi

# =========================================================================================
# B. `--for`: WHICH APP'S BUILD A RUN IS
# =========================================================================================
#
# The CEO, 2026-09-26: *"the native mobile apps are 2 COMPLETELY INDEPENDENT DIFFERENT APPS
# ... So, WHY THE FUCK ARE THEY PART OF THE SAME FUCKING BUILD???"* `--for desktop` leaves
# out every suite `phone-app-suites.tsv` names. The two ways that goes wrong are opposite and
# both silent: a phone suite stays in the desktop build (his complaint, back again), or a
# suite that proves the Mac side of the phone connection leaves it (the desktop app ships
# untested bytes). B1-B6 hold the mechanism on a fixture; B7 holds the REAL inventory to
# both directions; B8 watches B7's checker go red for each.
echo ""
echo "=== B. --for: which app's build a run is ==="
BBOX="$TMP/builds"; mkdir -p "$BBOX"
install_harness "$BBOX"
for b in desk ios1 droid both; do
  printf '%s\n' "echo \"=== $b tests: all 1 passed ===\"" 'exit 0' > "$BBOX/$b.test.sh"
done
TAB="$(printf '\t')"
write_list() {  # write_list <row>...   rows use | for the tab, to keep them readable here
  : > "$BBOX/phone-app-suites.tsv"
  printf '# a comment, and the blank line after it, are not rows\n\n' >> "$BBOX/phone-app-suites.tsv"
  local r
  for r in "$@"; do printf '%s\n' "$r" | tr '|' "$TAB" >> "$BBOX/phone-app-suites.tsv"; done
}
GOOD_ROWS=("ios1.test.sh|ios|the fixture's iPhone app" "droid.test.sh|android|the fixture's Android app" \
           "both.test.sh|ios,android|both fixture apps")
write_list "${GOOD_ROWS[@]}"
brun() { harness RUN_TESTS_DECLARED_GAPS= -- "$BBOX/run-tests.sh" "$@"; }
blist() { brun "$@" --list; LISTED="$(tr '\n' ' ' <<<"$OUT")"; }

BRES="$TMP/builds-results.json"
brun --for desktop --results-out "$BRES"
if [ "$CODE" != 0 ]; then
  bad "B1 --for desktop runs the desktop suites and names the rest" "exit $CODE: $(tail -3 <<<"$OUT" | tr '\n' ' ')"
elif ! says "all 1 suites passed" || says "--- ios1.test.sh" || says "--- droid.test.sh"; then
  bad "B1 --for desktop runs the desktop suites and names the rest" \
      "a phone-app suite ran, or the desktop one did not: $(tail -2 <<<"$OUT" | tr '\n' ' ')"
elif ! says "NOT IN THIS BUILD: ios1.test.sh — ios: the fixture's iPhone app" \
     || ! says "NOT IN THIS BUILD: both.test.sh — ios,android: both fixture apps"; then
  bad "B1 --for desktop runs the desktop suites and names the rest" \
      "a suite left out was not named with its app and its reason"
elif ! /usr/bin/python3 -c '
import json, sys
r = json.load(open(sys.argv[1]))
assert r["build"] == "desktop", r["build"]
assert [s["name"] for s in r["suites"]] == ["desk.test.sh"], r["suites"]
out = {s["name"]: s["belongs_to"] for s in r["not_in_this_build"]}
assert out == {"ios1.test.sh": "ios", "droid.test.sh": "android", "both.test.sh": "ios,android"}, out
assert all(s["reason"] for s in r["not_in_this_build"])
' "$BRES" 2>"$TMP/b1.err"; then
  bad "B1 --for desktop runs the desktop suites and names the rest" \
      "the results file does not record what was left out: $(tr '\n' ' ' < "$TMP/b1.err" | cut -c1-240)"
else
  ok "B1 --for desktop runs only the desktop suite, and prints and records each phone suite with its app and reason"
fi

B2_WHY=""
blist --for ios;     [ "$LISTED" = "both.test.sh ios1.test.sh " ]  || B2_WHY="$B2_WHY; ios gave [$LISTED]"
blist --for android; [ "$LISTED" = "both.test.sh droid.test.sh " ] || B2_WHY="$B2_WHY; android gave [$LISTED]"
blist --for phone;   [ "$LISTED" = "both.test.sh droid.test.sh ios1.test.sh " ] || B2_WHY="$B2_WHY; phone gave [$LISTED]"
blist --for desktop; [ "$LISTED" = "desk.test.sh " ] || B2_WHY="$B2_WHY; desktop gave [$LISTED]"
if [ -n "$B2_WHY" ]; then
  bad "B2 each app's build lists exactly its own suites" "${B2_WHY#; }"
else
  ok "B2 ios, android, phone and desktop each list exactly their own suites; a both-apps suite is in each app's"
fi

brun
if [ "$CODE" = 0 ] && says "all 4 suites passed" && ! says "NOT IN THIS BUILD"; then
  ok "B3 without --for every suite runs, exactly as before"
else
  bad "B3 without --for every suite runs, exactly as before" "exit $CODE: $(tail -2 <<<"$OUT" | tr '\n' ' ')"
fi

B4_WHY=""
b4() {  # b4 <label> <needle> <row>...
  local label="$1" needle="$2"; shift 2
  write_list "$@"
  brun --for desktop --list
  if [ "$CODE" != 2 ] || ! says "$needle"; then
    B4_WHY="$B4_WHY; $label: exit $CODE, $(tail -1 <<<"$OUT")"
  fi
}
b4 "a row with no reason"    "A row with no reason declares nothing" "ios1.test.sh|ios|"
b4 "a row with no app"       "is not '<suite><TAB><apps><TAB><reason>'" "ios1.test.sh"
b4 "an app that is not one"  "names the app 'watch'" "ios1.test.sh|watch|a watch app"
b4 "a stale row"             "there is no such suite" "gone.test.sh|ios|a suite that was removed"
b4 "a suite named twice"     "names ios1.test.sh twice" "ios1.test.sh|ios|once" "ios1.test.sh|ios|twice"
rm -f "$BBOX/phone-app-suites.tsv"
brun --for desktop --list
if [ "$CODE" != 2 ] || ! says "is missing"; then B4_WHY="$B4_WHY; a missing list: exit $CODE"; fi
write_list "${GOOD_ROWS[@]}"
if [ -n "$B4_WHY" ]; then
  bad "B4 a list that does not reconcile is refused before anything runs" "${B4_WHY#; }"
else
  ok "B4 a row with no reason, no app, an unknown app, a stale name or a duplicate, and a missing list, are each refused"
fi

# B5 — the one structural fact a row cannot override. The desktop UI is compiled into the
# Mac executable, so a suite that claims one of its files is the desktop app's.
printf '%s\n' '# run-tests: covers richos/app/ui/main.js' \
  'echo "=== ios1 tests: all 1 passed ==="' 'exit 0' > "$BBOX/ios1.test.sh"
brun --for desktop --list
if [ "$CODE" = 2 ] && says "files the Mac app ships: richos/app/ui/main.js"; then
  ok "B5 a listed suite whose covers row claims a file the Mac app ships is refused"
else
  bad "B5 a listed suite whose covers row claims a file the Mac app ships is refused" \
      "exit $CODE — it would have left the desktop build: $(tail -1 <<<"$OUT")"
fi
printf '%s\n' 'echo "=== ios1 tests: all 1 passed ==="' 'exit 0' > "$BBOX/ios1.test.sh"

B6_WHY=""
brun --for desktop --only desk.test.sh
{ [ "$CODE" = 2 ] && says "give one of them"; } || B6_WHY="$B6_WHY; --for with --only: exit $CODE"
brun --for tablet --list
{ [ "$CODE" = 2 ] && says "--for takes desktop, ios, android or phone"; } || B6_WHY="$B6_WHY; --for tablet: exit $CODE"
write_list "ios1.test.sh|ios|x"
brun --for android --list
{ [ "$CODE" = 2 ] && says "selects NO suite"; } || B6_WHY="$B6_WHY; an empty selection: exit $CODE"
write_list "${GOOD_ROWS[@]}"
if [ -n "$B6_WHY" ]; then
  bad "B6 an ambiguous or empty selection is refused" "${B6_WHY#; }"
else
  ok "B6 --for with --only, an unknown build, and a build that selects nothing are refused"
fi

# B7 — THE REAL INVENTORY. The two sets are written out here on purpose: they are what this
# case checks the list AGAINST, so they cannot be derived from the list.
#
#   PHONE_ONLY  test only a phone app, and must not run in the desktop build.
#   MAC_SIDE    read richos/mobile/ but test the Mac side of the phone connection, which the
#               desktop app ships: the phone server compiled into its crate (mobile-mac), the
#               Mac's request verifier (native-conformance), and the shared mobile core with
#               the reference client modules the corpus is generated from (mobile-headless).
PHONE_ONLY="native-ios-app.test.sh native-ios-core.test.sh native-ios-share.test.sh \
native-ios-ui.test.sh native-ios-physical-tool.test.sh mobile-ios.test.sh \
native-android-app.test.sh native-android-core.test.sh native-android-ui.test.sh \
native-android-pair-lab.test.sh native-release-policy.test.sh mobile-perf.test.sh review-mock.test.sh"
MAC_SIDE="mobile-mac.test.sh native-conformance.test.sh mobile-headless.test.sh"
build_check() {  # build_check <directory holding run-tests.sh>  -> sets BC_WHY ("" = right)
  local d="$1" desk phone all s
  BC_WHY=""
  harness RUN_TESTS_DECLARED_GAPS= -- "$d/run-tests.sh" --for desktop --list
  [ "$CODE" = 0 ] || { BC_WHY="--for desktop --list exited $CODE: $(tail -1 <<<"$OUT")"; return; }
  desk=" $(tr '\n' ' ' <<<"$OUT")"
  harness RUN_TESTS_DECLARED_GAPS= -- "$d/run-tests.sh" --for phone --list
  [ "$CODE" = 0 ] || { BC_WHY="--for phone --list exited $CODE: $(tail -1 <<<"$OUT")"; return; }
  phone=" $(tr '\n' ' ' <<<"$OUT")"
  # The WHOLE inventory, said outright: without a selection in a teammate workspace the harness
  # lists only what the branch maps to (workspace_scope.py, 2026-10-01), and `harness` runs under
  # `env -i`, so not even proof-run.py's RICHOS_TEST_SCOPE=full reaches it.
  harness RUN_TESTS_DECLARED_GAPS= -- "$d/run-tests.sh" --full --list
  all="$(tr '\n' ' ' <<<"$OUT")"
  for s in $PHONE_ONLY; do
    case "$desk" in *" $s "*) BC_WHY="$BC_WHY; $s (phone only) is in the desktop build" ;; esac
    case "$phone" in *" $s "*) ;; *) BC_WHY="$BC_WHY; $s (phone only) is in no phone build" ;; esac
  done
  for s in $MAC_SIDE; do
    case "$desk" in *" $s "*) ;; *) BC_WHY="$BC_WHY; $s (the Mac side) is NOT in the desktop build" ;; esac
  done
  # Every suite runs in exactly one of the two, so nothing can fall between them.
  for s in $all; do
    case "$desk$phone" in
      *" $s "*" $s "*) BC_WHY="$BC_WHY; $s is in both the desktop and a phone build" ;;
      *" $s "*) ;;
      *) BC_WHY="$BC_WHY; $s runs in no build at all" ;;
    esac
  done
  BC_WHY="${BC_WHY#; }"
}
build_check "$DIR"
if [ -n "$BC_WHY" ]; then
  bad "B7 the real desktop build leaves out every phone-only suite and keeps every Mac-side one" "$BC_WHY"
else
  ok "B7 the real desktop build leaves out the $(wc -w <<<"$PHONE_ONLY" | tr -d ' ') phone-only suites, keeps the \
$(wc -w <<<"$MAC_SIDE" | tr -d ' ') Mac-side ones, and every suite is in exactly one build"
fi

# B8 — B7's checker watched going red, once per direction, on a copy of the real inventory
# (suites are only listed, never run). A checker nobody has seen fail proves nothing.
MBOX="$TMP/builds-mutant"; mkdir -p "$MBOX"
install_harness "$MBOX"
cp "$DIR"/*.test.sh "$MBOX/"
B8_WHY=""
# Direction 1: a phone-only suite drops off the list and is back in the desktop build.
grep -v "^native-ios-app\.test\.sh${TAB}" "$DIR/phone-app-suites.tsv" > "$MBOX/phone-app-suites.tsv"
build_check "$MBOX"
case "$BC_WHY" in *"native-ios-app.test.sh (phone only) is in the desktop build"*) ;;
  *) B8_WHY="$B8_WHY; dropping native-ios-app's row was not caught (checker said: ${BC_WHY:-nothing})" ;; esac
# Direction 2: a Mac-side suite is listed as a phone suite and leaves the desktop build.
cp "$DIR/phone-app-suites.tsv" "$MBOX/phone-app-suites.tsv"
printf 'mobile-mac.test.sh\tios\ta mistaken row\n' >> "$MBOX/phone-app-suites.tsv"
build_check "$MBOX"
case "$BC_WHY" in *"mobile-mac.test.sh (the Mac side) is NOT in the desktop build"*) ;;
  *) B8_WHY="$B8_WHY; listing mobile-mac was not caught (checker said: ${BC_WHY:-nothing})" ;; esac
# ...and for a suite that proves the Mac's own frontend payload, run-tests.sh itself refuses.
cp "$DIR/phone-app-suites.tsv" "$MBOX/phone-app-suites.tsv"
printf 'frontend-payload.test.sh\tios\ta mistaken row\n' >> "$MBOX/phone-app-suites.tsv"
build_check "$MBOX"
case "$BC_WHY" in *"belongs to the desktop build"*) ;;
  *) B8_WHY="$B8_WHY; listing frontend-payload was not refused (checker said: ${BC_WHY:-nothing})" ;; esac
if [ -n "$B8_WHY" ]; then
  bad "B8 B7's checker goes red in each direction" "${B8_WHY#; }"
else
  ok "B8 B7's checker goes red when a phone suite returns to the desktop build and when a Mac-side suite leaves it"
fi

# =========================================================================================
# E. THIS FILE'S OWN FOOTING — the verdict must not depend on who started the run
# =========================================================================================
#
# Everything above asks whether `run-tests.sh` behaves. These two ask whether THIS FILE is
# capable of reporting that honestly, which is the question 2026-09-19 answered with "no":
# green by hand, red inside the build, same commit, identical harness output.
echo ""
echo "=== E. this file's own footing ==="

# E1 — THE WHOLE SUITE, RUN AGAIN INSIDE THE ENVIRONMENT A BUILD HANDS IT. Not a fixture:
# this file, re-executed with every variable the build's gates phase exports or could
# export, all hostile at once. On `62e5affd` this came back with S2 and S4 RED — from
# `RUN_TESTS_NO_HOST_SCREEN=1` alone, the documented env form (run-tests.sh, the
# `--no-host-screen` section) of the flag every build passes. Nothing was wrong with the
# tree; the fixture had simply inherited the outer run's screen mode.
#
# The inner run is told not to recurse. It is the only case here that costs a second full
# pass of this file, and the reason it is worth the seconds is that no smaller case can
# state the property: EVERY case, under a caller's environment, unchanged.
if [ -n "${RUN_TESTS_TEST_INNER:-}" ]; then
  ok "E1 skipped in the inner run — this is that inner run, and it must not recurse"
else
  # THE LIST IS DERIVED, NOT COPIED. It used to be nine names typed here, and a typed copy
  # of somebody else's list is a copy that goes stale the first time they add a name --
  # silently, because a variable this case never exports is a variable this case never
  # proves anything about. `nightly-local.py gate-environment` prints what a gate actually
  # receives, so the build declares it once and this reads it.
  #
  # A DECLARED NAME WITH NO VALUE BELOW IS A FAILURE, never a skip. That is the whole
  # mechanism: add a variable to the build's gate environment and this case goes red until
  # somebody decides what it should be here.
  E1NAMES="$(python3 "$DIR/nightly-local.py" gate-environment \
             | awk -F'\t' '$1 == "set" || $1 == "per-step" { print $2 }')"
  if [ -z "$E1NAMES" ]; then
    bad "E1 the build's gate environment can be read" \
        "nightly-local.py gate-environment printed nothing; this case cannot derive its list"
    E1NAMES=""
  fi
  E1ENV=()
  E1MISSING=""
  E1STORAGE="$TMP/e1-storage"
  mkdir -p "$E1STORAGE/tmp/richos-gates"
  for E1N in $E1NAMES; do
    case "$E1N" in
      # PATH is the one name that must keep working: a hostile PATH does not test the
      # suite's footing, it stops `bash` finding anything at all.
      PATH) continue ;;
      # The values a build genuinely hands a gate. E1's property is "the environment a
      # build hands it", so these are the build's own values, not invented ones.
      # The build's own folder (nightly-local.py gate_tmpdir), never the caller's.
      TMPDIR) E1ENV+=("TMPDIR=$E1STORAGE/tmp/richos-gates/") ;;
      RICHOS_BULK_ROOT) E1ENV+=("RICHOS_BULK_ROOT=$E1STORAGE") ;;
      RICHOS_MACHINE_SETTINGS_RESOLVED) E1ENV+=("RICHOS_MACHINE_SETTINGS_RESOLVED=1") ;;
      RICHOS_CARGO_CACHE_ROOT|SCCACHE_DIR|RICHOS_SCCACHE_TMPDIR|RICHOS_BUILD_CACHE|\
      RICHOS_RECOVERY_EVIDENCE_ROOT|RICHOS_RECOVERY_WIP_ROOT|RICHOS_CPU_GUARD_STATE|\
      RICHOS_ENGINE_PASS_DIR|RICHOS_PROOF_RUN_DIR|RICHOS_AUTOCHECK_PROOF_ROOT|\
      RICHOS_TEST_RESULTS_ROOT|RICHOS_ANDROID_CACHES_ROOT|RICHOS_NATIVE_IOS_CACHE|\
      RICHOS_NATIVE_ANDROID_CACHE|RANDROID_CACHE|RICHOS_GPL_SOURCE_CACHE|RICHOS_MOBILE_CACHE|RICHOS_PHONE_LOCK_DIR|\
      RICHOS_PHONE_WATCH_HOME|RICHOS_PHYSICAL_STORE|RICHOS_PHYSICAL_PRODUCTS|\
      GRADLE_USER_HOME|RICHOS_ENGINE_CHECK_PROOF_DIR|RICHOS_CAMPAIGN_PARENT)
        E1ENV+=("$E1N=$E1STORAGE/$E1N") ;;
      PYTHONDONTWRITEBYTECODE) E1ENV+=("PYTHONDONTWRITEBYTECODE=1") ;;
      CARGO_PROFILE_DEV_DEBUG) E1ENV+=("CARGO_PROFILE_DEV_DEBUG=0") ;;
      CARGO_PROFILE_TEST_DEBUG) E1ENV+=("CARGO_PROFILE_TEST_DEBUG=0") ;;
      GIT_TERMINAL_PROMPT) E1ENV+=("GIT_TERMINAL_PROMPT=0") ;;
      GIT_SSH_COMMAND) E1ENV+=("GIT_SSH_COMMAND=ssh -o BatchMode=yes -o ConnectTimeout=15") ;;
      RICHOS_NIGHTLY_RUN_ID) E1ENV+=("RICHOS_NIGHTLY_RUN_ID=e1-hostile") ;;
      RICHOS_TEST_SCOPE) E1ENV+=("RICHOS_TEST_SCOPE=full") ;;
      RICHOS_RUNTIME_DIR) E1ENV+=("RICHOS_RUNTIME_DIR=$TMP/e1-runtime") ;;
      RICHOS_GUI_SPEECH_MODELS) E1ENV+=("RICHOS_GUI_SPEECH_MODELS=$TMP/e1-ggml-small.en.bin:$TMP/e1-ggml-large-v3-turbo-q5_0.bin") ;;
      RICHOS_NAMED_PERSONS_FILE) E1ENV+=("RICHOS_NAMED_PERSONS_FILE=$TMP/e1-named-persons") ;;
      RUN_TESTS_SKIP_UNCHANGED) E1ENV+=("RUN_TESTS_SKIP_UNCHANGED=1") ;;
      RUN_TESTS_DECLARED_GAPS) E1ENV+=("RUN_TESTS_DECLARED_GAPS=front-door.test.sh: whatever the build declares") ;;
      RICHOS_NATIVE_IOS_APP_A8) E1ENV+=("RICHOS_NATIVE_IOS_APP_A8=1") ;;
      RICHOS_IOS_POOL_WAIT) E1ENV+=("RICHOS_IOS_POOL_WAIT=6900") ;;
      RICHOS_IOS_POOL_LEASES) E1ENV+=("RICHOS_IOS_POOL_LEASES=2") ;;
      RICHOS_FOURTEEN_MUTANTS) E1ENV+=("RICHOS_FOURTEEN_MUTANTS=1") ;;
      RICHOS_MUTATION_PASSES) E1ENV+=("RICHOS_MUTATION_PASSES=1") ;;
      CI_SHARD_UNIT_TIMEOUT) E1ENV+=("CI_SHARD_UNIT_TIMEOUT=7200") ;;
      *) E1MISSING="$E1MISSING $E1N" ;;
    esac
  done
  # `run-tests.sh`'s OWN env-form flags, which the build passes as arguments rather than
  # exporting. They are not in the declared list and they belong here anyway: a caller CAN
  # export them, and RUN_TESTS_NO_HOST_SCREEN alone is what turned S2 and S4 red on 62e5affd.
  E1ENV+=("RUN_TESTS_TEST_INNER=1" "RUN_TESTS_NO_HOST_SCREEN=1" "RICHOS_GUI_HOST=richos-test-e1" \
          "RUN_TESTS_JOBS=1" "RUN_TESTS_STATE=$TMP/e1-store")
  if [ -n "$E1MISSING" ]; then
    bad "E1 exercises every variable the build's gate environment declares" \
        "the build now hands a gate$E1MISSING, and this case has no value for it. Add one to \
the case statement above so the suite is proven under it, rather than leaving it untested."
  fi
  E1OUT="$(env "${E1ENV[@]}" bash "$DIR/run-tests.test.sh" 2>&1)"; E1CODE=$?
  E1FAILS="$(grep -c '^  FAIL' <<<"$E1OUT")"
  if [ "$E1CODE" != 0 ] || [ "$E1FAILS" != 0 ]; then
    bad "E1 the suite's verdict does not change under the environment a build exports" \
        "exit $E1CODE with $E1FAILS failed case(s) while RUN_TESTS_NO_HOST_SCREEN, \
RICHOS_GUI_HOST, RUN_TESTS_JOBS, RUN_TESTS_SKIP_UNCHANGED, RUN_TESTS_STATE, \
RICHOS_NIGHTLY_RUN_ID, RICHOS_RUNTIME_DIR and RUN_TESTS_DECLARED_GAPS were exported: \
$(grep '^  FAIL' <<<"$E1OUT" | tr '\n' ' ' | cut -c1-240)"
  elif ! grep -Fq "=== run-tests tests: all " <<<"$E1OUT"; then
    bad "E1 the suite's verdict does not change under the environment a build exports" \
        "exit 0 and no failed case, but no summary line — the inner run did not finish: \
$(tail -2 <<<"$E1OUT" | tr '\n' ' ')"
  else
    ok "E1 every case survives the build's own environment exported hostile at it"
  fi
fi

# E2 — THE ASSERTION'S OWN PLUMBING. `printf '%s' "$OUT" | grep -Fq NEEDLE` was every check
# in this file until 2026-09-20. `set -o pipefail` is on and `grep -q` exits at the first
# match, so the writer can be killed by SIGPIPE AFTER the match and the pipeline reports
# 141 — "no match" for text that is there. It is a function of payload size and scheduling,
# which is to say a function of the MACHINE, which is exactly how a suite comes back red
# inside a build and green by hand.
E2PAD="$(printf 'padding line that makes the payload large %s\n' $(seq 1 6000))"
OUT="=== app/scripts: 2 of 3 suites passed — 5 checks ===
$E2PAD"
# stderr is discarded on this ONE line only: the retired idiom announces its own mechanism
# ("printf: write error: Broken pipe") and that sentence, printed in the middle of a build
# log by a case that is PASSING, reads like a failure. Its exit code is reported instead.
if printf '%s' "$OUT" 2>/dev/null | grep -Fq "2 of 3 suites passed"; then E2OLD=0; else E2OLD=$?; fi
if ! says "2 of 3 suites passed"; then
  bad "E2 an assertion finds text that is in the output" \
      "says() reported absence for a needle on the payload's FIRST line ($(wc -c <<<"$OUT" | tr -d ' ') bytes)"
elif says "a line this payload does not contain"; then
  bad "E2 an assertion finds text that is in the output" \
      "says() reported a match for text that is not in the payload at all"
else
  ok "E2 says() reads the payload, not the plumbing — on these $(wc -c <<<"$OUT" | tr -d ' ') bytes \
the retired 'printf | grep -Fq' idiom exited $E2OLD for the same needle on the same bytes"
fi
OUT=""

# C1 — CLEANUP WAITS FOR ITS CHILDREN, ESCALATES, AND DELETES THE SCRATCH LAST (hunt part 2,
# section 15). The fake suite starts one owned child that ignores TERM (and so does its `sleep`,
# because an ignored signal is inherited). The runner is sent TERM, as a canceling caller would.
# When the runner has exited, that child must be gone and the scratch directory must be gone; the
# old cleanup sent TERM, deleted the scratch and returned with the child still running.
C1BOX="$TMP/c1box"; mkdir -p "$C1BOX"
install_harness "$C1BOX"
C1PID="$TMP/c1.pid"; C1TMP="$TMP/c1tmp"; mkdir -p "$C1TMP"
printf '%s\n' "bash -c 'trap \"\" TERM; echo \$\$ > \"$C1PID\"; while :; do sleep 1; done' &" \
  'wait' > "$C1BOX/stubborn.test.sh"
env -i RICHOS_HOST_SHIELD_TOOL="$SHIELD_TOOL" PATH="$FIXTURE_PATH" HOME="$FIXTURE_HOME" TMPDIR="$C1TMP" LC_ALL=C \
    RUN_TESTS_STATE="$TMP/fixture-state" RUN_TESTS_KILL_GRACE=1 \
    bash "$C1BOX/run-tests.sh" > "$TMP/c1.out" 2>&1 &
C1RUNNER=$!
n=0; while [ ! -s "$C1PID" ] && [ "$n" -lt 100 ]; do sleep 0.1; n=$((n + 1)); done
C1CHILD="$(cat "$C1PID" 2>/dev/null)"
kill -TERM "$C1RUNNER" 2>/dev/null
wait "$C1RUNNER" 2>/dev/null
C1ALIVE=""; C1LEFT=""
if [ -n "$C1CHILD" ] && kill -0 "$C1CHILD" 2>/dev/null; then C1ALIVE=1; fi
if [ -n "$(ls -A "$C1TMP" 2>/dev/null)" ]; then C1LEFT="$(find "$C1TMP" -mindepth 1 -maxdepth 1 | tr '\n' ' ')"; fi
[ -z "$C1ALIVE" ] || kill -KILL "$C1CHILD" 2>/dev/null   # this test's own child, by its recorded pid
if [ -z "$C1CHILD" ]; then
  bad "C1 cleanup outlives no child" "the stand-in child never started, so nothing was tested"
elif [ -n "$C1ALIVE" ]; then
  bad "C1 cleanup outlives no child" "the runner exited while its TERM-ignoring child $C1CHILD was still running"
elif [ -n "$C1LEFT" ]; then
  bad "C1 cleanup outlives no child" "the child is gone but the scratch was not removed: $C1LEFT"
else
  ok "C1 cleanup waits, escalates past a child that ignores TERM, and deletes the scratch only after it is gone"
fi

# R15 — THE RETRY IS STOPPED WITH THE RUNNER (hunt part 2 v3, R15). The suite fails its first
# attempt, and its retry (run alone) starts a 30-second child. The runner is sent TERM, as a
# caller stopping it would. Within 5 s the runner must have exited and the retry's child be gone;
# the foreground retry deferred the TERM trap and was not in the cleanup's inventory, so both
# stayed alive until the retry finished on its own.
R15BOX="$TMP/r15box"; mkdir -p "$R15BOX"
install_harness "$R15BOX"
R15PID="$TMP/r15.pid"
printf '%s\n' \
  "if [ ! -e \"$TMP/r15.first\" ]; then touch \"$TMP/r15.first\"; echo '  FAIL  r15 first attempt'; exit 1; fi" \
  "echo \$\$ > \"$R15PID\"; exec sleep 30" > "$R15BOX/retried.test.sh"
env -i RICHOS_HOST_SHIELD_TOOL="$SHIELD_TOOL" PATH="$FIXTURE_PATH" HOME="$FIXTURE_HOME" LC_ALL=C RUN_TESTS_STATE="$TMP/fixture-state" \
    RUN_TESTS_KILL_GRACE=1 bash "$R15BOX/run-tests.sh" > "$TMP/r15.out" 2>&1 &
R15RUNNER=$!
n=0; while [ ! -s "$R15PID" ] && [ "$n" -lt 150 ]; do sleep 0.1; n=$((n + 1)); done
R15CHILD="$(cat "$R15PID" 2>/dev/null)"
kill -TERM "$R15RUNNER" 2>/dev/null
n=0; while kill -0 "$R15RUNNER" 2>/dev/null && [ "$n" -lt 50 ]; do sleep 0.1; n=$((n + 1)); done
R15RUNNING=""; R15ALIVE=""
if kill -0 "$R15RUNNER" 2>/dev/null; then R15RUNNING=1; fi
if [ -n "$R15CHILD" ] && kill -0 "$R15CHILD" 2>/dev/null; then R15ALIVE=1; fi
# This test's own processes, by their recorded pids, whatever the verdict.
[ -z "$R15ALIVE" ] || kill -KILL "$R15CHILD" 2>/dev/null
[ -z "$R15RUNNING" ] || kill -KILL "$R15RUNNER" 2>/dev/null
wait "$R15RUNNER" 2>/dev/null
if [ -z "$R15CHILD" ]; then
  bad "R15 a retry is stopped with the runner" "the retry never started: $(tr '\n' ' ' < "$TMP/r15.out" | cut -c1-300)"
elif [ -n "$R15RUNNING" ] || [ -n "$R15ALIVE" ]; then
  bad "R15 a retry is stopped with the runner" "5 s after TERM: runner running=${R15RUNNING:-no}, retry child $R15CHILD alive=${R15ALIVE:-no}"
else
  ok "R15 TERM during a retry ends the runner and the retry's child within 5 s"
fi

# R44 — AN OPTION WITH NO VALUE ENDS THE RUN (hunt part 2, R44). `--jobs` as the last word made
# `shift 2` fail without shifting, so the argument loop never ended. The runner is started in
# the background and given 10 s.
R44BOX="$TMP/r44box"; mkdir -p "$R44BOX"
install_harness "$R44BOX"
printf '%s\n' 'echo "=== r44: all 1 passed ==="' > "$R44BOX/one.test.sh"
env -i RICHOS_HOST_SHIELD_TOOL="$SHIELD_TOOL" PATH="$FIXTURE_PATH" HOME="$FIXTURE_HOME" LC_ALL=C RUN_TESTS_STATE="$TMP/fixture-state" \
    bash "$R44BOX/run-tests.sh" --jobs > "$TMP/r44.out" 2>&1 &
R44PID=$!
n=0; while kill -0 "$R44PID" 2>/dev/null && [ "$n" -lt 100 ]; do sleep 0.1; n=$((n + 1)); done
if kill -0 "$R44PID" 2>/dev/null; then
  kill -KILL "$R44PID" 2>/dev/null   # this test's own child, by its recorded pid
  bad "R44 an option with no value is refused" "the runner was still looping after 10 s"
else
  wait "$R44PID" 2>/dev/null; R44RC=$?
  if [ "$R44RC" -eq 2 ] && grep -q -- '--jobs needs a value' "$TMP/r44.out"; then
    ok "R44 an option with no value exits 2 and names the option"
  else
    bad "R44 an option with no value is refused" "exit $R44RC: $(tr '\n' ' ' < "$TMP/r44.out")"
  fi
fi

# R45 — EACH SCREEN SUITE KEEPS ITS OWN PROOF (hunt part 2, R45). Two suites that boot a window,
# one --proof-out path: the second used to overwrite the first.
R45BOX="$TMP/r45box"; mkdir -p "$R45BOX"
install_harness "$R45BOX"
for k in a b; do
  { printf '%s\n' '# run-tests: host-screen'; printf '%s\n' "echo \"=== r45$k: all 1 passed ===\""; } > "$R45BOX/$k.test.sh"
done
env -i RICHOS_HOST_SHIELD_TOOL="$SHIELD_TOOL" PATH="$FIXTURE_PATH" HOME="$FIXTURE_HOME" LC_ALL=C RUN_TESTS_STATE="$TMP/fixture-state" \
    bash "$R45BOX/run-tests.sh" --proof-out "$TMP/r45.proof" > "$TMP/r45.out" 2>&1
R45N="$(find "$TMP" -maxdepth 1 -name 'r45.proof*' | wc -l | tr -d ' ')"
if [ "$R45N" -eq 2 ] && grep -q 'r45a' "$TMP/r45.proof" && grep -q 'r45b' "$TMP/r45.proof.b.test.sh"; then
  ok "R45 two screen suites leave two proof files, none overwritten"
else
  bad "R45 each screen suite keeps its own proof" "found $R45N proof files; $(tr '\n' ' ' < "$TMP/r45.out" | cut -c1-300)"
fi
# ...and a proof path that cannot be written is not announced as written.
env -i RICHOS_HOST_SHIELD_TOOL="$SHIELD_TOOL" PATH="$FIXTURE_PATH" HOME="$FIXTURE_HOME" LC_ALL=C RUN_TESTS_STATE="$TMP/fixture-state" \
    bash "$R45BOX/run-tests.sh" --proof-out "$TMP/no-such-dir/x.proof" > "$TMP/r45b.out" 2>&1 || true
if grep -q 'could not write the gui proof' "$TMP/r45b.out" && ! grep -q 'gui proof written' "$TMP/r45b.out"; then
  ok "R45 a failed proof write is reported, never announced as written"
else
  bad "R45 a failed proof write is reported" "$(grep 'gui proof' "$TMP/r45b.out" | tr '\n' ' ')"
fi

# HS — THE HOST SHIELD (ceo-decisions §65; engine lib/host_shield.py). 2026-10-11: a draft guard
# test's backticked fixture ran the host's halt command as the normal user, and nothing in the
# runner stood in the way. The two throwaway suites come from host-shield-fixture.sh, which
# carries the walls that keep the real command out of reach on a runner without the shield (the
# bare-name suite checks what PATH finds before it calls; the by-path suite exits on line one).
. "$DIR/../../engine/scripts/lib/host-shield-fixture.sh"
HSBOX="$TMP/hsbox"; mkdir -p "$HSBOX"
install_harness "$HSBOX"
hs_fixture "$TMP/hstrip" "$HSBOX"
hs_run() {  # <suite> <out>: the box's runner on one suite, the tripwire first, neither sbin on PATH
  env -i PATH="$(hs_path "$TMP/hstrip")" HOME="$FIXTURE_HOME" LC_ALL=C TMPDIR="$TMP" RUN_TESTS_RETRY=0 \
      RUN_TESTS_STATE="$TMP/fixture-state" RICHOS_HOST_SHIELD_TOOL="$HS_TOOL" \
      bash "$HSBOX/run-tests.sh" --only "$1" > "$2" 2>&1
}
hs_run hs-bare.test.sh "$TMP/hs1.out"; HSRC=$?
if [ "$HSRC" -ne 0 ] && grep -q "^  FAIL  host-shield: refused \`$HS_CMD -h +60\`" "$TMP/hs1.out" \
    && [ ! -s "$TMP/hstrip/trip.log" ]; then
  ok "HS1 a suite calling $HS_CMD -h +60 by bare name fails the run with the stand-in's logged line; nothing reached the tripwire"
else
  bad "HS1 a bare-name host call fails with the stand-in's line" "exit $HSRC: $(tr '\n' ' ' < "$TMP/hs1.out" | cut -c1-600)"
fi
hs_run hs-path.test.sh "$TMP/hs2.out"; HSRC=$?
if [ "$HSRC" -ne 0 ] && grep -q "REFUSED before it ran" "$TMP/hs2.out" && [ ! -e "$TMP/hstrip/path-suite-ran" ]; then
  ok "HS2 a suite whose source names /sbin/$HS_CMD is refused before it runs (its first line never ran)"
else
  bad "HS2 a by-path host call is refused before the suite runs" "exit $HSRC: $(tr '\n' ' ' < "$TMP/hs2.out" | cut -c1-600)"
fi
# HS3 — a copy with no shield beside it and none exported runs NOTHING (review
# rv-20261011T173659Z-e08ee8a2-2c75 #1: it used to warn and run its suites unshielded).
HS3BOX="$TMP/hs3box"; mkdir -p "$HS3BOX"
install_harness "$HS3BOX"
printf '%s\n' "touch \"$TMP/hs3-started\"" 'echo "=== hs3: all 1 passed ==="' > "$HS3BOX/started.test.sh"
harness RICHOS_HOST_SHIELD_TOOL= "PATH=$(hs_path "$TMP/hstrip")" -- "$HS3BOX/run-tests.sh"
if [ "$CODE" -eq 2 ] && says "REFUSED: NO HOST SHIELD" && [ ! -e "$TMP/hs3-started" ]; then
  ok "HS3 a runner copy with no host shield refuses (exit 2) and starts no suite"
else
  bad "HS3 a runner copy with no host shield refuses" "exit $CODE, suite started: $([ -e "$TMP/hs3-started" ] && echo yes || echo no): $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-400)"
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
  echo "=== run-tests.test.sh: $FAIL FAILED, $PASS passed ==="
  exit 1
fi
echo "=== run-tests tests: all $PASS passed ==="
