#!/usr/bin/env python3
"""Manually check, build, publish or release a nightly using this Mac's own signing keys.

Nothing installs a job, watches main or runs on a schedule. `release` builds and
publishes a nightly in one motion. `build` stops at a signed, notarized,
engine-pinned candidate for QA to walk; only an explicit later `publish --run
<run-id>` makes that candidate's release installable and moves the update
channel. `candidate --run <run-id>` prints what a walker needs to open it.
Only `release` and `publish` may publish. Keys remain in the local
Keychain/private files.
"""
import argparse
import base64
from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import json
import os
from pathlib import Path
import platform
import re
import select
import shlex
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import threading
import time
import uuid

# Gate ceilings include rebuilds, not just reuse of successful receipts. Cargo
# uses fivefold cold/warm headroom rounded up to a minute. The script envelope
# adds cold Cargo runs and the generated-command integration to a full no-reuse
# reference before doubling it. Script/UI ceilings round up to five minutes.
# Measurements and model assumptions are recorded in the private handoff.
# Gates skipped by a land proof do not spawn a command or spend a deadline.
# Every ceiling here, and CLEANUP_TIMEOUT, is spent on EXECUTION only: owned_run() starts it
# when the command is admitted to a machine worker token, never while it queues for one.
GATE_BUDGETS = {
    "gates/release-smoke": 60,   # One-minute startup floor for fixture-only Python work.
    "gates/core-tests": 300,    # At least 5x the worst cold/warm Cargo sample.
    "gates/updater-tests": 360, # At least 5x its separate workspace's cold/warm sample.
    # UNVERIFIED SINCE 2026-09-26: this envelope was derived WITH the iPhone simulator suites,
    # which `run-tests.sh --for desktop` no longer runs here (phone-app-suites.tsv), and every
    # simulator figure below is theirs. The ceiling is left where it was rather than cut to a
    # number nobody has measured (the 1800 s it replaced killed a run that was passing). One
    # full no-reuse run of the desktop-only gate is what would settle it. The derivation as
    # it stood:
    # At least 2x the full-execution envelope, without reuse. Re-derived 2026-09-25 when the
    # nightly-only A8 case made the simulator suites the long pole: the machine admits one
    # prepared-simulator lease at a time, so native-ios-app (1572 s alone with A8, measured
    # 2026-09-25) and native-ios-ui (two devices at 787-884 s each, measured 2026-09-24) run one
    # after the other -- native-ios-ui sat waiting for 16 minutes in run 20260925T173516Z-218870fc
    # -- while every other suite finishes in parallel inside the first 7.3 minutes. Envelope
    # about 3340 s; 2x is 6680 s, rounded up to five minutes. The 1800 s it replaces predates A8
    # (the whole gate took 413 s on 2026-09-22) and killed that run with the suite still passing.
    "gates/script-suites": 6900,
    # A hang guard over `lint.sh --all`, whose own caps decide its verdict: Tauri Clippy's
    # 180 s of work (lint/driver.py TAURI_CAP) plus up to 1800 s waiting for Cargo's lock
    # (lint/rust.py LOCK_WAIT_GUARD, not counted in that cap since 2026-09-29, audit R12),
    # plus 120 s for the static checks and tool versions (measured whole gate: 5-11 s). The
    # 300 s this replaces sat INSIDE the lock-wait guard and would have refused a Clippy
    # queued behind another build for two minutes before it compiled a line.
    "gates/lint-tauri": 2100,
    # Re-derived 2026-09-25: the unit took 1291 s alone (run 20260925T201442Z-f9f0f9b9), and it ran
    # past 1800 s twice beside the simulator suites once gates ran at once (runs 20260925T222652Z-723fee53
    # and 20260925T225819Z-2a11fdd0). 792 s (2026-09-23) is stale; this is about 2x the contended envelope.
    "gates/workspace-mutants": 3600,
    # Re-derived 2026-09-29, when each UI shard started holding a machine worker token of its own
    # (ui/tests/run.js): beside a mutation pool the shards now wait for tokens instead of running
    # uncounted, and the progress guarantee is ONE shard at a time on the gate's own token, i.e.
    # the inventory's serial cost -- 1512 s by the committed suite-weights.tsv. 2x is 3024 s,
    # rounded up to five minutes. The 1200 s it replaces was 2x the four-shard reference on an
    # otherwise free Mac, which the gate never gets beside workspace-mutants.
    "gates/ui-suite": 3300,
    "gates/privacy-sweep": 120, # At least 4x a full scan; no receipt-reuse assumption.
}
CLEANUP_TIMEOUT = 30
TERM_GRACE = 15
KILL_GRACE = 2


class CommandCleanupError(RuntimeError):
    """A command did not surrender its owned group before the cleanup deadline."""


def finish_group(process):
    sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
    from owned_command import finish_group as finish
    return finish(process, term_grace=TERM_GRACE, kill_grace=KILL_GRACE,
                  error_type=CommandCleanupError)


class GateStopped(RuntimeError):
    """A gate's command was not started because another gate already refused the build."""


class OwnedGroups:
    """The process groups this run started and still owns, so one failure can stop the rest.

    Only groups registered here at spawn are ever signalled -- the pid Popen returned, never
    one found by name. `stop()` sends each live group the SIGTERM that finish_group() sends
    first anyway; the thread that owns the command then sees it end and runs finish_group()'s
    bounded TERM-then-KILL cleanup exactly as it does on a timeout. Cleanup commands (the UI
    suite's `git status`/`git checkout` that put the source tree back) are registered as
    such and are left to finish: tidying up after a stop is not something a stop cancels.
    """

    def __init__(self):
        self._lock = threading.Lock()
        self._live = {}
        self.stopping = False

    def add(self, process, cleanup=False):
        with self._lock:
            self._live[process.pid] = (process, cleanup)
            late = self.stopping and not cleanup
        if late:
            # Spawned in the instant between the stop and this registration: it goes too.
            self._term(process)

    def discard(self, process):
        with self._lock:
            self._live.pop(process.pid, None)

    def stop(self):
        with self._lock:
            self.stopping = True
            live = [process for process, cleanup in self._live.values() if not cleanup]
        for process in live:
            self._term(process)

    @staticmethod
    def _term(process):
        if process.returncode is not None:
            return  # Already reaped by its owner: the id is no longer ours to signal.
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except (ProcessLookupError, PermissionError):
            pass


class GateTimedOut(RuntimeError):
    pass


class GateNotAdmitted(RuntimeError):
    pass


def owned_run(args, **kwargs):
    # Keep nightly's cleanup/group ownership while sharing its admission clock with commits.
    library = Path(__file__).resolve().parents[2] / "engine/scripts/lib"
    sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
    from owned_command import owned_run as admitted_run
    return admitted_run(args, library=library, finish=finish_group, **kwargs)


ROOT = Path(__file__).resolve().parents[3]
REPO = "WebDevBooster/richos"
SCRIPTS = Path("richos/app/scripts")

# THE ONLY DECLARED HOST GAP OF THIS BUILD. `run-tests.sh` discovers every *.test.sh beside
# it from disk and runs each with no arguments; a suite that answers "this host cannot
# answer" (exit 2) is tolerated ONLY when its caller names it here WITH a reason, is RED
# when undeclared, and is RED AGAIN on the day the suite starts answering — so this is an
# allowance that cannot outlive its reason (run-tests.sh:20-92). Exit 2 is never read as a
# skip anywhere; this is the whole of the tolerance.
#
# It is written HERE, in the file, rather than taken from the environment on purpose:
# local_environment() drops RUN_TESTS_DECLARED_GAPS out of the operator's shell (see the
# pop below) so a stray export can never declare a gap the build's own source does not.
DECLARED_GAPS = (
    "front-door.test.sh: this suite drives the SHIPPED window — it needs a built RichOS.app, "
    "which THIS build has not produced yet at the gate stage, and a person's unlocked screen to "
    "put it on (its own P1/P2 refuse a sleeping or locked one). There is no app for it to drive "
    "on a build host, so it reports the gap and launches nothing. It is run by hand, or from a "
    "walk, against a published release: scripts/front-door.test.sh --release <release dir>. "
    "Measured on this Mac, 2026-09-18: a bare invocation exits 2 in nine lines and no process."
)

# WHAT THE LAND ALREADY PROVED, and therefore the gates a candidate build may be told to
# skip. A land runs, from the main checkout, on the exact commit that then becomes `main`
# (the commit this build fetches), the checks `proof-for.sh` selects for its change.
#
# `--checks-done-at-land <sha>` REFUSES when the sha it is handed is not the sha this run
# actually fetched. A flag that trusted its own argument would let a stale sha wave a
# different tree's tests through, which is worse than having no flag: the skip has to be
# unable to lie, not merely documented as honest.
#
# THE SHA SAYS WHICH TREE WAS CHECKED, NEVER HOW MUCH OF IT (hunt part 2, finding 24). So
# each skip also needs evidence, on this machine, that the land ran the WHOLE gate on this
# tree; a build must not pay for a proof the land already made, nor accept one nobody made:
#
#   `gates/core-tests`: autocheck's land receipt for the fetched commit's tree has to list
#     `cargo test -p richos-core` with no filter, as run and passed (LAND_CORE_COMMAND).
#     This used to go on the sha alone, "because the land always runs them". It does not:
#     proof-for.sh selects only the `--test <target>` or `--lib <module>::` tests a change
#     touches, and the whole crate only when its manifest changes, so a matching sha with a
#     module's tests behind it skipped every other core test.
#   `gates/ui-suite`: a coverage proof for that sha, written by the land's own
#     `run.js --coverage --proof-out` on a pass.
#
# No evidence, no skip: the gate simply runs, which is the safe direction for a missing
# file to fail in.
#
# Nothing else in gates() is skippable, because nothing at the land runs it.
# `richos-user-update` is the updater's own crate; `run-tests.sh` is the packaging,
# signing, updater and release-chain suites; `named-persons.sh --tree` is the privacy sweep
# over the tree that is about to be published. Those are exactly the invariants a land does
# NOT check, so they stay on the candidate path.
#
# The default is unchanged. When the skip is taken it is written into the run log and into
# `build-info.json` (through the plan, which nightly.py copies into the candidate's
# provenance), so a candidate can never quietly claim a gate it did not run.
LAND_PROVEN_GATE = "gates/core-tests"
# The line proof-for.sh writes, and autocheck.py records in a land receipt's `commands`, when
# a land runs the whole richos-core crate; any filter after it (`--test`, `--lib m::`) is a
# subset and does not count. The receipt lives where autocheck.py's `land_receipt` puts it.
LAND_CORE_COMMAND = "cd richos/app && cargo test -p richos-core"
LAND_RECEIPTS = Path("richos-autocheck") / "land"

# WHAT `--no-host-screen` IS FOR, in the CEO's words, 2026-09-19:
#
#     "So, every engineer will keep opening the app making me unable to do anything here
#      or WHAT???"
#
# `gui-boot.test.sh` boots the real app on the real screen for ~162 s of every build
# (measured, run 20260919T180454Z-ac11d13e), and on a Mac somebody is working on that is
# not a test, it is an interruption. `--no-host-screen` hands `run-tests.sh` the flag that
# holds back every suite that boots the shipped binary, and the candidate is built,
# notarized and walkable without one pixel reaching this machine.
#
# THE COST IS REAL AND IT IS NOT HIDDEN. Such a candidate carries no gui-boot result, so
# `publish` REFUSES it until `--gui-proof <path>` names one taken against the same commit
# (`scripts/gui-proof-in-vm.sh` produces that file from the candidate's own bundle, inside
# a guest, with nothing on this screen). A mode that quietly published an unproven boot
# would be worse than the interruption it saves.
#
# IT IS ACCEPTED FOR `build` AND NOT FOR `release`. `release` is build-and-publish in one
# motion with no `publish` step to refuse anything, so the one command that can make a
# build installable always runs the whole gate. The screenless path is build -> walk ->
# publish --gui-proof, which is the documented flow anyway.
NO_HOST_SCREEN_COMMANDS = ("build",)

# AND IT IS THE DEFAULT FOR `build`, WITH THE PROOF TAKEN BY THE BUILD ITSELF (2026-09-29).
# The CEO: *"how much retarded fuckery is needed to get a build done THE FIRST FUCKING TIME if
# the Mac is "quiet"???"* That day's attempt 1 (run 20260929T100554Z-3155ee52) was started
# without the flag, ran gui-boot.test.sh on this Mac's own display, and failed because the
# display was asleep. His Mac is never the on-screen host (§65); the test VM is. So `build`
# holds every screen suite back unless `--host-screen` is said out loud, and once the signed
# candidate exists the build boots THAT bundle in a test-VM guest (`gui-proof-in-vm.sh`,
# suite=shipped-bundle-boot) and keeps the proof where `publish` finds it without being told.
# Nobody sets a variable, remembers a flag or runs a second command.
#
# WHAT IT DOES NOT DO, SAID HERE SO NOBODY FINDS IT LATER: gui-boot.test.sh itself (a debug
# binary built from the checkout, B0-B8/C1-C5) is still recorded NOT RUN (no screen) here: the
# gates do not hand RICHOS_GUI_HOST to `run-tests.sh`. Given it, `run-tests.sh` runs the suite in
# a guest through testvm/run-suite.sh (2026-09-30), which builds the two executables on this Mac
# because the guest has no cargo and no repository. The proof `publish` accepts is the shipped
# bundle's boot.
VM_PROOF_PHASE = "vm-boot-proof"
# gui-proof-in-vm.sh waits up to 3600 s for one of the two guest slots (its --wait default,
# chosen so a build that is walked away from still gets its proof), then boots, watches and
# cleans up a guest; a measured boot-to-verdict is minutes. The budget is the wait plus 20
# minutes, so the one thing that can end it is a guest that never answers.
VM_PROOF_BUDGET = 3600 + 1200

# A SETTING GIVEN TO THE BUILD IS NEVER SILENTLY DROPPED (2026-09-29). Attempt 2 that day was
# started as `RICHOS_GUI_HOST=richos-test-1 nightly-local.py build --no-host-screen`; every
# gate runs in the allowlisted environment below (GATE_PASSTHROUGH), the variable never
# reached `run-tests.sh`, and the run went on for minutes to a screenless result nobody had
# asked for. So at start every name in this project's setting namespaces is accounted for:
# handed to the gates, consumed by a step of this script, or refused before anything runs,
# with the reason and the way out. Names the agent harness gives every process on this Mac
# are not settings and are named, with that reason, in AMBIENT_NAMES.
SETTING_PREFIXES = ("RICHOS_", "RUN_TESTS_", "TESTVM_", "TART_")
GUI_HOST_ENV = "RICHOS_GUI_HOST"
# The test-VM tooling's own settings (testvm/lib.sh, testvm/slots.py): handed to the VM boot
# proof, the one step of a build that uses a guest.
VM_STEP_PREFIXES = ("TESTVM_", "TART_")
AMBIENT_NAMES = {
    "RICHOS_AGENT_OWNER": "the agent harness's name for the session that started this process",
    "RICHOS_AGENT_SESSION": "the agent harness's session id for this process",
}


def settings_this_build_drops(environ, command):
    """The setting-shaped names in `environ` that no step of `command` would receive."""
    dropped = []
    for name in sorted(environ):
        if not name.startswith(SETTING_PREFIXES):
            continue
        if name in GATE_PASSTHROUGH or name in AMBIENT_NAMES or is_credential(name):
            # A credential (RICHOS_NOTARY_*, RICHOS_SIGNING_IDENTITY, ...) is read from the
            # shell ON PURPOSE and handed to the signing steps only: delivered, not dropped.
            continue
        if command == "build" and (name == GUI_HOST_ENV or name.startswith(VM_STEP_PREFIXES)):
            continue
        if command != "build" and name.startswith(VM_STEP_PREFIXES):
            # No step of any other command touches a guest; the name has no consumer to be
            # dropped on the way to. (A TESTVM_ROOT kept in a shell profile is ordinary.)
            continue
        dropped.append(name)
    return dropped

# `run-tests.sh` writes this beside the plan; its contents travel into the candidate's
# `build-info.json`, so a candidate can never quietly claim a suite it did not run.
SUITE_RESULTS = "script-suites.json"

# ── The UI suite, and why it is a build gate at all ──────────────────────────────────────
#
# UNTIL NOW IT GATED NOTHING. `app/ui/tests/run.js` holds 55 suites and 863 checks over the
# screens the CEO actually looks at, and no script in `app/scripts/` invoked it -- measured
# at bf1381e0, `grep -n run.js scripts/*.sh scripts/*.py` matched three COMMENTS and no call.
# A build could be signed, notarized and published with every one of those checks red.
#
# WHAT IT COSTS, MEASURED ON THIS MAC (M4, 4 performance cores), 2026-09-20 at bf1381e0:
# 1510 s serially, 557 s over 4 shards before the packing was fixed, 378 s after. Four is
# the shard count because four is the performance-core count; the packer reports the same
# 378 s for all four shards, which is the arithmetic floor for this inventory at this count
# (total/4), and no shard count at all gets under the heaviest single suite, contrast.js at
# 150 s.
UI_TESTS = Path("richos/app/ui/tests")
UI_SUITE_GATE = "gates/ui-suite"
WORKSPACE_MUTANTS_GATE = "gates/workspace-mutants"
# The engine units whose mutation passes run only under RICHOS_MUTATION_PASSES=1, at that gate
# (hunt part 4 finding 19): every land runs their behavioral checks, never their mutants.
# mega-lander/tests/app.test.sh takes the same switch. It joined on 2026-09-30, once its
# behavioral half stopped failing under ci-shard.sh with RECORD-TOUCHED: the suite now points
# the land-lock home inside its fixture (693528de, PASS 172.4 s, ci-shard 1/1 unit passed).
MUTATION_PASS_UNITS = (
    "mega-lander/tests/workspaces.test.sh",
    "mega-lander/tests/create-teammate-worktree.test.sh",
    "mega-lander/tests/workspace-probes.test.sh",
    "mega-lander/tests/app.test.sh",
)
UI_SHARDS = 4

# ── Quarantine: the list that keeps test rot from stopping a build ───────────────────────
#
# THE CEO'S QUESTION, 2026-09-19, and it is the right one to hold this against: "Will that
# get me back to that endless fixing of millions of bugs without doing any actual work?"
#
# A gate is worth having only if a red suite means the APP is wrong. When a suite is red
# because its own assertion, fixture or screenshot has drifted, stopping the build buys
# nothing and costs a night. So a named suite here still RUNS, is still reconciled, and is
# still printed in this build's output on every run -- it just does not hold the build.
#
# EVERY ROW CARRIES A FILE, A REASON AND A DATE, because a quarantine is a debt and an
# undated debt is a permanent one. `run.js` prints a row that is no longer needed as a NOTE
# naming the row to delete, so the list cannot rot silently, and prints a row naming a suite
# that no longer exists for the same reason.
#
# IT IS EMPTY, AND THAT IS A MEASUREMENT, NOT AN ASPIRATION. The full inventory was run at
# bf1381e0 on 2026-09-20: 55 of 55 suites ran, 863 checks observed, every suite exited 0 and
# the coverage job reconciled all four shards green. There is nothing to quarantine today.
UI_QUARANTINE = ()

# ── How much runs at once: two decisions the operator makes on every build ───────────────
#
# THE CEO, 2026-09-25: "why the fuck do I fucking have to wait for ONE FUCKING HOUR WHEN THE
# WHOLE FUCKING MAC IS FREE???" -- and, asked what happens with ten engineers working: the
# number is chosen when the build is started, from what is running on the Mac at the time.
# Then: "how do I know that you won't fuck this up next time?" So there is NO DEFAULT for the
# decision a build actually uses: `build`, `release` and `stable` refuse to start unless
# --gates-at-once is named on the command line, and the run log's first lines say what was
# chosen and by whom. A forgotten decision is a refusal nobody can miss, not an hour of a free
# Mac running things in a line.
#
#   --gates-at-once N|all   how many of the gates below run at the same time. 1 is the old
#                           order exactly (one after another, inline, in GATE ORDER below);
#                           `all` starts every gate whose inputs are ready.
#   --simulated-phones N    how many prepared iPhone simulators the suites may lease at once
#                           (testdevices.py pool_leases). Handed to the script-suites gate as
#                           RICHOS_IOS_POOL_LEASES; every other caller keeps one.
#                           Since 2026-09-26 no suite in the desktop build leases one (the
#                           phone apps' suites left it, phone-app-suites.tsv), so the number
#                           changes nothing, and it is NOT required (hunt part 2 v3, R48; the
#                           CEO's order of 2026-10-04 to fix it settles the question this note
#                           used to leave to the command lines' owner). Still accepted, and
#                           recorded when given; when it is not, the log says so and a suite
#                           that ever leased one would get one at a time, stated literally.
#
# Neither replaces a backstop. Every command still takes a machine worker token
# (worker_tokens.py), a simulator boot still passes simulator_budget.py's live/boot limits
# and its CPU/memory check, and cargo still serializes builds on one target directory.
# ── Resuming after the gates: `--gates-passed-in <run-id>` ──────────────────────────────────
#
# THE CEO, 2026-09-25: "use everything he did before". Run 20260928T190111Z-40a16163 passed
# all eight gates on 9fbf4332 (about 38 minutes of them), and then the CPU guard stopped the
# compiler in the build step. Running those gates again proves nothing new about 9fbf4332.
#
# `--gates-passed-in RUN` (build or release) builds THAT run's source commit -- not main's
# tip -- and runs no gate, and it refuses unless RUN's own log shows, from the coordinator's
# own lines: the source it fetched, every gate below PASSED (or, one at a time, ended), no
# gate FAILED, STOPPED or SKIPPED, and the run went on to `plan-recheck`, which it only
# reaches when gates() returned. `release` further requires RUN to have been a release or
# stable run, because a `build` run's script suites may skip over unchanged inputs and
# `release` never does. The script-suites record in the state directory must still be RUN's,
# because it travels into the candidate's provenance. The resumed run records each gate as
# SKIPPED, naming RUN, so a resume can never be resumed from: the chain always points at
# the run that actually ran the gates.
PASSED_GATES_FLAG = "--gates-passed-in"
GATE_NAMES = ("gates/release-smoke", LAND_PROVEN_GATE, "gates/updater-tests", "gates/script-suites",
              "gates/lint-tauri", WORKSPACE_MUTANTS_GATE, UI_SUITE_GATE, "gates/privacy-sweep")

GATES_AT_ONCE_FLAG = "--gates-at-once"
SIMULATED_PHONES_FLAG = "--simulated-phones"
SIMULATED_PHONES_ENV = "RICHOS_IOS_POOL_LEASES"
GATE_COMMANDS = ("build", "release", "stable")

# WHICH GATE CONSUMES WHAT ANOTHER GATE PRODUCES. Everything not listed here reads nothing
# another gate writes and starts at once; these wait, and only for the gates named. The order
# of the gates themselves (the old one-after-another order, which `--gates-at-once 1` keeps)
# is written in gates().
GATE_AFTER = {
    # `lint.sh --all --suite-results` reads the script-suites receipt to skip the fast and
    # Tauri lint that lint.test.sh's own `lint.sh --all` already passed in this build
    # (lint/driver.py fast_was_run; hunt part 2 v3, R47).
    "gates/lint-tauri": ("gates/script-suites",),
    # The privacy sweep waited here for the UI suite until 2026-09-29, because the suite
    # rewrote committed screenshots in the tree the sweep scans. The suite now runs in its own
    # worktree (Runner.ui_checkout) and writes nothing in that tree, so the sweep starts at once.
}

# The inside of the one long `nightly.py build` step, matched IN ORDER against the lines
# its children stream past TimestampedLog, so the split is readable without coupling this
# script to a child's internals beyond these strings.
#
# IN ORDER is the protection that does the work: the first marker gates every later one,
# so a pattern cannot fire early unless the one ahead of it already has. Measured against
# the real 3,158-line log of run 20260919T180454Z-ac11d13e, NONE of the eleven patterns
# matches any of the 2,727 lines before the build step -- so arming the matcher for the
# duration of that step (phase(), below) changes nothing today, and this comment used to
# claim a collision it does not have. It is kept as the second condition because the
# suites' whole job is to drive the scripts these markers come from:
# `make-engine-asset.test.sh` and `make-release.test.sh` run `make-engine-asset.sh` and
# `make-release.sh`. The day one of them stops shimming a banner, the first marker fires a
# thousand lines early and every segment after it is wrong. It costs one boolean.
#
# A marker that never appears is reported as "not observed" rather than folded into the
# segment beside it, because a silently merged boundary makes one phase look expensive and
# hides another entirely.
BUILD_MILESTONES = (
    ("build/engine-asset", r"^building the engine asset for "),
    ("build/engine-asset-recheck", r"^=== --check: building a second time"),
    ("build/engine-member-audit", r"^=== every member accounted for"),
    ("build/engine-verify-download", r"^fetching the PUBLISHED asset"),
    ("build/app-preflight", r"^building RichOS \S+ for "),
    ("build/app-compile", r"^building \(release\) with RICHOS_REQUIRE_REAL_ICONS"),
    ("build/app-bundle-and-sign", r"^\s*Finished `release` profile"),
    ("build/app-notarize", r"^\s*submitting to Apple's notary service"),
    ("build/app-staple-and-verify", r'^\s*\{"status":"Accepted"'),
    ("build/app-archive", r"^OK: RichOS\.app is bundled,"),
)


def stamp(epoch):
    return datetime.fromtimestamp(epoch, timezone.utc).strftime("[%Y-%m-%dT%H:%M:%S.") \
        + f"{int(epoch % 1 * 1000):03d}Z]"


class TimestampedLog:
    """The run log, every line of it stamped with UTC wall clock as it is written.

    A log that says WHAT happened but never WHEN cannot answer where a build's twenty
    minutes went. On 2026-09-19 the only available answer was the mtimes of the staging
    directory, which stop at four coarse boundaries and say nothing at all about the eight
    and a half minutes of gates that run before the first of those files is written.

    Children are handed the write end of a pipe (via `fileno()`, so no call site changes)
    rather than the file itself, and one reader thread stamps each line and writes it
    through. Nothing any child prints has to change, now or ever, for its phase to become
    timeable -- which is the whole point of putting this here instead of threading a timer
    through nightly.py, make-release.sh and package-app.sh, none of which this script even
    supplies (they are read from the FETCHED tree, not from this checkout).

    Undecodable bytes are replaced rather than raised: a reader thread that dies takes the
    log with it and leaves the build writing into a full pipe until it blocks forever.

    A PROCESS CAN OUTLIVE THE COMMAND THAT STARTED IT and keep the pipe's write end, so
    close() cannot always wait for end of file. It used to close a buffered reader while
    the pump thread was inside it: close() then waited for that process to write or exit,
    and Python 3.14 raised `PyMemoryView_FromBuffer(): info->buf must not be NULL` in the
    pump (run 20260928T143332Z-1f0e1b3a). The pump now reads the raw descriptor with a
    short select() and stops when told; the descriptor is closed only after it has.
    """

    SENTINEL = "richos-nightly-log-sync-"
    # How long close() waits for end of file before it stops reading, and says so.
    CLOSE_WAIT_SECONDS = 30

    def __init__(self, path, milestones=()):
        self.path = Path(path)
        self.file = self.path.open("w")
        self._read_fd, self._write_fd = os.pipe()
        self._stop = threading.Event()
        self._milestones = list(milestones)
        self._next = 0
        self._armed = False
        self._awaited = None
        self._lock = threading.Lock()
        self._file_lock = threading.Lock()
        self._synced = threading.Event()
        self.seen = []
        self._thread = threading.Thread(target=self._pump, daemon=True)
        self._thread.start()

    def _pump(self):
        pending = b""
        while not self._stop.is_set():
            ready, _, _ = select.select([self._read_fd], [], [], 0.2)
            if not ready:
                continue
            chunk = os.read(self._read_fd, 65536)
            if not chunk:
                break
            *lines, pending = (pending + chunk).split(b"\n")
            for raw in lines:
                self._take(raw)
        if pending:
            self._take(pending)
        with self._file_lock:
            self.file.flush()

    def _take(self, raw):
        now = time.time()
        line = raw.decode("utf-8", "replace")
        with self._lock:
            if line == self._awaited:
                self._synced.set()
                return
            if self._armed and self._next < len(self._milestones):
                name, pattern = self._milestones[self._next]
                if re.search(pattern, line):
                    self.seen.append((name, now))
                    self._next += 1
        with self._file_lock:
            self.file.write(f"{stamp(now)} {line}\n")

    def fileno(self):
        """Every existing `stdout=self.log` / `stderr=self.log` call site keeps working."""
        return self._write_fd

    def sync(self):
        """Block until everything written so far has been read by the pump thread.

        The pipe is asynchronous, so flipping the matcher off the instant a child exits
        would race that child's last lines: they are already in the pipe and not yet
        matched, and the milestone they carry would be lost. A sentinel pushed through the
        same pipe is the only ordering guarantee available, because the pump reads in
        order -- when the sentinel comes back, everything ahead of it has been seen.
        """
        token = self.SENTINEL + uuid.uuid4().hex
        with self._lock:
            self._awaited = token
            self._synced.clear()
        os.write(self._write_fd, (token + "\n").encode())
        self._synced.wait(timeout=30)
        with self._lock:
            self._awaited = None

    def arm(self, armed):
        self.sync()
        with self._lock:
            self._armed = armed

    def write(self, text):
        """This script's own lines go down the same pipe, so ordering is one thread's.

        Only ever called at a phase boundary, when no child holds the write end, so a line
        longer than PIPE_BUF cannot interleave with a child's output.
        """
        os.write(self._write_fd, text.encode("utf-8", "replace"))

    def append_verbatim(self, text):
        """Put lines that are ALREADY stamped straight into the file, in one piece.

        A gate that ran beside others wrote its own stamped log (Runner.run_gates); this is
        how that log becomes one unbroken section of the run log, every line keeping the time
        it was written rather than the time it was copied. Everything already in the pipe is
        written first (sync), so the section lands after the lines that announced it.
        """
        self.sync()
        with self._file_lock:
            self.file.write(text if not text or text.endswith("\n") else text + "\n")
            self.file.flush()

    def close(self):
        os.close(self._write_fd)
        self._thread.join(timeout=self.CLOSE_WAIT_SECONDS)
        if self._thread.is_alive():
            self._stop.set()
            self._thread.join()
            with self._file_lock:
                self.file.write(
                    f"{stamp(time.time())} [log] stopped reading {self.CLOSE_WAIT_SECONDS} s after "
                    "the last command ended: a process it started still holds this log's pipe, "
                    "and nothing it writes from here on is recorded\n")
        os.close(self._read_fd)
        self.file.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
        return False


class _BufferedGateLog:
    """A gate's own log when the run log is a plain stream rather than a TimestampedLog.

    It has a real descriptor, because command() hands the log to a child as its stdout.
    """

    def __init__(self):
        self._file = tempfile.TemporaryFile()

    def fileno(self):
        return self._file.fileno()

    def write(self, text):
        os.write(self._file.fileno(), text.encode("utf-8", "replace"))

    def read(self):
        self._file.seek(0)
        text = self._file.read().decode("utf-8", "replace")
        self._file.close()
        return text


def private_file(path):
    path = Path(path).expanduser().resolve(strict=True)
    mode = path.stat()
    if not path.is_file() or mode.st_uid != os.getuid() or stat.S_IMODE(mode.st_mode) & 0o077:
        raise ValueError(f"expected an owner-private file: {path}")
    return path


def notary_environment(path):
    """Parse assignments as data. Never source a shell file or execute its values."""
    allowed = {"RICHOS_NOTARY_KEY", "RICHOS_NOTARY_KEY_ID", "RICHOS_NOTARY_ISSUER",
               "RICHOS_NOTARY_PROFILE"}
    result = {}
    for line in private_file(path).read_text().splitlines():
        tokens = shlex.split(line, comments=True)
        if tokens[:1] == ["export"]:
            tokens = tokens[1:]
        if not tokens:
            continue
        if len(tokens) != 1 or "=" not in tokens[0]:
            raise ValueError("notary.env must contain plain NAME=value assignments")
        name, value = tokens[0].split("=", 1)
        if name not in allowed or any(c in value for c in ('`', '$(', '\n', '\r')):
            raise ValueError("unsupported notary.env assignment")
        if name == "RICHOS_NOTARY_KEY":
            value = value.replace("${HOME}", str(Path.home())).replace("$HOME", str(Path.home()))
            value = str(private_file(value))
        elif "$" in value:
            raise ValueError("variable expansion is not supported for notary credentials")
        result[name] = value
    if not result.get("RICHOS_NOTARY_PROFILE") and not all(result.get(k) for k in
            ("RICHOS_NOTARY_KEY", "RICHOS_NOTARY_KEY_ID", "RICHOS_NOTARY_ISSUER")):
        raise ValueError("notarization credentials are incomplete")
    return result


CREDENTIAL_PREFIXES = ("RICHOS_NOTARY_", "TAURI_SIGNING_", "APPLE_")
CREDENTIAL_NAMES = ("RICHOS_NOTARIZE", "RICHOS_SIGNING_IDENTITY")


def is_credential(name):
    return name.startswith(CREDENTIAL_PREFIXES) or name in CREDENTIAL_NAMES


def split_credentials(env):
    """Remove the signing and notarization variables from `env` and return them.

    Only the steps that actually sign or notarize are given them back. The gates --
    cargo test, the twelve script suites, the privacy sweep -- run without them, and
    that is a correctness rule before it is a secrecy one. On 2026-09-16 the first
    nightly attempt died on four package-app.test.sh cases that refuse when notary
    credentials are absent or half-supplied: the credentials were not absent, because
    this function's predecessor put the operator's real App Store Connect key into
    every subprocess. A suite that can see a credential can also print it into a log
    under ~/.richos-nightly/logs, which is the second reason and the smaller one.
    """
    return {name: env.pop(name) for name in list(env) if is_credential(name)}


IDENTITY_OVERRIDES = ("GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL", "GIT_AUTHOR_DATE",
                      "GIT_COMMITTER_NAME", "GIT_COMMITTER_EMAIL", "GIT_COMMITTER_DATE",
                      "EMAIL")


def strip_identity_overrides(env):
    """Remove a shell-exported commit identity from `env` and return it.

    The release commit is authored with this Mac's CONFIGURED git identity. These
    variables override every level of git config, so leaving them in place would let
    a stray `export GIT_AUTHOR_EMAIL=...` author the release as someone else with
    neither the configuration nor this file saying so -- and `git config user.email`
    would still report the configured address. richos-core does the same thing to the
    engine's environment (crates/richos-core/src/engine_profile.rs:198-200).
    """
    return {name: env.pop(name) for name in IDENTITY_OVERRIDES if name in env}


# WHAT A GATE MAY SEE FROM THE OPERATOR'S SHELL. AN ALLOWLIST, AND THE INVERSION IS THE
# WHOLE POINT.
#
# This was `os.environ.copy()` minus four names, and four names is a GUESS about which of
# the operator's exports can break a build. The guess has to be re-made, correctly, every
# time anyone adds a variable to any script in the chain -- and on 2026-09-19 it was wrong,
# which is what broke that day's builds. A deny-list is a list of the mistakes somebody has
# already made; an allowlist is a statement of what the work needs, and a variable nobody
# thought about is excluded by construction instead of included by construction.
#
# MEASURED ON THIS MAC, 2026-09-20: a shell here exports 44 names, of which the old form
# passed 40 into every cargo test, every script suite and the privacy sweep -- among them
# CLAUDE_CODE_MESSAGING_TOKEN, CLAUDE_CODE_MESSAGING_SOCKET, ANDROID_HOME, ANDROID_SDK_ROOT,
# JAVA_HOME, AI_AGENT and CLAUDECODE. A suite's answer must not depend on whose terminal
# started it, and a gate that can see an agent's messaging token can print it into a log.
#
# EVERY NAME BELOW CARRIES THE REASON IT IS NEEDED. That is the bar for adding one: not
# "it seems harmless" -- nothing here is about harm -- but "this step cannot do its job
# without it". Values are never listed, only names; what a name holds is the machine's
# business.
GATE_PASSTHROUGH = (
    # The machine's identity. cargo, rustup, playwright's browser cache, git's own config, the
    # login keychain and `gh`'s credential store all live under HOME. TMPDIR is NOT here: the
    # build sets it (gate_tmpdir), so a suite's scratch paths never depend on who started it.
    "HOME",
    "USER",
    "LOGNAME",
    # Text encoding. Without these a subprocess can decode its own output differently than
    # the run that measured it, which is a suite that fails on one terminal and not another.
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    # `origin` is git@github.com:WebDevBooster/richos.git, so `fetch` authenticates over ssh
    # through the agent. Without the socket the fetch phase prompts, and GIT_TERMINAL_PROMPT=0
    # (set below) turns that prompt into a failure whose message names none of this.
    "SSH_AUTH_SOCK",
    # macOS keychain access for codesign, which is how the security session is found. Not
    # needed by the gates; needed by the signing step that runs in the same environment.
    "SECURITYSESSIONID",
    # `gh api` in preflight and `gh release` at publish, for an operator who authenticates by
    # token or against a non-default host rather than through `gh auth login`'s store.
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_HOST",
    "GH_CONFIG_DIR",
    # A Rust toolchain that is not at ~/.cargo and ~/.rustup.
    "CARGO_HOME",
    "RUSTUP_HOME",
    # Xcode selection on a machine carrying more than one.
    "DEVELOPER_DIR",
    # The privacy deny-list for the `named-persons.sh --tree` gate. Operator-configurable by
    # design, and defaulted below when unset.
    "RICHOS_NAMED_PERSONS_FILE",
)

# WHAT THIS SCRIPT SETS FOR EVERY GATE, whatever the shell held. Declared as names here so
# that `run-tests.test.sh` case E1 -- which re-runs the whole suite under the environment a
# build hands it -- derives the list instead of keeping a copy that silently goes stale the
# next time a variable is added. `nightly-local.py gate-environment` prints it.
GATE_SET_BY_BUILD = (
    "PATH",
    # macOS's per-user temporary folder, whatever the launching shell held (gate_tmpdir).
    "TMPDIR",
    "PYTHONDONTWRITEBYTECODE",
    "CARGO_PROFILE_DEV_DEBUG",
    "CARGO_PROFILE_TEST_DEBUG",
    "GIT_TERMINAL_PROMPT",
    "GIT_SSH_COMMAND",
    "RICHOS_NIGHTLY_RUN_ID",
    # Always "full": no suite a nightly runs narrows to a workspace's changes (workspace_scope.py).
    "RICHOS_TEST_SCOPE",
    "RICHOS_NAMED_PERSONS_FILE",
    # Set by Runner.runtime() once the pinned runtimes are verified.
    "RICHOS_RUNTIME_DIR",
    # Set by Runner.runtime() beside it: the pinned speech models gui-boot's healthy machine
    # needs now that the video tools are a setup essential (lib/runtime_cache.py).
    "RICHOS_GUI_SPEECH_MODELS",
)

# WHAT THIS SCRIPT SETS FOR ONE STEP ONLY, at that step's own call site, through
# `command(env_extra=...)`. These never come from the shell -- that is the point of them --
# but a suite still has to survive seeing them, which is what E1 proves.
# The UI suite's quarantine is NOT here, and that is deliberate: it is passed to `run.js` as
# `--quarantine=` arguments, where it is visible in the command line the log records, rather
# than through an environment variable a stray export could also set.
GATE_SET_PER_STEP = (
    "RUN_TESTS_DECLARED_GAPS",
    "RUN_TESTS_SKIP_UNCHANGED",
    # How long a simulator suite waits for the machine's one prepared-simulator lease
    # (testdevices.py pool_wait_seconds): the script-suites gate's own deadline, so the iOS
    # suites it starts side by side queue for the pool instead of failing after 300 s.
    "RICHOS_IOS_POOL_WAIT",
    # The workspace-spec mutation pass, the same ruling; set at the workspace-mutants gate.
    "RICHOS_FOURTEEN_MUTANTS",
    # ...and the other workspace suites' passes (MUTATION_PASS_UNITS), at the same gate.
    "RICHOS_MUTATION_PASSES",
    # How many prepared-simulator leases the pool hands out at once (testdevices.py
    # pool_leases): the operator's `--simulated-phones`, set at the script-suites call site.
    SIMULATED_PHONES_ENV,
)


def gate_tmpdir(environ=None):
    """One temporary folder for both build gates and their author checks.

    WHY NOT THE LAUNCHING SHELL'S (2026-10-01). Nightly attempt 2 failed cargo-cache-env.test.sh
    with "sccache: error: path must be shorter than SUN_LEN": a Unix socket under
    `$TMPDIR/<mktemp>/...` fits in the 104 bytes macOS allows only when TMPDIR is short. A
    build started from a terminal passes through `/var/folders/<..>/T/` (57 bytes once
    resolved to /private/var/...); an agent's shell can hand down something shorter, and the
    same suite then passes for the engineer and fails in the nightly. So the build names ONE
    folder independent of the caller: the project's short external-SSD gate root when that
    volume is mounted, otherwise the account's platform root. Each command creates its own
    disposable subdirectories there. proof-run.py gets the same root via gate_conditions.
    """
    environ = os.environ if environ is None else environ
    if Path("/Volumes/E1TB").is_mount():
        folder = Path("/Volumes/E1TB/tmp/richos-gates")
        folder.mkdir(parents=True, exist_ok=True)
        return str(folder) + "/"
    try:
        out = subprocess.run(["/usr/bin/getconf", "DARWIN_USER_TEMP_DIR"], capture_output=True,
                             text=True, timeout=10, stdin=subprocess.DEVNULL)
        folder = out.stdout.strip() if out.returncode == 0 else ""
    except (OSError, subprocess.SubprocessError):
        folder = ""
    if folder and Path(folder).is_dir():
        return folder
    return environ.get("TMPDIR") or tempfile.gettempdir()


def local_environment(run_id=None):
    """Return (environment, credentials). Nothing merges them but the signing steps.

    `run_id`, when given, becomes this process's own RICHOS_NIGHTLY_RUN_ID instead of
    a freshly generated one -- used only so `build` can be told to resume a specific
    id; `publish`/`candidate` never pass one, since they act on an EXISTING build's
    run id (given separately, as `--run`) rather than minting their own.
    """
    # THE TWO QUESTIONS ARE ANSWERED SEPARATELY, and that is why credentials are collected
    # from the FULL environment while the gate environment is built from the allowlist. A
    # signing variable the operator exported must still reach the step that signs; it must
    # simply never reach a gate. Reading them from `os.environ` rather than popping them out
    # of `env` is what lets both be true at once.
    credentials = split_credentials(os.environ.copy())
    credentials.pop("TAURI_SIGNING_PRIVATE_KEY", None)  # The key's literal bytes: the path form is used.
    credentials.update(notary_environment(Path.home() / ".richos-signing/notary.env"))
    credentials["TAURI_SIGNING_PRIVATE_KEY_PATH"] = str(private_file(
        credentials.get("TAURI_SIGNING_PRIVATE_KEY_PATH", Path.home() / ".richos-signing/richos-updater.key")))
    credentials["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = credentials.get("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", "")
    return gate_environment(os.environ, run_id), credentials


def gate_environment(environ, run_id=None, strict=True):
    """What every gate's commands get, built from `environ` (the operator's shell). No credential.

    The one place this environment is built: local_environment() hands it to the gates, and
    gate_conditions() hands the same values to the checks proof-run.py runs before a nightly.
    `strict=False` (gate_conditions only) leaves out RICHOS_NAMED_PERSONS_FILE when the
    privacy list is not an owner-private file here, instead of refusing; the build itself
    always refuses.
    """
    env = {name: environ[name] for name in GATE_PASSTHROUGH if name in environ}
    # Explicit PATH also works from a fresh terminal, without an interactive shell.
    env["PATH"] = f"{Path.home()}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    env["TMPDIR"] = gate_tmpdir(environ)
    # NOT NEEDED ANY MORE, AND KEPT AS A PROOF RATHER THAN A STEP. Every name these two
    # removed -- GIT_AUTHOR_*, GIT_COMMITTER_*, EMAIL, RICHOS_EXTRA_TAURI_CONFIG,
    # TAURI_CONFIG, CARGO_TARGET_DIR, RUN_TESTS_DECLARED_GAPS -- is absent from
    # GATE_PASSTHROUGH, so an allowlisted environment cannot contain one. Asserting that
    # here means the day somebody adds a name to the allowlist without thinking, this fails
    # loudly instead of quietly restoring the leak.
    leaked = [n for n in IDENTITY_OVERRIDES + ("RICHOS_EXTRA_TAURI_CONFIG", "TAURI_CONFIG",
                                               "CARGO_TARGET_DIR", "RUN_TESTS_DECLARED_GAPS")
              if n in env]
    if leaked:
        raise ValueError(
            f"GATE_PASSTHROUGH admits {', '.join(leaked)}, which a gate must never take from "
            "the operator's shell. Remove the name from the allowlist.")
    # Not a credential: the privacy deny-list that engine/scripts/lib/named-persons.py
    # reads (line 297) for the `named-persons.sh --tree` gate. It stays with the gates.
    named = env.pop("RICHOS_NAMED_PERSONS_FILE", Path.home() / ".richos-privacy/named-persons")
    try:
        env["RICHOS_NAMED_PERSONS_FILE"] = str(private_file(named))
    except (OSError, ValueError):
        if strict:
            raise
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    env["CARGO_PROFILE_DEV_DEBUG"] = "0"
    env["CARGO_PROFILE_TEST_DEBUG"] = "0"
    env["GIT_TERMINAL_PROMPT"] = "0"
    env["GIT_SSH_COMMAND"] = "ssh -o BatchMode=yes -o ConnectTimeout=15"
    # A nightly runs every suite whole. Its source is a detached worktree, which a runner never
    # reads as a teammate workspace; stated outright anyway, so no change to where the source
    # lives can ever narrow a gate (engine/scripts/lib/workspace_scope.py, CEO 2026-10-01).
    env["RICHOS_TEST_SCOPE"] = "full"
    env["RICHOS_NIGHTLY_RUN_ID"] = run_id or (
        datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ-") + uuid.uuid4().hex[:8])
    return env


def script_suites_environment(simulated_phones, skip_unchanged=False):
    """What the script-suites gate adds for `run-tests.sh`, beyond gate_environment().

    Every value is written here rather than taken from the operator's shell, for the same
    reason DECLARED_GAPS is: a stray export must not be able to hold back a suite or skip one.
    The gate and gate_conditions() both call this, so the checks run before a nightly meet
    exactly these values (nightly attempt 2: merge-check-scope.test.py read the inherited
    RICHOS_IOS_POOL_WAIT and failed only here).
    """
    extra = {"RUN_TESTS_DECLARED_GAPS": DECLARED_GAPS,
             # THE PHONE APPS ARE NOT THIS BUILD (CEO, 2026-09-26: "the native
             # mobile apps are 2 COMPLETELY INDEPENDENT DIFFERENT APPS ... So, WHY
             # THE FUCK ARE THEY PART OF THE SAME FUCKING BUILD???"). `--for
             # desktop` leaves out every suite phone-app-suites.tsv names, so
             # native-ios-app's middle-size case A8 (RICHOS_NATIVE_IOS_APP_A8) is no
             # longer set here: it runs in the iPhone app's release check, and a
             # suite that no longer runs here has nothing to be told.
             #
             # No suite this build keeps leases an iPhone simulator. The pool's two
             # settings are still stated here, literally, so that a desktop suite
             # that someday does lease one queues for this gate's own deadline (run
             # 20260925T190759Z-2b4b0a7e: two suites gave up at the CLI's 300 s
             # default) and gets the operator's number, never the shell's.
             "RICHOS_IOS_POOL_WAIT": str(GATE_BUDGETS["gates/script-suites"]),
             # The operator's own number, from the command line (never the shell).
             SIMULATED_PHONES_ENV: str(simulated_phones)}
    if skip_unchanged:
        extra["RUN_TESTS_SKIP_UNCHANGED"] = "1"
    return extra


# THE NIGHTLY'S CONDITIONS, MET BEFORE THE NIGHTLY (2026-10-01). Five nightly attempts in one
# night each failed on a suite that had passed for its engineer: one under the build's long
# TMPDIR, one under a variable only the build sets (RICHOS_IOS_POOL_WAIT). The suites met the
# build's environment for the first time inside the build. proof-run.py -- the merge gate,
# autocheck's land checks and every engineer's proof run -- now hands each check of a suite a
# nightly gate runs what that gate would hand it, from gate_conditions() below, which is built
# by the same functions the gates call. Nothing is copied, so the two cannot drift: a value
# added to gate_environment() or script_suites_environment() reaches the checks with no
# second edit. A name the gates get that the checks deliberately do NOT is declared here, with
# the reason, and nightly-local.test.py fails on any other difference.
CONDITIONS_GATES = ("gates/script-suites", UI_SUITE_GATE)
CONDITIONS_NOT_REPRODUCED = {
    "RICHOS_RUNTIME_DIR": "set by Runner.runtime() to <state>/runtime-<recipe sha256[:12]> once verify-runtime.py "
                          "accepts it; proof-run.py's supply_runtime() hands a check that same "
                          "verified folder itself, or says why it could not",
    "RICHOS_GUI_SPEECH_MODELS": "set by Runner.runtime() to lib/runtime_cache.py's "
                                "gui_speech_models(); proof-run.py's supply_speech_models() hands "
                                "a check those same files, or says why it could not",
    "RUN_TESTS_SKIP_UNCHANGED": "the operator's choice to skip a suite whose inputs passed "
                                "before; it decides which suites run, not what they run under, "
                                "and a check that honored it could skip the very suite it was "
                                "asked to run (autocheck.py removes it for the same reason)",
}
# A run id in the build's own shape (local_environment), fixed so that a check's input identity
# (proof_evidence.checkout_identity hashes RICHOS_* values) is the same from one run to the next.
CONDITIONS_RUN_ID = "20000101T000000Z-00000000"
# The smallest number `--simulated-phones` accepts. No suite a nightly gate runs leases a
# simulator; a check on a shared Mac never claims more than one.
CONDITIONS_SIMULATED_PHONES = 1


def gate_conditions(gate, environ=None):
    """(conditions, missing): what nightly `gate` sets for its commands, whatever the shell held.

    `conditions` maps each name the build itself sets for that gate (GATE_SET_BY_BUILD and the
    gate's own per-step values) to the build's value; the names a check inherits from the
    shell anyway (GATE_PASSTHROUGH) are not repeated. `missing` maps a name the build would set
    and this machine cannot reproduce here (the privacy list is not an owner-private file) to
    why, so the caller can say so instead of silently testing less.
    """
    if gate not in CONDITIONS_GATES:
        raise ValueError(f"no nightly conditions are declared for {gate}; "
                         f"declared: {', '.join(CONDITIONS_GATES)}")
    environ = os.environ if environ is None else environ
    env = gate_environment(environ, CONDITIONS_RUN_ID, strict=False)
    if gate == "gates/script-suites":
        env.update(script_suites_environment(CONDITIONS_SIMULATED_PHONES))
    conditions = {name: value for name, value in env.items()
                  if name not in CONDITIONS_NOT_REPRODUCED
                  and (name not in GATE_PASSTHROUGH or name in GATE_SET_BY_BUILD)}
    missing = {name: "the build refuses to start without it; it is not an owner-private file here"
               for name in GATE_SET_BY_BUILD
               if name not in env and name not in CONDITIONS_NOT_REPRODUCED}
    return conditions, missing


def walk_recipe(bundle_zip, run_id, temp_root=None):
    """The shell lines that walk a candidate on a scratch home: (scratch folder, lines).

    The rule is nightly-launch.sh's, for the same reason (adoption ledger §2.5, a state
    directory per install). HOME alone is NOT a scratch home: Foundation answers
    NSHomeDirectory() from the account record, so WebKit's store and the URL cache of an
    instance started with HOME alone land in the REAL ~/Library/WebKit/com.richos.app and
    ~/Library/Caches/com.richos.app, the folders the daily driver (same bundle identifier)
    uses. Measured in a test guest on 2026-09-24 (richos-hq
    docs/verification/2026-09-24-nightly-launcher/, run 1 finding 3). So:

      * HOME and CFFIXED_USER_HOME are both the scratch home, and TMPDIR is inside it;
      * `env -i` with launchd's PATH, so nothing from the calling shell reaches the app;
      * `.noindex` folders, so Spotlight never offers this copy beside the real one;
      * the scratch folder is CANONICAL: the product refuses a home that is not its own
        realpath, and macOS's temporary folder sits behind the /var symlink.
    """
    base = Path(temp_root or tempfile.gettempdir()).resolve()
    scratch = base / f"richos-qa-{run_id}.noindex"
    home = scratch / "home.noindex"
    q = shlex.quote
    return scratch, [
        f"mkdir -p {q(str(home / 'tmp'))} && ditto -x -k {q(str(bundle_zip))} {q(str(scratch))}",
        f"/usr/bin/env -i HOME={q(str(home))} CFFIXED_USER_HOME={q(str(home))} "
        f"TMPDIR={q(str(home / 'tmp'))}/ \\",
        '    USER="$USER" PATH=/usr/bin:/bin:/usr/sbin:/sbin RICHOS_ACTIVATION=regular \\',
        f"    {q(str(scratch / 'RichOS.app/Contents/MacOS/richos-tauri'))}",
    ]


@contextmanager
def exclusive(state):
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    if state.is_symlink() or state.stat().st_uid != os.getuid():
        raise ValueError("nightly state must be owned by the current user")
    state.chmod(0o700)
    with (state / "release.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError("another local nightly command is running") from error
        yield


class Runner:
    def __init__(self, repo, state, env, log, credentials=None, gates_at_once=1,
                 simulated_phones=1, chosen_by=None, gui_host=None, vm_settings=None):
        # What runs in a gate's own thread -- its phase and its log -- is per thread, so gates
        # running side by side never write into each other's section or claim each other's
        # deadline. Outside a gate thread both are the run's own.
        self._local = threading.local()
        self.repo, self.state, self.env, self.log = repo, state, env, log
        # Merged in only where `credentials=True` says a step signs or notarizes.
        self.credentials = dict(credentials or {})
        self.source = state / "source"
        self.timings = []
        # phase -> (seconds its commands queued for machine worker tokens, seconds they ran)
        self.clocks = {}
        self.skipped = {}
        self.started = time.time()
        self.active_phase = None
        # The operator's decisions (GATES_AT_ONCE_FLAG, required; SIMULATED_PHONES_FLAG, None
        # when not named, which no desktop suite needs). 1 and 1 here is what a Runner built by
        # hand gets, which is the old behavior exactly.
        self.gates_at_once = gates_at_once
        self.simulated_phones = simulated_phones
        self.chosen_by = chosen_by
        # What the operator's shell said about the test VM, consumed by the VM boot proof
        # rather than dropped (settings_this_build_drops): the guest's name, if one was
        # given, and the testvm tooling's own settings.
        self.gui_host = gui_host
        self.vm_settings = dict(vm_settings or {})
        self.groups = OwnedGroups()
        self.gate_evidence = None
        self.reused_gates = set()

    @property
    def log(self):
        return getattr(self._local, "log", None) or self._log

    @log.setter
    def log(self, value):
        self._log = value

    @property
    def active_phase(self):
        return getattr(self._local, "phase", None)

    @active_phase.setter
    def active_phase(self, value):
        self._local.phase = value

    def record_settings(self):
        """The run log's first lines: what runs at once, and who decided it."""
        who = self.chosen_by or "whoever constructed this Runner (no command line)"
        self.announce(f"Gates at once: {self.gates_at_once} "
                      f"({GATES_AT_ONCE_FLAG} {self.gates_at_once}, chosen by {who})")
        if self.simulated_phones is None:
            self.announce(f"Simulated phones at once: not named ({SIMULATED_PHONES_FLAG} not given; "
                          "no suite in this build leases one, and one that did would get one at a time)")
        else:
            self.announce(f"Simulated phones at once: {self.simulated_phones} "
                          f"({SIMULATED_PHONES_FLAG} {self.simulated_phones}, chosen by {who})")

    def announce(self, text):
        """Say it on the terminal AND in the run log, so neither has to be read beside
        the other to know what this run did."""
        print(text, flush=True)
        self.log.write(text + "\n")

    def watch_child_milestones(self, armed):
        """Milestone matching belongs to the log, and only the `build` phase has any.

        A plain writable stream -- which is what the tests hand the Runner -- has no
        matcher, so there is nothing to arm and nothing to fail about.
        """
        arm = getattr(self.log, "arm", None)
        if arm:
            arm(armed)

    @contextmanager
    def phase(self, name):
        """One named, wall-clocked step. Nested phases are named `parent/child`."""
        self.log.write(f"=== phase {name} begins ===\n")
        self.watch_child_milestones(name == "build")
        start = time.time()
        previous_phase, self.active_phase = self.active_phase, name
        previous_clock = getattr(self._local, "clock", None)
        self._local.clock = [0.0, 0.0, 0]
        try:
            yield
        finally:
            clock, self._local.clock = self._local.clock, previous_clock
            self.active_phase = previous_phase
            end = time.time()
            self.watch_child_milestones(False)
            self.timings.append((name, start, end))
            if clock[2]:
                self.clocks[name] = (clock[0], clock[1])
                # Inside the section, so `--gates-passed-in` never reads it as the coordinator's.
                self.log.write(f"  {name}: admission {clock[0]:.1f}s waiting for machine worker "
                               f"tokens, execution {clock[1]:.1f}s over {clock[2]} command(s); "
                               "a deadline counts execution only\n")
            self.log.write(f"=== phase {name} ends: {end - start:.1f}s ===\n")

    def _count_clock(self, admission, execution):
        """Add one command's queueing and running time to the phase it ran in."""
        clock = getattr(self._local, "clock", None)
        if clock is None or admission is None:
            return
        clock[0] += admission
        clock[1] += execution or 0.0
        clock[2] += 1

    def clock_note(self, name):
        """` (admission A s, execution E s)` for a gate that ran commands, else nothing."""
        if name not in self.clocks:
            return ""
        admission, execution = self.clocks[name]
        return f" (admission {admission:.1f}s, execution {execution:.1f}s)"

    def skip(self, name, reason):
        self.skipped[name] = reason
        self.timings.append((name, None, None))
        self.announce(f"SKIPPED {name}: {reason}")

    def segments(self):
        """Every phase as (name, seconds), with the `build` step split by its milestones.

        The build step is one opaque subprocess from this script's side, so its interior
        comes from the milestones TimestampedLog matched while it ran. The leading stretch
        before the first milestone is named rather than dropped, and a milestone that was
        never seen is reported as such -- an unobserved boundary must not quietly inflate
        the segment next to it.
        """
        rows, seen = [], list(self.log.seen) if hasattr(self.log, "seen") else []
        observed = {name for name, _ in seen}
        for name, start, end in self.timings:
            if start is None:
                rows.append((name, None))
                continue
            if name != "build" or not seen:
                rows.append((name, end - start))
                continue
            bounds = [("build/tag-and-prepare", start)] + seen + [(None, end)]
            for (label, at), (_, nxt) in zip(bounds, bounds[1:]):
                rows.append((label, nxt - at))
            for label, _ in BUILD_MILESTONES:
                if label not in observed:
                    rows.append((label, None))
        return rows

    def summary(self):
        rows = self.segments()
        width = max([len(name) for name, _ in rows] + [len("total")])
        self.announce("")
        self.announce(f"phase timings for run {self.env.get('RICHOS_NIGHTLY_RUN_ID', 'manual')}:")
        for name, seconds in rows:
            if seconds is None:
                note = self.skipped.get(name, "not observed in this run's log")
                self.announce(f"  {name.ljust(width)} : -       ({note})")
            else:
                self.announce(f"  {name.ljust(width)} : {seconds:7.1f}s")
        self.announce(f"  {'total'.ljust(width)} : {time.time() - self.started:7.1f}s")

    def command(self, *args, cwd=None, capture=False, timeout=None, credentials=False,
                env_extra=None, cleanup=False, release_build=False):
        if self.groups.stopping and not cleanup:
            raise GateStopped(f"{self.active_phase or Path(str(args[0])).name} was stopped: "
                              "another gate already refused this build")
        env = {**self.env, **self.credentials} if credentials else self.env
        # A value this build states about ONE step, never a channel for the operator's
        # shell: everything in `env_extra` is written literally at the call site.
        if env_extra:
            env = {**env, **env_extra}
        try:
            result = owned_run([str(a) for a in args], cwd=cwd or self.source,
                               env=env, stdin=subprocess.DEVNULL, text=True,
                               stdout=subprocess.PIPE if capture else self.log,
                               stderr=self.log, timeout=timeout,
                               groups=self.groups, cleanup=cleanup,
                               **({"release_build": True} if release_build else {}))
        except CommandCleanupError as error:
            label = self.active_phase or Path(str(args[0])).name
            raise CommandCleanupError(f"{label} cleanup failed (command budget {timeout}s): {error}") from None
        except subprocess.TimeoutExpired as expired:
            # TimeoutExpired's default message includes argv, potentially a password.
            label = self.active_phase or Path(str(args[0])).name
            queued = getattr(expired, "admission_seconds", None)
            self._count_clock(queued, getattr(expired, "execution_seconds", None))
            after = (f" of execution, admitted after {queued:.1f}s in the worker queue"
                     if queued is not None else "")
            raise GateTimedOut(f"{label} timed out after {timeout}s{after}; owned group stopped; "
                               "see the run log") from None
        self._count_clock(getattr(result, "admission_seconds", None),
                          getattr(result, "execution_seconds", None))
        if result.returncode == 75 and getattr(result, "admitted", None) is False:
            # worker_tokens.py gave up waiting for a machine token: nothing ran.
            label = self.active_phase or Path(str(args[0])).name
            raise GateNotAdmitted(f"{label} was never admitted: waited {result.admission_seconds:.0f}s "
                               "for a machine worker token and nothing ran; see the run log")
        if result.returncode:
            # Do not echo argv: signing commands can carry a password.
            raise RuntimeError(f"{Path(str(args[0])).name} failed (exit {result.returncode}); see the run log")
        return result.stdout.strip() if capture else None

    def checkout(self, sha=None):
        """Put the dedicated worktree on `sha`, or on `origin/main` when none is given.

        `stable` is the only caller that names one: it rebuilds the commit a published
        nightly was built from, which is by definition not the tip.
        """
        self.command("git", "fetch", "origin", "main", cwd=self.repo, timeout=120)
        if sha is None:
            sha = self.command("git", "rev-parse", "FETCH_HEAD", cwd=self.repo, capture=True)
        if self.source.exists():
            self.require_clean_dedicated_source()
            self.command("git", "checkout", "--detach", sha)
        else:
            self.command("git", "worktree", "add", "--detach", self.source, sha, cwd=self.repo)
        return sha

    def require_clean_dedicated_source(self):
        """Refuse unless `self.source` is this repository's dedicated worktree, unchanged.

        Moving it never discards anything: changes in it are somebody's, so they stop the
        move and are left where they are.
        """
        common = self.command("git", "rev-parse", "--path-format=absolute", "--git-common-dir", capture=True)
        expected = self.command("git", "rev-parse", "--path-format=absolute", "--git-common-dir", cwd=self.repo, capture=True)
        if common != expected or Path(self.command("git", "rev-parse", "--show-toplevel", capture=True)) != self.source:
            raise ValueError("nightly source is not the dedicated worktree for this repository")
        if self.command("git", "status", "--porcelain", capture=True):
            raise ValueError("nightly worktree has changes; inspect it before continuing")

    def restore_build_commit(self, info):
        """Put the dedicated worktree back on the commit `info`'s candidate was built from.

        `build` and `publish` are separate commands an arbitrary time apart, sharing the one
        dedicated worktree, and every `check` or `build` in between checks it out on main.
        `nightly.py finish` refuses a checkout that is not the build commit, rightly: it runs
        that checkout's make-release.sh and reads its engine VERSION, and this command runs
        that checkout's nightly.py. But the candidate's files are digested on their own and
        the build commit is immutable, so the repair is that commit back, not a second build
        and a second walk (hunt part 2, section 16). A candidate recorded before the field
        existed carries no build commit, and `finish`'s own guard decides for it.
        """
        commit = info.get("build_commit")
        if not commit or not self.source.exists():
            return
        if self.command("git", "rev-parse", "HEAD", capture=True) == commit:
            return
        self.require_clean_dedicated_source()
        # The release commit is on no branch. A fresh clone, or a pruned repository, has it
        # only on the remote's hidden reservation ref, which verify_candidate_ref checks is
        # still this commit before anything is published.
        ref = info.get("candidate_ref") or info.get("reservation_ref")
        if not self._has_commit(commit) and ref:
            self.command("git", "fetch", "origin", ref, cwd=self.repo, timeout=120)
        if not self._has_commit(commit):
            raise ValueError(f"{commit}, the commit run {info.get('run_id')} was built from, is not "
                             f"in this repository{' or on ' + ref if ref else ''}, so it cannot "
                             "be checked out again to publish from; build it again")
        moved_from = self.command("git", "rev-parse", "HEAD", capture=True)
        self.command("git", "checkout", "--detach", commit)
        self.announce(f"The dedicated worktree was at {moved_from[:12]} (checked out since this "
                      f"candidate was built); put back on its build commit {commit[:12]}. "
                      "Nothing was rebuilt.")

    def _has_commit(self, commit):
        try:
            self.command("git", "cat-file", "-e", f"{commit}^{{commit}}", cwd=self.repo)
        except RuntimeError:
            return False
        return True

    def identity(self):
        """The identity the release commit will carry, resolved the way git resolves it.

        Not read from `git config`: GIT_AUTHOR_EMAIL / GIT_COMMITTER_EMAIL in the
        environment beat every config level and config cannot see them, so only
        `git var` answers what the commit will record. `user.useConfigOnly=true`
        removes git's hostname fallback, which is otherwise silent -- git 2.52.0
        happily builds a commit authored `user@host.home` and the machine's push
        guard then refuses the whole release.
        """
        idents = {}
        for var in ("GIT_AUTHOR_IDENT", "GIT_COMMITTER_IDENT"):
            result = subprocess.run(["git", "-c", "user.useConfigOnly=true", "var", var],
                                    cwd=self.source, env=self.env, stdin=subprocess.DEVNULL,
                                    text=True, capture_output=True, timeout=30)
            if result.returncode or "<" not in result.stdout:
                raise ValueError(
                    "this Mac has no configured git identity, so the release commit could "
                    "not be attributed to you: set user.name and user.email (git config "
                    "--global user.name '<your name>', git config --global user.email "
                    "'<your email>') and run check again")
            idents[var] = result.stdout.strip().rsplit(" ", 2)[0]
        if idents["GIT_AUTHOR_IDENT"] != idents["GIT_COMMITTER_IDENT"]:
            raise ValueError(f"author {idents['GIT_AUTHOR_IDENT']} and committer "
                             f"{idents['GIT_COMMITTER_IDENT']} identities differ; the "
                             "release commit must carry one identity")
        return idents["GIT_AUTHOR_IDENT"]

    def preflight(self):
        # First, and before any network call: it costs milliseconds and it is what the
        # 2026-09-17 attempt discovered after passing every gate.
        print(f"Release commits will be authored as {self.identity()}.", flush=True)
        print("Checking GitHub access, local signing and notarization credentials...", flush=True)
        if self.command("gh", "api", f"repos/{REPO}", "--jq", ".permissions.push", capture=True, timeout=60) != "true":
            raise ValueError("GitHub credentials cannot publish to this repository")
        # Until 2026-09-17 this asserted the OPPOSITE: that the `main-only` ruleset had
        # been given an exception for `refs/heads/nightly-channel`, and it refused to
        # run without one. That is how a publisher came to require a hole in the rule
        # that keeps every branch but `main` off the public repository. The channel is
        # a release tag now and needs no exception, so the check is inverted: the run
        # is refused while any branch but `main` is exempt. `nightly.py` owns the rule
        # and re-checks it immediately before the channel moves; this is the fail-fast
        # copy, ahead of forty minutes of gates.
        self.command(sys.executable, self.source / SCRIPTS / "nightly.py", "check-rules",
                     timeout=120)
        self.command("cargo", "tauri", "--version", timeout=30)
        if self.credentials.get("RICHOS_NOTARY_PROFILE") and not self.credentials.get("RICHOS_NOTARY_KEY"):
            auth = ["--keychain-profile", self.credentials["RICHOS_NOTARY_PROFILE"]]
        else:
            auth = ["--key", self.credentials["RICHOS_NOTARY_KEY"],
                    "--key-id", self.credentials["RICHOS_NOTARY_KEY_ID"],
                    "--issuer", self.credentials["RICHOS_NOTARY_ISSUER"]]
        self.command("xcrun", "notarytool", "history", *auth, "--output-format", "json",
                     capture=True, timeout=90, credentials=True)
        identities = self.command("security", "find-identity", "-v", "-p", "codesigning", capture=True)
        found = re.findall(r'\b([0-9A-F]{40}) "Developer ID Application:[^"]+"', identities)
        wanted = self.credentials.get("RICHOS_SIGNING_IDENTITY")
        if wanted:
            if wanted not in identities:
                raise ValueError("configured Developer ID identity is unavailable")
        elif len(found) == 1:
            wanted = found[0]
        else:
            raise ValueError("exactly one Developer ID signing identity is required")
        with tempfile.TemporaryDirectory(prefix="richos-nightly-signing-") as tmp:
            probe = Path(tmp) / "probe"
            # Do not copy protected system-file flags or extended attributes.
            shutil.copyfile("/usr/bin/true", probe)
            probe.chmod(0o755)
            self.command("codesign", "--force", "--sign", wanted, "--timestamp", "--options", "runtime",
                         probe, timeout=90, credentials=True)
            self.command("codesign", "--verify", "--strict", probe, timeout=30)
            self.command("cargo", "tauri", "signer", "sign",
                         "-f", self.credentials["TAURI_SIGNING_PRIVATE_KEY_PATH"],
                         "-p", self.credentials["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"],
                         probe, timeout=30, credentials=True)
            # Public key configuration must match the existing local key pair. The
            # release pipeline also verifies the actual artifact signature before publishing.
            public = Path(self.credentials["TAURI_SIGNING_PRIVATE_KEY_PATH"] + ".pub").read_text().strip()
            conf = json.loads((self.source / "richos/app/src-tauri/tauri.conf.json").read_text())
            expected = conf["plugins"]["updater"]["pubkey"].strip()
            if public not in (expected, base64.b64decode(expected).decode().strip()):
                raise ValueError("local updater public key does not match the app")
        print("Local signing and Apple notarization authentication passed.", flush=True)

    def plan(self, force):
        path = self.state / "plan.json"
        self.command(sys.executable, self.source / SCRIPTS / "nightly.py", "plan",
                     *(["--force"] if force else []), "--output", path)
        return path, json.loads(path.read_text())

    def stable_plan(self, nightly_tag):
        """Resolve a STABLE release as a rebuild of the commit `nightly_tag` was built from.

        Read at MAIN, deliberately, and before the worktree is moved to the older commit:
        the CEO's promotion record is a fact about today, written after the nightly he
        tested was published, so it does not exist in that nightly's own commit.
        """
        path = self.state / "stable-plan.json"
        self.command(sys.executable, self.source / SCRIPTS / "nightly.py", "stable-plan",
                     "--from-nightly", nightly_tag, "--output", path, timeout=300)
        return path, json.loads(path.read_text())

    def describe_stable(self, info):
        """Exactly what `stable` would build and publish, and nothing it would not.

        A dry run whose output is a summary is a dry run nobody can check. Every value
        below is the one the real path would use, read from the plan it just resolved --
        the version from the source commit's own Cargo.toml, the endpoint from the
        channel table, his words from the promotion record.
        """
        print("")
        print(f"  Stable release        : {info['tag']}  (version {info['version']})")
        print(f"  Rebuilt from          : {info['promoted_from']} "
              f"(version {info['promoted_from_version']})")
        print(f"  Source commit         : {info['source_commit']}")
        print(f"  Compiled-in endpoint  : {info['endpoint']}")
        print("")
        print("  HIS DECISION, which is the only thing that allows this (ceo-decisions §69):")
        print(f"    decided on {info['promotion_decided_on']}")
        print(f"    \"{info['promotion_words']}\"")
        print("")
        print("  WOULD THEN:")
        print(f"    1. move the dedicated worktree to {info['source_commit'][:12]} and run "
              "every gate there")
        print(f"    2. build, sign and notarize {info['tag']} from that commit, with the "
              "stable endpoint and the stable version compiled in")
        print(f"    3. upload every asset to {info['tag']} while it is still a PRERELEASE, "
              "so nothing installed can see a partial release")
        print("    4. flip that release to `--latest --prerelease=false`, which is the "
              "whole channel move")
        print("")
        print("  Nothing above has been done. No tag was created, nothing was built, "
              "uploaded or published.")

    def runtime_path(self, override=None):
        """The runtime this run verifies: the operator's --runtime-dir, else the state directory's
        runtime for THIS source's recipe (`lib/runtime_cache.py`), so a recipe change builds its
        own runtime rather than refusing the previous one."""
        if override:
            return override
        library = str(Path(__file__).resolve().parent / "lib")
        if library not in sys.path:
            sys.path.insert(0, library)
        import runtime_cache
        return runtime_cache.cache_path(self.state, self.source / SCRIPTS / "runtime-sources.json")

    def runtime(self, override):
        path = self.runtime_path(override)
        if not path.exists():
            if override:
                raise ValueError("specified runtime directory does not exist")
            print("Building the pinned public runtimes...", flush=True)
            self.command(sys.executable, self.source / SCRIPTS / "build-runtimes.py", path)
        self.command(sys.executable, self.source / SCRIPTS / "verify-runtime.py", path,
                     self.source / SCRIPTS / "runtime-sources.json")
        self.env["RICHOS_RUNTIME_DIR"] = str(path)
        # gui-boot's healthy machine needs the pinned speech models (the video tools are a setup
        # essential). Absent, nothing is set and gui-boot refuses naming this input.
        library = str(Path(__file__).resolve().parent / "lib")
        if library not in sys.path:
            sys.path.insert(0, library)
        import runtime_cache
        models = runtime_cache.gui_speech_models()
        if models:
            self.env["RICHOS_GUI_SPEECH_MODELS"] = models

    def ui_proof_path(self, sha):
        return self.state / "ui-coverage" / f"{sha}.json"

    def accept_ui_proof(self, sha):
        """The coverage proof for `sha`, or None. Never a bare boolean, and never a guess.

        `--checks-done-at-land` says a land already ran the sharded UI suite on the commit
        this build then fetched. The proof that makes that true is the file `run.js
        --coverage --proof-out` writes on a PASS, and the only thing tying it to a tree is
        the commit inside it -- so it is compared against the sha this run actually fetched,
        exactly as accept_land_proof does. A proof file named after the right sha whose
        CONTENTS name a different one proves nothing; that is why both are checked.

        THE READER ENFORCES THE WRITER'S SHAPE; IT DOES NOT ASSUME IT (hunt part 2, finding
        17). `run.js` writes this file only on a PASS, with the full commit it ran at
        (`commitSha()`, `git rev-parse HEAD`) and the counts it observed. Until 2026-09-30 this
        reader checked only the `proof` tag and a commit PREFIX, so `{proof, commit}` with no
        counts at all, or a one-character commit, skipped the whole UI gate and announced
        "None suite(s), None checks". A proof skips the gate only when it names this exact
        commit in full and records at least one check observed and every discovered suite
        run (part 2 recheck, R17). Anything less claims coverage it does not show,
        and is refused like a proof for another tree, never quietly run over.
        """
        path = self.ui_proof_path(sha)
        if not path.exists():
            return None
        try:
            proof = json.loads(path.read_text())
        except (OSError, ValueError):
            return None
        if not isinstance(proof, dict) or proof.get("proof") != "ui-suite-coverage":
            return None
        seen = proof.get("commit", "")
        if not isinstance(seen, str) or not seen or seen != sha:
            raise ValueError(
                f"{path} is a coverage proof for {seen or '<no commit>'}, not for {sha}. "
                "A proof taken against a different tree proves nothing about this one; "
                "run.js records the full commit, so nothing shorter is accepted.")

        def count(key):
            value = proof.get(key)
            # bool is an int in Python; `true` is not a count.
            return value if isinstance(value, int) and not isinstance(value, bool) else None

        suites, ran, checks = count("suites"), count("ran"), count("checks")
        if suites is None or ran is None or checks is None or not (1 <= ran <= suites) or checks < 1:
            raise ValueError(
                f"{path} names {sha} but records no coverage (suites={proof.get('suites')!r}, "
                f"ran={proof.get('ran')!r}, checks={proof.get('checks')!r}). run.js writes this "
                "file only after at least one suite ran and at least one check was observed; "
                "a proof without those counts proves nothing, so the UI gate is not skipped.")
        # A PASS IS NOT THE SAME CLAIM AS THE GATE IT REPLACES (part 2 recheck, R17). run.js
        # writes a PASS proof with `ran` below `suites` when a suite was skipped under
        # --allow-skip or excused by a quarantine without evidence, and says so. The gate
        # this proof stands in for runs every discovered suite, so a proof may skip it only
        # when it ran every one; otherwise it certifies suites nobody ran.
        if ran != suites:
            raise ValueError(
                f"{path} names {sha} but ran only {ran} of {suites} discovered UI suite(s); "
                f"{suites - ran} were not run. The UI gate runs every suite, so a proof of "
                "fewer does not stand in for it and the UI gate is not skipped. Run the land's "
                "UI suite over every suite, or build without --checks-done-at-land.")
        return proof

    def accept_core_proof(self, sha):
        """The path of a land receipt that ran the whole richos-core crate on `sha`'s tree, or None.

        Keyed the way autocheck.py keys it, by TREE, in this repository's common git
        directory: a receipt says which commands passed on exactly that content. Only the
        unfiltered LAND_CORE_COMMAND counts, and not when the receipt lists it as NOT RUN. A
        missing repository, commit, receipt or field is None, and the gate runs.
        """
        if not re.fullmatch(r"[0-9a-f]{7,40}", sha or ""):
            return None
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}

        def git(*args):
            try:
                done = subprocess.run(["git", "-C", str(self.repo), *args], env=env, capture_output=True,
                                      text=True, stdin=subprocess.DEVNULL, timeout=30)
            except (OSError, subprocess.SubprocessError):
                return ""
            return done.stdout.strip() if done.returncode == 0 else ""

        tree = git("rev-parse", "--verify", "-q", f"{sha}^{{tree}}")
        common = git("rev-parse", "--path-format=absolute", "--git-common-dir")
        if not tree or not common:
            return None
        path = Path(common) / LAND_RECEIPTS / tree
        try:
            receipt = json.loads(path.read_text())
        except (OSError, ValueError):
            return None
        if not isinstance(receipt, dict) or receipt.get("tree") != tree:
            return None
        commands = receipt.get("commands")
        not_run = {row.get("check") for row in receipt.get("not_run") or [] if isinstance(row, dict)}
        if not isinstance(commands, list) or LAND_CORE_COMMAND not in commands or LAND_CORE_COMMAND in not_run:
            return None
        return path

    def ui_suite(self, checks_done_at_land=None):
        """Run the whole UI inventory, sharded, and refuse the build on a red suite.

        `run.js --shards=N` owns fan-out, receipts and the coverage verdict.
        command() owns its process group, credential split, log and deadline.
        This method never launches a second process outside that boundary.

        THE SHARDS' EXIT CODES ARE NOT THE VERDICT and this method never sees them. `run.js`
        says why in its own words -- a shard has seen a subset, and only the coverage job has
        seen every receipt, so only it can tell a failed suite from an absent one.
        """
        if checks_done_at_land:
            proof = self.accept_ui_proof(checks_done_at_land)
            if proof:
                self.skip(UI_SUITE_GATE,
                          f"the land already ran the sharded UI suite on {checks_done_at_land} "
                          f"({proof.get('ran')} suite(s), {proof.get('checks')} checks, "
                          f"proven {proof.get('at')}), which is the commit this run fetched")
                return
        receipts = self.state / "ui-receipts"
        with self.phase(UI_SUITE_GATE):
            tree = self.ui_checkout()
            tests = tree / UI_TESTS
            if UI_QUARANTINE:
                self.announce(f"  ui quarantine ({len(UI_QUARANTINE)}): " + "; ".join(UI_QUARANTINE))
            else:
                self.announce("  ui quarantine: empty - every suite holds this build.")
            args = ["node", "run.js", f"--shards={UI_SHARDS}", f"--receipts={receipts}"]
            for suite in UI_QUARANTINE:
                args.append(f"--quarantine={suite}")
            try:
                self.command(*args, cwd=tests, timeout=GATE_BUDGETS[UI_SUITE_GATE])
                self.restore_source_tree("the UI suite", tree)
            except RuntimeError as error:
                if not isinstance(error, CommandCleanupError):
                    self.restore_source_tree("the UI suite", tree)
                # NAME THE SUITE AND ITS LOG. A gate that refuses a build and leaves the
                # reader to find out which of 55 suites did it is a gate people learn to
                # re-run rather than read.
                red = self.red_ui_suites(receipts)
                if self.retry_red_ui_suites_alone(error, receipts, tests, tree, red):
                    return
                raise RuntimeError(
                    f"the UI suite refused this build: {error}"
                    + (f": {', '.join(red)}" if red else " (see the coverage output)")
                    + f". Every shard's output and the coverage verdict are in this run's log "
                    f"under the {UI_SUITE_GATE} phase; the receipts are in {receipts}.") from None

    UI_CHECKOUT = "ui-checkout"

    def ui_checkout(self):
        """The UI suite's OWN worktree, at the commit this run fetched: never `self.source`.

        THE UI SUITE WRITES INTO ITS CHECKOUT (restore_source_tree says why), and until
        2026-09-29 that checkout was the nightly's `source`, which other gates WATCH while
        it runs: workspace-mutants runs ci-shard.sh from its root, and ci-shard's leak canary
        turns any new or changed `git status` line under `$PWD` into a LEAKED verdict for its
        unit (ci-shard.sh:519-520, :626-627), as does every script suite that runs ci-shard
        from the checkout. So a screenshot rewritten mid-run would fail a gate that never
        touched it (audit R5, docs/verification/2026-09-29-load-sensitive-checks-audit.md).
        The suite now runs here, a sibling of `source` that no canary watches; the privacy
        sweep, which scans `source`, no longer has to wait for it. A leftover change from a
        run that died before its restore is discarded by the forced checkout; each run's own
        changes are still named by restore_source_tree before they are put back. Its git
        commands run inside the UI gate's phase and carry the gate's ceiling, as a hang guard.
        """
        path = self.state / self.UI_CHECKOUT
        sha = self.command("git", "rev-parse", "HEAD", cwd=self.source, capture=True,
                           timeout=GATE_BUDGETS[UI_SUITE_GATE])
        if path.exists():
            common = self.command("git", "rev-parse", "--path-format=absolute", "--git-common-dir",
                                  cwd=path, capture=True, timeout=GATE_BUDGETS[UI_SUITE_GATE])
            expected = self.command("git", "rev-parse", "--path-format=absolute", "--git-common-dir",
                                    cwd=self.repo, capture=True, timeout=GATE_BUDGETS[UI_SUITE_GATE])
            top = self.command("git", "rev-parse", "--show-toplevel", cwd=path, capture=True,
                               timeout=GATE_BUDGETS[UI_SUITE_GATE])
            if common != expected or Path(top).resolve() != path.resolve():
                raise ValueError(f"{path} is not the UI suite's worktree of this repository")
            self.command("git", "checkout", "--force", "--detach", sha, cwd=path,
                         timeout=GATE_BUDGETS[UI_SUITE_GATE])
        else:
            self.command("git", "worktree", "add", "--detach", path, sha, cwd=self.repo,
                         timeout=GATE_BUDGETS[UI_SUITE_GATE])
        return path

    def restore_source_tree(self, who, tree=None):
        """Put the fetched source back the way it was fetched, naming anything that moved.

        `tree` is the checkout to inspect: the UI suite's own (ui_checkout) for the UI gate,
        `self.source` otherwise.

        THIS EXISTS BECAUSE THE UI SUITE WRITES TO ITS OWN CHECKOUT, and until this gate
        existed nothing in a build did. `lib/harness.js:publishShot` deliberately REWRITES a
        committed screenshot when the picture it takes differs, announcing "shot changed" --
        good behavior for an engineer running the suite by hand, and a trap for a build:
        `checkout()` refuses to reuse the nightly worktree when `git status --porcelain`
        says anything at all ("nightly worktree has changes; inspect it before continuing").
        So a gate that left the tree dirty would pass tonight and REFUSE TO START tomorrow,
        with a message about the worktree that names nothing about the suite that dirtied it.

        MEASURED, 2026-09-20, two full runs at bf1381e0: `shots-phone/phone-light.png` and
        `shots-contrast/phone-pairing.png` were untouched by the first and rewritten by the
        second -- 775 of 1,330,000 pixels, worst channel delta 164, in one 120x13 box. The
        two runs differed only in which shard each suite landed in, so that surface is not
        the same picture twice and is not declared in `lib/shot-stability.js`, which is the
        file that exists to declare exactly that.

        IT REPORTS BEFORE IT RESTORES, and that order is the whole point. A silent `git
        checkout -- .` would erase the evidence that a screen changed, which is the one thing
        anybody would want to know. It does not FAIL the build: an unstable screenshot is
        test-artifact drift, not a broken app, and stopping a night's build over it is the
        trade this gate is explicitly not making.
        """
        # NOTHING HERE MAY RAISE. One of the two callers is an `except` block reporting which
        # suite refused the build, and an exception thrown from inside it would replace that
        # message with a git error -- losing the only sentence that says what actually
        # happened. Tidying up is never allowed to become the reported failure.
        tree = tree or self.source
        try:
            dirty = self.command("git", "status", "--porcelain", cwd=tree, capture=True,
                                 timeout=CLEANUP_TIMEOUT, cleanup=True)
        except (RuntimeError, OSError) as error:
            self.announce(f"  could not check whether {who} left its checkout dirty: {error}")
            return
        if not dirty:
            return
        self.announce(f"  {who} modified its own checkout; restoring it so the next build's "
                      f"`checkout()` is not refused. Files:")
        for line in dirty.splitlines():
            self.announce(f"    {line}")
        self.announce("  If one of these is a real visual change, it is a change to look at, "
                      "not a file to re-commit from here; if it differs run to run, it belongs "
                      "in richos/app/ui/tests/lib/shot-stability.js with its cause and bound.")
        try:
            self.command("git", "checkout", "--", ".", cwd=tree, timeout=CLEANUP_TIMEOUT,
                         cleanup=True)
        except (RuntimeError, OSError) as error:
            # Say it plainly rather than swallowing it: the next build will refuse to start
            # and this line is what tells somebody why.
            self.announce(f"  RESTORE FAILED ({error}). {tree} is left with these changes; "
                          "the next build's checkout() refuses `source` until it is clean, and "
                          "ui_checkout() discards them in the UI suite's own worktree.")

    @staticmethod
    def red_ui_suites(receipts):
        """Which suites a finished sharded run recorded as red, read off the receipts.

        Read here rather than parsed out of the coverage job's prose: the receipt is the
        thing `run.js` wrote down, and a second parser for the same fact is a second thing
        to keep true.

        EACH FAILED CHECK IS NAMED, with the first line of its message (`lib/harness.js`
        report() writes both into the receipt). Run 20260928T233221Z-56bcde43 refused the
        build with "home.js (exit 1, 1 failed check(s))" and nothing else, and the log had
        lost the FAIL line, so the only way to learn which check was a rerun.
        """
        return [text for _, text in Runner.red_ui_receipts(receipts)]

    @staticmethod
    def red_ui_receipts(receipts):
        """(suite, the line red_ui_suites prints for it) for each red receipt in `receipts`."""
        red = []
        for path in sorted(Path(receipts).glob("*.receipt.json")):
            try:
                receipt = json.loads(path.read_text())
            except (OSError, ValueError):
                continue
            records = [r for r in receipt.get("records", []) if isinstance(r.get("checks"), int)]
            failed = sum(r.get("failed", 0) for r in records)
            if receipt.get("exit") or failed:
                named = [f"{f.get('check')}: {str(f.get('message') or '').splitlines()[0] if f.get('message') else ''}"
                         for r in records for f in r.get("failures") or [] if isinstance(f, dict)]
                red.append((receipt.get("suite"),
                            f"{receipt.get('suite')} (exit {receipt.get('exit')}, "
                            f"{failed} failed check(s))" + (" [" + "; ".join(named) + "]" if named else "")))
        return red

    def mutation_units(self, units, environment, label):
        """Use the existing per-unit runner/receipts instead of losing a batch's passes.

        Each retry gets a fresh evidence directory. The existing qualified-input owner
        store reuses completed units only after validating their inputs and receipts.
        ci-shard keeps each unit's deadlines, leak canary and worker admission unchanged.
        """
        plans = self.state / "mutation-plans"
        plans.mkdir(parents=True, exist_ok=True)
        selection = plans / (label + ".txt")
        selection.write_text("cd richos/engine && bash scripts/ci-shard.sh --only-units " +
                             ",".join(units) + "\n")
        evidence = self.state / "mutation-proof"
        evidence.mkdir(parents=True, exist_ok=True)
        logdir = Path(tempfile.mkdtemp(prefix=label + "-", dir=evidence))
        self.command(sys.executable, self.source / SCRIPTS / "proof-run.py",
                     "--commands", selection, "--log-dir", logdir,
                     # The engine never reads this output-only nightly generation id.
                     # Keep the actual id on the coordinator, normalize only this batch.
                     env_extra={**environment, "RICHOS_NIGHTLY_RUN_ID": CONDITIONS_RUN_ID},
                     timeout=GATE_BUDGETS[WORKSPACE_MUTANTS_GATE])

    def retry_red_ui_suites_alone(self, error, receipts, tests, tree, first):
        """Give each suite the sharded run left red ONE attempt alone; True when the build goes on.

        THE SAME RULE run-tests.sh HAS FOR A SCRIPT SUITE (`retry_alone`, per
        docs/development/verification-retries.md). Run 20261010T025551Z-f0359d9b refused a
        whole build over splash.js check 10b ("the shutter's window moved from 238ms to
        353ms"), which passed alone on the same tree minutes later; so did setup.js check 24 in
        run 20261010T002045Z-ded5dc34. The command that failed has returned, so every shard has
        finished; each red suite then runs alone (`run.js --suite`, which writes its receipt
        over the red one) and the coverage job reconciles the whole receipt directory again,
        so a missing receipt or any other refusal still refuses. If a suite fails again alone
        the build is refused here, quoting both attempts; nothing runs a third time.

        Only a plain red run is retried: a stop (another gate refused the build), a timeout,
        an admission refusal or a failed cleanup returns False and the caller refuses as before,
        as does a refusal with no red receipt or only quarantined ones.
        """
        if type(error) is not RuntimeError or self.groups.stopping:
            return False
        red = [(suite, text) for suite, text in self.red_ui_receipts(receipts)
               if suite not in UI_QUARANTINE]
        if not red:
            return False
        budget = GATE_BUDGETS[UI_SUITE_GATE]
        failed_again = None
        try:
            for suite, text in red:
                self.announce(f"  {UI_SUITE_GATE}: {text}; retrying it ONCE, alone "
                              "(verification-retries.md).")
                try:
                    self.command("node", "run.js", f"--suite={suite}", f"--receipts={receipts}",
                                 cwd=tests, timeout=budget)
                except RuntimeError as again:
                    if type(again) is not RuntimeError:
                        raise  # stopped, timed out, not admitted or not cleaned up: not a verdict
                    second = dict(self.red_ui_receipts(receipts)).get(suite) or f"{suite} ({again})"
                    self.announce(f"  {UI_SUITE_GATE}: {suite} FAILED AGAIN on the retry, alone. "
                                  f"First attempt: {text}. Solo attempt: {second}.")
                    failed_again = (text, second)
                    break
                self.announce(f"  {UI_SUITE_GATE}: {suite} PASSED on the retry, alone. "
                              f"First attempt: {text}. It is counted as passed; the first failure "
                              "is recorded above and is load-sensitive until shown otherwise.")
            if failed_again is None:
                coverage = ["node", "run.js", f"--coverage={receipts}"]
                coverage += [f"--quarantine={suite}" for suite in UI_QUARANTINE]
                self.command(*coverage, cwd=tests, timeout=budget)
        except CommandCleanupError:
            raise
        except RuntimeError as again:
            self.restore_source_tree("the UI suite's solo attempt", tree)
            if type(again) is not RuntimeError:
                # Raised as it is, so the gate's record keeps "stopped" or "timed-out".
                self.announce(f"  {UI_SUITE_GATE}: the solo attempt did not finish ({again}); "
                              f"the sharded run had failed on: {', '.join(first)}.")
                raise
            raise RuntimeError(
                f"the UI suite refused this build: the sharded run failed ({error}: "
                f"{', '.join(first)}), and after the solo attempt the coverage job still refused "
                f"it ({again}). The run log has both under the {UI_SUITE_GATE} phase; the "
                f"receipts are in {receipts}.") from None
        self.restore_source_tree("the UI suite's solo attempt", tree)
        if failed_again is not None:
            raise RuntimeError(
                f"the UI suite refused this build: a suite failed in the sharded run and FAILED "
                f"AGAIN alone: first attempt: {failed_again[0]}; solo attempt: {failed_again[1]}. "
                f"Every shard's output, the solo attempt and the coverage verdict are in this "
                f"run's log under the {UI_SUITE_GATE} phase; the receipts are in {receipts}.")
        return True

    def gates(self, checks_done_at_land=None, no_host_screen=False, skip_unchanged=False):
        # Deliberately without `credentials=True`: a gate that can see the operator's
        # notary key answers questions the suites ask precisely because the answer
        # should be absent. See split_credentials() and package-app.test.sh section E.
        self.announce("Running core, updater and packaging checks...")
        results = self.state / SUITE_RESULTS

        # FIRST IN THE ORDER, AND IT COSTS A QUARTER OF A SECOND. Every step a release
        # performs and an ordinary day does not -- the version written into the manifest,
        # the endpoint compiled into the binary, the candidate's digests, the updater
        # metadata, the CEO's promotion record -- exercised against a throwaway directory
        # before a single crate is compiled. See `nightly.py`'s `release_smoke` for what is
        # in it and what is deliberately not. It is NEVER skipped for a candidate: skipping
        # is for work whose inputs are unchanged, and this gate's input is the release path
        # itself. With gates side by side it starts with the rest and still refuses first.
        def release_smoke():
            with self.phase("gates/release-smoke"):
                self.command(sys.executable, self.source / SCRIPTS / "nightly.py",
                             "release-smoke", "--out", self.state / "release-smoke",
                             timeout=GATE_BUDGETS["gates/release-smoke"])

        def core_tests():
            if checks_done_at_land:
                receipt = self.accept_core_proof(checks_done_at_land)
                if receipt:
                    self.skip(LAND_PROVEN_GATE,
                              f"the land ran the whole `cargo test -p richos-core` on {checks_done_at_land} "
                              f"(land receipt {receipt}), which is the commit this run fetched")
                    return
                self.announce(f"  {LAND_PROVEN_GATE} runs: no land receipt for {checks_done_at_land} shows "
                              "the whole richos-core crate ran (a land runs only the targets its change "
                              "selects)")
            with self.phase(LAND_PROVEN_GATE):
                self.command("cargo", "test", "--locked", "--manifest-path",
                             "richos/app/Cargo.toml", "-p", "richos-core",
                             timeout=GATE_BUDGETS[LAND_PROVEN_GATE])

        def updater_tests():
            with self.phase("gates/updater-tests"):
                self.command("cargo", "test", "--locked", "--manifest-path",
                             "richos/app/crates/richos-user-update/Cargo.toml",
                             timeout=GATE_BUDGETS["gates/updater-tests"])

        def script_suites():
            with self.phase("gates/script-suites"):
                # Written in script_suites_environment(), which gate_conditions() also calls,
                # so the checks run before a nightly meet these exact values.
                extra = script_suites_environment(
                    1 if self.simulated_phones is None else self.simulated_phones, skip_unchanged)
                args =["bash", self.source / SCRIPTS / "run-tests.sh", "--for", "desktop",
                        "--results-out", results]
                if no_host_screen:
                    args.append("--no-host-screen")
                self.command(*args, env_extra=extra, timeout=GATE_BUDGETS["gates/script-suites"])

        def lint_tauri():
            with self.phase("gates/lint-tauri"):
                self.command("bash", self.source / SCRIPTS / "lint.sh", "--all",
                             "--suite-results", results, timeout=GATE_BUDGETS["gates/lint-tauri"])

        # The workspace-spec mutation pass: 86 deliberately broken copies of the workspace code,
        # each of which one of the suite's checks must catch. OFF every land and ON before
        # every nightly (CEO, 2026-09-23, "Only before nightlies", esc-20260923T123440Z-b5371531);
        # the suite's fourteen checks still run on every land that touches that code. Through
        # ci-shard.sh, so the unit keeps its leak canary and its deadline; a failed or unproven
        # mutant fails this gate and stops the nightly. Every mutant works in its own copy of
        # the engine (mutation-harness.sh), so nothing it does touches the tree another gate reads.
        #
        # The other workspace suites' mutation passes, the same ruling applied to them (hunt
        # part 4 finding 19, 2026-09-30): OFF at every land, ON here with
        # RICHOS_MUTATION_PASSES=1, one unit after another, a failed pass failing the gate
        # (MUTATION_PASS_UNITS says why app.test.sh is not among them yet). Their own budget is
        # UNVERIFIED: the dated CI-era weights put the three units, mutation passes included,
        # near 700 s (workspaces 580 s, workspace-probes 114 s, create-teammate-worktree 4 s;
        # their behavioral halves took 89, 25 and 9 s here on 2026-09-30), so they get the
        # fourteen-point pass's own 3600 s until the first nightly measures them.
        def workspace_mutants():
            with self.phase(WORKSPACE_MUTANTS_GATE):
                self.mutation_units(["mega-lander/tests/workspace-spec-fourteen.test.sh"],
                                    {"RICHOS_FOURTEEN_MUTANTS": "1"}, "fourteen")
                self.mutation_units(MUTATION_PASS_UNITS, {"RICHOS_MUTATION_PASSES": "1"}, "workspace")

        def privacy_sweep():
            with self.phase("gates/privacy-sweep"):
                self.command("bash", "richos/engine/scripts/named-persons.sh", "--tree",
                             "--repo", self.source, timeout=GATE_BUDGETS["gates/privacy-sweep"])

        # GATE ORDER: the old one-after-another order, which `--gates-at-once 1` runs exactly
        # and which is also the order a limited number of slots is filled in. The cheap
        # refusals come first so that, one at a time, there is no sense spending 378 s of
        # WebKit to learn that `cargo test` was going to fail anyway. What waits for what is
        # GATE_AFTER, and nothing else.
        gates = [
            ("gates/release-smoke", release_smoke),
            (LAND_PROVEN_GATE, core_tests),
            ("gates/updater-tests", updater_tests),
            ("gates/script-suites", script_suites),
            ("gates/lint-tauri", lint_tauri),
            (WORKSPACE_MUTANTS_GATE, workspace_mutants),
            (UI_SUITE_GATE, lambda: self.ui_suite(checks_done_at_land)),
            ("gates/privacy-sweep", privacy_sweep),
        ]
        # `--gates-passed-in` checks a recorded run against GATE_NAMES; a gate added here
        # and not there would be waved through by every resume.
        if tuple(name for name, _ in gates) != GATE_NAMES:
            raise ValueError("gates() and GATE_NAMES disagree; update GATE_NAMES")
        # Candidates can reconcile actual passes across failed attempts. Commands
        # that publish still run every gate fresh. Smoke always runs. Reused UI
        # evidence is reconciled again by run.js's own coverage verifier.
        if skip_unchanged:
            gates = self.reconcile_gates(gates, checks_done_at_land, no_host_screen)
        self.run_gates(gates)
        try:
            return json.loads(results.read_text())
        except (OSError, ValueError):
            # The suites are what matter and they passed; a missing report is not a reason
            # to throw away a green gate. It IS a reason for `publish` to refuse, which it
            # does: a candidate with no recorded gui-boot result is treated exactly like
            # one that recorded NOT RUN.
            return None

    def gate_input_identity(self, checks_done_at_land, no_host_screen):
        """Conservative contract: the whole checkout, tools, installs and external inputs.

        This intentionally does not guess which source changes a broad gate can
        ignore. Dirty and untracked source, the privacy roster, runtime content,
        Git configuration and execution settings are bound alongside the commit.
        Only the generated run id is normalized. Unreadable inputs disable reuse.
        """
        library = str(Path(__file__).resolve().parent / "lib")
        if library not in sys.path:
            sys.path.insert(0, library)
        import proof_evidence
        env = {**self.env, "RICHOS_NIGHTLY_RUN_ID": CONDITIONS_RUN_ID}
        inputs = proof_evidence.checkout_identity(self.source, [sys.executable], env)
        inputs["readers"] = {str(path): proof_evidence.file_digest(path) for path in (
            Path(__file__).resolve(), Path(proof_evidence.__file__).resolve(),
            Path(library) / "nightly_gate_evidence.py", Path(library) / "owned_command.py")}
        home = Path(env.get("HOME", str(Path.home())))
        paths = {"privacy": Path(env.get("RICHOS_NAMED_PERSONS_FILE") or
                                 home / ".richos-privacy/named-persons"),
                 "global-git": home / ".gitconfig"}
        if env.get("RICHOS_RUNTIME_DIR"):
            paths["runtime"] = Path(env["RICHOS_RUNTIME_DIR"])
        for i, model in enumerate(env.get("RICHOS_GUI_SPEECH_MODELS", "").split(os.pathsep)):
            if model:
                paths[f"speech-model-{i}"] = Path(model)
        inputs["external"] = {key: proof_evidence.path_identity(path) for key, path in paths.items()}
        inputs["python"] = proof_evidence.path_identity(sys.executable)
        inputs["other_tools"] = {}
        for tool in ("cargo", "cc", "xcrun", "codesign"):
            resolved = shutil.which(tool, path=env.get("PATH"))
            inputs["other_tools"][tool] = proof_evidence.path_identity(resolved) if resolved else {"absent": True}
        tracked = subprocess.run(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
                                 cwd=self.source, env=env, capture_output=True, timeout=15, check=True).stdout
        inputs["linked_inputs"] = {os.fsdecode(p): proof_evidence.path_identity(self.source / os.fsdecode(p))
                                   for p in tracked.split(b"\0") if p and (self.source / os.fsdecode(p)).is_symlink()}
        ui_tree = self.state / self.UI_CHECKOUT
        inputs["ui_modules"] = proof_evidence.checkout_installs(ui_tree) if ui_tree.exists() else {}
        # ui_checkout replaces tracked files with source's commit, but retains
        # untracked fixtures and local installs. Those are additional inputs.
        inputs["ui_untracked"] = {}
        if ui_tree.exists():
            names = subprocess.run(["git", "ls-files", "--others", "--exclude-standard", "-z"],
                                   cwd=ui_tree, env=env, capture_output=True, timeout=15, check=True).stdout
            inputs["ui_untracked"] = {os.fsdecode(p): proof_evidence.path_identity(ui_tree / os.fsdecode(p))
                                      for p in names.split(b"\0") if p}
        inputs["environment"] = {k: proof_evidence.digest(v) for k, v in env.items()}
        inputs["settings"] = [checks_done_at_land, bool(no_host_screen), self.gates_at_once,
                              self.simulated_phones, GATE_BUDGETS, GATE_AFTER]
        inputs["git_config"] = proof_evidence.digest(subprocess.run(
            ["git", "config", "--null", "--list", "--show-origin"], cwd=self.source,
            env=env, capture_output=True, timeout=15, check=True).stdout.hex())
        sha = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.source, env=env,
                             capture_output=True, text=True, timeout=15, check=True).stdout.strip()
        return {"source_commit": sha, "inputs_sha256": proof_evidence.digest(inputs)}

    def reconcile_gates(self, gates, checks_done_at_land, no_host_screen):
        library = str(Path(__file__).resolve().parent / "lib")
        if library not in sys.path:
            sys.path.insert(0, library)
        from nightly_gate_evidence import GateEvidence
        run_id = self.env.get("RICHOS_NIGHTLY_RUN_ID", "")
        if not re.fullmatch(r"\d{8}T\d{6}Z-[0-9a-f]{8}", run_id):
            return gates
        identity_reader = lambda: self.gate_input_identity(checks_done_at_land, no_host_screen)
        try:
            identity = identity_reader()
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            self.announce(f"Gate reuse unavailable: execution inputs cannot be read ({type(error).__name__})")
            return gates
        store = GateEvidence(self.state / "gate-evidence", run_id, identity_reader, self.announce)
        self.gate_evidence = store
        fresh = {"gates/release-smoke"}
        pending = []

        def read_log(name):
            log = self.log
            if isinstance(log, TimestampedLog):
                log.sync()
                with log._file_lock:
                    log.file.flush()
                text = log.path.read_text()
            else:
                text = log.getvalue() if hasattr(log, "getvalue") else log.read()
            start = text.rfind(f"=== phase {name} begins ===")
            return text[text.rfind("\n", 0, start) + 1:] if start >= 0 else text

        def artifacts(name):
            if name in self.skipped:
                return {"skipped": True}
            if name == "gates/script-suites":
                try:
                    report = json.loads((self.state / SUITE_RESULTS).read_text())
                    if report.get("commit") == identity["source_commit"] and report.get("run_id") == run_id:
                        return {"script_suites": report}
                except (OSError, ValueError, AttributeError):
                    pass
                return {"missing": True}
            if name == UI_SUITE_GATE:
                try:
                    receipts = {p.name: json.loads(p.read_text()) for p in
                                (self.state / "ui-receipts").glob("*.receipt.json")}
                    if receipts and all(r.get("commit") == identity["source_commit"] for r in receipts.values()):
                        return {"ui_receipts": receipts}
                except (OSError, ValueError, AttributeError):
                    pass
                return {"missing": True}
            return None

        for name, body in gates:
            if name in fresh:
                pending.append((name, body))
                continue
            saved = store.reusable(name, identity)
            if saved:
                record = saved[1]
                payload = record.get("artifacts") or {}
                valid = name != "gates/script-suites" or isinstance(payload.get("script_suites"), dict)
                if name == UI_SUITE_GATE:
                    valid = self.revalidate_ui_receipts(payload.get("ui_receipts"), run_id)
                if valid:
                    result = store.reuse(name, identity)
                    if name == "gates/script-suites":
                        # Preserve its original run id and commit. The candidate
                        # names this actual run, never a fabricated current pass.
                        (self.state / SUITE_RESULTS).write_text(json.dumps(result["script_suites"]) + "\n")
                    self.reused_gates.add(name)
                    self.skipped[name] = f"reconciled actual pass from {record['run_id']}"
                    self.timings.append((name, None, None))
                    continue
            pending.append((name, store.wrap(name, body, identity, lambda name=name: read_log(name),
                                             lambda name=name: artifacts(name),
                                             stopping=lambda: self.groups.stopping)))
        # Failures, cancellations and missing evidence get the first free slots.
        # Their prerequisite gates retain the same dependency graph and limits.
        pending.sort(key=lambda g: 0 if store.unresolved(g[0], identity) else 1)
        return pending

    def revalidate_ui_receipts(self, receipts, run_id):
        """Reconcile the frozen UI receipt set without starting any UI suite."""
        if not isinstance(receipts, dict) or not receipts:
            return False
        if any(not re.fullmatch(r"[a-z0-9-]+\.receipt\.json", name) for name in receipts):
            return False
        folder = self.state / "gate-evidence" / run_id / "ui-coverage"
        folder.mkdir(parents=True, exist_ok=False)
        for name, receipt in receipts.items():
            (folder / name).write_text(json.dumps(receipt) + "\n")
        args = ["node", "run.js", f"--coverage={folder}"]
        args += [f"--quarantine={suite}" for suite in UI_QUARANTINE]
        try:
            self.command(*args, cwd=self.source / UI_TESTS, timeout=GATE_BUDGETS[UI_SUITE_GATE])
        except RuntimeError:
            self.announce("Saved UI receipts did not verify; the UI gate runs again")
            return False
        return True

    def run_gates(self, gates):
        """Run `gates` -- (name, body) pairs in GATE ORDER -- `gates_at_once` at a time.

        One at a time is the old path exactly: inline, in order, into the run log, and the
        first failure propagates before the next gate starts.

        More than one: each gate runs in its own thread, as soon as a slot is free and every
        gate GATE_AFTER names for it has passed. Each keeps its own deadline (command()),
        its own log (a stamped file of its own, copied whole into the run log as one section
        when it ends) and its own verdict line. The FIRST gate to fail refuses the build:
        no further gate starts, every running one is stopped through the process groups
        this run owns (OwnedGroups), and once all of them have ended the first failure is
        raised as it would have been one at a time. The wall clock of the whole set is
        recorded as the `gates-wall-clock` timing either way.
        """
        names = [name for name, _ in gates]
        unknown = {d for name in names for d in GATE_AFTER.get(name, ())
                   if d not in names and d not in self.reused_gates}
        if unknown:
            raise ValueError(f"GATE_AFTER names gates this build does not run: {sorted(unknown)}")
        limit = len(gates) if self.gates_at_once == "all" else int(self.gates_at_once)
        started = time.time()
        try:
            if limit <= 1:
                pending = list(gates)
                passed = set(self.reused_gates)
                while pending:
                    eligible = next((g for g in pending if all(
                        d in passed for d in GATE_AFTER.get(g[0], ()))), None)
                    if eligible is None:
                        raise RuntimeError("gate prerequisites could not be satisfied")
                    name, body = eligible
                    pending.remove(eligible)
                    body()
                    passed.add(name)
                return
            self._run_gates_side_by_side(gates, limit)
        finally:
            self.timings.append(("gates-wall-clock", started, time.time()))

    def _gate_log(self, name):
        """A log of the gate's own: (writable log, its file or None)."""
        main = self._log
        if isinstance(main, TimestampedLog):
            folder = main.path.with_name(main.path.name + ".gates")
            folder.mkdir(mode=0o700, exist_ok=True)
            path = folder / (name.replace("/", "-") + ".log")
            return TimestampedLog(path), path
        return _BufferedGateLog(), None

    def _merge_gate_log(self, log, path, verdict):
        """Copy a finished gate's log into the run log as one section, then drop the copy."""
        if path is not None:
            self._log.append_verbatim(path.read_text(errors="replace"))
            try:
                path.unlink()
                path.parent.rmdir()  # Succeeds only once the last gate's file is gone.
            except OSError:
                pass
        else:
            text = log.read()
            if text:
                self._log.write(text if text.endswith("\n") else text + "\n")
        self.announce(f"  {verdict}")

    def _run_gates_side_by_side(self, gates, limit):
        import queue
        finished = queue.Queue()
        pending = list(gates)
        running = {}
        passed = set(self.reused_gates)
        first = None      # (name, exception) of the gate that refused the build

        def work(name, body, log):
            self._local.log = log
            error = None
            try:
                body()
            except BaseException as exc:  # carried to the coordinator, which raises it
                error = exc
            finally:
                self._local.log = None
                self._local.phase = None
                if isinstance(log, TimestampedLog):
                    log.close()
                finished.put((name, error))

        try:
            while pending or running:
                if first is None:
                    for gate in list(pending):
                        if len(running) >= limit:
                            break
                        name, body = gate
                        if not all(d in passed for d in GATE_AFTER.get(name, ())):
                            continue
                        pending.remove(gate)
                        log, path = self._gate_log(name)
                        where = f" (its log until it ends: {path})" if path else ""
                        self.announce(f"  started {name}{where}")
                        thread = threading.Thread(target=work, args=(name, body, log),
                                                  name=name, daemon=True)
                        running[name] = (thread, log, path, time.time())
                        thread.start()
                if not running:
                    break
                name, error = finished.get()
                thread, log, path, began = running.pop(name)
                thread.join()
                took = time.time() - began
                # The gate line says how much of its wall clock was queueing for machine worker
                # tokens and how much was its own work; only the second spends its budget.
                note = self.clock_note(name)
                if error is None:
                    passed.add(name)
                    verdict = (f"SKIPPED {name}" if name in self.skipped
                               else f"PASSED {name} in {took:.1f}s{note}")
                elif first is None:
                    first = (name, error)
                    self.groups.stop()
                    verdict = f"FAILED {name} after {took:.1f}s{note}: {error}"
                else:
                    verdict = (f"STOPPED {name} after {took:.1f}s{note}, because {first[0]} failed "
                               f"first ({error})")
                self._merge_gate_log(log, path, verdict)
                if first is not None and first[0] == name and running:
                    self.announce(f"  stopping {', '.join(running)}: {name} refused this build")
        except BaseException:
            # Interrupted here (Ctrl-C): stop what runs, and wait for the owners' bounded cleanup.
            self.groups.stop()
            for thread, *_ in running.values():
                thread.join(timeout=CLEANUP_TIMEOUT + TERM_GRACE + KILL_GRACE)
            raise
        if first is not None:
            if pending:
                self.announce(f"  never started: {', '.join(name for name, _ in pending)}")
            raise first[1]
        if pending:
            # Reachable only if GATE_AFTER could never be satisfied: say so, never pass.
            raise RuntimeError(f"gates never started: {', '.join(n for n, _ in pending)}")

    def run_pointer(self, run_id):
        if not run_id or "/" in run_id or run_id in (".", ".."):
            raise ValueError("a valid --run <run-id> is required")
        return self.state / "runs" / f"{run_id}.json"

    def record_run(self, run_id, out):
        path = self.run_pointer(run_id)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps({"out": str(out)}, indent=2) + "\n")

    def load_candidate(self, run_id):
        path = self.run_pointer(run_id)
        if not path.exists():
            raise ValueError(f"no recorded build for run {run_id}; run \"build\" first")
        out = Path(json.loads(path.read_text())["out"])
        candidate = out / "candidate.json"
        if not candidate.exists():
            raise ValueError(f"{out} has no candidate.json; the build for run {run_id} "
                             "did not finish")
        return out, json.loads(candidate.read_text())["info"]

    def print_candidate(self, info, out):
        # This Mac is Apple Silicon only (see main()'s own precondition), so the
        # first-install archive's arch component is always "aarch64" --
        # make-release.sh's own ARCH mapping (arm64|aarch64 -> aarch64).
        bundle_zip = out / f"RichOS-{info['version']}-macos-aarch64.zip"
        _scratch, recipe = walk_recipe(bundle_zip, info['run_id'])
        number = info['version'].rsplit('.', 1)[-1]
        print(f"Candidate build {number}: {info['tag']}, source {info['source_commit']}", flush=True)
        print(f"  staged at : {out}", flush=True)
        print(f"  bundle zip: {bundle_zip}", flush=True)
        print("", flush=True)
        print("To walk it, in the test VM and never on this Mac's screen (CEO ruling §65): unpack", flush=True)
        print("into a scratch home so it never touches the operator's own app data (README.md's", flush=True)
        print("activation invariant D). HOME and CFFIXED_USER_HOME are both that home, because", flush=True)
        print("HOME alone leaves WebKit's store in the real ~/Library. Run it directly under", flush=True)
        print("`env -i` rather than via `open`, so a harness holds the process (invariant P) and", flush=True)
        print("nothing from this shell reaches it; RICHOS_ACTIVATION=regular shows the window:", flush=True)
        print("", flush=True)
        for line in recipe:
            print("  " + line, flush=True)
        print("", flush=True)
        if info.get("candidate_ref"):
            print("The candidate has no public version tag or release-list entry. Its digest-named", flush=True)
            print("engine is public on the existing nightly channel release. The app is not", flush=True)
            print("installed or offered as an update; the update channel has not moved.", flush=True)
        else:
            print("This legacy candidate has an engine-only prerelease. The app is not installed", flush=True)
            print("or offered as an update; the update channel has not moved.", flush=True)
        print("", flush=True)
        print(f"Publish only on a READY verdict:  nightly-local.py publish --run {info['run_id']}", flush=True)

    @staticmethod
    def gui_boot_state(info):
        """What this candidate recorded for the suite that boots the app: `passed`, `gap`,
        `not-run`, or None when the build recorded nothing either way.

        NONE IS NOT THE SAME AS `not-run`, AND THE FIRST VERSION OF THIS TREATED THEM AS
        ONE. "Silence is never read as a pass" is a fine-sounding rule and it would have
        made every candidate already staged on this Mac unpublishable, because none of them
        carries a field that did not exist when they were built — `nightly-local.test.py`'s
        publish case said so within a minute of the change. Those builds DID run
        gui-boot.test.sh; it was not skippable then. So silence is read for what it is:
        a build from before the field, which ran the suite the only way there was.

        The case that must never be waved through is a build that was TOLD not to use the
        screen and then produced no report, and `no_host_screen` in the plan is what
        distinguishes it — see `require_gui_proof`.
        """
        for suite in (info.get("script_suites") or {}).get("suites", []):
            if suite.get("name") == "gui-boot.test.sh":
                return suite.get("state")
        return None

    @staticmethod
    def read_gui_proof(path):
        """Parse a `--gui-proof` file into its fields, or refuse with a reason.

        Hand-parsed rather than trusted: this file is the ONE thing standing between a
        candidate whose boot was never exercised and a published release, so a malformed
        one is refused loudly instead of being read past.
        """
        try:
            text = Path(path).expanduser().read_text(errors="replace")
        except OSError as error:
            raise ValueError(f"--gui-proof {path} cannot be read: {error}") from None
        head, _, _ = text.partition("\n--- output ---")
        if not head.startswith("richos-gui-proof 1"):
            raise ValueError(
                f"{path} is not a gui-boot proof: it does not begin with `richos-gui-proof 1`. "
                "`run-tests.sh --only gui-boot.test.sh --proof-out <path>` writes one, and "
                "`scripts/gui-proof-in-vm.sh` writes one from a candidate's own bundle.")
        fields = {}
        for line in head.splitlines()[1:]:
            name, sep, value = line.partition("=")
            if sep:
                fields[name.strip()] = value.strip()
        return fields

    @classmethod
    def needs_gui_proof(cls, info):
        """True when nobody has seen this candidate boot: the same rule require_gui_proof
        enforces, asked before publish so the build can take the proof itself."""
        state = cls.gui_boot_state(info)
        if state == "passed":
            return False
        return state is not None or bool(info.get("no_host_screen"))

    def gui_proof_path(self, run_id):
        """Where the build keeps its own VM boot proof: gui-proof-in-vm.sh's default."""
        return self.state / "gui-proofs" / f"{run_id}.proof"

    def proven_boot_leftover(self, proof, started):
        """The guest name a passing proof written during this attempt names, else None.

        Read only after the boot command failed. A proof from an earlier attempt of the same
        run (older than `started`), a missing or unreadable one, or any result but `pass`
        answers None: that failure stays "not proven"."""
        try:
            if Path(proof).stat().st_mtime < started - 1:
                return None
            fields = self.read_gui_proof(proof)
        except (OSError, ValueError):
            return None
        if fields.get("result") != "pass":
            return None
        where = fields.get("where", "")
        return where.partition(":")[2] or where or "the guest named in the run log"

    def vm_boot_proof(self, run_id):
        """Boot the candidate's own signed bundle in a test-VM guest. None on a pass, else
        the sentence that says what did not happen and how to take it again.

        Nothing here touches this Mac's screen: gui-proof-in-vm.sh boots, watches and deletes
        a guest on the guest's own display, inside one of the two guest slots.
        """
        proof = self.gui_proof_path(run_id)
        args = ["bash", self.source / SCRIPTS / "gui-proof-in-vm.sh", "--run", run_id, "--out", proof]
        if self.gui_host:
            args += ["--vm", self.gui_host]
        # Written literally for this one step: where the run pointer lives, and the testvm
        # settings the operator's shell gave (TESTVM_ROOT, TESTVM_SLOTS, ...).
        extra = {"RICHOS_NIGHTLY_STATE": str(self.state), **self.vm_settings}
        guest = f"guest {self.gui_host}" if self.gui_host else "a fresh guest"
        self.announce(f"Booting the candidate's own bundle in {guest} on the test VM "
                      "(nothing is put on this Mac's screen)...")
        started = time.time()
        try:
            with self.phase(VM_PROOF_PHASE):
                self.command(*args, env_extra=extra, timeout=VM_PROOF_BUDGET)
        except RuntimeError as error:
            leftover = self.proven_boot_leftover(proof, started)
            if leftover is not None:
                # gui-proof-in-vm.sh exits 1 over a passing proof only when it could not stop its
                # guest (its cleanup). The boot WAS proven and publish accepts the proof as it
                # always did; what is wrong is a guest left on this Mac (finding 19 follow-up).
                return (f"THE CANDIDATE IS BUILT AND ITS BOOT WAS PROVEN IN THE TEST VM ({proof} says "
                        f"result=pass), BUT THE GUEST COULD NOT BE STOPPED AFTERWARD: {error}. "
                        f"Left behind: {leftover}. Delete it with richos/app/scripts/testvm/stop.sh "
                        f"{leftover} and richos/app/scripts/testvm/reap.sh. `publish --run {run_id}` "
                        "still accepts the proof; nothing needs a rebuild or a second boot.")
            return (f"THE CANDIDATE IS BUILT, AND ITS BOOT WAS NOT PROVEN IN THE TEST VM: {error}. "
                    f"The proof file, if any, is {proof}. `publish` refuses this candidate until a "
                    f"passing proof exists; take it again with "
                    f"richos/app/scripts/gui-proof-in-vm.sh --run {run_id} (no rebuild needed).")
        self.announce(f"VM boot proof PASSED: {proof}. `publish --run {run_id}` uses it without --gui-proof.")
        return None

    def require_gui_proof(self, info, gui_proof, run_id=None):
        """Refuse to publish a candidate whose boot nobody has seen.

        `--no-host-screen` buys a build that never touches this Mac's screen by NOT running
        `gui-boot.test.sh`. That is a trade, and this is the other half of it: the evidence
        has to arrive before anything becomes installable, or the mode is just a way of
        skipping a gate.
        """
        if not gui_proof and self.needs_gui_proof(info):
            # The build takes this proof itself (vm_boot_proof) and keeps it here, so nobody
            # has to carry its path to this command. It is checked exactly like a given one.
            own = self.gui_proof_path(run_id or info.get("run_id") or "none")
            if own.is_file():
                print(f"Using the build's own VM boot proof: {own}", flush=True)
                gui_proof = own
        state = self.gui_boot_state(info)
        if state is None and not info.get("no_host_screen"):
            # A candidate from before this field existed. `build` had no way to hold the
            # suite back, so it ran. See gui_boot_state.
            if gui_proof:
                raise ValueError(
                    "this candidate predates the gui-boot record and was built with the suite "
                    "running, so --gui-proof names evidence nothing is waiting for.")
            return None
        if state == "passed":
            if gui_proof:
                raise ValueError(
                    "this candidate ran gui-boot.test.sh during its build and it passed, so "
                    "--gui-proof names evidence nothing is waiting for. Publish without it.")
            return None
        if not gui_proof:
            recorded = state or "nothing at all, from a build that was told --no-host-screen"
            raise ValueError(
                f"this candidate recorded `{recorded}` for gui-boot.test.sh, so no one has "
                "seen the app it is about to publish actually boot.\n"
                "  Take the proof against this exact commit and pass it back:\n"
                f"    richos/app/scripts/gui-proof-in-vm.sh --run {info['run_id']}\n"
                f"    nightly-local.py publish --run {info['run_id']} --gui-proof <the file it names>\n"
                "  Nothing here opens a window on this Mac; the app boots inside a guest.")
        fields = self.read_gui_proof(gui_proof)
        # TWO KINDS OF EVIDENCE, AND THEY ARE NOT THE SAME CLAIM, so they are named
        # differently and both are accepted for what each one is:
        #
        #   gui-boot.test.sh      the suite: a debug binary built from the checkout, held
        #                         to B0-B8 and C1-C5 on a synthetic machine -- engine
        #                         resolution, the plist's shape, the company registry.
        #   shipped-bundle-boot   `gui-proof-in-vm.sh`: THE ARTIFACT THIS RELEASE WILL
        #                         PUBLISH -- signed, notarized, stapled -- started on a
        #                         clean guest with no developer environment, drawing a
        #                         real window. Fewer assertions, truer artifact.
        #
        # A proof that borrowed the other's name would be the most useful lie in the
        # release chain, so neither may.
        accepted_suites = ("gui-boot.test.sh", "shipped-bundle-boot")
        if fields.get("suite") not in accepted_suites:
            raise ValueError(f"--gui-proof names a proof for {fields.get('suite')!r}. "
                             f"Only {' or '.join(accepted_suites)} says whether the app boots.")
        if fields.get("result") != "pass":
            raise ValueError(f"--gui-proof records {fields.get('result')!r}, not a pass. "
                             "A failed boot is not evidence of a working one.")
        # THE SHA IS THE WHOLE POINT, exactly as --checks-done-at-land's is: a proof taken
        # against a different tree proves something about that tree. `build_commit` is the
        # commit the bundle was compiled from (it carries the version bump); `source_commit`
        # is its parent. Either identifies THIS candidate and nothing else.
        accepted = [s for s in (info.get("build_commit"), info.get("source_commit")) if s]
        seen = fields.get("commit", "")
        #
        # An abbreviation is accepted because a proof file can be written by hand, but only at
        # 7-40 lowercase hex digits (the same floor --checks-done-at-land enforces): one shared
        # character names thousands of unrelated commits, so a shorter prefix identifies nothing.
        if not re.fullmatch(r"[0-9a-f]{7,40}", seen) or not any(s.startswith(seen) for s in accepted):
            raise ValueError(
                f"--gui-proof was taken against {seen or '<no commit>'}, and this candidate is "
                f"{' / '.join(accepted)}. The proof is about a different tree, so it proves "
                "nothing about this one; take it again against this candidate.")
        return fields

    def accept_land_proof(self, sha, source):
        """Return the sha to record, or refuse. Never a bare boolean.

        The flag says "a land already ran this, on THIS source". The only thing that can
        make that true is the sha, so it is compared against the sha this run actually
        fetched -- not against `main`, not against HEAD, and never taken on trust. An
        abbreviation is accepted because Rich passes one by hand, but only at git's own
        minimum unambiguous length, and only as a prefix of the fetched sha.
        """
        if not re.fullmatch(r"[0-9a-f]{7,40}", sha or ""):
            raise ValueError("--checks-done-at-land needs a hex commit sha of at least 7 characters")
        if not source.startswith(sha):
            raise ValueError(
                f"--checks-done-at-land {sha} is not the source this run fetched ({source}). "
                "The land proved a different tree, so its checks prove nothing about this one; "
                "run without the flag.")
        return source

    LOG_LINE = re.compile(r"^\[\d{4}-\d\d-\d\dT[\d:.]+Z\] (.*)$")
    GATE_VERDICT = re.compile(r"^ {0,2}(PASSED|FAILED|STOPPED|SKIPPED) (gates/[a-z-]+)\b")

    DEPENDENCIES_LINE = re.compile(r"Installed dependencies: ([0-9a-f]{64})")

    def installed_dependencies_digest(self):
        """The identity of what the gates execute that git does not track, or None.

        The SAME identity the proof runner binds a saved pass to (`lib/proof_evidence.py`
        `installed_dependencies`: git-ignored `node_modules`, Playwright browser installs and
        the Rust toolchain), read in the dedicated source worktree with this run's
        environment. None when it cannot be read: an unreadable state is never a match.
        """
        library = str(Path(__file__).resolve().parent / "lib")
        if library not in sys.path:
            sys.path.insert(0, library)
        import proof_evidence
        try:
            return proof_evidence.digest(proof_evidence.installed_dependencies(self.source, self.env))
        except (OSError, ValueError, subprocess.SubprocessError):
            return None

    def accept_passed_gates(self, run_id, command):
        """What `--gates-passed-in RUN` may reuse: RUN's source and its suite record, or refuse.

        Read from RUN's log, which the coordinator writes whatever happens, and only from
        the coordinator's own lines: verdicts outside every gate's merged section. Even a
        forged PASSED could not pass a failed run, because a failed run never reaches
        `plan-recheck`.
        """
        if not re.fullmatch(r"\d{8}T\d{6}Z-[0-9a-f]{8}", run_id or ""):
            raise ValueError(f"{PASSED_GATES_FLAG} needs a run id like 20260928T190111Z-40a16163")
        path = self.state / "logs" / f"{run_id}.log"
        try:
            raw = path.read_text(errors="replace").splitlines()
        except OSError as error:
            raise ValueError(f"{PASSED_GATES_FLAG} {run_id}: its log cannot be read ({error})") from None
        # A gate's own output sits between its `=== phase gates/X begins/ends ===` lines
        # (merged whole when it ends); the coordinator's verdict follows the section. Only
        # lines outside every section are the coordinator's, and only those are read.
        outer, ended, inside = [], set(), []
        for match in map(self.LOG_LINE.match, raw):
            line = match[1] if match else None
            section = line and re.fullmatch(r"=== phase (gates/[a-z-]+(?:/[a-z-]+)*) (begins ===|ends: [\d.]+s ===)", line)
            if section:
                if section[2] == "begins ===":
                    inside.append(section[1])
                elif inside and inside[-1] == section[1]:
                    inside.pop()
                    ended.add(section[1])
            elif line is not None and not inside:
                outer.append(line)
        source = next((m[1] for m in (re.fullmatch(r"Source: ([0-9a-f]{40})", l) for l in outer) if m), None)
        if not source:
            raise ValueError(f"run {run_id} never recorded the source it fetched, so there is nothing to resume")
        verdicts = {}
        for match in filter(None, map(self.GATE_VERDICT.match, outer)):
            verdicts.setdefault(match[2], set()).add(match[1])
        bad = sorted(f"{gate} {verdict}" for gate, seen in verdicts.items()
                     for verdict in seen - {"PASSED"})
        if bad:
            raise ValueError(f"run {run_id} did not pass every gate on {source[:12]}: {', '.join(bad)}. "
                             "Run the gates again.")
        # One gate at a time writes no verdict line; a gate that ended and a run that went
        # on to plan-recheck is the same fact there, because the first failure raises.
        one_at_a_time = any(re.match(r"Gates at once: 1 \(", l) for l in outer)
        missing = [g for g in GATE_NAMES if "PASSED" not in verdicts.get(g, ()) and
                   not (one_at_a_time and g in ended)]
        if missing:
            raise ValueError(f"run {run_id} has no pass recorded for {', '.join(missing)} on "
                             f"{source[:12]}. Run the gates again.")
        if "=== phase plan-recheck begins ===" not in outer:
            raise ValueError(f"run {run_id} stopped before its gates returned; nothing it ran can be reused")
        after = outer[len(outer) - 1 - outer[::-1].index("=== phase plan-recheck begins ==="):]
        grade = ("release" if any(l.startswith("Building and publishing v") for l in after) else
                 "build" if any(re.fullmatch(r"Building v\S+\.\.\.", l) for l in after) else None)
        if grade is None or (command == "release" and grade != "release"):
            raise ValueError(f"run {run_id} was a {grade or 'unknown'} run; `release` reuses only the "
                             "gates of a release or stable run, which never skip a suite over "
                             "unchanged inputs")
        try:
            suites = json.loads((self.state / SUITE_RESULTS).read_text())
        except (OSError, ValueError):
            suites = None
        if not isinstance(suites, dict) or suites.get("commit") != source or suites.get("run_id") != run_id:
            raise ValueError(f"{self.state / SUITE_RESULTS} is no longer run {run_id}'s record of its "
                             "script suites, and the candidate's provenance needs it. Run the gates again.")
        recorded = [m[1] for m in filter(None, map(self.DEPENDENCIES_LINE.fullmatch, outer))]
        now = self.installed_dependencies_digest()
        if not recorded:
            stale = "the run recorded no identity for the dependencies its gates ran against"
        elif now is None:
            stale = "the installed dependencies cannot be read here now"
        elif recorded[-1] != now:
            stale = (f"the installed dependencies differ from the ones run {run_id} ran its gates "
                     f"against (then {recorded[-1][:12]}, now {now[:12]})")
        else:
            stale = None
        return {"run_id": run_id, "source_commit": source, "script_suites": suites,
                "stale_dependencies": stale}

    def perform(self, command, force=False, runtime=None, run_id=None,
                checks_done_at_land=None, no_host_screen=False, gui_proof=None,
                from_nightly=None, dry_run=False, gates_passed_in=None):
        passed = None
        if gates_passed_in:
            if command not in ("build", "release") or checks_done_at_land or no_host_screen:
                raise ValueError(f"{PASSED_GATES_FLAG} is for build or release, alone: it runs no gate, "
                                 "so nothing else about the gates means anything beside it")
            # Before any network call: the whole question is answered by files on this Mac.
            passed = self.accept_passed_gates(gates_passed_in, command)
            if passed["stale_dependencies"]:
                # The earlier pass certified another dependency state: run the gates.
                self.announce(f"Gates run: {passed['stale_dependencies']}, so run {gates_passed_in}'s "
                              f"pass on {passed['source_commit']} ({PASSED_GATES_FLAG}) is not reused.")
                passed = None
                gates_passed_in = None
            else:
                self.announce(f"Gates: none run. Every gate passed on {passed['source_commit']} in run "
                              f"{gates_passed_in} ({PASSED_GATES_FLAG}); this run builds that commit, "
                              "and the installed dependencies are the ones they ran against.")
        if not gates_passed_in and command in GATE_COMMANDS and not (command == "stable" and dry_run):
            # The run log's first lines: what runs at once, and who chose it.
            self.record_settings()
            if command == "build":
                # And which screen the screen suites get, with whatever the shell gave for the
                # test VM, so it is never reconstructed from a missing effect afterwards.
                self.announce("Screen suites: " + (
                    "never on this Mac's display; the signed candidate is booted in the test VM"
                    + (f" (guest {self.gui_host}, from {GUI_HOST_ENV})" if self.gui_host else "")
                    if no_host_screen else "on THIS Mac's display (--host-screen)"))
                if self.vm_settings:
                    self.announce("Handed to the VM boot proof from your shell: "
                                  + ", ".join(sorted(self.vm_settings)))
        if command == "stable":
            # THE COMMIT, NOT THE BYTES. T3 Code's sentence, which is what is being copied
            # (`t3code:.github/workflows/release.yml:44-47`): *"Manual stable releases
            # build the commit of the latest published nightly, so stable only ever ships
            # a build that nightly users have already run."*
            #
            # The worktree starts at main because that is where his promotion record and
            # this tooling live; only once the plan is resolved does it move to the older
            # commit, which is then built by its own release scripts.
            with self.phase("fetch"):
                self.checkout()
            with self.phase("stable-plan"):
                plan_path, info = self.stable_plan(from_nightly)
            if dry_run:
                print(f"DRY RUN -- {from_nightly} would be rebuilt as a stable release.",
                      flush=True)
                self.describe_stable(info)
                self.summary()
                return
            with self.phase("source"):
                self.checkout(info["source_commit"])
            with self.phase("preflight"):
                self.preflight()
            with self.phase("runtime-verify"):
                self.runtime(runtime)
            # EVERY GATE, EVERY TIME. `release` does the same and for the same reason:
            # skipping is for candidates nobody can install, and this is the command that
            # makes a build the one every stable user receives.
            self.gates(no_host_screen=no_host_screen)
            out = self.state / "releases" / info["tag"]
            self.announce(f"Building and publishing {info['tag']} from "
                          f"{info['source_commit'][:12]}...")
            with self.phase("build"):
                self.command(sys.executable, self.source / SCRIPTS / "nightly.py", "run",
                             "--plan", plan_path, "--out", out, credentials=True,
                             release_build=True)
            print(f"Published https://github.com/{REPO}/releases/tag/{info['tag']}",
                  flush=True)
            self.summary()
            return
        if command == "candidate":
            out, info = self.load_candidate(run_id)
            self.print_candidate(info, out)
            return
        if command == "publish":
            out, info = self.load_candidate(run_id)
            if not self.source.exists():
                raise ValueError(f"{self.source} no longer exists; the dedicated worktree that "
                                 f"built run {run_id} is gone, so it cannot be published from here")
            # BEFORE the network, the upload and the channel move: this is a refusal about
            # what was never checked, and it costs nothing to make it first.
            proof = self.require_gui_proof(info, gui_proof, run_id=run_id)
            if proof:
                print(f"gui-boot proof accepted: {proof.get('result')} at {proof.get('where')}, "
                      f"commit {proof.get('commit')}, taken {proof.get('at')}", flush=True)
            # A `check` or another `build` since this candidate was built moved the worktree
            # that `finish` runs from; put its build commit back rather than refuse.
            self.restore_build_commit(info)
            print(f"Publishing {info['tag']} from run {run_id}...", flush=True)
            # finish() only uploads, verifies and promotes -- no signing identity or
            # notarization credential is needed here, unlike the build step below.
            self.command(sys.executable, self.source / SCRIPTS / "nightly.py", "finish", "--out", out)
            print(f"Published https://github.com/{REPO}/releases/tag/{info['tag']}", flush=True)
            return

        with self.phase("fetch"):
            source = self.checkout(passed["source_commit"] if passed else None)
            if passed:
                # checkout() fetched main; the resumed commit must still be on it.
                try:
                    self.command("git", "merge-base", "--is-ancestor", source, "FETCH_HEAD", cwd=self.repo)
                except RuntimeError:
                    raise ValueError(f"{source} is no longer on main, so its gates prove nothing "
                                     "about what main is; run the gates again") from None
        with self.phase("plan"):
            plan_path, info = self.plan(force)
        self.announce(f"Source: {source}")
        if checks_done_at_land:
            checks_done_at_land = self.accept_land_proof(checks_done_at_land, source)
        if not info["build"] and command in ("release", "build"):
            print(info["reason"] + "; nothing published.", flush=True)
            return
        with self.phase("preflight"):
            self.preflight()
        if command == "check":
            recipe = self.source / SCRIPTS / "runtime-sources.json"
            if runtime or (recipe.is_file() and self.runtime_path().exists()):
                with self.phase("runtime-verify"):
                    self.runtime(runtime)
            print("Preflight passed. No release was triggered or published.", flush=True)
            self.summary()
            return
        if command not in ("release", "build"):
            raise ValueError("only build, release or publish may build or publish")
        with self.phase("runtime-verify"):
            self.runtime(runtime)
        # SKIPPING IS FOR CANDIDATES, NEVER FOR THE COMMAND THAT PUBLISHES. `release` runs
        # every suite every time, whatever a proof file on this host remembers.
        if passed:
            for name in GATE_NAMES:
                self.skip(name, f"passed in run {passed['run_id']} on {passed['source_commit']}")
            suites = passed["script_suites"]
        else:
            suites = self.gates(checks_done_at_land, no_host_screen=no_host_screen,
                                skip_unchanged=(command == "build"))
            # What these gates ran against, so `--gates-passed-in` can tell whether a later
            # run has the same installed dependencies. Read after the gates: they may install.
            digest = self.installed_dependencies_digest()
            if digest:
                self.announce(f"Installed dependencies: {digest}")
        # Capture the UTC date at allocation, even if checks crossed midnight.
        with self.phase("plan-recheck"):
            plan_path, info = self.plan(force)
        if not info["build"]:
            print(info["reason"] + "; nothing published.", flush=True)
            return
        if no_host_screen or isinstance(suites, dict):
            # Into the plan for the same reason the land proof is: nightly.py copies the
            # plan verbatim into build-info.json and the candidate's committed provenance,
            # so which suites ran, which were skipped over unchanged inputs, and which were
            # held back from the screen travel WITH the candidate instead of living in a
            # log on this Mac.
            #
            # `no_host_screen` is recorded EVEN IF the suite report went missing, and that
            # is the whole point of recording it separately: a screenless build with no
            # report is the one shape `publish` must refuse, and it can only tell that
            # shape from an old candidate's silence by this flag.
            info = {**info, "no_host_screen": bool(no_host_screen)}
            if isinstance(suites, dict):
                info["script_suites"] = suites
            plan_path.write_text(json.dumps(info, indent=2) + "\n")
        if self.gate_evidence:
            info = {**info, "gate_evidence": self.gate_evidence.selected,
                    "checks_skipped": sorted(self.skipped)}
            plan_path.write_text(json.dumps(info, indent=2) + "\n")
        if checks_done_at_land:
            # Into the plan, because nightly.py copies the plan verbatim into the
            # candidate's `build-info.json` and its committed provenance. A skip recorded
            # only in a log lives on this Mac; a skip recorded here travels with the
            # candidate to whoever walks it.
            # WHAT WAS ACTUALLY SKIPPED, not what this line used to assume was. It named
            # `[LAND_PROVEN_GATE]` literally, which was true while exactly one gate could be
            # skipped; the UI suite can be skipped too now, and only when its own coverage
            # proof exists for this sha. Reading the runner's own record of its skips means
            # the candidate's provenance cannot claim a gate ran that did not, or the
            # reverse, the next time a third skippable gate appears.
            info = {**info, "checks_done_at_land": checks_done_at_land,
                    "checks_skipped": sorted(self.skipped)}
            plan_path.write_text(json.dumps(info, indent=2) + "\n")
        if passed:
            # Into the candidate's provenance for the same reason: the run that proved
            # these gates is named wherever the candidate goes.
            info = {**info, "gates_passed_in": {"run_id": passed["run_id"],
                                                "source_commit": passed["source_commit"]},
                    "checks_skipped": sorted(self.skipped)}
            plan_path.write_text(json.dumps(info, indent=2) + "\n")
        out = self.state / "releases" / info["tag"]
        if command == "release":
            self.announce(f"Building and publishing {info['tag']}...")
            # The publisher builds, signs, notarizes and signs the updater manifest; it is
            # the one step that reads these variables out of its environment
            # (make-release.sh:359 and its notarize_env at :410).
            with self.phase("build"):
                self.command(sys.executable, self.source / SCRIPTS / "nightly.py", "run",
                             "--plan", plan_path, "--out", out, credentials=True,
                             release_build=True)
            print(f"Published https://github.com/{REPO}/releases/tag/{info['tag']}", flush=True)
            self.summary()
            return
        # command == "build": stop at the signed, notarized, engine-pinned candidate.
        self.announce(f"Building {info['tag']}...")
        with self.phase("build"):
            # The engine asset's reproducibility re-build (~25 s, measured 2026-09-19) may
            # be remembered for a CANDIDATE when the bytes and the build script are both
            # byte-identical to a run that already proved them. Passed for `build` and
            # never for `release`, exactly like skip-when-unchanged above: a proof file on
            # this host may excuse work for a candidate, never for the command that makes
            # a build installable. See make-engine-asset.sh's CHECK_PROOF_DIR note.
            self.command(sys.executable, self.source / SCRIPTS / "nightly.py", "build",
                         "--plan", plan_path, "--out", out, credentials=True,
                         env_extra={"RICHOS_ENGINE_CHECK_PROOF_DIR":
                                    str(self.state / "engine-repro-proofs")},
                         release_build=True)
        candidate_info = json.loads((out / "candidate.json").read_text())["info"]
        self.record_run(self.env["RICHOS_NIGHTLY_RUN_ID"], out)
        # THE SCREEN SUITES' PROOF, TAKEN HERE AND NOT LEFT FOR SOMEBODY TO REMEMBER: a
        # candidate whose boot nobody watched is booted in the test VM now (see VM_PROOF_PHASE).
        proof_error = None
        if self.needs_gui_proof(candidate_info):
            proof_error = self.vm_boot_proof(self.env["RICHOS_NIGHTLY_RUN_ID"])
        self.summary()
        print("", flush=True)
        self.print_candidate(candidate_info, out)
        if proof_error:
            raise RuntimeError(proof_error)


def gates_at_once_value(text):
    """`--gates-at-once`: a whole number of at least 1, or `all`."""
    if text == "all":
        return "all"
    if text.isdigit() and int(text) >= 1:
        return int(text)
    raise argparse.ArgumentTypeError(f"{text!r} is not a whole number of at least 1, or all")


def simulated_phones_value(text):
    """`--simulated-phones`: a whole number of at least 1."""
    if text.isdigit() and int(text) >= 1:
        return int(text)
    raise argparse.ArgumentTypeError(f"{text!r} is not a whole number of at least 1")


def chosen_by():
    """Who named the settings: this account, on this command line, started from what."""
    import pwd
    user = pwd.getpwuid(os.getuid()).pw_name
    try:
        parent = subprocess.run(["ps", "-o", "comm=", "-p", str(os.getppid())],
                                capture_output=True, text=True, timeout=5).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        parent = ""
    started = f" (started from {Path(parent).name}, pid {os.getppid()})" if parent else ""
    return f"{user} on the command line{started}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["check", "build", "publish", "candidate",
                                           "release", "stable", "gate-environment"])
    parser.add_argument("--repo", type=Path, default=ROOT)
    parser.add_argument("--state-dir", type=Path, default=Path.home() / ".richos-nightly")
    parser.add_argument("--runtime-dir", type=Path, help="existing verified runtime cache")
    parser.add_argument("--force", action="store_true", help="explicitly rebuild a previously released source")
    parser.add_argument("--run", help="an existing build's run id (required for publish/candidate)")
    parser.add_argument("--checks-done-at-land", metavar="SHA",
                        help="skip the gates a land already ran whole on this exact commit "
                             "(cargo test -p richos-core when the land's receipt for SHA's "
                             "tree shows the unfiltered crate ran, and the sharded UI suite "
                             "when a coverage proof for SHA is on this machine); refused "
                             "unless SHA is the sha this run fetches")
    parser.add_argument("--no-host-screen", action="store_true",
                        help="the DEFAULT for build: hold back every suite that boots the app "
                             "on this Mac's screen; the build then boots the signed candidate "
                             "in the test VM itself, and publish uses that proof")
    parser.add_argument("--host-screen", action="store_true",
                        help="build only: run the screen suites on THIS Mac's display instead "
                             "of the test VM. Never on the CEO's Mac (ruling §65)")
    parser.add_argument("--from-nightly", metavar="TAG",
                        help="the published nightly whose SOURCE COMMIT `stable` rebuilds; "
                             "refused unless the CEO's own promotion decision for that exact "
                             "tag is recorded (ceo-decisions §69)")
    parser.add_argument("--dry-run", action="store_true",
                        help="print exactly what `stable` would build and publish, and stop")
    parser.add_argument(GATES_AT_ONCE_FLAG, type=gates_at_once_value, metavar="N|all",
                        help="REQUIRED for build, release and stable: how many gates run at the "
                             "same time. 1 runs them one after another; all starts every gate "
                             "whose inputs are ready. Choose it from what else is running on "
                             "this Mac.")
    parser.add_argument(SIMULATED_PHONES_FLAG, type=simulated_phones_value, metavar="N",
                        help="optional for build, release and stable: how many simulated "
                             "iPhones the suites may use at once (one per device type; the "
                             "machine still boots at most two simulators at a time). No suite "
                             "in the desktop build leases one; not named, a suite that did "
                             "would get one at a time.")
    parser.add_argument(PASSED_GATES_FLAG, metavar="RUN_ID",
                        help="build or release: run no gate, and build the source commit of RUN_ID, "
                             "a recorded run whose log shows every gate PASSED on that commit; "
                             "refused otherwise (see the note above GATE_NAMES)")
    parser.add_argument("--gui-proof", metavar="PATH",
                        help="a gui-boot proof taken against this candidate's commit, required "
                             "to publish a candidate built with --no-host-screen")
    args = parser.parse_args()
    # THE ONE COMMAND THAT BUILDS NOTHING, SIGNS NOTHING AND NEEDS NO STATE. It prints the
    # declared gate environment so a test in another language can read the list instead of
    # keeping a copy of it: `run-tests.test.sh` case E1 re-runs the whole suite under every
    # variable a build exports, and a hand-written copy of that list goes stale in silence
    # the next time one is added. It runs before the Apple-Silicon precondition on purpose --
    # it is a question about this file's source, not about this machine.
    if args.command == "gate-environment":
        for kind, names in (("passthrough", GATE_PASSTHROUGH),
                            ("set", GATE_SET_BY_BUILD),
                            ("per-step", GATE_SET_PER_STEP)):
            for name in names:
                print(f"{kind}\t{name}")
        return
    if args.command in ("publish", "candidate") and not args.run:
        parser.error(f"{args.command} requires --run <run-id> (see the output of a prior `build`)")
    if args.checks_done_at_land and args.command not in ("build", "release"):
        parser.error("--checks-done-at-land only means anything for build or release")
    if args.no_host_screen and args.command not in NO_HOST_SCREEN_COMMANDS:
        parser.error("--no-host-screen is for `build`. `release` publishes in one motion with "
                     "no publish step to refuse an unproven boot, so it always runs the whole "
                     "gate; the screenless path is build -> walk -> publish --gui-proof.")
    if args.host_screen and (args.command != "build" or args.no_host_screen):
        parser.error("--host-screen is for `build` alone, and it contradicts --no-host-screen: "
                     "say which screen the suites may use")
    # `build` never uses this Mac's screen unless told to (NO_HOST_SCREEN_COMMANDS, above). A
    # build reusing a recorded run's gates runs no screen suite at all, so it is not set there.
    no_host_screen = args.no_host_screen or (
        args.command == "build" and not args.host_screen and not args.gates_passed_in)
    if args.command == "stable" and not args.from_nightly:
        parser.error("stable requires --from-nightly <tag>: a stable release is a rebuild "
                     "of the commit a published nightly was built from, and it is refused "
                     "unless the CEO has himself tested that nightly and his decision to "
                     "promote it is recorded (ceo-decisions §69)")
    if args.from_nightly and args.command != "stable":
        parser.error("--from-nightly names the nightly `stable` rebuilds; it means nothing "
                     "to any other command")
    if args.dry_run and args.command != "stable":
        parser.error("--dry-run is implemented for `stable`. For a nightly, `build` already "
                     "produces the whole train without publishing anything: nobody's "
                     "existing install can see or fetch what it makes.")
    if args.gui_proof and args.command != "publish":
        parser.error("--gui-proof is evidence `publish` demands; it means nothing to any other "
                     "command")
    if args.gates_passed_in and args.command not in ("build", "release"):
        parser.error(f"{PASSED_GATES_FLAG} names a run whose gates a build or release reuses; it means "
                     f"nothing to {args.command}")
    # NO SILENT DEFAULT (CEO, 2026-09-25: "how do I know that you won't fuck this up next
    # time?"). A command that runs the gates names how many run at once, or it does not start.
    # It is not made to name a simulated-phone count no gate of the desktop build uses (hunt
    # part 2 v3, R48). A command that reuses a recorded run's gates runs none, so it names neither.
    runs_gates = (args.command in GATE_COMMANDS and not (args.command == "stable" and args.dry_run)
                  and not args.gates_passed_in)
    named = [flag for flag, value in ((GATES_AT_ONCE_FLAG, args.gates_at_once),
                                      (SIMULATED_PHONES_FLAG, args.simulated_phones))
             if value is not None]
    if runs_gates and args.gates_at_once is None:
        parser.error(
            f"{args.command} refuses to start without {GATES_AT_ONCE_FLAG}. "
            f"{GATES_AT_ONCE_FLAG} N|all is how many gates run at the same time (1 = one after "
            f"another, all = every gate whose inputs are ready). Choose it from what else is "
            "running on this Mac: all on a free Mac, fewer when engineers are busy.")
    if not runs_gates and named:
        parser.error(f"{GATES_AT_ONCE_FLAG} and {SIMULATED_PHONES_FLAG} decide how the gates run; "
                     f"they mean nothing to {args.command}"
                     + (" --dry-run, which runs no gate" if args.command == "stable" else "")
                     + (f" {PASSED_GATES_FLAG}, which runs no gate" if args.gates_passed_in else ""))
    dropped = settings_this_build_drops(os.environ, args.command)
    if dropped:
        parser.error(
            f"your shell sets {', '.join(dropped)}, and no step of `{args.command}` would receive "
            f"{'it' if len(dropped) == 1 else 'them'}: every gate runs in an allowlisted "
            "environment (GATE_PASSTHROUGH in this file), so the setting would be dropped "
            "without a word and the run would not be the one you asked for. Nothing has "
            f"started. Unset it to run without it (unset {' '.join(dropped)}). A setting a gate "
            "really needs becomes a command-line flag or a GATE_PASSTHROUGH entry with its "
            "reason. (The test VM needs none: `build` boots its candidate there by itself.)")
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        parser.error("local nightly releases currently require an Apple Silicon Mac")
    os.umask(0o077)
    state = args.state_dir.expanduser().absolute()
    env, credentials = local_environment(args.run if args.command == "build" else None)
    vm_settings = {name: value for name, value in os.environ.items()
                   if args.command == "build" and name.startswith(VM_STEP_PREFIXES)}
    gui_host = os.environ.get(GUI_HOST_ENV) if args.command == "build" else None
    with exclusive(state):
        logs = state / "logs"
        logs.mkdir(exist_ok=True)
        log_path = logs / (env["RICHOS_NIGHTLY_RUN_ID"] + ".log")
        print(f"Run log: {log_path}", flush=True)
        with TimestampedLog(log_path, BUILD_MILESTONES) as log:
            runner = Runner(args.repo.resolve(), state, env, log, credentials,
                            gates_at_once=args.gates_at_once, simulated_phones=args.simulated_phones,
                            chosen_by=chosen_by(), gui_host=gui_host, vm_settings=vm_settings)
            runner.perform(
                args.command, args.force, args.runtime_dir.resolve() if args.runtime_dir else None,
                args.run, args.checks_done_at_land, no_host_screen, args.gui_proof,
                args.from_nightly, args.dry_run, args.gates_passed_in)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, RuntimeError, OSError, subprocess.TimeoutExpired) as error:
        raise SystemExit(f"nightly: {error}") from error
