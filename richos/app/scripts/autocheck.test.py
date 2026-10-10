#!/usr/bin/env python3
"""autocheck.test.py — git itself runs the basic checks, at commit and at the land into main.

Every case is a throwaway repository with the real shim, installer and autocheck.py, and
stand-ins for the tools they call: a lint that refuses a file saying LINT-BAD, a selector
that assigns scripts/suite.sh to any change under richos/app, the owning suite (fails when
src/thing.txt says BROKEN) and a runner that runs the selected commands. Escalations go to a
scratch ledger through the real escalate.sh. Nothing touches this repository or its hooks.

  COMMIT   a failing lint refuses the commit; a clean change commits with no record;
           --no-verify is recorded; a linked worktree, `commit -a`, a cherry-pick, a branch
           older than the check and a forged exemption behave; before the check exists the
           hooks do nothing; bytes written while the check runs are never overwritten.
  LAND     a merge that breaks its owning suite is refused before main moves; a good one
           lands with a receipt; --no-verify and a fast-forward are recorded and the push
           then runs the checks; an uncovered path, a direct commit and a dirty tree are
           refused; the land that introduces the check is checked by it. The merge gate
           asks for proof-for.sh --gate, lints the change (--changed; the push: --all), caps
           each check at 600 s and the gate at 900 s, and a check that reached no verdict is
           named NOT RUN and never blocks; a failing one does. A branch cut before main's
           last land is asked about only what it brings, read from the tree being landed,
           and its own unclaimed file is still refused.
  INSTALL  install, --check and --uninstall, a foreign hook left alone, a chain that
           would never call the hook reported.
"""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
AUTOCHECK = Path(os.environ.get("AUTOCHECK_UNDER_TEST", HERE / "autocheck"))
ENGINE = HERE.parents[1] / "engine"
HANG_GUARD = 600

LINT = """#!/usr/bin/env bash
# Fixture lint: refuses when a file under richos/app/src says LINT-BAD.
cd "$(dirname "$0")/../../.."
printf 'lint %s\\n' "$*" >> "$AUTOCHECK_FIXTURE_LOG"
# A write made while the check runs: by the check itself, or by an editor saving.
if [ -n "${AUTOCHECK_FIXTURE_DURING_CHECK:-}" ]; then eval "$AUTOCHECK_FIXTURE_DURING_CHECK"; fi
if grep -rq LINT-BAD richos/app/src; then
    echo "Lint refused: lint growth: fixture-rule: 1 > 0" >&2
    exit 1
fi
echo "Lint passed"
"""
DRIVER = "# Fixture: this lint knows the commit mode, --changed, and --strict.\n"
PROOF_FOR = """#!/usr/bin/env bash
# Fixture selector: scripts/suite.sh proves every change under richos/app. It takes the
# merge gate's selection flag and records every call that asks for it.
cd "$(dirname "$0")/../../.."
pending=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --quiet) shift ;;
        --gate) printf 'proof-for --gate %s\\n' "$*" >> "$AUTOCHECK_FIXTURE_LOG.select"; shift ;;
        --commit-pending) pending=1; shift ;;
        *) break ;;
    esac
done
# The real selector asks the battery question (CEO ruling §81) of every commit in a range before
# it selects anything, and exits 3 on a refusal; with --commit-pending the range's head stands
# for a commit not made yet and is left to commit-msg. Each commit's message is read by the same
# battery-check.py --message the commit-msg hook uses (only the trailer cases ship it).
case "${1:-}" in *..*)
    if [ -f richos/app/scripts/battery-check.py ]; then
        range="$1"
        if [ -n "$pending" ]; then range="${1%%..*}..${1##*..}^"; fi
        for c in $(git rev-list --no-merges "$range" -- richos/mobile/); do
            git log -1 --format=%B "$c" > "$AUTOCHECK_FIXTURE_LOG.message"
            if ! python3 richos/app/scripts/battery-check.py --message "$AUTOCHECK_FIXTURE_LOG.message" >/dev/null 2>&1; then
                echo "battery: $c touches richos/mobile/ and has not answered the battery question" >&2
                exit 3
            fi
        done
    fi ;;
esac
if [ "$1" = --paths ]; then
    paths=$(printf '%s\\n' "$2" | tr ',' '\\n')
else
    paths=$(git diff --name-only "$1")
fi
printf '%s\\n' "$paths" >> "$AUTOCHECK_FIXTURE_LOG.paths"
if printf '%s\\n' "$paths" | grep -q uncovered; then
    echo "UNCOVERED: richos/app/src/uncovered.txt" >&2
    exit 1
fi
# A file under src/owned/ is covered only by its claim, owners/<name>, in the tree the selection
# reads: a range's head (the engine's selector reads its suites there), or the working tree for
# a path list.
case "$1" in *..*) tree="${1##*..}" ;; *) tree="" ;; esac
for p in $(printf '%s\\n' "$paths" | grep '^richos/app/src/owned/'); do
    name="${p##*/}"
    if [ -n "$tree" ]; then
        git cat-file -e "$tree:richos/app/owners/$name" 2>/dev/null && continue
    elif [ -e "richos/app/owners/$name" ]; then
        continue
    fi
    echo "UNCOVERED: $p" >&2
    exit 1
done
if printf '%s\\n' "$paths" | grep -q '^richos/app/'; then
    echo "  cd richos/app && bash scripts/suite.sh"
fi
if printf '%s\\n' "$paths" | grep -q 'screen'; then
    echo "  cd richos/app && bash scripts/screen.sh"
fi
if printf '%s\\n' "$paths" | grep -q 'claims'; then
    echo "  cd richos/app/ui/tests && node quick.js"
    echo "  cd richos/app/ui/tests && node heavy.js"
fi
if printf '%s\\n' "$paths" | grep -q 'state'; then
    echo "  cd richos/app && bash scripts/state.sh"
fi
if printf '%s\\n' "$paths" | grep -q 'later'; then
    echo "  cd richos/app && bash scripts/later.sh"
fi
if printf '%s\\n' "$paths" | grep -q 'mutant'; then
    # The unit the merge of 4e73fd89 ran into its cap, and an ordinary unit beside it.
    shard="bash scripts/ci-shard.sh --only-units"
    fence_pass=scripts/operator-fences-mutation.test.sh
    echo "  cd richos/engine && $shard $fence_pass"
    echo "  cd richos/engine && $shard scripts/spawn.test.sh"
fi
if printf '%s\\n' "$paths" | grep -q 'device'; then
    runner=scripts/run-tests.sh
    echo "  cd richos/app && $runner --only phone-unit.test.sh --only front-door.test.sh --only native-ios-share.test.sh --no-host-screen"
    echo "  cd richos/app && bash scripts/native-ios-ui.test.sh CaseA"
fi
"""
# Fixture run-tests.sh: runs each --only suite in order and records it.
RUN_TESTS = """#!/usr/bin/env bash
cd "$(dirname "$0")/.."
while [ "$#" -gt 0 ]; do
    case "$1" in
        --only) printf 'suite %s\\n' "$2" >> "$AUTOCHECK_FIXTURE_LOG"; bash "scripts/$2" || exit 1; shift 2 ;;
        *) shift ;;
    esac
done
"""
# Suites that need a device the merge never uses: they fail here, so running one refuses.
SIMULATOR_SUITE = """#!/usr/bin/env bash
# Fixture iPhone-simulator suite.
printf 'ran-simulator-suite %s\\n' "$(basename "$0")" >> "$AUTOCHECK_FIXTURE_LOG"
exit 1
"""
SCREEN_SUITE = """#!/usr/bin/env bash
# run-tests: host-screen
printf 'ran-screen-suite %s\\n' "$(basename "$0")" >> "$AUTOCHECK_FIXTURE_LOG"
exit 1
"""
# The phone apps' release policy, reduced to rule L1: no log call in the iPhone app's sources.
RELEASE_POLICY = """#!/usr/bin/env bash
# run-tests: inputs richos/app/scripts/native-release-policy.test.sh richos/mobile/native-ios/App richos/mobile/native-android/app/src
printf 'ran-release-policy\\n' >> "$AUTOCHECK_FIXTURE_LOG"
if grep -rq 'Logger(' "$(dirname "$0")/../../mobile/native-ios/App" 2>/dev/null; then
    echo "  FAIL  L1 no log call in production code" >&2
    exit 1
fi
"""
PHONE_UNIT = """#!/usr/bin/env bash
# Fixture headless suite beside them.
exit 0
"""
# A document-vs-tree check that measures under a second (weight 0): fails when the claim in
# src/claims.txt says FALSE. And a heavy suite selected beside it, which a commit never runs.
QUICK = """const fs = require("fs");
fs.appendFileSync(process.env.AUTOCHECK_FIXTURE_LOG, "node quick.js\\n");
if (fs.readFileSync("../../src/claims.txt", "utf8").includes("FALSE")) {
  console.error("quick: FAIL the README's count is stale");
  process.exit(1);
}
console.log("quick: PASS");
"""
HEAVY = """require("fs").appendFileSync(process.env.AUTOCHECK_FIXTURE_LOG, "node heavy.js\\n");
"""
WEIGHTS = "# fixture weights\nquick.js\t0\nheavy.js\t40\n"
PROOF_RUN = """import json
import os
import subprocess
import sys

# Fixture runner, proof-run.py's contract: exit 0 all passed, 1 a check did not pass, 3 nothing
# failed and a check was NOT RUN; --summary-out gets summary.json's rows; outcomes.json is keyed
# by check, with the reason an invalid result is invalid. A command that prints "NOT-RUN <why>"
# and exits 0 is a suite run-tests.sh did not run, for that reason; one that prints
# "STATE <state> [<reason>]" ended in that runner state (timed-out at its cap, ended at the run's
# cap, not admitted, invalid for that reason). Its own arguments go to <log>.runner. It takes
# --cap and --run-cap and leaves them to the real runner (proof-run.test.py P40). It takes
# --only-check LABEL (a resume that runs only the named checks), as the retry of a check with
# no verdict passes it.
from pathlib import Path
with open(os.environ["AUTOCHECK_FIXTURE_LOG"] + ".runner", "a") as log:
    log.write(" ".join(sys.argv[1:]) + "\\n")
directory = (Path(sys.argv[sys.argv.index("--log-dir") + 1]) if "--log-dir" in sys.argv
             else Path(os.environ["RICHOS_AUTOCHECK_PROOF_ROOT"]) / "legacy")
directory.mkdir(parents=True, exist_ok=True)
previous = []
if "--resume" in sys.argv:
    prior = Path(sys.argv[sys.argv.index("--resume") + 1])
    lines = json.loads((prior / "plan.json").read_text())
    previous = list(json.loads((prior / "outcomes.json").read_text()).values())
    with open(os.environ["AUTOCHECK_FIXTURE_LOG"], "a") as log:
        log.write("resume " + str(prior) + "\\n")
else:
    lines = Path(sys.argv[sys.argv.index("--commands") + 1]).read_text().splitlines()
    if "--reuse" in sys.argv:
        prior = Path(sys.argv[sys.argv.index("--reuse") + 1])
        previous = list(json.loads((prior / "outcomes.json").read_text()).values())
        with open(os.environ["AUTOCHECK_FIXTURE_LOG"], "a") as log:
            log.write("reuse " + str(prior) + "\\n")
(directory / "plan.json").write_text(json.dumps(lines))
only = [sys.argv[i + 1] for i, a in enumerate(sys.argv) if a == "--only-check"]
rows = []
for line in lines:
    line = line.strip()
    if line:
        old = next((row for row in previous if row["check"] == line and row["result"] == "passed"), None)
        if old:
            rows.append(old)
            continue
        if only and line not in only:
            # --only-check: what is not named is left out of this run, as the real runner does.
            rows.append({"check": line, "result": "not-run",
                         "not_run": {"why": "not selected for this retry",
                                     "suites": [{"name": line, "state": "retry-unselected",
                                                 "reason": "not selected for this retry"}]}})
            continue
        with open(os.environ["AUTOCHECK_FIXTURE_LOG"], "a") as log:
            log.write("run " + line + "\\n")
        if line.startswith("cd richos/engine"):
            # richos/engine is the real engine (a symlink): its units are recorded, never run.
            rows.append({"check": line, "result": "passed", "not_run": None})
            continue
        done = subprocess.run(["bash", "-c", line], stdout=subprocess.PIPE, text=True)
        sys.stdout.write(done.stdout)
        said = [l.split()[1] for l in done.stdout.splitlines() if l.startswith("NOT-RUN ")]
        ended = [l.split(None, 2)[1:] for l in done.stdout.splitlines() if l.startswith("STATE ")]
        if done.returncode:
            print("FAILED " + line)
            rows.append({"check": line, "result": "failed", "not_run": None})
        elif said:
            print("NOT RUN (%s) " % said[0] + line)
            rows.append({"check": line, "result": "not-run",
                         "not_run": {"why": said[0], "suites": [{"name": line, "state": "notrun", "reason": said[0]}]}})
        elif ended:
            print("%s %s" % (ended[0][0].upper(), line))
            rows.append({"check": line, "result": ended[0][0], "not_run": None,
                         **({"invalid": ended[0][1]} if len(ended[0]) > 1 else {})})
        else:
            rows.append({"check": line, "result": "passed", "not_run": None})
        # Each check that ran has its own log in this run's directory, as the real runner's does.
        rows[-1]["log"] = str(directory / ("%02d.log" % len(rows)))
        Path(rows[-1]["log"]).write_text(done.stdout)
(directory / "outcomes.json").write_text(json.dumps({row["check"]: row for row in rows}))
if "--summary-out" in sys.argv:
    with open(sys.argv[sys.argv.index("--summary-out") + 1], "w") as out:
        json.dump({"checks": rows}, out)
results = {row["result"] for row in rows}
sys.exit(1 if results - {"passed", "not-run"} else 3 if "not-run" in results else 0)
"""
SUITE = """#!/usr/bin/env bash
# Fixture owning suite. It records the mutation switches it was run with.
cd "$(dirname "$0")/.."
printf 'suite-env RICHOS_MUTATION_PASSES=%s RICHOS_FOURTEEN_MUTANTS=%s\\n' \\
    "${RICHOS_MUTATION_PASSES:-unset}" "${RICHOS_FOURTEEN_MUTANTS:-unset}" >> "$AUTOCHECK_FIXTURE_LOG"
if grep -q BROKEN src/thing.txt; then
    echo "suite: FAIL src/thing.txt is broken" >&2
    exit 1
fi
echo "suite: PASS"
"""
SCREEN = """#!/usr/bin/env bash
# Fixture screen suite: does not run, for the reason src/screen.txt names.
cd "$(dirname "$0")/.."
echo "NOT-RUN $(cat src/screen.txt)"
"""
STATE = """#!/usr/bin/env bash
# Fixture check that ends in the runner state src/<its name>.txt names (and its reason, if any).
cd "$(dirname "$0")/.."
name=$(basename "$0" .sh)
state=$(cat "src/$name.txt")
runs=$(( $(cat "$AUTOCHECK_FIXTURE_LOG.$name.runs" 2>/dev/null || echo 0) + 1 ))
echo "$runs" > "$AUTOCHECK_FIXTURE_LOG.$name.runs"
# "once-<state>" ends in <state> on its first run only, "twice-<state>" on its first two; then it
# passes. "held-<state>" ends in <state> while <log>.hold exists (a Mac that stays too busy).
case "$state" in
    once-*) if [ "$runs" -gt 1 ]; then exit 0; fi; state="${state#once-}" ;;
    twice-*) if [ "$runs" -gt 2 ]; then exit 0; fi; state="${state#twice-}" ;;
    held-*) if [ ! -e "$AUTOCHECK_FIXTURE_LOG.hold" ]; then exit 0; fi; state="${state#held-}" ;;
esac
echo "STATE $state"
"""
ENDED = "cancelled"  # dialect-exempt: proof-run.py's state value for a check its run ended


class Fixture(unittest.TestCase):
    def setUp(self):
        # The land check refuses proof storage off the mounted external SSD, so the fixture's
        # proof root lives there whatever TMPDIR the caller set (a suite must not depend on it).
        ssd = Path("/Volumes/E1TB/tmp/autocheck-tests")
        ssd.mkdir(parents=True, exist_ok=True)
        self.tmp = tempfile.TemporaryDirectory(prefix="autocheck-", dir=ssd)
        self.base = Path(self.tmp.name)
        self.log = self.base / "tools.log"
        self.ledger = self.base / "ledger.jsonl"
        (self.base / "gitconfig").write_text("[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n"
                                              "[init]\n\tdefaultBranch = main\n")
        self.env = {k: v for k, v in os.environ.items() if not k.startswith(("GIT_", "RICHOS_AUTOCHECK"))}
        self.env.update(GIT_CONFIG_GLOBAL=str(self.base / "gitconfig"), GIT_CONFIG_NOSYSTEM="1",
                        AUTOCHECK_FIXTURE_LOG=str(self.log), RICHOS_ESCALATION_LEDGER=str(self.ledger),
                        RICHOS_AUTOCHECK_PROOF_ROOT=str(self.base / "proof-runs"))
        self.repo = self.base / "repo"

    def tearDown(self):
        self.tmp.cleanup()

    # -- building ------------------------------------------------------------------------
    def make(self, with_checker=True, install=True):
        app = self.repo / "richos/app"
        for rel, text, mode in (("scripts/lint.sh", LINT, 0o755), ("scripts/lint/driver.py", DRIVER, 0o644),
                                ("scripts/proof-for.sh", PROOF_FOR, 0o755), ("scripts/proof-run.py", PROOF_RUN, 0o644),
                                ("scripts/suite.sh", SUITE, 0o755), ("scripts/screen.sh", SCREEN, 0o755),
                                ("scripts/state.sh", STATE, 0o755), ("scripts/later.sh", STATE, 0o755),
                                ("scripts/run-tests.sh", RUN_TESTS, 0o755),
                                ("scripts/native-ios-share.test.sh", SIMULATOR_SUITE, 0o755),
                                ("scripts/native-ios-ui.test.sh", SIMULATOR_SUITE, 0o755),
                                ("scripts/front-door.test.sh", SCREEN_SUITE, 0o755),
                                ("scripts/phone-unit.test.sh", PHONE_UNIT, 0o755),
                                ("scripts/native-release-policy.test.sh", RELEASE_POLICY, 0o755),
                                ("ui/tests/quick.js", QUICK, 0o644), ("ui/tests/heavy.js", HEAVY, 0o644),
                                ("ui/tests/suite-weights.tsv", WEIGHTS, 0o644),
                                ("src/thing.txt", "fine\n", 0o644), ("src/claims.txt", "true\n", 0o644)):
            path = app / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
            path.chmod(mode)
        (self.repo / "richos/engine").symlink_to(ENGINE)
        self.git("init", "-q")
        (self.repo / ".git/info/exclude").write_text("richos/engine\n")
        if with_checker:
            self.add_checker()
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "base")
        if install:
            self.install()

    def add_checker(self, repo=None):
        dest = (repo or self.repo) / "richos/app/scripts/autocheck"
        dest.mkdir(parents=True, exist_ok=True)
        shared = dest.parent / "lib"
        shared.mkdir(exist_ok=True)
        shutil.copy(HERE / "lib/owned_command.py", shared / "owned_command.py")
        lint = dest.parent / "lint"
        lint.mkdir(exist_ok=True)
        for name in ("load_rules.py", "common.py"):
            shutil.copy(HERE / "lint" / name, lint / name)
        for name in ("autocheck.py", "shim.sh", "install.sh"):
            shutil.copy(AUTOCHECK / name, dest / name)

    def install(self):
        out = self.run_(["bash", str(AUTOCHECK / "install.sh"), str(self.repo)])
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)

    def run_(self, argv, cwd=None, env=None):
        return subprocess.run(argv, cwd=cwd or self.repo, env=env or self.env, capture_output=True, text=True,
                              timeout=HANG_GUARD)

    def git(self, *args, cwd=None, expect=0, env=None):
        result = self.run_(["git", *args], cwd=cwd, env=env)
        if expect is not None:
            self.assertEqual(result.returncode, expect, f"git {' '.join(args)}\n{result.stdout}{result.stderr}")
        return result

    def write(self, rel, text, cwd=None):
        path = (cwd or self.repo) / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def head(self, ref="HEAD", cwd=None):
        return self.git("rev-parse", ref, cwd=cwd).stdout.strip()

    def tools(self):
        return self.log.read_text() if self.log.exists() else ""

    def side_log(self, suffix):
        path = Path(str(self.log) + suffix)
        return path.read_text() if path.exists() else ""

    def recorded(self):
        return self.ledger.read_text() if self.ledger.exists() else ""

    def branch_with(self, name, rel, text, message="change", no_verify=False):
        self.git("checkout", "-q", "-b", name)
        self.write(rel, text)
        self.git("add", "-A")
        self.git("commit", "-q", "-m", message, *(["--no-verify"] if no_verify else []))
        self.git("checkout", "-q", "main")
        self.log.unlink(missing_ok=True)

    def land_a_merge_commit_on_main(self):
        """main's tip becomes a land merge (two parents), checked and passed like any land."""
        self.branch_with("other", "richos/app/src/thing.txt", "fine, better\n")
        self.git("merge", "--no-ff", "-m", "land other", "other")
        self.assertEqual(len(self.git("rev-list", "--parents", "-n", "1", "main").stdout.split()), 3)


ENGINE_SHARD = "ci-shard.sh"   # kept apart: the publication guard reads a long verbatim run as private speech


class Commit(Fixture):
    def test_a_commit_whose_lint_fails_is_refused_with_the_reason(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        before = self.head()
        self.write("richos/app/src/bad.txt", "LINT-BAD\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "bad", expect=1)
        text = out.stdout + out.stderr
        self.assertIn("COMMIT REFUSED", text)
        self.assertIn("Lint refused: lint growth: fixture-rule: 1 > 0", text)
        self.assertEqual(self.head(), before)
        self.assertIn("lint --changed --strict", self.tools())

    def test_a_clean_commit_passes_and_leaves_no_record(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/thing.txt", "fine, again\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "good")
        self.assertIn("autocheck: commit: passed", out.stderr)
        self.assertEqual(self.recorded(), "")

    def test_a_change_outside_the_app_runs_no_lint(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("README.md", "docs\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "docs")
        self.assertIn("no lint applies", out.stderr)
        self.assertEqual(self.tools(), "")

    def test_no_verify_commits_and_is_recorded_for_the_lead(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/bad.txt", "LINT-BAD\n")
        self.git("add", "-A")
        out = self.git("commit", "-q", "-m", "bad", "--no-verify")
        self.assertIn("AUTOCHECK SKIPPED, RECORDED", out.stderr)
        rows = [json.loads(line) for line in self.recorded().splitlines() if line.strip()]
        self.assertTrue(any("skipped the automatic checks (--no-verify)" in json.dumps(r) for r in rows), rows)
        self.assertIn("--no-verify", (self.repo / ".git/richos-autocheck/bypass.log").read_text())

    def test_commit_all_and_a_cherry_pick_leave_no_false_record(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/thing.txt", "fine, via -a\n")
        self.git("commit", "-q", "-a", "-m", "all")
        self.git("checkout", "-q", "-b", "other", "main")
        self.git("cherry-pick", "feature")
        self.assertEqual(self.recorded(), "")

    def test_a_linked_worktree_is_checked_by_the_same_hooks(self):
        self.make()
        wt = self.base / "wt"
        self.git("worktree", "add", "-q", "-b", "wtbranch", str(wt))
        self.write("richos/app/src/bad.txt", "LINT-BAD\n", cwd=wt)
        self.git("add", "-A", cwd=wt)
        out = self.git("commit", "-m", "bad", cwd=wt, expect=1)
        self.assertIn("COMMIT REFUSED", out.stderr)

    def test_a_copied_exemption_exempts_nothing(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/bad.txt", "LINT-BAD\n")
        self.git("add", "-A")
        forged = dict(self.env, RICHOS_AUTOCHECK_ACTIVE=f"{(self.repo / '.git').resolve()}:1")
        out = self.git("commit", "-m", "bad", env=forged, expect=1)
        self.assertIn("COMMIT REFUSED", out.stderr)

    def test_a_branch_older_than_the_check_is_checked_by_mains_copy(self):
        self.make(with_checker=False)
        self.git("checkout", "-q", "-b", "old")
        self.git("checkout", "-q", "main")
        self.add_checker()
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "introduce the check")
        self.git("checkout", "-q", "old")
        self.write("richos/app/src/bad.txt", "LINT-BAD\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "bad", expect=1)
        self.assertIn("COMMIT REFUSED", out.stderr)
        self.write("richos/app/src/bad.txt", "fine\n")
        self.write("richos/app/src/claims.txt", "true, an old branch with a quick check\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "old branch uses the committed admission helper")
        self.assertIn("ran quick.js", out.stderr)

    def test_a_commit_the_land_would_refuse_as_uncovered_is_refused_at_the_commit(self):
        # 2026-09-29: isaac-opus-speckle1 and andy-opus-speckle1 each had four UNCOVERED paths
        # that nobody could see until Rich's merge; their commits had passed this hook.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        before = self.head()
        self.write("richos/app/src/uncovered.txt", "new code\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "uncovered", expect=1)
        self.assertIn("COMMIT REFUSED: a changed code path is covered by no suite", out.stderr)
        self.assertIn("UNCOVERED: richos/app/src/uncovered.txt", out.stderr)
        self.assertEqual(self.head(), before)

    def test_a_commit_outside_the_app_that_no_suite_claims_is_refused_at_the_commit(self):
        # 2026-10-01: a Swift file under richos/mobile/native-ios and walk step lists under
        # docs/verification passed the commit check (it returned early when nothing under
        # richos/app changed) and were refused at the merge as UNCOVERED.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        before = self.head()
        for path in ("richos/mobile/native-ios/App/Platform/uncovered.swift",
                     "docs/verification/2026-10-01-walk/steps/uncovered.json"):
            self.write(path, "new\n")
            self.git("add", "-A")
            out = self.git("commit", "-m", "uncovered", expect=1)
            self.assertIn("COMMIT REFUSED: a changed code path is covered by no suite", out.stderr)
            self.assertIn("UNCOVERED:", out.stderr)
            self.assertEqual(self.head(), before)
            self.git("reset", "-q")
            (self.repo / path).unlink()

    def test_a_log_call_in_a_phone_app_file_is_refused_at_the_commit(self):
        # 2026-10-01: 84f1ec3af added a Logger to the iPhone app (rule L1 of
        # native-release-policy.test.sh); the commit check never ran that script suite, and
        # only the merge saw it, after two agents had built on it.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        before = self.head()
        app_file = "richos/mobile/native-ios/App/Platform/BackgroundSendTime.swift"
        self.write(app_file, "let log = Logger(subsystem: \"x\")\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "logs", expect=1)
        self.assertIn("COMMIT REFUSED: this change breaks the phone apps' release policy", out.stderr)
        self.assertIn("L1 no log call in production code", out.stderr)
        self.assertEqual(self.head(), before)
        # Without the log call the same file commits, the suite having run; a change that owns
        # none of its inputs does not run it.
        self.write(app_file, "let value = 1\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "fine", expect=0)
        self.assertIn("ran-release-policy", self.tools())
        self.log.unlink()
        self.write("README.md", "docs\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "docs")
        self.assertNotIn("ran-release-policy", self.tools())

    def test_an_unstaged_policy_header_does_not_decide_a_non_app_commit(self):
        # Recheck R10 v2 (2026-10-01): for a commit with no staged richos/app file, whether the
        # phone policy applied was read from the working-tree header, so an unstaged edit that
        # drops the phone folder from the inputs let a staged Logger through.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        before = self.head()
        self.write("richos/mobile/native-ios/App/Platform/BackgroundSendTime.swift",
                   "let log = Logger(subsystem: \"x\")\n")
        self.git("add", "-A")
        policy = self.repo / "richos/app/scripts/native-release-policy.test.sh"
        policy.write_text(policy.read_text().replace("richos/mobile/native-ios/App ", "nothing "))
        out = self.git("commit", "-m", "logs", expect=1)
        self.assertIn("COMMIT REFUSED: this change breaks the phone apps' release policy", out.stderr)
        self.assertEqual(self.head(), before)
        self.assertIn("nothing ", policy.read_text())  # the unstaged edit is still there
        self.assert_aside_restored()

    def test_the_whole_branch_is_asked_not_only_the_staged_files(self):
        # An uncovered path committed earlier with --no-verify is still in what the land will
        # diff, so the next ordinary commit on the branch refuses it too.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/uncovered.txt", "new code\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "uncovered", "--no-verify")
        self.write("richos/app/src/thing.txt", "fine, again\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "an innocent change", expect=1)
        self.assertIn("UNCOVERED: richos/app/src/uncovered.txt", out.stderr)

    def test_the_commit_asks_for_the_selection_the_way_the_merge_gate_does(self):
        # 2026-10-01: a hook script passed every commit here, where the branch was asked about
        # as a path list, and was refused at the merge, which asks as a base..head range: the
        # engine's selector takes different roads for the two. The commit check now asks as a
        # range too, built from the index, so the commit sees what the merge will see.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/thing.txt", "fine, again\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "good")
        calls = self.side_log(".select").splitlines()
        self.assertTrue(calls, "the commit check never asked proof-for.sh --gate")
        self.assertTrue(all(".." in c and "--paths" not in c for c in calls), calls)

    PIN_SOURCE = "richos/app/scripts/pinned.sh"
    QUALIFICATIONS = "docs/development/verification-input-qualifications.json"

    def pin(self, text):
        import hashlib
        self.write(self.QUALIFICATIONS, json.dumps({"schema": 1, "units": {"pinned-unit": {"sources": {
            self.PIN_SOURCE: hashlib.sha256(text.encode()).hexdigest()}}}}))

    def pin_fixture(self):
        """A reviewed unit pins scripts/pinned.sh by SHA-256, as the real qualification file does."""
        self.write(self.PIN_SOURCE, "echo v1\n")
        self.pin("echo v1\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "pin it", "--no-verify")
        self.git("checkout", "-q", "-b", "feature")

    def test_a_changed_file_a_reviewed_check_pins_is_refused_until_its_pin_is_renewed(self):
        # 2026-09-29: three branches passed every commit and were refused at the merge with
        # UnqualifiedReader, because only the merge compared a source to its pin.
        self.make()
        self.pin_fixture()
        before = self.head()
        self.write(self.PIN_SOURCE, "echo v2\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "edit a pinned file", expect=1)
        self.assertIn("COMMIT REFUSED: a changed file is pinned by a reviewed check", out.stderr)
        self.assertIn(self.PIN_SOURCE, out.stderr)
        self.assertIn('unit "pinned-unit"', out.stderr)
        self.assertIn("qualification-pins.py --renew", out.stderr)
        self.assertEqual(self.head(), before)

    def test_commit_a_reads_the_index_the_hook_was_given_for_a_pinned_file(self):
        # `git commit -a` hands the hook a temporary index in GIT_INDEX_FILE; the pin check used
        # to drop it and read the real index, where the pinned file still matched its pin.
        self.make()
        self.pin_fixture()
        before = self.head()
        self.write(self.PIN_SOURCE, "echo v2\n")
        out = self.git("commit", "-a", "-m", "edit a pinned file", expect=1)
        self.assertIn("COMMIT REFUSED: a changed file is pinned by a reviewed check", out.stderr)
        self.assertEqual(self.head(), before)
        self.write(self.PIN_SOURCE, "echo v2\n")
        self.pin("echo v2\n")
        out = self.git("commit", "-a", "-m", "edit a pinned file and renew its pin")
        self.assertIn("autocheck: commit: passed", out.stderr)

    def test_a_pinned_file_committed_with_its_renewed_pin_passes(self):
        self.make()
        self.pin_fixture()
        self.write(self.PIN_SOURCE, "echo v2\n")
        self.pin("echo v2\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "edit a pinned file and renew its pin")
        self.assertIn("autocheck: commit: passed", out.stderr)

    def test_a_quick_document_check_the_change_falsifies_refuses_the_commit(self):
        # 2026-09-29: a Rust test file made app/README.md's counts false; docs-claims.js (under
        # a second) ran nowhere before the nightly. The commit runs the quick suites selected.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/claims.txt", "FALSE\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "stale claim", expect=1)
        self.assertIn("COMMIT REFUSED: quick.js failed for this branch", out.stderr)
        self.assertIn("quick: FAIL the README's count is stale", out.stderr)
        self.assertNotIn("node heavy.js", self.tools())

    def test_a_true_claim_commits_and_heavy_suites_wait_for_the_land(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/claims.txt", "still true\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "claim kept true")
        self.assertIn("ran quick.js", out.stderr)
        self.assertIn("node quick.js", self.tools())
        self.assertNotIn("node heavy.js", self.tools())
        self.assertEqual(self.recorded(), "")

    def record_duration(self, label, seconds):
        """The merge gate's recorded duration of a check (proof-run.py record_weights), where the
        commit check reads it: <proof-runs>/<sha256(main checkout)[:12]>/weights.tsv."""
        import hashlib
        base = self.base / "proof-history"
        wid = hashlib.sha256(str(self.repo.resolve()).encode()).hexdigest()[:12]
        (base / wid).mkdir(parents=True, exist_ok=True)
        (base / wid / "weights.tsv").write_text(f"{label}\t{seconds}\t{seconds}\n")
        self.env["RICHOS_PROOF_RUN_DIR"] = str(base)

    def test_a_selected_check_that_is_fast_by_its_recorded_duration_refuses_the_commit(self):
        # 2026-10-09: a merge was refused at the end of a 25-minute gate by two checks of 3 and 4
        # seconds (mutation-anchors.test.sh, spawn-guard-audience.test.sh); every commit had passed.
        self.make()
        self.record_duration("suite", 2.5)
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/thing.txt", "BROKEN\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "breaks the owning suite", expect=1)
        self.assertIn("COMMIT REFUSED: suite failed for this branch", out.stderr)
        self.assertIn("suite: FAIL src/thing.txt is broken", out.stderr)

    def test_a_fast_check_that_passes_does_not_refuse_the_commit_and_is_named(self):
        self.make()
        self.record_duration("suite", 2.5)
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/thing.txt", "fine, better\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "keeps the owning suite green")
        self.assertIn("ran suite", out.stderr)
        self.assertIn("suite-env", self.tools())

    def test_a_check_recorded_as_slow_or_never_recorded_waits_for_the_land(self):
        for label, seconds in (("suite", 40.0), ("other", 1.0)):   # slow; and fast but not selected
            with self.subTest(label=label):
                self.setUp()
                self.make()
                self.record_duration(label, seconds)
                self.git("checkout", "-q", "-b", "feature")
                self.write("richos/app/src/thing.txt", "BROKEN\n")
                self.git("add", "-A")
                out = self.git("commit", "-m", "broken, but the check is not a commit check")
                self.assertIn("1 heavier check command(s) run at the land", out.stderr)
                self.tearDown()

    def finish_fast_setup(self):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "fixture setup")
        self.install()
        self.git("checkout", "-q", "-b", "feature")

    def test_an_engine_only_commit_runs_the_fast_unit_and_the_anchor_check(self):
        # Review rv-20261009T093443Z: the no-app path ran branch_selection(run_quick=False), so a
        # commit touching only richos/engine ran none of the fast checks (today's incidents).
        self.make(install=False)
        (self.repo / "richos/engine").unlink()      # our own tiny engine tree instead of the symlink
        (self.repo / ".git/info/exclude").write_text("")
        (self.repo / "richos/engine/scripts").mkdir(parents=True)
        (self.repo / "richos/engine/scripts/lib").symlink_to(ENGINE / "scripts/lib")
        self.write("richos/engine/probe.txt", "fine\n")
        self.write("richos/engine/scripts/ci-shard.sh",
                   '#!/bin/bash\necho "ran-engine-fast" >> "$AUTOCHECK_FIXTURE_LOG"\n'
                   'grep -q BROKEN probe.txt && { echo "engine fast: FAIL"; exit 1; }\nexit 0\n')
        self.write("richos/engine/scripts/mutation-anchors.py",
                   'from pathlib import Path\nimport sys\n'
                   'bad = "BROKEN" in Path("probe.txt").read_text()\n'
                   'print("anchor: FAIL" if bad else "anchor: PASS")\nsys.exit(1 if bad else 0)\n')
        selector = self.repo / "richos/app/scripts/proof-for.sh"
        selector.write_text(selector.read_text() +
                            '\nif printf "%s\\n" "$paths" | grep -q "^richos/engine/"; then\n'
                            ' echo "cd richos/engine && bash scripts/' + ENGINE_SHARD +
                            ' --only-units scripts/fast.test.sh"; fi\n')
        self.finish_fast_setup()
        self.record_duration("engine scripts/fast.test.sh", 2.5)
        weights = next((self.base / "proof-history").glob("*/weights.tsv"))
        weights.write_text(weights.read_text() + "mutation-anchors.py\t2.3\t2.3\n")
        self.write("richos/engine/probe.txt", "BROKEN\n")
        self.git("add", "richos/engine/probe.txt")
        out = self.git("commit", "-m", "engine-only broken change", expect=1)
        self.assertIn("COMMIT REFUSED: mutation-anchors.py failed", out.stderr)   # fastest recorded, runs first
        (self.repo / "richos/engine/scripts/mutation-anchors.py").unlink()
        self.git("add", "-A")
        out = self.git("commit", "-m", "engine-only broken unit", expect=1)
        self.assertIn("COMMIT REFUSED: engine scripts/fast.test.sh failed", out.stderr)
        self.assertIn("ran-engine-fast", self.tools())

    def test_engine_timing_growth_refuses_commit_and_merge_without_application_lint(self):
        self.make(install=False)
        (self.repo / "richos/engine").unlink()
        (self.repo / ".git/info/exclude").write_text("")
        self.write("richos/engine/scripts/growth.test.py", "assert ready\n")
        self.finish_fast_setup()
        # load-bound: this assertion is fixture SOURCE, never executed by this suite.
        self.write("richos/engine/scripts/growth.test.py", "self.assertLess(\n elapsed, 2)\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "adds a host timing verdict", expect=1)
        self.assertIn("engine test load-rule growth", out.stderr)
        self.assertIn("growth.test.py", out.stderr)
        self.git("commit", "--no-verify", "-qm", "bypass the author check")
        self.git("checkout", "-q", "main")
        before = self.head()
        out = self.git("merge", "--no-ff", "--no-edit", "feature", expect=1)
        self.assertIn("load-rule growth", out.stdout + out.stderr)
        self.assertEqual(self.head(), before)
        self.assertIn("load_rules.py", self.tools())

    def test_a_node_suite_selected_and_recorded_as_fast_runs_at_the_commit(self):
        self.make(install=False)
        self.write("richos/app/ui/tests/heavy.js",
                   'require("fs").appendFileSync(process.env.AUTOCHECK_FIXTURE_LOG, "node failing-heavy.js\\n");\n'
                   'console.error("heavy: FAIL"); process.exit(1);\n')
        self.finish_fast_setup()
        self.record_duration("heavy.js", 2.5)
        self.write("richos/app/src/claims.txt", "true, still\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "selects a failing 2.5-second node suite", expect=1)
        self.assertIn("COMMIT REFUSED: heavy.js failed for this branch", out.stderr)
        self.assertIn("node failing-heavy.js", self.tools())

    def test_fast_forwarding_a_branch_onto_a_land_merge_records_no_skip(self):
        # 2026-09-29: echo-opus-speckle1 fast-forwarded its branch onto main's 5ddcce1c, a
        # land merge, and the ledger said "skipped the automatic checks (--no-verify)". No
        # --no-verify was passed; a fast-forward writes no commit and has nothing to skip.
        self.make()
        self.git("branch", "feature")
        self.land_a_merge_commit_on_main()
        self.git("checkout", "-q", "feature")
        out = self.git("merge", "--ff-only", "main")
        self.assertNotIn("AUTOCHECK SKIPPED", out.stderr)
        self.assertEqual(self.recorded(), "")

    def test_a_real_merge_into_a_branch_with_no_verify_is_still_recorded(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/feature.txt", "feature work\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "feature work")
        self.git("checkout", "-q", "main")
        self.land_a_merge_commit_on_main()
        self.git("checkout", "-q", "feature")
        self.git("merge", "--no-ff", "--no-verify", "-m", "bring main in", "main")
        self.assertIn("skipped the automatic checks (--no-verify)", self.recorded())

    # Hunt part 2, finding 10: the commit check linted the working copy and approved the staged
    # copy. A bad staged edit whose unstaged replacement passed committed with no record.
    def assert_aside_restored(self):
        self.assertFalse((self.repo / ".git/richos-autocheck-aside").exists(),
                         "the set-aside edits were left behind")

    def test_a_staged_copy_that_fails_the_lint_is_refused_although_the_working_copy_passes(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        before = self.head()
        self.write("richos/app/src/bad.txt", "LINT-BAD\n")
        self.git("add", "-A")
        self.write("richos/app/src/bad.txt", "fine in the working tree\n")
        out = self.git("commit", "-m", "bad staged, clean unstaged", expect=None)
        self.assertEqual(out.returncode, 1, "a bad staged copy was committed:\n" + out.stderr)
        self.assertIn("Lint refused: lint growth: fixture-rule: 1 > 0", out.stderr)
        self.assertEqual(self.head(), before)
        # The unstaged edit is back byte for byte, and still unstaged.
        self.assertEqual((self.repo / "richos/app/src/bad.txt").read_text(), "fine in the working tree\n")
        self.assertEqual(self.git("show", ":richos/app/src/bad.txt").stdout, "LINT-BAD\n")
        self.assert_aside_restored()

    def test_a_clean_staged_copy_commits_and_unstaged_and_untracked_work_survives(self):
        # The same defect the other way: the verdict was about bytes the commit does not hold.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/thing.txt", "fine, staged\n")
        self.git("add", "-A")
        self.write("richos/app/src/thing.txt", "LINT-BAD, still being written\n")
        self.write("richos/app/src/draft/notes.txt", "LINT-BAD draft, never staged\n")
        out = self.git("commit", "-m", "the staged copy is clean", expect=None)
        self.assertEqual(out.returncode, 0, "unstaged work refused a clean commit:\n" + out.stderr)
        self.assertIn("autocheck: commit: passed", out.stderr)
        self.assertEqual(self.git("show", "HEAD:richos/app/src/thing.txt").stdout, "fine, staged\n")
        self.assertEqual((self.repo / "richos/app/src/thing.txt").read_text(), "LINT-BAD, still being written\n")
        self.assertEqual((self.repo / "richos/app/src/draft/notes.txt").read_text(), "LINT-BAD draft, never staged\n")
        self.assertEqual(self.git("status", "--porcelain").stdout,
                         " M richos/app/src/thing.txt\n?? richos/app/src/draft/\n")
        self.assertEqual(self.recorded(), "")
        self.assert_aside_restored()

    def test_commit_with_a_path_checks_only_what_that_commit_holds(self):
        # `git commit <path>` commits through a temporary index; the check must use it too.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/thing.txt", "fine, by path\n")
        self.write("richos/app/src/claims.txt", "true\nLINT-BAD not in this commit\n")
        out = self.git("commit", "-m", "one path", "richos/app/src/thing.txt", expect=None)
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertEqual(self.git("show", "HEAD:richos/app/src/claims.txt").stdout, "true\n")
        self.assertEqual((self.repo / "richos/app/src/claims.txt").read_text(), "true\nLINT-BAD not in this commit\n")
        self.assertEqual(self.git("status", "--porcelain").stdout, " M richos/app/src/claims.txt\n")
        self.assert_aside_restored()

    # Recheck N01 (2026-09-30): when the patch did not apply because a check wrote into a file
    # it edits, the restore reset the WHOLE tree to the index, erasing a newer editor save in a
    # file the patch never names, and dropped the bytes it overwrote, with no error.
    def test_bytes_written_while_the_check_runs_are_never_overwritten(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/thing.txt", "fine, staged\n")
        self.git("add", "-A")
        self.write("richos/app/src/thing.txt", "fine, unstaged edit\n")
        before = self.head()
        env = dict(self.env, AUTOCHECK_FIXTURE_DURING_CHECK=(
            "printf 'fine, written during the check\\n' > richos/app/src/thing.txt; "
            "printf 'true\\neditor save during the check\\n' > richos/app/src/claims.txt"))
        out = self.git("commit", "-m", "a save lands while the check runs", env=env, expect=None)
        # The editor's save in a file the patch does not edit is left exactly as found.
        self.assertEqual((self.repo / "richos/app/src/claims.txt").read_text(), "true\neditor save during the check\n",
                         "a save made while the check ran was reset:\n" + out.stderr)
        # The engineer's edit from before the check is back, and the bytes that collided with it
        # are kept, not overwritten; the commit is refused so both are seen.
        self.assertEqual((self.repo / "richos/app/src/thing.txt").read_text(), "fine, unstaged edit\n")
        kept = self.repo / ".git/richos-autocheck-aside/changed-during-check/richos/app/src/thing.txt"
        self.assertTrue(kept.is_file(), "the bytes written during the check were dropped:\n" + out.stderr)
        self.assertEqual(kept.read_text(), "fine, written during the check\n")
        self.assertEqual(out.returncode, 1, out.stderr)
        self.assertIn("richos/app/src/thing.txt changed while the check ran", out.stderr)
        self.assertIn(str(kept), out.stderr)
        self.assertEqual(self.head(), before)
        self.assertEqual(self.git("show", ":richos/app/src/thing.txt").stdout, "fine, staged\n")
        self.assertFalse((self.repo / ".git/richos-autocheck-aside/unstaged.patch").exists())
        # Until the engineer has looked and removed it, nothing is set aside on top of it.
        again = self.git("commit", "-m", "again", expect=None)
        self.assertEqual(again.returncode, 1, again.stderr)
        self.assertIn("compare each file under", again.stderr)
        self.assertEqual(kept.read_text(), "fine, written during the check\n")
        shutil.rmtree(self.repo / ".git/richos-autocheck-aside")
        self.assertEqual(self.git("commit", "-m", "after comparing", expect=None).returncode, 0)
        self.assertEqual((self.repo / "richos/app/src/thing.txt").read_text(), "fine, unstaged edit\n")
        self.assertEqual((self.repo / "richos/app/src/claims.txt").read_text(), "true\neditor save during the check\n")
        self.assert_aside_restored()

    def test_a_deletion_while_the_check_runs_is_recorded_and_named_never_silently_undone(self):
        # Recheck N01 v2 (2026-10-01): a file deleted during the check was recreated from the
        # index and the older unstaged patch reapplied over it; no bytes were saved for the
        # deletion, so the restore reported nothing and removed the aside.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/thing.txt", "fine, staged\n")
        self.git("add", "-A")
        self.write("richos/app/src/thing.txt", "fine, unstaged edit\n")
        before = self.head()
        env = dict(self.env, AUTOCHECK_FIXTURE_DURING_CHECK="rm -f richos/app/src/thing.txt")
        out = self.git("commit", "-m", "a deletion lands while the check runs", env=env, expect=None)
        self.assertEqual(out.returncode, 1, out.stderr)
        self.assertEqual(self.head(), before)
        self.assertIn("richos/app/src/thing.txt was deleted while the check ran", out.stderr)
        marker = self.repo / ".git/richos-autocheck-aside/changed-during-check/richos/app/src/thing.txt.deleted-during-check"
        self.assertTrue(marker.is_file(), "the deletion left no record:\n" + out.stderr)
        self.assertIn(str(marker), out.stderr)
        self.assertEqual((self.repo / "richos/app/src/thing.txt").read_text(), "fine, unstaged edit\n")
        self.assertEqual(self.git("commit", "-m", "again", expect=None).returncode, 1)

    def test_a_save_elsewhere_while_the_check_runs_commits_and_is_named(self):
        # The ordinary case: editing goes on during the commit, in a file the commit's unstaged
        # edits do not touch. It is not refused, nothing is reset, and the save is named.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/thing.txt", "fine, staged\n")
        self.git("add", "-A")
        self.write("richos/app/src/thing.txt", "fine, unstaged edit\n")
        env = dict(self.env, AUTOCHECK_FIXTURE_DURING_CHECK=(
            "printf 'true\\neditor save during the check\\n' > richos/app/src/claims.txt"))
        out = self.git("commit", "-m", "a save elsewhere", env=env, expect=None)
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertIn("written while the check ran, left exactly as found: richos/app/src/claims.txt", out.stderr)
        self.assertEqual((self.repo / "richos/app/src/thing.txt").read_text(), "fine, unstaged edit\n")
        self.assertEqual((self.repo / "richos/app/src/claims.txt").read_text(), "true\neditor save during the check\n")
        self.assert_aside_restored()

    def test_an_interrupted_set_aside_refuses_the_next_commit_and_touches_nothing(self):
        # A check killed while edits were set aside leaves them in the git directory; the next
        # commit must not set aside on top of them, and must say how to put them back.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        leftover = self.repo / ".git/richos-autocheck-aside"
        leftover.mkdir()
        (leftover / "unstaged.patch").write_text("an earlier check's saved edits\n")
        self.write("richos/app/src/thing.txt", "fine, again\n")
        self.git("add", "-A")
        before = self.head()
        out = self.git("commit", "-m", "after an interrupted check", expect=None)
        self.assertEqual(out.returncode, 1, out.stderr)
        self.assertIn("COMMIT REFUSED", out.stderr)
        self.assertIn(str(leftover), out.stderr)
        self.assertEqual(self.head(), before)
        self.assertEqual((leftover / "unstaged.patch").read_text(), "an earlier check's saved edits\n")

    def test_before_the_check_exists_the_hooks_do_nothing(self):
        self.make(with_checker=False)
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/bad.txt", "LINT-BAD\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "bad")
        self.assertEqual(out.stderr, "")
        self.assertEqual(self.tools(), "")


class BatteryTrailer(Fixture):
    """CEO ruling §81 at the commit (commit-msg). 2026-10-01: a trailer wrapped over three
    unindented lines is no trailer to git; only the land noticed."""
    WRAPPED = ("android: a change\n\nBattery-check: NO — no new background work: the deadline is a coroutine\n"
               "delay inside the existing bound, no timer, no wakeup,\nno wake lock.\n")
    INDENTED = ("android: a change\n\nBattery-check: NO — no new background work: the deadline is a coroutine\n"
                "  delay inside the existing bound, no timer, no wakeup,\n  no wake lock.\n")
    ONE_LINE = "android: a change\n\nBattery-check: NO - a record file only; no background work.\n"

    def stage_mobile(self, name="a.kt"):
        self.make()
        shutil.copy(HERE / "battery-check.py", self.repo / "richos/app/scripts/battery-check.py")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "battery check", "--no-verify")
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/mobile/" + name, "fun a() {}\n")
        self.git("add", "-A")

    def test_the_wrapped_unindented_trailer_is_refused_at_the_commit_and_says_how_to_write_it(self):
        self.stage_mobile()
        before = self.head()
        out = self.git("commit", "-m", self.WRAPPED, expect=1)
        text = out.stdout + out.stderr
        self.assertIn("COMMIT REFUSED", text)
        self.assertIn("has no `Battery-check:` trailer", text)
        self.assertIn("INDENTED", text)
        self.assertEqual(self.head(), before)

    def test_a_mobile_commit_with_no_trailer_at_all_is_refused(self):
        self.stage_mobile()
        self.git("commit", "-m", "android: a change", expect=1)

    def test_an_indented_continuation_and_a_one_line_trailer_both_commit(self):
        self.stage_mobile()
        self.git("commit", "-q", "-m", self.INDENTED)
        self.write("richos/mobile/b.kt", "fun b() {}\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", self.ONE_LINE)

    def test_a_commit_outside_the_mobile_folder_needs_no_trailer(self):
        self.stage_mobile()
        self.git("reset", "-q")
        self.write("README.md", "docs\n")
        self.git("add", "README.md")
        self.git("commit", "-q", "-m", "docs only")

    def test_amending_only_the_message_of_a_mobile_commit_is_checked_too(self):
        self.stage_mobile()
        self.git("commit", "-q", "-m", self.ONE_LINE)
        self.git("commit", "--amend", "-m", self.WRAPPED, expect=1)

    def test_the_pre_commit_selection_does_not_ask_the_battery_question_of_the_commit_being_made(self):
        # 2026-10-01, from 50a5bd1bd: pre-commit asked the merge gate's question over
        # <base>..<the index written as a commit>, and that stand-in's fixed message can never
        # carry a trailer, so every commit touching richos/mobile/ was refused at pre-commit
        # whatever its own message said. The answer is in the real message; commit-msg reads it.
        self.stage_mobile()
        before = self.head()
        out = self.git("commit", "-m", self.ONE_LINE)
        self.assertIn("maps cleanly", out.stderr)
        self.assertNotEqual(self.head(), before)
        self.assertIn("Battery-check: NO", self.git("log", "-1", "--format=%B").stdout)

    def test_an_earlier_unanswered_mobile_commit_on_the_branch_is_still_refused_at_pre_commit(self):
        # Only the commit not made yet is left to commit-msg: every commit before it is asked.
        self.stage_mobile()
        self.git("commit", "-q", "-m", "android: no answer", "--no-verify")
        self.write("richos/mobile/b.kt", "fun b() {}\n")
        self.git("add", "-A")
        before = self.head()
        out = self.git("commit", "-m", self.ONE_LINE, expect=1)
        self.assertIn("has not answered the battery question", out.stdout + out.stderr)
        self.assertEqual(self.head(), before)


class Land(Fixture):
    def test_a_merge_that_breaks_its_owning_suite_is_refused_before_main_moves(self):
        self.make()
        self.branch_with("feature", "richos/app/src/thing.txt", "BROKEN\n")
        before = self.head("main")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        text = out.stdout + out.stderr
        self.assertIn("MERGE INTO MAIN REFUSED", text)
        self.assertIn("suite: FAIL src/thing.txt is broken", text)
        self.assertEqual(self.head("main"), before)
        self.assertIn("run cd richos/app && bash scripts/suite.sh", self.tools())
        self.assertIn("lint --changed", self.tools())
        self.git("merge", "--abort")

    def test_a_merge_whose_tree_holds_a_listed_device_identifier_is_refused_without_printing_it(self):
        # 2026-10-02: a CoreDevice id reached a branch in a commit made before the commit-time
        # scan existed, and a merge carries what a branch already holds. The gate reads the whole
        # tree being landed: file:line and a masked preview, never the identifier (synthetic here).
        serial = "ZZDEVSERIAL0042"
        listing = self.base / "device-identifiers"
        listing.write_text(f"# synthetic\n{serial}\n")
        self.env["RICHOS_DEVICE_IDENTIFIERS_FILE"] = str(listing)
        self.make()
        self.branch_with("feature", "richos/app/src/audit.txt", f"line one\nphone {serial}\n", no_verify=True)
        before = self.head("main")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        text = out.stdout + out.stderr
        self.assertIn("MERGE INTO MAIN REFUSED: the tree holds an identifier of a known test device", text)
        self.assertIn("richos/app/src/audit.txt:2", text)
        self.assertNotIn(serial, text)
        self.assertEqual(self.head("main"), before)
        self.git("merge", "--abort")
        self.branch_with("clean", "richos/app/src/audit.txt", "phone <android-phone>\n")
        self.git("merge", "--no-ff", "-m", "land clean", "clean")

    def test_the_merge_gate_selects_with_gate_lints_the_change_and_caps_the_run(self):
        # 2026-09-30: merges of 25 to 71 minutes, whole-product suites for tooling changes and
        # no cap that a planned weight could not stretch. The land asks proof-for.sh for the
        # merge gate's selection, lints what the merge changes against the main it lands on, and
        # hands the runner a 600 s cap per check and the rest of the gate's 900 s for the run.
        self.make()
        self.branch_with("feature", "richos/app/src/thing.txt", "fine, better\n")
        self.git("merge", "--no-ff", "-m", "land feature", "feature")
        self.assertIn("proof-for --gate", self.side_log(".select"))
        self.assertIn("lint --changed", self.tools())
        self.assertNotIn("lint --all", self.tools())
        runner = self.side_log(".runner")
        self.assertIn("--cap 600", runner)
        left = int(runner.split("--run-cap ")[1].split()[0])
        self.assertTrue(60 <= left <= 900, runner)

    def test_a_check_that_reached_no_verdict_is_named_and_never_blocks(self):
        # Not admitted, a NOT RUN for any reason, or a pass invalidated only because its inputs
        # moved during the run: none of these is a failure of the change, so none refuses the
        # land. Each is named in the verdict and the receipt. (Over the cap or ended at the
        # gate's cap is retried once alone first: the tests after this one.)
        self.make()
        cases = (("not-admitted", "not admitted"),
                 ("invalid execution inputs changed during the check", "what it read changed while the gate ran"))
        for n, (state, why) in enumerate(cases):
            self.branch_with(f"feature{n}", "richos/app/src/state.txt", state + "\n")
            out = self.git("merge", "--no-ff", "-m", f"land {state}", f"feature{n}")
            self.assertIn("MERGE INTO MAIN ALLOWED WITH 1 CHECK(S) NOT RUN, WHICH IS NOT A PASS", out.stderr)
            self.assertIn(f"NOT RUN: cd richos/app && bash scripts/state.sh ({why})", out.stderr)
            receipt = json.loads((self.repo / ".git/richos-autocheck/land" / self.head("HEAD^{tree}")).read_text())
            self.assertEqual([(row["check"], row["why"]) for row in receipt["not_run"]],
                             [("cd richos/app && bash scripts/state.sh", why)])
        self.assertEqual(self.recorded(), "")

    def refused_after_retry(self, state, after="ONE RETRY"):
        self.make()
        self.branch_with("feature", "richos/app/src/state.txt", state + "\n")
        before = self.head("main")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        self.assertIn("MERGE INTO MAIN REFUSED", out.stderr)
        self.assertIn(f"NO VERDICT AFTER {after}: cd richos/app && bash scripts/state.sh ({state}); re-run it alone.",
                      out.stderr)
        self.assertEqual(self.head("main"), before)
        self.assertEqual(self.tools().count("run cd richos/app && bash scripts/state.sh"), 2)
        self.git("merge", "--abort")

    def landed_after_retry(self, state):
        self.make()
        self.branch_with("feature", "richos/app/src/state.txt", "once-" + state + "\n")
        self.git("merge", "--no-ff", "-m", "land feature", "feature")
        runner = self.side_log(".runner")
        self.assertIn("--resume", runner)
        self.assertIn("--retry-reason", runner)
        self.assertEqual(self.tools().count("run cd richos/app && bash scripts/state.sh"), 2)
        receipt = json.loads((self.repo / ".git/richos-autocheck/land" / self.head("HEAD^{tree}")).read_text())
        self.assertEqual(receipt["not_run"], [])

    # 2026-10-01, merge 7af4c981e: owning suites timed out and were ended at the Mac's 99% CPU
    # and the merge landed. No verdict twice (the retry alone included) refuses, naming the unit
    # and saying to re-run it alone; a pass on the retry lands.
    def test_the_retry_alone_runs_only_the_check_that_had_no_verdict(self):
        # Hunt v2 V02 (2026-10-01): the retry said "alone" but resumed the whole saved plan, so
        # every other unfinished check went back into the same scheduler and the contention
        # that timed the first one out came back. The retry now names its checks.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/state.txt", "once-timed-out\n")
        self.write("richos/app/src/screen.txt", "busy\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "two owning checks")
        self.git("checkout", "-q", "main")
        self.log.unlink(missing_ok=True)
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature")
        state, screen = "cd richos/app && bash scripts/state.sh", "cd richos/app && bash scripts/screen.sh"
        retries = [line for line in self.side_log(".runner").splitlines() if "--resume" in line]
        self.assertEqual(len(retries), 1, self.side_log(".runner"))
        self.assertIn("--only-check " + state, retries[0])
        self.assertNotIn(screen, retries[0])
        self.assertEqual(self.tools().count("run " + state), 2)
        self.assertEqual(self.tools().count("run " + screen), 1, "the retry ran a check it was not asked to retry")
        receipt = json.loads((self.repo / ".git/richos-autocheck/land" / self.head("HEAD^{tree}")).read_text())
        self.assertEqual([(row["check"], row["why"]) for row in receipt["not_run"]], [(screen, "busy")])
        self.assertIn("MERGE INTO MAIN ALLOWED WITH 1 CHECK(S) NOT RUN", out.stderr)

    def test_a_check_that_times_out_twice_refuses_the_merge(self):
        self.refused_after_retry("timed-out")

    def test_a_check_ended_at_the_gate_cap_twice_refuses_the_merge(self):
        # Ended in both rounds and nothing else decided in the second: a third would not either.
        self.refused_after_retry(ENDED, after="2 ROUNDS")

    # 2026-10-02, the combined land of cc/zach-opus-e2fix1 (214 checks): refused four times with
    # no failing check, because the plan needed about 2200 s of a gate capped at 900 s, and every
    # retry re-ran what had already passed. A large land runs in rounds that keep every pass.
    def make_two_slow_checks(self, state, later=None):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/state.txt", state + "\n")
        if later:
            self.write("richos/app/src/later.txt", later + "\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "two slow owning checks")
        self.git("checkout", "-q", "main")
        self.log.unlink(missing_ok=True)

    def runner_calls(self):
        return [line for line in self.side_log(".runner").splitlines() if line.strip()]

    def test_a_large_land_runs_in_rounds_until_every_check_has_a_verdict_and_lands(self):
        # Round 1: the suite and the lint pass, both slow checks are ended at the round's cap.
        # Round 2 (only those two): `later` passes, `state` is ended again. Round 3 (only
        # `state`): it passes. Nothing that passed runs again, and the merge lands.
        self.make_two_slow_checks("twice-" + ENDED, "once-" + ENDED)
        state, later = "cd richos/app && bash scripts/state.sh", "cd richos/app && bash scripts/later.sh"
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature")
        self.assertIn("round 3 of at most 6", out.stderr)
        self.assertEqual(self.tools().count("run cd richos/app && bash scripts/suite.sh"), 1)
        self.assertEqual(self.tools().count("run cd richos/app && bash scripts/lint.sh --changed"), 1)
        self.assertEqual((self.tools().count("run " + state), self.tools().count("run " + later)), (3, 2))
        calls = self.runner_calls()
        self.assertEqual(len(calls), 3, calls)
        # Each round resumes the round before it, and names only what still has no verdict.
        dirs = [call.split("--log-dir ")[1].split()[0] for call in calls]
        self.assertIn("--resume " + dirs[0], calls[1])
        self.assertIn("--resume " + dirs[1], calls[2])
        self.assertIn("--only-check " + later, calls[1])
        self.assertNotIn("--only-check " + later, calls[2])
        receipt = json.loads((self.repo / ".git/richos-autocheck/land" / self.head("HEAD^{tree}")).read_text())
        self.assertEqual((receipt["not_run"], receipt["rounds"]), ([], 3))

    def test_every_round_waits_for_admission_the_same_time_so_a_pass_keeps_its_identity(self):
        # The runner keys a pass by its settings; the first run waited what was left of the
        # gate (893 s) and the retry 900 s, so the retry matched none of the first run's passes.
        self.make_two_slow_checks("once-" + ENDED)
        self.git("merge", "--no-ff", "-m", "land feature", "feature")
        calls = self.runner_calls()
        self.assertEqual(len(calls), 2, calls)
        for flag in ("--admission-wait", "--slot-wait"):
            values = {call.split(flag + " ")[1].split()[0] for call in calls}
            self.assertEqual(values, {"900"}, (flag, calls))

    def test_a_refused_attempt_keeps_its_passes_and_the_next_attempt_on_the_same_tree_runs_only_the_rest(self):
        # Attempt 1 passes the suite and the lint and runs out of rounds for `state` (the Mac
        # stays too busy: round 2 decides nothing). Attempt 2 on the same tree, the merge
        # concluded with `git commit`, resumes it: only `state` runs, and the merge commits.
        self.make_two_slow_checks("held-" + ENDED)
        Path(str(self.log) + ".hold").touch()
        state = "cd richos/app && bash scripts/state.sh"
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        self.assertIn(f"NO VERDICT AFTER 2 ROUNDS: {state} ({ENDED}); re-run it alone.", out.stderr)
        self.assertIn("What passed on this tree is kept", out.stderr)
        before = self.head("main")
        Path(str(self.log) + ".hold").unlink()
        self.git("commit", "-m", "land feature")
        self.assertNotEqual(self.head("main"), before)
        self.assertEqual(self.tools().count("run cd richos/app && bash scripts/suite.sh"), 1)
        self.assertEqual(self.tools().count("run " + state), 3)
        calls = self.runner_calls()
        self.assertIn("--resume " + calls[1].split("--log-dir ")[1].split()[0], calls[2])
        for flag in ("--admission-wait", "--slot-wait"):
            self.assertEqual({call.split(flag + " ")[1].split()[0] for call in calls}, {"900"}, calls)

    def test_a_pass_not_carried_into_a_round_is_run_again_never_counted(self):
        # A check that passed in round 1 and that the runner could not carry into round 2 (its
        # inputs changed under it) is left out of round 2 like a check the round did not name.
        # It has no verdict, so round 3 runs it; it is never counted as passed from round 1.
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/state.txt", "once-" + ENDED + "\n")
        runner = self.repo / "richos/app/scripts/proof-run.py"
        carried = "        old = next((row for row in previous if row[\"check\"] == line and row[\"result\"] == \"passed\"), None)\n"
        self.assertIn(carried, runner.read_text())
        runner.write_text(runner.read_text().replace(carried, carried.replace(
            "), None)", "\n                    and not (\"suite.sh\" in line and only)), None)")))
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "a slow check; the runner cannot carry the suite's pass into a named round")
        self.git("checkout", "-q", "main")
        self.log.unlink(missing_ok=True)
        self.git("merge", "--no-ff", "-m", "land feature", "feature")
        self.assertEqual(self.tools().count("run cd richos/app && bash scripts/suite.sh"), 2, self.tools())
        self.assertIn("--only-check cd richos/app && bash scripts/suite.sh", self.runner_calls()[2])

    def test_a_check_that_times_out_and_passes_on_the_retry_lands(self):
        self.landed_after_retry("timed-out")

    def test_a_check_ended_at_the_gate_cap_and_passing_on_the_retry_lands(self):
        self.landed_after_retry(ENDED)

    # 2026-10-08, two merges: the gate refused on one timing check (setup.js case 24, splash.js
    # case 10b) while its own parallel checks held the Mac at 97-99% CPU, and the same check passed
    # when Rich reran it alone on the same tree. The gate now does that step itself: a check that
    # failed runs again alone, once, after the rest has finished, before the gate decides.
    STATE_CHECK = "cd richos/app && bash scripts/state.sh"

    def test_a_check_that_fails_only_under_the_gates_load_passes_alone_and_lands_saying_so(self):
        self.make_two_slow_checks("once-failed")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature")
        state = self.STATE_CHECK
        self.assertEqual(self.tools().count("run " + state), 2)
        self.assertEqual(self.tools().count("run cd richos/app && bash scripts/suite.sh"), 1,
                         "the retry alone ran a check that had passed")
        calls = self.runner_calls()
        self.assertEqual(len(calls), 2, calls)
        first = calls[0].split("--log-dir ")[1].split()[0]
        self.assertIn("--resume " + first, calls[1])
        self.assertIn("--only-check " + state, calls[1])
        self.assertIn("--retry-reason", calls[1])
        self.assertEqual(calls[1].count("--only-check"), 1, "alone means only this check")
        self.assertIn("--cap 600", calls[1])
        receipt = json.loads((self.repo / ".git/richos-autocheck/land" / self.head("HEAD^{tree}")).read_text())
        self.assertEqual(receipt["not_run"], [])
        [record] = receipt["failed_under_load_passed_alone"]
        self.assertEqual(record["check"], state)
        self.assertEqual((record["under_load"]["result"], record["alone"]["result"]), ("failed", "passed"))
        self.assertTrue(record["under_load"]["log"].startswith(first), record)
        self.assertNotEqual(Path(record["under_load"]["log"]).parent, Path(record["alone"]["log"]).parent)
        self.assertIn("STATE failed", Path(record["under_load"]["log"]).read_text())
        self.assertTrue(Path(record["alone"]["log"]).is_file())
        self.assertIn("FAILED UNDER LOAD, PASSED ALONE: " + state, out.stderr)
        self.assertIn(record["under_load"]["log"], out.stderr)
        self.assertIn(record["alone"]["log"], out.stderr)
        self.assertIn("failed under load and passed alone: " + state, out.stderr)

    def test_a_check_that_fails_alone_too_refuses_the_merge_naming_both_logs(self):
        self.make_two_slow_checks("failed")
        before = self.head("main")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        self.assertIn("MERGE INTO MAIN REFUSED: a check it owns failed", out.stderr)
        self.assertIn(f"FAILED: {self.STATE_CHECK} (failed)", out.stderr)
        self.assertIn("under the gate's load: failed, log ", out.stderr)
        self.assertIn("alone: failed, log ", out.stderr)
        self.assertEqual(self.head("main"), before)
        self.assertEqual(self.tools().count("run " + self.STATE_CHECK), 2)
        self.git("merge", "--abort")

    def test_never_a_second_retry_alone(self):
        # Fails in the gate and fails alone; it would pass a third time, which is never tried.
        self.make_two_slow_checks("twice-failed")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        self.assertIn("MERGE INTO MAIN REFUSED: a check it owns failed", out.stderr)
        self.assertEqual(self.tools().count("run " + self.STATE_CHECK), 2)
        self.assertEqual(len(self.runner_calls()), 2, self.runner_calls())
        self.git("merge", "--abort")

    def test_a_check_passed_alone_then_the_rounds_finish_the_checks_with_no_verdict(self):
        # `state` fails under load and `later` is ended at the round's cap in the same round: the
        # alone retry passes `state`, then round 2 runs `later`, and the merge lands.
        self.make_two_slow_checks("once-failed", "once-" + ENDED)
        later = "cd richos/app && bash scripts/later.sh"
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature")
        calls = self.runner_calls()
        self.assertEqual(len(calls), 3, calls)
        self.assertIn("--only-check " + self.STATE_CHECK, calls[1])
        self.assertNotIn(later, calls[1])
        self.assertIn("--only-check " + later, calls[2])
        self.assertIn("--resume " + calls[1].split("--log-dir ")[1].split()[0], calls[2])
        self.assertEqual((self.tools().count("run " + self.STATE_CHECK), self.tools().count("run " + later)), (2, 2))
        receipt = json.loads((self.repo / ".git/richos-autocheck/land" / self.head("HEAD^{tree}")).read_text())
        self.assertEqual((receipt["not_run"], receipt["rounds"]), ([], 2))
        self.assertEqual([r["check"] for r in receipt["failed_under_load_passed_alone"]], [self.STATE_CHECK])
        self.assertIn("FAILED UNDER LOAD, PASSED ALONE", out.stderr)

    def test_simulator_and_screen_suites_are_left_to_the_nightly_and_named(self):
        # 2026-09-30: the iPhone suites fought over the one simulator in merge after merge. The
        # merge never runs a suite that needs a device this Mac has one of: the simulator, or a
        # screen (the host's, or the test VM's guest). The other suites on the same line run.
        self.make()
        self.branch_with("feature", "richos/app/src/device.txt", "a phone change\n")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature")
        self.assertIn("suite phone-unit.test.sh", self.tools())
        self.assertNotIn("ran-simulator-suite", self.tools())
        self.assertNotIn("ran-screen-suite", self.tools())
        self.assertIn("MERGE INTO MAIN ALLOWED WITH 3 CHECK(S) NOT RUN, WHICH IS NOT A PASS", out.stderr)
        receipt = json.loads((self.repo / ".git/richos-autocheck/land" / self.head("HEAD^{tree}")).read_text())
        whys = {row["check"]: row["why"] for row in receipt["not_run"]}
        self.assertEqual(sorted(whys), ["front-door.test.sh", "native-ios-share.test.sh", "native-ios-ui.test.sh"])
        self.assertTrue(all(why.endswith("the nightly runs it") for why in whys.values()), whys)
        self.assertIn("iPhone simulator", whys["native-ios-ui.test.sh"])
        self.assertIn("screen", whys["front-door.test.sh"])

    def test_a_mutation_unit_is_never_started_in_the_merge_and_is_named_for_the_nightly(self):
        # 2026-09-30, the merge of cc/zach-opus-q1 (4e73fd89): the selector chose the fence
        # suite's mutation unit (planned 1408 s) because a comment in it names a file the land
        # changed. It ran into its cap and the gate passed with 60 of 63 checks NOT RUN. A unit
        # that is a mutation pass never reaches the runner; the unit beside it still runs.
        self.make()
        self.branch_with("feature", "richos/app/src/mutant.txt", "a change the selector maps to it\n")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature")
        self.assertNotIn("operator-fences-mutation", self.tools())
        self.assertIn("--only-units scripts/spawn.test.sh", self.tools())
        receipt = json.loads((self.repo / ".git/richos-autocheck/land" / self.head("HEAD^{tree}")).read_text())
        whys = {row["check"]: row["why"] for row in receipt["not_run"]}
        self.assertEqual(sorted(whys), ["engine scripts/operator-fences-mutation.test.sh"], out.stderr)
        self.assertIn("a mutation pass", whys["engine scripts/operator-fences-mutation.test.sh"])
        self.assertIn("NOT RUN: engine scripts/operator-fences-mutation.test.sh (a mutation pass", out.stderr)

    def test_the_merge_gate_runs_every_check_with_the_mutation_passes_switched_off(self):
        # The lead's decision on esc-20260930T223507Z-b12f0d6a: the passes suites run at their own
        # end leave the merge too; the nightly engine run switches them on.
        self.make()
        self.branch_with("feature", "richos/app/src/thing.txt", "fine, better\n")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature")
        self.assertIn("suite-env RICHOS_MUTATION_PASSES=0 RICHOS_FOURTEEN_MUTANTS=0", self.tools())
        self.assertIn("mutation passes are off in the merge", out.stderr)

    def test_measure_runs_the_land_check_on_what_is_staged_and_writes_no_receipt(self):
        # How the gate is measured on a replayed merge: the land check of the staged change,
        # run by hand on a branch, with its verdict and time and no land receipt.
        self.make()
        self.git("checkout", "-q", "-b", "replay")
        measure = [sys.executable, str(self.repo / "richos/app/scripts/autocheck/autocheck.py"), "measure"]
        self.write("richos/app/src/thing.txt", "BROKEN\n")
        self.git("add", "-A")
        out = self.run_(measure)
        self.assertEqual(out.returncode, 1, out.stderr)
        self.assertIn("MEASURE REFUSED: a check it owns failed", out.stderr)
        self.write("richos/app/src/thing.txt", "fine, replayed\n")
        self.git("add", "-A")
        out = self.run_(measure)
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertIn("autocheck: measure: every selected check passed", out.stderr)
        # The broken run, its one retry alone (it fails there too), and the fixed run.
        self.assertEqual(self.tools().count("run cd richos/app && bash scripts/suite.sh"), 3)
        self.assertFalse((self.repo / ".git/richos-autocheck/land").exists())

    def test_a_failing_check_blocks_and_so_does_an_invalid_result_that_hid_a_failure(self):
        self.make()
        self.branch_with("feature", "richos/app/src/state.txt",
                         "invalid exited 0 but its evidence ledger records 1 failed check(s)\n")
        before = self.head("main")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        self.assertIn("MERGE INTO MAIN REFUSED: a check it owns failed", out.stderr)
        self.assertIn("FAILED: cd richos/app && bash scripts/state.sh (invalid)", out.stderr)
        self.assertEqual(self.head("main"), before)
        self.git("merge", "--abort")

    def test_same_merge_retry_resumes_saved_plan_and_keeps_passing_lint(self):
        self.make()
        self.branch_with("feature", "richos/app/src/thing.txt", "BROKEN\n")
        self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        first = self.tools()
        self.assertEqual(first.count("run cd richos/app && bash scripts/lint.sh --changed"), 1)
        self.git("commit", "-m", "land feature", expect=1)
        second = self.tools()
        self.assertIn("resume ", second[len(first):])
        self.assertEqual(second.count("run cd richos/app && bash scripts/lint.sh --changed"), 1)
        # Each attempt runs the failing suite and then once more alone (retry_failed_alone).
        self.assertEqual(first.count("run cd richos/app && bash scripts/suite.sh"), 2)
        self.assertEqual(second.count("run cd richos/app && bash scripts/suite.sh"), 4)
        attempts = list((self.base / "proof-runs").glob("*/attempt-*/plan.json"))
        self.assertEqual(len(attempts), 4)
        self.git("merge", "--abort")

    def test_fixed_merge_tree_selects_new_plan_and_offers_old_evidence_for_validation(self):
        self.make()
        self.branch_with("feature", "richos/app/src/thing.txt", "BROKEN\n")
        self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        first = self.tools()   # the failing suite, and its one retry alone (a resume)
        (self.repo / "richos/app/src/thing.txt").write_text("fixed\n")
        self.git("add", "richos/app/src/thing.txt")
        self.git("commit", "-m", "land fixed feature")
        second = self.tools()[len(first):]
        self.assertIn("reuse ", second)
        self.assertNotIn("resume ", second)
        self.assertEqual(first.count("run cd richos/app && bash scripts/suite.sh"), 2)
        self.assertEqual(second.count("run cd richos/app && bash scripts/suite.sh"), 1)

    def test_missing_retry_plan_refuses_without_restarting_passing_work(self):
        self.make()
        self.branch_with("feature", "richos/app/src/thing.txt", "BROKEN\n")
        self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        # The plan the next attempt resumes: the one last-attempt.json names. A refused merge now
        # leaves two attempts (the round and the failed check's retry alone), so "the first
        # plan.json a glob finds" is either of them, by directory order.
        prior = next((self.base / "proof-runs").glob("*/last-attempt.json"))
        plan = Path(json.loads(prior.read_text())["directory"]) / "plan.json"
        self.assertTrue(plan.is_file())
        plan.unlink()
        before = self.tools()
        out = self.git("commit", "-m", "land feature", expect=1)
        self.assertIn("retry evidence is unreadable", out.stderr)
        self.assertEqual(self.tools(), before)
        self.git("merge", "--abort")

    def test_a_good_merge_lands_with_a_receipt_and_no_record(self):
        self.make()
        self.branch_with("feature", "richos/app/src/thing.txt", "fine, better\n")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature")
        self.assertIn("every selected check passed", out.stderr)
        tree = self.head("HEAD^{tree}")
        self.assertTrue((self.repo / ".git/richos-autocheck/land" / tree).exists())
        self.assertEqual(self.recorded(), "")

    def test_every_line_of_the_land_plan_is_one_the_real_runner_reads_the_phone_scan_included(self):
        # 2026-10-02: the land added the physical-phone scan as a bare `python3 richos/mobile/
        # physical.py scan`, and the real proof-run.py refused the whole plan ("cannot read line 2
        # of the selection") before any check ran, so every land that selected it was refused. The
        # fixture runner runs any shell line, so only the real runner's reader can catch it.
        scanner = self.repo / "richos/mobile/physical.py"
        scanner.parent.mkdir(parents=True, exist_ok=True)
        scanner.write_text("import os, sys\n"
                           "with open(os.environ['AUTOCHECK_FIXTURE_LOG'], 'a') as log:\n"
                           "    log.write('physical ' + ' '.join(sys.argv[1:]) + '\\n')\n"
                           "print('physical.py scan: no phone install, uninstall or wipe outside the command lines')\n")
        self.make()
        self.branch_with("feature", "richos/app/src/thing.txt", "fine, better\n")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature")
        self.assertIn("every selected check passed", out.stderr)
        self.assertIn("physical scan", self.tools())
        plan = next((self.base / "proof-runs").glob("*/attempt-*/plan.json"))
        lines = json.loads(plan.read_text())
        self.assertTrue(any("physical.py scan" in line for line in lines), lines)
        selection = self.base / "selection.txt"
        selection.write_text("\n".join(lines) + "\n")
        logs = self.base / "real-runner-logs"
        env = dict(self.env, RICHOS_RUNTIME_DIR=str(self.base / "no-runtime"))
        real = subprocess.run([sys.executable, str(HERE / "proof-run.py"), "--commands", str(selection),
                               "--dry-run", "--log-dir", str(logs)],
                              env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=120)
        self.assertEqual(real.returncode, 0, real.stdout + real.stderr)
        self.assertIn("from %d selected command(s)" % len(lines), real.stdout)
        self.assertIn("cd richos/mobile && python3 physical.py scan", real.stdout)

    def test_a_land_that_changes_nothing_under_the_app_runs_no_application_lint(self):
        # Hunt part 2, finding 13: a docs-only land paid for `lint.sh --all` (58 s warm) although
        # nothing it changed is application code. The commit check already skips it.
        self.make()
        self.branch_with("docs", "README.md", "docs\n")
        out = self.git("merge", "--no-ff", "-m", "land docs", "docs")
        self.assertNotIn("lint --", self.tools())
        self.assertNotIn("lint.sh --", out.stderr)
        self.assertEqual(self.recorded(), "")

    def test_a_land_that_deletes_an_app_file_still_runs_the_lint(self):
        self.make()
        self.git("checkout", "-q", "-b", "gone")
        self.git("rm", "-q", "richos/app/src/thing.txt")
        self.git("commit", "-q", "-m", "delete", "--no-verify")
        self.git("checkout", "-q", "main")
        self.log.unlink(missing_ok=True)
        self.git("merge", "--no-ff", "-m", "land deletion", "gone", expect=None)
        self.assertIn("lint --changed", self.tools())

    def test_a_land_whose_screen_suites_did_not_run_lands_and_says_not_run(self):
        # 2026-09-29: front-door and gui-boot were NOT RUN (no screen) in the refused land of
        # 6ef73abf and recorded as passed. NOT RUN is not a pass; the one a land accepts is a
        # suite that needs the screen a land may never use, and it says so where it lands.
        self.make()
        self.branch_with("feature", "richos/app/src/screen.txt", "no-screen\n")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature")
        self.assertIn("MERGE INTO MAIN ALLOWED WITH 1 CHECK(S) NOT RUN, WHICH IS NOT A PASS", out.stderr)
        self.assertIn("NOT RUN: cd richos/app && bash scripts/screen.sh (no-screen)", out.stderr)
        self.assertNotIn("every selected check passed", out.stderr)
        receipt = json.loads((self.repo / ".git/richos-autocheck/land" / self.head("HEAD^{tree}")).read_text())
        self.assertEqual([row["check"] for row in receipt["not_run"]], ["cd richos/app && bash scripts/screen.sh"])
        self.assertEqual(self.recorded(), "")

    def test_a_land_with_any_other_check_not_run_lands_and_names_it(self):
        # Until 2026-09-30 a host-gap NOT RUN refused the land. A check that did not run is not
        # a failure of the change; it is named, recorded as NOT RUN and left to the nightly.
        self.make()
        self.branch_with("feature", "richos/app/src/screen.txt", "host-gap\n")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature")
        self.assertIn("MERGE INTO MAIN ALLOWED WITH 1 CHECK(S) NOT RUN, WHICH IS NOT A PASS", out.stderr)
        self.assertIn("NOT RUN: cd richos/app && bash scripts/screen.sh (host-gap)", out.stderr)
        self.assertNotIn("every selected check passed", out.stderr)

    def test_no_verify_merge_is_recorded(self):
        self.make()
        self.branch_with("feature", "richos/app/src/thing.txt", "BROKEN\n")
        self.git("merge", "--no-ff", "--no-verify", "-m", "land feature", "feature")
        self.assertIn("without the land checks (git merge --no-verify)", self.recorded())

    def test_a_fast_forward_is_recorded_and_the_push_runs_the_checks(self):
        self.make()
        remote = self.base / "remote.git"
        self.git("init", "-q", "--bare", str(remote))
        self.git("remote", "add", "origin", str(remote))
        self.git("push", "-q", "origin", "main")  # the base: checked here, and it passes
        self.branch_with("feature", "richos/app/src/thing.txt", "BROKEN\n")
        self.git("merge", "--ff-only", "feature")
        self.assertIn("without the land checks (a fast-forward)", self.recorded())
        out = self.git("push", "origin", "main", expect=1)
        self.assertIn("PUSH REFUSED: a check it owns failed", out.stderr)
        # HEAD is already the land there, so the lint measures the whole tree.
        self.assertIn("lint --all", self.tools())
        self.assertNotEqual(self.git("rev-parse", "main", cwd=remote).stdout.strip(), self.head("main"))

    def test_main_fast_forwarded_onto_a_merge_commit_is_named_a_fast_forward(self):
        self.make()
        self.git("checkout", "-q", "-b", "side")
        self.write("richos/app/src/side.txt", "side work\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "side work")
        self.git("checkout", "-q", "main")
        self.land_a_merge_commit_on_main()
        self.git("checkout", "-q", "side")
        self.git("merge", "--no-ff", "--no-verify", "-m", "a merge made off main", "main")
        self.git("checkout", "-q", "main")
        self.ledger.unlink(missing_ok=True)
        self.git("merge", "--ff-only", "side")
        self.assertIn("without the land checks (a fast-forward)", self.recorded())
        self.assertNotIn("--no-verify", self.recorded())

    def test_an_uncovered_path_refuses_the_merge(self):
        # Its own commit refuses it now (Commit, below); a branch that got past that with
        # --no-verify is still refused at the land.
        self.make()
        self.branch_with("feature", "richos/app/src/uncovered.txt", "new code\n", no_verify=True)
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        self.assertIn("covered by no suite", out.stderr)

    # 2026-10-01: cc/echo-opus-voiceecho1, cut at ae55aa0d2 and touching no engine file, was
    # refused at main bb112ab68 with seven of main's own engine files "named by NO suite": the
    # gate asked about `HEAD..<branch tip>`, whose diff is the two trees (main's later files
    # included), read from the branch tip that predates their claims.
    def land_on_main(self, files):
        """main moves on: `files` land through an ordinary checked merge."""
        self.git("checkout", "-q", "-b", "landed")
        for rel, text in files.items():
            self.write(rel, text)
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "main's next land")
        self.git("checkout", "-q", "main")
        self.git("merge", "--no-ff", "-q", "-m", "land landed", "landed")
        for suffix in ("", ".paths", ".select"):
            Path(str(self.log) + suffix).unlink(missing_ok=True)

    def asked(self):
        return {line.strip() for line in self.side_log(".paths").splitlines() if line.strip()}

    def test_a_branch_cut_before_mains_last_land_is_mapped_to_its_own_files_only(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/thing.txt", "fine, better\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "the branch's own change")
        self.git("checkout", "-q", "main")
        self.land_on_main({"richos/app/src/owned/a.txt": "main's code\n", "richos/app/owners/a.txt": "claim\n"})
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=None)
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertEqual(self.asked(), {"richos/app/src/thing.txt"})

    def test_a_file_main_claimed_after_the_branch_was_cut_maps_at_the_land(self):
        # The branch changes a file whose claim main landed after the cut: the tree being landed
        # holds the claim, so the selection reads it there and the merge lands.
        self.write("richos/app/src/owned/b.txt", "old\n")
        self.make()
        self.branch_with("feature", "richos/app/src/owned/b.txt", "new\n", no_verify=True)
        self.land_on_main({"richos/app/owners/b.txt": "claim\n"})
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=None)
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertEqual(self.asked(), {"richos/app/src/owned/b.txt"})

    def test_a_branch_cut_before_mains_last_land_is_still_refused_for_its_own_unclaimed_file(self):
        self.make()
        self.git("checkout", "-q", "-b", "feature")
        self.write("richos/app/src/owned/c.txt", "nobody claims this\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "unclaimed", "--no-verify")
        self.git("checkout", "-q", "main")
        self.land_on_main({"richos/app/src/owned/a.txt": "main's code\n", "richos/app/owners/a.txt": "claim\n"})
        before = self.head("main")
        out = self.git("merge", "--no-ff", "-m", "land feature", "feature", expect=1)
        self.assertIn("UNCOVERED: richos/app/src/owned/c.txt", out.stderr)
        self.assertIn("covered by no suite", out.stderr)
        self.assertNotIn("owned/a.txt", out.stderr)
        self.assertEqual(self.head("main"), before)
        self.git("merge", "--abort")

    def test_a_direct_commit_on_main_runs_the_land_checks(self):
        self.make()
        self.write("richos/app/src/thing.txt", "BROKEN\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "straight onto main", expect=1)
        self.assertIn("COMMIT TO MAIN REFUSED", out.stderr)

    def test_a_working_tree_that_differs_from_the_merge_is_refused(self):
        self.make()
        self.branch_with("feature", "richos/app/src/thing.txt", "fine, better\n")
        self.git("merge", "--no-ff", "--no-commit", "feature", expect=None)
        self.write("richos/app/scripts/suite.sh", SUITE + "# edited during the merge\n")
        out = self.git("commit", "-m", "land feature", expect=1)
        self.assertIn("working tree differs", out.stderr)

    def test_the_land_that_introduces_the_check_is_checked_by_it(self):
        self.make(with_checker=False)
        self.git("checkout", "-q", "-b", "intro")
        self.add_checker()
        self.write("richos/app/src/thing.txt", "BROKEN\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "introduce the check")
        self.git("checkout", "-q", "main")
        out = self.git("merge", "--no-ff", "-m", "land intro", "intro", expect=1)
        self.assertIn("MERGE INTO MAIN REFUSED", out.stderr)


class Tables(unittest.TestCase):
    def test_every_suite_the_merge_leaves_to_the_nightly_exists_in_this_tree(self):
        # A named table rots when a suite is renamed; this reads it against the real tree.
        import importlib.util
        spec = importlib.util.spec_from_file_location("autocheck_under_test", AUTOCHECK / "autocheck.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        missing = [suite for suite in module.SHARED_DEVICE_SUITES if not (HERE / suite).is_file()]
        self.assertEqual(missing, [])
        self.assertTrue(module.SHARED_DEVICE_SUITES)

    def test_a_mutation_unit_is_known_by_its_name_or_its_declaration(self):
        # Every mutation unit, not only the one that ran into its cap: by the name convention
        # the fence pass follows, or by a header line for a unit whose name does not say so.
        import importlib.util
        spec = importlib.util.spec_from_file_location("autocheck_under_test", AUTOCHECK / "autocheck.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as top:
            scripts = Path(top) / "richos/engine/scripts"
            scripts.mkdir(parents=True)
            (scripts / "declared.test.sh").write_text("#!/usr/bin/env bash\n# merge-gate: mutation-pass (86 mutants)\n")
            (scripts / "plain.test.sh").write_text("#!/usr/bin/env bash\n# mentions merge-gate: mutation-pass inline\n")

            class Repo:
                pass
            repo = Repo()
            repo.top = Path(top)
            self.assertTrue(module.mutation_unit(repo, "scripts/declared.test.sh"))
            self.assertTrue(module.mutation_unit(repo, "scripts/absent-mutation.test.sh"))
            self.assertTrue(module.mutation_unit(repo, "scripts/declared.test.sh:SECTION"))
            self.assertFalse(module.mutation_unit(repo, "scripts/plain.test.sh"))
            self.assertFalse(module.mutation_unit(repo, "scripts/lib/mutation-harness.test.sh"))
        # And the real tree: the fence pass is one, the harness library's own tests are not.
        real = Repo()
        real.top = HERE.parents[1].parent
        self.assertTrue(module.mutation_unit(real, "scripts/operator-fences-mutation.test.sh"))
        self.assertFalse(module.mutation_unit(real, "scripts/lib/mutation-pool.test.sh"))

    def test_every_qualified_unit_that_reads_a_harness_lets_the_switch_through(self):
        # A qualified engine unit runs under proof-run's private profile, whose environment holds
        # only the names its reviewed contract declares (lib/proof_evidence.py
        # prepare_environment). Undeclared, RICHOS_MUTATION_PASSES=0 never reaches the harness
        # and the pass runs in the merge after all: found replaying the merge of 4e73fd89.
        data = json.loads((HERE.parents[2] / "docs/development/verification-input-qualifications.json").read_text())
        reading = {name: unit for name, unit in data["units"].items()
                   if any(s.endswith((".mutation.sh", ".mutation.py")) for s in unit.get("sources", {}))}
        self.assertTrue(reading)
        undeclared = sorted(name for name, unit in reading.items()
                            if not {"RICHOS_MUTATION_PASSES", "RICHOS_FOURTEEN_MUTANTS"}
                            <= set(unit.get("requires", {}).get("environment", [])))
        self.assertEqual(undeclared, [], "qualified units whose harness never sees the merge gate's switch")

    def test_every_app_mutation_harness_honors_the_merge_gates_switch(self):
        # The engine's harnesses are held to the same line by the engine's own
        # scripts/mutation-inventory.test.sh (section 3). These are the app's: discovered from
        # disk, each must stop at once under the switch, print NOT RUN and exit 0.
        harnesses = sorted(HERE.rglob("*.mutation.py"))
        self.assertTrue(harnesses)
        missing = [str(h.relative_to(HERE)) for h in harnesses
                   if 'environ.get("RICHOS_MUTATION_PASSES") == "0"' not in h.read_text()]
        self.assertEqual(missing, [], "harnesses the merge gate cannot switch off")
        env = {**os.environ, "RICHOS_MUTATION_PASSES": "0"}
        for harness in harnesses:
            out = subprocess.run([sys.executable, str(harness)], env=env, capture_output=True, text=True, timeout=30)
            self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
            self.assertIn("NOT RUN: " + harness.name, out.stdout)

    def test_with_passes_off_a_mutant_that_no_longer_matches_its_source_refuses_the_merge(self):
        # 2026-10-05: 419e71e71 renamed a nightly-local case and the text a mutant read. The merge
        # ran nightly-local.mutation.py with RICHOS_MUTATION_PASSES=0, the harness checked nothing,
        # and only the nightly refused (df0feab6e). With passes off each app harness still
        # searches its source: every mutant's text exactly once, every case it names present.
        # Each drift is made in a private copy of what the harness reads; nothing shipped is written.
        root = HERE.parents[2]
        s = "richos/app/scripts/"

        def first_mutant(rel):
            spec = importlib.util.spec_from_file_location("harness_under_test", root / rel)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            return module.MUTANTS[0]

        nl = first_mutant(s + "nightly-local.mutation.py")      # (name, case, old, ...)
        rr = first_mutant(s + "runner-reliability.mutation.py")  # (name, file, case, old, ...)
        cl = first_mutant(s + "testvm/test/claude-login.mutation.py")  # (name, file, old, new, case)
        nl_reads = [s + "nightly-local.py", s + "nightly-local.test.py"]
        rr_reads = [s + "runner-reliability.test.py"] + [
            "richos/engine/scripts/lib/" + n for n in ("proc_tree.py", "worker_tokens.py", "operator_fences.py")]
        pa_reads = ([str(p.relative_to(root)) for p in HERE.glob("*.test.sh")]
                    + [s + "phone-app-suites.tsv", s + "phone-apps-independent.test.py",
                       "richos/mobile/test/client-part2.test.js"])
        cl_reads = [s + "testvm/claude-login.sh", s + "testvm/test/run-tests.sh"]
        self.assertEqual(rr[1], "proc_tree.py", "the drift below edits the file runner-reliability's first mutant reads")
        # A mutant's text drifts the way an edit drifts it: one space after the indentation is
        # doubled, so the file still parses (runner-reliability imports proc_tree before it reads
        # anything) and the old text is no longer inside the new.
        def drift(old):
            i = old.index(" ", len(old) - len(old.lstrip()))
            return old[:i] + " " + old[i:]

        # (harness, what it reads, the file drifted, old text, new text (None empties it), the refusal)
        drifts = [
            (s + "nightly-local.mutation.py", nl_reads, s + "nightly-local.py", nl[2], drift(nl[2]),
             f"{nl[0]}: the text to mutate appears 0 times"),
            (s + "nightly-local.mutation.py", nl_reads, s + "nightly-local.test.py",
             f"def {nl[1]}(", f"def {nl[1]}_renamed(", f"{nl[1]} is not a test"),
            (s + "runner-reliability.mutation.py", rr_reads, "richos/engine/scripts/lib/proc_tree.py", rr[3], drift(rr[3]),
             f"{rr[0]}: the text to mutate appears 0 times"),
            (s + "runner-reliability.mutation.py", rr_reads, s + "runner-reliability.test.py",
             f"def {rr[2]}(", f"def {rr[2]}_renamed(", f"{rr[2]} is not a test"),
            (s + "phone-apps-independent.mutation.py", pa_reads, s + "phone-app-suites.tsv", None, None,
             "its edit changed nothing"),
            (s + "testvm/test/claude-login.mutation.py", cl_reads, s + "testvm/" + cl[1], cl[2], drift(cl[2]),
             f"{cl[0]}: the text to mutate appears 0 times"),
            (s + "testvm/test/claude-login.mutation.py", cl_reads, s + "testvm/test/run-tests.sh", cl[4], "a renamed case",
             f'no case in test/run-tests.sh is named "{cl[4]}"'),
        ]
        env = {**os.environ, "RICHOS_MUTATION_PASSES": "0", "PYTHONDONTWRITEBYTECODE": "1"}
        for harness, reads, drifted, old, new, refusal in drifts:
            with self.subTest(harness=harness, drifted=drifted), tempfile.TemporaryDirectory() as top:
                for rel in [harness] + reads:
                    (Path(top) / rel).parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(root / rel, Path(top) / rel)
                run = [sys.executable, str(Path(top) / harness)]
                clean = subprocess.run(run, env=env, capture_output=True, text=True, timeout=60)
                self.assertEqual(clean.returncode, 0, "the undrifted copy must pass: " + clean.stdout + clean.stderr)
                target = Path(top) / drifted
                text = target.read_text()
                if old is None:
                    text = ""
                else:
                    self.assertEqual(text.count(old), 1, old)
                    text = text.replace(old, new)
                target.write_text(text)
                out = subprocess.run(run, env=env, capture_output=True, text=True, timeout=60)
                self.assertEqual(out.returncode, 1, "a drifted mutant passed the merge: " + out.stdout + out.stderr)
                self.assertIn(refusal, out.stdout)
                self.assertIn("NOT RUN: " + Path(harness).name, out.stdout)


class Install(Fixture):
    def test_install_check_and_uninstall_touch_only_their_own_hooks(self):
        self.make(install=False)
        foreign = self.repo / ".git/hooks/pre-push"
        foreign.write_text("#!/bin/sh\nexit 0\n")
        out = self.run_(["bash", str(AUTOCHECK / "install.sh"), str(self.repo)])
        self.assertEqual(out.returncode, 1)
        self.assertIn("REFUSED", out.stdout)
        self.assertEqual(foreign.read_text(), "#!/bin/sh\nexit 0\n")
        foreign.unlink()
        self.install()
        self.assertEqual(self.run_(["bash", str(AUTOCHECK / "install.sh"), "--check", str(self.repo)]).returncode, 0)
        out = self.run_(["bash", str(AUTOCHECK / "install.sh"), "--uninstall", str(self.repo)])
        # Six hooks, and the merge driver for the verification maps the install registered
        # (install.sh header; added in 480bf8dd5). Its repository config goes with them.
        self.assertEqual(out.stdout.count("REMOVED"), 7)
        self.assertIn("REMOVED    merge.richos-verification-pins", out.stdout)
        self.git("config", "--get", "merge.richos-verification-pins.driver", expect=1)
        self.assertEqual(self.run_(["bash", str(AUTOCHECK / "install.sh"), "--check", str(self.repo)]).returncode, 1)

    def test_a_hooks_path_that_never_calls_the_hook_is_reported(self):
        self.make()
        empty = self.base / "empty-hooks"
        empty.mkdir()
        self.git("config", "core.hooksPath", str(empty))
        out = self.run_(["bash", str(AUTOCHECK / "install.sh"), "--check", str(self.repo)])
        self.assertEqual(out.returncode, 1)
        self.assertIn("UNREACHABLE", out.stdout)


class TimeLimit(unittest.TestCase):
    """A check that runs out of time ends gracefully and leaves no process (2026-10-09: the
    test browser of a UI suite killed by subprocess.run's SIGKILL aborted at launch, parent
    "Exited process", and macOS showed "Playwright quit unexpectedly")."""

    SUITE = (
        "import os, signal, subprocess, sys, time\n"
        "browser = subprocess.Popen(['sleep', '300'])\n"  # the stand-in test browser
        "open(sys.argv[1], 'w').write(str(browser.pid))\n"
        "if sys.argv[2] == 'polite':\n"  # Playwright closes its browser on SIGTERM
        "    def bye(*_):\n"
        "        browser.terminate(); open(sys.argv[1] + '.term', 'w').write('x'); os._exit(0)\n"
        "    signal.signal(signal.SIGTERM, bye)\n"
        "else:\n"
        "    signal.signal(signal.SIGTERM, signal.SIG_IGN)\n"
        "time.sleep(300)\n"
    )

    def alive(self, pid):
        try:
            os.kill(pid, 0)
            return True
        except ProcessLookupError:
            return False

    def run_late(self, mode, grace):
        spec = importlib.util.spec_from_file_location("autocheck_under_test", AUTOCHECK / "autocheck.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as top:
            marker = os.path.join(top, "browser.pid")
            with self.assertRaises(subprocess.TimeoutExpired):
                # load-bound: the 1 s limit is the stimulus (the suite sleeps 300 s, never finishing); no verdict depends on host speed
                module.run_bounded([sys.executable, "-c", self.SUITE, marker, mode], timeout=1, grace=grace, text=True)
            pid = int(Path(marker).read_text())
            deadline = __import__("time").time() + 3
            while self.alive(pid) and __import__("time").time() < deadline:
                __import__("time").sleep(0.05)
            alive = self.alive(pid)
            if alive:
                os.kill(pid, 9)
            return alive, os.path.exists(marker + ".term")

    def test_admitted_checks_do_not_spend_their_hang_guard_in_the_worker_queue(self):
        import fcntl
        import threading
        import time
        import types
        spec = importlib.util.spec_from_file_location("autocheck_under_test", AUTOCHECK / "autocheck.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as scratch:
            workers = Path(scratch) / "workers"
            tool = ENGINE / "scripts/lib/worker_tokens.py"
            subprocess.run([sys.executable, str(tool), "init", str(workers),
                            str(max(1, int((os.cpu_count() or 4) * 0.8)))], check=True)
            env = {k: v for k, v in os.environ.items() if not k.startswith("RICHOS_WORKER_")}
            env["RICHOS_MACHINE_WORKERS"] = str(workers)
            held = []
            for token in sorted(workers.glob("token-*")):
                fd = os.open(token, os.O_RDWR)
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                held.append(fd)
            def release():
                try:
                    guard = time.monotonic() + HANG_GUARD
                    while not list(workers.glob("wait-*")) and time.monotonic() < guard:
                        time.sleep(0.05)
                    time.sleep(3)  # Deliberately outlast the two-second execution budget.
                finally:
                    for fd in held:
                        os.close(fd)
            releaser = threading.Thread(target=release)
            releaser.start()
            try:
                repo = types.SimpleNamespace(top=HERE.parents[2], env=env)
                # load-bound: two seconds is the queue test stimulus, never a host-speed verdict.
                result = module.run_admitted(repo, ["/usr/bin/true"], timeout=2,
                                             env=env, text=True, stdin=subprocess.DEVNULL)
            finally:
                releaser.join()
            self.assertEqual(result.returncode, 0)
            self.assertTrue(result.admitted)
            self.assertGreaterEqual(result.admission_seconds, 3)

    def test_admitted_timeout_allows_the_owned_supervisor_to_close_a_detached_browser(self):
        import types
        spec = importlib.util.spec_from_file_location("autocheck_under_test", AUTOCHECK / "autocheck.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as scratch:
            workers = Path(scratch) / "workers"
            subprocess.run([sys.executable, str(ENGINE / "scripts/lib/worker_tokens.py"), "init",
                            str(workers), str(max(1, int((os.cpu_count() or 4) * 0.8)))], check=True)
            env = {k: v for k, v in os.environ.items() if not k.startswith("RICHOS_WORKER_")}
            env["RICHOS_MACHINE_WORKERS"] = str(workers)
            repo = types.SimpleNamespace(top=HERE.parents[2], env=env)
            marker = str(Path(scratch) / "browser.pid")
            suite = self.SUITE.replace("subprocess.Popen(['sleep', '300'])",
                                      "subprocess.Popen(['sleep', '300'], start_new_session=True)")
            with self.assertRaises(subprocess.TimeoutExpired):
                # load-bound: stimulus for a blocked fixture; pid exit and its trap are the verdict.
                module.run_admitted(repo, [sys.executable, "-c", suite, marker, "polite"], timeout=2,
                                    env=env, text=True, stdin=subprocess.DEVNULL)
            pid = int(Path(marker).read_text())
            self.assertTrue(Path(marker + ".term").exists(), "the suite never got to close its browser")
            alive = self.alive(pid)
            if alive:
                os.kill(pid, 9)
            self.assertFalse(alive, "the detached browser outlived the admitted check")

    def test_a_suite_that_closes_its_browser_on_sigterm_is_given_the_chance(self):
        alive, termed = self.run_late("polite", 3)
        self.assertTrue(termed, "the suite was never sent SIGTERM")
        self.assertFalse(alive, "the browser outlived its suite")

    def test_a_suite_that_ignores_sigterm_still_leaves_no_browser(self):
        alive, _termed = self.run_late("stubborn", 1)
        self.assertFalse(alive, "the browser outlived its suite")

    WRAPPED_SUITE = (
        "import os, signal, subprocess, sys, time\n"
        "browser = subprocess.Popen(['sleep', '300'], start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)\n"  # own process group, as Playwright's
        "open(sys.argv[1], 'w').write(str(browser.pid))\n"
        "def bye(*_):\n"  # a suite that needs a moment to close its browser
        "    time.sleep(1); browser.terminate(); os._exit(0)\n"
        "signal.signal(signal.SIGTERM, bye)\n"
        "time.sleep(300)\n"
    )

    def test_a_shell_wrapper_that_dies_on_sigterm_does_not_cut_the_grace_short(self):
        spec = importlib.util.spec_from_file_location("autocheck_under_test", AUTOCHECK / "autocheck.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as top:
            marker = os.path.join(top, "browser.pid")
            argv = ["bash", "-c", '"$@"; :', "wrapper", sys.executable, "-c", self.WRAPPED_SUITE, marker]
            with self.assertRaises(subprocess.TimeoutExpired):
                # load-bound: the 2 s limit is the stimulus (the suite sleeps 300 s); no verdict depends on host speed
                module.run_bounded(argv, timeout=2, grace=10, text=True)
            pid = int(Path(marker).read_text())
            __import__("time").sleep(0.2)
            alive = self.alive(pid)
            if alive:
                os.kill(pid, 9)
            self.assertFalse(alive, "the browser outlived its suite: the wrapper's exit cut the grace short")

    def load(self):
        spec = importlib.util.spec_from_file_location("autocheck_under_test", AUTOCHECK / "autocheck.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_a_process_in_the_group_gets_exactly_one_sigterm(self):
        # Counted at the send, not at the receiver: two SIGTERMs sent back to back coalesce in a Python
        # handler, so a receiver-side count would be red only some of the time.
        import signal
        from unittest import mock
        module = self.load()
        sent = []
        real_kill, real_killpg = os.kill, os.killpg

        def kill(pid, sig):
            if sig == signal.SIGTERM:
                sent.append(("pid", pid, os.getpgid(pid)))
            return real_kill(pid, sig)

        def killpg(group, sig):
            if sig == signal.SIGTERM:
                sent.append(("group", group, group))
            return real_killpg(group, sig)

        stays = "import signal, time; signal.signal(signal.SIGTERM, lambda *_: None); time.sleep(300)"  # outlives the first SIGTERM
        argv = ["bash", "-c", '"$@"; :', "wrapper", sys.executable, "-c", stays]
        with mock.patch.object(module.os, "kill", kill), mock.patch.object(module.os, "killpg", killpg):
            with self.assertRaises(subprocess.TimeoutExpired):
                # load-bound: the 2 s limit is the stimulus (the suite sleeps 300 s)
                module.run_bounded(argv, timeout=2, grace=1, text=True)
        self.assertEqual([entry[0] for entry in sent], ["group"],
                         f"every process here is in the group, so each must get SIGTERM once, from the group alone: {sent}")

    def test_a_failed_process_listing_is_reported_not_read_as_no_descendants(self):
        import contextlib
        import io
        from unittest import mock
        module = self.load()
        with mock.patch.object(module.subprocess, "run", side_effect=OSError("no ps")):
            self.assertIsNone(module._descendants(os.getpid()))
            seen = io.StringIO()
            with contextlib.redirect_stderr(seen):
                with self.assertRaises(subprocess.TimeoutExpired):
                    # load-bound: the 1 s limit is the stimulus (sleep 300 never finishes)
                    module.run_bounded(["sleep", "300"], timeout=1, grace=1, text=True)
        self.assertIn("could not list", seen.getvalue())


if __name__ == "__main__":
    result = unittest.main(exit=False, verbosity=2).result
    sys.exit(0 if result.wasSuccessful() else 1)
