#!/usr/bin/env python3
"""guard-ci-turn-gate.rows.test.py - one test per hunt row P3-13 .. P3-18, P3-34.

Each test is red on the code before the fix and green after. Run:
    python3 guard-ci-turn-gate.rows.test.py
"""
import importlib.util
import json
import os
import subprocess
from unittest.mock import patch
import sys
import tempfile
import time
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("gate", os.path.join(HERE, "guard-ci-turn-gate.py"))
gate = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gate)


def push_cmd(cmd):
    return gate.parse_pushes(cmd, "/tmp")


class Rows(unittest.TestCase):
    def test_p3_34_wrapper_option_values(self):
        for cmd in ("env -u UNUSED git push origin main", "sudo -u root git push origin main"):
            got = push_cmd(cmd)
            self.assertEqual([p["branch"] for p in got], ["main"], cmd)

    def test_unreadable_words_are_no_repository_or_branch(self):
        for cmd in ("cd $R && git push origin main", "git -C $R push origin main",
                    "git push origin $B", "git push $REMOTE main"):
            self.assertEqual(push_cmd(cmd), [], cmd)

    def test_redirects_are_not_branch_names(self):
        for cmd in ("git push origin main 2>&1", "git push origin main >/dev/null 2>&1",
                    "git push origin main > /tmp/out", "git push origin main &>/dev/null"):
            self.assertEqual([p["branch"] for p in push_cmd(cmd)], ["main"], cmd)
        self.assertEqual([p["branch"] for p in push_cmd("git push 2>&1")], [""])

    def test_p3_16_every_pushed_ref(self):
        got = push_cmd("git push origin main release")
        self.assertEqual(sorted(p["branch"] for p in got), ["main", "release"])

    def test_p3_18_half_written_line_is_not_skipped(self):
        rec = {"cwd": "/tmp", "timestamp": "2026-10-02T00:00:00Z",
               "message": {"content": [{"type": "tool_use", "name": "Bash", "id": "half",
                                        "input": {"command": "git push origin main"}}]}}
        line = json.dumps(rec) + "\n"
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "t.jsonl")
            with open(path, "w") as fh:
                fh.write(line[:40])                      # half a record, no newline
            state = {}
            seen, _ = gate.observe_pushes(path, state, gate.Budget(5))
            self.assertEqual(seen, [])
            with open(path, "w") as fh:
                fh.write(line)                           # the rest arrives
                fh.write(json.dumps({"message": {"content": [
                    {"type": "tool_result", "tool_use_id": "half", "content": "ok"}]}}) + "\n")
            seen, _ = gate.observe_pushes(path, state, gate.Budget(5))
            self.assertEqual([p["branch"] for p in seen], ["main"])

    def test_p3_19_v3_refused_push_is_not_an_obligation(self):
        # A Bash push the host refused (its result is an error) never ran.
        recs = [{"cwd": "/tmp", "message": {"content": [
                    {"type": "tool_use", "name": "Bash", "id": "b",
                     "input": {"command": "git push origin main"}}]}},
                {"type": "user", "message": {"content": [
                    {"type": "tool_result", "tool_use_id": "b", "is_error": True,
                     "content": "PreToolUse refused; command never ran"}]}},
                {"cwd": "/tmp", "message": {"content": [
                    {"type": "tool_use", "name": "Bash", "id": "c",
                     "input": {"command": "git push origin release"}}]}},
                {"type": "user", "message": {"content": [
                    {"type": "tool_result", "tool_use_id": "c", "content": "ok"}]}}]
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "t.jsonl")
            with open(path, "w") as fh:
                fh.write("".join(json.dumps(r) + "\n" for r in recs))
            seen, why = gate.observe_pushes(path, {}, gate.Budget(5))
            self.assertEqual(why, "")
            self.assertEqual([p["branch"] for p in seen], ["release"])

    def test_p3_14_v3_skipped_only_runs_are_not_green(self):
        def answer(conclusions):
            body = {"workflow_runs": [{"path": ".github/workflows/w%d.yml" % i, "name": "w",
                                       "status": "completed", "conclusion": c, "id": i}
                                      for i, c in enumerate(conclusions)]}
            return lambda *_a, **_k: (0, json.dumps(body), "")
        real = gate.run
        try:
            gate.run = answer(["skipped"])
            self.assertEqual(gate.probe_runs("o/r", "abc", gate.Budget(5))["state"], "none")
            gate.run = answer(["skipped", "success"])
            self.assertEqual(gate.probe_runs("o/r", "abc", gate.Budget(5))["state"], "green")
            gate.run = answer(["skipped", "failure"])
            self.assertEqual(gate.probe_runs("o/r", "abc", gate.Budget(5))["state"], "red")
        finally:
            gate.run = real

    def test_p3_15_green_expires(self):
        old = {"state": "green", "age_seconds": 24 * 3600}
        self.assertFalse(gate.cache_is_usable(old))
        self.assertTrue(gate.cache_is_usable({"state": "green", "age_seconds": 1}))

    def test_p3_13_timeout_keeps_the_obligation(self):
        with tempfile.TemporaryDirectory() as d:
            state_dir = os.path.join(d, "state")
            gate.STATE_DIR = state_dir
            repo = os.path.join(d, "repo")
            os.makedirs(repo)
            sid = "s13"
            key = "%s\torigin\tmain" % repo
            gate.save_state(sid, {"pushes": {key: {"dir": repo, "remote": "origin",
                                                   "branch": "main", "at": time.time()}},
                                  "transcript": {}})
            budget = gate.Budget(5)
            budget.seconds = 0.0                         # every command is "out of time"
            payload = {"session_id": sid, "transcript_path": "", "cwd": repo}
            gate.evaluate(payload, budget)
            self.assertIn(key, gate.load_state(sid).get("pushes", {}))

    def test_p3_13_v3_branch_lookup_timeout_keeps_the_obligation(self):
        # An omitted-refspec push records no branch; the branch is read at
        # judgment time. A lookup that runs out of budget is transient and
        # must not drop the push as unknowable (v3 re-check).
        with tempfile.TemporaryDirectory() as d:
            gate.STATE_DIR = os.path.join(d, "state")
            repo = os.path.join(d, "repo")
            os.makedirs(repo)
            sid = "s13v3"
            key = "%s\torigin\t" % repo
            gate.save_state(sid, {"pushes": {key: {"dir": repo, "remote": "origin",
                                                   "branch": "", "at": 1}},
                                  "transcript": {}})
            real = gate.repo_facts
            gate.repo_facts = lambda *_a: (repo, "fixture/repo", "")
            try:
                budget = gate.Budget(5)
                budget.seconds = 0.0
                gate.evaluate({"session_id": sid, "transcript_path": "", "cwd": repo}, budget)
            finally:
                gate.repo_facts = real
            self.assertIn(key, gate.load_state(sid).get("pushes", {}))

    def test_p3_17_older_obligation_is_not_starved(self):
        with tempfile.TemporaryDirectory() as d:
            gate.STATE_DIR = os.path.join(d, "state")
            sid = "s17"
            now = time.time()
            pushes = {}
            for n in range(gate.MAX_TARGETS + 1):
                key = "/nonexistent/%d\torigin\tmain" % n
                pushes[key] = {"dir": "/nonexistent/%d" % n, "remote": "origin",
                               "branch": "main", "at": now - 1000 + n}
            gate.save_state(sid, {"pushes": pushes, "transcript": {}})
            checked = []
            real = (gate.repo_facts, gate.head_of, gate.verdict_for)

            def spy(directory, remote, budget, cache):
                checked.append(directory)
                return (directory, "o/" + os.path.basename(directory), "")
            gate.repo_facts = spy
            gate.head_of = lambda root, remote, branch, budget: ("abc123", "")
            gate.verdict_for = lambda slug, sha, budget: {"state": "green"}
            try:
                for _ in range(2):
                    gate.evaluate({"session_id": sid, "transcript_path": "", "cwd": d}, gate.Budget(5))
            finally:
                gate.repo_facts, gate.head_of, gate.verdict_for = real
            self.assertIn("/nonexistent/0", checked)     # the OLDEST was reached by turn 2

    def test_p3_17_v3_a_selected_target_is_not_marked_judged(self):
        # Six targets; each turn's first CI lookup spends the whole budget, so
        # the other five run out of time. A target only SELECTED must keep its
        # place in the rotation: over six turns every one is actually read.
        with tempfile.TemporaryDirectory() as d:
            gate.STATE_DIR = os.path.join(d, "state")
            sid = "s17v3"
            pushes = {}
            for n in range(gate.MAX_TARGETS):
                key = "%s/%d\torigin\tmain" % (d, n)
                pushes[key] = {"dir": "%s/%d" % (d, n), "remote": "origin",
                               "branch": "main", "at": 100 + n}
            gate.save_state(sid, {"pushes": pushes, "transcript": {}})
            read, holder = [], []
            real = (gate.repo_facts, gate.head_of, gate.verdict_for)

            def facts(directory, *_a):
                if holder[0].spent():
                    return "", "", gate.TRANSIENT + "budget spent"
                return directory, "fixture/" + os.path.basename(directory), ""

            def verdict(slug, sha, budget):
                read.append(slug)
                budget.seconds = 0.0                     # a slow lookup eats the budget
                return {"state": "green", "runs": []}
            gate.repo_facts, gate.head_of, gate.verdict_for = (
                facts, lambda *_a: ("abc", ""), verdict)
            try:
                for _ in range(gate.MAX_TARGETS):
                    b = gate.Budget(5)
                    holder[:] = [b]
                    gate.evaluate({"session_id": sid, "transcript_path": "", "cwd": d}, b)
            finally:
                gate.repo_facts, gate.head_of, gate.verdict_for = real
            self.assertEqual(sorted(set(read)),
                             sorted("fixture/%d" % n for n in range(gate.MAX_TARGETS)))


class G3(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="g3-")
        self.addCleanup(self.tmp.cleanup)
        self.root = self.tmp.name
        self.transcript = os.path.join(self.root, "transcript.jsonl")
        self.enterContext(patch.object(gate, "STATE_DIR", os.path.join(self.root, "state")))
        self.enterContext(patch.object(gate, "pause_for", lambda *_: False))
        self.enterContext(patch.object(gate, "log_ack", lambda *_: None))
        self.payload = {"session_id": "g3", "transcript_path": self.transcript}

    def record(self, blocks, cwd=None):
        with open(self.transcript, "a") as f:
            f.write(json.dumps({"cwd": cwd or self.root, "timestamp": time.time(),
                                "message": {"content": blocks}}) + "\n")

    def call(self, cmd, cwd=None, ident="push"):
        self.record([{"type": "tool_use", "name": "Bash", "id": ident,
                      "input": {"command": cmd}}], cwd)

    def result(self, output="ok", error=False, ident="push"):
        self.record([{"type": "tool_result", "tool_use_id": ident,
                      "is_error": error, "content": output}])

    def git(self, directory, *args, check=True):
        return subprocess.run(["git", "-C", directory, *args], capture_output=True,
                              text=True, check=check, env={**os.environ,
                              "GIT_CONFIG_GLOBAL": os.devnull,
                              "GIT_CONFIG_NOSYSTEM": "1"})

    def repo(self):
        bare = os.path.join(self.root, "remote.git")
        repo = os.path.join(self.root, "repo")
        os.mkdir(bare)
        os.mkdir(repo)
        self.git(bare, "init", "--bare", "-q")
        self.git(repo, "init", "-q", "-b", "main")
        for key, value in (("user.name", "Fixture"), ("user.email", "f@example.invalid"),
                           ("core.hooksPath", os.devnull), ("commit.gpgsign", "false")):
            self.git(repo, "config", key, value)
        self.git(repo, "commit", "--allow-empty", "-qm", "initial")
        self.git(repo, "remote", "add", "origin", bare)
        self.enterContext(patch.object(gate, "repo_facts", lambda d, *_: (d, "fixture/repo", "")))
        return repo, bare

    def test_p3_19_refusal_across_scans_never_creates_obligation(self):
        repo, _ = self.repo()
        self.git(repo, "push", "origin", "main")
        self.call("git push origin main", repo)
        self.enterContext(patch.object(gate, "verdict_for", lambda *_: {"state": "red"}))
        self.assertEqual(gate.evaluate(self.payload, gate.Budget(5))[0], 0)
        self.result("PreToolUse refused; command never ran", True)
        self.assertEqual(gate.evaluate(self.payload, gate.Budget(5))[0], 0)
        self.assertFalse(gate.load_state("g3").get("pushes"))

    def test_p3_19_partial_push_keeps_only_successful_ref(self, quiet=False):
        repo, bare = self.repo()
        self.git(repo, "push", "origin", "main:release")
        self.git(repo, "checkout", "--orphan", "release")
        self.git(repo, "commit", "--allow-empty", "-qm", "unrelated")
        self.git(repo, "checkout", "main")
        options = ["-q"] if quiet else []
        self.call("git push " + " ".join(options + ["origin", "main", "release"]), repo)
        pushed = self.git(repo, "push", *options, "origin", "main", "release", check=False)
        self.assertNotEqual(pushed.returncode, 0)
        self.result(pushed.stdout + pushed.stderr, True)
        read = []
        self.enterContext(patch.object(gate, "verdict_for", lambda slug, sha, b:
                                      read.append(sha) or {"state": "red"}))
        self.assertEqual(gate.evaluate(self.payload, gate.Budget(5))[0], 2)
        own = self.git(bare, "rev-parse", "main").stdout.strip()
        self.assertEqual(read, [own])
        self.assertEqual([p["branch"] for p in gate.load_state("g3")["pushes"].values()], ["main"])

    def test_p3_19_quiet_partial_push_keeps_successful_ref(self):
        self.test_p3_19_partial_push_keeps_only_successful_ref(quiet=True)

    def test_successful_updates_include_porcelain_and_forced_pushes(self):
        output = (" + abc1234...def5678 topic -> topic (forced update)\n"
                  "*\trefs/heads/new:refs/heads/new\t[new branch]\n"
                  " \trefs/heads/main:refs/heads/main\tabc1234..def5678\n"
                  "!\trefs/heads/release:refs/heads/release\t[rejected]\n")
        self.assertEqual(gate.push_updates(output),
                         {"topic": "def5678", "new": "", "main": "def5678"})

    def test_noop_push_keeps_existing_failed_obligation(self):
        repo, _ = self.repo()
        self.call("git push origin main", repo)
        self.git(repo, "push", "origin", "main")
        self.result()
        self.enterContext(patch.object(gate, "verdict_for", lambda *_: {"state": "red"}))
        self.assertEqual(gate.evaluate(self.payload, gate.Budget(5))[0], 2)
        self.call("git push origin main", repo, "noop")
        self.result("Everything up-to-date", ident="noop")
        self.assertEqual(gate.evaluate(self.payload, gate.Budget(5))[0], 2)

    def test_p3_45_fetch_before_and_after_first_check_keeps_pushed_sha(self):
        repo, bare = self.repo()
        self.call("git push origin main", repo)
        self.git(repo, "push", "origin", "main")
        self.result()
        own = self.git(repo, "rev-parse", "HEAD").stdout.strip()
        other = os.path.join(self.root, "other")
        self.git(self.root, "clone", "-q", "-b", "main", bare, other)
        for key, value in (("user.name", "Fixture"), ("user.email", "f@example.invalid"),
                           ("core.hooksPath", os.devnull), ("commit.gpgsign", "false")):
            self.git(other, "config", key, value)
        self.git(other, "commit", "--allow-empty", "-qm", "other")
        self.git(other, "push", "origin", "main")
        self.git(repo, "fetch", "origin")
        read = []
        self.enterContext(patch.object(gate, "verdict_for", lambda slug, sha, b:
                                      read.append(sha) or {"state": "green" if sha == own else "red"}))
        for _ in range(2):
            self.assertEqual(gate.evaluate(self.payload, gate.Budget(5))[0], 0)
        self.assertEqual(read, [own, own])

    def test_p3_17_unknown_attempt_rotates_without_losing_obligations(self):
        pushes = {str(n): {"dir": str(n), "branch": "main", "sha": "abc", "at": n}
                  for n in range(gate.MAX_TARGETS)}
        gate.save_state("g3", {"pushes": pushes})
        self.enterContext(patch.object(gate, "repo_facts", lambda d, r, b, c:
                                      ("", "", gate.TRANSIENT + "budget spent") if b.left() <= 0.05
                                      else (d, "fixture/" + d, "")))
        self.enterContext(patch.object(gate, "head_of", lambda *_: ("abc", "")))
        read = []
        def unknown(slug, sha, budget):
            read.append(slug)
            # Leave less than run()'s admission floor. Untouched targets must
            # retain their place even though spent() is not quite true yet.
            budget.seconds = time.time() - budget.start + 0.01
            return {"state": "unknown"}
        self.enterContext(patch.object(gate, "verdict_for", unknown))
        for _ in pushes:
            gate.evaluate(self.payload, gate.Budget(5))
        self.assertEqual(set(read), {"fixture/" + str(n) for n in range(gate.MAX_TARGETS)})
        self.assertEqual(len(gate.load_state("g3")["pushes"]), len(pushes))

    def test_p3_50_only_live_waiver_clears_refusal(self):
        gate.save_state("g3", {"pushes": {"p": {"dir": self.root, "branch": "main", "sha": "abc"}}})
        self.enterContext(patch.object(gate, "repo_facts", lambda d, *_: (d, "fixture/repo", "")))
        self.enterContext(patch.object(gate, "head_of", lambda *_: ("abc", "")))
        self.enterContext(patch.object(gate, "verdict_for", lambda *_: {"state": "red"}))
        ack = "ci-red-ack: fixture/repo -- Credentials are unavailable until the operator restores access."
        for text in ("Example only:\n```text\n" + ack + "\n```", "> " + ack,
                     "Example:\n~~~~text\n" + ack + "\n~~~~", "    " + ack):
            self.payload["last_assistant_message"] = text
            self.assertEqual(gate.evaluate(self.payload, gate.Budget(5))[0], 2, text)
        self.payload["last_assistant_message"] = ack
        self.assertEqual(gate.evaluate(self.payload, gate.Budget(5))[0], 0)


if __name__ == "__main__":
    unittest.main()
