#!/usr/bin/env python3
"""guard-ci-turn-gate.rows.test.py - one test per hunt row P3-13 .. P3-18, P3-34.

Each test is red on the code before the fix and green after. Run:
    python3 guard-ci-turn-gate.rows.test.py
"""
import importlib.util
import json
import os
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
               "message": {"content": [{"type": "tool_use", "name": "Bash",
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


if __name__ == "__main__":
    unittest.main()
