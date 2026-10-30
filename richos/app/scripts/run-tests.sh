#!/usr/bin/env bash
#
# run-tests.sh — every *.test.sh under app/scripts, discovered from disk.
#
# WHY THE INVENTORY IS NOT TYPED. This repository has now shipped the same defect
# five times: a hand-written list of things to check that drifted from the things
# that existed, under a reassuring fraction. "13/13 guards", "18/18 suites", an
# install.sh whose HOOK_FILES had drifted from its registration, a `run.js` that
# reported "all 4 suites passed" while running none of `steering.js`'s 24 checks.
# `engine/scripts/run-all-tests.sh` and `app/ui/tests/run.js` both ended up here.
# So: add a suite next to this file and it runs. There is no second place to edit.
#
# ZERO SUITES IS EXIT 2, not "all 0 suites passed". An empty inventory reporting
# green is how an unreachable workflow looked healthy for months.
#
# macOS ONLY, and it says so rather than skipping. Every suite under this directory
# is about codesign, keychains and TCC. On Linux they would not fail; they would be
# meaningless, and a green run of meaningless suites is worse than no run.
#
# =======================================================================================
# EXIT 2 FROM A SUITE MEANS "THIS HOST CANNOT ANSWER", AND IT USED TO BE READ AS "FAILED"
# =======================================================================================
#
# `gui-boot.test.sh` has said, in its own words, since it was written:
#
#     That is a fact about THIS MACHINE, not a verdict about the code.
#     ... exit 2
#
# and this harness had no notion of such a host, so it counted that suite as a failure.
# Loro is delivered by the public engine. Missing GUI capabilities can still
# be declared as host gaps; a missing public component is a product failure.
#
# THE DANGEROUS VERSION OF THIS FIX IS THE OBVIOUS ONE — treat exit 2 as a skip and move
# on. That is how a suite stops running and nobody finds out, which is the defect this
# repository has now paid for five times and the exact reason the workflow's author
# refused to patch it in a hurry. So the allowance is not "exit 2 is fine". It is:
#
#   * a gap is only tolerated if the CALLER DECLARED IT, by suite name, WITH A REASON —
#     a bare name declares nothing, exactly as `gui-boot.test.sh`'s own A4 case requires
#     of the gaps it accounts for;
#   * a suite that gaps WITHOUT a declaration is red — so a NEW suite that starts
#     reporting "this host cannot answer" stops the build on the first run;
#   * a declaration whose suite DID answer is red — so an allowance cannot outlive the
#     condition it was written for and quietly hide a later gap;
#   * a gapping suite that FAILED A CASE is not gapping — see below;
#   * the summary line never says "all N suites passed" while a gap exists. It says how
#     many ran, how many could not, and which.
#
# =======================================================================================
# THE HOLE THE FOUR CONDITIONS ABOVE DID NOT COVER, FOUND 2026-09-10
# =======================================================================================
#
# Every one of them asks whether the DECLARATION is honest. None asks whether the SUITE
# still works. Those are different questions, and on 2026-09-10 the difference cost nine
# days: `gui-boot.test.sh` had been dead on every host since `01e9b8d8` (2026-09-08) —
# `update_startup::prepare` began reading an `Info.plist` the boot fixture had never
# written, so the app exited on its first line and B1/B2 went red while B3-B8 went green
# over the same dead boot. At that time the compiler was not publicly delivered.
# The declared host gap was the only thing standing between that death and
# somebody noticing, because the one machine that would have reported it is the operator's
# own, and CI — the machine that runs this every day — was reading a legitimate host gap.
#
#     "THIS HOST CANNOT ANSWER" AND "THIS SUITE CANNOT ANSWER ANYWHERE" ARE DIFFERENT
#     STATES, AND UNDER A DECLARATION THEY LOOKED IDENTICAL.
#
# So exit 2 is now read for what it claims. A gap is a claim about the HOST: it says this
# machine lacks something, not that anything is wrong. A suite that has already PRINTED A
# FAILED CASE has found something wrong, and a claim about the host cannot outrank it. Such
# a suite is counted as FAILED here no matter what any declaration says.
#
# It is enforced on the suite's own output rather than on its exit code, and that is
# deliberate: `gui-boot.test.sh` also refuses to exit 2 over a failure from the inside
# (`host_gap_exit`), but a suite that never adopts that discipline — or a new one written
# next year by somebody who never reads this file — must not be able to hide behind a
# declaration either. All nine suites in this directory print `  FAIL  <case>` from the
# same two-line `bad()` helper, so the marker is a repo-wide fact and not a convention this
# file hopes for.
#
# THE OTHER HALF OF THE REPAIR IS NOT HERE, and it belongs beside this note: a gapping
# suite must have something left to fail. `gui-boot.test.sh` now runs its C1-C5 cases —
# which hold its own fixture to what the product demands of a bundle — BEFORE its host
# gate, so the runner that can never boot anything still decides whether the thing it would
# have booted is still valid. A gap over a suite with no host-independent cases at all is
# still a gap over silence; that is a property of the suite, and each suite owns it.
#
# The failure modes therefore all point at RED, and the tolerated set can only ever get
# smaller without somebody editing a declaration. Drift makes the unaccounted list longer.
#
#   RUN_TESTS_DECLARED_GAPS="gui-boot.test.sh: <why this host cannot answer>"
#
# one declaration per line, `<suite>: <reason>`. Unset — the default, and what an operator
# on a complete machine gets — means no gap is tolerated at all.
#
# =======================================================================================
# THE SUITES RUN CONCURRENTLY, AND THE OUTPUT DOES NOT
# =======================================================================================
#
# MEASURED, 2026-09-19, run `20260919T180454Z-ac11d13e`: this harness was 522.4 s of a
# 950.2 s nightly build — 55.0% of everything, with fourteen independent suites run one
# after another. Three of them are 89% of that: `gui-boot.test.sh` 161.6 s,
# `make-release.test.sh` 157.5 s, `make-engine-asset.test.sh` 128.7 s. Nothing about any
# of them needed the other thirteen to have finished.
#
# INDEPENDENCE IS A PROPERTY THAT WAS CHECKED, NOT ASSUMED. What two suites could collide
# over, and what each one actually does:
#
#   scratch directories — every one of the fourteen makes its own `mktemp -d`, and none
#       writes to a fixed path under `$TMPDIR`. Verified by reading every `mktemp` call
#       site in this directory.
#   the operator's HOME — nothing writes under `$HOME`. `make-release.test.sh` READS
#       `$RICHOS_NAMED_PERSONS_FILE` (and supplies its own fixture list instead);
#       `signing-setup.test.sh` and `package-app.test.sh` READ the keychain inventory
#       through `security find-identity` and each has a case Z proving it left the real
#       keychain untouched. Concurrent readers of a keychain are safe; there are no
#       concurrent writers, because both suites shim `security` for every write.
#   TCP ports — exactly one suite binds one: `make-release.test.sh`, override
#       `RICHOS_RELEASE_TEST_PORT`. No second suite listens on anything, so no two of the
#       fourteen can collide. Two SIMULTANEOUS run-tests.sh runs on one host DID, for as
#       long as that port was a fixed 8975 — and this Mac holds a worktree per engineer
#       plus the nightly's own checkout, so two simultaneous runs is the normal state of
#       the machine rather than an edge case. Since 2026-09-20 the port is derived from
#       the checkout's path (`lib/worktree-resource.sh`): stable for one worktree,
#       distinct between siblings. The proof store below is derived the same way.
#   cargo target directories — `gui-boot.test.sh` and `updater-setup.test.sh` both build
#       under `app/src-tauri/target`; `voice-component.test.sh` builds under `app/target`
#       (a DETACHED nested workspace, app/Cargo.toml:5-8, so those are two different
#       directories). Cargo takes an exclusive file lock on a target directory and the
#       second builder WAITS rather than corrupting anything — so this is a throughput
#       question, never a correctness one. It is the reason the pool is bounded well below
#       fourteen: three suites that each fan out to every core do not get faster by being
#       started at the same instant.
#   the repository — no suite writes into the checkout. `make-engine-asset.test.sh` builds
#       the real archive into its own scratch directory; `nightly.py`'s own guard
#       (`git status --porcelain` after the build) would refuse the release if any of this
#       dirtied the tree.
#
# THE POOL SIZE IS MEASURED, AND THE OBVIOUS ANSWER IS WRONG. The first version of the
# default here computed `min(<logical cores>, <GiB of RAM>/4)` = 6 on this Mac, which is a
# perfectly reasonable derivation and 61 seconds slower than the truth. The curve, same
# tree, same twelve suites, same machine (Apple M4, 10 logical cores, 24 GiB), 2026-09-19:
#
#     pool  1  341 s      <- serial, the "before"
#     pool  2  223 s
#     pool  3  167 s      <- the optimum, and the default
#     pool  4  183 s
#     pool  6  228 s
#     pool 10  236 s
#
# MORE CONCURRENCY IS SLOWER PAST THREE, and the per-suite table says why: two suites
# dominate the run and contend with each other for disk and cores, so their own durations
# INFLATE as the pool grows — `make-release.test.sh` alone goes 131 s -> 167 s -> 228 s ->
# 236 s across that curve, and the wall clock is just whichever of the two finishes last.
# The pool's job is therefore to get the other ten suites out of the heavies' way, not to
# start everything at once. `RUN_TESTS_JOBS` or `--jobs N` overrides it.
#
# OUTPUT IS CAPTURED WHOLE AND PRINTED IN THE DISCOVERY ORDER — the same order, line for
# line, that a serial run produced. Fourteen concurrent writers to one terminal is an
# unreadable log, and an unreadable log is how a failure gets skimmed past. Each suite's
# output goes to its own file and is printed, entire, the moment every suite ahead of it
# has been printed. Nothing is summarized, elided or reordered.
#
# =======================================================================================
# THE TWO HEAVIEST SUITES ARE SKIPPED WHEN THEIR INPUTS HAVE NOT CHANGED
# =======================================================================================
#
# `RUN_TESTS_SKIP_UNCHANGED=1` (never the default, and never set for a `release`) lets
# `make-release.test.sh` and `make-engine-asset.test.sh` — 286 s between them, 30% of a
# whole build — be skipped when nothing they read has changed since the last run that
# proved them green ON THIS HOST.
#
# A SKIP THAT CANNOT NAME WHAT IT COMPARED IS THE DEFECT THIS FILE'S HEADER IS ABOUT. So:
#
#   * the digest is over the INPUTS the suite declares IN ITSELF, on a
#     `# run-tests: inputs <paths>` line — never a table in this file, for the same reason
#     the inventory is not typed. The declaration is a deliberate SUPERSET of what the
#     suite reads: a superset re-runs a suite that did not need re-running, which is the
#     harmless direction;
#     `# run-tests: covers <files>` separately claims exact-file behavior for
#     proof-for.sh, not this digest. Write `covers -` when there is no claim.
#   * it is `git ls-files -s` over those paths (git's own content hashes, exact and
#     instant) AND a `git status --porcelain --untracked-files=all` over the same paths
#     that must be EMPTY. A modified or untracked input is not hashed, it is a change, so
#     an edit nobody committed can never be skipped over;
#   * a suite with no enumerated inputs is never skippable. Adding a heavy suite gets you
#     a suite that runs every time, not a silent skip;
#   * the proof is per-suite, in `$RUN_TESTS_STATE` (default
#     `~/.richos-nightly/suite-proofs/<worktree-id>/`, one directory per checkout — see
#     `lib/worktree-resource.sh` for why nothing here keeps a fixed name), and names the
#     digest, the run id and the commit it was proven on. It is written only after the
#     suite ran and was GREEN;
#   * every skip is printed, named in the summary, and written into `--results-out`, from
#     where `nightly-local.py` puts it in the candidate's `build-info.json`. A candidate
#     can never quietly claim a suite it did not run.
#
# =======================================================================================
# `--no-host-screen`: A BUILD THAT PUTS NOTHING ON THE OPERATOR'S SCREEN
# =======================================================================================
#
# `gui-boot.test.sh` boots the real app, on the real screen, for ~162 s of every build.
# On a Mac somebody is working on, that is not a test, it is an interruption — the CEO's
# words, 2026-09-19: *"So, every engineer will keep opening the app making me unable to do
# anything here or WHAT???"*
#
# `--no-host-screen` (or `RUN_TESTS_NO_HOST_SCREEN=1`) promises that this run opens no
# window on this machine, and it enforces the promise rather than documenting it:
#
#   * A HOST-SCREEN SUITE IS ONE THAT SOURCES `lib/gui-launch.sh` — the one library in
#     this repository that boots the shipped binary — or that declares itself with
#     `# run-tests: host-screen`. Structural, not a guess: `gui-boot.test.sh` and
#     `front-door.test.sh` are what that finds today.
#   * With `RICHOS_GUI_HOST=<vm-name>`, with or without `--no-host-screen`, such a suite is
#     run in a test-VM guest instead, through `scripts/testvm/run-suite.sh`: this Mac builds
#     what the suite launches (the guest has no cargo and no checkout), a fresh guest is
#     booted for that suite's run only, the suite runs there in PREBUILT MODE
#     (lib/gui-launch.sh), and its exit code and results folder come back as the suite's
#     own. If the caller named a VM and no runner is there, the run REFUSES. It never falls
#     back to the host's screen, and it never quietly downgrades a named VM to "not run" — a
#     caller who asked for the proof gets the proof or an error.
#   * THE ONE EXCEPTION IS DECLARED, NOT DISCOVERED. A suite that cannot run in a guest says
#     `# run-tests: host-only: <reason>`, and under RICHOS_GUI_HOST it is recorded NOT RUN
#     with that reason, never promised a guest. `front-door.test.sh` is that suite: it drives
#     a published release on a person's unlocked screen. A bare marker is refused.
#   * Without one, the suite is recorded `NOT RUN (no screen)`, the candidate is still
#     built and still walkable, and `nightly-local.py publish` REFUSES that candidate
#     until `--gui-proof <path>` names a gui-boot result produced later against the same
#     commit. `--proof-out <path>` is how that file is made.
#   * A suite that could put a window on a screen and is neither classified nor declared
#     inert is caught by `run-tests.test.sh` case S3, which scans the REAL inventory. A
#     new suite that opens a window without saying so turns that case red on the day it
#     lands, rather than turning up on the operator's screen.
#
# =======================================================================================
# `--for desktop|ios|android|phone`: WHICH APP'S BUILD THIS RUN IS
# =======================================================================================
#
# The CEO, 2026-09-26: *"the native mobile apps are 2 COMPLETELY INDEPENDENT DIFFERENT
# APPS ... So, WHY THE FUCK ARE THEY PART OF THE SAME FUCKING BUILD???"* The phone apps
# live in `richos/mobile/`, but their suites sit here beside the desktop app's, so the
# desktop nightly ran them all: in run 20260926T004003Z-f09b045a the whole script-suites
# gate was 1412 s, and all 1412 s were `native-ios-app.test.sh` on an iPhone simulator.
#
# `phone-app-suites.tsv`, beside this file, names every suite that tests ONLY a phone app,
# which app, and why. `--for desktop` leaves those suites out; `--for ios`, `--for android`
# and `--for phone` run only them. Without `--for`, every suite runs, exactly as before,
# which is what `--only` and a land's proof selection still get.
#
# A SUITE LEFT OUT IS NEVER SILENT. Every one is printed with the app it belongs to and its
# reason, and written into `--results-out` under `not_in_this_build`, from where it
# travels into the candidate's `build-info.json`. A desktop candidate says which phone
# suites it did not run instead of looking like a build that ran everything.
#
# THE LIST IS RECONCILED BEFORE ANYTHING RUNS, the way RUN_TESTS_DECLARED_GAPS is:
#
#   * a row naming no suite in this directory is REFUSED — a stale row would otherwise
#     sit there ready to drop the next suite that happens to take the name;
#   * a row with no reason is refused, and so is an app other than ios or android;
#   * a row whose suite CLAIMS a file the Mac app ships (its `# run-tests: covers` row names
#     a path under richos/app/src-tauri, richos/app/crates, richos/app/ui or richos/engine)
#     is REFUSED. A suite that proves bytes the desktop app ships is a desktop suite whatever
#     else it tests, and it cannot be moved out of the desktop build by a row in a list.
#     (richos/web/web-app left that list on 2026-10-30, when the phone web page the Mac
#     compiled in from it was retired.)
#   * `--for` with a missing list is refused rather than read as "no phone suites".
#
# A NEW SUITE NOBODY LISTED RUNS IN THE DESKTOP BUILD. That is the direction to fail in:
# it costs time, and it cannot quietly stop a suite from running anywhere.
#
# WHY A LIST BESIDE THE SUITES AND NOT A LINE INSIDE EACH ONE, when this header argues for
# the second: a suite's own file is one of its declared inputs, so a line added to thirteen
# phone suites would have made the land that introduced this run all thirteen, simulators
# included, to prove a comment. The list is reconciled against the inventory in both
# directions above, which is what keeps it from drifting.
#
# `--list` prints the suites this run would start, one per line, and starts none. It takes
# no worker token, so it answers at once even on a busy machine.
#
# =======================================================================================
# A TEAMMATE WORKSPACE RUNS WHAT ITS BRANCH TOUCHES; `--full` IS EVERYTHING (2026-10-01)
# =======================================================================================
#
# The CEO, 2026-10-01, after an engineer ran a whole iPhone UI suite seven times (828 s each)
# to iterate on one test: the narrow run must be the default, not something each agent has to
# remember. So in a teammate workspace (a `cc/` branch in a linked worktree; the decision is
# `engine/scripts/lib/workspace_scope.py`'s, and nowhere else's):
#
#   * no `--only` and no `--for`: the suites are the ones `proof-for.sh` maps the branch's
#     changed paths to (the same map an engineer is told to read), printed in one line with the
#     command for everything. A branch that maps to no suite is REFUSED (exit 2), never a green
#     run of nothing;
#   * `--only <suite>`: that suite, which narrows its own cases where it can (native-ios-ui,
#     native-android-ui);
#   * `--full`: every suite, and RICHOS_TEST_SCOPE=full for each, so none of them narrows.
#
# The main checkout, the nightly (`--for`, and RICHOS_TEST_SCOPE=full in its gate environment)
# and proof-run.py (RICHOS_TEST_SCOPE=full for every check: the merge gate, the land, the engine
# nightly) are never narrowed.
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$DIR/lib/cargo-cache-env.sh"
# Standalone fixture copies have no engine; a complete checkout uses its shared budget.
WORKER_TOOL="$DIR/../../engine/scripts/lib/worker_tokens.py"
# `--list` starts nothing, so it must not wait for a machine worker token to say so.
LISTING=""
for _arg in "$@"; do [ "$_arg" = "--list" ] && LISTING=1; done
if [ -z "$LISTING" ] && [ -f "$WORKER_TOOL" ]; then
  if ! python3 "$WORKER_TOOL" inherited-admission; then
    exec python3 "$WORKER_TOOL" machine -- bash "${BASH_SOURCE[0]}" "$@"
  fi
fi

# THE HOST SHIELD (ceo-decisions §65). Every suite runs through engine/scripts/lib/host_shield.py:
# its source is read first (a host power, sleep, lock or display command called by path refuses
# it before it runs), then it runs with stand-ins for those commands first on PATH, and a suite
# that called one FAILS with the stand-in's logged line. A standalone fixture copy of this file
# has no engine beside it and uses the one its caller exports (RICHOS_HOST_SHIELD_TOOL); with
# neither it REFUSES to run anything: a suite is never started without the stand-ins (review
# rv-20261011T173659Z-e08ee8a2-2c75 #1: a fixture copy run under `env -i` ran a synthetic
# suite's halt call unshielded, and said only a warning).
SHIELD="$DIR/../../engine/scripts/lib/host_shield.py"
[ -f "$SHIELD" ] || SHIELD="${RICHOS_HOST_SHIELD_TOOL:-}"
SHIELDED=()
if [ -n "$SHIELD" ] && [ -f "$SHIELD" ]; then
  SHIELDED=(python3 -S "$SHIELD" run --)
elif [ -z "$LISTING" ]; then
  echo "run-tests.sh: REFUSED: NO HOST SHIELD beside this copy and none exported (RICHOS_HOST_SHIELD_TOOL), so no suite can run behind the power, sleep, lock and display stand-ins (ceo-decisions §65). Nothing was run." >&2
  exit 2
fi

JOBS=""
ONLY=""
PROOF_OUT=""
RESULTS_OUT=""
FOR_BUILD=""
FULL=""
NO_HOST_SCREEN="${RUN_TESTS_NO_HOST_SCREEN:-}"
while [ $# -gt 0 ]; do
  # An option that takes a value but is last on the line: `shift 2` fails and shifts nothing,
  # so the loop would never end (hunt part 2, R44). Refuse it instead.
  case "$1" in
    --jobs|--only|--for|--proof-out|--results-out)
      if [ $# -lt 2 ]; then
        echo "run-tests.sh: $1 needs a value." >&2
        exit 2
      fi ;;
  esac
  case "$1" in
    --jobs)            JOBS="${2:-}"; shift 2 ;;
    --only)            ONLY="$ONLY ${2:-}"; shift 2 ;;
    --for)             FOR_BUILD="${2:-}"; shift 2 ;;
    --list)            shift ;;
    --no-host-screen)  NO_HOST_SCREEN=1; shift ;;
    --full)            FULL=1; shift ;;
    --proof-out)       PROOF_OUT="${2:-}"; shift 2 ;;
    --results-out)     RESULTS_OUT="${2:-}"; shift 2 ;;
    *)
      echo "run-tests.sh: unknown argument '$1'." >&2
      echo "              [--jobs N] [--only <suite>]... [--for desktop|ios|android|phone] [--full]" >&2
      echo "              [--list] [--no-host-screen] [--proof-out <path>] [--results-out <path>]" >&2
      exit 2 ;;
  esac
done
case "$FOR_BUILD" in
  ''|desktop|ios|android|phone) ;;
  *) echo "run-tests.sh: --for takes desktop, ios, android or phone, not '$FOR_BUILD'." >&2; exit 2 ;;
esac
if [ -n "$FOR_BUILD" ] && [ -n "$ONLY" ]; then
  # Two selections at once can disagree, and the only honest answers to "run this suite,
  # but only if it belongs to that build" are a refusal or a surprise. This is the refusal.
  echo "run-tests.sh: --only and --for both choose the suites; give one of them." >&2
  exit 2
fi

if [ "$(uname -s)" != "Darwin" ]; then
  echo "run-tests.sh: these suites exercise codesign, the keychain and TCC — macOS only." >&2
  echo "              (uname -s reports $(uname -s).) Refusing to report a result." >&2
  exit 3
fi

ALL=()
while IFS= read -r t; do [ -n "$t" ] && ALL+=("$t"); done <<EOF
$(find "$DIR" -maxdepth 1 -type f -name '*.test.sh' | LC_ALL=C sort)
EOF

if [ "${#ALL[@]}" -eq 0 ]; then
  echo "run-tests.sh: found NO *.test.sh under $DIR — refusing to report green over an empty inventory." >&2
  exit 2
fi

# `--only` names suites by basename. A name that matches nothing is REFUSED rather than
# silently dropped: "I asked for a suite and got a green run of nothing" is the same
# failure this file's header is about.
SUITES=()
if [ -n "$ONLY" ]; then
  for want in $ONLY; do
    found=""
    for t in "${ALL[@]}"; do
      if [ "$(basename "$t")" = "$want" ]; then SUITES+=("$t"); found=1; break; fi
    done
    if [ -z "$found" ]; then
      echo "run-tests.sh: --only $want names no suite under $DIR." >&2
      exit 2
    fi
  done
else
  SUITES=("${ALL[@]}")
fi

# ---------------------------------------------------------------------------------------
# THE WORKSPACE DEFAULT (CEO, 2026-10-01). See the header's `--full` section.
# ---------------------------------------------------------------------------------------
# `--full` is everything, and every suite it starts is told so (a heavy suite started alone
# from a workspace narrows itself otherwise). With no selection at all in a teammate workspace,
# the suites are the ones proof-for.sh maps the branch's changed paths to; a suite given by
# `--only` narrows its own cases. A standalone fixture copy has no engine and no workspace.
SCOPE_TOOL="$DIR/../../engine/scripts/lib/workspace_scope.py"
SCOPE_LINE=""
if [ -n "$FULL" ]; then
  export RICHOS_TEST_SCOPE=full
elif [ -z "$ONLY" ] && [ -z "$FOR_BUILD" ] && [ -f "$SCOPE_TOOL" ] && [ -f "$DIR/proof-for.sh" ]; then
  if ! _decided="$(python3 "$SCOPE_TOOL" decide "$DIR")"; then
    echo "run-tests.sh: could not decide whether this is a workspace run; nothing ran." >&2
    exit 2
  fi
  scope=""; why=""; branch=""; base=""; root=""
  eval "$_decided"
  if [ "$scope" = narrow ]; then
    # Read into a variable first: in a pipeline a failed read (workspace_scope.py exits
    # 64) was swallowed and the empty list refused the run as "maps to nothing" (P5-79).
    if ! _changed="$(python3 "$SCOPE_TOOL" changed "$root" "$base")"; then
      echo "run-tests.sh: could not read the branch's changed paths; nothing ran. Everything: scripts/run-tests.sh --full" >&2
      exit 2
    fi
    _paths="$(printf '%s\n' "$_changed" | tr '\n' ',' | sed 's/,$//')"
    _where="workspace run on ${branch:-this checkout}, compared with main at ${base:0:12}"
    _picked=""
    if [ -n "$_paths" ]; then
      _errf="$(mktemp "${TMPDIR:-/tmp}/run-tests-scope.XXXXXX")" || { echo "run-tests.sh: cannot create a scratch file." >&2; exit 2; }
      _mapped="$(bash "$DIR/proof-for.sh" --paths "$_paths" --quiet 2>"$_errf")"; _rc=$?
      _err="$(cat "$_errf" 2>/dev/null)"; rm -f "$_errf"
      if [ "$_rc" -ne 0 ] && [ "$_rc" -ne 1 ]; then
        [ -z "$_err" ] || printf '%s\n' "$_err" >&2
        echo "run-tests.sh: REFUSED: $_where: proof-for.sh could not map the branch (exit $_rc)." >&2
        echo "              Everything: scripts/run-tests.sh --full ; chosen suites: --only <suite>" >&2
        exit 2
      fi
      # The suites of its `run-tests.sh --only ...` line and of its `bash scripts/<suite>` lines.
      _picked="$(printf '%s\n' "$_mapped" | awk '
        /scripts\/run-tests\.sh/ { for (i = 1; i < NF; i++) if ($i == "--only") print $(i + 1); next }
        { for (i = 1; i <= NF; i++) if ($i ~ /^scripts\/[^\/]+\.test\.sh$/) { sub(/^scripts\//, "", $i); print $i } }' \
        | LC_ALL=C sort -u)"
    fi
    KEPT=()
    for t in "${SUITES[@]}"; do
      if printf '%s\n' "$_picked" | grep -Fxq "$(basename "$t")"; then KEPT+=("$t"); fi
    done
    if [ "${#KEPT[@]}" -eq 0 ]; then
      echo "run-tests.sh: REFUSED: $_where: proof-for.sh maps the branch to no suite under $DIR," >&2
      echo "              so nothing would run. Everything: scripts/run-tests.sh --full ; chosen: --only <suite>" >&2
      exit 2
    fi
    SUITES=("${KEPT[@]}")
    _names="$(for t in "${SUITES[@]}"; do basename "$t"; done | tr '\n' ' ' | sed 's/ $//')"
    SCOPE_LINE="run-tests.sh: $_where: the ${#SUITES[@]} suite(s) proof-for.sh maps it to: $_names. Everything: scripts/run-tests.sh --full"
  fi
fi

# ---------------------------------------------------------------------------------------
# `--for`: which app's build this is. See the header. Reconciled before anything runs.
# ---------------------------------------------------------------------------------------
# Beside this file and nowhere else: no environment variable can point a build at another list.
PHONE_LIST="$DIR/phone-app-suites.tsv"
OTHER_NAME=(); OTHER_APP=(); OTHER_WHY=()
if [ -n "$FOR_BUILD" ]; then
  if [ ! -f "$PHONE_LIST" ]; then
    echo "run-tests.sh: --for $FOR_BUILD needs $PHONE_LIST, which says which suites test only a" >&2
    echo "              phone app, and it is missing. Read as 'no phone suites' it would put every" >&2
    echo "              phone suite back into the desktop build without a word, so this refuses." >&2
    exit 2
  fi
  L_SUITE=(); L_APP=(); L_WHY=()
  _tab="$(printf '\t')"
  _lineno=0
  while IFS= read -r line || [ -n "$line" ]; do
    _lineno=$((_lineno + 1))
    case "$line" in ''|'#'*) continue ;; esac
    l_suite="${line%%"$_tab"*}"
    rest="${line#*"$_tab"}"
    l_app="${rest%%"$_tab"*}"
    l_why="${rest#*"$_tab"}"
    if [ "$l_suite" = "$line" ] || [ "$l_app" = "$rest" ] || [ -z "$l_why" ]; then
      echo "run-tests.sh: $PHONE_LIST line $_lineno is not '<suite><TAB><apps><TAB><reason>'." >&2
      echo "              A row with no reason declares nothing." >&2
      exit 2
    fi
    case "$l_app" in
      ios|android|ios,android) ;;
      *) echo "run-tests.sh: $PHONE_LIST line $_lineno names the app '$l_app'; it is ios, android or ios,android." >&2
         exit 2 ;;
    esac
    found=""
    for a in "${ALL[@]}"; do [ "$(basename "$a")" = "$l_suite" ] && found="$a" && break; done
    if [ -z "$found" ]; then
      echo "run-tests.sh: $PHONE_LIST line $_lineno names $l_suite, and there is no such suite under $DIR." >&2
      echo "              A stale row is waiting to drop whichever suite takes that name next. Remove it." >&2
      exit 2
    fi
    for d in ${L_SUITE[@]+"${L_SUITE[@]}"}; do
      if [ "$d" = "$l_suite" ]; then
        echo "run-tests.sh: $PHONE_LIST names $l_suite twice." >&2; exit 2
      fi
    done
    # The one structural fact the list cannot override: a suite that claims a file the Mac
    # app ships is a desktop suite. `covers` is the claim (proof-for.sh reads the same row).
    shipped="$(sed -n 's/^# run-tests: covers[[:space:]]*//p' "$found" | head -1 | tr ' ' '\n' \
               | grep -E '^richos/(app/(src-tauri|crates|ui)|engine)/' | head -3 | tr '\n' ' ')"
    if [ -n "$shipped" ]; then
      echo "run-tests.sh: $PHONE_LIST lists $l_suite as a phone-app suite, and its covers row claims" >&2
      echo "              files the Mac app ships: ${shipped% }." >&2
      echo "              A suite that proves the desktop app's bytes belongs to the desktop build." >&2
      exit 2
    fi
    L_SUITE+=("$l_suite"); L_APP+=("$l_app"); L_WHY+=("$l_why")
  done < "$PHONE_LIST"

  # Where a suite belongs: "desktop", or the app(s) its row names.
  home_of() {
    local k=0
    while [ "$k" -lt "${#L_SUITE[@]}" ]; do
      if [ "${L_SUITE[$k]}" = "$1" ]; then printf '%s\t%s\n' "${L_APP[$k]}" "${L_WHY[$k]}"; return; fi
      k=$((k + 1))
    done
    printf 'desktop\t%s\n' "not listed in $(basename "$PHONE_LIST"), so it is the desktop app's"
  }
  in_build() {  # $1 = home ("desktop", "ios", "android", "ios,android")
    case "$FOR_BUILD:$1" in
      desktop:desktop) return 0 ;;
      desktop:*)       return 1 ;;
      *:desktop)       return 1 ;;
      phone:*)         return 0 ;;
    esac
    case ",$1," in *",$FOR_BUILD,"*) return 0 ;; esac
    return 1
  }
  KEPT=()
  for t in "${SUITES[@]}"; do
    h="$(home_of "$(basename "$t")")"
    app="${h%%"$_tab"*}"; why="${h#*"$_tab"}"
    if in_build "$app"; then
      KEPT+=("$t")
    else
      OTHER_NAME+=("$(basename "$t")"); OTHER_APP+=("$app"); OTHER_WHY+=("$why")
    fi
  done
  if [ "${#KEPT[@]}" -eq 0 ]; then
    echo "run-tests.sh: --for $FOR_BUILD selects NO suite under $DIR — refusing to report green over nothing." >&2
    exit 2
  fi
  SUITES=("${KEPT[@]}")
fi

if [ -n "$LISTING" ]; then
  [ -z "$SCOPE_LINE" ] || echo "$SCOPE_LINE" >&2
  for t in "${SUITES[@]}"; do basename "$t"; done
  exit 0
fi

# The declarations, read once, before anything runs — a malformed one should stop the run
# rather than be discovered after fifteen minutes of runner time.
DECLARED_SUITE=()
DECLARED_WHY=()
while IFS= read -r line; do
  case "$line" in ''|'#'*) continue ;; esac
  d_suite="${line%%:*}"
  d_why="${line#*:}"
  # Trim, without an external process.
  d_suite="${d_suite#"${d_suite%%[![:space:]]*}"}"; d_suite="${d_suite%"${d_suite##*[![:space:]]}"}"
  d_why="${d_why#"${d_why%%[![:space:]]*}"}";       d_why="${d_why%"${d_why##*[![:space:]]}"}"
  if [ "$d_suite" = "$line" ] || [ -z "$d_why" ]; then
    echo "run-tests.sh: RUN_TESTS_DECLARED_GAPS names '$d_suite' with no reason after a colon." >&2
    echo "              A bare name declares nothing. Write '<suite>: <why this host cannot answer>'." >&2
    exit 2
  fi
  DECLARED_SUITE+=("$d_suite")
  DECLARED_WHY+=("$d_why")
done <<EOF
${RUN_TESTS_DECLARED_GAPS:-}
EOF

# ---------------------------------------------------------------------------------------
# The pool size. See the header: the memory bound and the core bound, smaller wins.
# ---------------------------------------------------------------------------------------
if [ -z "$JOBS" ]; then
  JOBS="${RUN_TESTS_JOBS:-}"
fi
if [ -z "$JOBS" ]; then
  # MEASURED, not derived. The first version of this line computed
  # `min(cores, GiB/4)` = 6 on this Mac and was WRONG BY 61 SECONDS; see the header for
  # the curve. A third of the cores, floored at two, is the shape the measurement has:
  # enough slots to keep the light suites off the two heavy ones, not enough to make the
  # heavy ones fight. On this Mac (10 logical cores) that is 3, which is the measured
  # optimum; the memory bound is kept as a second ceiling for a host with many cores and
  # little RAM, where three concurrent cargo builds would swap.
  _cores="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 4)"
  _gib=$(( $(sysctl -n hw.memsize 2>/dev/null || echo 8589934592) / 1073741824 ))
  JOBS=$(( _cores / 3 ))
  [ "$JOBS" -ge 2 ] || JOBS=2
  _memcap=$(( _gib / 4 ))
  [ "$_memcap" -ge 1 ] || _memcap=1
  [ "$JOBS" -le "$_memcap" ] || JOBS="$_memcap"
fi
case "$JOBS" in
  ''|*[!0-9]*) echo "run-tests.sh: --jobs/RUN_TESTS_JOBS must be a positive integer, got '$JOBS'." >&2; exit 2 ;;
esac
[ "$JOBS" -ge 1 ] || JOBS=1

# ---------------------------------------------------------------------------------------
# Scratch, and the promise that nothing this run started outlives it (CEO ruling §54).
# ---------------------------------------------------------------------------------------
WORK="$(mktemp -d "${TMPDIR:-/tmp}/run-tests.XXXXXX")" || {
  echo "run-tests.sh: cannot create a scratch directory." >&2; exit 2; }
PIDS=()

# THE PROMISE IS KEPT, NOT ASSUMED. The first version of `cleanup` sent TERM to each suite's
# process tree and deleted the scratch directory on the very next line. It never waited for
# anything to exit and never escalated, so a child that ignored TERM (or was merely slow to
# unwind) kept running against a scratch directory that no longer existed, after the runner
# had reported that it was finished. Found by the 2026-09-29 hunt, part 2, section 15, with an
# owned child that ignores TERM. So cleanup now: collects the whole tree FIRST (a child whose
# parent dies is re-parented and can no longer be found through it), sends TERM, waits up to
# RUN_TESTS_KILL_GRACE seconds (default 10) for all of it to exit, sends KILL to what is left,
# and deletes the scratch only once nothing it started is running. A process that survives even
# KILL is named and the scratch is KEPT: deleting it under a live process is the defect.
KILL_GRACE="${RUN_TESTS_KILL_GRACE:-10}"
case "$KILL_GRACE" in ''|*[!0-9]*) KILL_GRACE=10 ;; esac
CLEANED=""

tree_of() {  # every descendant first, then the process itself; one pid per line
  local pid="$1" child
  for child in $(pgrep -P "$pid" 2>/dev/null); do tree_of "$child"; done
  echo "$pid"
}

is_running() {  # a zombie awaiting its parent's wait is finished, whatever kill -0 says
  local st
  st="$(ps -o stat= -p "$1" 2>/dev/null | tr -d ' ')"
  [ -n "$st" ] && [ "${st#Z}" = "$st" ]
}

any_running() {  # any_running <pid>... -> 0 when at least one is still running
  local p
  for p in "$@"; do is_running "$p" && return 0; done
  return 1
}

cleanup() {
  [ -z "$CLEANED" ] || return 0
  CLEANED=1
  local pid p waited victims=() left=()
  for pid in ${PIDS[@]+"${PIDS[@]}"}; do
    [ -n "$pid" ] || continue
    is_running "$pid" || continue
    for p in $(tree_of "$pid"); do victims+=("$p"); done
  done
  if [ "${#victims[@]}" -gt 0 ]; then
    for p in "${victims[@]}"; do kill -TERM "$p" 2>/dev/null || true; done
    waited=0
    # Tenths of a second, so a well-behaved tree costs a tenth of a second and not a whole one.
    while any_running "${victims[@]}" && [ "$waited" -lt $((KILL_GRACE * 10)) ]; do
      sleep 0.1; waited=$((waited + 1))
    done
    for p in "${victims[@]}"; do
      if is_running "$p"; then kill -KILL "$p" 2>/dev/null || true; fi
    done
    waited=0
    while any_running "${victims[@]}" && [ "$waited" -lt 30 ]; do
      sleep 0.1; waited=$((waited + 1))
    done
    for p in "${victims[@]}"; do
      if is_running "$p"; then left+=("$p"); fi
    done
  fi
  if [ "${#left[@]}" -gt 0 ]; then
    echo "run-tests.sh: process(es) ${left[*]} survived TERM and KILL; keeping $WORK because they" >&2
    echo "              may still be using it. Remove it by hand once they are gone." >&2
    return 0
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT
trap 'cleanup; exit 130' INT TERM

# ---------------------------------------------------------------------------------------
# Classification: which suites put a window on a screen.
# ---------------------------------------------------------------------------------------
is_host_screen() {  # $1 = full path to a suite
  # IT MUST BE SOURCED, NOT MERELY MENTIONED, and the difference is not pedantry: the first
  # version of this line matched the library's NAME anywhere in the file, and on the first
  # full run it classified `run-tests.test.sh` as a suite that boots the app — because that
  # file's own S1 case writes a FIXTURE containing the string `. "$DIR/lib/gui-launch.sh"`.
  # The harness's own self-test was recorded NOT RUN (no screen), under a reason that reads
  # perfectly legitimately, which is exactly the shape of "a suite stopped running and
  # nobody found out" this file counts five instances of. Matching at COMMAND POSITION —
  # `.` or `source` as the first word of a line — separates sourcing the library from
  # writing its name inside a quoted string or a comment.
  grep -qE '^[[:space:]]*(\.|source)[[:space:]]+[^[:space:]]*lib/gui-launch\.sh' "$1" && return 0
  grep -q '^# run-tests: host-screen' "$1" && return 0
  return 1
}

# A host-screen suite that CANNOT run in a guest says so, with the reason, in its own file:
#   # run-tests: host-only: <why it needs this Mac's screen>
# Prints the reason (empty for every other suite). A bare marker declares nothing and is
# refused below, before anything runs.
host_only_reason() {  # $1 = full path to a suite
  sed -n 's/^# run-tests: host-only:[[:space:]]*//p' "$1" | head -1
}

# ---------------------------------------------------------------------------------------
# Skip-when-unchanged. The input sets are a deliberate SUPERSET; see the header.
# ---------------------------------------------------------------------------------------
SKIP_UNCHANGED="${RUN_TESTS_SKIP_UNCHANGED:-}"
RUN_ID="${RICHOS_NIGHTLY_RUN_ID:-manual}"
ROOT="$(git -C "$DIR" rev-parse --show-toplevel 2>/dev/null || true)"
# ONE DIRECTORY PER CHECKOUT, derived from the checkout's path — never one shared store.
# A proof says "this suite was green over these inputs, in run <id>, at commit <sha>"; a
# store shared by every worktree on this Mac hands one checkout's proof to another, and a
# skip is the one outcome that must never rest on something this tree did not prove. The
# derivation is `lib/worktree-resource.sh`, which is the only place any shared name in this
# directory is derived. An explicit RUN_TESTS_STATE still wins outright.
#
# A MISSING LIBRARY REFUSES RATHER THAN FALLING BACK. The fallback would be a fixed path —
# exactly the shared store this replaces — and it would be taken silently, on a machine
# where nobody is looking. Every fixture that copies this harness copies the library with
# it, for the same reason.
if [ ! -f "$DIR/lib/worktree-resource.sh" ]; then
  echo "run-tests.sh: $DIR/lib/worktree-resource.sh is missing, and every shared name this" >&2
  echo "              harness uses is derived there, one per checkout. A fixed fallback" >&2
  echo "              would put two checkouts back on one directory without saying so." >&2
  exit 2
fi
. "$DIR/lib/worktree-resource.sh"
STATE="${RUN_TESTS_STATE:-$(worktree_dir "$(worktree_root "$DIR")" "$HOME/.richos-nightly/suite-proofs")}"
HEAD_SHA="$(git -C "$DIR" rev-parse HEAD 2>/dev/null || echo unknown)"

# ---------------------------------------------------------------------------------------
# A FAILING TEST IS NAMED, AND ITS RESULT FILES ARE KEPT (2026-09-25).
# ---------------------------------------------------------------------------------------
# A proof run's `native-android-app` said "133 tests completed, 1 failed" and named no test;
# the Gradle report that did was in the checkout's build cache, and `native-android-ui`, the
# next suite, replaced it with its own passing report before anyone read it. The failing test
# could not be named. So every suite gets a results folder of its OWN, exported to it as
# RICHOS_TEST_RESULTS_DIR: `bin/randroid test` and the iOS suites copy (or move) their per-test
# results there the moment a test run ends (lib/test_results.py). A suite that fails is named
# here with the tests that failed in it, read from those files, else from its log, else its own
# `  FAIL  ` lines; its folder is kept and its path printed. A suite that passes leaves nothing.
#
# Where: under RICHOS_TEST_RESULTS_ROOT when the caller names one (proof-run.py names a folder in
# the run's own log directory); otherwise in this run's scratch, and a FAILED suite's folder is
# then moved to a per-checkout store (RUN_TESTS_RESULTS_STATE overrides it), which keeps the last
# KEEP_FAILED_RESULTS and deletes older ones (CEO ruling §54: bounded by construction).
KEEPER="$DIR/lib/test_results.py"
if [ ! -f "$KEEPER" ]; then
  echo "run-tests.sh: $KEEPER is missing, and it is what names a failing test. Without it a" >&2
  echo "              failure would be reported as a count with no name, the defect it exists for." >&2
  exit 2
fi
RESULTS_ROOT="${RICHOS_TEST_RESULTS_ROOT:-}"
RESULTS_KEEP=""
KEEP_FAILED_RESULTS=10
if [ -z "$RESULTS_ROOT" ]; then
  RESULTS_ROOT="$WORK/results"
  if [ -n "${RUN_TESTS_RESULTS_STATE:-}" ]; then
    RESULTS_KEEP="$RUN_TESTS_RESULTS_STATE"
  else
    resolved_results="$(python3 "$DIR/../../engine/scripts/lib/machine_settings.py" state test-results --key RICHOS_TEST_RESULTS_ROOT)" || exit 2
    RESULTS_KEEP="$(worktree_dir "$(worktree_root "$DIR")" "$resolved_results")"
  fi
fi
FAILING=()

results_dir() { printf '%s/%s\n' "$RESULTS_ROOT" "${REL[$1]%.test.sh}"; }  # $1 = suite index

# The failing tests of suite $1, one line each: from its result files, else its log, else the
# suite's own `  FAIL  ` lines, else, for a suite that died before any test ran, the last error it
# printed. Its results folder is kept (and moved to the store when this run's root is scratch);
# FAILING[$1] remembers the names for the summary and --results-out.
name_failures() {
  local idx="$1" rel="${REL[$1]}" dir found dest old n count
  dir="$(results_dir "$idx")"
  local paths=()
  [ -d "$dir" ] && paths+=("$dir")
  found="$(python3 "$KEEPER" names --log "$WORK/$idx.out" ${paths[@]+"${paths[@]}"} 2>&1 \
           | sed -n 's/^  FAILED TEST  //p')"
  if [ -z "$found" ]; then
    # Indented or not: extension-native-browser.test.sh prints `FAIL  LEG1 ...` at column 0 (the
    # nightly of 2026-10-03 said it "named nothing" over exactly that line).
    found="$(sed -n 's/^[[:space:]]*FAIL  //p' "$WORK/$idx.out" 2>/dev/null | head -20)"
  fi
  if [ -z "$found" ]; then
    # The suite's own tally, when it prints one but no FAIL line: "7 checks passed, 1 failed".
    found="$(sed -n 's/^[[:space:]]*\([0-9][0-9]* checks passed, [1-9][0-9]* failed\).*/\1/p' "$WORK/$idx.out" 2>/dev/null | tail -1)"
  fi
  if [ -z "$found" ]; then
    found="$(python3 "$KEEPER" last-error "$WORK/$idx.out" 2>/dev/null | sed 's/^/(no test ran to fail) /')"
  fi
  FAILING[idx]="$found"
  if [ -n "$found" ]; then
    echo "    failed in $rel:"
    printf '%s\n' "$found" | sed 's/^/      /'
  else
    echo "    $rel failed and named nothing: no result file, no test log line, no FAIL line and no error line"
  fi
  if [ -d "$dir" ] && [ -n "$(ls -A "$dir" 2>/dev/null)" ]; then
    if [ -n "$RESULTS_KEEP" ]; then
      dest="$RESULTS_KEEP/$(date -u +%Y%m%dT%H%M%SZ)-${rel%.test.sh}-$$"
      if mkdir -p "$RESULTS_KEEP" && mv "$dir" "$dest"; then
        echo "    per-test results kept at $dest"
        # Bounded: the newest KEEP_FAILED_RESULTS folders of this checkout, nothing older. The
        # names start with a UTC time, so the glob's sorted order is oldest first.
        old=("$RESULTS_KEEP"/*)
        n=0
        count="${#old[@]}"
        while [ "$n" -lt $((count - KEEP_FAILED_RESULTS)) ]; do
          rm -rf "${old[$n]}"
          n=$((n + 1))
        done
      else
        echo "    run-tests.sh: could NOT keep the per-test results at $dest; they are lost with $dir"
      fi
    else
      echo "    per-test results kept at $dir"
    fi
  fi
}

suite_inputs() {  # $1 = full path to a suite; prints repo-relative paths, or exits 1
  # READ OUT OF THE SUITE, never from a table here. A second place to edit is the defect this
  # file's header counts five instances of, and the suite is the only thing that knows what
  # it reads. A suite with no `# run-tests: inputs` line declares nothing and is therefore
  # never skippable — which is the safe default for every suite written from now on.
  local decl pinned
  decl="$(sed -n 's/^# run-tests: inputs[[:space:]][[:space:]]*//p' "$1" | head -1)"
  [ -n "$decl" ] || return 1
  printf '%s\n' "$decl"
  # A suite that checks reviewed reader pins (`# run-tests: pins <file>`) is never skipped
  # over a changed reader: every reader the pin file binds joins its digest, read from the
  # file by the same code proof-for.sh selects with. Unreadable -> not skippable.
  if grep -q '^# run-tests: pins' "$1"; then
    pinned="$(python3 "$DIR/lib/proof_declarations.py" --pinned "$ROOT" "$1")" || return 1
    [ -n "$pinned" ] || return 1
    printf '%s\n' "$pinned"
  fi
}

suite_input_digest() {  # $1 = full path to a suite; prints a sha256, or exits 1
  local paths dirty
  # A land omits A8. The run that asks for it (the iPhone app's release check, see
  # native-ios-app.test.sh) must execute it even if the same source passed a land or an
  # earlier run; a source-only receipt cannot prove that A8 ran this time.
  case "$(basename "$1"):${RICHOS_NATIVE_IOS_APP_A8:-0}" in
    native-ios-app.test.sh:1) return 1 ;;
  esac
  paths="$(suite_inputs "$1")" || return 1
  [ -n "$ROOT" ] || return 1
  dirty="$(git -C "$ROOT" status --porcelain --untracked-files=all -- $paths 2>/dev/null)" || return 1
  [ -z "$dirty" ] || return 1
  {
    git -C "$ROOT" ls-files -s -- $paths 2>/dev/null || return 1
    # Not in git, and both suites run against it: the pinned runtime the engine asset
    # embeds and `make-engine-asset.test.sh`'s L19 executes. Included for both, because a
    # superset re-runs a suite that did not need it and that is the harmless direction.
    if [ -n "${RICHOS_RUNTIME_DIR:-}" ] && [ -f "$RICHOS_RUNTIME_DIR/delivery.json" ]; then
      /usr/bin/shasum -a 256 "$RICHOS_RUNTIME_DIR/delivery.json" | awk '{print "runtime " $1}'
    else
      echo "runtime none"
    fi
  } | /usr/bin/shasum -a 256 | awk '{print $1}'
}

proof_path() { printf '%s/%s.proof\n' "$STATE" "$1"; }

proven_run_for() {  # $1 = suite basename, $2 = digest; prints the run id if it matches
  local p; p="$(proof_path "$1")"
  [ -f "$p" ] || return 1
  grep -Fxq "inputs=$2" "$p" || return 1
  sed -n 's/^run=//p' "$p" | head -1
}

record_proof() {  # $1 = suite basename, $2 = digest
  mkdir -p "$STATE" 2>/dev/null || return 0
  {
    echo "suite=$1"
    echo "inputs=$2"
    echo "run=$RUN_ID"
    echo "commit=$HEAD_SHA"
    echo "at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } > "$(proof_path "$1")" 2>/dev/null || true
}

# ---------------------------------------------------------------------------------------
# Plan every suite BEFORE anything starts, so a refusal costs no runner time.
# ---------------------------------------------------------------------------------------
N=${#SUITES[@]}
REL=(); STATE_OF=(); NOTE=(); DIGEST=(); RUNNER=()
i=0
while [ "$i" -lt "$N" ]; do
  t="${SUITES[$i]}"
  rel="${t#"$DIR"/}"
  REL[$i]="$rel"
  STATE_OF[$i]="run"
  NOTE[$i]=""
  DIGEST[$i]=""
  RUNNER[$i]="host"
  if grep -q '^# run-tests: host-only' "$t" && [ -z "$(host_only_reason "$t" | tr -d '[:space:]')" ]; then
    echo "run-tests.sh: $rel declares '# run-tests: host-only' with no reason after the colon." >&2
    echo "              A bare marker declares nothing. Say why it cannot run in a guest." >&2
    exit 2
  fi
  if is_host_screen "$t"; then
    # A NAMED GUEST MEANS THIS MAC's SCREEN IS NOT TO BE USED, with or without
    # --no-host-screen: the caller said where the suite's windows go.
    if [ -n "${RICHOS_GUI_HOST:-}" ]; then
      host_only="$(host_only_reason "$t")"
      if [ -n "$host_only" ]; then
        # Said plainly, never promised: this suite has no guest route at all.
        STATE_OF[i]="notrun"
        NOTE[i]="runs on this Mac's own screen only (no guest route): $host_only"
      elif [ -x "$DIR/testvm/run-suite.sh" ]; then
        RUNNER[$i]="vm:${RICHOS_GUI_HOST}"
      else
        echo "run-tests.sh: RICHOS_GUI_HOST names '${RICHOS_GUI_HOST}', and this checkout has no" >&2
        echo "              $DIR/testvm/run-suite.sh to run $rel in a guest with." >&2
        echo "              A caller who named a guest asked for the proof. This run will not" >&2
        echo "              fall back to the operator's screen and will not pretend the suite" >&2
        echo "              was skipped on purpose. Unset RICHOS_GUI_HOST (with --no-host-screen" >&2
        echo "              to record the suite as NOT RUN)." >&2
        exit 2
      fi
    elif [ -n "$NO_HOST_SCREEN" ]; then
      STATE_OF[$i]="notrun"
      NOTE[$i]="no screen this run may use; --no-host-screen is in effect and RICHOS_GUI_HOST is unset"
    fi
  fi
  if [ "${STATE_OF[$i]}" = "run" ] && [ -n "$SKIP_UNCHANGED" ]; then
    if d="$(suite_input_digest "$t")" && [ -n "$d" ]; then
      DIGEST[$i]="$d"
      if r="$(proven_run_for "$rel" "$d")" && [ -n "$r" ]; then
        STATE_OF[$i]="skipped"
        NOTE[$i]="inputs unchanged since run $r (sha256 $d)"
      fi
    fi
  fi
  i=$((i + 1))
done

[ -z "$SCOPE_LINE" ] || echo "$SCOPE_LINE"
echo "${#ALL[@]} suite(s) discovered under $DIR"
[ -z "$ONLY" ] || [ "$N" -eq "${#ALL[@]}" ] || echo "  --only: running $N of them"
echo "  pool: $JOBS concurrent (override with --jobs N or RUN_TESTS_JOBS)"
if [ -n "$FOR_BUILD" ]; then
  echo "  build: $FOR_BUILD — $N suite(s) belong to it; ${#OTHER_NAME[@]} belong to another app's build and are not run here"
  i=0
  while [ "$i" -lt "${#OTHER_NAME[@]}" ]; do
    echo "  NOT IN THIS BUILD: ${OTHER_NAME[$i]} — ${OTHER_APP[$i]}: ${OTHER_WHY[$i]}"
    i=$((i + 1))
  done
fi
if [ "${#DECLARED_SUITE[@]}" -gt 0 ]; then
  for i in "${!DECLARED_SUITE[@]}"; do
    echo "  declared host gap: ${DECLARED_SUITE[$i]} — ${DECLARED_WHY[$i]}"
  done
fi
i=0
while [ "$i" -lt "$N" ]; do
  case "${STATE_OF[$i]}" in
    notrun)  echo "  NOT RUN (no screen): ${REL[$i]} — ${NOTE[$i]}" ;;
    skipped) echo "  SKIPPED: ${REL[$i]} — ${NOTE[$i]}" ;;
  esac
  case "${RUNNER[$i]}" in
    vm:*) echo "  in a guest: ${REL[$i]} — ${RUNNER[$i]#vm:}" ;;
  esac
  i=$((i + 1))
done
echo ""

# ---------------------------------------------------------------------------------------
# The pool. bash 3.2 is what /usr/bin/env bash resolves to on this Mac (3.2.57) — there is
# no `wait -n`, so the pool is polled rather than event-driven. 5 Hz over a run measured in
# minutes costs nothing and needs no bash 4.
# ---------------------------------------------------------------------------------------
launch() {
  # Two statements, not one `local idx=... t=${SUITES[$idx]}`: bash expands every word of a
  # command before running it, so `$idx` would still be unset inside the same `local`.
  local idx="$1"
  local t="${SUITES[$idx]}"
  local lease=()
  # This suite's own results folder; created by whoever writes into it, never shared.
  local results; results="$(results_dir "$idx")"
  if [ -n "${RICHOS_WORKER_TOKENS:-}" ]; then
    lease=(python3 "${RICHOS_WORKER_TOKENS_TOOL:-$WORKER_TOOL}" run "$RICHOS_WORKER_TOKENS"
           --free "$WORK/suite-free.lock" --)
  fi
  case "${RUNNER[$idx]}" in
    vm:*)
      # The suite writes its results folder in the guest; run-suite.sh brings it back here.
      ( RICHOS_TEST_RESULTS_DIR="$results" ${lease[@]+"${lease[@]}"} "$DIR/testvm/run-suite.sh" "${RUNNER[$idx]#vm:}" "$t" > "$WORK/$idx.out" 2>&1
        c=$?; date +%s > "$WORK/$idx.end"; echo $c > "$WORK/$idx.rc" ) &
      ;;
    *)
      # The finish time is stamped by the child, never by the printer: output is drained in
      # discovery order, so a fast suite can sit finished for minutes waiting for a slow one
      # ahead of it, and timing it at print would charge it that wait.
      ( RICHOS_TEST_RESULTS_DIR="$results" ${lease[@]+"${lease[@]}"} ${SHIELDED[@]+"${SHIELDED[@]}"} bash "$t" > "$WORK/$idx.out" 2>&1
        c=$?; date +%s > "$WORK/$idx.end"; echo $c > "$WORK/$idx.rc" ) &
      ;;
  esac
  PIDS[$idx]=$!
  STARTED[$idx]="$(date +%s)"
}

running_count() {
  local n=0 k=0
  while [ "$k" -lt "$N" ]; do
    if [ -n "${PIDS[$k]:-}" ] && [ ! -f "$WORK/$k.rc" ]; then n=$((n + 1)); fi
    k=$((k + 1))
  done
  echo "$n"
}

FAILED=()
GAPPED=()
NOTRUN=()
SKIPPED=()
TOTAL_CHECKS=0
STARTED=()
ELAPSED=()

# A real failure (not a declared-gap exit 2) is retried once; RUN_TESTS_RETRY=0 turns it off.
retryable() {  # retryable <exit code> <output>
  [ "${RUN_TESTS_RETRY:-1}" != 0 ] || return 1
  [ "$1" -ne 0 ] || return 1
  [ "$1" -ne 2 ] && return 0
  grep -q '^[[:space:]]*FAIL  ' <<<"$2"
}

# Run suite $1 once more, alone: the pool is drained first (nothing new launches while the printer
# is here), the suite's results folder is fresh, and it runs in the foreground the same way
# launch() runs it. Its first-run exit code is $2 and output $3. If it passes, the suite counts as
# passed and says what the first run failed on; if it fails again, $idx.out holds BOTH runs so
# name_failures and the refusal quote both.
RETRY_OUT=""
retry_alone() {
  local idx="$1" first_code="$2" first_out="$3"
  local t="${SUITES[$idx]}" rel="${REL[$idx]}" results code lease=() first_named
  results="$(results_dir "$idx")"
  first_named="$(printf '%s\n' "$first_out" | sed -n 's/^[[:space:]]*FAIL  //p' | head -3 | tr '\n' ';')"
  echo "    run-tests.sh: $rel failed (exit $first_code${first_named:+; $first_named}); retrying it ONCE, alone (verification-retries.md)."
  while [ "$(running_count)" -gt 0 ]; do sleep 0.5; done
  rm -rf "$results"
  if [ -n "${RICHOS_WORKER_TOKENS:-}" ]; then
    lease=(python3 "${RICHOS_WORKER_TOKENS_TOOL:-$WORKER_TOOL}" run "$RICHOS_WORKER_TOKENS"
           --free "$WORK/suite-free.lock" --)
  fi
  # SUPERVISED LIKE THE FIRST RUN (hunt part 2 v3, R15): launched in the background and recorded
  # in PIDS, then waited for. A foreground command defers the TERM trap until it ends and is not in
  # PIDS, so stopping the runner left the retry running past cleanup; `wait` returns at once on a
  # trapped signal, and cleanup stops this pid's whole tree like any other launch.
  case "${RUNNER[$idx]}" in
    vm:*) RICHOS_TEST_RESULTS_DIR="$results" ${lease[@]+"${lease[@]}"} "$DIR/testvm/run-suite.sh" "${RUNNER[$idx]#vm:}" "$t" > "$WORK/$idx.retry.out" 2>&1 & ;;
    *)    RICHOS_TEST_RESULTS_DIR="$results" ${lease[@]+"${lease[@]}"} ${SHIELDED[@]+"${SHIELDED[@]}"} bash "$t" > "$WORK/$idx.retry.out" 2>&1 & ;;
  esac
  PIDS[idx]=$!
  wait "${PIDS[idx]}" && code=0 || code=$?
  echo "$code" > "$WORK/$idx.rc"
  date +%s > "$WORK/$idx.end"
  RETRY_OUT="$(cat "$WORK/$idx.retry.out" 2>/dev/null)"
  printf '%s\n' "$RETRY_OUT"
  if [ "$code" -eq 0 ]; then
    echo "    run-tests.sh: $rel PASSED on the retry, alone. First run: exit $first_code${first_named:+; $first_named}."
    echo "    The suite is counted as passed; the first failure is recorded above and is load-sensitive until shown otherwise."
  else
    echo "    run-tests.sh: $rel FAILED AGAIN on the retry, alone (exit $code): first run exit $first_code, second run exit $code. Both are quoted below."
    { printf '%s\n' "$first_out"; echo "--- retry, alone ---"; printf '%s\n' "$RETRY_OUT"; } > "$WORK/$idx.out"
  fi
}

print_result() {
  local idx="$1"
  local rel="${REL[$idx]}"
  local out code n
  echo "--- $rel"
  case "${STATE_OF[$idx]}" in
    notrun)
      echo "    NOT RUN (no screen): ${NOTE[$idx]}"
      echo "    This candidate carries no gui-boot result. \`nightly-local.py publish\` will"
      echo "    refuse it until --gui-proof names one taken against the same commit."
      NOTRUN+=("$rel")
      ELAPSED[$idx]=0
      echo ""
      return ;;
    skipped)
      echo "    SKIPPED: ${NOTE[$idx]}"
      SKIPPED+=("$rel")
      ELAPSED[$idx]=0
      echo ""
      return ;;
  esac
  out="$(cat "$WORK/$idx.out" 2>/dev/null)"
  code="$(cat "$WORK/$idx.rc" 2>/dev/null || echo 1)"
  ELAPSED[$idx]=$(( $(cat "$WORK/$idx.end" 2>/dev/null || date +%s) - ${STARTED[$idx]:-0} ))
  printf '%s\n' "$out"
  # A suite that failed is retried ONCE, alone, before this run counts it as failed
  # (docs/development/verification-retries.md; the nightly of 2026-10-03 lost an attempt to a
  # suite that failed with the host at 98% CPU and passed 26 of 26 alone minutes later).
  if retryable "$code" "$out"; then
    retry_alone "$idx" "$code" "$out"   # prints its own narration; sets RETRY_OUT, rewrites $idx.rc
    out="$RETRY_OUT"
    code="$(cat "$WORK/$idx.rc" 2>/dev/null || echo 1)"
  fi
  # Each suite ends with "all N passed" or "N FAILED, M passed"; the count is read
  # off the suite's own output rather than asserted here, for the same reason the
  # inventory is not typed.
  n="$(printf '%s' "$out" | sed -n 's/.*all \([0-9]*\) passed ===.*/\1/p' | tail -1)"
  [ -n "$n" ] && TOTAL_CHECKS=$((TOTAL_CHECKS + n))
  if [ -n "$PROOF_OUT" ] && is_host_screen "${SUITES[$idx]}"; then
    # One file per suite (hunt part 2, R45): the first suite takes the path as given, every
    # later one gets its own name beside it, so no suite's proof overwrites another's.
    proof_path="$PROOF_OUT"
    if [ -n "${PROOF_USED:-}" ]; then proof_path="$PROOF_OUT.$(basename "$rel")"; fi
    PROOF_USED=1
    {
      echo "richos-gui-proof 1"
      echo "suite=$rel"
      echo "commit=$HEAD_SHA"
      echo "where=${RUNNER[$idx]}"
      echo "result=$( [ "$code" = 0 ] && echo pass || echo "fail:$code" )"
      echo "at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
      echo "--- output ---"
      printf '%s\n' "$out"
    } > "$proof_path" 2>/dev/null \
      && echo "    gui proof written to $proof_path" \
      || echo "    run-tests.sh: could not write the gui proof to $proof_path"
  fi
  # A HERE-STRING, NEVER A PIPE INTO `grep -q`. This file runs under `pipefail`, and
  # `grep -q` exits the instant it matches — so with a pipe, the writer on the left can be
  # killed by SIGPIPE AFTER the match, the pipeline reports 141, and this test reads FALSE
  # over output that DID contain a failed case. Measured on this Mac, 2026-09-20, on this
  # exact idiom: 400 of 400 iterations reported "no match" for a 400 KB payload whose first
  # line was the needle, and 14 of 200 for `gui-boot.test.sh` through the scanner in
  # `run-tests.test.sh` case S6. Here that false reading would file a suite that FAILED a
  # case as a tolerated host gap — the concealment this harness exists to refuse.
  if [ "$code" -eq 2 ] && grep -q '^  FAIL  ' <<<"$out"; then
    # A GAP IS A CLAIM ABOUT THE HOST, AND THIS SUITE ALREADY FOUND SOMETHING WRONG.
    # See the header. Counted as a failure, which no declaration tolerates.
    echo "    run-tests.sh: $rel exited 2 (\"this host cannot answer\") after failing a case of its own."
    echo "    A gap says this machine lacks something. A failed case says something is WRONG, and that"
    echo "    outranks it — this is counted as a FAILURE, and no declaration can tolerate it."
    printf '%s\n' "$out" | grep '^  FAIL  ' | sed 's/^/    /'
    FAILED+=("$rel")
    name_failures "$idx"
  elif [ "$code" -eq 2 ]; then
    GAPPED+=("$rel")
    rm -rf "$(results_dir "$idx")"
  elif [ "$code" -ne 0 ]; then
    FAILED+=("$rel")
    name_failures "$idx"
  else
    [ -n "${DIGEST[$idx]}" ] && record_proof "$rel" "${DIGEST[$idx]}"
    rm -rf "$(results_dir "$idx")"
  fi
  echo ""
}

RUN_STARTED="$(date +%s)"
next=0
printed=0
while [ "$printed" -lt "$N" ]; do
  while [ "$next" -lt "$N" ]; do
    if [ "${STATE_OF[$next]}" != "run" ]; then
      PIDS[$next]=""
      : > "$WORK/$next.rc"
      next=$((next + 1))
      continue
    fi
    [ "$(running_count)" -lt "$JOBS" ] || break
    launch "$next"
    next=$((next + 1))
  done
  progressed=""
  while [ "$printed" -lt "$next" ] && [ -f "$WORK/$printed.rc" ]; do
    print_result "$printed"
    printed=$((printed + 1))
    progressed=1
  done
  [ "$printed" -lt "$N" ] && [ -z "$progressed" ] && sleep 0.2
done
wait 2>/dev/null || true
RUN_ELAPSED=$(( $(date +%s) - RUN_STARTED ))

# ---------------------------------------------------------------------------------------
# What ran, how long each one took. Serial or not, this is the table that says where the
# time went — and with a pool it is the only way to see it at all.
# ---------------------------------------------------------------------------------------
echo "per-suite wall clock (pool of $JOBS, $RUN_ELAPSED s total):"
i=0
while [ "$i" -lt "$N" ]; do
  case "${STATE_OF[$i]}" in
    run)     printf '  %-32s %5ss\n' "${REL[$i]}" "${ELAPSED[$i]:-0}" ;;
    skipped) printf '  %-32s %5s  (skipped, inputs unchanged)\n' "${REL[$i]}" "-" ;;
    notrun)  printf '  %-32s %5s  (not run, no screen)\n' "${REL[$i]}" "-" ;;
  esac
  i=$((i + 1))
done
echo ""

if [ -n "$RESULTS_OUT" ]; then
  json_escape() { printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'; }
  {
    printf '{\n'
    printf '  "jobs": %s,\n' "$JOBS"
    printf '  "seconds": %s,\n' "$RUN_ELAPSED"
    printf '  "run_id": "%s",\n' "$(json_escape "$RUN_ID")"
    printf '  "commit": "%s",\n' "$(json_escape "$HEAD_SHA")"
    printf '  "build": "%s",\n' "$(json_escape "${FOR_BUILD:-all}")"
    printf '  "not_in_this_build": ['
    i=0
    while [ "$i" -lt "${#OTHER_NAME[@]}" ]; do
      [ "$i" -gt 0 ] && printf ','
      printf '\n    {"name": "%s", "belongs_to": "%s", "reason": "%s"}' \
        "$(json_escape "${OTHER_NAME[$i]}")" "$(json_escape "${OTHER_APP[$i]}")" "$(json_escape "${OTHER_WHY[$i]}")"
      i=$((i + 1))
    done
    [ "${#OTHER_NAME[@]}" -gt 0 ] && printf '\n  '
    printf '],\n'
    printf '  "suites": [\n'
    i=0
    while [ "$i" -lt "$N" ]; do
      state="${STATE_OF[$i]}"
      if [ "$state" = "run" ]; then
        code="$(cat "$WORK/$i.rc" 2>/dev/null || echo 1)"
        if [ "$code" = 0 ]; then state="passed"
        elif [ "$code" = 2 ]; then state="gap"
        else state="failed"; fi
        for f in ${FAILED[@]+"${FAILED[@]}"}; do [ "$f" = "${REL[$i]}" ] && state="failed"; done
      fi
      printf '    {"name": "%s", "state": "%s", "seconds": %s, "where": "%s"' \
        "$(json_escape "${REL[$i]}")" "$state" "${ELAPSED[$i]:-0}" "$(json_escape "${RUNNER[$i]}")"
      [ -n "${DIGEST[$i]}" ] && printf ', "inputs_sha256": "%s"' "${DIGEST[$i]}"
      if [ -n "${FAILING[$i]:-}" ]; then
        printf ', "failing_tests": ['
        sep=""
        while IFS= read -r f; do
          printf '%s"%s"' "$sep" "$(json_escape "$f")"; sep=", "
        done <<<"${FAILING[$i]}"
        printf ']'
      fi
      if [ "${STATE_OF[$i]}" = "skipped" ] || [ "${STATE_OF[$i]}" = "notrun" ]; then
        printf ', "reason": "%s"' "$(json_escape "${NOTE[$i]}")"
      fi
      i=$((i + 1))
      [ "$i" -lt "$N" ] && printf '},\n' || printf '}\n'
    done
    printf '  ]\n}\n'
  } > "$RESULTS_OUT"
fi

# The suites another app's build owns, named once more beside the verdict, so the line a
# reader stops at cannot be mistaken for "every suite in this directory".
if [ "${#OTHER_NAME[@]}" -gt 0 ]; then
  echo "    not in this build ($FOR_BUILD): ${OTHER_NAME[*]}"
fi

# A real failure outranks everything: report it and stop, so a gap can never be the
# headline over a suite that ran and lost.
if [ "${#FAILED[@]}" -gt 0 ]; then
  # Beside each failed suite, what failed in it, so the last lines of a long log name it.
  i=0
  while [ "$i" -lt "$N" ]; do
    if [ -n "${FAILING[$i]:-}" ]; then
      while IFS= read -r f; do echo "  ${REL[$i]}: $f"; done <<<"${FAILING[$i]}"
    fi
    i=$((i + 1))
  done
  echo "=== app/scripts: ${#FAILED[@]} of ${#SUITES[@]} suite(s) FAILED: ${FAILED[*]} ==="
  exit 1
fi

# An undeclared gap. This is the case that must never pass, because it is the shape of a
# suite that silently stopped running.
UNDECLARED=()
for g in "${GAPPED[@]:-}"; do
  [ -n "$g" ] || continue
  known=""
  for d in "${DECLARED_SUITE[@]:-}"; do [ "$d" = "$g" ] && known=1 && break; done
  [ -n "$known" ] || UNDECLARED+=("$g")
done
if [ "${#UNDECLARED[@]}" -gt 0 ]; then
  echo "=== app/scripts: ${#UNDECLARED[@]} suite(s) reported that this host cannot answer, and nobody declared them: ${UNDECLARED[*]} ==="
  echo "    Exit 2 from a suite is 'a fact about THIS MACHINE'. Undeclared, it is indistinguishable"
  echo "    from a suite that quietly stopped running, so it is refused rather than tolerated."
  echo "    Declare it — with a reason — in RUN_TESTS_DECLARED_GAPS, or fix the host."
  exit 2
fi

# A declaration that is no longer needed. Left alone it would sit there ready to swallow a
# future gap in the same suite for a reason that has nothing to do with the one written down.
#
# A suite that was NOT RUN or SKIPPED this run did not answer, so its declaration has not
# outlived anything — it was never given the chance to. Only a suite that RAN and answered
# makes a declaration stale.
# A declaration for a suite `--only` left out of THIS run is excused for the same reason:
# it was never asked to answer. A declaration naming a suite that is not in the inventory
# at all is still STALE — that one names nothing, which is the original case.
STALE=()
for d in "${DECLARED_SUITE[@]:-}"; do
  [ -n "$d" ] || continue
  excused=""
  in_inventory=""
  for a in "${ALL[@]}"; do [ "$(basename "$a")" = "$d" ] && in_inventory=1 && break; done
  if [ -n "$in_inventory" ]; then
    selected=""
    for r in ${REL[@]+"${REL[@]}"}; do [ "$r" = "$d" ] && selected=1 && break; done
    [ -n "$selected" ] || excused=1
    for g in ${GAPPED[@]+"${GAPPED[@]}"} ${NOTRUN[@]+"${NOTRUN[@]}"} ${SKIPPED[@]+"${SKIPPED[@]}"}; do
      [ "$g" = "$d" ] && excused=1 && break
    done
  fi
  [ -n "$excused" ] || STALE+=("$d")
done
if [ "${#STALE[@]}" -gt 0 ]; then
  echo "=== app/scripts: a host gap is declared for ${STALE[*]}, and that suite answered on this host ==="
  echo "    The allowance has outlived its reason. Remove the declaration; an allowance nobody needs"
  echo "    is an allowance waiting to hide something else."
  exit 2
fi

ASIDE=$(( ${#GAPPED[@]} + ${#NOTRUN[@]} + ${#SKIPPED[@]} ))
if [ "$ASIDE" -gt 0 ]; then
  RAN=$(( ${#SUITES[@]} - ASIDE ))
  LINE="=== app/scripts: $RAN of ${#SUITES[@]} suites passed — $TOTAL_CHECKS checks"
  [ "${#GAPPED[@]}" -gt 0 ]  && LINE="$LINE — ${#GAPPED[@]} could not run on this host: ${GAPPED[*]}"
  [ "${#NOTRUN[@]}" -gt 0 ]  && LINE="$LINE — ${#NOTRUN[@]} NOT RUN (no screen): ${NOTRUN[*]}"
  [ "${#SKIPPED[@]}" -gt 0 ] && LINE="$LINE — ${#SKIPPED[@]} skipped, inputs unchanged: ${SKIPPED[*]}"
  echo "$LINE ==="
  for i in "${!DECLARED_SUITE[@]}"; do
    echo "    ${DECLARED_SUITE[$i]}: ${DECLARED_WHY[$i]}"
  done
  i=0
  while [ "$i" -lt "$N" ]; do
    case "${STATE_OF[$i]}" in
      notrun|skipped) echo "    ${REL[$i]}: ${NOTE[$i]}" ;;
    esac
    i=$((i + 1))
  done
  exit 0
fi
echo "=== app/scripts: all ${#SUITES[@]} suites passed — $TOTAL_CHECKS checks ==="
