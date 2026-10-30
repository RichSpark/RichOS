#!/usr/bin/env bash
#
# gui-boot.test.sh — boot the shipped binary the way LaunchServices boots it, and hold
# every line it prints to account.
#
# =========================================================================================
# THE DEFECT THIS EXISTS TO MAKE UNSHIPPABLE
# =========================================================================================
#
# One premise failure — a component reading its configuration out of an environment a Finder
# double-click does not have — was found and fixed THREE TIMES on 2026-09-01:
#
#   970ac5e  the engine directory:  current_dir()/../engine  ->  "/../engine" under cwd=/
#   c179cc1  the loro READ path:    LORO_CORPUS / LORO_ROOT / RICHOS_LORO_DIR, all unset
#   49e2cd4  the loro WRITE path:   CliLoroWriter::from_env(), the same three, still unset
#
# Every one was found by RUNNING the app, never by reading it. Every one shipped working for
# developers and dead for the CEO, because a developer's shell has the variables and his
# launch has `HOME`, `USER`, `PATH=/usr/bin:/bin:/usr/sbin:/sbin` and nothing else.
#
# The job is not to make the fourth instance easier to find. It is to make it impossible to
# ship.
#
# =========================================================================================
# WHY THIS IS NOT A LIST OF THINGS THAT CAN GO WRONG
# =========================================================================================
#
# The obvious check is an inventory: every environment variable a component reads, listed,
# with an assertion that each has a non-environment fallback. `run-tests.sh` twelve lines up
# from here warns about exactly that shape in its own words — this repository has shipped the
# drift defect five times, and it shipped it again on 2026-09-01 when a sandbox file list was
# found five guards behind. A hand-maintained list that stops matching produces a SHORTER
# list, silently, and everything looks greener than it did.
#
# So nothing here is keyed by a variable name, and nothing here enumerates failures. The
# check boots a real process and accounts for its real output, under one rule:
#
#     EVERY LINE OF THE BOOT LOG IS EITHER PROOF THAT SOMETHING RESOLVED, A ROUTINE FACT,
#     OR A DECLARED GAP. ANYTHING ELSE IS A FAILURE.
#
# The asymmetry is the whole point. When a fourth resolver is added and it cannot find its
# configuration on a Finder launch, it prints a sentence saying so — and that sentence
# matches no rule below, so this check goes RED without anyone having remembered to extend
# it. Drift makes the UNACCOUNTED list LONGER, never shorter. There is no way for this to
# fail open.
#
# =========================================================================================
# THE LIMIT, NAMED RATHER THAN LEFT TO BE DISCOVERED
# =========================================================================================
#
# A seam that fails SILENTLY — resolves nothing and prints nothing at all — is invisible
# here, exactly as it is invisible to a human reading the same log. This check raises the
# floor from "somebody has to notice" to "the machine notices anything that speaks"; it does
# not conjure a signal that was never emitted. `S1` below is the half of that gap that CAN
# be closed mechanically: every operator line in the shell carries the `[richos]` prefix, so
# scoping the accounting to that prefix loses nothing.
#
# =========================================================================================
# CASES
# =========================================================================================
#
#   S1  every eprintln! in src-tauri outside its tests carries the [richos] prefix
#   A1  a line no rule accounts for FAILS, and the line is printed
#   A2  a RESOLVED rule with no matching line FAILS — silence is not success
#   A3  a declared gap passes AND its reason is printed where a reviewer sees it
#   A4  a gap declared with no reason is REFUSED — a bare marker declares nothing
#   A5  an empty log FAILS, never "all 0 lines accounted for"
#   A6  every line declared UNACCOUNTABLE is still unaccounted  <- the anti-widening case
#   C1  the harness builds a real .app, with an Info.plist in it
#   C2  ...carrying every Info.plist key the PRODUCT's own updater reads   <- extracted, not typed
#   C3  ...declaring the product's own bundle identifier, read the product's own way
#   C4  ...as a file that satisfies the shape `bundle_plist` demands, and that parses
#   C5  ...and, on the machine, declaring the version the binary says it is
#   C6  the machine's claude stand-in starts by a plain execve under launchd's PATH
#   D1  an active-display count of 0 is REFUSED by the precondition, never passed
#   D2  ...a count of one or more is accepted
#   D3  ...a count that could not be measured says exactly that, and is read as neither
#   D4  and THIS host is asked, before the boot: no awake display -> a named refusal, exit 2
#   B1  the healthy machine boots to completion under launchd's environment
#   B1a ...as an ACCESSORY launch, decided on facts     <- nothing reaches anybody's screen
#   B2  ...and every line of that boot is accounted for  <- THE POSITIVE HALF
#   B3  engine pointer removed          -> RED           <- and six negative halves,
#   B4  corpus pointer removed          -> RED              each proving that the detector
#   B5  loro tools removed              -> RED              for one seam is alive on this
#   B6  the claude stand-in removed     -> RED              run and not a corpse
#   B6a missing Claude fails first-run setup specifically, even with deferred connection
#   B7  the saved company removed       -> RED
#   B8  the company registry corrupted  -> RED
#   B9  the speech model removed        -> RED (the video tools, a setup essential since 2026-10-07)
#   B9a ...and it is first-run setup that says so, naming the video tools
#   Y1  gui_kill ends an ordinary process, verified by pid and not by signal
#   Y2  ...and escalates to KILL for one that ignores TERM
#   Z   this run left nothing running, and the count is printed
#
# B3-B8 break the MACHINE, not the source, so they run on every invocation in seconds. The
# proof that this check catches the three HISTORICAL defects — the source premises put back
# one at a time — is a one-off and its red output is committed under
# `docs/verification/gui-boot-check-2026-09-01/`.
#
# macOS only, for the reason run-tests.sh gives: launchd's environment is the thing under
# test and it does not exist elsewhere.
#
# IN A TEST-VM GUEST the same cases run unchanged. `run-tests.sh` under RICHOS_GUI_HOST hands
# this file to `testvm/run-suite.sh`, which builds richos-tauri and the gui_boot_machine
# example on the host (the guest has no cargo and no checkout) and runs this suite in the
# guest with RICHOS_GUI_PREBUILT naming them: PREBUILT MODE in lib/gui-launch.sh. The only
# difference is where the two executables were built; every case below still runs.
#
# =========================================================================================
# C1-C5, AND WHY THE SUITE NOW CHECKS ITS OWN HARNESS BEFORE IT CHECKS THE PRODUCT
# =========================================================================================
#
# On 2026-09-10 this suite was found dead. `01e9b8d8` (2026-09-08) put
# `update_startup::prepare` at the top of `main()`, and it reads `Contents/Info.plist` out of
# the bundle around the executable. The harness had never written one — it made a directory
# shaped like a `.app` — so every boot here ended on its first line with
#
#     [richos] application startup: No such file or directory (os error 2)
#
# B1 and B2 went red. That is not the interesting half. B3-B8 — the six NEGATIVE cases whose
# entire job is to prove each detector is alive on this run — went GREEN, because a boot that
# never happened is indistinguishable from a boot with its engine pointer removed. Six false
# passes, measured on `7272ecf8`: `3 FAILED, 18 passed`.
#
# THE LESSON IS NOT "ADD A PLIST". It is that this suite had no case holding its own fixture
# to the product's requirements, so a product change could retire the whole B half and the
# only visible symptom was two reds nobody could see (below).
#
# So C1-C4 hold the bundle the harness builds to what the product demands of one, using a
# STAND-IN executable in a scratch directory. No loro checkout, no cargo, no boot — which
# means they run on the one host that actually runs this suite regularly: the CI runner,
# where everything from the host gate down is skipped. C5 is the one that needs the real
# binary and runs beside B0.
#
# =========================================================================================
# D1-D4, AND THE PRECONDITION THIS SUITE DEPENDED ON WITHOUT EVER SAYING SO
# =========================================================================================
#
# On 2026-09-17 a nightly release failed here on B2 alone, reporting
#
#     [richos] window: the runtime reported no attached display
#
# and the source was hunted for two hours. The source was innocent. `available_monitors()`
# is `CGGetActiveDisplayList` (tao-0.35.3/src/platform_impl/macos/monitor.rs:146), and macOS
# ACTIVE means connected, AWAKE and available for drawing — a Mac whose screens have gone to
# sleep reports every display ONLINE and NO display ACTIVE. The app was telling the truth.
# The check was asserting something about the machine's power state without knowing it.
#
# The measurement, and the last row is the one that settles it:
#
#   pmset -g log      displays off 01:33:47Z, on 02:16:29Z
#   84aa5e62          02:06:26-02:13:09Z  screens ASLEEP  ->  1 FAILED (B2), 28 passed
#   same source again 02:25:06-02:27:11Z  screens awake   ->  all 29 passed
#
# SO B2 IS NOT WEAKENED BY A LINE. A display that cannot be read stays unaccountable, the
# `refused` declarations below stay exactly as they were, and A6 still holds them there. What
# changes is that the host is ASKED FIRST, and a host with no awake display is REFUSED BY
# NAME (exit 2, `host_gap_exit`, with the one thing the operator must do) instead of being
# handed to a boot that cannot answer. D1-D3 prove the refusal is alive on numbers this run
# did not produce; D4 puts THIS host's measured answer in the run's own output, which is the
# line that would have ended the two-hour hunt in five seconds.
#
# =========================================================================================
# EXIT 2 IS A CLAIM ABOUT THE HOST, AND IT MAY NOT OUTRANK A FAILED CASE
# =========================================================================================
#
# This suite exits 2 for "this host cannot answer", and `run-tests.sh` tolerates that when a
# caller has declared the gap by name with a reason. `packaging-ci.yml` declares exactly that
# for this suite, correctly: the fixture copies 37 files of a loro compiler that lives in a
# private repository and no public runner can hold it.
#
# THAT DECLARATION IS WHAT KEPT THE DEATH ABOVE INVISIBLE FOR NINE DAYS. It is a true
# statement about the runner, and it silently became cover for a suite that had stopped
# working everywhere. "This host cannot answer" and "this suite cannot answer anywhere" are
# different states, and under a declaration they looked identical.
#
# They are separated here by one rule, applied at every gap exit through `host_gap_exit`:
#
#     A GAP IS A CLAIM ABOUT THE HOST. A RUN THAT HAS ALREADY FAILED A CASE HAS FOUND
#     SOMETHING WRONG WITH THE SUITE OR THE CODE, AND EXITS 1 INSTEAD.
#
# `run-tests.sh` enforces the same rule from outside, on every suite in the directory, by
# refusing to read an exit 2 as a gap when the suite's own output carries a `FAIL` line —
# so a suite that never adopts `host_gap_exit` cannot hide behind a declaration either.

# run-tests: inputs richos/app/scripts/gui-boot.test.sh richos/app/scripts/lib/gui-launch.sh richos/app/scripts/package-app.sh richos/app/scripts/make-engine-asset.sh richos/app/src-tauri richos/app/crates richos/app/ui richos/engine
# run-tests: covers richos/app/scripts/lib/gui-launch.sh richos/app/src-tauri/src/main.rs richos/app/src-tauri/src/engine.rs richos/app/src-tauri/src/memory.rs richos/app/crates/richos-core/examples/gui_boot_machine.rs
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$(cd "$DIR/.." && pwd)"
REPO_DIR="$(cd "$APP_DIR/.." && pwd)"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "${2:-}"; FAIL=$((FAIL + 1)); }

# host_gap_exit — the ONLY way out of this file with "this host cannot answer".
#
# See the header. Exit 2 is a claim about the HOST, and it is tolerated by `run-tests.sh`
# under a declaration. A run that has already failed a case is not making a claim about the
# host; it has found something wrong, and it says so with exit 1, which no declaration
# tolerates. The tally is printed on that path too — a run that ends without one is a run
# somebody has to reproduce to read.
host_gap_exit() {
  if [ "$FAIL" -gt 0 ]; then
    echo "" >&2
    echo "gui-boot.test.sh: $FAIL case(s) had ALREADY FAILED before this host ran out of answers." >&2
    echo "                  A gap is a claim about the HOST. This run found something wrong with" >&2
    echo "                  the SUITE or the CODE, and that outranks it — exit 1, not 2, so no" >&2
    echo "                  declared host gap can tolerate it." >&2
    echo ""
    echo "=== gui-boot.test.sh: $FAIL FAILED, $PASS passed ==="
    exit 1
  fi
  exit 2
}

if [ "$(uname -s)" != "Darwin" ]; then
  echo "gui-boot.test.sh: the condition under test is launchd's environment on macOS." >&2
  echo "                  (uname -s reports $(uname -s).) Refusing to report a result." >&2
  exit 3
fi

# =========================================================================================
# THE RULES
# =========================================================================================
#
# Three classes, and the class is the claim being made about the line. There is a fourth
# outcome — UNACCOUNTED, which is red — and it is the DEFAULT, deliberately: a line nobody
# has classified is a line nobody has read.
#
# Choosing a class is a decision about what a sentence MEANS, and it is the whole of the
# work here. `first-run setup: nothing missing.` is a claim about this machine that somebody
# checked, so it is a PROOF and the boot is held to it. `company registry: 1 compan(ies),
# <path> (file)` is a report of what is in a file, so it is a ROUTINE FACT. Reading the
# first one as the second is how a check silently stops checking — and the fourth class
# below exists so that reading is not merely discouraged but caught.

DECL_ERRORS=0

RESOLVED_ID=(); RESOLVED_RE=(); RESOLVED_WHY=()
ROUTINE_RE=();  ROUTINE_WHY=()
GAP_ID=();      GAP_RE=();      GAP_WHY=()
REFUSED_ID=();  REFUSED_LINE=(); REFUSED_WHY=()

# resolved <id> <extended-regex> <what it proves>
#
# A line that PROVES a configuration was found, naming what was found. It must match at
# least once in a healthy boot; its absence is a failure on its own, because the way all
# three historical defects presented was a success line that stopped appearing.
resolved() { RESOLVED_ID+=("$1"); RESOLVED_RE+=("$2"); RESOLVED_WHY+=("$3"); }

# routine <extended-regex> <why it proves nothing>
#
# A line that is fine and carries no claim about configuration.
routine() { ROUTINE_RE+=("$1"); ROUTINE_WHY+=("$2"); }

# gap <id> <extended-regex> <reason>
#
# A KNOWN OPEN GAP: a line that says a configuration is missing, on a machine where it
# genuinely cannot be supplied, because nothing installs the thing yet. Declaring one is a
# claim that the gap is real and unfixable today, so the reason is MANDATORY and is printed
# in the run's output where a reviewer sees it — the discipline `dialect-exempt:` uses. A
# bare marker declares nothing.
gap() {
  if [ -z "${3:-}" ] || [ -z "$(printf '%s' "${3:-}" | tr -d '[:space:]')" ]; then
    printf 'gui-boot.test.sh: gap "%s" was declared with NO REASON — refusing it. A gap is a\n' "$1" >&2
    printf '                  claim that a missing configuration is unfixable today; state why\n' >&2
    printf '                  or fix it.\n' >&2
    DECL_ERRORS=$((DECL_ERRORS + 1))
    return 1
  fi
  GAP_ID+=("$1"); GAP_RE+=("$2"); GAP_WHY+=("$3")
  return 0
}

# refused <id> <the line, as the boot really prints it> <why it must stay unaccounted>
#
# THE FOURTH CLASS, AND THE ONLY ONE THAT IS NOT A RULE. It declares a line the boot can
# print that is DELIBERATELY not accounted for, so that a boot printing it goes red. The
# declaration adds nothing to the accounting — `A6` asserts that each of these is STILL
# unaccounted, and goes red if any rule ever grows wide enough to swallow one.
#
# It exists because of the one repair that would make this check useless: widening a rule
# until B2 passes. A boot line that reports a company registry nobody could read, or a
# saved company that no longer resolves, is a fact about a BROKEN install; accounting for
# it would turn the negative halves B3-B8 into cases that pass by classification rather
# than by working. So each of those sentences is written down HERE, with the reason it may
# never become a rule, and the reason is checked by a case rather than trusted.
refused() { REFUSED_ID+=("$1"); REFUSED_LINE+=("$2"); REFUSED_WHY+=("$3"); }

declare_rules() {
  RESOLVED_ID=(); RESOLVED_RE=(); RESOLVED_WHY=()
  ROUTINE_RE=();  ROUTINE_WHY=()
  GAP_ID=();      GAP_RE=();      GAP_WHY=()
  REFUSED_ID=();  REFUSED_LINE=(); REFUSED_WHY=()

  # -- the configuration and startup policy a launch must report -------------------------
  # Each regex demands the RESOLVED shape and not merely the topic word: `engine directory:`
  # is printed on both paths, and the failing one reads `NOT FOUND - N place(s) tried`
  # (engine.rs::describe), which this does not match and which nothing else matches either.
  resolved 'engine directory' \
    '^\[richos\] engine directory: /.+ \(via .+\)$' \
    'the working directory claude is started in (engine.rs, 970ac5e)'

  # WHETHER THIS BOOT WAS ALLOWED TO TAKE THE SCREEN — held as a PROOF and not as routine
  # noise, because its absence is the exact failure mode this whole file exists to refuse: a
  # decision that stopped being taken, over a run that still reports green. The regex demands
  # one of the four sentences `activation.rs::log_message` can produce, each of which carries
  # a REASON; a line that says `activation:` and then nothing that explains it matches
  # nothing here and goes red.
  #
  # In THIS suite the answer is always `accessory`: the boot is a child of this script, and a
  # harness has to hold the process it boots. That is the point rather than a limitation —
  # every boot below is a boot that no longer reaches the CEO's keyboard.
  resolved 'activation' \
    '^\[richos\] activation: (regular|accessory) — (an installed launch|no Dock icon, no window on screen, no focus taken, because this is not an installed launch:|RICHOS_ACTIVATION=)' \
    'whether this launch may activate, and why (activation.rs, 2026-09-06)'

  # A placement backed by a current display is a startup decision. Require its
  # dimensions, position and reason; a fallback or an unreadable display stays red.
  local rect='[0-9]+x[0-9]+ pt at \(-?[0-9]+,-?[0-9]+\), min [0-9]+x[0-9]+'
  local display='.+ work area [0-9]+x[0-9]+ pt at \(-?[0-9]+,-?[0-9]+\), scale [0-9]+(\.[0-9]+)?'
  resolved 'window placement' \
    "^\[richos\] window: (derived $rect — $display — (the preferred size fits, so it caps the window|the display is smaller than the preferred size, so the display decided)|restored $rect — where it was left, still inside $display)$" \
    'a window derived from or restored inside an attached display (window_geometry.rs)'

  # The connection starts inside the first accepted request so preparation has Working,
  # progress and Stop ownership. This line proves the selected startup policy and command,
  # not an initialize handshake. Executable readiness remains the separate first-run setup
  # proof below, with B6 requiring that detector specifically when Claude is removed.
  resolved 'compute startup policy' \
    '^\[richos\] compute connection: starts with the first cancellable request over .+$' \
    'the selected compute command is deferred to the first cancellable request (main.rs)'

  # THE 2026-09-17 CANDIDATE-WALK FIX. `resolve_claude_bin_checked` refuses, naming every
  # place looked, rather than letting boot silently freeze on a bare `claude` no provider
  # `PATH` can ever resolve. It is a fact about a BROKEN machine, so — like the first-run
  # setup refusals below it — it must stay unaccounted, never widened into a rule that would
  # certify a machine with no executable to connect to.
  refused 'compute connection unresolved' \
    '[richos] compute connection: no `claude` install could be found on this Mac — looked in: /a; /b — install Claude Code, or set RICHOS_CLAUDE_BIN to its path' \
    'The counterpart of `compute startup policy` above: a boot that never found `claude`
       anywhere it looked. Accepting it would certify a machine `EngineLeaseFactory` can
       never actually spawn against.'

  resolved 'loro read half' \
    '^\[richos\] memory: reading from .+ \(via .+\), runtime .+$' \
    'the corpus and the compiler that reads it (loro.rs, c179cc1)'

  resolved 'loro write half' \
    '^\[richos\] memory: saving corrections to .+ via .+$' \
    'the corpus a confirmed correction is written to (correction.rs, 49e2cd4)'

  resolved 'company' \
    '^\[richos\] company: .+ \(via .+\)$' \
    'the entity this launch files work under (main.rs::boot_entity)'

  # THE REGISTRY IS A FILE ON THIS MACHINE SINCE 2026-09-04, and this is the line that says
  # it was trusted. `EntityRegistry::load` (entity.rs:734) reaches this note on exactly one
  # path: the file was read, parsed, version-checked, every row validated and no id
  # duplicated. Every other path pushes a DIFFERENT note and returns the empty registry, so
  # the absence of this sentence is the presence of a refusal — which is why it is a PROOF
  # and not a fact about a file. The line one below it, the summary, IS a fact about a file.
  resolved 'company registry' \
    '^\[richos\] company registry: [0-9]+ compan(y|ies) from /.+$' \
    'the company list this install owns, read and trusted (entity.rs::load, 8dde942/ea91fa2)'

  # THE CENTRAL FOLDER, held as a PROOF rather than as a fact about a setting, because the
  # `(present)` half is `root.is_dir()` — the boot checking the world, not reporting a path it
  # was handed. `richos-central-folder-2026-09-06.md` §1.3 is the reason it may not be routine:
  # this machine already carries four `corpus.*` symlinks pointing at a directory that is not
  # there, and the rule that stops the fifth is that a pointer is checked at boot and a missing
  # target is a failure rather than a fallback. The `NOT THERE` form is refused below.
  resolved 'central folder' \
    '^\[richos\] central folder: /.+ \(present\)$' \
    'the folder the company layer is read from (company.rs, 2026-09-06)'

  # WHETHER THIS LAUNCH WILL ASK HIM ABOUT HIS BUSINESS. The onboarding audit's whole finding
  # is that the trigger for the interview lived in a file nothing created, and that nothing
  # said so — so the boot now states what it will do, and the ONLY form that counts as
  # resolved is the one where his own words actually reached the priming turn. Every other
  # sentence `onboarding::describe` can produce is refused below, one at a time, because each
  # is a distinct claim a widened rule could swallow separately.
  resolved 'onboarding' \
    '^\[richos\] onboarding [a-z0-9-]+: described — the company layer is in the priming turn$' \
    'what he told RichOS about this company, reaching the model (onboarding.rs, 2026-09-06)'

  # A CLAIM ABOUT THE MACHINE, HELD TO. `setup_view::detect` looks for Claude Code and for
  # the engine and this sentence asserts it found both — it is the boot checking the world
  # and reporting a verdict, not reporting a setting. Held as a proof, B3 (engine pointer
  # removed) and B6 (claude removed) each turn it red. B6 checks this specific proof, because
  # boot deliberately does not attempt a compute handshake any more.
  resolved 'first-run setup' \
    '^\[richos\] first-run setup: nothing missing\.$' \
    'Claude Code and the engine are both installed where setup.rs looks (main.rs:1147, 57cd903)'

  # -- lines that are facts about the launch, not about configuration ---------------------
  # `start` is the ORDINAL or `-`, and the dash is not an edge case: `start_ordinal()` is
  # `None` for every launch that is not a counted start, so a crash-restart prints
  # `(start -, 1 window(s))` (main.rs:817). The pattern accepted only digits until now and
  # the dash went unaccounted — invisible in the healthy boot, which is always `fresh`, and
  # visible in every one of B3-B8, where it was read as noise beside a real failure. Both
  # shapes are the same routine fact.
  routine '^\[richos\] launch: [a-z-]+ \(start ([0-9]+|-), [0-9]+ window\(s\)\)$' \
    'which kind of start this is; carries no claim about anything being found'

  # A FACT ABOUT A FILE: how many companies are in it, where it is, and which of the three
  # load outcomes produced it. It asserts nothing was verified — the verification is the
  # proof line above — so it is routine.
  #
  # ONLY `(file)`, AND THAT IS THE DECISION. `(absent)` and `(unreadable)` are the same
  # sentence about an install that has no company list it can use, and on a machine this
  # check calls COMPLETE neither is an ordinary state. They stay unaccounted, declared
  # below, so B8 fails on the fact rather than on a side effect of it.
  routine '^\[richos\] company registry: [0-9]+ compan\(ies\), /.+ \(file\)$' \
    'how many companies are on disk and where; the trusting of the file is the proof above'
  routine '^\[richos\] boot complete' \
    'the terminator, printed by setup as its last act'
  # `engine::source_commit_note` (54e79b7f): the artifact under test is built inside this checkout
  # with RICHOS_SOURCE_SHA set, so the healthy line names a commit. The other shape — "built
  # outside a checkout, so it cannot name its source commit" — stays UNACCOUNTED on purpose: a
  # bundle that cannot identify itself is audit-8 row 2, never routine.
  routine '^\[richos\] this app: built from [0-9a-f]{7,40}$' \
    'which commit this artifact was built from; the cannot-name-it form stays unaccounted'

  # THE BOOT SWEEP (annex §6.1-6.3, landed 7c37fea6 on 2026-09-17): one line per boot from
  # `recovery.rs:187` `log_message`. ONLY the all-zero form is routine — it says the sweep ran
  # and found nothing that was running when the last process ended. A non-zero count of
  # assignments unknown until re-witnessed, an orphan grant closed, a grant that could NOT be
  # closed, or an unreadable register are each a boot after something went wrong, and they
  # stay unaccounted so that boot is SEEN rather than filed as routine.
  routine '^\[richos\] recovery: 0 orphan grant\(s\) closed; 0 grant\(s\) could NOT be closed$' \
    'the boot sweep ran and had nothing to re-witness; every other shape of this line stays unaccounted'
  # THE APP'S OWN CLEANUP AT START (app_cleanup.rs `run`, 2e3338b89): one line per boot. Only a
  # sweep that finished with nothing undecidable and nothing failed is routine, whatever it
  # deleted: the app removing what it left behind is its job. An undecidable or failed item,
  # "not done", and "no verified engine yet" (a boot with no engine, B5) stay unaccounted.
  routine '^\[richos\] cleanup at start: deleted [0-9]+, undecidable 0, failures 0 \(verdict: .*\)$' \
    'the app swept its own leftovers at start and every item was decided and done; every other shape stays unaccounted'
  routine '^\[richos\] memory: this folder has no company partitions;' \
    'an unpartitioned corpus, which is a shape rather than a missing setting'
  routine '^\[richos\] loro corpus: in-repo layout at .+ - this is .+ own' \
    'whose record an in-repo corpus is; a statement about ownership, not resolution'

  # THE WINDOW'S LAUNCH KIND AND SPLASH STATE (main.rs `launch_init_script`, ef9fbbe81): one line
  # per window. Only the three pairings the code can print are accepted: a fresh start with the
  # splash on or off by the switch, and the other two kinds, which never show it.
  routine '^\[richos\] window: launch kind (fresh, splash (on|off by the switch)|(crash-restart|second-window), splash not for this kind)$' \
    'which kind of window this is and whether the splash applies; a fact about the launch'
  # THE SECOND-REVIEW WATCHER (main.rs `review_watch::ensure`, d74d92732): printed only when it
  # started at launch. Its "could not start" form stays UNACCOUNTED on purpose.
  routine '^\[richos\] second review: monitoring started \(process [1-9][0-9]*\)$' \
    'the review watcher is running, and its pid; the could-not-start form stays unaccounted'
  # THE CODEX WATCHER (main.rs `ensure_app_codex_watch`, 5de81da8c / ef7a26057, gap X): printed
  # only when it started at launch, once per boot. It is configured, not discovered: N is the
  # number of channel folders the user's CODEX_CHANNELS setting names, or the two conventional
  # ones when it sets none (`codex_watch.rs` `channel_folders`), counted whether or not the folder
  # exists. So the healthy synthetic machine, which sets nothing, prints `watching 2` -- that is a
  # statement about configuration and a running pid, and carries no claim a folder was found. `ensure`
  # returns Some(pid) only for a non-empty list, so N is never 0. Nightly 78's proof failed B2
  # on exactly this line (run 20261028T111233Z-41017cf2) because the watcher landed with no rule here.
  #
  # ONLY THE RUNNING FORM. Its three siblings stay UNACCOUNTED on purpose: "the watcher could not
  # start" and "not watching yet, the engine is not verified" are a watcher that is not running,
  # and "no Codex channel is set" is printed both for a user who set the setting empty AND for a
  # launch with no `$HOME` (`channel_folders` returns nothing without a user folder), so it cannot
  # tell a choice from the failure the declared no-`$HOME` case below is about.
  routine '^\[richos\] Codex watch: watching [1-9][0-9]* channel\(s\) \(process [1-9][0-9]*\)$' \
    'the Codex watcher is running on the configured channels, and its pid; the could-not-start, engine-not-verified and no-channel forms stay unaccounted'
  # THE APP MANAGEMENT WATCHER (main.rs `ensure_app_management_watch`): printed only when it
  # started at launch, once per boot. Its could-not-start and engine-not-verified forms stay
  # UNACCOUNTED on purpose: each is a watcher that is not running.
  routine '^\[richos\] App Management watch: recording \(process [1-9][0-9]*\)$' \
    'the read-only App Management watcher is running, and its pid; the could-not-start and engine-not-verified forms stay unaccounted'

  # -- the declared gaps ------------------------------------------------------------------
  gap 'RICHOS_SERVICE_BIN' \
    '^\[richos\] no RICHOS_SERVICE_BIN' \
    'CliVocabulary::from_env (staging.rs:187) is the same environment-only premise as the
       three defects above, and it is left open ON PURPOSE: nothing installs a vocabulary
       service, so there is no second candidate to fall back TO. Inventing a path here
       would be a fabricated fix. Confirming a spoken correction reports that it has
       nowhere to write, which is the honest behavior. CLOSE THIS by deciding where the
       vocabulary service ships from, then delete this declaration and the check turns
       red until the boot line goes away.'

  # `CliEvidenceLookup::from_env` (evidence.rs:114, landed db5ac14d 2026-09-17) resolves
  # RICHOS_EVIDENCE_BIN outright, or falls back to richos-evidence.mjs sitting beside
  # RICHOS_SERVICE_BIN — the SAME service entry point the gap immediately above says nothing
  # installs. There is no second, independent installer for the evidence half: it ships
  # inside tools/richos-service, the package the RICHOS_SERVICE_BIN gap already names as not
  # shipping to a real machine today (docs/verification/2026-09-17-evidence-lookup-wired-
  # end-to-end.md §5 exercises it only under a fabricated temp HOME with every variable set
  # by hand). So a healthy real install prints this same line, and the fixture printing it
  # is not a gap IN the harness — it is the harness telling the truth about an install that
  # genuinely has neither pointer. Inventing a fixture-only richos-evidence.mjs would make
  # the harness green on a premise the real product does not meet.
  gap 'RICHOS_EVIDENCE_BIN' \
    '^\[richos\] file lookup: not configured;' \
    'CliEvidenceLookup::from_env (evidence.rs:114) needs RICHOS_EVIDENCE_BIN, or
       RICHOS_SERVICE_BIN with richos-evidence.mjs beside it, and nothing installs either on
       a real machine today — this is the exact same open premise as the RICHOS_SERVICE_BIN
       gap above, not a second one. A turn whose compiled slice has nothing simply says so
       and stops, which is the honest behavior. CLOSE THIS the moment RICHOS_SERVICE_BIN
       closes (they ship from the same package) by deciding where tools/richos-service
       ships from, then delete this declaration and the check turns red until the boot line
       goes away.'

  # -- THE LINES THAT MAY NEVER BECOME RULES ---------------------------------------------
  #
  # The entity-registry pass of 2026-09-04 (8dde942 -> ea91fa2) took the CEO's six companies
  # out of the binary and made the registry a per-user FILE. That gave the boot a new seam,
  # and a seam speaks: fourteen sentences below, plus three from first-run setup, every one
  # of which reports a machine that cannot file work somewhere. They are classified HERE, as
  # unaccountable, rather than left to be discovered by whoever next sees B2 go red and
  # reaches for the shortest way to make it green.
  #
  # `A6` boots none of these — it appends each to the healthy fixture and asserts the
  # accounting still refuses it. So this list is not documentation about a decision; it is
  # the decision, executed on every run.

  # --- the registry file itself: the four ways `EntityRegistry::load` refuses one ---------
  refused 'display unavailable' \
    '[richos] window: the runtime reported no attached display' \
    'There is no current display to validate the window against.'
  refused 'display unreadable' \
    '[richos] window: displays could not be read: unavailable' \
    'The display query failed; a fallback is not a display-backed placement.'
  refused 'window fallback' \
    '[richos] window: fallback 1024x700 pt centered by the platform, min 1024x700 — no display could be read; the smallest size the UI is designed for is the last resort and the platform is left to center it' \
    'A constant carries no evidence that the window fits an attached display. The number
       changed on 2026-09-17 (window_geometry.rs: the blind path opens at the smallest size
       the UI declares, not the largest it would like) and this fixture is the shipped
       sentence, so it changes with it — what it proves here, that a fallback is not a
       display-backed placement, is unchanged.'
  refused 'window missing reason' \
    '[richos] window: derived 1400x880 pt at (260,102), min 1024x700 — unknown' \
    'A placement without the measured display and decision cannot resolve this proof.'

  refused 'registry absent' \
    '[richos] no company registry at /m/Library/Application Support/com.richos.app/entities.json — this install has registered no companies yet' \
    'No registry means no company is registered, which means every send is refused and no
       thread resolves. It is the ORDINARY state of a fresh install and the app asks in the
       window — but this check boots a machine it calls COMPLETE, and a complete machine has
       a company list. Accounting for it would let B8 pass with no registry at all.'

  refused 'registry unreadable (io)' \
    '[richos] company registry at /m/entities.json could not be read: Permission denied (os error 13)' \
    'A file that exists and could not be opened. Distinct from absent on purpose (entity.rs
       RegistrySource) — it calls for a repair, not a question — and neither is routine.'

  refused 'registry unreadable (parse)' \
    '[richos] company registry at /m/entities.json is not valid: key must be a string at line 1 column 3. Nothing was loaded from it — no company is registered until it parses.' \
    'One typo in his own file empties the registry whole (a bad row fails the WHOLE file,
       entity.rs::load, deliberately). This is the sentence B8 is built to produce.'

  refused 'registry version refused' \
    '[richos] company registry at /m/entities.json is version 2, and this build reads version 1 — refusing it rather than reading it as something it is not' \
    'A registry this build will not read is a registry this install does not have.'

  refused 'registry row unusable' \
    '[richos] company registry at /m/entities.json: entry 1 is not usable (id must be lower-case). Nothing was loaded from the file.' \
    'Same outcome as a parse failure and a different cause; both leave the install with no
       companies, so neither may be classified as a fact that passes.'

  refused 'registry duplicate id' \
    '[richos] company registry at /m/entities.json: acme is listed twice. Nothing was loaded from the file.' \
    'Same outcome again. Listed separately because each of these is a distinct sentence a
       future widened rule could swallow one at a time.'

  # --- the summary line, on the two sources that are not `file` --------------------------
  refused 'registry summary absent' \
    '[richos] company registry: 0 compan(ies), /m/entities.json (absent)' \
    'The routine rule above deliberately admits `(file)` only. Nothing is registered here.'

  refused 'registry summary unreadable' \
    '[richos] company registry: 0 compan(ies), /m/entities.json (unreadable)' \
    'A registry that exists and was refused. The count of 0 is not a measurement of his
       companies — it is the absence of any measurement, and it must not read as one.'

  # --- the central folder and the interview: every form except the two that worked --------
  #
  # These are the sentences a machine prints when the CEO is not going to be asked about his
  # business, or has been asked and the answer did not land. Each is a legitimate state of
  # SOME install and none is a legitimate state of a COMPLETE one, which is the standard this
  # whole file holds to. They are listed one at a time rather than as a family for the reason
  # the registry sentences above are: a future widened rule swallows them one at a time.

  refused 'central folder missing' \
    '[richos] central folder: /m/myrichos (NOT THERE — no company has anything on file)' \
    'The folder resolved and is not there, so every company reads as un-described and the
       interview is offered for all of them. That is the correct behavior and it is not a
       complete machine. Accounting for it would let the onboarding proof below be satisfied
       by an install where nothing was ever written.'

  refused 'central folder no home' \
    '[richos] central folder: no home directory — no company layer this launch' \
    'No `$HOME` means the company layer cannot be read at all this launch. Distinct from the
       line above on purpose: one is a folder that is missing and one is a machine that
       cannot say where to look.'

  refused 'onboarding due' \
    '[richos] onboarding northwind: nothing on file and no declination — Rich will offer the interview' \
    'The ordinary and correct state of a FIRST run, and the state this work exists to make
       visible. It is refused here because a machine this check calls complete has been
       described; if this line appears on one, a company file has gone missing.'

  refused 'onboarding declined' \
    '[richos] onboarding northwind: offered and declined at 1757000000000 — Rich will not offer again' \
    'A real answer, durably recorded, and not a fault. Refused for the same reason as the
       line above: a declination can only exist for a company with nothing on file, so it
       cannot occur on a complete machine.'

  refused 'onboarding unusable' \
    '[richos] onboarding northwind: UNUSABLE — /m/myrichos/companies/northwind/company.md is 9001 bytes, over the 8192-byte budget — NOT sent, and not truncated' \
    'His notes exist and did not reach the model. This is the one onboarding state that needs
       somebody to act, and it must never be classified as an ordinary fact.'

  refused 'onboarding not looked' \
    '[richos] onboarding northwind: no central folder configured — nothing has looked, and he will not be asked' \
    'Nothing looked, which is not the same as nothing being there. Kept apart from the two
       above so that a rule written for a missing folder cannot come to cover a missing file.'

  refused 'onboarding no company' \
    '[richos] onboarding: no company resolved, so nothing to ask about yet — the question comes after he says which company this is' \
    'Correct behavior with no company chosen, and it travels with the `no company resolved`
       sentences that are already refused above. A complete machine has resolved one.'

  # --- the one-time no-orphan migration --------------------------------------------------
  #
  # AND THE THING IT DOES TO THIS CHECK, measured rather than assumed: DELETING
  # `entities.json` on a machine that has already booted DOES NOT BREAK IT. The install has
  # threads under `northwind`, `registry_load.source == Absent`, the migration re-registers
  # the ids that already own records and the boot resolves normally — verified on
  # 2026-09-04, output under `docs/verification/gui-boot-entity-registry-2026-09-04/`. That
  # is correct product behavior and it is why B8 CORRUPTS the registry instead of removing
  # it: a negative case a repair path heals is a negative case that proves nothing.
  refused 'registry migration ran' \
    '[richos] company registry: this install already had threads under 1 compan(ies) (acme). They have been written to /m/entities.json so they keep resolving. No folder was guessed for any of them — open the company settings to add one.' \
    'A repair that ran once on an upgrade. It is not the steady state of a complete machine,
       and a boot under test that prints it is a boot somebody should look at.'

  refused 'registry migration failed' \
    '[richos] company registry: could not write /m/entities.json (Read-only file system (os error 30)). Existing threads will not resolve until a company is registered in the window.' \
    'Threads that exist on disk and resolve nowhere. Never routine.'

  # --- resolution: the four ways `resolve_boot_entity` files work nowhere -----------------
  refused 'entity env refused' \
    '[richos] RICHOS_ENTITY="acme" is not a registered entity — refusing it' \
    'An operator named a company deliberately and it does not exist here. Fails closed in
       the product and must fail red here.'

  refused 'entity saved choice stale' \
    '[richos] the saved company "northwind" is not a registered entity any more — ignoring it and asking again rather than filing work under a guess' \
    'The one state in which the CEO believes he has answered this question and has not.
       (The run of spaces is verbatim from main.rs:637 — this file quotes the boot, it does
       not tidy it.)'

  refused 'entity cwd unresolved' \
    '[richos] entity not resolved from /: unknown root /: no registered entity owns this path — refusing to guess' \
    'The last fallback found nothing. Under launchd cwd is ALWAYS `/`, so this sentence is
       the normal shape of a failed resolution on the only condition that matters.'

  refused 'entity no cwd' \
    '[richos] no working directory could be read' \
    'The fallback could not even be attempted.'

  # --- and what the CEO is told when nothing resolved, in both of its shapes --------------
  refused 'no company registered' \
    '[richos] no company is registered on this install yet — RichOS will ask in the window and save the company details to /m/entities.json.' \
    'The visible end of a boot that
       resolved no entity, which the `company` proof already fails on; accounting for it
       would leave that proof as the only thing standing between this check and a launch
       that can file nothing.'


  refused 'no company resolved' \
    '[richos] choose a company in the window. Available companies: acme.' \
    'The other shape of the same condition: companies ARE registered and none was chosen.
       This is what B7 produces.'



  # --- first-run setup, on every path that is not "nothing missing" -----------------------
  refused 'first-run setup missing' \
    '[richos] first-run setup: claude-code is NOT installed — looked in: /a; /b' \
    'The counterpart of the `first-run setup` proof. §19 says RichOS "would not run on
       anyone else'"'"'s" Mac; this line is that condition speaking, and a check that accepted
       it would be certifying a machine the product cannot run on.'

  refused 'first-run setup pinned' \
    '[richos] first-run setup: this build installs engine 2026.09.04.' \
    'Only ever printed beside a missing component. It is good news about a bad state, and
       the bad state is what the accounting is for.'

  refused 'first-run setup unpinned' \
    '[richos] first-run setup: this build carries NO engine pin, so it cannot install one. Build with RICHOS_ENGINE_VERSION / RICHOS_ENGINE_URL / RICHOS_ENGINE_SHA256 set.' \
    'A build that cannot repair the machine it is running on. The worst of the three and the
       one most likely to reach a stranger who downloaded a release.'

  refused 'first-run setup video tools downloading' \
    '[richos] video tools: downloading in the background' \
    'Only ever printed beside missing video tools: the boot started their download (the CEO,
       2026-10-07, "begin downloading in the background at first launch"). Good news about a
       bad state, like the pinned-engine line above.'
}

# =========================================================================================
# THE ACCOUNTING
# =========================================================================================
#
# gui_account <log-file>
#
# Exit 0 when every `[richos]` line is accounted for AND every RESOLVED rule matched.
# Exit 1 otherwise. Prints a full report either way, because a check whose failure output
# does not name the line is a check somebody has to reproduce by hand.
gui_account() {
  local log="$1"
  local problems=0

  if [ ! -s "$log" ]; then
    echo "    NOTHING TO ACCOUNT FOR: $log is empty. A boot that printed nothing did not"
    echo "    boot; this is a failure, not a clean sheet."
    return 1
  fi

  local richos_lines
  richos_lines="$(grep -c '^\[richos\]' "$log" 2>/dev/null || true)"
  if [ "${richos_lines:-0}" -eq 0 ]; then
    echo "    NOTHING TO ACCOUNT FOR: $log has no [richos] lines at all."
    return 1
  fi

  # -- every line is classified, and unknown is a failure ---------------------------------
  local line i matched
  while IFS= read -r line; do
    matched=""
    for i in "${!RESOLVED_RE[@]}"; do
      if printf '%s\n' "$line" | grep -Eq -- "${RESOLVED_RE[$i]}"; then matched="resolved:${RESOLVED_ID[$i]}"; break; fi
    done
    if [ -z "$matched" ]; then
      for i in "${!ROUTINE_RE[@]}"; do
        if printf '%s\n' "$line" | grep -Eq -- "${ROUTINE_RE[$i]}"; then matched="routine"; break; fi
      done
    fi
    if [ -z "$matched" ]; then
      for i in "${!GAP_RE[@]}"; do
        if printf '%s\n' "$line" | grep -Eq -- "${GAP_RE[$i]}"; then matched="gap:${GAP_ID[$i]}"; break; fi
      done
    fi
    if [ -z "$matched" ]; then
      echo "    UNACCOUNTED  $line"
      problems=$((problems + 1))
    fi
  done < <(grep '^\[richos\]' "$log")

  # -- and every RESOLVED rule has to have been satisfied ---------------------------------
  # The three historical defects all presented as an ABSENT success line. A check that only
  # looked for bad sentences would have passed c6cf4ea, where the write half printed nothing
  # at all.
  for i in "${!RESOLVED_RE[@]}"; do
    if ! grep -Eq -- "${RESOLVED_RE[$i]}" "$log"; then
      echo "    NOT RESOLVED  ${RESOLVED_ID[$i]} — nothing in this boot proved it was found."
      echo "                  It should have said: ${RESOLVED_WHY[$i]}"
      problems=$((problems + 1))
    fi
  done

  # -- the gaps, printed WHERE A REVIEWER SEES THEM ---------------------------------------
  for i in "${!GAP_ID[@]}"; do
    if grep -Eq -- "${GAP_RE[$i]}" "$log"; then
      echo "    DECLARED GAP  ${GAP_ID[$i]}"
      # Reflowed, because a declaration a reviewer has to squint at is a declaration a
      # reviewer skips, and skipping it is the whole failure mode a gap declaration exists
      # to prevent.
      printf '%s\n' "${GAP_WHY[$i]}" | tr -s '[:space:]' ' ' | fold -s -w 84 \
        | sed 's/^ *//; s/^/                  /'
    fi
  done

  # -- anything that is not ours is shown, never silently dropped -------------------------
  local foreign
  foreign="$(grep -v '^\[richos\]' "$log" | grep -v '^$' || true)"
  if [ -n "$foreign" ]; then
    echo "    OUT OF SCOPE (not [richos] lines; S1 proves the shell prints none of these):"
    printf '%s\n' "$foreign" | sed 's/^/      /'
  fi

  [ "$problems" -eq 0 ] && return 0
  return 1
}

# =========================================================================================
# S1 — the prefix, so scoping the accounting to it loses nothing
# =========================================================================================

SCAN_AWK="$(mktemp -t gui-boot-scan.XXXXXX)"
cat > "$SCAN_AWK" <<'AWK'
# Stop at the test module: a test's own output never reaches a boot log.
/^#\[cfg\(test\)\]/ { exit }
/eprintln!\(/ { want = 1 }
{
  if (want) {
    i = index($0, "eprintln!(")
    rest = (i > 0) ? substr($0, i + 10) : $0
    q = index(rest, "\"")
    if (q > 0) {
      lit = substr(rest, q + 1)
      if (substr(lit, 1, 8) != "[richos]") printf "%s:%d: %s\n", FILENAME, FNR, substr(lit, 1, 70)
      want = 0
    }
  }
}
AWK

STRAY="$(cd "$APP_DIR/src-tauri/src" && awk -f "$SCAN_AWK" ./*.rs 2>/dev/null)"
if [ -z "$STRAY" ]; then
  ok "S1 every operator line in src-tauri carries the [richos] prefix"
else
  bad "S1 every operator line in src-tauri carries the [richos] prefix" \
      "these would be invisible to the accounting below: $(printf '%s' "$STRAY" | tr '\n' ' ')"
fi
rm -f "$SCAN_AWK"

# =========================================================================================
# A1-A5 — the accounting itself, driven against logs built by hand
# =========================================================================================
#
# These are instant and they are the reason the boot cases below can be trusted: they prove
# the detector reacts, and reacts for the stated reason, rather than being a function that
# returns 0.

# THE SCRATCH ROOT IS CANONICALIZED, and that is a fixture-fidelity fix rather than a
# convenience. On macOS `mktemp -d` hands back `/var/folders/…`, and `/var` is a symlink to
# `/private/var`; a home under it does not equal its own `canonicalize()`. The product
# refuses such a home outright — `richos_user_update::root` (lib.rs:135-142): "home is not a
# canonical directory" — and `prepare` turns that into a dead boot before the first line.
#
# A REAL home is canonical: `/Users/alex` is its own realpath. So the un-canonicalized
# scratch root was a condition no launch this suite claims to reproduce has ever had, and
# `pwd -P` removes it rather than working around the rule. Measured 2026-09-10: without this
# every boot ended on `[richos] application startup: Could not establish application update
# exclusion: home is not a canonical directory`.
TMP="$(cd "$(mktemp -d -t gui-boot-test.XXXXXX)" && pwd -P)"

# EVERY PROCESS THIS RUN STARTS IS ENDED BY THIS RUN, ON EVERY PATH INCLUDING THE FAILING
# ONES. A launched app outlives the harness that launched it, and deleting the temp
# directory does not kill what is running out of it — this suite proved that the hard way on
# 2026-09-01 by leaving 157 orphaned `richos-tauri` processes on the CEO's Dock, six per
# round, because it killed the subshell instead of the app (`lib/gui-launch.sh`'s header has
# the full diagnosis). The trap reaps PROCESSES first and the directory second, in that
# order, and it is `EXIT` rather than end-of-script so a `set -e` abort, a failed case or a
# Ctrl-C cannot skip it.
GUI_LAUNCHED_PIDS="$TMP/launched.pids"
export GUI_LAUNCHED_PIDS
: > "$GUI_LAUNCHED_PIDS"
cleanup() {
  if declare -f gui_reap_all >/dev/null 2>&1; then
    local left; left="$(gui_reap_all)"
    [ "${left:-0}" -gt 0 ] && echo "  gui-boot.test.sh: $left launched process(es) SURVIVED cleanup" >&2
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT INT TERM

# A healthy log, written out here so A1/A2 differ from it by exactly one line.
#
# IT IS A TRANSCRIPT, NOT A SKETCH. Every line below is the shape the binary really printed
# on this machine on 2026-09-04, in the order it printed them, with the paths shortened —
# `docs/verification/gui-boot-entity-registry-2026-09-04/healthy-boot.txt` is the capture.
# A fixture that drifts from the boot is a fixture that certifies A0-A6 against a log the
# product no longer produces, which is the same silent-shortening failure this file's header
# refuses everywhere else.
#
# The `activation:` line arrived on 2026-09-06 and is captured at
# `docs/verification/activation-2026-09-06/accessory-child-boot.log`, first line. It is the
# ACCESSORY form because that is the form every boot in this suite produces — the app is a
# child of this script, and a harness has to hold the process it boots.
#
# IT LOST A CLAUSE ON 2026-09-10 and the loss is the point. It used to read
# `…/Info.plist carries no readable CFBundleIdentifier; a program is holding this process…`
# — TWO unmet conditions. The harness bundle now carries a real Info.plist (it has to; see
# `gui_bundle_plist`), so condition B is met and only the parent-pid condition is left. The
# fixture is retaken from `docs/verification/gui-boot-info-plist-2026-09-10/healthy-boot.log`
# rather than edited to keep the old sentence, and `B1a` asserts the surviving condition
# still holds on every run — because it is now the only one, and one condition is a thing
# you check rather than a thing you assume.
#
# The `central folder:` and `onboarding …:` lines arrived later the same day and are captured
# at `docs/verification/onboarding-honesty-2026-09-06/raw/C2-boot-described-femcboost.log`,
# where the real binary printed them in this order. The `NOT THERE` and `will offer the
# interview` forms both refused below were captured on the same binary in
# `raw/C3-boot-first-run-no-central-folder.log`, so the refused declarations are transcripts
# too rather than sentences somebody imagined the boot might say.
# On 2026-09-07 compute attachment moved into the first cancellable request. Its boot
# diagnostic records that policy; the separate first-run setup line still checks installation.
#
# The `loro Tier E:` line arrived on 2026-09-17 (db5ac14d, memory.rs:333-350), printed inside
# the same `wire_company_memory` call as the two `loro …` lines above it and therefore before
# `main.rs:1631` prints `engine directory:` — this fixture is ordered to match. It is the
# `Ok(None)` sentence, held here as the DECLARED GAP two screens up rather than as a proof: no
# real install ships `tools/richos-service` (and `richos-evidence.mjs` inside it) today, so
# this is the honest line a healthy machine prints, not a fixture standing in for one.
#
# The `engine directory:` line grew its release on 2026-09-17 (spec point 22): the resolved
# path no longer identifies what is about to run, because a rollback finds a NEWER engine
# where the app it rolled back from installed one. A pinned build — every release and every
# nightly — prints `(via <source>, engine <version> as this build pins)`, which is the form
# below; a `cargo run` carries no pin and prints `engine <version>, pinned by nothing in this
# build`. Both satisfy the `engine directory` proof, whose regex demands the resolved shape
# and a closing parenthesis (`engine.rs::EngineResolution::describe`). The FAILING form —
# `NOT FOUND — this build boots engine <v>, and none of the N place(s) tried carries it` —
# and the `passed over <path> (<source>) — <why>` line that names an engine of another
# release are both left UNACCOUNTED on purpose: a boot printing either is a machine holding
# the wrong engine, and that is a red run, not a classification.
healthy_log() {
  cat <<'LOG'
[richos] activation: accessory — no Dock icon, no window on screen, no focus taken, because this is not an installed launch: a program is holding this process (parent pid 98012), and macOS hands a launch to launchd (pid 1). The window is still real and still driveable; call show() on it, or set RICHOS_ACTIVATION=regular for the whole normal treatment.
[richos] launch: fresh (start 1, 1 window(s))
[richos] window: derived 1400x880 pt at (260,102), min 1024x700 — Monitor #30942 work area 1920x1005 pt at (0,25), scale 1 — the preferred size fits, so it caps the window
[richos] company registry: 1 company from /m/Library/Application Support/com.richos.app/entities.json
[richos] company registry: 1 compan(ies), /m/Library/Application Support/com.richos.app/entities.json (file)
[richos] central folder: /m/myrichos (present)
[richos] company: northwind (via the saved choice)
[richos] onboarding northwind: described — the company layer is in the priming turn
[richos] memory: reading from /m/corpus (via the corpus pointer in Application Support), runtime /opt/homebrew/bin/node
[richos] memory: saving corrections to /m/corpus via /m/loro-tools/bin/loro-write.mjs
[richos] file lookup: not configured; questions that saved memory cannot answer will say so.
[richos] engine directory: /m/.claude/richos-engine (via engine install pointer, engine 1.2.0 as this build pins)
[richos] compute connection: starts with the first cancellable request over /m/.local/bin/claude
[richos] first-run setup: nothing missing.
[richos] no RICHOS_SERVICE_BIN — spoken corrections will be recorded and asked, and confirming one will report that there is no vocabulary to write to
[richos] boot complete — every line above is what this launch resolved
LOG
}

declare_rules
healthy_log > "$TMP/healthy.log"
if OUT="$(gui_account "$TMP/healthy.log" 2>&1)"; then
  ok "A0 the hand-written healthy log is accepted (the fixture the next four vary)"
else
  bad "A0 the hand-written healthy log is accepted" "$(printf '%s' "$OUT" | tr '\n' ' ')"
fi

# A1 — the fourth instance, simulated. A new seam that cannot find its configuration prints
# a sentence nobody wrote a rule for.
{ healthy_log; echo "[richos] voice profile: no RICHOS_VOICE_DIR — dictation will not start"; } > "$TMP/a1.log"
OUT="$(gui_account "$TMP/a1.log" 2>&1)"; CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'UNACCOUNTED.*RICHOS_VOICE_DIR'; then
  ok "A1 a line no rule accounts for fails, and the line is printed verbatim"
else
  bad "A1 a line no rule accounts for fails" "exit $CODE; output: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"
fi

# A2 — the shape all three historical defects actually had: not a bad sentence, an ABSENT
# good one. `c6cf4ea` printed no write-half line at all.
#
# The line is removed by the `loro write half` rule's OWN regex, never by a retyped phrase.
# A retyped phrase went stale on 2026-10-15 (9a6ff39eb reworded the boot line to `memory:
# saving corrections to`, the rule and the fixture with it, and A2 kept grepping out `correction
# desk: writing to`): it removed nothing, the log it fed the checker was the healthy one, and
# A2 failed nightly 58's boot proof for a checker that was working. The line count is asserted
# too, so the case cannot again quietly test an unchanged log.
A2_RE=""
for i in "${!RESOLVED_ID[@]}"; do
  [ "${RESOLVED_ID[$i]}" = 'loro write half' ] && A2_RE="${RESOLVED_RE[$i]}"
done
healthy_log | grep -Ev -- "$A2_RE" > "$TMP/a2.log"
A2_REMOVED=$(( $(healthy_log | wc -l) - $(wc -l < "$TMP/a2.log") ))
OUT="$(gui_account "$TMP/a2.log" 2>&1)"; CODE=$?
if [ "$A2_REMOVED" -ne 1 ]; then
  bad "A2 a missing success line fails" "the fixture lost $A2_REMOVED line(s), not exactly the one write-half line, so A2 tested nothing"
elif [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'NOT RESOLVED  loro write half'; then
  ok "A2 a missing success line fails — silence is not success"
else
  bad "A2 a missing success line fails" "exit $CODE; output: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"
fi

# A3 — the declaration is visible, not merely tolerated.
OUT="$(gui_account "$TMP/healthy.log" 2>&1)"; CODE=$?
# Whitespace-normalized on both sides: the reason is REFLOWED for a reader, so asserting on
# a raw line would be asserting on the wrapping rather than on the words.
FLAT="$(printf '%s' "$OUT" | tr -s '[:space:]' ' ')"
if [ "$CODE" -eq 0 ] \
  && printf '%s' "$FLAT" | grep -q 'DECLARED GAP RICHOS_SERVICE_BIN' \
  && printf '%s' "$FLAT" | grep -q 'nothing installs a vocabulary service'; then
  ok "A3 a declared gap passes and its reason is printed where a reviewer sees it"
else
  bad "A3 a declared gap prints its reason" "exit $CODE; output: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"
fi

# A4 — a bare marker declares nothing.
DECL_ERRORS=0
if gap 'BARE' '^\[richos\] whatever' '   ' 2>/dev/null; then
  bad "A4 a gap with no reason is refused" "it was accepted"
elif [ "$DECL_ERRORS" -eq 1 ]; then
  ok "A4 a gap declared with no reason is refused"
else
  bad "A4 a gap with no reason is refused" "refused but did not record the refusal"
fi
declare_rules   # the refused declaration must not linger in the rule set

# A5 — an empty log is a failure.
: > "$TMP/a5.log"
if gui_account "$TMP/a5.log" >/dev/null 2>&1; then
  bad "A5 an empty log fails" "it passed"
else
  ok "A5 an empty log fails, never 'all 0 lines accounted for'"
fi

# A6 — THE ANTI-WIDENING CASE.
#
# The cheapest way to make B2 green is to widen a rule until it swallows whatever is red.
# That repair is invisible in a passing run and it deletes the check, which is exactly what
# this suite was written to stop someone doing to the product. So every sentence declared
# unaccountable by `refused` is appended to the healthy log, one at a time, and the
# accounting has to refuse each one BY NAME. A rule that grows wide enough to cover one of
# them turns this red, on the run that widened it.
UNREFUSED=()
for i in "${!REFUSED_ID[@]}"; do
  { healthy_log; printf '%s\n' "${REFUSED_LINE[$i]}"; } > "$TMP/a6.log"
  OUT="$(gui_account "$TMP/a6.log" 2>&1)"; CODE=$?
  if [ "$CODE" -eq 0 ] || ! printf '%s\n' "$OUT" | grep -Fq "UNACCOUNTED  ${REFUSED_LINE[$i]}"; then
    UNREFUSED+=("${REFUSED_ID[$i]}")
  fi
done
if [ "${#REFUSED_ID[@]}" -eq 0 ]; then
  bad "A6 every line declared unaccountable is still unaccounted" \
      "nothing is declared — this case cannot pass over an empty list"
elif [ "${#UNREFUSED[@]}" -eq 0 ]; then
  ok "A6 all ${#REFUSED_ID[@]} lines declared unaccountable are still unaccounted"
else
  bad "A6 every line declared unaccountable is still unaccounted" \
      "a rule now accounts for: ${UNREFUSED[*]}"
fi

# All successful placement shapes remain accountable, including secondary displays
# with negative coordinates and a saved placement discarded after a display change.
PLACEMENT_FAILURES=0
while IFS= read -r placement; do
  { healthy_log | grep -v '^\[richos\] window:'; printf '%s\n' "$placement"; } > "$TMP/a7.log"
  if ! gui_account "$TMP/a7.log" >/dev/null 2>&1; then
    PLACEMENT_FAILURES=$((PLACEMENT_FAILURES + 1))
  fi
done <<'LOG'
[richos] window: derived 1000x680 pt at (-1900,58), min 1000x680 — Side display work area 1080x760 pt at (-1920,25), scale 1.5 — the display is smaller than the preferred size, so the display decided
[richos] window: restored 1200x800 pt at (20,58), min 1024x700 — where it was left, still inside Built-in display work area 1512x944 pt at (0,25), scale 2
[richos] window: derived 1400x880 pt at (260,102), min 1024x700 — the saved 1400x880 at (2000,80) on Removed display fits no display attached now — discarded; Monitor #30942 work area 1920x1005 pt at (0,25), scale 1 — the preferred size fits, so it caps the window
LOG
if [ "$PLACEMENT_FAILURES" -eq 0 ]; then
  ok "A7 display-sized, restored and re-derived window placements are accounted for"
else
  bad "A7 successful window placements are accounted for" "$PLACEMENT_FAILURES forms were refused"
fi

healthy_log | grep -v '^\[richos\] window:' > "$TMP/a8.log"
OUT="$(gui_account "$TMP/a8.log" 2>&1)"; CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'NOT RESOLVED  window placement'; then
  ok "A8 a missing window placement fails its own proof"
else
  bad "A8 a missing window placement fails its own proof" "exit $CODE: $OUT"
fi

GUI_APP_DIR="$APP_DIR"
GUI_ENGINE_DIR="${RICHOS_GUI_ENGINE_SOURCE:-$REPO_DIR/engine}"
# PREBUILT MODE (lib/gui-launch.sh): in a test-VM guest there is no cargo, and the binary
# this Mac built for this checkout is the one booted. Unset, the suite builds it, as always.
if [ -n "${RICHOS_GUI_PREBUILT:-}" ]; then
  GUI_BINARY="$RICHOS_GUI_PREBUILT/richos-tauri"
else
  . "$DIR/lib/cargo-target.sh"
  GUI_BINARY="$(cargo_target_dir "$APP_DIR/src-tauri")/debug/richos-tauri"
fi
export GUI_APP_DIR GUI_ENGINE_DIR GUI_BINARY
# shellcheck source=lib/gui-launch.sh
. "$DIR/lib/gui-launch.sh"

# =========================================================================================
# C1-C4 — the bundle this harness gives the product, held to what the product demands
# =========================================================================================
#
# These capability checks need only a scratch directory and `sed`. The real
# boot below also requires the public engine and its delivered runtime.
#
# The executable is a two-line shell script. None of these boots anything: the question is
# whether the CONTAINER is one the product will open, and that question has an answer with no
# binary in it at all.

PRODUCT_RS="$APP_DIR/crates/richos-user-update/src/lib.rs"
CAPS="$TMP/capability"
mkdir -p "$CAPS"
printf '#!/bin/sh\nexit 0\n' > "$CAPS/richos-tauri"
chmod +x "$CAPS/richos-tauri"

# The product's own bundle identifier, read out of the product. `const ID` is what
# `bundle_version` compares against (lib.rs:595-601) and it is not retyped here.
PRODUCT_ID="$(sed -n 's/^const ID: &str = "\([^"]*\)";$/\1/p' "$PRODUCT_RS" | head -1)"
CONF_ID="$(sed -n 's/.*"identifier"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
            "$APP_DIR/src-tauri/tauri.conf.json" | head -1)"

CAPS_PLIST="$CAPS/RichOS.app/Contents/Info.plist"
if gui_bundle "$CAPS/RichOS.app" "$CAPS/richos-tauri" "${PRODUCT_ID:-com.richos.app}" 9.9.9 \
   && [ -f "$CAPS_PLIST" ] && [ -x "$CAPS/RichOS.app/Contents/MacOS/richos-tauri" ]; then
  ok "C1 the harness builds a real .app — Contents/MacOS and Contents/Info.plist"
else
  bad "C1 the harness builds a real .app" \
      "gui_bundle produced no Info.plist at $CAPS_PLIST. A bundle with none is a directory \
shaped like a bundle: update_startup::prepare reads that file first and main() returns \
before the boot prints a line."
fi

# C2 — THE KEYS, EXTRACTED FROM THE PRODUCT AND NOT LISTED HERE.
#
# `bundle_version` is the function `prepare` calls (update_startup.rs:65) and every
# `dict.get("…")` in its body is a key the plist must carry. Reading them out of the source
# means a key the product STARTS requiring turns this red, on the run that added it, without
# anyone remembering this file exists. A hand-written list would go quietly short instead —
# the failure the header of this suite refuses everywhere else.
#
# The extraction proving EMPTY is itself a failure: a renamed function must not read as
# "nothing is required".
REQUIRED_KEYS="$(awk '/^pub fn bundle_version/{f=1} f{print} f && /^}/{exit}' "$PRODUCT_RS" \
                 | grep -o '\.get("[A-Za-z]*")' | sed 's/^\.get("//; s/")$//' | LC_ALL=C sort -u)"
MISSING_KEYS=""
if [ -z "$REQUIRED_KEYS" ]; then
  bad "C2 the Info.plist carries every key the product's own updater reads" \
      "no dict.get(\"…\") keys could be read out of bundle_version in $PRODUCT_RS — the \
extraction has drifted from the source, and an empty requirement list is not a green result."
else
  while IFS= read -r k; do
    [ -n "$k" ] || continue
    grep -Fq "<key>$k</key>" "$CAPS_PLIST" 2>/dev/null || MISSING_KEYS="$MISSING_KEYS $k"
  done <<EOF
$REQUIRED_KEYS
EOF
  if [ -z "$MISSING_KEYS" ]; then
    ok "C2 the Info.plist carries every key bundle_version reads:$(printf ' %s' $REQUIRED_KEYS)"
  else
    bad "C2 the Info.plist carries every key the product's own updater reads" \
        "missing from gui_bundle_plist:$MISSING_KEYS"
  fi
fi

# C3 — the identifier, read the way the PRODUCT reads it.
#
# `activation::parse_bundle_identifier` (activation.rs:203-220) splits on the key and then
# takes the first `<string>` after it. That is reproduced exactly below rather than matched
# with a pattern of this file's own invention, because the question is not "is the identifier
# in the file" — it is "does the product's reader find it".
FLAT_PLIST="$(tr -d '\n\t' < "$CAPS_PLIST" 2>/dev/null || true)"
GOT_ID="${FLAT_PLIST#*<key>CFBundleIdentifier</key>}"
GOT_ID="${GOT_ID#*<string>}"
GOT_ID="${GOT_ID%%</string>*}"
if [ -z "$PRODUCT_ID" ]; then
  bad "C3 the Info.plist declares the product's own bundle identifier" \
      "no 'const ID' could be read out of $PRODUCT_RS — the source of truth moved."
elif [ "$PRODUCT_ID" != "$CONF_ID" ]; then
  bad "C3 the Info.plist declares the product's own bundle identifier" \
      "the updater's const ID is '$PRODUCT_ID' and tauri.conf.json says '$CONF_ID'. No plist \
can satisfy both, so this is a defect in the product, not in the harness."
elif [ "$GOT_ID" = "$PRODUCT_ID" ]; then
  ok "C3 the Info.plist declares $PRODUCT_ID, found by the product's own reader"
else
  bad "C3 the Info.plist declares the product's own bundle identifier" \
      "parse_bundle_identifier's algorithm finds '$GOT_ID', and the product requires '$PRODUCT_ID'"
fi

# C4 — the FILE, not its contents. `bundle_plist` (lib.rs:570-586) opens it O_NOFOLLOW and
# then refuses anything that is not a regular file, has more than one link, exceeds 1 MiB,
# or carries a group- or other-write bit. A plist with perfect contents and mode 666 is a
# boot that dies with `invalid Info.plist`, which is a stranger message than the one that
# started all this.
if [ ! -f "$CAPS_PLIST" ]; then
  bad "C4 the Info.plist is a file of the shape bundle_plist demands" "there is no file"
else
  P_MODE="$(stat -f '%Lp' "$CAPS_PLIST")"
  P_LINKS="$(stat -f '%l' "$CAPS_PLIST")"
  P_SIZE="$(stat -f '%z' "$CAPS_PLIST")"
  C4_WHY=""
  [ "$((8#$P_MODE & 8#022))" -eq 0 ] || C4_WHY="mode $P_MODE carries a group- or other-write bit"
  [ "$P_LINKS" -eq 1 ] || C4_WHY="$C4_WHY; $P_LINKS hard links, and bundle_plist demands 1"
  [ "$P_SIZE" -lt 1048576 ] || C4_WHY="$C4_WHY; $P_SIZE bytes, over the 1 MiB ceiling"
  plutil -lint "$CAPS_PLIST" >/dev/null 2>&1 || C4_WHY="$C4_WHY; plutil will not parse it"
  if [ -z "$C4_WHY" ]; then
    ok "C4 the Info.plist is mode $P_MODE, $P_LINKS link, $P_SIZE bytes, and parses"
  else
    bad "C4 the Info.plist is a file of the shape bundle_plist demands" "${C4_WHY#; }"
  fi
fi

# =========================================================================================
# B0-B8 — the real thing
# =========================================================================================

if gui_prebuilt_mode; then
  # The artifacts were built on the host from this checkout (gui_prebuild), so nothing is
  # built here. A missing one is the route's failure, never a gap about this machine.
  if ! PREBUILT_WHY="$(gui_prebuilt_check 2>&1)"; then
    bad "B0 the prebuilt artifacts named by RICHOS_GUI_PREBUILT are present" "$PREBUILT_WHY"
    echo ""
    echo "=== gui-boot.test.sh: $FAIL FAILED, $PASS passed ==="
    exit 1
  fi
  echo "  ... prebuilt mode: booting $GUI_BINARY, built on the host (nothing is built here)"
elif ! command -v cargo >/dev/null 2>&1; then
  if [ -x "$HOME/.cargo/bin/cargo" ]; then
    PATH="$HOME/.cargo/bin:$PATH"; export PATH
  else
    echo "gui-boot.test.sh: no cargo on PATH — the binary under test cannot be built." >&2
    echo "                  Refusing to report a result over an artifact this run did not make." >&2
    host_gap_exit
  fi
fi

# A missing public component is a product failure, never the old private-source
# host gap. The caller may supply an extracted immutable engine candidate.
if ! GUI_SRC="$(gui_compiler_source 2>&1)"; then
  bad "B0 public Loro component is present" "$GUI_SRC"
  exit 1
fi
if [ ! -f "$GUI_ENGINE_DIR/runtime/delivery.json" ] && [ -z "${RICHOS_RUNTIME_DIR:-}" ]; then
  echo "gui-boot.test.sh: provide an extracted engine with runtimes via RICHOS_GUI_ENGINE_SOURCE, or a verified RICHOS_RUNTIME_DIR." >&2
  host_gap_exit
fi
# The video tools are a setup essential (2026-10-07), present only with both pinned speech
# models, so the healthy machine needs real ones (gui-launch.sh `gui_machine`).
if [ -z "${RICHOS_GUI_SPEECH_MODELS:-}" ]; then
  echo "gui-boot.test.sh: provide both pinned speech models via RICHOS_GUI_SPEECH_MODELS, joined by ':' (the nightly hands it ~/Models/Whisper/ggml-small.en.bin and ggml-large-v3-turbo-q5_0.bin, lib/runtime_cache.py)." >&2
  host_gap_exit
fi

gui_prebuilt_mode || echo "  ... building richos-tauri (the artifact under test is built here, never assumed)"
# The artifact is built INSIDE this checkout, so it names its commit the way a packaged bundle
# does (package-app.sh stamps RICHOS_SOURCE_SHA; engine::source_commit reads it at compile time).
if ! gui_prebuilt_mode \
   && ! ( cd "$APP_DIR/src-tauri" && RICHOS_SOURCE_SHA="$(git -C "$APP_DIR" rev-parse HEAD)" cargo build --quiet --bin richos-tauri ); then
  # EXIT 1, NOT 2, AND THE DIFFERENCE IS THE WHOLE POINT OF THE TWO CODES. Exit 2 from this
  # suite means "this host cannot answer" and `run-tests.sh` will tolerate it when a caller
  # has declared the gap. A binary that does not compile is not a fact about the host; it is
  # a verdict about the code, and it must be able to stop a build even on a runner that is
  # allowed to skip the boot.
  echo "gui-boot.test.sh: richos-tauri did not build. That is a verdict about the CODE." >&2
  exit 1
fi

# `.noindex`, as nightly-launch.sh names its folders: Spotlight skips a folder so named, so a
# machine's RichOS.app is never offered beside his own copy while a run is under way.
MACHINE="$TMP/machine.noindex"
if MOUT="$(gui_machine "$MACHINE" 2>&1)"; then
  ok "B0 a complete machine was built (corpus, tools, engine pointer, claude, saved company)"
  printf '%s\n' "$MOUT" | sed 's/^/         /'
else
  echo "gui-boot.test.sh: could not build the machine to boot on:" >&2
  printf '%s\n' "$MOUT" | sed 's/^/  /' >&2
  bad "B0 complete delivered machine could be provisioned" "public component/runtime provisioning failed"
  exit 1
fi

# C6 — THE CLAUDE STAND-IN MUST START AS NODE, NOT AS SHELL. gui_machine writes it with
# `#!<node>`, and in the test guest the only `node` is the delivered runtime's `bin/node`, a
# `#!/bin/sh` wrapper (it sets OPENSSL_MODULES, then execs ../node/bin/node). macOS will not
# take a script as another script's interpreter: execve answers ENOEXEC, and the app's spawn
# (Rust's Command, like execvp) then hands the stand-in to /bin/sh, which runs the JavaScript
# as shell. Nightly 71's proof (run 20261023T101237Z-84442061) failed B2 on exactly that:
# `quota reader 1: initialize: exit status: 2 ... line 2: //: is a directory`. Nightly 70
# had the same broken stand-in; its quota reader failed silently until 766f54932 printed why.
#
# The interpreter goes through /usr/bin/env, a real executable, which execs the same absolute
# node whether it is a wrapper or a binary. It is set HERE and not in lib/gui-launch.sh on
# purpose: `publish` accepts a boot proof from a descendant of the candidate that changes only
# this file (nightly-local.py only_the_checker_changed), so the fix needs no rebuild.
#
# The case then starts the stand-in the way the kernel sees it: python's subprocess calls
# execve and never falls back to /bin/sh, so an interpreter macOS refuses is a red case here
# instead of a JavaScript file run as shell inside the boot.
stand_in_interpreter() {   # stand_in_interpreter <claude stand-in>
  case "$(head -1 "$1")" in
    '#!/usr/bin/env '*) ;;
    '#!/'*) sed -i '' "1s|^#!|#!/usr/bin/env |" "$1" ;;
  esac
}
stand_in_case() {   # stand_in_case <claude stand-in>
  local stand_in="$1" out
  if out="$(python3 -I - "$stand_in" 2>&1 <<'PY'
import json, select, subprocess, sys
try:
    child = subprocess.Popen([sys.argv[1], "--print"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, env={"PATH": "/usr/bin:/bin:/usr/sbin:/sbin"})
except OSError as error:
    sys.exit(f"execve refused the stand-in: {error}")
child.stdin.write(b'{"type":"control_request","request_id":"quota-1","request":{"subtype":"initialize"}}\n')
child.stdin.flush()
ready, _, _ = select.select([child.stdout], [], [], 10)
line = child.stdout.readline() if ready else b""
child.stdin.close()
try:
    child.wait(timeout=10)
except subprocess.TimeoutExpired:
    child.kill()
    child.wait()
try:
    reply = json.loads(line)
except ValueError:
    sys.exit(f"no reply to initialize (exit {child.returncode}); stderr: {child.stderr.read()[:300]!r}")
if reply.get("type") != "control_response" or reply["response"].get("request_id") != "quota-1":
    sys.exit(f"not an answer to initialize: {line[:300]!r}")
PY
)"; then
    ok "C6 the claude stand-in starts by a plain execve under launchd's PATH and answers initialize"
  else
    bad "C6 the claude stand-in starts by a plain execve under launchd's PATH" "$out; its first line: $(head -1 "$stand_in")"
  fi
}
stand_in_interpreter "$MACHINE/.local/bin/claude" \
  || bad "C6 the claude stand-in starts by a plain execve" "could not set its interpreter line"
stand_in_case "$MACHINE/.local/bin/claude"

# C5 — THE ONE CAPABILITY CASE THAT NEEDS THE BINARY, so it lives here rather than with
# C1-C4. The bundle on the machine has to declare the version the binary reports for itself:
# `update_startup::prepare` reads the plist version and, when it differs from the compiled
# one, treats the running app as CHANGED and — with no publication receipt on a scratch
# machine — refuses to start at all (update_startup.rs:74-78, 124-133). A stale or copied
# bundle would kill the boot as thoroughly as no plist did, and say something else entirely.
C5_WANT="$(gui_identity "$GUI_BINARY" 2>/dev/null || true)"
C5_PLIST="$MACHINE/Applications/RichOS.app/Contents/Info.plist"
C5_FLAT="$(tr -d '\n\t' < "$C5_PLIST" 2>/dev/null || true)"
C5_GOT="${C5_FLAT#*<key>CFBundleShortVersionString</key>}"
C5_GOT="${C5_GOT#*<string>}"
C5_GOT="${C5_GOT%%</string>*}"
if [ -n "$C5_WANT" ] && [ -n "$C5_GOT" ] && [ "$C5_GOT" = "${C5_WANT##* }" ]; then
  ok "C5 the machine's bundle declares ${C5_WANT##* }, the version the binary says it is"
else
  bad "C5 the machine's bundle declares the version the binary says it is" \
      "the binary reports '${C5_WANT:-<no answer>}' and the plist says '${C5_GOT:-<nothing>}'"
fi

# =========================================================================================
# D1-D4 — the precondition, decided on numbers and then measured on this host
# =========================================================================================
#
# The header carries the finding. These four run AFTER the build and after B0, deliberately:
# a binary that does not compile and a machine that cannot be provisioned are verdicts about
# the CODE, and they must be able to report themselves on a host this gate is about to
# refuse. Order is the whole of that rule and it costs nothing here — D1-D3 are arithmetic.

gui_display_verdict 0 >/dev/null 2>&1; D_RC=$?
if [ "$D_RC" -eq 1 ]; then
  ok "D1 an active-display count of 0 is refused"
else
  bad "D1 an active-display count of 0 is refused" \
      "gui_display_verdict answered $D_RC for a machine with no awake display. A precondition \
that accepts a zero is a precondition that would have passed the run this exists to stop."
fi

gui_display_verdict 3 >/dev/null 2>&1; D_RC=$?
if [ "$D_RC" -eq 0 ]; then
  ok "D2 an active-display count of 3 is accepted"
else
  bad "D2 an active-display count of 3 is accepted" \
      "gui_display_verdict answered $D_RC for a machine with three awake displays — a check \
that refuses everything is not a check, it is an outage."
fi

gui_display_verdict "unknown" >/dev/null 2>&1; D_RC=$?
gui_display_verdict "" >/dev/null 2>&1; D_RC_EMPTY=$?
if [ "$D_RC" -eq 2 ] && [ "$D_RC_EMPTY" -eq 2 ]; then
  ok "D3 a count that could not be measured is neither a zero nor a pass"
else
  bad "D3 a count that could not be measured is neither a zero nor a pass" \
      "gui_display_verdict answered $D_RC for 'unknown' and $D_RC_EMPTY for an empty string. \
An unmeasured premise read as either answer is the silent-pass this file refuses everywhere."
fi

# D4 — THE HOST, ASKED. The answer is printed whichever way it goes, because the absence of
# this line is what made the 2026-09-17 nightly unreadable.
read -r D_ACTIVE D_ONLINE <<<"$(gui_display_counts)"
D_SENTENCE="$(gui_display_verdict "$D_ACTIVE")"; D_RC=$?
case "$D_RC" in
  0)
    ok "D4 this host can answer: $D_SENTENCE ($D_ONLINE online)"
    ;;
  1)
    echo "" >&2
    echo "gui-boot.test.sh: NO DISPLAY ON THIS MAC IS AWAKE, so this host cannot answer B2." >&2
    echo "                  CGGetActiveDisplayList reports 0 active of $D_ONLINE online display(s)." >&2
    echo "                  macOS drops a SLEEPING display from the ACTIVE list, and" >&2
    echo "                  app.available_monitors() IS that list (tao-0.35.3, macos/monitor.rs:146)," >&2
    echo "                  so the boot below would truthfully print 'window: the runtime reported" >&2
    echo "                  no attached display' and B2 would go red over the state of the screen." >&2
    echo "                  Measured 2026-09-17: run 84aa5e62 failed exactly this way at 02:06Z," >&2
    echo "                  inside the 01:33:47Z-02:16:29Z window pmset recorded the displays off;" >&2
    echo "                  the same source passed at 02:25Z with them awake." >&2
    echo "" >&2
    echo "                  WAKE THE SCREEN AND RE-RUN. Press a key, or:" >&2
    echo "                      caffeinate -u -t 1                 # wake it, then run the suite" >&2
    echo "                      caffeinate -u -- bash scripts/gui-boot.test.sh" >&2
    echo "                                                         # wake it and hold it awake for" >&2
    echo "                                                         # the whole run (it turns the" >&2
    echo "                                                         # screen ON — that is a visible" >&2
    echo "                                                         # side effect on this desk)" >&2
    host_gap_exit
    ;;
  *)
    echo "" >&2
    echo "gui-boot.test.sh: THIS HOST COULD NOT BE ASKED how many of its displays are awake." >&2
    echo "                  gui_display_counts needs python3 and CoreGraphics; one of them did not" >&2
    echo "                  answer. Refusing to report a result over an unmeasured precondition —" >&2
    echo "                  an unknown is not a zero and it is not a green light either." >&2
    host_gap_exit
    ;;
esac

# B1/B2 — the positive half.
if gui_boot "$MACHINE" "$TMP/healthy-boot.log" 60; then
  ok "B1 the healthy machine boots to completion under launchd's environment"
else
  bad "B1 the healthy machine boots to completion" "$(tail -3 "$TMP/healthy-boot.log" | tr '\n' ' ')"
fi

# B1a — NOTHING THIS SUITE STARTS REACHES ANYBODY'S SCREEN, asserted rather than assumed.
#
# This case is new because the fix above changed the facts it rests on. `activation::decide`
# needs B (a bundle declaring this build's identifier), D (the installed data directory) and
# P (parent pid 1) for a REGULAR launch — Dock icon, window in front, keyboard taken. Until
# 2026-09-10 the harness bundle carried no Info.plist, so B failed and two conditions stood
# between this suite and the CEO's screen. It now carries one, deliberately, so B holds and
# D holds; P is what is left, and P cannot hold here by construction — `gui_boot` runs the
# app as `( … ) &` under this shell, so the parent is this script and never launchd.
#
# One condition is enough and it is also exactly one condition, so it is CHECKED. The
# `resolved 'activation'` rule accounts for `regular` too — correctly, since it is a real
# answer for a real install — which means B2 alone would pass a boot that took the screen.
NO_SCREEN='^\[richos\] activation: accessory — no Dock icon, no window on screen, no focus taken, because this is not an installed launch:'
if grep -Eq -- "$NO_SCREEN" "$TMP/healthy-boot.log"; then
  ok "B1a the boot took no Dock icon, no window and no focus, decided on its own facts"
else
  bad "B1a the boot took no Dock icon, no window and no focus" \
      "the activation line was: $(grep -m1 '^\[richos\] activation:' "$TMP/healthy-boot.log" | cut -c1-160)"
fi
echo "  --- the boot log, as captured ---"
sed 's/^/      /' "$TMP/healthy-boot.log"
echo "  --- accounting ---"
if OUT="$(gui_account "$TMP/healthy-boot.log" 2>&1)"; then
  printf '%s\n' "$OUT"
  ok "B2 every line of a healthy Finder-condition boot is accounted for"
else
  printf '%s\n' "$OUT"
  bad "B2 every line of a healthy Finder-condition boot is accounted for" "see the report above"
  # THE ONE RED THAT IS NOT ABOUT THE CODE, NAMED WHERE IT IS READ. D4 passed minutes ago, so
  # if the boot found no display the screen went to sleep DURING this run — a race a
  # precondition checked once cannot close. B2 STAYS RED (the boot really did fail to place a
  # window against a display), and the reader is told which repair this is.
  if grep -Fq '[richos] window: the runtime reported no attached display' "$TMP/healthy-boot.log"; then
    read -r D2_ACTIVE D2_ONLINE <<<"$(gui_display_counts)"
    echo "         ---"
    echo "         THIS RED IS ABOUT THE SCREEN, NOT THE SOURCE. D4 measured $D_ACTIVE awake"
    echo "         display(s) before the boot; this host now reports $D2_ACTIVE awake of $D2_ONLINE online."
    echo "         available_monitors() is CGGetActiveDisplayList, and macOS drops a SLEEPING"
    echo "         display from that list (tao-0.35.3, macos/monitor.rs:146), so a screen that"
    echo "         slept mid-run leaves the app truthfully reporting no attached display."
    echo "         Re-run with the screen held awake:  caffeinate -u -- bash scripts/gui-boot.test.sh"
  fi
fi

# B3-B7 — the negative halves. One configuration removed from the machine at a time. Each
# proves that the detector for THAT seam is alive on this run: a check that only ever sees a
# healthy boot is a check that would keep passing after it stopped working.
break_and_boot() {   # break_and_boot <name> <case-id> <what-to-remove-cmd...>
  local name="$1" id="$2"; shift 2
  local broken="$TMP/broken-$id.noindex"
  rm -rf "$broken"
  # Cloned (APFS clonefile, `cp -c`): the machine carries a 322 MB runtime and, since the video
  # tools became a setup essential, a 487 MB speech model, and a byte copy of both per case is
  # gigabytes for nothing. A plain copy where cloning is not possible.
  if ! cp -ac "$MACHINE" "$broken" 2>/dev/null; then
    rm -rf "$broken"
    cp -a "$MACHINE" "$broken" || { bad "$id $name" "could not copy the machine"; return; }
  fi
  ( cd "$broken" && "$@" ) || { bad "$id $name" "could not break the machine"; return; }
  gui_boot "$broken" "$TMP/broken-$id.log" 60
  if gui_account "$TMP/broken-$id.log" >"$TMP/broken-$id.report" 2>&1; then
    bad "$id $name" "the check PASSED a machine with this missing. Report: $(tr '\n' ' ' < "$TMP/broken-$id.report" | cut -c1-200)"
  else
    ok "$id $name"
    grep -E 'UNACCOUNTED|NOT RESOLVED' "$TMP/broken-$id.report" | head -4 | sed 's/^/       /'
  fi
}

break_and_boot "the engine pointer is removed -> caught"      B3 rm -f  .claude/richos-engine
break_and_boot "the corpus pointer is removed -> caught"      B4 rm -rf "Library/Application Support/RichOS/corpus" RichOS
break_and_boot "the delivered Loro component is removed -> caught" B5 rm -rf FixtureDelivery/engine/loro
break_and_boot "the claude stand-in is removed -> caught"     B6 rm -f  .local/bin/claude
# Require the intended readiness detector, not an unrelated boot failure. The deferred
# compute-policy line alone must never certify a machine with no executable to connect to.
if grep -Fq 'NOT RESOLVED  first-run setup' "$TMP/broken-B6.report" \
  && grep -Fq '[richos] first-run setup: claude-code is NOT installed' "$TMP/broken-B6.log"; then
  ok "B6a missing Claude is rejected specifically by first-run setup"
else
  bad "B6a missing Claude is rejected specifically by first-run setup" \
      "the executable-readiness proof did not detect the missing Claude stand-in"
fi
break_and_boot "the saved company is removed -> caught"       B7 rm -f  "Library/Application Support/com.richos.app/config.json"

# B9 — the video tools, a setup essential since 2026-10-07 (media-tools plan, slice 3). With the
# speech model gone, setup is incomplete, and it must be first-run setup that says so. The boot
# line names the component by its machine name (`Component::as_str`, "media-tools"), never by the
# name on the sheet, so a rename on screen cannot break this check (dictation plan revision 2, M8).
break_and_boot "the speech model is removed -> caught"        B9 rm -rf .config/richos/models
if grep -Fq 'NOT RESOLVED  first-run setup' "$TMP/broken-B9.report" \
  && grep -Fq '[richos] first-run setup: media-tools is NOT installed' "$TMP/broken-B9.log"; then
  ok "B9a a missing speech model is rejected specifically by first-run setup, as the video tools"
else
  bad "B9a a missing speech model is rejected specifically by first-run setup, as the video tools" \
      "the video-tools proof did not detect the missing speech model"
fi

# B8 — the seam the entity registry added, and the break is a CORRUPTION rather than a
# removal. Deleting `entities.json` is HEALED by the no-orphan migration (main.rs:895): the
# machine has already booted once above, so its ledger holds a thread under `northwind`, the
# registry loads `Absent`, the ids that already own records are re-registered and the boot
# resolves normally. Measured on 2026-09-04, capture under
# `docs/verification/gui-boot-entity-registry-2026-09-04/`. A negative case a repair path
# heals proves nothing about the detector, so this one leaves a file that exists and cannot
# be trusted — the state the migration is gated against and the one the product must refuse.
break_and_boot "the company registry is corrupt -> caught"    B8 \
  sh -c 'printf "{ not json" > "Library/Application Support/com.richos.app/entities.json"'

# =========================================================================================
# Y — the reaper, proved on real processes
# =========================================================================================
#
# `gui_kill` is the piece that failed on 2026-09-01, so it does not get to be trusted. Two
# cases, and the second is the one that matters: a process that IGNORES `TERM` must still be
# gone when the function returns, or "I killed it" is a claim about a signal rather than
# about a process.

# THE BACKGROUND JOB'S STDOUT IS REDIRECTED, and that redirect is worth two sentences
# because leaving it out cost this suite two minutes a run. `$( … )` waits for the pipe to
# CLOSE, not for the shell to exit — and a backgrounded `sleep 60` inherits that pipe, so
# `$(bash -c 'sleep 60 & echo $!')` blocks for the full sixty seconds before handing back a
# pid. Measured: a traced run showed 60.009s and 60.010s parked on exactly these two lines,
# out of 128s total, and the rest of the suite adds up to about eight.
Y="$(bash -c 'sleep 60 >/dev/null 2>&1 & echo $!')"
if gui_kill "$Y" && ! kill -0 "$Y" 2>/dev/null; then
  ok "Y1 gui_kill ends an ordinary process and the pid is gone when it returns"
else
  bad "Y1 gui_kill ends an ordinary process" "pid $Y survived, or gui_kill reported failure"
  kill -9 "$Y" 2>/dev/null
fi

# `trap "" TERM` — TERM is ignored, so only the KILL escalation can end this one.
Y="$(bash -c 'bash -c '"'"'trap "" TERM; sleep 60'"'"' >/dev/null 2>&1 & echo $!')"
if gui_kill "$Y" && ! kill -0 "$Y" 2>/dev/null; then
  ok "Y2 gui_kill escalates to KILL for a process that ignores TERM"
else
  bad "Y2 gui_kill escalates to KILL" "pid $Y survived both signals"
  kill -9 "$Y" 2>/dev/null
fi

# =========================================================================================
# Z — this run left nothing running
# =========================================================================================
#
# A CASE AND NOT A COMMENT, because on 2026-09-01 this suite left 157 orphaned RichOS
# processes running with their windows on the CEO's Dock — six per round, across about
# twenty-six rounds, every one of them reparented to PID 1 out of a temp directory that had
# already been deleted. `gui_boot` killed the subshell and not the app.
#
# So the count is ASSERTED and PRINTED, here, on every run. Each pid this run started is
# TERM'd, waited for, KILL'd and then checked; anything still alive is a failure with a
# number beside it. The number in the run output is the evidence — "there is no residue" is
# not a thing anybody gets to say without it.
LAUNCHED="$(grep -c . "$GUI_LAUNCHED_PIDS" 2>/dev/null || echo 0)"
STILL=0
while IFS= read -r p; do
  [ -n "$p" ] || continue
  kill -0 "$p" 2>/dev/null && STILL=$((STILL + 1))
done < "$GUI_LAUNCHED_PIDS"
if [ "$STILL" -eq 0 ]; then
  ok "Z this run launched $LAUNCHED app(s) and $STILL are still running"
else
  bad "Z this run launched $LAUNCHED app(s) and $STILL are still running" \
      "every launch must be ended by the run that made it; the EXIT trap will try again"
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
  echo "=== gui-boot.test.sh: $FAIL FAILED, $PASS passed ==="
  exit 1
fi
echo "=== gui-boot.test.sh: all $PASS passed ==="
