#!/usr/bin/env python3
"""inflight.assess must not turn a question git did not answer into a clean answer.

Hunt P5-08 (part 5): a failed `git worktree list` became "no peers", and a
failed merge-base became CLEAN-NOT-BEHIND. Both are now an `error`, which
inflight.py main() exits 2 on and guard-inflight-notify.sh refuses on
(fail-closed). Each case plants a REAL repository and a real linked worktree
cut from an older base, then makes exactly one git question fail.

The control (case 1) is the same repository with nothing failing: the teammate
is OWED-NO-NOTICE and blocking, so the failing cases are not silent because
the fixture was silent.
"""
import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("inflight", os.path.join(HERE, "inflight.py"))
inflight = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inflight)


def run(*args, cwd=None):
    subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True)


class InflightGaps(unittest.TestCase):
    def setUp(self):
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="inflight-gaps."))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.repo = os.path.join(self.tmp, "repo")
        os.makedirs(os.path.join(self.repo, "src"))
        patcher = mock.patch.dict(os.environ, {
            "RICHOS_INFLIGHT_ACK_LEDGER": os.path.join(self.tmp, "acks.jsonl")})
        patcher.start()
        self.addCleanup(patcher.stop)
        run("git", "init", "-q", "-b", "main", self.repo)
        # Inherit the operator's identity rather than inventing one: this machine
        # runs a global pre-commit identity guard (inflight-notify.test.sh does
        # the same), and a hard-coded address fails on the guard, not the code.
        for key, fallback in (("user.email", "tester@example.invalid"), ("user.name", "tester")):
            got = subprocess.run(["git", "config", key], capture_output=True, text=True).stdout.strip()
            run("git", "config", key, got or fallback, cwd=self.repo)
        with open(os.path.join(self.repo, "src", "a.txt"), "w") as fh:
            fh.write("one\n")
        run("git", "add", "-A", cwd=self.repo)
        run("git", "commit", "-q", "-m", "base", cwd=self.repo)
        self.wt = os.path.join(self.tmp, "wt", "norm-sonnet-x1")
        os.makedirs(os.path.dirname(self.wt))
        run("git", "worktree", "add", "-q", "-b", "worktree-norm", self.wt, cwd=self.repo)
        with open(os.path.join(self.wt, "src", "b.txt"), "w") as fh:
            fh.write("teammate\n")
        run("git", "add", "-A", cwd=self.wt)
        run("git", "commit", "-q", "-m", "teammate", cwd=self.wt)
        with open(os.path.join(self.repo, "src", "a.txt"), "a") as fh:
            fh.write("two\n")
        run("git", "add", "-A", cwd=self.repo)
        run("git", "commit", "-q", "-m", "a land that moves main", cwd=self.repo)
        self.tip = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo, check=True,
                                  capture_output=True, text=True).stdout.strip()
        self.teams = os.path.join(self.tmp, "teams")
        os.makedirs(self.teams)

    def assess(self):
        return inflight.assess(self.repo, tip=self.tip, teams_dir=self.teams)

    def test_1_control_the_fixture_does_see_a_debt(self):
        res = self.assess()
        self.assertFalse(res.get("error"), res.get("error"))
        self.assertEqual([w["verdict"] for w in res["worktrees"]], ["OWED-NO-NOTICE"])
        self.assertEqual(len(res["blocking"]), 1)

    def test_2_a_failed_worktree_inventory_is_an_error_not_no_peers(self):
        real = getattr(inflight, "git_checked", None)

        def fail_inventory(repo, *args):
            if args[:2] == ("worktree", "list"):
                return False, ""
            return real(repo, *args) if real else (True, "")

        with mock.patch.object(inflight, "git_checked", fail_inventory, create=True):
            res = self.assess()
        self.assertIn("inventory", res.get("error", ""))
        self.assertIn("UNKNOWN", res["error"])
        self.assertEqual(res["blocking"], [])            # nothing was learned, and it says so
        with mock.patch.object(inflight, "git_checked", fail_inventory, create=True):
            self.assertEqual(inflight.main(["--repo", self.repo, "--tip", self.tip,
                                            "--teams-dir", self.teams]), 2)

    def test_3_a_failed_merge_base_is_an_error_not_clean_not_behind(self):
        real = getattr(inflight, "git_checked", None)

        def fail_merge_base(repo, *args):
            if args[:1] == ("merge-base",):
                return False, ""
            return real(repo, *args) if real else (True, "")

        with mock.patch.object(inflight, "git_checked", fail_merge_base, create=True):
            res = self.assess()
        self.assertIn("merge-base", res.get("error", ""))
        self.assertEqual([w["verdict"] for w in res["worktrees"]], ["UNVERIFIED"])
        self.assertNotIn("CLEAN-NOT-BEHIND", [w["verdict"] for w in res["worktrees"]])

    def test_4_a_failed_moved_path_listing_is_an_error(self):
        real = getattr(inflight, "git_checked", None)

        def fail_diff(repo, *args):
            if args[:1] == ("diff",):
                return False, ""
            return real(repo, *args) if real else (True, "")

        with mock.patch.object(inflight, "git_checked", fail_diff, create=True):
            res = self.assess()
        self.assertTrue(res.get("error"))
        self.assertEqual([w["verdict"] for w in res["worktrees"]], ["UNVERIFIED"])

    def test_5_an_ancestry_question_git_cannot_answer_is_an_error(self):
        with mock.patch.object(inflight, "ancestor_state", lambda *a: None, create=True):
            res = self.assess()
        self.assertIn("could not say", res.get("error", ""))

    def test_6_a_gap_on_a_finished_teammate_does_not_refuse_everything(self):
        # The worktree directory is gone -> NOT-LIVE whatever git says about it.
        shutil.rmtree(self.wt)
        with mock.patch.object(inflight, "ancestor_state", lambda *a: None, create=True):
            res = self.assess()
        self.assertFalse(res.get("error"), res.get("error"))
        self.assertEqual([w["verdict"] for w in res["worktrees"]], ["NOT-LIVE"])

    def test_7_answers_about_two_commits_are_asked_of_git_once(self):
        # 2026-10-22: the Stop notice asked git the same questions about every worktree at
        # every turn end (16.4 CPU-s a turn over richos's 50 worktrees). An answer about two
        # full commit ids never changes, so the next assessment reuses it and decides the same.
        first = self.assess()
        asked = []
        real_checked, real_ancestor = inflight.git_checked, inflight.ancestor_state

        def checked(repo, *args):
            asked.append(args[0])
            return real_checked(repo, *args)

        def ancestor(repo, a, b):
            asked.append("is-ancestor")
            return real_ancestor(repo, a, b)

        with mock.patch.object(inflight, "git_checked", checked), \
                mock.patch.object(inflight, "ancestor_state", ancestor):
            second = self.assess()
        self.assertEqual([a for a in asked if a != "worktree"], [], "the same questions were asked again")
        keys = ("verdict", "landed", "behind", "base", "moved_shas", "moved_paths", "own_paths", "overlap")
        self.assertEqual([{k: w[k] for k in keys} for w in second["worktrees"]],
                         [{k: w[k] for k in keys} for w in first["worktrees"]])
        self.assertEqual(second["blocking"], first["blocking"])

    def test_8_a_question_git_could_not_answer_is_asked_again(self):
        with mock.patch.object(inflight, "ancestor_state", lambda *a: None, create=True):
            res = self.assess()
        self.assertIn("could not say", res.get("error", ""))
        res = self.assess()
        self.assertFalse(res.get("error"), res.get("error"))
        self.assertEqual([w["verdict"] for w in res["worktrees"]], ["OWED-NO-NOTICE"])

    def test_9_a_worktree_that_moves_is_asked_about_anew(self):
        self.assertEqual([w["verdict"] for w in self.assess()["worktrees"]], ["OWED-NO-NOTICE"])
        run("git", "merge", "-q", "--no-edit", "main", cwd=self.wt)
        self.assertEqual([w["verdict"] for w in self.assess()["worktrees"]], ["CLEAN-NOT-BEHIND"])

    # 2026-10-24: the Stop notice named the nightly's frozen `source`, detached release checks,
    # codex/ branches and the lead's own checkouts as NEVER TOLD and told the lead to message or
    # stop them. A checkout that resolves to no teammate is not a teammate. The hook names exactly
    # these verdicts, so a checkout carrying none of them is never named.
    NAMED = ("OWED-NO-NOTICE", "NOTIFIED-NO-ACK", "NOTIFIED-ACK-INVALID")

    def checkout(self, path, *branch):
        """A checkout cut from the base with a commit of its own, so main moved under it."""
        os.makedirs(os.path.dirname(path), exist_ok=True)
        run("git", "worktree", "add", "-q", *branch, path, "HEAD~1", cwd=self.repo)
        with open(os.path.join(path, "src", "own.txt"), "w") as fh:
            fh.write(path + "\n")
        run("git", "add", "-A", cwd=path)
        run("git", "commit", "-q", "-m", "own work", cwd=path)
        return path

    def native(self, agent_id):
        path = self.checkout(os.path.join(self.repo, ".claude", "worktrees", "agent-" + agent_id),
                             "-b", "worktree-" + agent_id)
        run("git", "worktree", "lock", "--reason", "agent running (pid %d)" % os.getpid(), path,
            cwd=self.repo)
        return path

    def verdicts(self):
        return {os.path.basename(w["path"]): w["verdict"] for w in self.assess()["worktrees"]}

    def test_10_a_checkout_that_is_no_teammate_is_never_named(self):
        self.checkout(os.path.join(self.tmp, "nightly", "source"), "--detach")
        self.checkout(os.path.join(self.tmp, "lead", "richos-hq"), "-b", "codex/owned-outcome-prd")
        res = self.assess()
        got = self.verdicts()
        for base in ("source", "richos-hq"):
            self.assertNotIn(got[base], self.NAMED, got)
        self.assertEqual(res["blocking"], [self.wt])     # the real teammate is still owed
        self.assertFalse(res.get("error"), res.get("error"))

    def test_11_a_teammate_named_checkout_never_told_is_still_named(self):
        self.checkout(os.path.join(self.tmp, "wt", "zach-opus-u1"), "-b", "worktree-zach-u1")
        got = self.verdicts()
        self.assertEqual(got["zach-opus-u1"], "OWED-NO-NOTICE")
        self.assertEqual(got["norm-sonnet-x1"], "OWED-NO-NOTICE")

    def test_12_a_native_worktree_whose_name_did_not_resolve_is_still_named(self):
        self.native("a5a1c0ffee1122334")
        res = self.assess()
        wt = [w for w in res["worktrees"] if w["kind"] == "native"]
        self.assertEqual([(w["resolved_name"], w["verdict"]) for w in wt], [("", "OWED-NO-NOTICE")])

    def test_13_a_registered_workspace_under_another_directory_name_is_still_named(self):
        # create-teammate-worktree.sh --dir may put a workspace anywhere; its branch is always
        # cc/<teammate name>, minted at registration.
        self.checkout(os.path.join(self.tmp, "elsewhere", "richos"), "-b", "cc/zach-opus-q1")
        res = self.assess()
        wt = [w for w in res["worktrees"] if w["branch"] == "cc/zach-opus-q1"]
        self.assertEqual([w["verdict"] for w in wt], ["OWED-NO-NOTICE"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
