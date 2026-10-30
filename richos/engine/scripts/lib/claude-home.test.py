#!/usr/bin/env python3
"""claude-home.test.py — ONE Claude home for the land lease, the fence settings and the review
records, the same for the terminal and the app (operator-fences.test.sh F36).

2026-10-28 (richos-hq follow-ups, "from the app, no worker can get a workspace in richos" and
"the usage hold reads the wrong Claude account"): on an added Claude account the app points a
lease's CLAUDE_CONFIG_DIR at that account's folder (claude-accounts/2), and every engine store
derived from that variable split by account. A Work lease's land lease resolved to
claude-accounts/2/state/land-locks while the installed fence read ~/.claude/state/land-locks (G10
refused it), Rich's `operator-fences.sh off` record went to claude-accounts/2, and a passing second
review written there was "no second review" to the land. Rich's decision (23:58Z): one machine-wide
home for the fence lease, the fence settings and the review records, the same for terminal and app.

Every case runs the engine's own code in a separate process with exactly the environment a
terminal or an app lease has (EngineProfile.configure: CLAUDE_CONFIG_DIR and
RICHOS_CLAUDE_ACCOUNT_DIR at the account's folder, RICHOS_CLAUDE_HOME at Account 1's), in a
sandbox HOME. Nothing here reads or writes the operator's own folders.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ENGINE = os.path.realpath(os.path.join(HERE, "..", ".."))

# One process of the engine: the paths each store resolves to, a write to each, a read of each,
# or the land lock held / tried. Each value is computed on its own, so a store that cannot be
# resolved reports why instead of hiding the others.
PROBE = r'''
import fcntl, json, os, sys
lib, engine, repo, action, tag = sys.argv[1:6]
sys.path.insert(0, lib)
sys.path.insert(1, os.path.join(engine, "mega-lander"))
import operator_fences as F
import operator_fences_admin as A
import second_review as SR
import workspaces as W

def each(**probes):
    out = {}
    for name, fn in probes.items():
        try:
            out[name] = fn()
        except Exception as error:      # reported, so the assertion names it
            out[name] = "ERROR %s: %s" % (type(error).__name__, error)
    return out

def settings_rows():
    try:
        with open(A.settings_log_path(), encoding="utf-8") as f:
            return [json.loads(line).get("by") for line in f if line.strip()]
    except FileNotFoundError:
        return []

if action == "paths":
    out = each(lease=lambda: F.keyed_paths(repo)[0],
               land_lock=lambda: os.path.dirname(W.land_lock_path(repo)),
               settings=lambda: A.settings_log_path(),
               registry=lambda: A.registry_path(),
               review_writer=lambda: SR.ledger_path(),
               review_fence=lambda: F.review_ledger_default())
elif action == "write":
    out = each(land=lambda: W.append_land_record(repo, {"commit": tag}),
               settings=lambda: F.append_jsonl(A.settings_log_path(),
                                               {"repository": repo, "value": "off", "by": tag}) or "ok",
               review=lambda: SR.append_row({"id": "rv-" + tag, "tip": tag, "verdict": "passed",
                                             "finished_at": "2026-10-29T00:00:00Z"}) or "ok")
elif action == "read":
    out = each(lands=lambda: sorted(r.get("commit") for r in W.land_records(repo)),
               settings=settings_rows,
               reviews=lambda: sorted(W._reviews_by_tip()))
elif action == "hold":
    fd = os.open(W.land_lock_path(repo), os.O_RDWR | os.O_CREAT, 0o600)
    fcntl.flock(fd, fcntl.LOCK_EX)
    print(json.dumps({"held": W.land_lock_path(repo)}), flush=True)
    sys.stdin.read()                    # held until the test closes our stdin
    out = {}
elif action == "try":
    fd = os.open(W.land_lock_path(repo), os.O_RDWR | os.O_CREAT, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        out = {"blocked": False, "path": W.land_lock_path(repo)}
    except BlockingIOError:
        out = {"blocked": True, "path": W.land_lock_path(repo)}
print(json.dumps(out))
'''

# Every variable that can move an engine store: none is inherited from whoever runs this test.
MOVERS = ("CLAUDE_CONFIG_DIR", "RICHOS_CLAUDE_HOME", "RICHOS_CLAUDE_ACCOUNT_DIR", "RICHOS_APP_STATE",
          "RICHOS_WORKSPACES_DIR", "RICHOS_LAND_LOCKS_DIR", "SECOND_REVIEW_STATE_DIR",
          "RICHOS_SESSIONS_DIR", "RICHOS_PROJECTS_DIR", "RICHOS_MACHINE_SETTINGS_RESOLVED")


class OneClaudeHome(unittest.TestCase):

    def setUp(self):
        self.root = os.path.realpath(tempfile.mkdtemp(prefix="claude-home-"))
        self.addCleanup(shutil.rmtree, self.root, True)
        self.home = os.path.join(self.root, "home")
        self.machine = os.path.join(self.home, ".claude")          # Account 1: the terminal's folder
        self.data = os.path.join(self.home, "Library", "Application Support", "com.richos.app")
        self.app_state = os.path.join(self.data, "engine-state")
        for d in (self.machine, self.app_state):
            os.makedirs(d)
        self.repo = os.path.join(self.root, "repo")
        subprocess.run(["git", "init", "-q", self.repo], check=True, env=self.base_env())
        self.want_locks = os.path.join(os.path.realpath(self.machine), "state", "land-locks")
        self.want_reviews = os.path.join(self.machine, "state", "reviews.jsonl")

    def account(self, name):
        folder = os.path.join(self.data, "claude-accounts", name)
        os.makedirs(folder, exist_ok=True)
        return folder

    def base_env(self):
        env = {k: v for k, v in os.environ.items() if k not in MOVERS and not k.startswith("GIT_")}
        env.update(HOME=getattr(self, "home", self.root), GIT_CONFIG_NOSYSTEM="1",
                   GIT_CONFIG_GLOBAL=os.devnull, RICHOS_MACHINE_SETTINGS_RESOLVED="1")
        return env

    def terminal(self):
        """A terminal: none of the app's variables, no CLAUDE_CONFIG_DIR (the operator's setup)."""
        return self.base_env()

    def app_lease(self, folder):
        """What EngineProfile.configure gives a lease: on an added account (folder) its
        CLAUDE_CONFIG_DIR and RICHOS_CLAUDE_ACCOUNT_DIR; on Account 1 (None) neither. Always
        RICHOS_CLAUDE_HOME (Account 1's folder) and the conversation's own partitions."""
        env = self.base_env()
        env.update(RICHOS_CLAUDE_HOME=self.machine, RICHOS_APP_STATE=self.app_state,
                   RICHOS_WORKSPACES_DIR=os.path.join(self.app_state, "workspaces", "thread-fixture"))
        if folder:
            env.update(CLAUDE_CONFIG_DIR=folder, RICHOS_CLAUDE_ACCOUNT_DIR=folder)
        return env

    def run_probe(self, env, action, tag="-"):
        r = subprocess.run([sys.executable, "-c", PROBE, HERE, ENGINE, self.repo, action, tag],
                           env=env, capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 0, "probe %s failed: %s %s" % (action, r.stdout, r.stderr))
        return json.loads(r.stdout.strip().splitlines()[-1])

    def leases(self):
        """Every lease the app can give: Account 1, two added accounts, and an added account
        whose folder lives anywhere at all (whatever CLAUDE_CONFIG_DIR says)."""
        elsewhere = os.path.join(self.root, "any", "folder")
        os.makedirs(elsewhere)
        return [("Account 1", self.app_lease(None)),
                ("Account 2", self.app_lease(self.account("2"))),
                ("Account 3", self.app_lease(self.account("3"))),
                ("an account folder elsewhere", self.app_lease(elsewhere))]

    # ---- one test per home ---------------------------------------------------------------------

    def test_the_fence_lease_resolves_to_the_machine_wide_home(self):
        for name, env in self.leases():
            got = self.run_probe(env, "paths")
            self.assertEqual(got["lease"], self.want_locks, "%s: the fence lease's home" % name)
            self.assertEqual(got["land_lock"], self.want_locks, "%s: the land lock's home" % name)

    def test_the_fence_settings_file_resolves_to_the_machine_wide_home(self):
        resolved = [(name, self.run_probe(env, "paths")) for name, env in self.leases()]
        for name, got in resolved:          # the installed repositories and their switches
            self.assertEqual(got["registry"], os.path.join(self.want_locks, "operator-fences.json"),
                             "%s: the fence registry" % name)
        for name, got in resolved:          # the record every `off` appends (operator_fences_admin)
            self.assertEqual(got["settings"], os.path.join(self.want_locks, "operator-fence-settings.jsonl"),
                             "%s: the fence settings record" % name)

    def test_the_second_review_records_resolve_to_the_machine_wide_home(self):
        for name, env in self.leases():
            got = self.run_probe(env, "paths")
            self.assertEqual(got["review_writer"], self.want_reviews, "%s: where second-review writes" % name)
            self.assertEqual(got["review_fence"], self.want_reviews, "%s: where the land fence reads" % name)

    # ---- the terminal and the app ------------------------------------------------------------

    def test_the_terminal_and_the_app_read_and_write_the_same_home(self):
        app = self.app_lease(self.account("2"))
        term = self.terminal()
        # Both resolve every store to the same place.
        self.assertEqual(self.run_probe(term, "paths"), self.run_probe(app, "paths"))
        # What one writes, the other reads, both ways.
        for tag, env in (("t" * 40, term), ("a" * 40, app)):
            wrote = self.run_probe(env, "write", tag)
            self.assertTrue(str(wrote["land"]).startswith(self.want_locks + os.sep), wrote)
            self.assertEqual((wrote["settings"], wrote["review"]), ("ok", "ok"), wrote)
        for name, env in (("terminal", term), ("app", app)):
            got = self.run_probe(env, "read")
            self.assertEqual(got["lands"], ["a" * 40, "t" * 40], "%s: land records" % name)
            self.assertEqual(sorted(got["settings"]), ["a" * 40, "t" * 40], "%s: fence settings" % name)
            self.assertEqual(got["reviews"], ["a" * 40, "t" * 40], "%s: review records" % name)
        account = os.path.join(self.data, "claude-accounts", "2")
        self.assertEqual(sorted(os.listdir(account)), [], "an engine store was written in the account's folder")

    def test_the_terminal_and_the_app_take_one_lease(self):
        app = self.app_lease(self.account("2"))
        for holder, other in ((self.terminal(), app), (app, self.terminal())):
            hold = subprocess.Popen([sys.executable, "-c", PROBE, HERE, ENGINE, self.repo, "hold", "-"],
                                    env=holder, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
            try:
                held = json.loads(hold.stdout.readline())["held"]
                tried = self.run_probe(other, "try")
                self.assertEqual(tried["path"], held)
                self.assertTrue(tried["blocked"], "the other side took a land lock while this one held it")
            finally:
                hold.stdin.close()
                hold.wait(timeout=30)
                hold.stdout.close()

    # ---- what must not move ------------------------------------------------------------------

    def test_a_sandbox_inside_an_app_lease_keeps_its_own_home(self):
        """A test run from an app shell inherits the lease's pair and sets its own
        CLAUDE_CONFIG_DIR (global-state-witness.sh's advice): its stores are its own, never the
        live ones."""
        sandbox = os.path.join(self.root, "sandbox-config")
        env = dict(self.app_lease(self.account("2")), CLAUDE_CONFIG_DIR=sandbox)
        got = self.run_probe(env, "paths")
        locks = os.path.join(os.path.realpath(sandbox), "state", "land-locks")
        self.assertEqual(got["lease"], locks)
        self.assertEqual(got["registry"], os.path.join(locks, "operator-fences.json"))
        self.assertEqual(got["review_writer"], os.path.join(sandbox, "state", "reviews.jsonl"))
        self.assertEqual(got["review_fence"], os.path.join(sandbox, "state", "reviews.jsonl"))

    def test_a_terminal_that_sets_its_own_config_folder_resolves_as_before(self):
        own = os.path.join(self.root, "terminal-config")
        got = self.run_probe(dict(self.terminal(), CLAUDE_CONFIG_DIR=own), "paths")
        self.assertEqual(got["lease"], os.path.join(os.path.realpath(own), "state", "land-locks"))
        self.assertEqual(got["review_writer"], os.path.join(own, "state", "reviews.jsonl"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
