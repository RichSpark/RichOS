#!/usr/bin/env python3
"""Codex's finished worktrees close; everything else survives (cleanup PRD A6).

Real git repositories in a sandbox: a plain Codex root and an app-managed root
with its own attachment database. Nothing outside the sandbox is read or touched.
"""
import importlib.util
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location("codex_worktrees", HERE / "codex_worktrees.py")
cw = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cw)
spec = importlib.util.spec_from_file_location("cw_workspaces", HERE.parent.parent / "mega-lander" / "workspaces.py")
ws = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ws)

GIT_ENV = {"GIT_CONFIG_NOSYSTEM": "1", "GIT_AUTHOR_NAME": "Fixture", "GIT_AUTHOR_EMAIL": "f@example.invalid",
           "GIT_COMMITTER_NAME": "Fixture", "GIT_COMMITTER_EMAIL": "f@example.invalid"}


def git(cwd, *args):
    r = subprocess.run(["git", "-C", str(cwd)] + list(args), capture_output=True, text=True,
                       env=dict(os.environ, **GIT_ENV))
    if r.returncode != 0:
        raise AssertionError("git %s: %s" % (" ".join(args), r.stderr))
    return r.stdout.strip()


class CodexWorktrees(unittest.TestCase):
    def setUp(self):
        self.base = Path(tempfile.mkdtemp(prefix="codex-worktrees-test.")).resolve()
        os.environ["GIT_CONFIG_GLOBAL"] = str(self.base / "gitconfig")
        (self.base / "gitconfig").write_text("[init]\n\tdefaultBranch = main\n")
        self.repo = self.base / "repo"
        self.repo.mkdir()
        git(self.repo, "init", "-q")
        (self.repo / ".gitignore").write_text("build/\nnotes/\n#regenerable: build/\n")
        (self.repo / "a.txt").write_text("a\n")
        git(self.repo, "add", "-A")
        git(self.repo, "commit", "-q", "-m", "base")
        self.plain = self.base / "plain-root"
        self.home = self.base / "codex-home"
        self.app = self.home / "worktrees"
        self.plain.mkdir()
        self.app.mkdir(parents=True)
        self.db = self.home / "state_5.sqlite"
        con = sqlite3.connect(self.db)
        con.execute("create table thread_attachments (id text primary key, thread_id text not null, "
                    "attachment_type text not null, identity_key text not null, payload text not null, "
                    "created_at integer not null)")
        con.commit()
        con.close()
        self.sleepers = []

    def tearDown(self):
        for p in self.sleepers:
            p.kill()
            p.wait()
        shutil.rmtree(self.base, ignore_errors=True)

    def attach(self, kind, path):
        con = sqlite3.connect(self.db)
        con.execute("insert into thread_attachments values (?, 't1', ?, ?, '{}', 0)",
                    (kind + str(path), kind, str(path)))
        con.commit()
        con.close()

    def worktree(self, root, task, branch, merged=True, detached=False):
        path = root / task / "repo"
        path.parent.mkdir(parents=True, exist_ok=True)
        (path.parent / cw.MARKER).write_text(task)
        if detached:
            git(self.repo, "worktree", "add", "-q", "--detach", str(path), "main")
        else:
            git(self.repo, "worktree", "add", "-q", "-b", branch, str(path), "main")
            (path / ("%s.txt" % task)).write_text(task)
            git(path, "add", "-A")
            git(path, "commit", "-q", "-m", task)
            if merged:
                git(self.repo, "merge", "-q", "--no-ff", "-m", "merge " + task, branch)
        return path

    def decide(self, path):
        attached, why = cw.attachments(str(self.home))
        self.assertEqual(why, "")
        return cw.decide(str(path), ws, str(self.app), attached, ws._process_cwds(strict=True))

    def close(self, path):
        return cw.close(str(path), {}, ws, str(self.app),
                        lambda: cw.attachments(str(self.home))[0],
                        lambda: ws._process_cwds(strict=True))

    def test_merged_clean_plain_worktree_closes_with_its_branch(self):
        wt = self.worktree(self.plain, "done", "codex/done")
        action, why, _info = self.decide(wt)
        self.assertEqual(action, cw.DELETE, why)
        self.assertEqual(self.close(wt), ("", ""))
        self.assertFalse(wt.exists())
        self.assertEqual(git(self.repo, "branch", "--list", "codex/done"), "")
        self.assertNotIn(str(wt), git(self.repo, "worktree", "list"))

    def test_regenerable_ignored_output_does_not_hold_it(self):
        wt = self.worktree(self.plain, "built", "codex/built")
        (wt / "build").mkdir()
        (wt / "build" / "out.o").write_text("x")
        self.assertEqual(self.decide(wt)[0], cw.DELETE)

    def test_controls_survive(self):
        dirty = self.worktree(self.plain, "dirty", "codex/dirty")
        (dirty / "a.txt").write_text("changed\n")
        untracked = self.worktree(self.plain, "untracked", "codex/untracked")
        (untracked / "new.txt").write_text("new\n")
        unmerged = self.worktree(self.plain, "unmerged", "codex/unmerged", merged=False)
        ignored = self.worktree(self.plain, "ignored", "codex/ignored")
        (ignored / "notes").mkdir()
        (ignored / "notes" / "needed.md").write_text("only copy\n")
        locked = self.worktree(self.plain, "locked", "codex/locked")
        git(self.repo, "worktree", "lock", str(locked))
        shelved = self.worktree(self.plain, "shelved", "shelved/keep-me")
        active = self.worktree(self.plain, "active", "codex/active")
        self.sleepers.append(subprocess.Popen(["sleep", "60"], cwd=str(active)))
        for path, want in ((dirty, "uncommitted"), (untracked, "uncommitted"), (unmerged, "not all in main"),
                           (ignored, "ignored material"), (locked, "locked"), (shelved, "not a codex/"),
                           (active, "works in it")):
            action, why, _info = self.decide(path)
            self.assertEqual(action, cw.KEEP, "%s: %s" % (path.name, why))
            self.assertIn(want, why)
            self.assertTrue(path.exists())

    def test_app_managed_live_attachment_is_unresolved_not_deleted(self):
        wt = self.worktree(self.app, "live", "codex/live")
        self.attach("worktree", wt)
        action, why, _info = self.decide(wt)
        self.assertEqual(action, cw.UNDECIDED)
        self.assertIn("archive_worktree", why)
        self.assertTrue(self.close(wt)[0].startswith("at deletion time"))
        self.assertTrue(wt.exists())

    def test_app_archived_but_present_and_unattached_close(self):
        archived = self.worktree(self.app, "archived", "codex/archived")
        self.attach("archived_worktree", archived)
        unattached = self.worktree(self.app, "orphan", "", detached=True)
        for wt in (archived, unattached):
            action, why, _info = self.decide(wt)
            self.assertEqual(action, cw.DELETE, why)
            self.assertEqual(self.close(wt), ("", ""))
            self.assertFalse(wt.exists())
            # The parent held only the app's marker after the checkout went.
            self.assertFalse(wt.parent.exists())

    def test_unreadable_attachment_records_are_unresolved(self):
        wt = self.worktree(self.app, "unknown", "codex/unknown")
        self.db.write_bytes(b"not a database")
        attached, why = cw.attachments(str(self.home))
        self.assertIsNone(attached)
        action, why, _info = cw.decide(str(wt), ws, str(self.app), attached, ws._process_cwds(strict=True))
        self.assertEqual(action, cw.UNDECIDED)
        self.assertTrue(wt.exists())

    def test_a_change_after_planning_keeps_it(self):
        wt = self.worktree(self.plain, "late", "codex/late")
        self.assertEqual(self.decide(wt)[0], cw.DELETE)
        (wt / "late-work.txt").write_text("written after the plan\n")
        kept, err = self.close(wt)
        self.assertEqual(err, "")
        self.assertIn("uncommitted", kept)
        self.assertTrue((wt / "late-work.txt").exists())

    def test_checkouts_and_husks(self):
        a = self.worktree(self.app, "one", "codex/one")
        husk = self.app / "gone"
        husk.mkdir()
        (husk / cw.MARKER).write_text("gone")
        (self.app / "empty").mkdir()
        found, husks = cw.checkouts([str(self.app)])
        self.assertEqual(found, [str(a)])
        self.assertEqual(sorted(husks), sorted([str(husk), str(self.app / "empty")]))


if __name__ == "__main__":
    unittest.main()
