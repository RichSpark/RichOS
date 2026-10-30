#!/usr/bin/env python3
"""proof-run.py — RUN a proof-for.sh selection: concurrently, admitted by CPU, one summary.

    proof-run.py [proof-for arguments]          e.g.  origin/main..main   |  --working  |  <sha>
    proof-run.py --commands <file>              a saved `proof-for.sh --quiet` output
    proof-run.py --resume <run-directory>       retry a frozen plan, preserving valid results
    proof-run.py --coverage <run-directory>     readiness preflight, reads only: does a saved
                                                handover run prove its candidate's whole plan here?
    proof-run.py --release <run-directory>      remove a handover hold once nothing needs that run
    options:
      --fail-fast          cancel independent unfinished work after a failure (opt-in)
      --keep-going         compatibility alias for default continuation
      --dry-run            print the plan (items, lanes, expected seconds) and run nothing
      --as-printed         run the printed commands exactly as printed, one after another (the
                           hand-written loop's shape, to measure what the runner saves)
      --capacity N         the run's budget of concurrent WORKERS, nested ones included
                           (default: 80% of logical cores — the admission line, in cores)
      --engine-shards N    at most N engine shards (default: logical cores / 2)
      --admission-wait S   how long the Mac may refuse one check before it is NOT ADMITTED
                           (default 1800 s); time queued behind this run's own checks does not
                           count (refusal_counts)
      --slot-wait S        how long this RUN may wait for a proof-run slot on this Mac (default
                           10800 s; see RUNS SHARE THE MAC below)
      --budget S           a check running past S seconds is named while the run goes (600)
      --deadline S         a check still running at max(S, 3 x its expected seconds), capped
                           at an hour, is stopped with its whole tree and fails (1800)
      --cap S              a HARD cap instead: every check, engine units included, is stopped
                           with its whole tree at S seconds, whatever it was expected to take
                           (TIMED-OUT, "stopped at its S s cap"). The merge gate passes 600.
      --run-cap S          the whole run stops at S seconds: running checks are stopped with
                           their trees and waiting ones never start, all `cancelled` with the
                           reason, and the summary is written as for any other end. Time the
                           run's engine units only wait for the engine slot, themselves or
                           through another run's identical check, does not count. Unset: none.
      --sample-every S     how often the host is sampled for the run's summary (10)
      --log-dir DIR        an empty directory for evidence (default: per-checkout SSD storage
                           on macOS; retain failed/unfinished runs and the last 3 green runs)
    There is no low-priority switch: CEO ruling §78's mode is for the native app build, and a
    runner of tests never skips the CPU line.

      --summary-out F      also write summary.json to F (the land gate reads its verdict there)
      --without-nightly-conditions
                           run every check in the caller's environment only, to tell whether a
                           failure comes from the nightly's conditions (below). Refused inside
                           the land checks (RICHOS_AUTOCHECK_ACTIVE): the merge gate always
                           meets them.
Exit: 0 every check passed; 1 a check failed, timed out, was not admitted, the run was not
admitted to a proof-run slot within --slot-wait (every check is then `not-admitted` in the
summary, which is written all the same), or proof-for.sh
found a changed code path no suite covers (nothing is run then); 2 usage or an unreadable
selection; 3 nothing failed, and at least one check was NOT RUN (see below). 3 is not 0: a
caller that reads any non-zero exit as "not green" stays right.

A CHECK THAT DID NOT RUN IS NEVER RECORDED AS PASSED (2026-09-29). The land of
cc/zach-opus-autocheck1 recorded `front-door` and `gui-boot` as `passed` while their logs said
"0 of 1 suites passed — 0 checks — 1 NOT RUN (no screen)": run-tests.sh exits 0 for a suite
it deliberately did not run, and exit 0 was all this runner read. Now every run-tests.sh check
is given `--results-out` in this run's log directory and its state is read from that file, the
same per-suite record the nightly puts into build-info.json: a suite recorded `notrun` (no
screen), `gap` (a declared host gap) or `skipped` (RUN_TESTS_SKIP_UNCHANGED) makes the check
NOT RUN, a state of its own, with the reason ("no-screen", "host-gap", "unchanged-inputs") in
progress.json, outcomes.json, summary.json (`result: not-run`, `not_run: {why, suites}`), the
live line and the final line. A run-tests.sh check that exits 0 without writing that file is
`invalid`: it cannot show that it ran. What a caller does with NOT RUN is the caller's
decision; the land gate's is in autocheck/README.md.

The same holds for a UI suite run directly (`cd richos/app/ui/tests && node <suite>.js`,
2026-09-30, hunt part 2 finding 18): it is given the evidence ledger run.js gives its children
(RICHOS_UI_TESTS_LEDGER) and its state is read from it. A suite that recorded only a skip
(lib/harness.js skipSuite) is NOT RUN (`suite-skipped`); one that exits 0 with no check or a
failed check in its ledger is `invalid`.

A CHECK MEETS THE NIGHTLY'S CONDITIONS BEFORE THE NIGHTLY DOES (2026-10-01). Five nightly
attempts in one night each failed on a suite that had passed for its engineer; two of them
only because the build hands its suites what an engineer's shell does not: a long TMPDIR
(cargo-cache-env.test.sh, "path must be shorter than SUN_LEN") and RICHOS_IOS_POOL_WAIT
(merge-check-scope.test.py, KeyError). So a check of a suite a nightly gate runs (a
`run-tests.sh --only` suite the desktop build runs, by run-tests.sh's own `--for desktop
--list`, and a UI suite run directly) gets what that gate would give it, from nightly-local.py
gate_conditions(): the build's TMPDIR, PATH and every variable it sets for that gate, its own
check-specific values (item.env) still winning. Derived, never copied: a value added to the
build reaches these checks with no edit here. The plan prints which checks run under which
gate's conditions and anything this machine cannot reproduce. Phone-app suites, cargo, engine
units and anything else no desktop gate runs keep the caller's environment.

A RETRY ON THE SAME TREE RUNS ONLY WHAT DID NOT PASS (2026-09-29). `--resume` and every new run
reuse a validated pass whose input identity is unchanged (lib/proof_evidence.py). A check with
a reviewed contract in proof-inputs.json is keyed by the inputs it declares; every other check
is keyed by the whole checkout's content (WHOLE_CHECKOUT there), so a refused land tried again,
or the push after it, never re-runs a check that already passed on that exact content.

Two gaps kept that promise from the merge gate until 2026-10-02 (the land of
cc/zach-opus-e2fix1, 214 checks, refused four times with no check failing; every retry re-ran
what had passed). Engine units without a reviewed contract (91 of the 214) were `fresh`, never
reused, and now take the whole-checkout identity like every other unqualified check; the
coverage proof still re-reads each one (proof_evidence.verify_target_receipts). And a check's
identity carried --admission-wait, --slot-wait and --engine-slot-wait, so the gate's retry
(900 s) matched nothing its first run (893 s, what was left of the gate) had passed: those
bound how long a check may wait to START, never what it runs, and are no longer part of it.

WHY THIS EXISTS (2026-09-23). Verification exposed unbounded nested workers, descendants
surviving timeouts and failures that were discovered only after long waits. This runner
owns scheduling, admission, logs and cancellation so callers do not reconstruct that protocol.

WHAT IT DOES WITH EACH FAMILY OF COMMAND, and why nothing is dropped:
  * engine `ci-shard.sh --only-units <u>` lines retain ONE full units file and the
    engine planner's shard assignment. Each shard lane admits one exact unit at a time,
    releasing its worker permit between units. Each unit leaves its own receipt; a
    final `ci-shard.sh --verify-receipts` proves the union of units run equals the set
    selected. A unit that silently did not run fails the land, as it does in CI.
  * `run-tests.sh --only a --only b ...` becomes one `run-tests.sh --only <suite>` per
    suite, so every suite is scheduled, timed and logged on its own. Each keeps
    run-tests.sh's own semantics (declared host gaps, NOT RUN without a screen).
  * `cargo test` lines share one Cargo target, whose lock would serialize them anyway;
    they run in one lane, and a filter already matched by a shorter filter for the same
    target is dropped only because cargo's filter is a substring match (`phone::` runs
    every test `phone::attachments::` names).
  * everything else runs as printed.

WHAT MUST NOT RUN AT THE SAME TIME, and how that is decided (derived, never typed):
  * lane `cargo`   every cargo command (one target directory, one lock);
  * lane `gradle`  every app suite that drives bin/randroid (one Gradle project);
  * lane `guest`   every host-screen suite when RICHOS_GUI_HOST names the test VM (one
                   guest slot at a time from this lane; run-suite.sh boots it through run-walk.py);
  * iOS suites own their devices. Shared simulator limits serialize boots and cap live devices;
    per-cache locks prevent independent runs from replacing one another's build or device state.

A FAILING CHECK IS NAMED BY WHAT FAILED IN IT (2026-09-25). Each check gets a results folder in
this run's log directory (RICHOS_TEST_RESULTS_ROOT); run-tests.sh gives each suite one under it,
and a test run copies its per-test result files there the moment it ends (lib/test_results.py),
so a later check writing the same build folder cannot erase them. The failing tests' names are
printed when the check ends and again beside it in the summary, and written to summary.json.
The run that made this necessary: `native-android-app` failed "133 tests completed, 1 failed",
and `native-android-ui` replaced the one report naming the test before anyone read it.

ENGINE SHARDS SHARE THIS CHECKOUT. In CI each shard had a machine of its own; here they run side
by side in one tree. Every unit still has ci-shard.sh's leak canary, so a write outside a sandbox
is still caught; what changes is attribution: a leak by a unit in one shard can also be charged
to whatever unit another shard is running at that moment. The run is red either way; read both.

REFERENCE (T3 Code adoption ledger, richos-hq docs/research/t3code-tooling-what-to-adopt-2026-09-19.md):
§2.3 "shard across runners, not workers" — COPIED as receipted engine shards and one simulator
per iOS shard, because on one Mac with CI paused the separate "runners" are processes and
simulators. §2.4 "every CI job capped at 10 minutes" — COPIED as a budget enforced while the run
goes: named live at --budget, stopped with its whole tree at the deadline. Their CI workflow
files themselves are NOT APPLICABLE (CI paused).

ADMISSION, TWO CONDITIONS, BOTH CHECKED BEFORE EVERY START:
  * A FREE WORKER TOKEN (scripts/lib/worker_tokens.py, engine). The run has --capacity tokens and also uses the shared machine ceiling;
    each check holds one while it runs, and the budget's directory is exported to it as
    RICHOS_WORKER_TOKENS, so a nested pool (the mutation harness's eight workers, native-ios-ui's
    extra simulators) runs one worker on its caller's token and takes a token for every other.
    The count is of workers on the machine from this run, not of commands this runner typed.
    A quarter of the tokens are reserved: nested workers never take them, so a check not yet
    started always can.
  * CEO RULING §77's LINE, the same rule and the same code as testvm/reserve.py: one sample of
    total CPU (user plus system) and memory; below 80% it starts, otherwise the check waits (samples at least 30 s
    apart) and the wait is reported beside its time. --admission-wait bounds only the time the Mac
    refused it while nothing of this run was running; time queued behind this run's own checks,
    including a CPU or pressure refusal while they run, is reported, never charged (refusal_counts).
Checks start longest-expected first, the expectation being each check's measured median, so the
long poles are not the ones left waiting; under --cap (the merge gate) a check planned past the
cap is not started (start_key, leave_over_cap, MEDIAN_SAMPLES). While the
run goes, a sampler records total CPU and the tokens held; the summary prints both, which is the
evidence of host use, not a guarantee that running compilers stay under the admission line.

THE MACHINE BUDGET for separate runners and nightlies is shared through worker_tokens.machine_directory(). Local --capacity remains an additional
ceiling. All nested workers must acquire both budgets or borrow their caller's held slot.

RUNS SHARE THE MAC (2026-09-27). The machine budget counts CHECKS, and one check (cargo, shellcheck,
xcodebuild, Gradle) can use several cores; the CPU line is one sample per check, so runs started in
the same minute each saw a free Mac and stacked. So before its first check a run takes a host-wide
proof-run SLOT (scripts/lib/proof_slots.py): measured, one run alone peaks at the whole Mac, so the
default is one run at a time, and the lead sets another number for the whole Mac with
`proof_slots.py set N`. A run over the limit waits in first-come order, recorded as a CPU-admission
wait for the lead's turn-end gate; `python3 scripts/lib/proof_slots.py status` shows who holds and
who waits. The slot is an flock the kernel drops with the run's last descriptor (the run's checks'
supervisors hold it too), so a killed run releases it once its checks are stopped. A proof run
started by a check of a proof run works inside its caller's slot.
"""
import argparse
import copy
import uuid
import hashlib
import importlib.util
import math
import tempfile
import datetime
import json
import os
import re
import shlex
import shutil
import signal
import statistics
import subprocess
import sys
sys.dont_write_bytecode = True
import threading
import time
import traceback
from pathlib import Path

HERE = os.path.dirname(os.path.abspath(__file__))
APP = os.path.dirname(HERE)
sys.path.insert(0, os.path.join(HERE, "testvm"))
import reserve  # noqa: E402  (the admission rule, not a copy of it)

SETTLE_SECONDS = 3        # a check just started has not shown its load yet
KEEP_RUNS = 3
# T3 Code caps every CI job at 10 minutes (adoption ledger §2.4, COPY). Here: a check past the
# budget is named while the run goes, and one still running at its deadline (deadline_for) is
# stopped with its whole tree and fails the run by name. The deadline sits above the budget
# because a kill at exactly ten minutes under a contended Mac would fail healthy checks.
BUDGET_SECONDS = 600
DEFAULT_WEIGHTS = (("native-ios", 900), ("native-android", 600), ("cargo", 300), ("engine", 300))
# A check whose measured time does not fit the hour ceiling gets its own deadline. native-ios-ui
# runs its two simulators one after the other, each the whole selection: in proof run
# 20261023T044649Z-3za63spa (2026-10-23) device 1 took 2234 s (124 cases, 0 failures) and device 2,
# at the same pace, was stopped at 2700 s. Two devices of about 2230 s plus a margin.
SUITE_DEADLINES = {"native-ios-ui": 5400.0}


def git_root():
    out = subprocess.run(["git", "-C", APP, "rev-parse", "--show-toplevel"], capture_output=True, text=True)
    return out.stdout.strip() or os.path.dirname(os.path.dirname(APP))


ROOT = git_root()
# How a check is stopped: the engine's one tree-kill, not a second copy of it.
sys.path.insert(0, os.path.join(ROOT, "richos", "engine", "scripts", "lib"))
import machine_settings as M
import proc_tree  # noqa: E402
import worker_tokens  # noqa: E402
import engine_pass  # noqa: E402
import cpu_guard  # noqa: E402
# THE HOST SHIELD (ceo-decisions §65, engine lib/host_shield.py). Every check starts through it:
# a shell suite's source is checked first, then the check runs with stand-ins for this Mac's power,
# sleep, lock and display commands first on PATH, and one that called a stand-in fails with its
# logged line. Applied to the started command only, never to item.argv or item.env, so it is no
# part of any check's input identity or its rerun record.
HOST_SHIELD = os.path.join(ROOT, "richos", "engine", "scripts", "lib", "host_shield.py")
if not os.path.isfile(HOST_SHIELD):
    HOST_SHIELD = os.environ.get("RICHOS_HOST_SHIELD_TOOL") or None
sys.path.insert(0, os.path.join(HERE, "lib"))
import test_results  # noqa: E402  (what names a failing test; one reader for every caller)
import proof_evidence  # noqa: E402
import proof_slots  # noqa: E402  (how many proof runs share the Mac at once)

# The run's host-wide slot (main); every check's supervisor holds it too, so a killed runner keeps it
# only until its checks are stopped. Empty when run() is driven directly (the tests).
SLOT = None
# A caller's execution deadline starts when the first check has every admission:
# the proof slot alone leaves worker, CPU and resource queues still to wait in.
# Consume the marker here so a child cannot start or reset its caller's clock.
EXECUTION_MARKER = os.environ.pop("RICHOS_EXECUTION_STARTS_MARKER", "")


def begin_execution():
    global EXECUTION_MARKER
    if EXECUTION_MARKER:
        try:
            open(EXECUTION_MARKER, "w").close()
        except OSError as exc:  # the caller refuses unconfirmed execution after bounded admission
            print("proof-run: could not tell the caller its clock starts now: %s" % exc, flush=True)
        EXECUTION_MARKER = ""  # later checks must never reset the same gate deadline


class Item:
    def __init__(self, label, cwd, argv, lane=None, weight=60.0, after=(), requires=()):
        self.label, self.cwd, self.argv, self.lane = label, cwd, argv, lane
        self.weight, self.after = weight, set(after) | set(requires)
        self.requires = set(requires)
        self.state = "waiting"          # waiting | running | passed | failed | not-admitted
        self.rc = None
        self.admission_wait = 0.0
        self.started = self.ended = None
        # Seconds the MAC refused this check (refusal_counts), the only time --admission-wait
        # bounds; queue time behind this run's own checks is admission_wait (total queue) only.
        self.refused_wait = 0.0
        self.refused_base = 0.0         # refused_wait when this attempt's limit began
        self.refusing = False           # the next scheduler interval is a counted refusal
        self.owner_wait_since = None    # waiting for another run's identical check (Pool)
        self.owner_slot_wait = False    # ...whose owner is itself queued for the engine slot
        self.proc = None
        self.log = None
        self.notes = []
        self.env = {}
        self.token = None
        self.over_budget = False
        self.results = None             # this check's own per-test results folder (launch)
        self.failing = []               # what failed in it, by name (name_failures)
        self.slot_wait = 0.0
        self.wait_times = {}
        self.wait_reason = "ready"
        self.queued_at = None
        self.attempts = []
        self.reservation = None

    def finish_queue(self):
        if self.queued_at is not None and self.started is None:
            self.admission_wait += time.monotonic() - self.queued_at
            self.queued_at = None

    @property
    def engine_unit(self):
        return self.argv[:2] == ["bash", "scripts/ci-shard.sh"] and "--only-units" in self.argv

    @property
    def seconds(self):
        return (self.ended - self.started) if (self.started and self.ended) else 0.0

    @property
    def previous_attempts(self):
        current = getattr(self, 'verification_result', None) if self.started is not None else None
        return [attempt for attempt in self.attempts if attempt['supervision'] != current]

    @property
    def total_seconds(self):
        return self.seconds + sum(attempt['seconds'] for attempt in self.previous_attempts)


# ---------------------------------------------------------------------------------------
# the selection
# ---------------------------------------------------------------------------------------
def selection(args):
    if args.commands and args.proof_for_args:
        raise SystemExit("proof-run: unexpected arguments with --commands: " + " ".join(args.proof_for_args))
    if args.commands:
        with open(args.commands) as fh:
            return [l.rstrip("\n") for l in fh if l.strip()]
    cmd = [os.path.join(HERE, "proof-for.sh"), "--quiet", *args.proof_for_args]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode == 1:
        sys.stderr.write(r.stderr)
        sys.stderr.write("proof-run: proof-for.sh found changed code no suite covers. Nothing was run:\n"
                         "           a green run of an incomplete selection would be a green over a gap.\n")
        sys.exit(1)
    if r.returncode != 0:
        sys.stderr.write(r.stderr)
        sys.stderr.write("proof-run: proof-for.sh exited %d; nothing was run.\n" % r.returncode)
        sys.exit(2)
    return [l for l in r.stdout.splitlines() if l.strip()]


def is_host_screen(path):
    try:
        text = open(path).read()
    except OSError:
        return False
    return bool(re.search(r"^[ \t]*(\.|source)[ \t]+\S*lib/gui-launch\.sh", text, re.M)
                or re.search(r"^# run-tests: host-screen", text, re.M))


def uses_gradle(path):
    try:
        return "bin/randroid" in open(path).read()
    except OSError:
        return False


# PLANNED FROM MEASURED MEDIANS, LONGEST FIRST (2026-10-05). weights.tsv kept ONE number per
# check, the last run's, and engine units were planned from lib/ci-unit-weights.tsv, dated data
# the run itself reported stale (contract-integrity section P planned at 51.5 s, measured 172.4 s).
# Under the merge gate's caps the plan then started a long check late: in merge110.log
# by-reference.test.sh waited 440 s in its lane, ran 443 s, was ended at the 882 s round cap and
# ran again in round 2; 15,508 s of finished work was redone that way (richos-hq
# docs/operations/2026-10-04-merge-check-speed.md, section 2). Now each row keeps the last
# MEDIAN_SAMPLES measured executions and its weight is their median: one slow or fast run does
# not move the plan, and a stale planned weight is replaced the first time the check is measured.
# Engine units are recorded too, and their measured median replaces the dated table's weight
# both in the lanes (ci-units.sh packs with it) and in the start order.
MEDIAN_SAMPLES = 9

# THE FOURTEEN-POINT SUITE WITHOUT ITS MUTATION PASS (2026-10-14). lib/ci-unit-weights.tsv has it
# at 2811.7 s, its run WITH the 86-mutant pass: nightly-engine.py runs it that way and ci-shard.sh
# derives that run's deadline from the row, so the row stays. Unless RICHOS_FOURTEEN_MUTANTS=1 the
# suite skips the pass (the merge gate sets 0), and this Mac measured that run at 166.9-217.3 s,
# median 172.0 (five checkouts' weights.tsv). Planned at 2811.7 s under the gate's 600 s cap it was
# never started (leave_over_cap): the merge of c2e47cfb7 changed workspaces.py, the gate selected
# this suite and left it to the nightly, and nightly 20261014T170730Z-784b711e found C1.5 red in
# its plain run. This checkout's own measured median still wins over this figure.
FOURTEEN_UNIT, FOURTEEN_WITHOUT_PASS = "mega-lander/tests/workspace-spec-fourteen.test.sh", 172.0


def history_samples(state):
    """{label: [seconds, ...]} from weights.tsv: `label <TAB> median [<TAB> s1,s2,...]`."""
    samples = {}
    try:
        with open(os.path.join(state, "weights.tsv")) as fh:
            for line in fh:
                parts = line.rstrip("\n").split("\t")
                if len(parts) < 2:
                    continue
                try:
                    values = ([float(v) for v in parts[2].split(",") if v] if len(parts) >= 3
                              else [float(parts[1])])
                except ValueError:
                    continue
                if values:
                    samples[parts[0]] = values[-MEDIAN_SAMPLES:]
    except OSError:
        pass
    return samples


def history_weights(state):
    """{label: median measured seconds}."""
    return {label: round(statistics.median(values), 1) for label, values in history_samples(state).items()}


def default_weight(label, hist):
    if label in hist:
        return hist[label]
    for prefix, secs in DEFAULT_WEIGHTS:
        if label.startswith(prefix):
            return float(secs)
    return 60.0


def cargo_key(argv):
    """(target, filter) for `cargo test ... [filter]`, or None when the shape is not the
    one proof-for.sh prints (then nothing is merged)."""
    if argv[:2] != ["cargo", "test"]:
        return None
    rest = argv[2:]
    flt = ""
    if rest and not rest[-1].startswith("-") and (len(rest) == 1 or rest[-2] not in ("-p", "--manifest-path", "--test", "--bin")):
        flt = rest[-1]
        rest = rest[:-1]
    return (" ".join(rest), flt)


# What a suite says when it refuses without the runtime (or the speech models): the check "needs" them.
RUNTIME_REFUSALS = ("set RICHOS_RUNTIME_DIR to a prepared, verified runtime", "a verified RICHOS_RUNTIME_DIR",
                    "a verified delivered runtime is required")


def suite_text(item):
    """The source of the run-tests suite an item runs, '' for any other kind of check."""
    argv = item.argv
    if len(argv) < 2 or argv[0] != "scripts/run-tests.sh" or argv[-2] != "--only":
        return ""
    try:
        with open(os.path.join(item.cwd, "scripts", argv[-1]), encoding="utf-8", errors="replace") as handle:
            return handle.read()
    except OSError:
        return ""


def runs_on_screen(item):
    """True when the suite will really boot the app (a host-screen suite under --no-host-screen is skipped)."""
    argv = item.argv
    return not ("--no-host-screen" in argv and is_host_screen(os.path.join(item.cwd, "scripts", argv[-1])))


def needs_runtime(item):
    """True when this check refuses without RICHOS_RUNTIME_DIR: its suite says so and will run."""
    text = suite_text(item)
    return bool(text) and any(marker in text for marker in RUNTIME_REFUSALS) and runs_on_screen(item)


def needs_speech_models(item):
    """True when this check boots gui-boot's healthy machine, which needs both speech models."""
    text = suite_text(item)
    return bool(text) and "RICHOS_GUI_SPEECH_MODELS" in text and runs_on_screen(item) and item.lane == "guest"


def supply_runtime(items, build=False):
    """Packaging suites need RICHOS_RUNTIME_DIR. Unset, the runner supplies the one the
    nightly uses (<state>/runtime-<recipe sha256[:12]>, nightly-local.py `runtime()`, named by
    lib/runtime_cache.py) to every check, only after verify-runtime.py accepts it against the
    tracked recipe, exactly as the nightly does. With `build`, a runtime that is not there is BUILT
    first when a selected check needs it (needs_runtime), so no lead or engineer builds one by hand
    and sets the variable (2026-10-28, twice); a planning pass never builds. Returns the line that
    says which, printed with the plan."""
    if os.environ.get("RICHOS_RUNTIME_DIR"):
        return "RICHOS_RUNTIME_DIR=%s (from the caller)" % os.environ["RICHOS_RUNTIME_DIR"]
    if os.path.join(HERE, "lib") not in sys.path:
        sys.path.insert(0, os.path.join(HERE, "lib"))
    import runtime_cache
    recipe = os.path.join(HERE, "runtime-sources.json")
    state = os.path.join(os.path.expanduser("~"), ".richos-nightly")
    try:
        path = str(runtime_cache.cache_path(state, recipe))
    except OSError as error:
        return "RICHOS_RUNTIME_DIR is unset and %s cannot be read (%s); suites that need it will refuse" % (
            recipe, error.strerror or error)
    if build and not os.path.isdir(path) and any(needs_runtime(it) for it in items):
        try:
            runtime_cache.ensure_runtime(state, recipe, scripts=HERE, build=True,
                                         say=lambda line: print("  " + line, flush=True))
        except runtime_cache.RuntimeNotReady as error:
            return "RICHOS_RUNTIME_DIR is unset and the runtime could not be had: %s" % error
    # The one command that builds it, printed with the refusal (what gui-proof-in-vm.sh prints too).
    build = "python3 %s %s" % (shlex.quote(os.path.join(HERE, "build-runtimes.py")), shlex.quote(path))
    if not os.path.isdir(path):
        return "RICHOS_RUNTIME_DIR is unset and %s does not exist; suites that need it will refuse. Build it with: %s" % (
            path, build)
    r = subprocess.run([sys.executable, os.path.join(HERE, "verify-runtime.py"), path,
                        os.path.join(HERE, "runtime-sources.json")], capture_output=True, text=True)
    if r.returncode != 0:
        return "RICHOS_RUNTIME_DIR is unset and %s did not verify; suites that need it will refuse. Rebuild it with: %s" % (
            path, build)
    for it in items:
        it.env["RICHOS_RUNTIME_DIR"] = path
    return "RICHOS_RUNTIME_DIR=%s (the nightly's runtime, verified against runtime-sources.json)" % path


def supply_speech_models(items, refuse=False):
    """gui-boot's healthy machine needs both pinned speech models (the video tools are a setup
    essential); the nightly hands it lib/runtime_cache.py's gui_speech_models(), so the checks
    get the same files. With `refuse`, a run that will boot gui-boot without them stops here with
    the command that fetches them instead of failing inside gui-boot. Returns the line that says which."""
    if os.path.join(HERE, "lib") not in sys.path:
        sys.path.insert(0, os.path.join(HERE, "lib"))
    import runtime_cache
    if os.environ.get("RICHOS_GUI_SPEECH_MODELS"):
        return "RICHOS_GUI_SPEECH_MODELS=%s (from the caller)" % os.environ["RICHOS_GUI_SPEECH_MODELS"]
    models = runtime_cache.gui_speech_models()
    if models is None:
        if refuse and any(needs_speech_models(it) for it in items):
            raise SystemExit("proof-run: NOT RUN: " + runtime_cache.speech_models_refusal())
        return ("RICHOS_GUI_SPEECH_MODELS is unset and %s are not all on this Mac; gui-boot will refuse. %s"
                % (", ".join(str(m) for m in runtime_cache.GUI_SPEECH_MODELS), runtime_cache.speech_models_refusal()))
    for it in items:
        it.env["RICHOS_GUI_SPEECH_MODELS"] = models
    return "RICHOS_GUI_SPEECH_MODELS=%s (the nightly's)" % models


def plan(lines, args, logdir, hist):
    items, units, cargo = [], [], []
    for n, line in enumerate(lines, 1):
        s = line.strip()
        m = re.match(r"^cd (\S+) && (.+)$", s)
        if not m:
            raise SystemExit("proof-run: cannot read line %d of the selection: %r" % (n, line))
        cwd, argv = os.path.join(ROOT, m.group(1)), shlex.split(m.group(2))
        if argv[:2] == ["bash", "scripts/ci-shard.sh"] and "--only-units" in argv:
            units.extend(argv[argv.index("--only-units") + 1].split(","))
        elif argv and argv[0] == "scripts/run-tests.sh" and "--only" in argv:
            flags = [a for i, a in enumerate(argv[1:], 1) if a != "--only" and argv[i - 1] != "--only"]
            for i, a in enumerate(argv):
                if a == "--only":
                    suite = argv[i + 1]
                    path = os.path.join(cwd, "scripts", suite)
                    lane = "gradle" if uses_gradle(path) else None
                    if os.environ.get("RICHOS_GUI_HOST") and is_host_screen(path):
                        lane = "guest"
                    label = suite[:-len(".test.sh")] if suite.endswith(".test.sh") else suite
                    # Its per-suite states (passed, notrun, gap, skipped), read when it ends: exit 0
                    # alone cannot tell a suite that passed from one that never ran (not_run()).
                    # Resolved, so the saved plan names it $RUN/... and identities stay stable.
                    report = os.path.join(os.path.realpath(logdir), "results-out", slug(label) + ".json")
                    items.append(Item(label, cwd, ["scripts/run-tests.sh", *flags, "--results-out", report,
                                                   "--only", suite], lane, default_weight(label, hist)))
        elif argv[:2] == ["cargo", "test"]:
            cargo.append((cwd, argv))
        else:
            first = argv[0]
            if first in ("bash", "node", "python3"):
                first = next((a for a in argv[1:] if not a.startswith("-")), first)
            base = os.path.basename(first)
            label = re.sub(r"\.test\.sh$|\.sh$", "", base)
            if (base.endswith(".sh") and not base.endswith(".test.sh")
                    and os.path.isfile(os.path.join(cwd, "scripts", label + ".test.sh"))):
                # A script named like a run-tests suite is not that suite. The land's `lint.sh
                # --changed` (about a second) was "lint", the label of lint.test.sh (`lint.sh
                # --all`, about 100 s), and its seconds pulled the suite's median to 1.7 s, so
                # the commit gate ran lint.test.sh as a fast check under a 120 s cap and refused
                # every richos/app/ui commit (2026-10-15). Its whole name and arguments label it.
                label = " ".join([base] + argv[argv.index(first) + 1:])
            if argv[:2] == ["node", "--test"]:
                label = "web-app"
            elif "testvm/test/run-tests.sh" in m.group(2):
                label = "testvm"
            elif base == "run-tests.sh" and "/test/" in first:
                # Every `<dir>/test/run-tests.sh` would otherwise be labeled "run-tests", the same
                # as the `--only run-tests.test.sh` suite: two checks under one label share one
                # identity, so the second's fingerprint made the first "changed during the check"
                # (INVALID, land of zach-sonnet-vmvalid1, 2026-09-30).
                label = os.path.basename(first.split("/test/")[0])
            items.append(Item(label, cwd, argv, None, default_weight(label, hist)))
    # cargo: one target and filter is one check however many changed files selected it, and a
    # filter a shorter filter on the same target already matches is dropped. proof-for.sh prints
    # `cargo test --bin richos-tauri` once for src-tauri/src/main.rs and once more for
    # src-tauri/Cargo.toml (its rows differ by kind); both were planned, and the saved plan held
    # the check twice (attempt-keq_bkth; the merge of cc/echo-opus-out3b crashed reusing it,
    # 2026-10-05). Only cargo is collapsed: any other line is a check of its own as printed.
    keyed, seen = [], set()
    for cwd, argv in cargo:
        if (cwd, tuple(argv)) not in seen:
            seen.add((cwd, tuple(argv)))
            keyed.append((cwd, argv, cargo_key(argv)))
    for cwd, argv, key in keyed:
        if key is not None and key[1] and any(
                k is not None and k != key and c == cwd and k[0] == key[0] and k[1] and k[1] in key[1]
                for c, _a, k in keyed):
            continue
        # Labeled by its target AND filter: the filter alone named `-p richos-voice --lib
        # dictation::` and `--bin richos-tauri dictation::` both "cargo dictation::", one
        # obligation twice, and the evidence record refused the plan (merge of cc/echo-opus-dict1,
        # 2026-10-08). A module name is unique only inside its crate; the target names the crate.
        label = "cargo " + " ".join(argv[2:])
        items.append(Item(label, cwd, argv, "cargo", default_weight("cargo", hist)))
    # engine: one units file, packed by the engine's planner, then the coverage proof
    if units:
        engine = os.path.join(ROOT, "richos/engine")
        ufile = os.path.join(logdir, "engine-units.txt")
        with open(ufile, "w") as fh:
            fh.write("\n".join(sorted(set(units))) + "\n")
        info = subprocess.run(["bash", "scripts/ci-units.sh", "units", "--units-file", ufile], cwd=engine,
                              capture_output=True, text=True)
        if info.returncode != 0:
            raise SystemExit("proof-run: ci-units.sh could not read the engine units:\n" + info.stderr)
        weight = {}
        for row in info.stdout.splitlines():
            f = row.split("\t")
            if len(f) >= 4:
                weight[f[0]] = float(f[3] or 0)
        if FOURTEEN_UNIT in weight and os.environ.get("RICHOS_FOURTEEN_MUTANTS") != "1":
            weight[FOURTEEN_UNIT] = FOURTEEN_WITHOUT_PASS
        # A unit this checkout has measured is planned at its measured median, not at the dated
        # table's row (MEDIAN_SAMPLES above): in the lanes ci-units.sh packs and in the start order.
        measured = {unit: hist["engine " + unit] for unit in weight if "engine " + unit in hist}
        weight.update(measured)
        measured_file = os.path.join(logdir, "engine-measured-weights.tsv")
        with open(measured_file, "w") as fh:
            fh.write("".join("%s\t%s\n" % (unit, measured[unit]) for unit in sorted(measured)))
        packer_env = {**os.environ, "CI_UNIT_WEIGHTS_MEASURED": measured_file}
        # Under --cap a unit planned past the cap is never started (leave_over_cap), so it takes
        # no shard: packed with the rest it would leave its lane idle and the others fuller.
        cap = getattr(args, "cap", None)
        over = sorted(u for u, w in weight.items() if cap and w > cap)
        for unit in over:
            items.append(Item("engine " + unit, engine, ["bash", "scripts/ci-shard.sh", "--only-units", unit],
                              None, weight.pop(unit)))
        if over:
            with open(ufile, "w") as fh:
                fh.write("".join(u + "\n" for u in sorted(weight)))
    if units and weight:
        # As many shards as allowed: the packer is longest-first, so a unit heavier than all the
        # rest together gets a shard of its own and the others spread over the remainder. A shard
        # count derived from the weights would trust lib/ci-unit-weights.tsv further than it can
        # be trusted: it is dated data, and a stale heavy row would idle the other shards.
        k = max(1, min(args.engine_shards, len(weight)))
        packed = subprocess.run(["bash", "scripts/ci-units.sh", "shards", str(k), "--units-file", ufile],
                                cwd=engine, capture_output=True, text=True, env=packer_env)
        if packed.returncode != 0 or not packed.stdout.strip():
            raise SystemExit("proof-run: engine shard planning failed: " + packed.stderr)
        per = {}
        for row in packed.stdout.splitlines():
            i, u = row.split("\t", 1)
            per.setdefault(i, []).append(u)
        receipts = os.path.join(logdir, "engine-receipts")
        os.makedirs(receipts, exist_ok=True)
        shard_labels = []
        for i in sorted(per, key=int):
            for unit in per[i]:
                label = "engine " + unit
                shard_labels.append(label)
                receipt_name = hashlib.sha256(unit.encode()).hexdigest() + ".jsonl"
                items.append(Item(label, engine,
                                  ["bash", "scripts/ci-shard.sh", "--only-units", unit,
                                   "--receipt", os.path.join(receipts, receipt_name)] +
                                  (["--fail-fast"] if getattr(args, "fail_fast", False) else []),
                                  "engine-shard-" + i, weight.get(unit, 60.0)))
                items[-1].notes.append("shard %s/%d; exact unit admission" % (i, k))
        items.append(Item("engine receipts", engine,
                          ["bash", "scripts/ci-shard.sh", "--verify-receipts", receipts, "--units-file", ufile],
                          None, 1.0, after=shard_labels))
    return place(items, args)


def place(items, args):
    """What plan() sets on an item beyond its saved row (encode_item), so an item decoded from a
    saved plan (author_coverage) carries what the run's own item carried."""
    for item in items:
        if item.label in {"native-ios-app", "native-ios-share", "native-ios-ui"}:
            item.lane = "ios-simulator"
            # Queue inside this gate before launch; other gates still use the pool
            # lease guard. Its finite wait fits this check's existing execution ceiling.
            if not os.environ.get("RICHOS_IOS_POOL_WAIT"):
                item.env["RICHOS_IOS_POOL_WAIT"] = str(deadline_for(item, args))
    return items


# ---------------------------------------------------------------------------------------
# running
# ---------------------------------------------------------------------------------------
def stamp():
    return datetime.datetime.now().strftime("%H:%M:%S")


def slug(label):
    return re.sub(r"[^A-Za-z0-9._-]+", "-", label).strip("-")[:60]


# ---------------------------------------------------------------------------------------
# the nightly's conditions (see A CHECK MEETS THE NIGHTLY'S CONDITIONS above)
# ---------------------------------------------------------------------------------------
# Set by main() from --without-nightly-conditions; a caller driving plan()/run() directly
# (the tests) gets the conditions, as the merge gate does.
NIGHTLY_CONDITIONS = True
_NIGHTLY = {}


def nightly_local():
    """nightly-local.py, loaded once: the one place the build's gate environment is written."""
    if "module" not in _NIGHTLY:
        spec = importlib.util.spec_from_file_location("proof_run_nightly_local",
                                                      os.path.join(HERE, "nightly-local.py"))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        _NIGHTLY["module"] = module
    return _NIGHTLY["module"]


def desktop_suites():
    """The suites the desktop nightly's script-suites gate runs, as run-tests.sh itself selects
    them (`--for desktop --list`, which reads phone-app-suites.tsv and starts nothing)."""
    if "desktop" not in _NIGHTLY:
        try:
            listed = subprocess.run(["bash", os.path.join(HERE, "run-tests.sh"), "--for", "desktop", "--list"],
                                    cwd=APP, capture_output=True, text=True, timeout=120,
                                    stdin=subprocess.DEVNULL)
        except (OSError, subprocess.SubprocessError) as exc:
            raise SystemExit("proof-run: cannot read which suites the nightly runs: %s" % exc)
        if listed.returncode != 0 or not listed.stdout.strip():
            raise SystemExit("proof-run: cannot read which suites the nightly runs: `run-tests.sh --for "
                             "desktop --list` exited %d: %s" % (listed.returncode, listed.stderr.strip()[-400:]))
        _NIGHTLY["desktop"] = frozenset(line.strip() for line in listed.stdout.splitlines() if line.strip())
    return _NIGHTLY["desktop"]


def nightly_gate_of(item):
    """The nightly gate that runs this check's suite, or None (a phone-app suite, cargo, an
    engine unit, anything run from outside this checkout's app)."""
    if not NIGHTLY_CONDITIONS or hasattr(item, "private_environment"):
        return None
    argv = item.argv
    if argv and argv[0] == "scripts/run-tests.sh" and "--only" in argv:
        if os.path.realpath(item.cwd) != os.path.realpath(APP):
            return None
        suites = [argv[i + 1] for i, a in enumerate(argv[:-1]) if a == "--only"]
        if suites and all(suite in desktop_suites() for suite in suites):
            return "gates/script-suites"
        return None
    if ui_suite_file(item):
        return nightly_local().UI_SUITE_GATE
    return None


def gate_conditions(gate, environ=None):
    """(conditions, missing) for `gate`, from nightly-local.py gate_conditions(), once per run
    (and once per other caller environment, `environ`: execution_environment)."""
    key = "gate " + gate + ("" if environ is None else " " + proof_evidence.digest(sorted(environ.items())))
    if key not in _NIGHTLY:
        try:
            _NIGHTLY[key] = nightly_local().gate_conditions(gate, environ)
        except (OSError, ValueError) as exc:
            raise SystemExit("proof-run: cannot derive the nightly's conditions for %s: %s" % (gate, exc))
    return _NIGHTLY[key]


def nightly_conditions(item, caller=None):
    gate = nightly_gate_of(item)
    return gate_conditions(gate, caller)[0] if gate else {}


def conditions_line(items):
    """The plan's line saying which checks meet which nightly gate's conditions."""
    if not NIGHTLY_CONDITIONS:
        return "nightly conditions: OFF (--without-nightly-conditions): every check runs in the caller's environment"
    gates = {}
    for it in items:
        gate = nightly_gate_of(it)
        if gate:
            gates.setdefault(gate, []).append(it.label)
    if not gates:
        return "nightly conditions: no check here runs a suite a nightly gate runs"
    parts = []
    for gate in sorted(gates):
        conditions, missing = gate_conditions(gate)
        part = "%d check(s) as %s runs them (TMPDIR=%s, %d variable(s) the build sets)" % (
            len(gates[gate]), gate, conditions.get("TMPDIR", "?"), len(conditions))
        if missing:
            part += "; NOT reproduced here: " + ", ".join("%s (%s)" % kv for kv in sorted(missing.items()))
        parts.append(part)
    return "nightly conditions: " + "; ".join(parts)


def canonical_path(value):
    """The PATH every check gets, whatever shell started the run: the checkout's own directories
    as the caller placed them, then the runner's tool directories in a fixed order, then every
    other entry in the caller's order, each once.

    2026-10-22, the land of cc/zach-opus-holds3 at e8e06b8e1: the author's run on that tip, on
    the same content, had passed 94 of 95, and the gate reused none of its whole-checkout passes,
    "made in another checkout, where it no longer stands in its own checkout (environment.PATH)".
    The two PATHs held the same 21 entries; the agent's shell had ~/.cargo/bin second and the
    lead's had it last (the runner only appended a missing one). A check's PATH, and so its
    identity, depended on which shell started the run."""
    root = os.path.realpath(ROOT)
    entries = [p for p in value.split(os.pathsep) if p]
    own = [p for p in entries if os.path.realpath(p) == root or os.path.realpath(p).startswith(root + os.sep)]
    tools = [os.path.join(os.path.expanduser("~"), ".cargo", "bin"), "/opt/homebrew/bin", "/usr/local/bin"]
    out = []
    for p in own + tools + [p for p in entries if p not in tools]:
        if p not in out:
            out.append(p)
    return os.pathsep.join(out)


# THE APP'S SESSION IS NEVER A CHECK'S ENVIRONMENT (2026-10-28). A worker shell the RichOS app
# starts carries the app's session: the installed engine, its live registry and hold records,
# its conversation and its coordination checkout. The engine units already run without it
# (ci-shard.sh through record-canary.sh rc_sandbox), but every other check (a run-tests.sh suite,
# a script, a UI suite) was handed the caller's whole environment, so a check that touches the
# registry or the hold records through its default wrote the app's live ones: record-canary.test.sh
# left agents/wait-regression.json in the live registry from a proof run in an app shell, and a
# unit in the same run failed only there. CI and the nightly start under `env -i` and have none of
# these names, so removing them is CI's shape; a check that tests app behaviour sets the names it
# needs itself. The list is record-canary.sh's _rc_roads_back (a test keeps the two equal) MINUS
# THE HOLD'S TWO TAGS: agent_hold.py finds an agent's running commands by RICHOS_AGENT_OWNER and
# RICHOS_AGENT_SESSION in their environment, so a check that lost them could not be frozen by a
# pause. Neither is a road to a live record: they name which agent a process belongs to.
HOLD_TAGS = frozenset(("RICHOS_AGENT_OWNER", "RICHOS_AGENT_SESSION"))
APP_SESSION_NAMES = frozenset((
    "RICHOS_ENGINE_ROOT", "RICHOS_ENGINE_DIR", "RICHOS_ENTITY_ROOT", "RICHOS_SA_ENTITY_ROOT",
    "RICHOS_SA_TEAMS_DIR", "RICHOS_WORKSPACES_DIR", "RICHOS_AGENT_HOLD_DIR", "STALL_WATCH_STATE_DIR",
    "RICHOS_SESSION_ID", "RICHOS_SESSION_PID",
    "RICHOS_SPAWN_HOOK_SOURCES", "RICHOS_CLAUDE_ACCOUNT", "CLAUDE_PROJECT_DIR",
))
APP_SESSION_PREFIX = "RICHOS_APP_"


def without_app_session(environment):
    return {k: v for k, v in environment.items() if k not in APP_SESSION_NAMES and not k.startswith(APP_SESSION_PREFIX)}


def execution_environment(item, caller=None):
    """The environment `item` runs in, from this runner's caller environment, or from `caller`
    (another run's, recorded_environment) to read the check as that run started it."""
    if hasattr(item, "private_environment"):
        return {**item.private_environment, **item.env}
    # Verification must not rewrite bytecode inside its own declared inputs. The nightly's
    # conditions sit between the caller's environment and the check's own values.
    env = {**without_app_session(os.environ if caller is None else caller), **nightly_conditions(item, caller),
           **item.env, "PYTHONDONTWRITEBYTECODE": "1"}
    env["PATH"] = canonical_path(env.get("PATH", ""))
    if item.argv[:1] == ["cargo"]:
        import cargo_identity
        return cargo_identity.enable_environment(env)
    return env


def recipe_of(item):
    """The item's reviewed recipe. A family recipe (the UI suites, proof_evidence.UI_FAMILY) holds
    only for the exact invocation its family qualifies, which only this caller can see."""
    recipe = proof_evidence.contract_for(ROOT, item.label)
    if recipe.get("family"):
        recipe = proof_evidence.family_invocation(ROOT, recipe, item.cwd, item.argv)
    return recipe


def input_identity(item, args, logdir, snapshot=None, caller=None):
    environment = execution_environment(item, caller)
    result = proof_evidence.recipe_identity(ROOT, recipe_of(item), environment, snapshot)
    native_toolchain_failed = False
    if item.label == "native-ios-ui" and not result.get("fresh"):
        # /usr/bin/xcodebuild is a shim: its bytes alone do not identify the selected
        # Xcode, SDK or simulator runtime. Read their identities without booting a device.
        import ios_ui_shards
        commands = (("xcode-select", "-p"), ("xcodebuild", "-version"),
                    ("xcodegen", "--version"), ("swiftc", "--version"),
                    ("xcrun", "--find", "swiftc"),
                    ("xcrun", "--sdk", "iphonesimulator", "--show-sdk-build-version"),
                    ("xcrun", "simctl", "list", "runtimes", "available", "-j"))
        try:
            toolchain = [
                subprocess.check_output(command, env=environment, text=True,
                                        stderr=subprocess.STDOUT, timeout=15).strip()
                for command in commands]
            # simctl updates lastUsage whenever a runtime is used. Bind its build and
            # configuration, not that timestamp, and canonicalize JSON object ordering.
            runtimes = json.loads(toolchain[-1])
            for runtime in runtimes["runtimes"]:
                runtime.pop("lastUsage", None)
            toolchain[-1] = runtimes
            result["native_toolchain"] = proof_evidence.digest(toolchain)
            result["native_environment"] = proof_evidence.digest(ios_ui_shards.retry_settings(
                {**environment, "RICHOS_TEST_SCOPE": "full"}))
        except (OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
            result = {"fresh": "native toolchain identity unavailable: " + str(exc)}
            native_toolchain_failed = True
    if (result.get("fresh") and not native_toolchain_failed
            and item.label not in proof_evidence.VERIFIES_OWN_INPUTS and item.argv):
        # No reviewed contract: keyed by the whole checkout's content, so a retry of the same
        # tree (--resume, a refused land tried again, the push after it) keeps what passed
        # (proof_evidence.WHOLE_CHECKOUT). Engine units too (2026-10-02): they were `fresh`, so
        # a gate round that ran out of time lost every unit it had passed. A reused unit's
        # receipt is copied with its provenance and the coverage proof re-reads it. Only
        # `engine receipts`, the coverage proof itself, always runs.
        try:
            result = proof_evidence.checkout_identity(ROOT, item.argv, environment, snapshot)
        except (OSError, ValueError, subprocess.SubprocessError) as exc:
            result = {"fresh": "%s; whole-checkout identity unavailable: %s" % (result["fresh"], exc)}
    result["command"] = proof_evidence.command_identity(item, ROOT, logdir)
    result["settings"] = {key: getattr(args, key, None) for key in IDENTITY_SETTINGS}
    return result


def seat_reader(item, snapshot):
    """How `item` reads in another checkout of this repository, so a pass made there (the
    author's handover run, offered to the merge gate in the main checkout) is reused when it still
    stands there on the same content (proof_evidence.seat_gap). 2026-10-21: the gate executed all
    23 checks of a branch whose author run the preflight had just read as proving it."""
    return proof_evidence.seat_reader(ROOT, item.argv, execution_environment(item), snapshot)


def recorded_environment(environ=None):
    """{"values", "withheld"}: the caller environment this run reads every check's identity from
    (execution_environment), as its saved plan records it (proof_evidence THE RECORDED
    ENVIRONMENT). Git's hook variables and the per-run names are no one else's to replay; a
    credential's value is never written (rerun_credential), only its name."""
    values, withheld = {}, []
    for name, value in sorted((os.environ if environ is None else environ).items()):
        if name.startswith("GIT_") or name in proof_evidence.RUN_SCOPED_ENVIRONMENT:
            continue
        if rerun_credential(name, value):
            withheld.append(name)
        else:
            values[name] = value
    return {"values": values, "withheld": withheld}


# What the runner hands every check itself when its caller has none (supply_runtime,
# supply_speech_models). They are in each check's item.env, which a saved plan does not carry, so
# the coverage verifier, rebuilding the items, finds them only in its own environment: that of
# the `engine receipts` check, which the runner started with them.
RUNNER_SUPPLIED = ("RICHOS_RUNTIME_DIR", "RICHOS_GUI_SPEECH_MODELS")


def as_caller(recorded, root, environ=None):
    """The caller environment a run in checkout `root` recorded (recorded_environment), named for
    this checkout, with this process's own Git, per-run and withheld credential values kept, and
    its own mutation switches: they are what this run asks its checks to do, not what a shell
    held, so a pass made with them off never stands for a run with them on (stands_in). So are
    the values this run supplies its checks (RUNNER_SUPPLIED), where this process holds them.

    2026-10-23, runs 20261023T200918Z-bq9lv1qe and 20261023T213406Z-qfskgufe: reuse() read a
    pass under the recorded environment with the runner's supplied values (item.env), and the
    verifier, whose items carry none, without them; so `engine receipts` refused as
    "unqualified historical receipt" every engine unit reuse() had accepted that way."""
    values = {k: v for k, v in recorded["values"].items() if k not in proof_evidence.MUTATION_SWITCHES}
    if os.path.realpath(root) != os.path.realpath(ROOT):
        values = proof_evidence.placed(values, root, ROOT)
    keep = set(recorded.get("withheld") or ()) | set(proof_evidence.MUTATION_SWITCHES) | set(RUNNER_SUPPLIED)
    own = {k: v for k, v in (os.environ if environ is None else environ).items()
           if k in keep or k.startswith("GIT_") or k in proof_evidence.RUN_SCOPED_ENVIRONMENT}
    return {**values, **own}


def identity_as_recorded(item, recorded, root, snapshot, args, logdir):
    """(identity, read_seat) of `item` in this checkout, read under the caller environment the
    run in checkout `root` recorded (as_caller): what reuse() judges a pass of that run by when
    it does not stand under this run's own environment. A private-profile check's environment
    is built from that one again, on a copy, as run_main builds it from this run's."""
    caller = as_caller(recorded, root)
    probe = copy.copy(item)
    probe.env = dict(item.env)
    if hasattr(probe, "private_environment"):
        del probe.private_environment
        proof_evidence.prepare_environment(probe, ROOT, logdir, execution_environment(probe, caller), create=False)
    environment = execution_environment(probe, caller)
    return (input_identity(probe, args, logdir, snapshot, caller),
            proof_evidence.seat_reader(ROOT, probe.argv, environment, snapshot))


# The runner settings that are part of a check's identity: those that change how it runs (its
# worker budget, its engine shards, the CPU line it ran under, its deadline). Not the bounds on
# how long it may WAIT to start (--admission-wait, --slot-wait, --engine-slot-wait): a check
# admitted after 10 s or after 800 s runs the same commands on the same inputs. Until
# 2026-10-02 they were in it, and the merge gate's retry (900 s) reused nothing its first run
# (893 s) had passed: 214 of 214 identities differed in `settings` alone.
IDENTITY_SETTINGS = ("capacity", "engine_shards", "max_cpu", "budget", "deadline", "fail_fast")


def describe_input_change(item, baseline, items, limit=5):
    """'; changed: <paths>; checks started by then: <labels>' for an invalidation note, or ''.

    The paths are what differs between the plan-time fingerprint (`baseline`, the snapshot the
    plan's identities were read through) and the tree now; the labels are every check of this
    run that had started, the check itself included, because any of them could be the writer.
    Diagnosis only: it never changes a verdict, and a failure to explain explains nothing."""
    try:
        recipe = recipe_of(item)
        if recipe.get("fresh"):
            return ""
        root = Path(ROOT).resolve()
        changed = []
        if recipe.get("family"):
            # A family binds content, not disk state: compare that, one fresh listing.
            now = proof_evidence.InputSnapshot()
            changed = [rel for rel in recipe["paths"] if (str(root), rel) in baseline.contents
                       and baseline.contents[(str(root), rel)] != now.content(root, rel)]
        for rel in ([] if recipe.get("family") else recipe.get("subset", {}).get("paths", recipe["paths"])):
            before = baseline.paths.get(str((root / rel).absolute()))
            if before is not None:
                changed += proof_evidence.identity_differences(before, proof_evidence.path_identity(root / rel), rel)
        started = sorted(it.label for it in items if it.started is not None)
        where = (", ".join(changed[:limit]) + (" (+%d more)" % (len(changed) - limit) if len(changed) > limit else "")
                 if changed else "nothing under its declared paths (a tool, the environment or its Git inputs)")
        return "; changed: %s; checks started by then: %s" % (where, ", ".join(started) or "none")
    except Exception:  # noqa: BLE001 — an explanation never breaks the verdict it explains
        return ""


def invalid_reason(item):
    """Why a check is INVALID, for summary.json: the reason its outcome row carries (outcomes.json
    `invalid`, which names the changed identity keys), or None when the check is not invalid.
    Before 2026-10-29 the summary said `invalid` and nothing else; the reason was only in the
    run's console output and outcomes.json."""
    if item.state != "invalid":
        return None
    evidence = getattr(item, "evidence", None)
    row = (evidence.results.get(item.label) if evidence is not None else None) or {}
    return row.get("invalid") or (item.notes[-1] if item.notes else None)


def reserve_item(item, n, args, logdir):
    if not getattr(args, 'managed_verification', False):
        return
    evidence = getattr(item, 'evidence', None)
    identity = evidence.identities[item.label] if evidence else {'fresh': 'unqualified direct command'}
    inputs = {key: value for key, value in identity.items() if key != 'settings'}
    if inputs.get('fresh'):
        inputs['source'] = evidence.source if evidence else source_identity()
    key = proof_evidence.digest({'check': item.label, 'inputs': inputs,
        'command': proof_evidence.command_identity(item, ROOT, logdir)})
    directory = os.path.join(logdir, 'attempts', '%02d-%s' % (n, slug(item.label)))
    os.makedirs(directory, exist_ok=True)
    native = proc_tree.identity(os.getpid())
    context = {'protocol': cpu_guard.VERIFICATION_PROTOCOL, 'input_key': key,
        'label': item.label, 'priority': 'integration' if (engine_pass.is_main_checkout(ROOT) and
                                                          not getattr(args, 'priority_turn_over', False)) else 'background',
        'result': os.path.join(directory, 'supervision.json'),
        'seed': {'pid': os.getpid(), 'generation': native}}
    if not inputs.get('fresh'):
        comparison = {name: inputs[name] for name in ('tools', 'profile', 'environment', 'external', 'platform')}
        comparison.update(command=proof_evidence.command_identity(item, ROOT, logdir),
                          settings=identity.get('settings', {}), check=item.label, cpu_count=os.cpu_count())
        context['cost_comparison'] = {'key': proof_evidence.digest(comparison),
                                      'identity': comparison,
                                      'predicted_seconds': item.weight, 'check': item.label}
        context['input_evidence'] = {'source': evidence.source, 'input': identity}
        previous_cost = cpu_guard.previous_verification_cost(context['cost_comparison']['key'])
        if previous_cost and previous_cost['status'] in ('growth', 'uncertain-growth'):
            note = 'previous execution needs cost review (' + previous_cost['status'] + '); investigate before repetition: ' + previous_cost['history']
            if note not in item.notes:
                item.notes.append(note)
                print('proof-run: COST REVIEW ' + item.label + ': ' + note, flush=True)
    lease = cpu_guard.reserve_verification(context, os.getpid(), native)
    item.reservation = lease
    context['reservation'] = {'fd': lease[0], 'id': Path(lease[2]).stem}
    item.verification_context = os.path.join(directory, 'context.json')
    item.verification_result = context['result']
    proof_evidence.atomic(item.verification_context, context)


def finish_attempt(item):
    """A controller intervention cannot become a behavioral failure or a pass."""
    path = getattr(item, 'verification_result', None)
    if not path:
        return False
    record = cpu_guard.read_json(path)
    if not record:
        item.state, item.rc = 'infrastructure-failed', 125
        item.notes.append('managed verification result is missing')
        record = {}
    elif record.get('cleanup') != 'complete':
        item.state, item.rc = 'cleanup-failed', 125
        item.notes.append('owned cleanup remains unresolved: ' + path)
    status = record.get('status')
    if item.state in ('infrastructure-failed', 'cleanup-failed'):
        status = None
    elif status in ('contained', 'resource-envelope-exceeded'):
        item.state, item.rc = status, 125
    elif status != 'completed':
        if item.state not in ('timed-out', 'cancelled'):
            item.state = 'not-admitted' if item.rc == 75 else 'infrastructure-failed'
            item.rc = 75 if item.rc == 75 else 125
    attempt = {'state': item.state, 'exit': item.rc, 'seconds': item.seconds,
               'queue_seconds': max(0, item.admission_wait - sum(a['queue_seconds'] for a in item.attempts)),
               'cpu_seconds': record.get('reaped_cpu_seconds'),
               'log': item.log, 'supervision': path}
    item.attempts.append(attempt)
    cost = record.get('cost')
    if cost:
        attempt['cost'] = cost
        if cost.get('status') == 'growth':
            item.notes.append('material verification cost growth: ' + json.dumps(cost['growth'], sort_keys=True)
                              + '; compare load and investigate: ' + cost['history'])
            print('proof-run: COST GROWTH ' + item.label + ': ' + item.notes[-1], flush=True)
        elif cost.get('status') == 'uncertain-growth':
            item.notes.append('elapsed cost increased but load is not comparable; investigate: ' + cost['history'])
            print('proof-run: COST REVIEW ' + item.label + ': ' + item.notes[-1], flush=True)
    if status not in ('contained', 'resource-envelope-exceeded'):
        return False
    recovery = cpu_guard.verification_recovery(record['input_key'])
    kind = 'containment' if status == 'contained' else 'resource'
    events = recovery.get(kind, [])
    if not events or record.get('budget_used') != len(events) or events[-1].get('result') != path:
        item.state, item.rc = 'infrastructure-failed', 125
        attempt.update(state=item.state, exit=item.rc)
        item.notes.append('controller cancellation has no matching durable budget event: ' + path)
        return False
    # Preserve the interrupted receipt before its one-unit retry writes a new one.
    receipt = proof_evidence.receipt_path(item)
    if receipt and receipt.exists():
        target = Path(path).parent / 'interrupted-receipt.jsonl'
        shutil.move(receipt, target)
        attempt['receipt'] = str(target)
    if recovery['blocked']:
        try:
            policy = cpu_guard.request_verification_recovery(record['input_key'], path)
        except (BlockingIOError, cpu_guard.RecoveryExhausted) as exc:
            item.state = 'scheduler-starvation' if status == 'contained' else status
            item.notes.append(str(exc) + '; evidence: ' + path)
            return False
        item.notes.append('recorded exclusive calibration after ' + policy['reason'] + '; evidence: ' + path)
    else:
        item.notes.append('pressure-contained attempt preserved; retry after controller recovery: ' + path)
    return True


# A REFUSED CHECK CARRIES THE COMMAND THAT RERUNS IT AS THE GATE RAN IT (2026-10-05).
# The merge gate ran `git merge` with LC_ALL=C (workspaces.py _git_env, until _land_env passed the
# caller's locale on, 2026-10-20), its hooks inherited it, and so did every check; a shell has
# en_GB.UTF-8. land-completeness.test.sh failed only in the gate and passed every hand rerun by
# silently skipping its check (fixed in 3c0383fba), because nobody
# could rerun it the way the gate had. So each check's exact environment, directory and command
# are written to <run>/rerun/<nn>-<check>.sh when it starts (`env -i`, every variable it got), and
# a check that did not pass is printed with that file and the locale and time zone it ran under.
# Left out: what ties a process to THIS run (worker tokens and their locks, the proof-run slot's
# descriptors, the verification controller's state, this run's evidence paths), which a rerun by
# hand must not borrow and does not need.
RERUN_RUN_SCOPED = ("RICHOS_WORKER_TOKENS", "RICHOS_WORKER_TOKENS_TOOL", "RICHOS_WORKER_TOKENS_RESERVED",
                    "RICHOS_MACHINE_WORKERS", "RICHOS_WORKER_SLOT_HELD", "RICHOS_WORKER_BORROW_LOCK",
                    "RICHOS_CPU_GUARD_STATE", "RICHOS_VERIFICATION_RUNNER_WAIT", "RICHOS_VERIFICATION_CONTAMINATION",
                    "RICHOS_TEST_RESULTS_ROOT", "RICHOS_UI_TESTS_LEDGER", "RICHOS_TEST_DEVICE_RUN_ID",
                    "RICHOS_PROOF_RUN_SLOT_HELD", "RICHOS_PROOF_RUN_SLOT_FDS")
RERUN_SHOWN = ("LC_ALL", "LANG", "LC_CTYPE", "LC_COLLATE", "LC_MESSAGES", "TZ", "TMPDIR")

# NOR A CREDENTIAL (2026-10-05). The file is plain text under proof-runs/ and it wrote
# CLAUDE_CODE_MESSAGING_TOKEN, a live session token, into every rerun of every run. So no value
# of a variable that carries a credential or a per-session secret is ever written: by its name (a
# word of it says token, secret, password, passphrase, credential, cookie, API key, private key or
# key) or by its value (a URL with a password in it). The rule is the kind, not a list of names,
# so a credential nobody has met yet is left out by construction. Its NAME stays in the file, as
# ${NAME+"NAME=$NAME"}: the rerun takes it from the shell that runs the file when that shell has
# it, and otherwise runs without it; the file says which names those are.
RERUN_CREDENTIAL_WORDS = ("TOKEN", "SECRET", "PASSWORD", "PASSWD", "PASSPHRASE", "CREDENTIAL", "APIKEY",
                          "ACCESSKEY", "PRIVATEKEY", "COOKIE", "AUTHORIZATION", "BEARER")
RERUN_CREDENTIAL_PARTS = frozenset(("KEY", "KEYS", "PASS", "PW", "PAT", "PSK", "CREDS", "AUTH"))
RERUN_CREDENTIAL_SAFE = frozenset(("SSH_AUTH_SOCK",))   # a socket path, and git over ssh needs it
RERUN_URL_PASSWORD = re.compile(r"[A-Za-z][A-Za-z0-9+.-]*://[^/\s:@]*:[^/\s@]+@")
RERUN_SHELL_NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")


def rerun_credential(name, value):
    """True when a rerun file must not carry this variable's value."""
    if name in RERUN_CREDENTIAL_SAFE:
        return False
    upper = name.upper()
    squashed = re.sub(r"[^A-Z0-9]", "", upper)
    parts = set(re.split(r"[^A-Z0-9]+", re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name).upper()))
    return (any(word in squashed for word in RERUN_CREDENTIAL_WORDS) or bool(parts & RERUN_CREDENTIAL_PARTS)
            or bool(RERUN_URL_PASSWORD.search(value)))

# AND ON THE SAME TREE (2026-10-05). The file above did `cd <checkout>`: the merge gate runs in
# the main checkout while the merge is in progress, a refused merge is aborted, and that checkout
# is back on main. A front-door.js A1 failure (attempt-ioj6a1_a) "passed" its rerun on main and
# proved nothing. So the tree each checkout held when this run started is recorded once
# (lib/rerun_tree.py capture), and the file runs a copy of rerun_tree.py kept beside it, which
# runs the check in the checkout while it still holds that tree and otherwise in a scratch
# checkout of it. Where no tree can be recorded, the file says so and runs in the checkout.
RERUN_TREES = {}


def rerun_tree_of(cwd):
    """(capture dict or None, why not) for the checkout holding cwd, recorded once per run."""
    import rerun_tree  # noqa: E402  (lib/, already on sys.path; only a run that writes reruns needs it)
    try:
        root = subprocess.run(["git", "-C", cwd, "rev-parse", "--show-toplevel"], capture_output=True, text=True,
                              timeout=60, env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")})
    except (OSError, subprocess.SubprocessError) as exc:
        return None, str(exc)
    key = os.path.realpath(root.stdout.strip()) if root.returncode == 0 else ""
    if not key:
        return None, "%s is not in a git checkout" % cwd
    if key not in RERUN_TREES:
        try:
            RERUN_TREES[key] = (rerun_tree.capture(key), "")
        except (OSError, ValueError, subprocess.SubprocessError) as exc:
            RERUN_TREES[key] = (None, str(exc)[:300])
    return RERUN_TREES[key]


def write_rerun(item, n, logdir, env):
    """<logdir>/rerun/<nn>-<check>.sh: this check, as this run started it. None if it cannot be written."""
    scoped = set(RERUN_RUN_SCOPED) | set(SLOT.env() if SLOT else ()) | {getattr(proc_tree, "SCOPE_ENV", "")}
    kept = sorted((k, v) for k, v in env.items() if k not in scoped and "\n" not in v)
    withheld = [k for k, v in kept if rerun_credential(k, v)]
    kept = [(k, v) for k, v in kept if k not in withheld]
    path = os.path.join(os.path.realpath(logdir), "rerun", "%02d-%s.sh" % (n, slug(item.label)))
    tree, why = rerun_tree_of(item.cwd)
    rel = os.path.relpath(os.path.realpath(item.cwd), tree["root"]) if tree else ""
    if tree and (rel == ".." or rel.startswith(".." + os.sep)):
        tree, why = None, "%s is outside its checkout %s" % (item.cwd, tree["root"])
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        helper = os.path.join(os.path.dirname(path), "rerun_tree.py")
        if tree and not os.path.isfile(helper):
            shutil.copyfile(os.path.join(HERE, "lib", "rerun_tree.py"), helper + ".new")
            os.replace(helper + ".new", helper)
        with open(path, "w") as out:
            out.write("#!/bin/sh\n# %s, exactly as proof-run started it: the same tree, the same directory, the same "
                      "environment (env -i: nothing from the shell that runs this) and the same command.\n" % item.label)
            if withheld:
                out.write("# Credentials the gate had, never written here: %s. Each is passed on only when the shell\n"
                          "# that runs this file has it.\n" % " ".join(withheld))
            if tree:
                out.write("# The tree it ran on: %s, in %s (HEAD %s%s). Where that checkout no longer holds it (a refused\n"
                          "# merge is aborted, so the main checkout is back on main), it runs in a scratch checkout of it.\n" % (
                              tree["tree"], tree["root"], tree["head"] or "(none)",
                              ", merging %s" % tree["merge_head"] if tree["merge_head"] else ""))
                out.write("exec %s %s --root %s --tree %s --head %s --merge-head %s --cwd %s -- \\\n" % (
                    shlex.quote(sys.executable), shlex.quote(helper), shlex.quote(tree["root"]), tree["tree"],
                    shlex.quote(tree["head"]), shlex.quote(tree["merge_head"]), shlex.quote(rel)))
                out.write("  env -i \\\n")
            else:
                out.write("# The tree it ran on could not be recorded (%s): this runs in whatever %s holds now.\n" % (
                    why.replace("\n", " "), item.cwd))
                out.write("cd %s || exit 2\n" % shlex.quote(item.cwd))
                out.write("exec env -i \\\n")
            for key, value in kept:
                out.write("  %s \\\n" % shlex.quote("%s=%s" % (key, value)))
            for key in withheld:
                if RERUN_SHELL_NAME.match(key):
                    out.write('  ${%s+"%s=$%s"} \\\n' % (key, key, key))
            # Started through the host shield, as launch() starts it (ceo-decisions §65): a rerun
            # is the check as the gate ran it, stand-ins included. Written here only, never into
            # item.argv, so the check's input identity is unchanged. Without a shield the file
            # refuses to run rather than replaying the check unprotected (review
            # rv-20261011T141442Z-0009fe5a-a898 #5).
            if HOST_SHIELD and os.path.isfile(HOST_SHIELD):
                shielded = [sys.executable, "-S", os.path.realpath(HOST_SHIELD), "run", "--"]
            else:
                shielded = ["/bin/sh", "-c", "echo 'proof-run rerun: NO HOST SHIELD was found when this file was "
                            "written; refusing to run the check without the power, sleep, lock and display "
                            "stand-ins (ceo-decisions 65).' >&2; exit 2", "rerun"]
            out.write("  %s\n" % " ".join(shlex.quote(a) for a in [*shielded, *item.argv]))
        os.chmod(path, 0o755)
    except OSError:
        return None
    item.rerun_shown = " ".join("%s=%s" % (k, env.get(k, "(unset)")) for k in RERUN_SHOWN if k in env or k in ("LC_ALL", "TZ"))
    item.rerun_shown += " tree=%s" % (tree["tree"][:12] if tree else "(not recorded: the checkout as it is now)")
    return path


def launch(item, n, logdir, tokens_dir, reserved):
    item.log = os.path.join(logdir, "%02d-%s.log" % (n, slug(item.label)))
    # Its own per-test results folder, in this run's evidence: run-tests.sh gives each suite a
    # folder under it, and the suites copy their result files there the moment a test run ends,
    # before any other check can replace them (lib/test_results.py; 2026-09-25).
    item.results = os.path.join(logdir, "results", "%02d-%s" % (n, slug(item.label)))
    item.env["RICHOS_TEST_RESULTS_ROOT"] = item.results
    if "--results-out" in item.argv:
        os.makedirs(os.path.dirname(item.argv[item.argv.index("--results-out") + 1]), exist_ok=True)
    fh = open(item.log, "wb")
    fh.write(("$ cd %s && %s\n" % (os.path.relpath(item.cwd, ROOT), " ".join(shlex.quote(a) for a in item.argv))).encode())
    fh.flush()
    # Its own session, so nothing it does can signal this runner; how it is stopped is
    # proc_tree.kill_tree (its whole tree), never a group or a name.
    env = {**execution_environment(item), "RICHOS_WORKER_TOKENS": tokens_dir,
           "RICHOS_VERIFICATION_CHECKOUT": ROOT,
           "RICHOS_WORKER_TOKENS_TOOL": os.path.abspath(worker_tokens.__file__),
           "RICHOS_WORKER_TOKENS_RESERVED": str(reserved),
           "RICHOS_MACHINE_WORKERS": item.machine_tokens, "RICHOS_WORKER_SLOT_HELD": "1",
           "RICHOS_WORKER_BORROW_LOCK": item.token.path + ".child", **(SLOT.env() if SLOT else {}),
           # The merge gate, Rich's land runs and the nightly's engine pass run every check whole,
           # in any checkout: a runner started from a teammate workspace narrows by default
           # (engine/scripts/lib/workspace_scope.py, CEO 2026-10-01), never under this runner.
           # Set here, never in item.env, so it is not part of the check's input identity.
           "RICHOS_TEST_SCOPE": "full"}
    if getattr(item, 'verification_context', None):
        env['RICHOS_CPU_GUARD_STATE'] = str(cpu_guard.STATE)
    if os.environ.get(proc_tree.SCOPE_ENV):
        env[proc_tree.SCOPE_ENV] = os.environ[proc_tree.SCOPE_ENV]
    if item.engine_unit:
        env["RICHOS_VERIFICATION_UNIT"] = item.argv[item.argv.index("--only-units") + 1]
        env["RICHOS_VERIFICATION_RUNNER_WAIT"] = json.dumps(item.wait_times)
    if item.label == "engine receipts" and getattr(item, "evidence", None):
        env["RICHOS_PROOF_RUN"] = str(logdir)
    # A UI suite run directly writes the evidence ledger run.js gates on (ui_not_run). Set here,
    # never in item.env, so it is not part of the check's input identity: the path is per run.
    item.ui_ledger = None
    if ui_suite_file(item):
        item.ui_ledger = os.path.join(os.path.realpath(logdir), "ui-ledger", "%02d-%s.jsonl" % (n, slug(item.label)))
        os.makedirs(os.path.dirname(item.ui_ledger), exist_ok=True)
        if os.path.exists(item.ui_ledger):
            os.unlink(item.ui_ledger)    # an earlier attempt's records are not this attempt's
        env["RICHOS_UI_TESTS_LEDGER"] = item.ui_ledger
        # And its navigation-failure bundles go to the check's own results folder in this run's
        # evidence (PRD step 3, T07/R9): left to lib/navigation-evidence.js's default, they went
        # to the checkout's `.shots/`, which a land in a temporary checkout removes with it. The
        # same folder takes the suite's failing pictures (RICHOS_TEST_RESULTS_ROOT, group B).
        # Per run, so here and never in item.env: no part of any input identity.
        env["RICHOS_UI_NAV_EVIDENCE_DIR"] = os.path.join(item.results, "navigation-failures")
    # Python bytecode is never a check's output into the checkout (2026-09-30): a hook test that
    # ran guard-idle-land.py wrote hooks/__pycache__/guard-idle-land.cpython-314.pyc inside six
    # other checks' declared inputs, and all six became "invalid: execution inputs changed
    # during the check". Set here, like the ledger path below, so it is not part of any input
    # identity. A check that needs bytecode (a test of the bytecode cache itself) sets its own.
    env.setdefault("PYTHONDONTWRITEBYTECODE", "1")
    item.rerun = write_rerun(item, n, logdir, env)
    item.state, item.started = "running", time.monotonic()
    evidence = getattr(item, "evidence", None)
    if evidence:
        evidence.launched(item)  # HEAD only (Record.launched says why)
    try:
        # Without a shield the check is REFUSED and never started (a failed check, named), as
        # run-tests.sh refuses (zach-opus-hostshim3, d645b9a77): never run without the stand-ins.
        if not (HOST_SHIELD and os.path.isfile(HOST_SHIELD)):
            raise OSError("REFUSED: NO HOST SHIELD in this checkout and none exported (RICHOS_HOST_SHIELD_TOOL), "
                          "so this check cannot run behind the power, sleep, lock and display stand-ins "
                          "(ceo-decisions 65); it was not started")
        started = [sys.executable, "-S", HOST_SHIELD, "run", "--", *item.argv]
        command = proc_tree.command(started, verification=getattr(item, 'verification_context', None))
        if hasattr(item, "private_environment"):
            command[0] = "python3"  # The recipe's fingerprinted -s -S wrapper also covers supervision.
        begin_execution()
        item.proc = subprocess.Popen(command, cwd=item.cwd, stdout=fh, stderr=subprocess.STDOUT,
                                     stdin=subprocess.DEVNULL, start_new_session=True, env=env,
                                     pass_fds=(SLOT.fds if SLOT else ()) + tuple(item.token.fds) + tuple(getattr(item, "slot_fds", ()))
                                     + ((evidence.lease.fd,) if evidence else ())
                                     + ((item.reservation[0],) if item.reservation else ())
                                     + ((item.input_owner_fd,) if hasattr(item, "input_owner_fd") else ()))
    except OSError as exc:
        # A check that cannot even start is a FAILED check, named, never a crash of the run
        # that leaves every other check running with nobody to stop it.
        fh.write(("proof-run: could not start: %s\n" % exc).encode())
        item.proc = None
        item.notes.append("could not start: %s" % exc)
        if item.reservation:
            cpu_guard.write_json(item.verification_result, {'root_generation': proc_tree.identity(os.getpid()),
                'cleanup': 'complete', 'status': 'incomplete', 'exit': 127}, durable=True)
    finally:
        if item.reservation:
            os.close(item.reservation[0])  # The supervisor inherited the same kernel lease.
            item.reservation = None
    fh.close()


def reserved_tokens(capacity):
    """Tokens only the runner may take, so checks not yet started are never starved by nested
    workers (worker_tokens.py header): a quarter of the budget, at least one."""
    return max(1, capacity // 4)


def admitted(args, sampler):
    """One sample, CEO ruling §77's line and memory rule; (ok, sample). Never skips the CPU line."""
    s = sampler()
    return not reserve._refusal(s, args.max_cpu, 16, cpu_rule=True), s


def refusal_counts(running):
    """Whether a refused start spends the check's --admission-wait (2026-09-29, 2026-09-30).

    THE LIMIT IS ON THE MAC REFUSING, NEVER ON THIS RUN'S OWN QUEUE. Until 2026-09-29 the clock
    ran from the moment a check was ready, so time spent behind this run's own checks for a
    worker token, a lane or the ramp was charged to it, and a check that only waited behind its
    siblings was refused: several land logs of 2026-09-29 show checks NOT ADMITTED after
    ~1,830 s (richos-hq docs/audits/2026-09-29-hunt/part-2-codex.md, section 01).

    It counts every refusal (a host sample over CEO ruling §77's CPU line or its memory rule,
    the machine's verification pressure controller, the machine worker budget, integration
    priority, the measured resource envelope) while NOTHING of this run is running: then the
    wait cannot be behind this run's own work, and a Mac that keeps refusing still ends in NOT
    ADMITTED.

    It does not count any refusal while this run's own checks run. The full worker budget and
    envelope are this run's queue by construction; a CPU sample over the line or a closed
    pressure controller cannot be told apart from the load those running checks make, so it is
    this run's queue too (part 2 recheck, R01: until 2026-09-30 those two still counted, and a
    sibling that kept the Mac busy for --admission-wait got the check behind it refused while
    the sibling was still doing its bounded work). A busy lane, the ramp, dependencies, the
    engine slot and an identical owner have their own bounds. A check waiting behind its
    siblings therefore waits for as long as they run, which their deadlines already bound,
    and then gets its turn; if the Mac still refuses once they are done, that refusal counts."""
    return not running


class HostSamples:
    """Share a complete host measurement for at most one measurement interval.

    Monitor and admission use the same serialized reader. Memory and swap data
    remain part of every admission decision. A failed refresh invalidates the
    previous value; neither errors nor old samples permit execution.
    """
    def __init__(self, sampler, max_age):
        self.sampler, self.max_age = sampler, min(1.0, max_age)
        self.lock = threading.Lock()
        self.value = None
        self.measured = 0.0

    def __call__(self):
        with self.lock:
            if self.value is None or time.monotonic() - self.measured >= self.max_age:
                self.value = None
                value = self.sampler()
                self.measured = time.monotonic()
                self.value = value
            return dict(self.value)


class Monitor(threading.Thread):
    """The machine while the run is going: total CPU and the worker tokens held, every few
    seconds. Reports contention during admitted work as well as worker use."""

    def __init__(self, every, budget, sampler):
        super().__init__(daemon=True)
        self.every, self.budget, self.sampler = every, budget, sampler
        self.samples = []
        self.halt = threading.Event()

    def run(self):
        while not self.halt.is_set():
            try:
                s = self.sampler()
                self.samples.append((time.monotonic(), s["cpu_user_percent"], s["cpu_system_percent"], self.budget.held(),
                                     self.budget.waiting()))
            except (BlockingIOError, OSError, ValueError):
                pass
            self.halt.wait(self.every)

    def report(self, line):
        if not self.samples:
            return ["host during the run: no sample was taken"]
        users = [row[1] for row in self.samples]
        systems = [row[2] for row in self.samples]
        held = [row[3] for row in self.samples]
        waiting = [row[4] for row in self.samples]
        busy = [u + s for u, s in zip(users, systems)]
        over = sum(1 for value in busy if value >= line)
        return ["host during the run: %d samples, total CPU mean %.0f%%, max %.0f%%, at or over the %g%% "
                "admission line in %d of them (system CPU mean %.0f%%, max %.0f%%); worker tokens held: max %d "
                "of %d, nested workers waiting for one: max %d" % (
                    len(busy), sum(busy) / len(busy), max(busy), line, over, sum(systems) / len(systems),
                    max(systems), max(held), len(self.budget.files), max(waiting))]


def deadline_for(item, args):
    """The kill point: the --deadline floor, or three times what this check is expected to take,
    whichever is later, never past an hour (ci-shard.sh's own rule for a unit). None for an engine
    shard: a shard is a list of units run one after another, and ci-shard.sh already stops EACH
    unit at its own deadline, with its whole tree. A deadline on the list as well killed two
    healthy shards in the third full run, at 3 x the shard's stale planned weight.

    --cap is the exception to all of that (the merge gate, 2026-09-30): one number for every
    check, engine units included (each is ONE unit now, exact unit admission), independent of
    any planned weight. A weight is dated data (`workspace-spec-fourteen` was planned at 2812 s
    and ran in 172-554 s), so a cap derived from it is not a cap."""
    cap = getattr(args, "cap", None)
    if cap:
        return float(cap)
    if item.label in SUITE_DEADLINES:
        return SUITE_DEADLINES[item.label]
    if item.label.startswith("engine ") and any(flag in item.argv for flag in ("--shard", "--only-units")):
        return None
    return min(3600.0, max(args.deadline, 3 * item.weight))


# The scheduler comes round every 0.2 s plus its own work (a stop can take ~11 s).
# A loop gap beyond this was a pause suspending the runner WITH its checks
# (agent_hold.py): that time is nobody's work, so it moves each running check's
# start later instead of bringing its deadline closer. Sage's catch 2 (2026-09-27):
# without it a five-minute pause killed every check with less than five minutes left.
HELD_GAP_SECONDS = 30.0


def discount_held_gap(interval, running):
    """Seconds of `interval` that were a pause; each running check's start moves by that much."""
    held = interval - HELD_GAP_SECONDS
    if held <= 0:
        return 0.0
    for it in running:
        it.started += held
    return held


def over_cap_why(weight, cap):
    return "planned %.0f s, over its %.0f s cap; not started, the nightly runs it" % (weight, cap)


def leave_over_cap(items, args):
    """--cap (the merge gate): a check whose planned weight is over the cap is never started.

    THE MERGE OF 4e73fd89 (2026-09-30). The gate started `operator-fences-mutation.test.sh`,
    planned at 1408 s, under a 600 s cap. It could not finish, it held its lane and most of the
    Mac for its whole 600 s, and the checks the change owned waited 611-870 s behind it until
    the gate's own 900 s cap ended them. A check planned past the cap is now NOT RUN at once,
    with its planned weight and the cap as the reason, and never takes a worker token, a lane or
    admission; an engine unit leaves the receipts proof too, which then proves exactly the units
    that ran. The weight is the planned one proof-run always uses (lib/ci-unit-weights.tsv for
    an engine unit, this checkout's measured history otherwise), dated data that a stale row can
    make wrong in either direction; a row that is too high is corrected by measuring the unit,
    as the SCR row was. Returns the checks left out."""
    cap = getattr(args, "cap", None)
    if not cap:
        return []
    left = []
    for it in items:
        if it.state == "waiting" and it.label != "engine receipts" and it.weight > cap:
            it.state, it.rc = "not-run", None
            it.not_run = {"why": over_cap_why(it.weight, cap),
                          "suites": [{"name": it.label, "state": "over-cap", "reason": over_cap_why(it.weight, cap)}]}
            it.notes.append("NOT RUN: " + over_cap_why(it.weight, cap))
            left.append(it)
    trim_receipts(items, left)
    for it in left:
        print("  NOT RUN %-40s %s" % (it.label, over_cap_why(it.weight, cap)), flush=True)
    return left


def trim_receipts(items, left):
    """The engine receipts proof covers exactly the units that were not left out of this run."""
    gone = {it.argv[it.argv.index("--only-units") + 1] for it in left if it.engine_unit}
    for receipts in (it for it in items if it.label == "engine receipts" and "--units-file" in it.argv):
        path = receipts.argv[receipts.argv.index("--units-file") + 1]
        try:
            with open(path) as stream:
                units = stream.read().split()
        except OSError:
            continue
        kept = [u for u in units if u not in gone]
        if kept != units:
            with open(path, "w") as stream:
                stream.write("".join(u + "\n" for u in kept))
        if not kept and receipts.state == "waiting":
            receipts.state, receipts.rc = "not-run", None
            receipts.not_run = {"why": "no engine unit it proves was started", "suites": []}


RETRY_UNSELECTED = ("not selected for this retry: the run it resumed holds its result, "
                    "and a retry runs only the checks it names")


def leave_unselected(items, selected):
    """--only-check (a resume that runs ONLY the named checks; hunt v2 V02, 2026-10-01).

    A `--resume` keeps every validated pass and then runs everything else in the frozen plan,
    all of it again, concurrently. The merge gate's retry of checks that timed out under
    contention said it ran them "alone" and still put every unfinished check back in the same
    scheduler, so the contention came back and a second no-verdict refused the merge. With
    --only-check the checks not named, and not already passed, are NOT RUN here (their earlier
    result stays in the run that was resumed) and take no worker token, lane or admission. The
    named checks keep the runner's full parallelism among themselves. The engine receipts proof
    covers the units that are left."""
    names = set(selected)
    unknown = sorted(names - {it.label for it in items})
    if unknown:
        raise SystemExit("proof-run: --only-check names no check of the saved plan: " + ", ".join(unknown))
    left = []
    for it in items:
        if it.state == "waiting" and it.label not in names and it.label != "engine receipts":
            it.state, it.rc = "not-run", None
            it.not_run = {"why": RETRY_UNSELECTED,
                          "suites": [{"name": it.label, "state": "retry-unselected", "reason": RETRY_UNSELECTED}]}
            it.notes.append("NOT RUN: " + RETRY_UNSELECTED)
            left.append(it)
    trim_receipts(items, left)
    for it in left:
        print("  NOT RUN %-40s %s" % (it.label, RETRY_UNSELECTED), flush=True)
    return left


def start_key(item, args):
    """The order checks are started in: longest-expected first, with or without --cap, the
    expectation being the check's measured median (MEDIAN_SAMPLES above).

    2026-09-30 to 2026-10-05 the merge gate (--cap) started cheapest first, because the merge of
    4e73fd89 had started a 1408 s mutation unit first under a 600 s cap. A check planned past the
    cap is now never started at all (leave_over_cap), so that reason is gone, and cheapest first
    had a cost of its own: the long poles started last and were ended at the round's cap after
    doing most of their work (merge110.log: by-reference.test.sh waited 440 s in its lane, ran
    443 s, was ended at the 882 s cap and ran again in round 2; 15,508 s redone in all). Started
    first, the longest check's time overlaps everything else's instead of following it."""
    return -item.weight


def run(items, args, logdir, sampler=None):
    args.managed_verification = cpu_guard.verification_enabled()
    remaining = list(items)
    resolved = set()
    while remaining:
        ready = [it for it in remaining if it.after <= resolved]
        if not ready:
            raise ValueError("unknown or cyclic check prerequisites: " + ", ".join(it.label for it in remaining))
        resolved.update(it.label for it in ready)
        remaining = [it for it in remaining if it not in ready]
    leave_over_cap(items, args)
    sampler = HostSamples(sampler or (lambda: reserve.host_sample()), args.sample_every)
    os.makedirs(logdir, exist_ok=True)
    tokens_dir = tempfile.mkdtemp(prefix="worker-tokens-", dir=logdir)
    machine = worker_tokens.machine_directory()
    run_id = uuid.uuid4().hex
    for item in items:
        item.machine_tokens = machine
        item.env["RICHOS_TEST_DEVICE_RUN_ID"] = run_id
        item.env["RICHOS_VERIFICATION_CONTAMINATION"] = os.path.join(logdir, "contamination")
    worker_tokens.init(tokens_dir, args.capacity)
    budget = worker_tokens.Budget(tokens_dir, runner=True, shared=machine)
    budget.shared.admission = engine_pass.Admission(machine, ROOT)
    # A worker permit is only the first admission step. Retain integration
    # intent while eligible checks wait for measured capacity as well.
    args.priority_turn_over = False
    args.integration_intent = (engine_pass.Admission(machine, ROOT)
                               if budget.shared.admission.main else None)
    # A proof run started by a check of a proof run (it works inside its caller's slot) is part
    # of its caller's plan, and that plan's episode already bounds it. It never opens a second
    # one: the episode lock is exclusive, so the inner run would wait for the lock its own caller
    # holds until its fixed bound expired. Measured 2026-09-29 in a main checkout: P16h of
    # proof-run.test.py sat 600 s and failed "integration priority exhausted", and the land
    # gate, whose selection includes that suite, stopped it at its own 600 s bound.
    nested = SLOT is not None and SLOT.borrowed
    args.integration_episode = (engine_pass.IntegrationPlan(machine,
        [{"check": it.label, "command": proof_evidence.command_identity(it, ROOT, logdir),
          "after": sorted(it.after), "requires": sorted(it.requires)} for it in items],
        os.path.abspath(logdir), limit=engine_pass.INTEGRATION_PLAN_SECONDS)
        if args.integration_intent and not nested else None)
    reserved = reserved_tokens(args.capacity)
    order = sorted(items, key=lambda it: (not getattr(it, "retry_first", False), start_key(it, args)))
    running = []
    units = {it.argv[it.argv.index("--only-units") + 1] for it in items if it.engine_unit and it.state != "not-run"}
    args.engine_gate = (engine_pass.PlanGate(len(units), "proof-run", ROOT, sorted(units),
                                            getattr(args, "engine_slot_wait", None))
                        if engine_pass.needs_slot(len(units)) and not engine_pass.held_by_ancestor()
                        else None)
    t0 = time.monotonic()
    args.run_started = t0   # --run-cap's clock (schedule moves it by any pause)
    for item in items:
        item.queued_at = t0
    monitor = Monitor(args.sample_every, budget, sampler)
    monitor.start()

    def stop_running():
        # The whole tree of every running check, not its group alone: a check's descendants
        # start groups and sessions of their own (stop-at-line.py, worker_tokens.py, xcodebuild),
        # and a group kill would leave those running (proc_tree.py's header).
        left = []
        for it in running:
            if it.proc is not None:
                left += stop_item(it)
        return left

    def stop_all(signum, _frame):
        left = stop_running()
        for it in items:
            if it.state in ("waiting", "running"):
                was_running = it.state == 'running'
                it.finish_queue()
                it.state, it.rc, it.ended = "cancelled", 130, time.monotonic()
                if was_running:
                    finish_attempt(it)
        checkpoint(items, logdir)
        if getattr(args, "pool", None):
            args.pool.close(items)
        running.clear()
        print("proof-run: interrupted; every check this run started was stopped%s. Logs: %s" % (
            "" if not left else " EXCEPT pids %s, which survived SIGKILL" % left, logdir), flush=True)
        sys.exit(130)

    previous = {sig: signal.signal(sig, stop_all)
                for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)}

    try:
        schedule(items, order, running, args, logdir, tokens_dir, budget, reserved, sampler)
    except BaseException:
        # Whatever ends this run early (a bug here, an exception nobody foresaw), nothing it
        # started is left running with nobody to stop it: the third full run died on a
        # FileNotFoundError and left two checks' trees under init.
        if running:
            left = stop_running()
            print("proof-run: stopped every running check after an error%s" % (
                "" if not left else "; pids %s survived SIGKILL" % left), flush=True)
        raise
    finally:
        STALL.over()
        monitor.halt.set()
        for sig, handler in previous.items():
            signal.signal(sig, handler)
        for it in items:
            if it.token:
                it.token.release()
        budget.close()
        if args.integration_intent:
            args.integration_intent.close()
        if args.engine_gate:
            args.engine_gate.close()
        # The simulator daemon outlives command processes. Finalize only this run's
        # registered devices, after process cleanup and outside the killed trees.
        import testdevices
        errors = testdevices.cleanup_run_simulators(run_id)
        if errors:
            failed = Item("simulator cleanup", ROOT, [])
            failed.state, failed.rc = "failed", 125
            failed.started = failed.ended = time.monotonic()
            failed.notes.extend(errors)
            items.append(failed)
            print("proof-run: simulator cleanup FAILED: " + "; ".join(errors), flush=True)
        if args.integration_episode:
            args.integration_episode.close("over-budget" if args.integration_episode.expired() else "finished")
            proof_evidence.atomic(os.path.join(logdir, "priority-episode.json"),
                args.integration_episode.record or {"identity": args.integration_episode.identity,
                    "status": "not-admitted", "requested": args.integration_episode.requested,
                    "deadline": args.integration_episode.deadline})
        checkpoint(items, logdir)
    monitor.join(timeout=5)
    args.monitor_lines = monitor.report(args.max_cpu)
    return time.monotonic() - t0


def end_priority_turn(args, budget, running, waiting):
    """The integration plan's PRIORITY turn is over; the plan itself goes on.

    THE BOUND WAS ON THE WRONG THING (2026-09-29). engine_pass.IntegrationPlan gives a
    main-checkout plan admission priority for INTEGRATION_PLAN_SECONDS (600) so background
    runs waiting behind it get a turn: a bound on PRIORITY, and a right one. Until today its
    expiry also stopped every running check and refused every waiting one, which made it a
    bound on how long a land may take. The land of cc/echo-opus-speckle3 was refused that way
    three times running, 47, 30 and 10 of 78 checks unrun, and no check had failed: the third
    try ran on a quiet Mac (mean CPU 52%) and still could not fit, because the runner admits
    one check per fresh CPU sample after a SETTLE_SECONDS ramp (CEO ruling §77's line), about
    one start every 4-5 s, so 78 starts alone take 350 s before any check waits for the CPU.
    No fixed number fits every selection. So now the turn ends and nothing is stopped: the
    plan's priority is released at once (waiters aged past BACKGROUND_PRIORITY_AGE are named
    in the record, exactly as before), its checks keep running, and the rest are admitted
    as background work, behind older background waiters like any other run. The land
    finishes its selection or refuses on a check that did not pass, never on its own clock."""
    args.priority_turn_over = True
    episode = args.integration_episode
    episode.close("over-budget")
    if args.integration_intent:
        args.integration_intent.close()
        args.integration_intent = None
    admission = getattr(budget.shared, "admission", None) if budget.shared else None
    if admission is not None:
        admission.close()
        admission.main = False  # from here on this run asks for admission as background work
    for it in waiting:
        if it.wait_reason == "integration-episode":
            it.wait_reason = "ready"
    print("[%s] proof-run: the %.0f s integration priority turn is over; nothing is stopped: %d running "
          "check(s) go on and %d waiting check(s) are admitted as background work" % (
              stamp(), episode.deadline - episode.requested, len(running), len(waiting)), flush=True)


def schedule(items, order, running, args, logdir, tokens_dir, budget, reserved, sampler):
    n, next_sample, last_launch = 0, 0.0, 0.0
    last_ramp = None      # the check whose start the ramp is waiting to see in a sample
    admitted_lanes = set()
    previous_loop = time.monotonic()
    backoff_reason = "ready"
    backoff_refusing = False
    slot_blocked = False  # the last pass held engine units back for the engine slot
    heartbeat = time.monotonic() + 30
    while True:
        now = time.monotonic()
        interval, previous_loop = now - previous_loop, now
        held = discount_held_gap(interval, running)
        heartbeat += held
        run_cap = getattr(args, "run_cap", None)
        if run_cap:
            # A pause is nobody's time here either: the run's clock moves with it. So is a wait
            # for the engine slot (CEO, 2026-10-30): attempt __c3r0s4 of a land gate was ended at
            # its 900 s cap after its engine units had queued 285 s for the slot.
            args.run_started = getattr(args, "run_started", now) + held
            if slot_blocked:
                args.run_started += max(0.0, interval - held)
            if now - args.run_started >= run_cap:
                end_run_at_cap(items, running, args, logdir, run_cap)
                break
        slot_blocked = False
        for it in order:
            if it.state == "waiting":
                it.wait_times[it.wait_reason] = it.wait_times.get(it.wait_reason, 0.0) + interval
                if it.refusing:
                    # Only an interval the Mac refused it spends its admission limit; a pause
                    # (agent_hold.py) is nobody's time, as for a running check's deadline.
                    it.refused_wait += max(0.0, interval - held)
            it.refusing = False     # set again below only while a counted refusal lasts
        for it in list(running):
            rc = it.proc.poll() if it.proc is not None else 127
            age = now - it.started
            deadline = deadline_for(it, args)
            if rc is None and deadline is not None and age >= deadline:
                # ENFORCED, while the run is going: the check and everything it started are
                # stopped, and the run is red by name. Never a skip.
                left = stop_item(it)
                rc = 124
                it.state = "timed-out"
                it.notes.append("stopped at its %.0f s %s%s" % (
                    deadline, "cap" if getattr(args, "cap", None) else "deadline",
                    "" if not left else "; pids %s survived SIGKILL" % left))
            elif rc is None and age >= args.budget and not it.over_budget:
                it.over_budget = True
                print("[%s] OVER BUDGET %-34s running %.0f s, past the %.0f s budget; %s" % (
                    stamp(), it.label, age, args.budget,
                    "stopped at %.0f s if still running" % deadline if deadline is not None
                    else "each of its units is stopped at its own deadline by ci-shard.sh"), flush=True)
            if rc is not None:
                it.rc, it.ended = rc, time.monotonic()
                if rc == 127:
                    it.notes.append("could not start; see command log")
                if it.state != "timed-out":
                    it.state = "passed" if rc == 0 else "failed"
                if it.state == "passed" or (it.state == "failed" and rc == 2):
                    not_run(it)
                retry_contained = finish_attempt(it)
                running.remove(it)
                it.token.release()
                checkpoint(items, logdir)
                if getattr(args, "pool", None):
                    args.pool.finish(it)
                print("[%s] %-9s %-40s %6.0f s%s" % (stamp(), {"passed": "PASS", "failed": "FAIL", "contained": "CONTAINED",
                      "not-run": "NOT RUN", "invalid": "INVALID",
                      "resource-envelope-exceeded": "RESOURCE", "not-admitted": "REFUSED",
                      "infrastructure-failed": "INFRA", "timed-out": "TIMEOUT"}.get(it.state, "KILLED"),
                                                     it.label, it.seconds, "" if rc == 0 else "  (exit %d)" % rc), flush=True)
                if it.state == "not-run":
                    for suite in it.not_run["suites"]:
                        print("         NOT RUN (%s): %s: %s" % (it.not_run["why"], suite["name"], suite["reason"]), flush=True)
                name_failures(it)
                for name in it.failing[:SHOWN_FAILURES]:
                    print("         failed: %s" % name, flush=True)
                if retry_contained:
                    it.state, it.rc = 'waiting', None
                    it.started = it.ended = it.owner_wait_since = None
                    it.refused_base = it.refused_wait   # a fresh admission limit for the retry
                    it.queued_at = time.monotonic()
                    it.wait_reason = 'controller-recovery'
                    it.retry_first = True
                    order.remove(it)
                    order.insert(0, it)
        contamination = os.path.join(logdir, "contamination")
        record = getattr(args, "evidence", None)
        if record and record.source_invalidated:
            os.makedirs(contamination, exist_ok=True)
            proof_evidence.atomic(os.path.join(contamination, "source.json"),
                                  {"reason": "the checkout's commit changed during verification"})
        unsafe = os.path.isdir(contamination) and bool(os.listdir(contamination))
        fail_fast = getattr(args, "fail_fast", False) and any(
            it.state in ("failed", "timed-out", "not-admitted") for it in items)
        if unsafe or fail_fast:
            if unsafe:
                finding = Item("execution domain contaminated", ROOT, [])
                finding.state, finding.rc = "failed", 1
                finding.notes.append("canary evidence: " + contamination)
                items.append(finding)
            for it in running:
                stop_item(it)
                it.state, it.rc, it.ended = "cancelled", 125, time.monotonic()
                finish_attempt(it)
                it.notes.append("execution domain contaminated" if unsafe else "explicit fail-fast")
                it.token.release()
            running.clear()
            for it in items:
                if it.state == "waiting":
                    it.finish_queue()
                    it.state, it.rc = "cancelled", 125
                    it.notes.append("execution domain contaminated" if unsafe else "explicit fail-fast")
            print("proof-run: %s; unfinished checks are CANCELLED" % (
                "execution domain contaminated" if unsafe else "stopping after the first failure"), flush=True)
            checkpoint(items, logdir)
            break
        if time.monotonic() >= heartbeat:
            print("[%s] progress: %d finished; running %s; logs %s" % (
                stamp(), sum(it.state not in ("waiting", "running") for it in items),
                ", ".join("%s %.0fs" % (it.label, now - it.started) for it in running) or "none", logdir), flush=True)
            checkpoint(items, logdir)
            heartbeat = time.monotonic() + 30
        waiting = [it for it in order if it.state == "waiting"]
        if not waiting and not running:
            break
        episode = getattr(args, "integration_episode", None)
        if getattr(args, "priority_turn_over", False):
            episode = None
        elif episode and episode.expired():
            end_priority_turn(args, budget, running, waiting)
            episode = None
        if episode and not episode.enter():
            for it in waiting:
                it.wait_reason = "integration-episode"
            time.sleep(0.2)
            continue
        busy_lanes = {it.lane for it in running if it.lane}
        done = {it.label for it in items if it.state not in ("waiting", "running")}
        passed = {it.label for it in items if it.state == "passed"}
        for it in waiting:
            if it.requires <= done and not it.requires <= passed:
                it.finish_queue()
                it.state = "blocked"
                it.notes.append("unsuccessful prerequisites: " + ", ".join(sorted(it.requires - passed)))
        waiting = [it for it in waiting if it.state == "waiting"]
        ready = [it for it in waiting if (not it.lane or it.lane not in busy_lanes) and it.after <= done]
        for it in waiting:
            it.wait_reason = ("dependency" if not it.after <= done else
                              "lane" if it.lane and it.lane in busy_lanes else "ready")
        pool = getattr(args, "pool", None)
        if pool:
            owned = []
            for it in ready:
                if pool.claim(it):
                    it.owner_wait_since = None
                    owned.append(it)
                elif it.state == "waiting" and it.owner_slot_wait:
                    # The identical owner is itself queued for the engine slot (its row says so,
                    # Pool.mark_slot_wait): that is a wait for the slot here too, so it spends
                    # neither this run's cap nor its admission wait (review rv-20261030T193938Z).
                    slot_blocked = True
                    it.slot_wait += interval
                    it.wait_reason = "engine-slot"
                    if it.owner_wait_since is not None:
                        it.owner_wait_since += interval
                elif it.state == "waiting":
                    it.wait_reason = "identical-owner"
                    # Its own clock: time waiting for another run's identical check, never
                    # this run's queue and never the Mac's refusals (refused_wait).
                    if it.owner_wait_since is None:
                        it.owner_wait_since = now
                    if now - it.owner_wait_since >= args.admission_wait:
                        it.finish_queue()
                        it.state, it.rc = "not-admitted", 75
                        it.notes.append("identical owner did not finish within the admission wait")
            ready = owned
        gate = getattr(args, "engine_gate", None)
        slot_queued = set()   # this run's own checks held back for the engine slot this pass
        if gate:
            engine_running = any(it.engine_unit for it in running)
            engine_ready = [it for it in ready if it.engine_unit]
            if not any(it.engine_unit for it in waiting) and not engine_running:
                gate.close()
            elif engine_ready:
                try:
                    allowed = gate.ready(engine_running)
                except engine_pass.Refused as exc:
                    allowed = False
                    for it in waiting:
                        if it.engine_unit:
                            it.finish_queue()
                            it.state, it.rc = "not-admitted", engine_pass.REFUSED
                            it.notes.append(exc.message())
                if not allowed:
                    slot_blocked = True
                    for it in engine_ready:
                        it.slot_wait += interval
                        it.wait_reason = "engine-slot"
                    slot_queued = {it.label for it in engine_ready if it.state == "waiting"}
                    ready = [it for it in ready if not it.engine_unit]
        if pool:
            pool.mark_slot_wait(slot_queued)   # what a run joining these checks reads
        intent = getattr(args, 'integration_intent', None)
        if intent:
            if ready:
                intent.begin()
            else:
                intent.close()
        # Ordering-only dependencies still run diagnostics after failure. Explicit success
        # prerequisites block dependent execution. Nested workers never take the
        # reserved tokens (worker_tokens.py), so a check not yet started always gets one.
        # Keep gradual ramp-up into additional lanes. A replacement in an
        # already admitted lane does not increase the established concurrency;
        # it still needs a fresh host sample and a real worker permit.
        # The ramp exists so the next sample shows the load of the check started last. When
        # that check has already exited there is no load left to wait for, and waiting anyway
        # made every short check cost a full SETTLE_SECONDS (2026-09-29: the last 30 checks of
        # the 78 in cc/echo-opus-speckle3's land each ran 2-6 s, one at a time, ~4-5 s apart).
        settled = last_ramp is not None and last_ramp.state != "running"
        eligible = [it for it in ready if not running or
                    (it.lane is not None and it.lane in admitted_lanes) or
                    now - last_launch >= SETTLE_SECONDS or settled]
        for it in ready:
            if it not in eligible:
                it.wait_reason = "ramp"
        if now < next_sample:
            for it in eligible:
                it.wait_reason = backoff_reason
                it.refusing = backoff_refusing
        if eligible and now >= next_sample:
            # Only HEAD stops the run. An edited file invalidates only the checks whose own
            # inputs hold it, when they finish (proof_evidence.Record, part-2 hunt section 07).
            if record and record.current_commit() != record.source["commit"]:
                record.source_invalidated = True
                continue
            it = eligible[0]
            # The machine pressure controller's verdict is read from THIS attempt only: when this
            # run's own tokens are full the machine budget is never asked, and an older refusal
            # left on it would count this run's own queue as the Mac refusing.
            pressure_owner = budget.shared or budget
            pressure_owner.refusal = None
            token = budget.try_acquire()
            ok, s = (False, None)
            if token is not None:
                try:
                    ok, s = admitted(args, sampler)
                except BaseException:
                    token.release()
                    raise
                if not ok:
                    token.release()
            if ok:
                admitted_item = None
                for candidate in eligible:
                    try:
                        reserve_item(candidate, n + 1, args, logdir)
                    except cpu_guard.RecoveryExhausted as exc:
                        candidate.finish_queue()
                        candidate.rc = 75
                        candidate.state = ('scheduler-starvation' if 'scheduler-starvation' in str(exc)
                                           else 'resource-recovery-exhausted')
                        candidate.notes.append(str(exc))
                        continue
                    except BlockingIOError as exc:
                        candidate.wait_reason = 'resource-envelope'
                        candidate.resource_refusal = str(exc)
                        continue
                    admitted_item = candidate
                    break
                if admitted_item is None:
                    token.release()
                    eligible = [candidate for candidate in eligible if candidate.state == 'waiting']
                    if not eligible:
                        checkpoint(items, logdir)
                        continue
                    it = eligible[0]
                    ok = False
                    s = None
                else:
                    it = admitted_item
            if ok:
                STALL.over()
                it.finish_queue()
                it.token = token
                n += 1
                running.append(it)
                if gate and it.engine_unit and gate.slot:
                    it.slot_fds = (gate.slot.fd,)
                if episode and episode.fd is not None:
                    it.slot_fds = tuple(getattr(it, "slot_fds", ())) + (episode.fd,)
                launch(it, n, logdir, tokens_dir, reserved)
                checkpoint(items, logdir)
                if it.lane is None or it.lane not in admitted_lanes:
                    last_launch, last_ramp = time.monotonic(), it
                if it.lane is not None:
                    admitted_lanes.add(it.lane)
                print("[%s] start  %-40s %s" % (stamp(), it.label,
                                                "(queued %.0f s before launch, %.0f s of it refused by admission)" % (
                                                    it.admission_wait, it.refused_wait)
                                                if it.admission_wait >= 1 else ""),
                      flush=True)
            else:
                resource_blocked = token is not None and s is None and any(
                    queued.wait_reason == 'resource-envelope' for queued in eligible)
                pressure = getattr(pressure_owner, 'refusal', None) if token is None else None
                refusal_kind = ('resource-envelope' if resource_blocked else
                                'host' if s is not None else 'pressure' if pressure else 'worker')
                counted = refusal_counts(running)
                for queued in eligible:
                    queued.wait_reason = refusal_kind
                    queued.refusing = counted
                waited = it.refused_wait - it.refused_base
                if counted and waited >= args.admission_wait:
                    STALL.over()
                    it.finish_queue()
                    it.state, it.rc = "not-admitted", 75
                    reason = (reserve.describe(s) if s is not None else getattr(it, 'resource_refusal', None)
                              or pressure or getattr(budget.shared or budget, 'refusal', None) or "worker budget is full")
                    it.notes.append("not admitted after %.0f s of refusals (%.0f s queued in all): %s" % (
                        waited, it.admission_wait, reason))
                    print("[%s] REFUSED %-39s not admitted after %.0f s of refusals (%s)" % (
                        stamp(), it.label, waited, reason), flush=True)
                else:
                    next_sample = now + (reserve.MIN_RETRY_SECONDS if s is not None else 0.2)
                    backoff_reason = refusal_kind
                    backoff_refusing = counted
                    if not running and s is not None:
                        STALL.refused("proof-run: %s not admitted: %s" % (it.label, reserve.describe(s)))
                    else:
                        STALL.over()
                    if not running and s is not None:
                        print("[%s] wait   %-40s admission: %s" % (stamp(), it.label, reserve.describe(s)), flush=True)
        time.sleep(0.2)


# The state a check ends in when the run ends it before it could finish.
CANCELED = "cancelled"  # dialect-exempt: the state value summary.json, progress.json and every caller already read


def end_run_at_cap(items, running, args, logdir, run_cap):
    """--run-cap reached (the merge gate, 2026-09-30): the run ends ON ITS OWN CLOCK, which
    end_priority_turn deliberately does not do for a land's selection. Every running check is
    stopped with its whole tree and every waiting one never starts; both end in CANCELED with the
    reason, so the caller can tell a check that did not finish in the time it was given from one
    that failed. What already finished keeps its result."""
    for it in list(running):
        left = stop_item(it)
        it.state, it.rc, it.ended = CANCELED, 125, time.monotonic()
        it.notes.append("stopped at the run's %.0f s cap (--run-cap)%s" % (
            run_cap, "" if not left else "; pids %s survived SIGKILL" % left))
        finish_attempt(it)
        it.token.release()
        print("[%s] RUN CAP  %-40s stopped at the run's %.0f s cap" % (stamp(), it.label, run_cap), flush=True)
    running.clear()
    for it in items:
        if it.state == "waiting":
            it.finish_queue()
            it.state, it.rc = CANCELED, 125
            it.notes.append("not started: the run's %.0f s cap (--run-cap) was reached" % run_cap)
    print("[%s] proof-run: the run's %.0f s cap (--run-cap) is reached; what did not finish is ended, "
          "not failed" % (stamp(), run_cap), flush=True)
    checkpoint(items, logdir)


SHOWN_FAILURES = 20

# run-tests.sh's per-suite states that mean the suite's code did not execute in this run, and
# the reason each one carries into summary.json. `gap` exits 0 only when declared.
NOT_RUN_STATES = {"notrun": "no-screen", "gap": "host-gap", "skipped": "unchanged-inputs"}


UI_TESTS_DIR = os.path.join(ROOT, "richos", "app", "ui", "tests")


def ui_suite_file(item):
    """The suite a check runs as `cd richos/app/ui/tests && node <suite>.js` (proof-for.sh's UI
    line), or None. Discovered exactly as run.js and proof-for.sh discover suites: any .js."""
    argv = item.argv
    if (len(argv) == 2 and argv[0] == "node" and argv[1].endswith(".js") and not argv[1].startswith("-")
            and os.path.realpath(item.cwd) == os.path.realpath(UI_TESTS_DIR)):
        return argv[1]
    return None


def ui_not_run(it):
    """For a UI suite run directly that exited 0: its state from the evidence ledger its harness
    wrote (lib/harness.js recordEvidence), the record run.js gates the nightly on.

    Hunt part 2, finding 18: realbytes.js calls skipSuite() and exits 0 when it cannot run, and
    run.js refuses that skip, but the land runs the suite directly and read only the exit code,
    so the skip was recorded `passed`. A suite whose only record is a skip is NOT RUN
    (`suite-skipped`); one that exits 0 with no check in its ledger, or with a failed check, is
    `invalid`, exactly the two faults run.js names (NO EVIDENCE; failures that did not reach
    the exit code)."""
    records = []
    try:
        with open(it.ui_ledger) as stream:
            for line in stream:
                if line.strip():
                    row = json.loads(line)
                    if isinstance(row, dict):
                        records.append(row)
    except FileNotFoundError:
        pass
    except (OSError, ValueError) as exc:
        it.state, it.rc = "invalid", 125
        it.notes.append("exited 0 with an unreadable evidence ledger (%s): it cannot show it ran" % exc)
        return
    runs = [r for r in records if isinstance(r.get("checks"), int) and not isinstance(r.get("checks"), bool)]
    skips = [r for r in records if isinstance(r.get("skipped"), str)]
    if skips and not runs:
        suite = ui_suite_file(it)
        reason = skips[0]["skipped"].strip().split("\n")[0]
        it.state = "not-run"
        it.not_run = {"why": "suite-skipped", "suites": [{"name": suite, "state": "skipped", "reason": reason}]}
        it.notes.append("NOT RUN (suite-skipped): %s: %s" % (suite, reason))
        return
    if not runs:
        it.state, it.rc = "invalid", 125
        it.notes.append("exited 0 without a single check in its evidence ledger: it cannot show it ran")
        return
    failed = sum(r.get("failed", 0) for r in runs if isinstance(r.get("failed"), int))
    if failed:
        it.state, it.rc = "invalid", 125
        it.notes.append("exited 0 but its evidence ledger records %d failed check(s)" % failed)
        return
    # The suite's OWN checks, as run.js counts them (recheck R26): `checks` also counts the
    # housekeeping the harness appends (the tracked-tree guard), which is not product coverage, so
    # a run whose only passing check is the harness verifying its own tree is not evidence. A
    # record written before `productChecks` existed falls back to `checks`, as run.js does.
    def own(r):
        value = r.get("productChecks", r["checks"])
        return value if isinstance(value, int) and not isinstance(value, bool) else r["checks"]
    observed = sum(own(r) for r in runs)
    declared = [r["declared"] for r in runs if isinstance(r.get("declared"), int) and not isinstance(r.get("declared"), bool)]
    floor = max(declared) if declared else None
    if floor == 0:
        it.state, it.rc = "invalid", 125
        it.notes.append("exited 0 but its source declares no `run.check(` at all: a suite that cannot fail")
    elif observed == 0:
        it.state, it.rc = "invalid", 125
        it.notes.append("exited 0 having run no check of its own (only the harness's housekeeping): it cannot show it ran")
    elif floor is not None and observed < floor:
        it.state, it.rc = "invalid", 125
        it.notes.append("exited 0 but ran %d check(s) of its own and its source declares %d: it stopped early" % (observed, floor))


def not_run(it):
    """For a check that exited 0 and keeps a machine record of what it ran: a run-tests.sh
    check's own --results-out record, or a directly run UI suite's evidence ledger (ui_not_run).
    A suite that did not run makes the check `not-run`; no record at all makes it `invalid`."""
    if it.rc not in (None, 0, 2):
        return
    if getattr(it, "ui_ledger", None):
        ui_not_run(it)
        return
    if "--results-out" not in it.argv:
        if it.label == "native-ios-ui" and it.rc == 2 and it.log:
            text = Path(it.log).read_text(errors="replace")
            idle = [line.strip() for line in text.splitlines() if line.startswith("  NOT RUN  ")]
            if idle and not any(line.startswith("  FAIL  ") for line in text.splitlines()):
                it.state = "not-run"
                it.not_run = {"why": "host-gap", "suites": [{"name": "native-ios-ui.test.sh", "state": "gap", "reason": idle[-1]}]}
        return
    report = it.argv[it.argv.index("--results-out") + 1]
    try:
        with open(report) as stream:
            suites = json.load(stream)["suites"]
        states = [(str(s["name"]), str(s["state"]), str(s.get("reason", ""))) for s in suites]
    except (OSError, ValueError, KeyError, TypeError) as exc:
        if it.rc:
            return
        it.state, it.rc = "invalid", 125
        it.notes.append("run-tests.sh exited 0 without a readable results record (%s): it cannot show it ran" % exc)
        return
    odd = [name for name, state, _ in states if state not in NOT_RUN_STATES and state != "passed"]
    if not states or odd:
        if it.rc:
            return
        it.state, it.rc = "invalid", 125
        it.notes.append("run-tests.sh exited 0 but its results record says %s" % (
            ", ".join("%s %s" % (n, st) for n, st, _ in states if n in odd) or "no suite ran"))
        return
    idle = [(name, state, reason) for name, state, reason in states if state in NOT_RUN_STATES]
    if idle:
        why = sorted({NOT_RUN_STATES[state] for _, state, _ in idle})
        it.state = "not-run"
        it.not_run = {"why": "+".join(why), "suites": [{"name": n, "state": st, "reason": r} for n, st, r in idle]}
        it.notes.append("NOT RUN (%s): %s" % (it.not_run["why"], "; ".join("%s: %s" % (n, r) for n, _, r in idle)))


class Stall(object):
    """A proof run with nothing of its own running whose next check admission refuses is a job
    waiting on the Mac's CPU. It is recorded for exactly that long (engine resource_waits.py), so
    the lead is stopped at turn end once it passes ten minutes. Checks waiting behind this run's
    own running checks are scheduling, not a wait, and are never recorded."""

    def __init__(self):
        self.wait = None

    def refused(self, why):
        rw = getattr(reserve, "resource_waits", None)
        if rw is None:
            return
        try:
            if self.wait is None:
                self.wait = rw.waiting(rw.CPU, why).start()
            else:
                self.wait.update(why)
        except Exception as exc:  # the record never breaks the run
            print("proof-run: admission wait not recorded: %s" % exc, flush=True)
            self.wait = None

    def over(self):
        if self.wait is not None:
            self.wait.close()
            self.wait = None


STALL = Stall()


def name_failures(it):
    """What failed in a finished check, BY NAME, into it.failing: the failing tests its result
    files hold (JUnit XML, Xcode bundles, under it.results), else the names its log carries
    (the FAILED TEST lines a test run prints, XCTest and Gradle lines), else its own `  FAIL  `
    lines, else the last error it printed. A check that passed leaves no results folder behind. 2026-09-25: a check failed with
    "133 tests completed, 1 failed" and no name, and the report that had one was replaced by
    the next check's before anyone read it."""
    if it.state == "passed":
        if it.results:
            shutil.rmtree(it.results, ignore_errors=True)
        return
    found = []
    if it.results and os.path.isdir(it.results):
        found = test_results.names([it.results])
    if not found and it.log:
        found = test_results.names([], [it.log])
    it.failing = [n + (" — " + d if d else "") for n, d in found]
    if not it.failing and it.log:
        try:
            with open(it.log, errors="replace") as fh:
                it.failing = [l[len("  FAIL  "):].rstrip("\n") for l in fh if l.startswith("  FAIL  ")][:SHOWN_FAILURES]
        except OSError:
            pass
    if not it.failing and it.log:
        # Died before any test ran (a simulator never leased, a build that did not compile):
        # the last error it printed, so the line beside the check still says what went wrong.
        err = test_results.last_error(it.log)
        if err:
            it.failing = ["(no test ran to fail) " + err]


def stop_item(it):
    """Give the independent supervisor its cleanup window before escalating."""
    if it.proc is None or it.proc.poll() is not None:
        return []
    it.proc.send_signal(signal.SIGTERM)
    try:
        it.proc.wait(timeout=15)
        return [] if it.proc.returncode != 125 else [it.proc.pid]
    except subprocess.TimeoutExpired:
        left = proc_tree.kill_tree(it.proc.pid, 3.0)
        it.proc.wait(timeout=5)
        return left


def checkpoint(items, logdir):
    records = {getattr(item, "evidence", None) for item in items} - {None}
    for record in records:
        record.checkpoint(items)
    path = os.path.join(logdir, "progress.json")
    with open(path + ".new", "w") as out:
        json.dump([{"check": i.label, "state": i.state, "exit": i.rc, "log": i.log,
                    "notes": i.notes} for i in items], out)
    os.replace(path + ".new", path)


def head_commit():
    return subprocess.check_output(["git", "-C", ROOT, "rev-parse", "HEAD"], text=True).strip()


def selected_by(args):
    """What the plan was selected from, saved in plan.json beside it (2026-10-20).

    An author's handover run names its range, `<recorded base>..<candidate>`; both ends are
    resolved to commits here, at proof time, so whoever reads the saved run later (the
    workspace registry's handover report, a land) knows which base and which candidate the
    plan covers without re-deriving it from a branch that has moved since. The mode says
    which selection it was (`--gate` is the merge gate's). A `--commands` plan names its file."""
    if args.commands:
        return {"commands": os.path.abspath(args.commands), "sha256": proof_evidence.file_digest(args.commands)}
    record = {"arguments": list(args.proof_for_args), "mode": "gate" if "--gate" in args.proof_for_args else "normal"}
    refs = [a for a in args.proof_for_args if not a.startswith("-")]
    if len(refs) == 1 and ".." in refs[0] and "..." not in refs[0]:
        base, candidate = refs[0].split("..", 1)
        for name, ref in (("base", base or "HEAD"), ("candidate", candidate or "HEAD")):
            found = subprocess.run(["git", "-C", ROOT, "rev-parse", "--verify", "--quiet", ref + "^{commit}"],
                                   capture_output=True, text=True, stdin=subprocess.DEVNULL)
            record[name] = found.stdout.strip() if found.returncode == 0 else None
    return record


def source_identity():
    """The whole checkout: HEAD, the tracked diff and every untracked file git does not ignore.

    Recorded with every plan and outcome. It binds a check with no identity of its own; HEAD
    binds every check. A change to the rest is judged per check (proof_evidence.Record)."""
    head = head_commit()
    diff = subprocess.check_output(["git", "-C", ROOT, "diff", "--binary", "HEAD"])
    digest = hashlib.sha256()
    untracked = subprocess.check_output(["git", "-C", ROOT, "ls-files", "--others", "--exclude-standard", "-z"])
    for raw in sorted(p for p in untracked.split(b"\0") if p):
        path = os.path.join(ROOT, os.fsdecode(raw))
        digest.update(raw + b"\0")
        try:
            if os.path.islink(path):
                digest.update(os.fsencode(os.readlink(path)))
            else:
                with open(path, "rb") as source:
                    for chunk in iter(lambda: source.read(65536), b""):
                        digest.update(chunk)
        except FileNotFoundError:
            # Listed, then gone before it was read (2026-09-30: updater-setup.test.sh's key,
            # removed by its own cleanup, crashed a land here). Absence is a state of the
            # checkout: recorded as one, so the identity differs from the one with the file,
            # and a vanished file never crashes the run.
            digest.update(b"\0absent\0")
    return {"commit": head, "tracked_diff_sha256": hashlib.sha256(diff).hexdigest(),
            "untracked_sha256": digest.hexdigest()}


def notes_from_logs(items):
    for it in items:
        if not it.log:
            continue
        try:
            with open(it.log, errors="replace") as source:
                text = source.read()
        except OSError:
            continue
        for line in text.splitlines():
            if re.search(r"NOT RUN|SKIPPED:|KNOWN-RED", line):
                it.notes.append(line.strip()[:160])


def summarize(items, wall, logdir, budget_seconds=600, monitor_lines=()):
    serial = sum(it.total_seconds for it in items)
    print("")
    print("proof-run: %d check(s), wall %.0f s (%.1f min); sum of check execution times: %.0f s" %
          (len(items), wall, wall / 60, serial))
    for line in monitor_lines:
        print("  " + line)
    # queue: all the time it waited to start; refused: the part the Mac refused it (refusal_counts),
    # the only part --admission-wait bounds.
    print("  %-40s %-12s %8s %10s %10s  %s" % ("check", "result", "seconds", "queue", "refused", "log"))
    rows = []
    for it in sorted(items, key=lambda i: -(i.total_seconds)):
        print("  %-40s %-12s %8.0f %9.0fs %9.0fs  %s" % (it.label, it.state.upper(), it.total_seconds, it.admission_wait,
                                                        getattr(it, "refused_wait", 0.0), os.path.basename(it.log or "-")))
        if it.total_seconds > budget_seconds:
            print("      OVER BUDGET: %.0f s is past the %.0f s budget one check may take" % (it.total_seconds, budget_seconds))
        for note in it.notes[:6]:
            print("      %s" % note)
        # Beside the check, what failed in it, by name, and where its result files are kept.
        for name in it.failing[:SHOWN_FAILURES]:
            print("      failed: %s" % name)
        if len(it.failing) > SHOWN_FAILURES:
            print("      ... and %d more (summary.json lists every one)" % (len(it.failing) - SHOWN_FAILURES))
        kept = it.results if it.results and os.path.isdir(it.results) and os.listdir(it.results) else None
        if kept and it.state != "passed":
            print("      per-test results: %s" % kept)
        rerun = getattr(it, "rerun", None)
        if rerun and it.state not in ("passed", "not-run"):
            print("      rerun as the gate ran it: sh %s" % shlex.quote(rerun))
            print("        (%s)" % getattr(it, "rerun_shown", ""))
        rows.append({"check": it.label, "result": it.state, "seconds": round(it.total_seconds, 1),
                     "last_attempt_seconds": round(it.seconds, 1),
                     "admission_wait": round(it.admission_wait, 1),  # legacy field: total queue
                     "total_queue_seconds": round(it.admission_wait, 1),
                     "admission_refused_seconds": round(getattr(it, "refused_wait", 0.0), 1),
                     "engine_slot_wait": round(it.slot_wait, 1),
                     "wait_seconds_by_reason": {k: round(v, 3) for k, v in it.wait_times.items()},
                     "exit": it.rc, "log": it.log,
                     "invalid": invalid_reason(it),
                     "reused_from": getattr(it, "reused_from", None),
                     "attempts": it.attempts,
                     "repeated_seconds": sum(a['seconds'] for a in it.previous_attempts),
                     "cpu_seconds": attempt_cpu(it.attempts),
                     "repeated_cpu_seconds": attempt_cpu(it.previous_attempts),
                     "failing_tests": list(it.failing), "results": kept,
                     "not_run": getattr(it, "not_run", None),
                     "known_red": known_red(it),
                     "rerun": getattr(it, "rerun", None),
                     "command": "cd %s && %s" % (os.path.relpath(it.cwd, ROOT), " ".join(shlex.quote(a) for a in it.argv))})
    coverage = coverage_line(rows)
    with open(os.path.join(logdir, "summary.json"), "w") as fh:
        json.dump({"wall_seconds": round(wall, 1), "serial_seconds": round(serial, 1), "host": list(monitor_lines),
                   "coverage": coverage, "checks": rows}, fh, indent=1)
    print("")
    print("proof-run: coverage: " + coverage)
    try:
        os.rmdir(os.path.join(logdir, "results"))  # only when no check kept anything
    except OSError:
        pass
    idle = [it for it in items if it.state == "not-run"]
    bad = [it for it in items if it.state not in ("passed", "not-run")]
    idle_line = ("%d check(s) NOT RUN, which is not a pass: %s" % (len(idle), ", ".join(
        "%s (%s)" % (it.label, it.not_run["why"]) for it in idle))) if idle else ""
    print("")
    if bad:
        print("=== proof-run: %d of %d check(s) did NOT pass: %s%s ===" % (len(bad), len(items),
              ", ".join("%s (%s)" % (b.label, b.state) for b in bad), "; " + idle_line if idle else ""))
        print("    logs: %s" % logdir)
        return 1
    if idle:
        print("=== proof-run: %d of %d check(s) passed; %s ===" % (len(items) - len(idle), len(items), idle_line))
        print("    logs: %s" % logdir)
        return 3
    reused = sum(bool(getattr(it, "reused_from", None)) for it in items)
    red = [row["check"] for row in rows if row["known_red"]]
    print("=== proof-run: all %d check(s) passed in %.0f s%s%s ===" % (
        len(items), wall, "; reconciled with %d reused result(s)" % reused if reused else "",
        "; %d of them KNOWN-RED, a declared exception and not a pass: %s" % (len(red), ", ".join(red))
        if red else ""))
    print("    logs: %s" % logdir)
    return 0


def known_red(it):
    """True for a check that passed only as a declared KNOWN-RED engine unit (its receipt's
    verdict). The policy lets it through; a report never counts it as passed (PRD A14)."""
    if it.state != "passed" or not getattr(it, "engine_unit", False):
        return False
    try:
        path = proof_evidence.receipt_path(it)
        rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
        return len(rows) == 1 and rows[0].get("verdict") == "KNOWN-RED"
    except (OSError, ValueError, AttributeError, TypeError):
        return False


def coverage_line(rows):
    """THE GENERATED COVERAGE LINE (verification reliability PRD, A11/A14): what the summary
    holds, counted from its own rows, so nobody reports arithmetic such as total minus failed
    (the October 19 "86 passed" counted four checks that never ran). Reused passes are named
    inside the passes, KNOWN-RED and NOT RUN beside them, never inside."""
    counts = {}
    for row in rows:
        counts[row["result"]] = counts.get(row["result"], 0) + 1
    red = sum(1 for row in rows if row["known_red"])
    passed = counts.pop("passed", 0) - red
    reused = sum(1 for row in rows if row["result"] == "passed" and row["reused_from"] and not row["known_red"])
    idle = counts.pop("not-run", 0)
    parts = ["%d planned" % len(rows), "%d passed (%d of them reused)" % (passed, reused)]
    if red:
        parts.append("%d KNOWN-RED (a declared exception, not a pass)" % red)
    if idle:
        parts.append("%d NOT RUN (not a pass)" % idle)
    parts += ["%d %s" % (n, state) for state, n in sorted(counts.items())]
    return ", ".join(parts)


def attempt_cpu(attempts):
    values = [attempt.get('cpu_seconds') for attempt in attempts]
    return sum(values) if values and all(value is not None for value in values) else None


def default_logdir():
    base = str(M.state("proof-runs", "RICHOS_PROOF_RUN_DIR"))
    wid = subprocess.run(["/usr/bin/shasum", "-a", "256"], input=ROOT, capture_output=True, text=True).stdout[:12]
    return os.path.join(base, wid)


def rotate(parent):
    """Bounded by construction (§54): the last KEEP_RUNS runs of this checkout, nothing more."""
    try:
        runs = sorted(d for d in os.listdir(parent) if re.fullmatch(r"\d{8}T\d{6}Z(-[A-Za-z0-9_]+)?", d))
    except OSError:
        return
    successful = []
    for d in runs:
        try:
            with open(os.path.join(parent, d, "summary.json")) as source:
                summary = json.load(source)
            # A run whose only non-passes are NOT RUN failed nothing; it rotates like a green one.
            if summary["checks"] and all(c["result"] in ("passed", "not-run") for c in summary["checks"]):
                successful.append(d)
        except (OSError, ValueError, KeyError, TypeError):
            continue
    for d in successful[:-KEEP_RUNS]:
        path = os.path.join(parent, d)
        if os.path.exists(os.path.join(path, "retain")):
            continue
        try:
            lease = proof_evidence.Lease(path)
        except (OSError, ValueError):
            continue
        try:
            shutil.rmtree(path, ignore_errors=True)
        finally:
            lease.close()


# A HANDOVER RUN IS HELD UNTIL ITS LAND HAS CONSUMED IT (verification reliability PRD, A11).
# An author's run over an explicit `<recorded base>..<candidate>` range in a teammate workspace is
# the evidence its handover points at (workspaces.py proof_report) and its land validates. Rotation
# keeps only the last KEEP_RUNS successful runs of a checkout, so three narrow green runs after the
# handover would have removed it before the land read it. So that run gets rotate()'s existing
# `retain` marker before its pointer exists (the `logs:` line, then the registry's report), naming
# what it holds and for whom. The land that merges its candidate removes it once the land's own
# evidence is written (autocheck.py post_merge, release_author_runs), and rotation applies again.
# A superseded or abandoned handover is named in the report (also_held) and released by the lead
# with `--release <run>` after checking nothing still needs it; nothing sweeps holds, and
# `--release` removes only a handover hold. The main checkout is never a handover seat.
HANDOVER_HOLD = "handover"


def hold_for_handover(logdir, selected):
    if not (selected and selected.get("base") and selected.get("candidate")):
        return
    if engine_pass.is_main_checkout(ROOT):
        return
    marker = os.path.join(logdir, "retain")
    proof_evidence.atomic(marker, {"holder": HANDOVER_HOLD, "checkout": ROOT, "base": selected["base"],
                                   "candidate": selected["candidate"], "mode": selected.get("mode"),
                                   "at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")})
    print("  held for its handover until its land consumes it: %s" % marker, flush=True)


def release_hold(run):
    """--release RUN: remove RUN's handover hold; refuse any other kind of hold."""
    marker = os.path.join(run, "retain")
    try:
        with open(marker) as stream:
            hold = json.load(stream)
    except FileNotFoundError:
        print("proof-run: %s holds no handover hold; nothing to release" % run)
        return 0
    except (OSError, ValueError):
        hold = None
    if not isinstance(hold, dict) or hold.get("holder") != HANDOVER_HOLD:
        print("proof-run: REFUSED: %s is not a handover hold, so it is not this command's to remove" % marker)
        return 2
    os.unlink(marker)
    print("proof-run: released the handover hold on %s (candidate %s); rotation applies to it again" % (
        run, str(hold.get("candidate"))[:12]))
    rotate(os.path.dirname(os.path.realpath(run)))
    return 0


def author_coverage(args):
    """--coverage RUN: THE READINESS PREFLIGHT (verification reliability PRD, R3/A8), reads only.

    Does the saved handover RUN prove its candidate's whole selected plan? Asked in RUN's own
    checkout at its candidate: the selection is made again from the run's recorded range and
    mode, so a suite the run never planned is missing, and every planned check's inputs are read
    now and judged against the run's outcomes by the predicate reuse() applies
    (proof_evidence.coverage). Nothing runs, nothing is copied. A NOT RUN check is the existing
    NOT RUN eligibility, named separately and never proven. Exit 0: every planned check proven;
    1: some are not, with the one runner job that resolves them all; 2: no answer here."""
    run = os.path.realpath(args.coverage)
    try:
        saved = proof_evidence.read_plan(run)
        with open(os.path.join(run, "outcomes.json")) as stream:
            outcomes = json.load(stream)
        try:
            with open(os.path.join(run, "summary.json")) as stream:
                not_run = {row.get("check"): (row.get("not_run") or {}).get("why")
                           for row in json.load(stream).get("checks") or []}
        except FileNotFoundError:
            not_run = {}
    except (OSError, ValueError, AttributeError) as exc:
        print("proof-run: coverage: %s is not a readable saved run: %s" % (run, exc))
        return 2
    chosen = saved.get("selection") or {}
    if saved.get("root") != ROOT:
        print("proof-run: coverage: %s ran in %s; its coverage is asked there" % (run, saved.get("root")))
        return 2
    if not (chosen.get("base") and chosen.get("candidate")):
        print("proof-run: coverage: %s names no recorded <base>..<candidate> range, so it stands for no handover" % run)
        return 2
    if head_commit() != chosen["candidate"]:
        print("proof-run: coverage: this checkout is at %s, not at the run's candidate %s" % (
            head_commit()[:12], chosen["candidate"][:12]))
        return 2
    # THE RUN'S OWN ENVIRONMENT (proof_evidence THE RECORDED ENVIRONMENT): every check's inputs
    # are read as the run started its checks, not as the shell asking this question holds them,
    # so a land asked from the lead's shell never calls an unchanged pass missing for LANG or
    # SDKROOT. A run that says nothing of it (proof_evidence.run_environment) is read under this
    # process's, as before.
    shell = dict(os.environ)
    recorded = proof_evidence.run_environment(run)
    if recorded:
        os.environ.clear()
        os.environ.update(as_caller(recorded, saved["root"], shell))
    try:
        return covered(args, run, saved, outcomes, not_run, chosen,
                       "the run's recorded environment" if recorded else "this shell's environment")
    finally:
        os.environ.clear()
        os.environ.update(shell)


def covered(args, run, saved, outcomes, not_run, chosen, read_under):
    """author_coverage's answer, in the environment it set."""
    started = time.monotonic()
    args.commands = None
    args.proof_for_args = [*(["--gate"] if chosen.get("mode") == "gate" else []),
                           "%s..%s" % (chosen["base"], chosen["candidate"])]
    # THE RUN'S OWN SETTINGS (review rv-20261020T174711Z-042986a6-bafe #2). IDENTITY_SETTINGS are
    # part of every check's identity, so reading the inputs under this process's defaults called
    # an unchanged pass made with --fail-fast (or another capacity) missing. The run's saved
    # identities name the settings it ran under; they are read back, and the recovery job keeps
    # the ones that differ from this runner's defaults, so it can reuse those passes.
    ran_with = next((row.get("settings") for row in (saved.get("identities") or {}).values()
                     if isinstance(row, dict) and isinstance(row.get("settings"), dict)), {})
    setting_flags = []
    for key in IDENTITY_SETTINGS:
        if key in ran_with and ran_with[key] != getattr(args, key, None):
            setattr(args, key, ran_with[key])
            if key == "fail_fast":
                setting_flags += ["--fail-fast"] if ran_with[key] else []
            else:
                setting_flags += ["--" + key.replace("_", "-"), str(ran_with[key])]
    # THE SAVED PLAN IS THE SELECTION WHEN THE BYTES ARE THE SAME (rollout, PRD line 139's
    # no-execution budget). The selection is a function of the range, the mode and the checkout's
    # bytes; at the run's candidate, with the tracked diff and every untracked file reading exactly
    # as when the run started (its saved source), selecting again gives the plan the run saved,
    # and cost about 45 s of the preflight's 49 (03-E). Any difference selects again, so a suite a
    # final edit adds is still missing. The inputs of every check are read now either way.
    now = source_identity()
    same_bytes = saved.get("source") == now
    # Resolved, as a run's own log directory is (machine storage resolves it): a check's command
    # names its result file as $RUN/... only when the path starts with the resolved directory,
    # so under a TMPDIR behind a symlink (macOS's /var -> /private/var) every run-tests.sh check
    # read as "declared execution inputs changed" (03-E, the recovery demonstration).
    scratch = os.path.realpath(tempfile.mkdtemp(prefix="coverage-"))
    try:
        if same_bytes:
            items = place([proof_evidence.decode_item(row, Item, ROOT, scratch) for row in saved["items"]], args)
        else:
            items = plan(selection(args), args, scratch, history_weights(default_logdir()))
        # The same runtime setup a run gives its checks before their inputs are read (review
        # rv-20261020T073921Z-1e0461cc-0956 #1): without it, every check that binds the runtime's
        # environment read differently here than it ran, and valid proof was called missing.
        supply_runtime(items)
        supply_speech_models(items)
        for item in items:
            proof_evidence.prepare_environment(item, ROOT, scratch, execution_environment(item))
        snapshot = proof_evidence.InputSnapshot()
        identities = {item.label: input_identity(item, args, scratch, snapshot) for item in items}
        gaps = proof_evidence.coverage(run, items, identities, now)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    planned_before = {row["check"] for row in saved["items"]}
    proven, eligible, fresh, missing = [], [], [], []
    for item in items:
        old = outcomes.get(item.label, {})
        if item.label not in planned_before:
            missing.append((item.label, "selected now, never in the run's plan"))
        elif gaps.get(item.label) is None:
            proven.append(item.label)
        elif gaps[item.label].startswith(proof_evidence.FRESH_GAP):
            fresh.append((item.label, gaps[item.label]))
        elif gaps[item.label].startswith(proof_evidence.KNOWN_RED_GAP):
            eligible.append((item.label, gaps[item.label]))
        elif old.get("state") == "not-run":
            eligible.append((item.label, "NOT RUN (%s), the existing NOT RUN eligibility" % (
                not_run.get(item.label) or "no reason saved")))
        else:
            missing.append((item.label, "%s: %s" % (old.get("state") or "no outcome", gaps[item.label])))
    line = ("author coverage of %s (base %s, %s selection): %d planned, %d proven by the saved run, "
            "%d NOT RUN or KNOWN-RED (an existing exception, not proof), "
            "%d run fresh at every seat (never proven by a saved run), %d missing" % (
                chosen["candidate"][:12], chosen["base"][:12], chosen.get("mode") or "normal",
                len(items), len(proven), len(eligible), len(fresh), len(missing)))
    print("proof-run: " + line)
    for label, why in eligible + fresh + missing:
        print("  %s: %s" % (label, why))
    recovery = None
    if missing:
        recovery = "cd %s && python3 richos/app/scripts/proof-run.py %s --reuse %s" % (
            shlex.quote(ROOT), " ".join(setting_flags + args.proof_for_args), shlex.quote(run))
        print("  recovery, ONE runner job over the whole missing plan in this checkout (every proven check "
              "is reused, only the missing ones run): %s" % recovery)
    print("  derived in %.1f s, nothing run (%s; inputs read under %s)" % (
        time.monotonic() - started, "the run's saved plan: the checkout reads as when it started"
        if same_bytes else "selected again: the checkout changed since the run started", read_under))
    if args.summary_out:
        proof_evidence.atomic(args.summary_out, {"run": run, "line": line, "planned": len(items),
            "proven": proven, "not_run": dict(eligible), "fresh": dict(fresh), "missing": dict(missing),
            "recovery": recovery,
            "candidate": chosen["candidate"], "base": chosen["base"], "mode": chosen.get("mode"),
            "seconds": round(time.monotonic() - started, 1)})
    return 1 if missing else 0


# The exit of a run that crashed: no verdict. Not 1 (a check failed) nor 3 (a check not run), the
# two codes the merge gate reads a summary for, so a crash is refused as "not a verdict".
CRASHED = 70


def main(argv=None):
    """A crash still leaves a readable summary.json naming every planned check CRASHED, and
    exits CRASHED. 2026-10-05: a crash reusing a saved plan left none, and its exit was 1."""
    run_state = {}
    try:
        return run_main(argv, run_state)
    except Exception as exc:  # noqa: BLE001  (SystemExit and KeyboardInterrupt pass through)
        traceback.print_exc()
        reason = "%s: %s" % (type(exc).__name__, exc)
        print("proof-run: CRASHED, which is not a verdict: %s" % reason, flush=True)
        logdir = run_state.get("logdir")
        if logdir and os.path.isdir(logdir):
            rows = [{"check": it.label, "result": "crashed", "exit": None, "log": it.log}
                    for it in run_state.get("items") or []]
            path = os.path.join(logdir, "summary.json")
            with open(path, "w") as fh:
                json.dump({"crashed": reason, "checks": rows}, fh, indent=1)
            if run_state.get("summary_out"):
                shutil.copyfile(path, run_state["summary_out"])
            print("    logs: %s" % logdir, flush=True)
        return CRASHED


_GIT_EXEC_DIR = []


def without_git_exec_path(path):
    """`path` without git's own exec-path directory (`git --exec-path`), wherever it stands.

    GIT PUTS ITS EXEC-PATH ON PATH (2026-10-21). A land runs its checks, and asks the readiness
    preflight, from git's pre-merge-commit hook, and git starts every hook with its exec-path
    (/opt/homebrew/opt/git/libexec/git-core here) first in PATH; the author's run, started from a
    shell, has no such entry. So every check that binds PATH read an unchanged pass as changed at
    the land (environment.PATH, and `git` resolved inside git-core), measured in a disposable
    clone's real merge. Anything that put a directory in front of it since (the Cargo shim below,
    reserve.py) leaves it second, so it is taken out wherever it is. Without it a check runs as
    the author's did: `git` is the same git through the rest of PATH, and finds its exec-path itself."""
    if not _GIT_EXEC_DIR:
        try:
            found = subprocess.run(["git", "--exec-path"], capture_output=True, text=True, timeout=30,
                                   stdin=subprocess.DEVNULL)
            _GIT_EXEC_DIR.append(os.path.realpath(found.stdout.strip()) if found.returncode == 0
                                 and found.stdout.strip() else None)
        except (OSError, subprocess.SubprocessError):
            _GIT_EXEC_DIR.append(None)
    if not _GIT_EXEC_DIR[0]:
        return path
    return os.pathsep.join(p for p in path.split(os.pathsep) if not p or os.path.realpath(p) != _GIT_EXEC_DIR[0])


def runner_environment(environ):
    """`environ` as this runner starts every check from it, whatever launched the runner (run_main).

    THE RUNNER'S ENVIRONMENT DOES NOT DEPEND ON WHAT LAUNCHED IT (03-E). reserve.py, the documented
    way to launch heavy work, adds the Cargo environment (RUSTC_WRAPPER, SCCACHE_DIR, the cargo
    shim on PATH) before its command starts; a land hook or a shell launching this runner directly
    did not, and those values are part of check identities, so a valid author run made through
    reserve.py read as changed inputs at every land and preflight. The runner applies the same
    rule itself (cargo_identity.enable_environment, idempotent), so both launches agree. Like
    reserve.py, only where the tree holds the Cargo helpers (a test fixture's runner may not).
    And git's exec-path, which a land's hook has on PATH and a shell does not, is taken out
    (without_git_exec_path).

    MUTATION PASSES ARE OFF UNLESS THE CALLER TURNS THEM ON (2026-10-22). An engineer's handover
    run left RICHOS_MUTATION_PASSES and RICHOS_FOURTEEN_MUTANTS as the shell had them, unset, so
    every suite's own mutation harness ran in full: 22.3% of the Mac in Hank's busy window
    (richos-hq docs/research/2026-10-22-mac-system-cpu-after.md), work the land never asks for.
    A name the caller has not set takes the merge gate's value (autocheck MUTATION_SWITCH, "0");
    a value the caller set is kept, so the nightlies (nightly-engine.py PASSES_ON, nightly-local
    mutation_units, both "1") still run every pass."""
    env = dict(environ)
    for name in proof_evidence.MUTATION_SWITCHES:
        env.setdefault(name, "0")
    if "PATH" in env:
        env["PATH"] = without_git_exec_path(env["PATH"])
    if all(os.path.isfile(os.path.join(HERE, "lib", name)) for name in ("cargo_identity.py", "machine_storage.py")):
        import cargo_identity
        env = cargo_identity.enable_environment(env)
    return env


def run_main(argv, run_state):
    os.environ.update(runner_environment(os.environ))
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter,
                                usage="proof-run.py [options] [proof-for arguments]")
    p.add_argument("--commands")
    p.add_argument("--resume", help="retry the exact saved plan and validate reusable evidence")
    p.add_argument("--reuse", action="append", default=[],
                   help="validate prior evidence against the newly selected target plan and inputs")
    p.add_argument("--only-check", action="append", default=[], metavar="LABEL",
                   help="with --resume: run only this check (repeatable); the other unfinished checks stay NOT RUN here")
    p.add_argument("--retry-reason", help="diagnosis authorizing the one retry of unchanged failed inputs")
    failure_mode = p.add_mutually_exclusive_group()
    failure_mode.add_argument("--keep-going", action="store_true", help="continue independent checks (the default)")
    failure_mode.add_argument("--fail-fast", action="store_true", help="cancel unfinished checks after an ordinary failure")
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--as-printed", action="store_true")
    p.add_argument("--capacity", type=int, default=max(2, int((os.cpu_count() or 4) * 0.8)))
    p.add_argument("--engine-shards", type=int, default=max(1, (os.cpu_count() or 4) // 2))
    p.add_argument("--engine-slot-wait", type=float,
                   help="an explicit bound on the wait for the large engine-plan slot (default: none, a run waits its turn)")
    p.add_argument("--admission-wait", type=float, default=1800)
    p.add_argument("--slot-wait", type=float, default=proof_slots.DEFAULT_WAIT_SECONDS)
    p.add_argument("--max-cpu", type=float, default=reserve.DEFAULT_MAX_CPU)
    p.add_argument("--budget", type=float, default=BUDGET_SECONDS)
    p.add_argument("--deadline", type=float, default=3 * BUDGET_SECONDS)
    p.add_argument("--cap", type=float, help="a hard cap in seconds on every check (the merge gate: 600)")
    p.add_argument("--run-cap", type=float, help="the whole run ends at this many seconds")
    p.add_argument("--sample-every", type=float, default=10)
    p.add_argument("--log-dir")
    p.add_argument("--summary-out", help="also write summary.json to this file")
    p.add_argument("--without-nightly-conditions", action="store_true",
                   help="run every check in the caller's environment only (diagnosis; refused in the land checks)")
    p.add_argument("--coverage", metavar="RUN",
                   help="readiness preflight: does the saved handover RUN prove its candidate's whole selected "
                        "plan in this checkout? Reads only (author_coverage)")
    p.add_argument("--release", metavar="RUN",
                   help="remove the handover hold from RUN once nothing still needs it (HANDOVER_HOLD)")
    args, rest = p.parse_known_args(argv)
    args.proof_for_args = rest
    if args.release:
        return release_hold(args.release)
    if args.coverage:
        if rest or args.commands or args.resume or args.reuse:
            p.error("--coverage takes its selection from the saved run")
        return author_coverage(args)
    if args.without_nightly_conditions:
        if os.environ.get("RICHOS_AUTOCHECK_ACTIVE"):
            p.error("--without-nightly-conditions is refused inside the land checks: the merge gate "
                    "always meets the nightly's conditions")
        global NIGHTLY_CONDITIONS
        NIGHTLY_CONDITIONS = False
    if args.resume and (args.commands or rest or args.as_printed or args.reuse):
        p.error("--resume takes its frozen plan from the saved run; no new selection is allowed")
    if args.only_check and not args.resume:
        p.error("--only-check selects among the checks of a saved plan: it needs --resume")
    if args.capacity < 1 or args.engine_shards < 1:
        p.error("--capacity and --engine-shards must be at least 1")
    for name in ("admission_wait", "slot_wait", "max_cpu", "budget", "deadline", "sample_every"):
        value = getattr(args, name)
        if not math.isfinite(value) or value < 0 or (name != "admission_wait" and value == 0):
            p.error("--" + name.replace("_", "-") + " must be finite and positive")
    if args.max_cpu > 100:
        p.error("--max-cpu must be at most 100")
    for name in ("cap", "run_cap"):
        value = getattr(args, name)
        if value is not None and (not math.isfinite(value) or value <= 0):
            p.error("--" + name.replace("_", "-") + " must be finite and positive")
    if args.engine_slot_wait is not None and (not math.isfinite(args.engine_slot_wait) or args.engine_slot_wait < 0):
        p.error("--engine-slot-wait must be finite and nonnegative")
    parent = args.log_dir or default_logdir()
    run_id = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    logdir = os.path.join(parent, run_id if not args.log_dir else "")
    if args.log_dir:
        os.makedirs(logdir, exist_ok=True)
        if os.listdir(logdir):
            p.error("--log-dir must be empty; refusing to overwrite another run's evidence")
    else:
        os.makedirs(parent, exist_ok=True)
        logdir = tempfile.mkdtemp(prefix=run_id + "-", dir=parent)
    run_state.update(logdir=logdir, summary_out=args.summary_out)
    hist_dir = default_logdir()
    if args.resume:
        saved = proof_evidence.read_plan(args.resume)
        if saved["root"] != ROOT:
            p.error("--resume requires the original checkout")
        items = [proof_evidence.decode_item(row, Item, ROOT, logdir) for row in saved["items"]]
        units = [item.argv[item.argv.index("--only-units") + 1] for item in items if item.engine_unit]
        if units:
            os.makedirs(os.path.join(logdir, "engine-receipts"), exist_ok=True)
            with open(os.path.join(logdir, "engine-units.txt"), "w") as stream:
                stream.write("\n".join(units) + "\n")
        lines = saved["items"]
        selected = saved.get("selection")
    else:
        lines = selection(args)
        items = as_printed(lines) if args.as_printed else plan(lines, args, logdir, history_weights(hist_dir))
        selected = selected_by(args)
    run_state["items"] = items
    if not items:
        print("proof-run: the selection is empty — nothing to run. (For a documentation-only change that is"
              " the right answer; proof-for.sh says so without --quiet.)")
        return 0
    print("proof-run: %d check(s) from %d selected command(s); a budget of %d workers for the whole run, "
          "nested workers included; %.0f s budget per check, stopped at its deadline" % (
              len(items), len(lines), args.capacity, args.budget))
    for it in sorted(items, key=lambda i: -i.weight):
        print("  %-40s lane %-7s ~%5.0f s  cd %s && %s" % (it.label, it.lane or "-", it.weight,
                                                         os.path.relpath(it.cwd, ROOT), " ".join(it.argv)))
    print("  " + supply_runtime(items))
    print("  " + supply_speech_models(items, refuse=not args.dry_run))
    print("  " + conditions_line(items))
    if args.dry_run:
        return 0
    if not args.log_dir:
        hold_for_handover(logdir, selected)
    print("  logs: %s" % logdir, flush=True)
    global SLOT
    try:
        SLOT, slot_wait = proof_slots.acquire(
            {"checkout": ROOT, "logs": logdir, "selection": " ".join(argv if argv is not None else sys.argv[1:])[:300]},
            args.slot_wait, say=lambda line: print("[%s] %s" % (stamp(), line), flush=True))
    except TimeoutError as exc:
        print("proof-run: NOT ADMITTED: %s. Nothing was run. Logs: %s" % (exc, logdir), flush=True)
        # Written like any other end (2026-09-30): a caller reading summary.json sees every check
        # NOT ADMITTED by name, instead of no verdict at all, which it cannot tell from a crash.
        for it in items:
            it.state, it.rc = "not-admitted", 75
            it.notes.append("the run was not admitted to a proof-run slot: %s" % exc)
        summarize(items, 0.0, logdir, args.budget, ["not admitted to a proof-run slot: %s" % exc])
        if args.summary_out:
            shutil.copyfile(os.path.join(logdir, "summary.json"), args.summary_out)
        return 1
    try:
        slot_line = ("inside its caller's proof-run slot (a proof run started by a check of one)" if SLOT.borrowed
                     else "proof-run slot %d on this Mac (%d run(s) at once), waited %.0f s for it" % (
                         SLOT.index, proof_slots.limit()[0], slot_wait))
        print("[%s] %s" % (stamp(), slot_line), flush=True)
        # A runtime a selected check needs and this Mac does not have yet is built now, under the
        # slot, once, before any input is read (the plan above only looked). Nobody builds it by hand.
        if any(needs_runtime(it) for it in items):
            print("  " + supply_runtime(items, build=True), flush=True)
        before = source_identity()
        with open(os.path.join(logdir, "source.json"), "w") as out:
            json.dump(before, out, indent=2)
        for item in items:
            proof_evidence.prepare_environment(item, ROOT, logdir, execution_environment(item))
        def identity(item):
            return input_identity(item, args, logdir)

        def identities(selected, snapshot=None):
            snapshot = snapshot or proof_evidence.InputSnapshot()
            return {item.label: input_identity(item, args, logdir, snapshot) for item in selected}

        # The plan-time reads are kept: an invalidation note names what changed since them.
        baseline = proof_evidence.InputSnapshot()
        args.evidence = proof_evidence.Record(ROOT, logdir, items, before,
            identities(items, baseline), args.resume, source_identity, identity, identities,
            explain=lambda item: describe_input_change(item, baseline, items), current_commit=head_commit,
            selection=selected, seat=seat_reader, environment=recorded_environment(),
            as_recorded=lambda item, recorded, root, snapshot: identity_as_recorded(
                item, recorded, root, snapshot, args, logdir))
        args.pool = proof_evidence.Pool(proof_evidence.pool_directory(ROOT, hist_dir), args.evidence,
                                        args.retry_reason)
        started = time.monotonic()
        try:
            if args.resume:
                lease = proof_evidence.Lease(args.resume)
                try:
                    proof_evidence.reuse(args.resume, items, args.evidence)
                finally:
                    lease.close()
            for previous in args.reuse:
                lease = proof_evidence.Lease(previous)
                try:
                    proof_evidence.reuse(previous, items, args.evidence, exact=False)
                finally:
                    lease.close()
            if args.only_check:
                leave_unselected(items, args.only_check)
            run(items, args, logdir)
            # A completely reused plan launches no child. Its final validation
            # still has a deadline; fresh plans already started it at launch.
            begin_execution()
            args.evidence.finalize(items)
        finally:
            args.pool.close(items)
            args.evidence.close()
        wall = time.monotonic() - started
        # HEAD only: a changed file already invalidated exactly the checks whose inputs hold it.
        if head_commit() != before["commit"]:
            changed = Item("the checkout's commit changed during verification", ROOT, [])
            changed.state, changed.rc = "failed", 1
            items.append(changed)
        notes_from_logs(items)
        rc = summarize(items, wall, logdir, args.budget, [slot_line] + list(getattr(args, "monitor_lines", ())))
        if args.summary_out:
            shutil.copyfile(os.path.join(logdir, "summary.json"), args.summary_out)
    finally:
        SLOT.release()
        SLOT = None
    if not args.as_printed:
        record_weights(hist_dir, items)
    if not args.log_dir:
        rotate(parent)
    return rc


def as_printed(lines):
    """The selection exactly as proof-for.sh printed it, one command after another: the shape a
    hand-written loop runs, kept so the runner's own saving can be measured on any selection."""
    items = []
    for n, line in enumerate(lines, 1):
        m = re.match(r"^cd (\S+) && (.+)$", line.strip())
        if not m:
            raise SystemExit("proof-run: cannot read line %d of the selection: %r" % (n, line))
        argv = shlex.split(m.group(2))
        it = Item("%02d %s" % (n, slug(" ".join(argv))[:36]), os.path.join(ROOT, m.group(1)), argv,
                  "as-printed", float(len(lines) - n + 1))
        items.append(it)
    return items


def record_weights(state, items):
    """What each check measured, so the next run plans its lanes and starts the long poles first
    from the median of its last MEDIAN_SAMPLES executions (see MEDIAN_SAMPLES). Only executions
    that reached a verdict count: a check stopped at a cap ran for the cap, not for its length,
    and a reused result did not run at all. `engine receipts` reads files; it is not measured."""
    os.makedirs(state, exist_ok=True)
    samples = history_samples(state)
    for it in items:
        if (it.state in ("passed", "failed") and it.started and it.ended and it.label != "engine receipts"
                and not getattr(it, "reused_from", None)):
            samples[it.label] = (samples.get(it.label, []) + [round(it.seconds, 1)])[-MEDIAN_SAMPLES:]
    path = os.path.join(state, "weights.tsv")
    with open(path + ".new", "w") as fh:
        for k in sorted(samples):
            fh.write("%s\t%s\t%s\n" % (k, round(statistics.median(samples[k]), 1),
                                       ",".join(str(v) for v in samples[k])))
    os.replace(path + ".new", path)


if __name__ == "__main__":
    sys.exit(main())
