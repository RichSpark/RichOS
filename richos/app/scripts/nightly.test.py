#!/usr/bin/env python3
"""Real local Git remotes exercise reservation and compare-and-swap publication.

The publishing tests substitute build/upload commands. No release, signing identity
or installed app is touched by this suite.
"""
import contextlib
from datetime import datetime, timezone
import importlib.util
import hashlib
import inspect
import io
import json
import os
from pathlib import Path
import re
import subprocess
import shutil
import tarfile
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("nightly", Path(__file__).with_name("nightly.py"))
n = importlib.util.module_from_spec(spec)
spec.loader.exec_module(n)
NOW = datetime(2026, 9, 11, 23, 59, tzinfo=timezone.utc)


class VersionTests(unittest.TestCase):
    def test_counter_continues_across_days_and_base_versions(self):
        tags = [f"v5.1.0-nightly.20260911.{i}" for i in range(1, 12)]
        tags += ["v5.2.0-nightly.20260911.999", "v5.1.0-nightly.20260910.999"]
        self.assertEqual(n.next_version("5.1.0", "20260911", tags), "5.1.0-nightly.20260911.1000")
        self.assertEqual(n.next_version("5.1.0", "20260912", tags), "5.1.0-nightly.20260912.1000")
        self.assertEqual(n.next_version("6.0.0", "20260912", tags), "6.0.0-nightly.20260912.1000")
        self.assertGreater(n.nightly_key("5.1.0-nightly.20260911.10"), n.nightly_key("5.1.0-nightly.20260911.9"))

    def test_empty_history_and_unrelated_tags(self):
        self.assertEqual(n.next_version("5.1.0", "20260911",
                                       {"nightly", "v5.0.0", "v5.1.0-beta.999"}),
                         "5.1.0-nightly.20260911.1")

    def test_reservations_survive_missing_release_and_exceed_four_digits(self):
        tags = {"v5.1.0-nightly.20260911.8"}
        self.assertEqual(n.next_version("5.2.0", "20260912", tags, {9999}),
                         "5.2.0-nightly.20260912.10000")

    def test_23_legacy_candidates_seed_24_without_counting_aliases_or_stable(self):
        tags = {f"v1.2.0-nightly.202609{day}.{number}"
                for day, count in ((17, 6), (18, 6), (19, 8), (20, 3))
                for number in range(1, count + 1)}
        tags.update({"nightly", "v1.0.0", "v1.0.1", "v1.0.2"})
        self.assertEqual(n.next_version("1.2.0", "20260921", tags),
                         "1.2.0-nightly.20260921.24")
        tags.add("v1.2.0-nightly.20260921.24")
        self.assertEqual(n.next_version("1.3.0", "20260922", tags, {24}),
                         "1.3.0-nightly.20260922.25")
        # A reserved failed attempt with no release/version entry still consumes 25.
        self.assertEqual(n.next_version("1.3.0", "20260922", tags, {24, 25}),
                         "1.3.0-nightly.20260922.26")

    def test_invalid_versions_and_dates(self):
        for version in ("5.1.0-nightly.20260911.01", "5.1.0-nightly.20260911.0",
                        "05.1.0-nightly.20260911.1", "5.1.0-nightly.20260230.1", "5.1.0"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                n.nightly_key(version)



class GitFixture(unittest.TestCase):
    """The fixture repository and remote. NO test methods live here.

    Split out from `GitTests` when `StableChannelTests` needed the same fixture:
    subclassing a class that holds test methods re-runs every one of them, which cost
    this suite 28 duplicate cases and ~50 s on a gate that runs in every build.
    """
    FIXTURE_IDENT = "Fixture <fixture@example.invalid>"

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        # A commit identity exported by whatever shell runs this suite overrides every
        # level of git config, so without this the identity assertions below would be
        # about the operator instead of the fixture. patch.dict restores the pops.
        environment = patch.dict(os.environ, {})
        environment.start()
        self.addCleanup(environment.stop)
        for name in ("GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL", "GIT_COMMITTER_NAME",
                     "GIT_COMMITTER_EMAIL", "EMAIL"):
            os.environ.pop(name, None)
        root = Path(self.temp.name)
        self.remote = root / "remote.git"
        self.repo = root / "repo"
        self.repo.mkdir()
        subprocess.run(["git", "init", "--bare", "-q", str(self.remote)], check=True)
        subprocess.run(["git", "init", "-q", "-b", "main", str(self.repo)], check=True)
        self.patcher = patch.object(n, "ROOT", self.repo)
        self.patcher.start()
        self.addCleanup(self.patcher.stop)
        # Three things in `promote` now reach GitHub rather than this fixture's local
        # remote: the ruleset read-back, the "does the rolling release exist yet" probe,
        # and the asset upload. Defaulted to no-ops here so every test below still
        # describes Git behavior alone -- a test that wants to inspect them patches
        # again locally, and `patch.object` nests. Before the channel moved to a release
        # tag, `promote` ran no external command at all, which is why these defaults did
        # not exist and why six tests reached for the operator's real `gh` when it did.
        for name, result in (("verify_repository_rules", None), ("succeeds", True),
                             ("execute", None)):
            patcher = patch.object(n, name, return_value=result)
            setattr(self, f"stub_{name}", patcher.start())
            self.addCleanup(patcher.stop)
        n.git("config", "user.name", "Fixture")
        n.git("config", "user.email", "fixture@example.invalid")
        n.git("config", "core.hooksPath", "/dev/null")
        n.git("remote", "add", "origin", str(self.remote))
        for name, text in {
            n.MANIFEST: '[package]\nname = "richos-tauri"\nversion = "5.1.0"\n',
            n.LOCK: 'version = 3\n\n[[package]]\nname = "richos-tauri"\nversion = "5.1.0"\n\n[[package]]\nname = "dependency"\nversion = "1.0.0"\n',
            n.CONFIG: n.json_text({"plugins": {"updater": {"endpoints": ["https://stable.invalid/latest.json"]}}}),
            Path("richos/engine/VERSION"): "1.2.0\n",
            # A stand-in for the commit's own release tooling. `stable_plan` refuses a
            # commit whose `nightly.py` predates the stable channel, because that commit
            # cannot build a stable release of itself -- so the fixture carries the marker
            # that check looks for.
            n.APP / "scripts/nightly.py":
                "CHANNEL_ENDPOINTS = {}\nstable_reservation_holds = None  # fixture tooling\n",
        }.items():
            file = self.repo / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text(text)
        n.git("add", ".")
        n.git("commit", "-qm", "source")
        n.git("push", "-q", "origin", "main")
        self.source = n.git("rev-parse", "HEAD")
        self.releases = {n.CHANNEL_TAG: {"id": 1, "tag_name": n.CHANNEL_TAG,
                                       "prerelease": True, "draft": False}}
        self.asset_bytes = {}
        # What the fixture's engine archive holds. A real engine is a gzipped tar, and the
        # publish step reads it (the FFmpeg source list); a test that needs ffmpeg in the
        # engine sets more files here before it builds.
        self.engine_files = {"engine/VERSION": b"1.2.0\n"}
        for name, replacement in (("get_release", lambda tag: self.releases.get(tag)),
                                  ("release_assets", self.fake_release_assets),
                                  ("verify_served_asset", self.fake_verify_asset)):
            patcher = patch.object(n, name, side_effect=replacement)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.stub_execute.side_effect = self.fake_external
        # Existing asset/race tests isolate the editorial gate. ChangelogTests below
        # restores it and uses real Git approval records and public changelog files.
        self.notes_gate = patch.object(n, "approved_changelog", return_value="- Fixed a lost message")
        self.notes_gate.start()
        self.addCleanup(self.notes_gate.stop)

    def fake_release_assets(self, release_id):
        tag = next(tag for tag, release in self.releases.items() if release["id"] == release_id)
        # GitHub reports every asset's digest; the FFmpeg source step compares it.
        return [{"name": name, "digest": "sha256:" + hashlib.sha256(body).hexdigest()}
                for (stored_tag, name), body in self.asset_bytes.items() if stored_tag == tag]

    def write_engine(self, path):
        with tarfile.open(path, "w:gz") as tar:
            for name, body in self.engine_files.items():
                member = tarfile.TarInfo(name)
                member.size = len(body)
                tar.addfile(member, io.BytesIO(body))

    def fake_verify_asset(self, url, path):
        tag, name = url.split("/releases/download/", 1)[1].split("/", 1)
        if self.asset_bytes.get((tag, name)) != Path(path).read_bytes():
            raise ValueError("published asset differs from the candidate")

    def fake_external(self, *args, **kwargs):
        # Only network/build boundaries are simulated. Reservation, candidate hashes,
        # promotion tags, capacity/reuse and resume decisions run their real code.
        if args[:3] == ("gh", "release", "create"):
            tag = args[3]
            self.assertTrue(n.git("ls-remote", "origin", f"refs/tags/{tag}"))
            self.releases[tag] = {"id": len(self.releases) + 1, "tag_name": tag,
                                  "prerelease": True, "draft": False,
                                  "body": args[args.index("--notes") + 1]
                                  if "--notes" in args else Path(args[args.index("--notes-file") + 1]).read_text()}
        elif args[:3] == ("gh", "release", "upload"):
            tag = args[3]
            paths = args[args.index("--repo") + 2:]
            for item in paths:
                if item.startswith("--"):
                    continue
                path = Path(item)
                self.asset_bytes[tag, path.name] = path.read_bytes()
        elif args[:3] == ("gh", "release", "edit") and "--prerelease=false" in args:
            self.releases[args[3]]["prerelease"] = False
        elif args[0] == "bash" and args[2] == "engine":
            out = Path(args[args.index("--out") + 1])
            self.write_engine(out / "richos-engine-1.2.0.tar.gz")
        elif args[0] == "bash" and args[2] == "app":
            out = Path(args[args.index("--out") + 1])
            info = json.loads((out / "build-info.json").read_text())
            (out / "latest.json").write_text(n.json_text(self.manifest(info)))

    def plan(self, **kwargs):
        return n.plan(now=NOW, **kwargs)

    def manifest(self, info):
        return {"version": info["version"], "platforms": {"darwin-aarch64": {
            "url": f"https://github.com/{n.REPO}/releases/download/{info['tag']}/RichOS.app.tar.gz",
            "signature": "fixture"}}}

    def reserve(self):
        return n.prepare(self.plan())

    def author_and_committer(self, commit):
        return n.git("show", "-s", "--format=%an <%ae>|%cn <%ce>", commit)


class GitTests(GitFixture):
    def test_release_commits_carry_the_checkouts_configured_identity(self):
        # The release is the operator's act, from the operator's checkout. A commit
        # saying otherwise is false, and the machine's push guard refuses it.
        both = f"{self.FIXTURE_IDENT}|{self.FIXTURE_IDENT}"
        info = self.reserve()
        self.assertEqual(self.author_and_committer(info["build_commit"]), both)
        self.assertEqual(self.author_and_committer(n.promote(info, self.manifest(info))), both)

    def test_retired_hard_coded_identity_would_fail_that_check(self):
        """Positive control for the test above.

        Until 2026-09-17 `commit_files` set these four variables itself, which is how
        `5df42d006c45` came to be authored `nightly@users.noreply.github.com` and why
        the first nightly never published. Supplying them through the environment is
        exactly what the old code did, so this proves the assertion bites instead of
        passing because both sides happen to read the same config.
        """
        retired = {"GIT_AUTHOR_NAME": "RichOS nightly", "GIT_COMMITTER_NAME": "RichOS nightly",
                   "GIT_AUTHOR_EMAIL": "nightly@users.noreply.github.com",
                   "GIT_COMMITTER_EMAIL": "nightly@users.noreply.github.com"}
        with patch.dict(os.environ, retired):
            info = self.reserve()
        observed = self.author_and_committer(info["build_commit"])
        self.assertNotEqual(observed, f"{self.FIXTURE_IDENT}|{self.FIXTURE_IDENT}")
        self.assertEqual(observed, "RichOS nightly <nightly@users.noreply.github.com>|"
                                   "RichOS nightly <nightly@users.noreply.github.com>")

    def test_unconfigured_identity_refuses_before_a_number_is_reserved(self):
        n.git("config", "--unset", "user.name")
        n.git("config", "--unset", "user.email")
        with patch.dict(os.environ, {"GIT_CONFIG_GLOBAL": os.devnull,
                                     "GIT_CONFIG_SYSTEM": os.devnull,
                                     "GIT_CONFIG_NOSYSTEM": "1"}):
            # git's hostname fallback would otherwise mint `user@host.home` here and
            # `git commit-tree` would accept it; the push guard denies that too.
            with self.assertRaisesRegex(ValueError, "no configured commit identity"):
                self.plan()
        self.assertEqual([tag for tag in n.remote_tags() if "nightly" in tag], [])

    def test_source_commit_is_complete_and_main_unchanged(self):
        info = self.reserve()
        self.assertEqual(n.git("rev-parse", "main"), self.source)
        self.assertEqual(n.git("rev-parse", "HEAD^"), self.source)
        self.assertEqual(n.git("status", "--porcelain"), "")
        self.assertEqual(n.tomllib.loads((self.repo / n.MANIFEST).read_text())["package"]["version"], info["version"])
        lock = n.tomllib.loads((self.repo / n.LOCK).read_text())["package"]
        self.assertEqual(lock[0]["version"], info["version"])
        self.assertEqual(lock[1]["version"], "1.0.0")
        conf = json.loads((self.repo / n.CONFIG).read_text())
        self.assertEqual(conf["plugins"]["updater"]["endpoints"], n.COMPILED_ENDPOINTS["nightly"])
        self.assertEqual(json.loads((self.repo / n.PROVENANCE).read_text())["source_commit"], self.source)
        self.assertEqual(n.remote_tags(), set())
        self.assertEqual(n.candidate_refs(), {1: info["build_commit"]})
        n.git("checkout", "-q", "main")
        self.assertEqual(self.plan()["version"], "5.1.0-nightly.20260911.2")

    def test_two_plans_cannot_reuse_a_number(self):
        first, second = self.plan(), self.plan()
        n.prepare(first)
        n.git("checkout", "-q", "main")
        with self.assertRaisesRegex(ValueError, "reserved"):
            n.prepare(second)
        self.assertEqual(n.git("rev-parse", "HEAD"), self.source)

    def test_stale_plan_on_another_day_cannot_reuse_a_number(self):
        next_day = n.plan(now=datetime(2026, 9, 12, tzinfo=timezone.utc))
        self.reserve()
        n.git("checkout", "-q", "main")
        with self.assertRaisesRegex(ValueError, "reserved|stale"):
            n.prepare(next_day)
        self.assertNotIn(next_day["tag"], n.remote_tags())

    def test_fresh_clone_recovers_hidden_candidate_and_next_number(self):
        info = self.reserve()
        clone = Path(self.temp.name) / "fresh"
        n.git("clone", "--no-tags", "--branch", "main", str(self.remote), str(clone))
        with patch.object(n, "ROOT", clone):
            self.assertEqual(n.remote_tags(), set())
            self.assertEqual(n.candidate_refs(), {1: info["build_commit"]})
            self.assertEqual(n.next_version("5.1.0", "20260912", n.remote_tags(), n.candidate_refs()),
                             "5.1.0-nightly.20260912.2")
            n.git("fetch", "--no-tags", "origin", info["candidate_ref"])
            self.assertEqual(n.git("rev-parse", "FETCH_HEAD"), info["build_commit"])
            provenance = json.loads(n.git("show", f"FETCH_HEAD:{n.PROVENANCE}"))
            self.assertEqual(provenance["candidate_ref"], "refs/candidates/1")

    def test_different_date_wins_during_reservation_without_any_public_tag(self):
        ours = self.plan()
        theirs = n.plan(now=datetime(2026, 9, 12, tzinfo=timezone.utc))
        original = n.commit_files
        winner = None
        def race(*args):
            nonlocal winner
            files = n.release_files(theirs, (self.repo / n.MANIFEST).read_text(),
                                    (self.repo / n.LOCK).read_text(), (self.repo / n.CONFIG).read_text())
            winner = original(files, self.source, "competing date")
            n.git("push", "origin", f"{winner}:refs/candidates/1")
            return original(*args)
        with patch.object(n, "commit_files", side_effect=race):
            with self.assertRaises(subprocess.CalledProcessError):
                n.prepare(ours)
        self.assertEqual(n.remote_tags(), set())
        self.assertEqual(n.candidate_refs(), {1: winner})

    def test_global_number_race_rejects_full_version_tag_atomically(self):
        # A different candidate wins AFTER prepare's remote read. Neither changing
        # the date nor changing the base version may evade the shared reservation.
        info = self.plan()
        original = n.commit_files
        def competing_number(*args):
            commit = original(*args)
            # The winning ref even points to our candidate's ancestor: an ordinary
            # fast-forward push would overwrite it. The create-only lease must refuse.
            n.git("push", "origin", f"{self.source}:refs/candidates/1")
            return commit
        with patch.object(n, "commit_files", side_effect=competing_number):
            with self.assertRaises(subprocess.CalledProcessError):
                n.prepare(info)
        self.assertNotIn(info["tag"], n.remote_tags())
        self.assertEqual(n.git("rev-parse", "HEAD"), self.source)
        self.assertEqual(self.plan()["version"], "5.1.0-nightly.20260911.2")

    def test_failed_candidate_consumes_number_across_date_and_base_bump(self):
        first = self.reserve()
        with patch.object(n, "execute", side_effect=RuntimeError("build failed")):
            with self.assertRaisesRegex(RuntimeError, "build failed"):
                n.build(first, Path(self.temp.name) / "failed-candidate")
        n.git("checkout", "-q", "main")
        for path in (n.MANIFEST, n.LOCK):
            file = self.repo / path
            file.write_text(file.read_text().replace('"5.1.0"', '"5.2.0"'))
        n.git("commit", "-qam", "next base")
        second = n.prepare(n.plan(now=datetime(2026, 9, 12, tzinfo=timezone.utc)))
        self.assertEqual(second["version"], "5.2.0-nightly.20260912.2")
        self.assertEqual(n.remote_tags(), set())
        self.assertEqual(n.candidate_refs(), {1: first["build_commit"], 2: second["build_commit"]})
        self.assertEqual(n.channel(), (None, None))

    def test_migration_starts_above_all_historical_suffixes(self):
        legacy = {"v5.0.0-nightly.20260909.8", "v5.1.0-nightly.20260910.3"}
        n.git("push", "origin", *(f"{self.source}:refs/tags/{tag}" for tag in legacy))
        info = self.reserve()
        self.assertEqual(info["version"], "5.1.0-nightly.20260911.9")
        self.assertTrue(legacy <= n.remote_tags())
        self.assertEqual(n.candidate_refs(), {9: info["build_commit"]})

    def test_remote_without_atomic_push_cannot_partially_reserve(self):
        n.git("config", "receive.advertiseAtomic", "false", cwd=self.remote)
        info = self.plan()
        with self.assertRaises(subprocess.CalledProcessError):
            n.prepare(info)
        self.assertEqual(n.remote_tags(), set())
        self.assertEqual(n.candidate_refs(), {})
        self.assertEqual(n.git("rev-parse", "HEAD"), self.source)

    def test_public_tag_pointing_elsewhere_refuses_promotion(self):
        info = self.reserve()
        n.git("push", "origin", f"{self.source}:refs/tags/{info['tag']}")
        with self.assertRaisesRegex(ValueError, "different commit"):
            n.ensure_version_tag(info)
        self.assertEqual(n.git("ls-remote", "origin", f"refs/tags/{info['tag']}").split()[0], self.source)
        self.assertEqual(n.candidate_refs(), {1: info["build_commit"]})

    def test_dirty_and_released_sources_are_refused(self):
        (self.repo / "untracked").write_text("dirty")
        with self.assertRaisesRegex(ValueError, "clean"):
            self.plan()
        (self.repo / "untracked").unlink()
        n.git("push", "origin", "HEAD:refs/tags/v5.1.0")
        with self.assertRaisesRegex(ValueError, "bump"):
            self.plan()

    def test_plan_is_invalid_if_source_changes(self):
        info = self.plan()
        n.git("commit", "--allow-empty", "-qm", "next source")
        with self.assertRaisesRegex(ValueError, "source changed"):
            n.prepare(info)

    def test_successful_source_skipped_but_manual_force_allocates(self):
        info = self.reserve()
        n.promote(info, self.manifest(info))
        n.git("checkout", "-q", "main")
        self.assertFalse(self.plan()["build"])
        self.assertEqual(self.plan(force=True)["version"], "5.1.0-nightly.20260911.2")
        later = n.plan(force=True,
                       now=datetime(2026, 9, 12, tzinfo=timezone.utc))
        self.assertEqual(later["version"], "5.1.0-nightly.20260912.2")

    def test_promotion_never_rolls_back(self):
        old = self.reserve()
        n.git("checkout", "-q", "main")
        new = self.reserve()
        n.promote(new, self.manifest(new))
        with self.assertRaisesRegex(ValueError, "newer"):
            n.promote(old, self.manifest(old))
        _, current = n.channel()
        self.assertEqual(current["version"], new["version"])

    def test_promotion_rejects_wrong_manifest(self):
        info = self.reserve()
        bad = self.manifest(info)
        bad["version"] = "5.1.0"
        with self.assertRaisesRegex(ValueError, "version"):
            n.promote(info, bad)
        bad = self.manifest(info)
        bad["platforms"]["darwin-aarch64"]["url"] = "https://elsewhere.invalid/archive"
        with self.assertRaisesRegex(ValueError, "immutable"):
            n.promote(info, bad)
        self.assertEqual(n.channel(), (None, None))

    def test_promotion_compare_and_swap_rejects_race(self):
        info = self.reserve()
        original_commit = n.commit_files
        def competing_writer(files, parent, message):
            planned = original_commit(files, parent, message)
            concurrent = original_commit({"race.txt": "another publisher"}, parent, "competing write")
            n.git("push", "origin", f"{concurrent}:{n.CHANNEL_REF}")
            return planned
        with patch.object(n, "commit_files", side_effect=competing_writer):
            with self.assertRaises(subprocess.CalledProcessError):
                n.promote(info, self.manifest(info))
        ref = n.git("ls-remote", "origin", n.CHANNEL_REF).split()[0]
        self.assertEqual(n.git("show", f"{ref}:race.txt"), "another publisher")

    def test_plan_rejects_divergent_source(self):
        n.git("commit", "--allow-empty", "-qm", "new main")
        advanced_source = n.git("rev-parse", "HEAD")
        info = self.reserve()
        self.assertEqual(info["source_commit"], advanced_source)
        n.promote(info, self.manifest(info))
        n.git("checkout", "--detach", self.source)
        with self.assertRaises(subprocess.CalledProcessError):
            self.plan(force=True)

    def test_failed_build_or_verification_never_moves_channel(self):
        info = self.reserve()
        for failure in ("engine", "verify-engine", "app", "verify-assets"):
            with self.subTest(failure=failure):
                out = Path(self.temp.name) / failure
                def fake(*args, **kwargs):
                    if args[0] == "bash" and args[2] == failure:
                        raise subprocess.CalledProcessError(1, args)
                    self.fake_external(*args, **kwargs)
                with patch.object(n, "execute", side_effect=fake), patch.object(n, "promote") as promote:
                    with self.assertRaises(subprocess.CalledProcessError):
                        n.publish(info, out)
                    promote.assert_not_called()
                self.assertEqual(n.channel(), (None, None))

    def test_upload_failure_does_not_move_channel(self):
        info = self.reserve()
        def fake(*args, **kwargs):
            if args[:3] == ("gh", "release", "upload"):
                raise subprocess.CalledProcessError(1, args)
            self.fake_external(*args, **kwargs)
        with patch.object(n, "execute", side_effect=fake), patch.object(n, "promote") as promote:
            with self.assertRaises(subprocess.CalledProcessError):
                n.publish(info, Path(self.temp.name) / "upload-failure")
            promote.assert_not_called()

    def test_publish_order_and_channel_manifest(self):
        info = self.reserve()
        out = Path(self.temp.name) / "publish"
        calls = []
        def fake(*args, **kwargs):
            calls.append(args)
            self.fake_external(*args, **kwargs)
        with patch.object(n, "execute", side_effect=fake):
            n.publish(info, out)
        stages = [c[2] for c in calls if c[0] == "bash"]
        self.assertEqual(stages, ["engine", "verify-engine", "app", "verify-assets"])
        create = next(c for c in calls if c[:3] == ("gh", "release", "create"))
        self.assertIn("--prerelease", create)
        self.assertIn("--latest=false", create)
        # The immutable release gets its manifest last of its own assets, and only then
        # is anything verified. The channel comes after that, and last of everything:
        # `verify-assets` is what earns the right to move it.
        manifest_upload = next(c for c in calls if c[:4] == ("gh", "release", "upload", info["tag"])
                               and c[-1].endswith("/latest.json"))
        self.assertEqual(manifest_upload[:5], ("gh", "release", "upload", info["tag"], "--repo"))
        self.assertTrue(manifest_upload[-1].endswith("/latest.json"))
        self.assertLess(calls.index(manifest_upload),
                        next(i for i, c in enumerate(calls) if c[0] == "bash" and c[2] == "verify-assets"))
        # Last call of the whole publish: the rolling channel release's asset, replaced.
        self.assertEqual(calls[-1][:6], ("gh", "release", "upload", n.CHANNEL_TAG,
                                         "--repo", n.REPO))
        self.assertIn("--clobber", calls[-1])
        self.assertTrue(calls[-1][-1].endswith("/latest.json"))
        oid, current = n.channel()
        self.assertEqual(current, info)
        self.assertEqual(json.loads(n.git("show", f"{oid}:latest.json")), self.manifest(info))
        # The branch ruleset is read back on the way through, not merely documented.
        self.assertGreaterEqual(self.stub_verify_repository_rules.call_count, 1)

    def fake_execute(self, out, calls=None):
        calls = calls if calls is not None else []
        def fake(*args, **kwargs):
            calls.append(args)
            self.fake_external(*args, **kwargs)
        return fake, calls

    def manifest_for(self, out):
        # `out` is not part of `info`; the candidate's own recorded info supplies the
        # version. Read it back rather than threading `info` through every caller.
        info = json.loads((out / "build-info.json").read_text())
        return self.manifest(info)

    def test_build_stops_before_any_app_upload_or_channel_move(self):
        info = self.reserve()
        out = Path(self.temp.name) / "build-only"
        fake, calls = self.fake_execute(out)
        with patch.object(n, "execute", side_effect=fake), patch.object(n, "promote") as promote:
            n.build(info, out)
        stages = [c[2] for c in calls if c[0] == "bash"]
        # Not "verify-assets": that is finish()'s, and nothing here reached it.
        self.assertEqual(stages, ["engine", "verify-engine", "app"])
        uploads = [c for c in calls if c[:3] == ("gh", "release", "upload")]
        # Only the engine asset -- never the app archive, the .sig or latest.json.
        self.assertEqual(len(uploads), 1)
        self.assertTrue(uploads[0][-1].endswith(".tar.gz"))
        self.assertEqual(uploads[0][3], n.CHANNEL_TAG)
        self.assertEqual(n.remote_tags(), set())
        self.assertEqual(set(self.releases), {n.CHANNEL_TAG})
        promote.assert_not_called()
        self.assertEqual(n.channel(), (None, None))
        self.assertTrue((out / n.CANDIDATE_MANIFEST).exists())

    def test_rejected_candidates_leave_gaps_and_publish_preserves_tested_build(self):
        for number in (1, 2):
            rejected = self.reserve()
            self.assertEqual(n.nightly_key(rejected["version"])[-1], number)
            out = Path(self.temp.name) / f"rejected-{number}"
            fake, _ = self.fake_execute(out)
            with patch.object(n, "execute", side_effect=fake):
                n.build(rejected, out)
            n.git("checkout", "-q", "main")
        accepted = n.prepare(n.plan(now=datetime(2026, 9, 12, tzinfo=timezone.utc)))
        self.assertEqual(accepted["version"], "5.1.0-nightly.20260912.3")
        out = Path(self.temp.name) / "accepted"
        fake, calls = self.fake_execute(out)
        with patch.object(n, "execute", side_effect=fake):
            n.build(accepted, out)
        before = n._candidate_files(out)
        tags_before = n.remote_tags()
        calls.clear()
        with patch.object(n, "execute", side_effect=fake), patch.object(n, "prepare") as prepare:
            n.finish(accepted, out)
        prepare.assert_not_called()
        self.assertEqual([c[2] for c in calls if c[0] == "bash"], ["verify-assets"])
        self.assertEqual(n._candidate_files(out), before)
        self.assertEqual(n.remote_tags(), tags_before | {n.CHANNEL_TAG, accepted["tag"]})
        self.assertEqual(n.channel()[1], accepted)

    def test_finish_refuses_a_tampered_candidate(self):
        info = self.reserve()
        out = Path(self.temp.name) / "tampered"
        fake, _ = self.fake_execute(out)
        with patch.object(n, "execute", side_effect=fake):
            n.build(info, out)
        # Something touched a staged, already-recorded file after `build` ran --
        # here the manifest `finish` is about to upload as `latest.json`.
        (out / "latest.json").write_text("not what was signed")
        with patch.object(n, "execute", side_effect=fake), patch.object(n, "promote") as promote:
            with self.assertRaisesRegex(ValueError, "changed since it was built"):
                n.finish(info, out)
            promote.assert_not_called()
        self.assertEqual(n.channel(), (None, None))

    def test_finish_refuses_an_unrecorded_extra_artifact(self):
        info = self.reserve()
        out = Path(self.temp.name) / "extra"
        n.build(info, out)
        (out / "unexpected.txt").write_text("never tested")
        with self.assertRaisesRegex(ValueError, "changed since"):
            n.finish(info, out)
        self.assertEqual(n.remote_tags(), set())

    def test_finish_publishes_the_standalone_doctor_with_candidate_assets(self):
        info = self.reserve()
        out = Path(self.temp.name) / "doctor"
        name, content = "RichDoctor-macos-aarch64.zip", b"signed Doctor archive fixture"

        def stage_doctor(*args, **kwargs):
            self.fake_external(*args, **kwargs)
            if args[0] == "bash" and args[2] == "app":
                (out / name).write_bytes(content)

        with patch.object(n, "execute", side_effect=stage_doctor):
            n.build(info, out)
        n.finish(info, out)
        self.assertEqual(self.asset_bytes[info["tag"], name], content)

    def test_promotion_resumes_after_tag_and_after_partial_upload(self):
        info = self.reserve()
        out = Path(self.temp.name) / "resume"
        n.build(info, out)
        before = n._candidate_files(out)
        engine_assets = dict(self.asset_bytes)
        calls = []
        def fail_create(*args, **kwargs):
            if args[:3] == ("gh", "release", "create"):
                raise subprocess.CalledProcessError(1, args)
            self.fake_external(*args, **kwargs)
        with patch.object(n, "execute", side_effect=fail_create):
            with self.assertRaises(subprocess.CalledProcessError):
                n.finish(info, out)
        self.assertEqual(n.remote_tags(), {info["tag"]})
        self.assertEqual(n.channel(), (None, None))
        self.assertNotIn(info["tag"], self.releases)
        def fail_manifest(*args, **kwargs):
            if args[:4] == ("gh", "release", "upload", info["tag"]) and args[-1].endswith("/latest.json"):
                raise subprocess.CalledProcessError(1, args)
            self.fake_external(*args, **kwargs)
        with patch.object(n, "execute", side_effect=fail_manifest):
            with self.assertRaises(subprocess.CalledProcessError):
                n.finish(info, out)
        uploaded = {name for tag, name in self.asset_bytes if tag == info["tag"]}
        self.assertTrue(uploaded)
        self.assertNotIn("latest.json", uploaded)
        self.assertEqual(n.channel(), (None, None))
        def finish(*args, **kwargs):
            calls.append(args)
            self.fake_external(*args, **kwargs)
        with patch.object(n, "execute", side_effect=finish), patch.object(n, "prepare") as prepare:
            n.finish(info, out)
        prepare.assert_not_called()
        uploads = [c for c in calls if c[:4] == ("gh", "release", "upload", info["tag"])]
        self.assertEqual([Path(c[-1]).name for c in uploads], ["latest.json"])
        self.assertEqual([c[2] for c in calls if c[0] == "bash"], ["verify-assets"])
        self.assertEqual(n._candidate_files(out), before)
        self.assertEqual(n.candidate_refs(), {1: info["build_commit"]})
        self.assertEqual(n.channel()[1], info)
        for key, value in engine_assets.items():
            self.assertEqual(self.asset_bytes[key], value)

    def test_retry_refuses_conflicting_existing_artifact_without_clobber(self):
        info = self.reserve()
        out = Path(self.temp.name) / "conflict"
        n.build(info, out)
        n.ensure_version_tag(info)
        n.ensure_nightly_release(info)
        self.asset_bytes[info["tag"], "build-info.json"] = b"different bytes"
        with self.assertRaisesRegex(ValueError, "differs"):
            n.finish(info, out)
        self.assertEqual(self.asset_bytes[info["tag"], "build-info.json"], b"different bytes")
        self.assertEqual(n.channel(), (None, None))

    def test_finish_publishes_an_untouched_candidate(self):
        """Positive control for the test above: the same candidate, unmodified, publishes."""
        info = self.reserve()
        out = Path(self.temp.name) / "untouched"
        fake, calls = self.fake_execute(out)
        with patch.object(n, "execute", side_effect=fake):
            n.build(info, out)
        with patch.object(n, "execute", side_effect=fake):
            n.finish(info, out)
        stages = [c[2] for c in calls if c[0] == "bash"]
        self.assertEqual(stages, ["engine", "verify-engine", "app", "verify-assets"])
        oid, current = n.channel()
        self.assertEqual(current, info)
        self.assertEqual(json.loads(n.git("show", f"{oid}:latest.json")), self.manifest(info))

    def test_an_engine_carrying_ffmpeg_is_published_with_its_corresponding_source(self):
        """GPLv3: the release carries every archive the engine's own runtime lists.

        Each is downloaded once into the cache, checked against its sha256, uploaded beside
        the engine under a name of its own (upstream names are not unique: `v1.6.0.tar.gz`,
        `download`), and all of it before verify-assets, so the channel never moves to a
        release whose GPL source is missing.
        """
        upstream = {"https://upstream.invalid/ffmpeg-9.0.2.tar.bz2": b"ffmpeg source",
                    "https://upstream.invalid/zvbi-0.2.35.tar.bz2/download": b"zvbi source"}
        rows = [{"name": name, "version": version, "url": url,
                 "sha256": hashlib.sha256(upstream[url]).hexdigest()}
                for (name, version), url in zip((("ffmpeg", "9.0.2"), ("zvbi", "0.2.35")), upstream)]
        recipe = {"sources": {"ffmpeg": {"version": "9.0.2", "corresponding_source": rows}}}
        self.engine_files = {"engine/runtime/bin/ffmpeg": b"a GPL program",
                             "engine/runtime/runtime-sources.json": json.dumps(recipe).encode()}
        # zvbi's host answers with a bot-wall page (code.videolan.org did, 2026-10-08); the
        # checkout's recipe names a mirror holding the pinned bytes, which is what is taken.
        mirror = "https://mirror.invalid/zvbi-0.2.35.tar.bz2"
        served = {**upstream, rows[1]["url"]: b"<title>Making sure you're not a bot!</title>",
                  mirror: b"zvbi source"}
        fetched = []
        def fetch(url, path):
            fetched.append(url)
            Path(path).write_bytes(served[url])
        cache = Path(self.temp.name) / "gpl-source-cache"
        info = self.reserve()
        out = Path(self.temp.name) / "with-ffmpeg"
        fake, calls = self.fake_execute(out)
        with patch.dict(os.environ, {"RICHOS_GPL_SOURCE_CACHE": str(cache)}), \
                patch.object(n, "fetch_source_archive", side_effect=fetch, create=True), \
                patch.object(n, "source_mirrors", create=True,
                             return_value={rows[1]["sha256"]: [mirror]}), \
                patch.object(n, "execute", side_effect=fake):
            n.build(info, out)
            n.finish(info, out)
            published = {name: body for (tag, name), body in self.asset_bytes.items()
                         if tag == info["tag"]}
            self.assertEqual(published.get("ffmpeg-source-ffmpeg-9.0.2.tar.bz2"), b"ffmpeg source")
            self.assertEqual(published.get("ffmpeg-source-zvbi-0.2.35.tar.bz2"), b"zvbi source")
            source_uploads = [i for i, c in enumerate(calls) if c[:3] == ("gh", "release", "upload")
                              and Path(c[-1]).name.startswith("ffmpeg-source-")]
            verify = next(i for i, c in enumerate(calls) if c[0] == "bash" and c[2] == "verify-assets")
            self.assertLess(max(source_uploads), verify)
            self.assertEqual(n.channel()[1], info)
            # The same step a second time (the backfill of an already-published release):
            # nothing downloaded again, nothing uploaded again, everything re-verified.
            uploads_before = len(calls)
            n.attach_gpl_sources(info["tag"], out / "richos-engine-1.2.0.tar.gz")
            self.assertEqual(len(calls), uploads_before)
        self.assertEqual(sorted(fetched), sorted([*upstream, mirror]))
        # An upstream that serves other bytes than the pin is refused and leaves no cache file.
        wrong = {**rows[0], "sha256": "0" * 64, "asset": "ffmpeg-source-ffmpeg-9.0.2.tar.bz2"}
        empty = Path(self.temp.name) / "empty-cache"
        empty.mkdir()
        with patch.object(n, "fetch_source_archive", side_effect=fetch), \
                self.assertRaisesRegex(ValueError, "not the pinned"):
            n.cached_gpl_source(wrong, empty)
        self.assertEqual(list(empty.iterdir()), [])

    def test_the_committed_recipe_names_the_assets_the_publish_step_uploads(self):
        """FFMPEG-SOURCE.txt prints each row's `asset`; the publish step derives the name.
        They must agree, or the directions inside the runtime point at files nobody uploads."""
        recipe = json.loads((Path(__file__).with_name("runtime-sources.json")).read_text())
        for row in recipe["sources"]["ffmpeg"]["corresponding_source"]:
            self.assertEqual(row.get("asset"), n.ffmpeg_source_asset(row), row["name"])

    def test_finish_refuses_when_source_is_no_longer_an_ancestor_of_main(self):
        info = self.reserve()
        out = Path(self.temp.name) / "stale-main"
        fake, _ = self.fake_execute(out)
        with patch.object(n, "execute", side_effect=fake):
            n.build(info, out)
        # main gets force-pushed to an unrelated, parentless history -- one that does
        # NOT contain the built source as an ancestor -- exactly what a walker being
        # asked to wait for a QA verdict is exposed to.
        empty_tree = n.git("hash-object", "-t", "tree", "/dev/null")
        orphan = n.git("commit-tree", empty_tree, input="rewritten history\n")
        n.git("push", "-q", "--force", "origin", f"{orphan}:main")
        with patch.object(n, "execute", side_effect=fake), patch.object(n, "promote") as promote:
            with self.assertRaises(subprocess.CalledProcessError):
                n.finish(info, out)
            promote.assert_not_called()
        self.assertEqual(n.channel(), (None, None))

    def test_a_moved_checkout_asks_for_the_build_commit_back_never_a_rebuild(self):
        """Hunt part 2, section 16: another command moved the shared worktree after `build`.

        The guard stays: `finish` runs this checkout's make-release.sh and reads its engine
        VERSION, so the checkout must be the build commit. But the candidate is intact and the
        build commit is immutable, so the repair is that commit back, never a second build.
        """
        info = self.reserve()
        out = Path(self.temp.name) / "moved"
        fake, calls = self.fake_execute(out)
        with patch.object(n, "execute", side_effect=fake):
            n.build(info, out)
        # What a `check` (or another `build`) does to the one dedicated worktree in between.
        n.git("checkout", "-q", "main")
        with patch.object(n, "execute", side_effect=fake), patch.object(n, "promote") as promote:
            with self.assertRaises(ValueError) as refused:
                n.finish(info, out)
            promote.assert_not_called()
        message = str(refused.exception)
        self.assertNotIn("build\" again", message)
        self.assertIn(info["build_commit"], message)
        self.assertEqual(n.channel(), (None, None))
        # A tree with changes in it still refuses, and nothing in it is discarded.
        n.git("checkout", "-q", "--detach", info["build_commit"])
        stray = self.repo / "stray.txt"
        stray.write_text("someone's work")
        with patch.object(n, "execute", side_effect=fake), self.assertRaisesRegex(ValueError, "changes"):
            n.finish(info, out)
        self.assertEqual(stray.read_text(), "someone's work")
        stray.unlink()
        # The repair it names is enough: the recorded commit is back, and nothing is rebuilt.
        calls.clear()
        with patch.object(n, "execute", side_effect=fake):
            n.finish(info, out)
        self.assertEqual([c[2] for c in calls if c[0] == "bash"], ["verify-assets"])
        self.assertEqual(n.channel()[1], info)

    def test_the_rolling_channel_release_is_created_once_then_only_replaced(self):
        """`--verify-tag` is why the create can only run after the tag has moved."""
        self.stub_succeeds.return_value = False       # no `nightly` release yet
        first = self.reserve()
        calls = []
        with patch.object(n, "execute", side_effect=lambda *a, **k: calls.append(a)):
            n.promote(first, self.manifest(first))
        create = [c for c in calls if c[:3] == ("gh", "release", "create")]
        self.assertEqual(len(create), 1)
        self.assertEqual(create[0][3], n.CHANNEL_TAG)
        for flag in ("--verify-tag", "--prerelease", "--latest=false"):
            self.assertIn(flag, create[0])
        # The tag it verifies must already be on the remote by the time create runs.
        self.assertEqual(n.git("ls-remote", "origin", n.CHANNEL_REF).split()[1], n.CHANNEL_REF)

        self.stub_succeeds.return_value = True        # it exists from here on
        n.git("checkout", "-q", "main")
        # The source has not moved, so plan() reports "already published" for it.
        second = n.prepare(self.plan(force=True))
        calls.clear()
        with patch.object(n, "execute", side_effect=lambda *a, **k: calls.append(a)):
            n.promote(second, self.manifest(second))
        self.assertEqual([c for c in calls if c[:3] == ("gh", "release", "create")], [])
        self.assertIn("--clobber", [c for c in calls if c[:3] == ("gh", "release", "upload")][0])

    def test_finish_repairs_an_asset_the_channel_already_names(self):
        """A crash between the lease and the upload must be re-runnable, not a rollback.

        Moving the channel tag and replacing the release asset are two acts, not one.
        Lease first means a crash in between leaves the tag naming a version whose asset
        was never replaced -- users stay on the older one. Re-running `finish` has to
        repair that, and `promote`'s ordering checks would otherwise call it a move
        backwards and refuse. Without the repair arm this state is unrecoverable by
        re-running the documented command.
        """
        info = self.reserve()
        with patch.object(n, "execute", side_effect=subprocess.CalledProcessError(1, "gh")):
            with self.assertRaises(subprocess.CalledProcessError):
                n.promote(info, self.manifest(info))
        # The lease was taken and the tag DID move; only the asset is missing.
        leased, current = n.channel()
        self.assertEqual(current, info)

        calls = []
        with patch.object(n, "execute", side_effect=lambda *a, **k: calls.append(a)):
            repaired = n.promote(info, self.manifest(info))
        self.assertEqual(repaired, leased, "the repair must not move the channel again")
        self.assertEqual(n.channel()[0], leased)
        uploads = [c for c in calls if c[:3] == ("gh", "release", "upload")]
        self.assertEqual(len(uploads), 1)
        self.assertEqual(uploads[0][3], n.CHANNEL_TAG)
        self.assertIn("--clobber", uploads[0])

    def test_the_repair_arm_does_not_excuse_a_genuine_rollback(self):
        """Positive control for the test above: only the SAME candidate is repairable."""
        old = self.reserve()
        n.git("checkout", "-q", "main")
        new = self.reserve()
        with patch.object(n, "execute"):
            n.promote(new, self.manifest(new))
            with self.assertRaisesRegex(ValueError, "newer"):
                n.promote(old, self.manifest(old))

    def test_release_equals_build_then_finish(self):
        """`release` (== `publish`) and an explicit `build` + `finish` reach the same state."""
        fused_info = self.reserve()
        fused_out = Path(self.temp.name) / "fused"
        fused_fake, fused_calls = self.fake_execute(fused_out)
        with patch.object(n, "execute", side_effect=fused_fake):
            n.publish(fused_info, fused_out)
        fused_stages = [c[2] for c in fused_calls if c[0] == "bash"]
        fused_oid, fused_channel = n.channel()

        n.git("checkout", "-q", "main")
        # The source has not moved, so plan() would otherwise report "already
        # published" for it -- force a second nightly of the same source, same as
        # test_successful_source_skipped_but_manual_force_allocates.
        split_info = n.prepare(self.plan(force=True))
        split_out = Path(self.temp.name) / "split"
        split_fake, split_calls = self.fake_execute(split_out)
        with patch.object(n, "execute", side_effect=split_fake):
            n.build(split_info, split_out)
        with patch.object(n, "execute", side_effect=split_fake):
            n.finish(split_info, split_out)
        split_stages = [c[2] for c in split_calls if c[0] == "bash"]
        split_oid, split_channel = n.channel()

        self.assertEqual(fused_stages, split_stages)
        self.assertEqual(fused_channel["version"], fused_info["version"])
        self.assertEqual(split_channel["version"], split_info["version"])
        self.assertEqual(json.loads(n.git("show", f"{fused_oid}:latest.json")),
                         self.manifest(fused_info))
        self.assertEqual(json.loads(n.git("show", f"{split_oid}:latest.json")),
                         self.manifest(split_info))


class ChangelogTests(GitFixture):
    def setUp(self):
        super().setUp()
        self.notes_gate.stop()
        for document in n.CHANGELOG_FILES.values():
            path = self.repo / document
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("# Changelog\n\n")
        n.git("add", "docs")
        n.git("commit", "-qm", "changelog documents")
        n.git("push", "-q", "origin", "main")
        self.source = n.git("rev-parse", "HEAD")

    def candidate(self):
        info = self.reserve()
        out = Path(self.temp.name) / info["tag"]
        n.build(info, out)
        return info, out

    def approve(self, info, request=None, land=True):
        request = request or n.changelog_request(info)
        n.git("checkout", "-qb", "notes-" + info["build_commit"][:8], "origin/main")
        path = n.write_changelog_draft(request, "- Fixed a lost message")
        n.git("add", "docs")
        n.git("commit", "-qm", "reviewed candidate notes")
        if land:
            n.git("push", "-q", "origin", "HEAD:main")
        n.git("checkout", "-q", "--detach", info["build_commit"])
        return path

    def test_missing_and_unlanded_notes_refuse_before_any_public_version(self):
        info, out = self.candidate()
        with self.assertRaisesRegex(ValueError, "reviewed changelog is not on main"):
            n.finish(info, out)
        self.approve(info, land=False)
        with self.assertRaisesRegex(ValueError, "reviewed changelog is not on main"):
            n.finish(info, out)
        self.assertEqual(n.remote_tags(), set())
        self.assertEqual(set(self.releases), {n.CHANNEL_TAG})

    def test_approved_notes_are_in_the_release_at_its_first_public_appearance(self):
        info, out = self.candidate()
        before = n._candidate_files(out)
        self.approve(info)
        n.finish(info, out)
        body = self.releases[info["tag"]]["body"]
        self.assertTrue(body.startswith("- Fixed a lost message\n\n"), body)
        self.assertIn("Source: " + info["source_commit"], body)
        self.assertEqual(n._candidate_files(out), before)

    def test_wrong_build_record_and_edited_public_entry_refuse(self):
        info, out = self.candidate()
        self.approve(info)
        n.git("checkout", "-qB", "approval-edit", "origin/main")
        path = self.repo / n.CHANGELOG_RECORDS
        records = json.loads(path.read_text())
        record = records[info["tag"]]
        original = dict(record)
        record["build_commit"] = "f" * 40
        path.write_text(n.json_text(records))
        n.git("add", "docs"); n.git("commit", "-qm", "wrong candidate")
        n.git("push", "-q", "origin", "HEAD:main")
        with self.assertRaisesRegex(ValueError, "different candidate"):
            n.approved_changelog(info)
        records[info["tag"]] = original
        path.write_text(n.json_text(records))
        document = self.repo / n.CHANGELOG_FILES["nightly"]
        document.write_text(document.read_text().replace("lost message", "different change"))
        n.git("add", "docs"); n.git("commit", "-qm", "different public prose")
        n.git("push", "-q", "origin", "HEAD:main")
        with self.assertRaisesRegex(ValueError, "public changelog disagree"):
            n.approved_changelog(info)
        self.assertEqual(n.remote_tags(), set())

    def test_failed_upload_retains_approved_notes_for_the_same_candidate(self):
        info, out = self.candidate()
        self.approve(info)
        original = self.fake_external
        def fail(*args, **kwargs):
            if args[:4] == ("gh", "release", "upload", info["tag"]):
                raise subprocess.CalledProcessError(1, args)
            return original(*args, **kwargs)
        with patch.object(n, "execute", side_effect=fail), self.assertRaises(subprocess.CalledProcessError):
            n.finish(info, out)
        self.assertEqual(n.channel(), (None, None))
        body = self.releases[info["tag"]]["body"]
        n.finish(info, out)
        self.assertEqual(self.releases[info["tag"]]["body"], body)
        self.assertEqual(n.channel()[1], info)

    def test_rejected_candidates_do_not_advance_the_notes_baseline(self):
        rejected, _ = self.candidate()
        request = n.changelog_request(rejected)
        self.assertIsNone(request["previous_tag"])
        n.git("checkout", "-q", "main")
        n.git("commit", "--allow-empty", "-qm", "Fixed another user-visible problem")
        n.git("push", "-q", "origin", "main")
        accepted = self.reserve()
        replacement = n.changelog_request(accepted)
        self.assertIsNone(replacement["previous_tag"])
        self.assertIn("changelog documents", replacement["changes"])
        self.assertIn("Fixed another user-visible problem", replacement["changes"])
        self.assertNotEqual(replacement["build_commit"], request["build_commit"])
        self.assertEqual(n.remote_tags(), set())

    def test_history_is_a_compact_index_and_inventory_is_the_pinned_net_implementation(self):
        for name in ("behavior.txt", "removed.txt"):
            (self.repo / name).write_text("before\n")
        n.git("add", ".")
        n.git("commit", "-qm", "published source")
        baseline = n.git("rev-parse", "HEAD")
        (self.repo / "behavior.txt").write_text("project instructions reach workers only\n")
        (self.repo / "removed.txt").unlink()
        (self.repo / "user feature.txt").write_text("recover after the connection returns\n")
        (self.repo / "reverted.txt").write_text("temporary implementation\n")
        n.git("add", "-A")
        n.git("commit", "-qm", "Misleading author account", "-m", "UNESTABLISHED_BODY_CLAIM")
        (self.repo / "reverted.txt").unlink()
        n.git("add", "-A")
        n.git("commit", "-qm", "remove the temporary behavior")
        info, _ = self.candidate()
        n.git("checkout", "-q", "main")
        (self.repo / "later.txt").write_text("not in this candidate\n")
        n.git("add", ".")
        n.git("commit", "-qm", "future behavior")
        with patch.object(n, "published_changelog_base", return_value=("v5.0.0", baseline)):
            request = n.changelog_request(info)
        self.assertEqual(request["changed_files"], [
            {"status": "M", "path": "behavior.txt"},
            {"status": "D", "path": "removed.txt"},
            {"status": "A", "path": "user feature.txt"},
        ])
        self.assertIn("Misleading author account", request["changes"])
        self.assertNotIn("UNESTABLISHED_BODY_CLAIM", request["changes"])
        self.assertNotIn("future behavior", request["changes"])

    def test_first_release_inventory_is_source_files_not_packaging_or_the_current_checkout(self):
        info, _ = self.candidate()
        (self.repo / "untracked later file").write_text("not shipped\n")
        request = n.changelog_request(info)
        source_files = n.git("ls-tree", "-r", "--name-only", info["source_commit"]).splitlines()
        self.assertEqual(request["changed_files"],
                         [{"status": "A", "path": name} for name in source_files])
        self.assertNotIn(str(n.PROVENANCE), source_files)

    def test_stale_baseline_refuses_and_empty_ranges_are_valid(self):
        info, out = self.candidate()
        self.approve(info)
        # An independently published nightly moves the baseline after drafting.
        other = {**info, "tag": "v5.1.0-nightly.20260911.2", "version": "5.1.0-nightly.20260911.2"}
        with patch.object(n, "published_changelog_base", return_value=(other["tag"], self.source)):
            with self.assertRaisesRegex(ValueError, "baseline changed"):
                n.finish(info, out)
            request = n.changelog_request(info)
            self.assertEqual(request["changes"], "")
            self.assertEqual(request["changed_files"], [])
        self.assertNotIn(info["tag"], self.releases)

    def test_stable_uses_previous_stable_not_the_promoted_nightly(self):
        info, _ = self.candidate()
        stable = {**info, "channel": "stable", "version": "5.1.0", "tag": "v5.1.0"}
        previous = {"tag_name": "v5.0.0", "body": "Source: " + self.source,
                    "prerelease": False, "draft": False}
        with patch.object(n, "github_json", return_value=previous) as read:
            request = n.changelog_request(stable)
        self.assertTrue(read.call_args.args[0].endswith("/releases/latest"))
        self.assertEqual(request["previous_tag"], "v5.0.0")
        self.assertEqual(request["changes"], "")
        self.assertEqual(request["changed_files"], [])

    def test_stable_publishes_approved_notes_and_completed_publish_is_retryable(self):
        (self.repo / n.CONFIG).write_text(n.json_text(
            {"plugins": {"updater": {"endpoints": n.COMPILED_ENDPOINTS["stable"]}}}))
        n.git("add", str(n.CONFIG))
        n.git("commit", "-qm", "stable-capable fixture endpoint")
        n.git("push", "-q", "origin", "main")
        nightly, out = self.candidate()
        self.approve(nightly)
        n.finish(nightly, out)
        decision = n.json_text([{"tag": nightly["tag"], "decided_on": "2026-09-11",
                                 "words": "Fixture promotion decision"}])
        stable = n.stable_plan(nightly["tag"], decision, now=NOW)
        n.git("checkout", "-q", "--detach", stable["source_commit"])
        stable = n.prepare(stable)
        out = Path(self.temp.name) / "stable"
        n.build(stable, out)
        def latest(*args, **kwargs):
            return next((r for r in self.releases.values() if not r["prerelease"]), None)
        with patch.object(n, "github_json", side_effect=latest):
            self.approve(stable)
            n.finish(stable, out)
            before = n._candidate_files(out)
            n.finish(stable, out)
        self.assertFalse(self.releases[stable["tag"]]["prerelease"])
        self.assertTrue(self.releases[stable["tag"]]["body"].startswith("- Fixed a lost message"))
        self.assertEqual(n._candidate_files(out), before)

    def test_draft_refuses_main_and_malformed_prose(self):
        info, _ = self.candidate()
        request = n.changelog_request(info)
        n.git("checkout", "-q", "main")
        with self.assertRaisesRegex(ValueError, "work branch"):
            n.write_changelog_draft(request, "- Fixed a lost message")
        for entry in ("", "Introduction\n- Fixed something", "- Internal details", "- Fixed good\n\n- Added bad",
                      "- [technical] Internal details", "- [Technical] Fixed a check"):
            with self.subTest(entry=entry), self.assertRaises(ValueError):
                n.validate_changelog_entry(entry)

    def test_draft_uses_the_commit_hooks_comma_rule_for_both_copies(self):
        info, _ = self.candidate()
        request = n.changelog_request(info)
        n.git("checkout", "-qb", "notes-comma", "origin/main")
        path = n.write_changelog_draft(request, "- Added Cut, Copy, and Paste")
        record = json.loads(path.read_text())[info["tag"]]
        self.assertEqual(record["entry"], "- Added Cut, Copy and Paste")
        document = (self.repo / n.CHANGELOG_FILES["nightly"]).read_text()
        self.assertIn(n.changelog_section(record).strip(), document)

    def test_changed_baseline_can_replace_unpublished_approved_notes_after_review(self):
        for _ in range(2):
            earlier, _ = self.candidate()
            n.git("checkout", "-q", "main")
        info, _ = self.candidate()
        self.approve(info)
        with patch.object(n, "published_changelog_base",
                          return_value=(earlier["tag"], earlier["source_commit"])):
            request = n.changelog_request(info)
            n.git("checkout", "-qb", "revised-notes", "origin/main")
            n.write_changelog_draft(request, "- Fixed another lost message")
            n.git("add", "docs")
            n.git("commit", "-qm", "reviewed revised notes")
            n.git("push", "-q", "origin", "HEAD:main")
            self.assertEqual(n.approved_changelog(info), "- Fixed another lost message")
        self.assertEqual(n.remote_tags(), set())

    def test_draft_cannot_rewrite_notes_after_the_release_body_appears(self):
        info, out = self.candidate()
        self.approve(info)
        n.finish(info, out)
        request = n.changelog_request(info)
        original = self.releases[info["tag"]]["body"]
        n.git("checkout", "-qb", "attempted-rewrite", "origin/main")
        with self.assertRaisesRegex(ValueError, "never rewrite"):
            n.write_changelog_draft(request, "- Changed the published notes")
        self.assertEqual(self.releases[info["tag"]]["body"], original)
        self.assertEqual(n.git("status", "--porcelain"), "")


class CommittedChangelogTests(unittest.TestCase):
    def test_entry_has_its_own_release_link_without_a_permanent_prepared_label(self):
        section = n.changelog_section({"version": "9.9.9", "tag": "v9.9.9", "entry": "- Fixed a lost message"})
        self.assertNotIn("Prepared", section)
        self.assertIn("https://github.com/RichSpark/RichOS/releases/tag/v9.9.9)", section)

    def test_technical_tag_marks_a_line_only_a_technical_person_would_care_about(self):
        entry = "- Added a file card\n- [technical] Fixed a teammate's command being refused"
        self.assertEqual(n.validate_changelog_entry(entry), entry)

    def test_every_approved_record_matches_its_public_user_facing_entry(self):
        records = json.loads((n.ROOT / n.CHANGELOG_RECORDS).read_text())
        self.assertIsInstance(records, dict)
        self.assertTrue(records)
        for tag, record in records.items():
            with self.subTest(tag=tag):
                self.assertEqual(record["tag"], tag)
                n.changelog_identity(record)
                n.validate_changelog_entry(record["entry"])
                document = (n.ROOT / n.CHANGELOG_FILES[record["channel"]]).read_text()
                # Published before the GitHub move, a section keeps its link on the old address.
                self.assertTrue(any(n.changelog_section(record, repo).strip() in document
                                    for repo in (n.REPO, n.FORMER_REPO)), n.changelog_section(record))
                if record["previous_tag"] is not None:
                    self.assertRegex(record["previous_source"], r"^[0-9a-f]{40}$")
                    pattern = ("v" + n.BASE_RE + r"-nightly\.[0-9]{8}\.[1-9][0-9]*"
                               if record["channel"] == "nightly" else "v" + n.BASE_RE)
                    self.assertRegex(record["previous_tag"], "^" + pattern + "$")


class EngineStorageTests(GitFixture):
    def archive(self, contents=b"candidate engine"):
        path = Path(self.temp.name) / "engine.tar.gz"
        path.write_bytes(contents)
        return path

    def test_upload_then_reuse_verifies_bytes_without_clobber(self):
        archive = self.archive()
        name, url = n.candidate_engine_asset(n.sha256_file(archive))
        n.stage_candidate_engine(archive, name, url)
        calls = self.stub_execute.call_count
        n.stage_candidate_engine(archive, name, url)
        self.assertEqual(self.stub_execute.call_count, calls)
        self.assertEqual(self.asset_bytes, {(n.CHANNEL_TAG, name): archive.read_bytes()})
        self.assertEqual(n.remote_tags(), set())
        self.assertEqual(set(self.releases), {n.CHANNEL_TAG})
        self.assertNotIn("--clobber", self.stub_execute.call_args.args)

    def test_existing_digest_name_with_wrong_bytes_is_never_overwritten(self):
        archive = self.archive()
        name, url = n.candidate_engine_asset(n.sha256_file(archive))
        self.asset_bytes[n.CHANNEL_TAG, name] = b"corrupt"
        with self.assertRaisesRegex(ValueError, "differs"):
            n.stage_candidate_engine(archive, name, url)
        self.stub_execute.assert_not_called()
        self.assertEqual(self.asset_bytes[n.CHANNEL_TAG, name], b"corrupt")

    def test_capacity_refuses_new_assets_but_allows_verified_reuse(self):
        archive = self.archive()
        name, url = n.candidate_engine_asset(n.sha256_file(archive))
        full = [{"name": f"old-{i}"} for i in range(998)]
        with patch.object(n, "release_assets", return_value=full):
            with self.assertRaisesRegex(ValueError, "capacity"):
                n.stage_candidate_engine(archive, name, url)
        self.stub_execute.assert_not_called()
        self.asset_bytes[n.CHANNEL_TAG, name] = archive.read_bytes()
        with patch.object(n, "release_assets", return_value=[*full, {"name": name}]):
            n.stage_candidate_engine(archive, name, url)
        self.stub_execute.assert_not_called()

    def test_missing_draft_or_immutable_channel_never_creates_a_release(self):
        archive = self.archive()
        name, url = n.candidate_engine_asset(n.sha256_file(archive))
        for release in (None, {"draft": True}, {"immutable": True}):
            with self.subTest(release=release), patch.object(n, "get_release", return_value=release):
                with self.assertRaisesRegex(ValueError, "existing public"):
                    n.stage_candidate_engine(archive, name, url)
        self.stub_execute.assert_not_called()

    def test_concurrent_identical_upload_is_verified_and_reused(self):
        archive = self.archive()
        name, url = n.candidate_engine_asset(n.sha256_file(archive))
        def race(*args, **kwargs):
            self.fake_external(*args, **kwargs)
            raise subprocess.CalledProcessError(1, args)
        with patch.object(n, "execute", side_effect=race):
            n.stage_candidate_engine(archive, name, url)
        self.assertEqual(self.asset_bytes[n.CHANNEL_TAG, name], archive.read_bytes())

    def test_failed_upload_is_not_mistaken_for_reuse(self):
        archive = self.archive()
        name, url = n.candidate_engine_asset(n.sha256_file(archive))
        with patch.object(n, "execute", side_effect=subprocess.CalledProcessError(1, "gh")):
            with self.assertRaises(subprocess.CalledProcessError):
                n.stage_candidate_engine(archive, name, url)
        self.assertEqual(self.asset_bytes, {})

    def test_channel_promotion_preserves_all_older_engine_downloads(self):
        retained = {}
        for contents in (b"engine one", b"engine two"):
            archive = self.archive(contents)
            name, url = n.candidate_engine_asset(n.sha256_file(archive))
            n.stage_candidate_engine(archive, name, url)
            retained[n.CHANNEL_TAG, name] = contents
            info = n.prepare(self.plan(force=True))
            n.promote(info, self.manifest(info))
            n.git("checkout", "-q", "main")
            for key, expected in retained.items():
                self.assertEqual(self.asset_bytes[key], expected)
        self.assertIn((n.CHANNEL_TAG, "latest.json"), self.asset_bytes)


class AssetApiTests(unittest.TestCase):
    def test_inventory_uses_every_page(self):
        with patch.object(n, "run", return_value=json.dumps([[{"name": "first"}], [{"name": "last"}]])) as run:
            self.assertEqual(n.release_assets(123), [{"name": "first"}, {"name": "last"}])
        self.assertIn("--paginate", run.call_args.args)
        self.assertIn("--slurp", run.call_args.args)

    def test_only_404_is_a_missing_release(self):
        for status in ("404", "403", "500"):
            result = subprocess.CompletedProcess([], 1, json.dumps({"status": status}), "failed")
            with self.subTest(status=status), patch.object(n.subprocess, "run", return_value=result):
                if status == "404":
                    self.assertIsNone(n.get_release("absent"))
                else:
                    with self.assertRaises(subprocess.CalledProcessError):
                        n.get_release("absent")

    def test_invalid_success_response_cannot_trigger_release_creation(self):
        result = subprocess.CompletedProcess([], 0, "not json", "")
        with patch.object(n.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(ValueError, "invalid release response"):
                n.get_release("absent")

    def test_readback_compares_downloaded_bytes_not_the_asset_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "archive"
            archive.write_bytes(b"expected")
            def download(*args):
                Path(args[args.index("--output") + 1]).write_bytes(b"wrong")
            with patch.object(n, "execute", side_effect=download):
                with self.assertRaisesRegex(ValueError, "differs"):
                    n.verify_served_asset("https://example.invalid/digest-name", archive)


class OneCodePathTests(unittest.TestCase):
    """The smoke and the release run the SAME functions, and that is not a convention.

    T3 Code has the same smoke in spirit and it has already drifted from the release it
    claims to exercise -- `release.yml:795` passes two positional arguments in one order,
    `release-smoke.ts:328-331` passes them in the other. Nothing told them. These cases
    are what tells us.
    """

    def smoke(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(io.StringIO()) as out:
            n.release_smoke(Path(tmp))
            # The fixtures are gone before the directory that held them is (CEO §54).
            self.assertEqual(list(Path(tmp).iterdir()), [])
        return out.getvalue()

    def test_the_smoke_reaches_every_registered_release_only_step(self):
        self.smoke()
        unreached = sorted(name for name, calls in n.RELEASE_STEP_CALLS.items() if not calls)
        self.assertEqual(unreached, [])
        self.assertTrue(n.RELEASE_STEPS, "no release-only step is registered at all")

    def test_a_registered_step_the_smoke_never_calls_turns_the_gate_red(self):
        """PROVED BY DEFEAT, which is the only way a guarantee like this is worth stating.

        Everything else here asserts the mechanism's answer today. This asserts it would
        NOTICE -- register a release-only step, do not smoke it, and the build fails
        naming the function. Without this case, the completeness check could be deleted
        and every other case in this class would still pass.
        """
        @n.release_step
        def decoy_step_nothing_smokes(value):
            return value

        self.addCleanup(n.RELEASE_STEPS.pop, "decoy_step_nothing_smokes", None)
        self.addCleanup(n.RELEASE_STEP_CALLS.pop, "decoy_step_nothing_smokes", None)
        with self.assertRaises(ValueError) as caught:
            self.smoke()
        self.assertIn("decoy_step_nothing_smokes", str(caught.exception))
        self.assertIn("never reached", str(caught.exception))

    def test_the_real_release_path_calls_the_steps_rather_than_copying_them(self):
        """Every registered step is named in the body of the code a real release runs.

        The failure this prevents is not a rename -- Python catches that with a
        NameError. It is the one T3 has: somebody re-implements a step inline "just for
        this path", the smoke keeps exercising the old function, and both are green.
        A step no real release entry point mentions is a step the smoke is alone with.
        """
        real = "".join(inspect.getsource(f) for f in
                       (n.plan, n.prepare, n.promote, n.build, n.finish, n.stable_plan,
                        n.read_changelog_record))
        for name in n.RELEASE_STEPS:
            with self.subTest(step=name):
                self.assertIn(name, real,
                              f"{name} is registered as a release-only step but no real "
                              "release path calls it")

    def test_the_smoke_exercises_refusals_and_not_only_happy_paths(self):
        """A step that only ever sees valid input proves its refusals compile."""
        refused = [line for line in self.smoke().splitlines() if "refused:" in line]
        self.assertGreaterEqual(len(refused), 20, refused)


class PromotionRecordTests(unittest.TestCase):
    """The COMMITTED record, not a fixture of one.

    Every other case about promotions passes `promotion_decision` text it wrote itself,
    which proves the parser and says nothing about the file the command actually reads.
    A record that has stopped being readable JSON refuses every stable release, and the
    place to find that out is here rather than on the morning he decides to promote one.
    """

    def record(self):
        return json.loads((n.ROOT / n.PROMOTIONS).read_text())

    def test_the_committed_record_is_a_list_this_command_can_read(self):
        self.assertIsInstance(self.record(), list)

    def test_every_recorded_decision_is_one_this_command_accepts(self):
        """Each entry is fed to the real check, so a malformed one cannot sit unnoticed.

        Empty today, and that is the correct state: nothing has been promoted. This case
        is written for the day it is not empty, and it costs nothing until then.
        """
        text = (n.ROOT / n.PROMOTIONS).read_text()
        for entry in self.record():
            with self.subTest(tag=entry.get("tag")):
                accepted = n.promotion_decision(entry["tag"], text)
                self.assertEqual(accepted["tag"], entry["tag"])
                self.assertTrue(accepted["words"].strip())
                # A promotion names a NIGHTLY, never a stable tag or a branch.
                n.nightly_key(entry["tag"].removeprefix("v"))


class StableChannelTests(GitFixture):
    """Stable is a REBUILD of a published nightly's commit, never its bytes re-tagged.

    T3's sentence, which is the thing being copied (`release.yml:44-47`): *"Manual stable
    releases build the commit of the latest published nightly, so stable only ever ships
    a build that nightly users have already run."*
    """

    def setUp(self):
        super().setUp()
        # The base fixture compiles in a deliberately wrong endpoint so the nightly cases
        # can assert it gets replaced. A STABLE release keeps the checked-in one, so for
        # these cases the fixture carries what the real tree carries.
        (self.repo / n.CONFIG).write_text(n.json_text(
            {"plugins": {"updater": {"endpoints": n.COMPILED_ENDPOINTS["stable"]}}}))
        n.git("add", ".")
        n.git("commit", "-qm", "stable endpoint")
        n.git("push", "-q", "origin", "main")
        self.source = n.git("rev-parse", "HEAD")

    DECISION = [{"tag": "v5.1.0-nightly.20260911.1", "decided_on": "2026-09-20",
                 "words": "fixture standing in for his sentence"}]

    def publish_a_nightly(self):
        """A published nightly in the fixture remote, with its provenance in its tag."""
        info = n.prepare(self.plan())
        n.ensure_version_tag(info)
        n.git("checkout", "-q", "main")
        return info

    def test_a_stable_release_rebuilds_the_nightlys_commit_with_its_own_version(self):
        info = self.publish_a_nightly()
        plan = n.stable_plan(info["tag"], n.json_text(self.DECISION), now=NOW)
        self.assertEqual(plan["source_commit"], self.source)
        self.assertEqual(plan["channel"], "stable")
        # The tree's own version, NOT the nightly's prerelease version.
        self.assertEqual(plan["version"], "5.1.0")
        self.assertEqual(plan["tag"], "v5.1.0")
        self.assertEqual(plan["endpoint"], n.STABLE_ENDPOINT)
        self.assertEqual(plan["promoted_from"], info["tag"])
        self.assertEqual(plan["promoted_from_version"], info["version"])
        # His decision travels into the release's provenance, so the record of why this
        # shipped is inside the thing that shipped.
        self.assertEqual(plan["promotion_words"], self.DECISION[0]["words"])

    def test_the_stable_build_compiles_in_the_stable_endpoint_and_no_prerelease_version(self):
        """The two facts that make re-tagging a nightly's bytes a broken stable release."""
        info = self.publish_a_nightly()
        plan = n.stable_plan(info["tag"], n.json_text(self.DECISION), now=NOW)
        files = n.release_files(plan, (self.repo / n.MANIFEST).read_text(),
                                (self.repo / n.LOCK).read_text(),
                                (self.repo / n.CONFIG).read_text())
        compiled = json.loads(files[str(n.CONFIG)])["plugins"]["updater"]["endpoints"]
        self.assertEqual(compiled, n.COMPILED_ENDPOINTS["stable"])
        self.assertNotIn("nightly", compiled[0])
        self.assertIn('version = "5.1.0"\n', files[str(n.MANIFEST)])
        self.assertNotIn("-nightly.", files[str(n.MANIFEST)])
        self.assertNotIn("-nightly.", files[str(n.LOCK)])

    def test_tooling_that_spends_the_public_tag_in_prepare_cannot_build_a_stable_release(self):
        """R05: the historical commit's own prepare is what runs, so it must hold the fix."""
        info = self.publish_a_nightly()
        # The stable-capable but pre-reservation tooling: it names the channel and nothing else.
        old = self.repo / n.APP / "scripts/nightly.py"
        old.write_text("CHANNEL_ENDPOINTS = {}  # pre-reservation tooling\n")
        n.git("add", ".")
        n.git("commit", "-qm", "tooling before the hidden reservation")
        n.git("push", "-q", "origin", "main")
        # Re-point the nightly's provenance at that commit by publishing a fresh nightly.
        second = n.prepare(self.plan(force=True))
        n.ensure_version_tag(second)
        n.git("checkout", "-q", "main")
        decision = n.json_text([{**self.DECISION[0], "tag": second["tag"]}])
        with self.assertRaisesRegex(ValueError, "reserves the public version tag"):
            n.stable_plan(second["tag"], decision, now=NOW)
        self.assertNotEqual(info["tag"], second["tag"])

    def test_no_decision_of_his_means_no_stable_release(self):
        """CEO §69, and the substitutes it names are each refused here by construction."""
        info = self.publish_a_nightly()
        with self.assertRaisesRegex(ValueError, "no promotion decision"):
            n.stable_plan(info["tag"], "[]", now=NOW)
        # A decision about a DIFFERENT nightly is not a decision about this one.
        other = n.json_text([{**self.DECISION[0], "tag": "v5.1.0-nightly.20260911.9"}])
        with self.assertRaisesRegex(ValueError, "no promotion decision"):
            n.stable_plan(info["tag"], other, now=NOW)

    def test_his_words_on_the_command_line_are_his_decision_with_no_record_committed_first(self):
        """CEO, 2026-10-19, after nightly 65's record needed its own branch, review and merge:
        "Will this whole retarded shitshow be repeated all over again?" His words are given
        with the command, dated today, checked by the same rules, and committed in the
        stable release commit's own promotion record."""
        info = self.publish_a_nightly()
        words = "fixture standing in for his sentence, given on the command line"
        plan = n.stable_plan(info["tag"], "[]", now=NOW, words=words)
        self.assertEqual(plan["promotion_words"], words)
        self.assertEqual(plan["promotion_decided_on"], NOW.astimezone().strftime("%Y-%m-%d"))
        built = n.prepare(plan)
        record = json.loads(n.git("show", f"{built['build_commit']}:{n.PROMOTIONS}"))
        self.assertEqual(record, [{"tag": info["tag"], "decided_on": plan["promotion_decided_on"],
                                   "words": words}])
        self.assertEqual(json.loads(n.git("show", f"{built['build_commit']}:{n.PROVENANCE}"))
                         ["promotion_words"], words)
        # The same rules as a written entry: no words is no decision, and a nightly the
        # record already decides on is not decided twice.
        n.git("checkout", "-q", "main")
        for empty in ("", "   "):
            with self.subTest(words=empty), self.assertRaisesRegex(ValueError, "missing words"):
                n.stable_plan(info["tag"], "[]", now=NOW, words=empty)
        with self.assertRaisesRegex(ValueError, "leave out --ceo-words"):
            n.stable_plan(info["tag"], n.json_text([{**self.DECISION[0], "tag": info["tag"]}]),
                          now=NOW, words=words)

    def test_a_written_record_still_promotes_and_is_carried_into_the_release_commit(self):
        info = self.publish_a_nightly()
        decision = [{**self.DECISION[0], "tag": info["tag"]}]
        built = n.prepare(n.stable_plan(info["tag"], n.json_text(decision), now=NOW))
        record = json.loads(n.git("show", f"{built['build_commit']}:{n.PROMOTIONS}"))
        self.assertEqual(record, decision)

    def test_a_tag_that_is_not_one_of_our_nightlies_is_refused(self):
        self.publish_a_nightly()
        n.git("tag", "v9.9.9-nightly.20260911.1", "main")
        n.git("push", "-q", "origin", "v9.9.9-nightly.20260911.1")
        decision = n.json_text([{**self.DECISION[0], "tag": "v9.9.9-nightly.20260911.1"}])
        with self.assertRaises(ValueError):
            n.stable_plan("v9.9.9-nightly.20260911.1", decision, now=NOW)

    def test_a_version_that_already_shipped_cannot_ship_again(self):
        info = self.publish_a_nightly()
        n.git("tag", "v5.1.0", "main")
        n.git("push", "-q", "origin", "v5.1.0")
        with self.assertRaisesRegex(ValueError, "already exists"):
            n.stable_plan(info["tag"], n.json_text(self.DECISION), now=NOW)

    def test_a_stable_release_never_calls_itself_a_nightly(self):
        """The label was hard-coded "Nightly" and reached two public surfaces.

        The releases page title and notes, and the `--notes` string `make-release.sh`
        puts in the bundle. A stable release announcing itself as "Nightly 1.2.0" is the
        kind of defect nobody finds in a test of the version number, because the version
        number is right.
        """
        info = self.publish_a_nightly()
        self.assertEqual(n.describe_release(info), f"Nightly {info['version']}")
        plan = n.stable_plan(info["tag"], n.json_text(self.DECISION), now=NOW)
        label = n.describe_release(plan)
        self.assertEqual(label, "RichOS 5.1.0")
        self.assertNotIn("Nightly", label)
        self.assertNotIn("nightly", label)

    def test_publishing_stable_flips_the_release_and_leaves_the_nightly_channel_alone(self):
        """The channel move for stable is GitHub's own `latest` pointer, nothing else.

        Assets go up while the release is still a PRERELEASE -- invisible to every
        installed copy however long that takes -- and one `gh release edit` is the flip.
        The nightly channel must not move: promoting a build to stable says nothing about
        what nightly users should receive.
        """
        info = self.publish_a_nightly()
        plan = n.stable_plan(info["tag"], n.json_text(self.DECISION), now=NOW)
        before, _ = n.channel()
        with patch.object(n, "execute") as execute:
            n.promote_stable(plan)
        flips = [call.args for call in execute.call_args_list]
        self.assertEqual(len(flips), 1, flips)
        self.assertEqual(flips[0][:3], ("gh", "release", "edit"))
        self.assertIn("--latest", flips[0])
        self.assertIn("--prerelease=false", flips[0])
        self.assertIn("v5.1.0", flips[0])
        self.assertEqual(n.channel()[0], before, "the nightly channel must not move")
        # And the rule that keeps branches off the public repository is re-read first.
        self.assertTrue(self.stub_verify_repository_rules.called)

    def fail_the_app_build(self):
        """Make `make-release.sh app` fail, where a real stable build dies: compiling,
        signing or notarizing. Returns the fixture's normal side effect to restore."""
        normal = self.stub_execute.side_effect

        def failing(*args, **kwargs):
            if args[0] == "bash" and args[2] == "app":
                raise subprocess.CalledProcessError(1, args)
            return normal(*args, **kwargs)

        self.stub_execute.side_effect = failing
        return normal

    def remote_tag(self, tag):
        return n.git("ls-remote", "origin", f"refs/tags/{tag}").split()[:1]

    def test_a_stable_build_that_fails_before_publishing_can_be_retried(self):
        """Hunt part 2, section 05: a failed stable build used up its version.

        `prepare` pushed the real `v<version>` tag before compiling and `build` opened its
        release entry before compiling, and `stable_plan` read that tag as "shipped". So a
        build that died at compile or notarization, having released nothing, left a version
        that could only be abandoned. The version is now reserved the way a nightly reserves
        its candidate, out of every public list, and the tag is created only by `finish`.
        """
        info = self.publish_a_nightly()
        decision = n.json_text(self.DECISION)
        # The runner stages every attempt of one stable version in the same directory
        # (`nightly-local.py`: state/releases/<tag>), so the retry reuses it here too.
        out = Path(self.temp.name) / "releases" / "v5.1.0"
        normal = self.fail_the_app_build()
        failed = n.prepare(n.stable_plan(info["tag"], decision, now=NOW))
        with self.assertRaises(subprocess.CalledProcessError):
            n.build(failed, out)
        # Nothing shipped, and nothing public claims otherwise: no tag, no release entry.
        self.assertEqual(self.remote_tag("v5.1.0"), [])
        self.assertNotIn("v5.1.0", self.releases)

        n.git("checkout", "-q", "main")
        self.stub_execute.side_effect = normal
        retry = n.prepare(n.stable_plan(info["tag"], decision, now=NOW))
        self.assertEqual(retry["promoted_from"], info["tag"])
        n.build(retry, out)
        self.assertEqual(self.remote_tag("v5.1.0"), [], "the tag waits for the publish step")
        n.finish(retry, out)
        self.assertEqual(self.remote_tag("v5.1.0"), [retry["build_commit"]])
        flips = [c.args for c in self.stub_execute.call_args_list
                 if c.args[:3] == ("gh", "release", "edit") and "v5.1.0" in c.args]
        self.assertEqual(len(flips), 1, flips)
        self.assertIn("--latest", flips[0])

        # Published is final: no new plan, and no attempt planned earlier, can replace it.
        n.git("checkout", "-q", "main")
        with self.assertRaisesRegex(ValueError, "already exists"):
            n.stable_plan(info["tag"], decision, now=NOW)

    def test_two_stable_builds_of_one_version_cannot_both_proceed(self):
        info = self.publish_a_nightly()
        decision = n.json_text(self.DECISION)
        first_plan = n.stable_plan(info["tag"], decision, now=NOW)
        second_plan = n.stable_plan(info["tag"], decision, now=NOW)
        first = n.prepare(first_plan)
        n.git("checkout", "-q", "main")
        # Planned from the same observation: the lease no longer matches.
        with self.assertRaisesRegex(ValueError, "reserved the version first"):
            n.prepare(second_plan)
        self.assertEqual(n.remote_ref(first["reservation_ref"]), first["build_commit"])

        # A build planned AFTER the first reserved takes the version over, and from that
        # moment the first can neither build nor publish.
        first_out = Path(self.temp.name) / "first"
        second = n.prepare(n.stable_plan(info["tag"], decision, now=NOW))
        n.git("checkout", "-q", "--detach", first["build_commit"])
        with self.assertRaisesRegex(ValueError, "no longer names this build"):
            n.build(first, first_out)
        self.assertFalse(first_out.exists())

        # The holder publishes; nothing planned before that can replace it afterwards.
        n.git("checkout", "-q", "--detach", second["build_commit"])
        second_out = Path(self.temp.name) / "second"
        n.build(second, second_out)
        n.finish(second, second_out)
        self.assertEqual(self.remote_tag("v5.1.0"), [second["build_commit"]])
        n.git("checkout", "-q", "main")
        with self.assertRaisesRegex(ValueError, "already reserved or its stable release exists"):
            n.prepare(first_plan)
        # Even an attempt that still somehow held the reservation cannot move the tag.
        with self.assertRaisesRegex(ValueError, "already published from another build"):
            n.stable_reservation_holds(first, first["build_commit"],
                                       self.remote_tag("v5.1.0")[0])

    def test_a_retry_leaves_unrelated_staging_alone(self):
        """Only a superseded attempt of THIS version is cleared; anything else still refuses."""
        info = self.publish_a_nightly()
        attempt = n.prepare(n.stable_plan(info["tag"], n.json_text(self.DECISION), now=NOW))
        out = Path(self.temp.name) / "releases" / "v5.1.0"
        out.mkdir(parents=True)
        (out / "keep.txt").write_text("not a stable attempt\n")
        with self.assertRaises(FileExistsError):
            n.build(attempt, out)
        self.assertEqual((out / "keep.txt").read_text(), "not a stable attempt\n")


class GitHubAccessTests(unittest.TestCase):
    """Run 20261014T135757Z-c7955224 passed every gate, then refused at plan-recheck because
    GitHub refused ONE `git ls-remote --tags origin`; a minute later it worked from a shell."""
    DENIED = ("git@github.com: Permission denied (publickey).\nfatal: Could not read from remote "
              "repository.\n\nPlease make sure you have the correct access rights\nand the "
              "repository exists.\n")

    def reads(self, call, *outcomes):
        """`call()` with GitHub giving `outcomes` in turn: (exit, stdout, stderr) or "timeout"."""
        argvs = []

        def fake(argv, **kwargs):
            argvs.append(argv)
            outcome = outcomes[len(argvs) - 1]
            if outcome == "timeout":
                raise subprocess.TimeoutExpired(argv, kwargs["timeout"])
            return subprocess.CompletedProcess(argv, *outcome)
        with patch.object(n.subprocess, "run", side_effect=fake), \
                patch.object(n.github_access, "sleep") as slept, \
                contextlib.redirect_stderr(io.StringIO()) as said:
            try:
                return call(), argvs, [c.args[0] for c in slept.call_args_list], said.getvalue()
            except subprocess.CalledProcessError as error:
                return error, argvs, [c.args[0] for c in slept.call_args_list], said.getvalue()

    def test_a_refusal_or_a_timeout_that_passes_is_tried_again(self):
        tags, argvs, slept, said = self.reads(
            n.remote_tags, (128, "", self.DENIED), "timeout", (0, "abc\trefs/tags/v1.0.0\n", ""))
        self.assertEqual(tags, {"v1.0.0"})
        self.assertEqual(argvs, [("git", "ls-remote", "--tags", "origin")] * 3)
        self.assertEqual(slept, list(n.github_access.WAITS[:2]))
        self.assertIn("GitHub access failed", said)

    def test_a_refusal_that_lasts_says_github_access_with_sshs_own_error(self):
        attempts = len(n.github_access.WAITS) + 1
        error, argvs, slept, _ = self.reads(n.remote_tags, *[(128, "", self.DENIED)] * attempts)
        self.assertEqual(len(argvs), attempts)
        self.assertTrue(45 <= sum(slept) <= 90, slept)          # about a minute
        self.assertIsInstance(error, subprocess.CalledProcessError)
        self.assertIn("GitHub access refused: `git ls-remote --tags origin`", str(error))
        self.assertIn("git@github.com: Permission denied (publickey).", str(error))

    def test_github_answering_is_not_tried_again(self):
        missing = "fatal: couldn't find remote ref refs/tags/v9\n"
        error, argvs, slept, said = self.reads(
            lambda: n.git("fetch", "--no-tags", "origin", "refs/tags/v9"), (128, "", missing))
        self.assertEqual((len(argvs), slept), (1, []))
        self.assertNotIsInstance(error, n.GitHubRefused)
        self.assertEqual(error.returncode, 128)
        self.assertIn(missing, said)


class RepositoryRuleTests(unittest.TestCase):
    """The publisher reads the branch ruleset back before it publishes anything.

    On 2026-09-16 at 21:57Z a Codex run authenticated as the repository owner added
    `refs/heads/nightly-channel` to the `main-only` ruleset's exclusion list so this
    publisher could push a channel branch; at 02:53Z the first nightly created it and
    the public repository page read "2 Branches" (CEO 2026-09-13, ceo-decisions 38:
    *"what is anything other than the `main` doing on GitHub?"*). The ruleset refused
    nothing because nothing asked it. These tests are about the asking.
    """
    GOOD = {"id": 23194738, "name": "main-only", "target": "branch",
            "enforcement": "active", "bypass_actors": [],
            "conditions": {"ref_name": {"include": ["~ALL"], "exclude": ["refs/heads/main"]}},
            "rules": [{"type": "creation"}, {"type": "update"}]}

    def check(self, ruleset=None, listing=None):
        """Run the real read-back against a ruleset the fake `gh` serves."""
        ruleset = self.GOOD if ruleset is None else ruleset
        def fake_run(*args, **kwargs):
            self.assertEqual(args[:2], ("gh", "api"))
            if args[2].endswith("/rulesets"):
                return json.dumps(self.GOOD if listing is None else listing)
            self.assertEqual(args[2], f"repos/{n.REPO}/rulesets/{ruleset['id']}")
            return json.dumps(ruleset)
        with patch.object(n, "run", side_effect=fake_run):
            n.verify_repository_rules()

    def test_the_rule_as_the_ceo_set_it_passes(self):
        """Negative control: without this, every assertion below could pass vacuously."""
        self.check(listing=[self.GOOD])

    def test_a_widened_exclusion_list_refuses_the_publish(self):
        """THE positive control, and the exact shape of what happened on 2026-09-16."""
        widened = json.loads(json.dumps(self.GOOD))
        widened["conditions"]["ref_name"]["exclude"].append("refs/heads/nightly-channel")
        with self.assertRaisesRegex(ValueError, "exempt branches"):
            self.check(widened, listing=[widened])

    def test_every_other_way_to_make_the_ban_a_no_op_also_refuses(self):
        """An exclusion list that reads correctly is not on its own the rule."""
        cases = {
            "enforcement": ({"enforcement": "evaluate"}, "not 'active'"),
            "bypass": ({"bypass_actors": [{"actor_id": 5, "actor_type": "RepositoryRole"}]},
                       "bypass actor"),
            "narrowed": ({"conditions": {"ref_name": {"include": ["refs/heads/release/*"],
                                                      "exclude": ["refs/heads/main"]}}},
                         r"not \['~ALL'\]"),
            "creation": ({"rules": [{"type": "update"}]}, "does not restrict creation"),
            "update": ({"rules": [{"type": "creation"}]}, "does not restrict update"),
        }
        for name, (override, expected) in cases.items():
            with self.subTest(case=name):
                broken = {**json.loads(json.dumps(self.GOOD)), **override}
                with self.assertRaisesRegex(ValueError, expected):
                    self.check(broken, listing=[broken])

    def test_a_missing_or_duplicated_ruleset_refuses(self):
        for listing in ([], [self.GOOD, {**self.GOOD, "id": 99}]):
            with self.subTest(count=len(listing)):
                with self.assertRaisesRegex(ValueError, "exactly one branch ruleset"):
                    self.check(listing=listing)

    def test_a_ruleset_of_another_name_or_target_is_not_this_rule(self):
        for other in ({"name": "something-else"}, {"target": "tag"}):
            with self.subTest(**other):
                with self.assertRaisesRegex(ValueError, "exactly one branch ruleset"):
                    self.check(listing=[{**self.GOOD, **other}])


class ChannelShapeTests(unittest.TestCase):
    def test_the_channel_is_a_release_tag_and_its_endpoint_is_a_release_asset(self):
        """The CEO's rule, asserted rather than left to a comment.

        `main` and release tags, nothing else. A regression here does not break a test
        somewhere downstream -- it silently puts a branch back on the public repository,
        which is the thing that has now happened once.
        """
        self.assertTrue(n.CHANNEL_REF.startswith("refs/tags/"), n.CHANNEL_REF)
        self.assertNotIn("refs/heads/", n.CHANNEL_REF)
        self.assertEqual(n.CHANNEL_REF, f"refs/tags/{n.CHANNEL_TAG}")
        self.assertEqual(
            n.ENDPOINT,
            f"https://github.com/{n.REPO}/releases/download/{n.CHANNEL_TAG}/latest.json")
        self.assertNotIn("raw.githubusercontent.com", n.ENDPOINT)


class ReleaseScriptTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.cargo_cache = tempfile.TemporaryDirectory()
        self.addCleanup(self.cargo_cache.cleanup)
        root = Path(self.temp.name)
        self.scripts = root / 'richos/app/scripts'
        self.scripts.mkdir(parents=True)
        for name in ('make-release.sh', 'nightly.py'):
            shutil.copy(Path(__file__).with_name(name), self.scripts / name)
        # Every library make-release.sh sources, read from the script itself so a new
        # `. "$here/lib/x.sh"` line needs no edit here.
        release_text = (self.scripts / 'make-release.sh').read_text()
        for lib in sorted(set(re.findall(r'^\s*\.\s+"\$here/(lib/[\w.-]+\.sh)"', release_text, re.M))):
            (self.scripts / lib).parent.mkdir(exist_ok=True)
            shutil.copy(Path(__file__).with_name(lib.split('/')[0]) / Path(lib).name,
                        self.scripts / lib)
        # lib/cargo-target.sh in turn sources lib/cargo-cache-env.sh and names the bundle
        # directory through ../bin/cargo (lib/cargo_identity.py). Without them the output
        # lock resolves to "/release" and `app` refuses before packaging.
        for rel in ('lib/cargo-cache-env.sh', 'lib/cargo_identity.py', 'lib/machine_storage.py',
                    'lib/github_access.py', 'bin/cargo'):
            (self.scripts / rel).parent.mkdir(exist_ok=True)
            shutil.copy2(Path(__file__).parent / rel, self.scripts / rel)
        self.config = root / n.CONFIG
        self.config.parent.mkdir(parents=True)
        (root / 'richos/engine').mkdir()
        (root / 'richos/engine/VERSION').write_text('1.2.0\n')
        engine_lib = root / 'richos/engine/scripts/lib'
        engine_lib.mkdir(parents=True)
        for name in ('machine_settings.py', 'machine-settings.sh'):
            shutil.copy2(Path(__file__).resolve().parents[2] / 'engine/scripts/lib' / name, engine_lib / name)
        self.version = '5.1.0-nightly.20260911.10'
        (root / n.MANIFEST).write_text(f'[package]\nversion = "{self.version}"\n')
        self.config.write_text(n.json_text({'plugins': {'updater': {'endpoints': [n.ENDPOINT]}}}))
        self.out = root / 'out'
        self.out.mkdir()
        self.remote = root / 'remote'
        self.remote.mkdir()
        self.bin = root / 'bin'
        self.bin.mkdir()
        curl = self.bin / 'curl'
        curl.write_text('''#!/usr/bin/env python3
import os, shutil, sys
from pathlib import Path
args = sys.argv[1:]
dest = Path(args[args.index('-o') + 1])
url = args[-1]
# verify-assets must not fetch or require the live channel yet. Since 2026-09-17 the
# channel endpoint is ITSELF a `/releases/download/` URL ending in `latest.json`, so
# the old substring test would have passed it and then served the immutable release's
# manifest in its place. The channel is named exactly and refused by name.
if url == os.environ['FAKE_CHANNEL_ENDPOINT'] or '/releases/download/' not in url:
    # make-release.sh discards curl's stderr, so a refusal is recorded where a test
    # can see it rather than only inferred from the command failing.
    # No backslash escape here: this script is written from a triple-quoted literal,
    # so an escape would be expanded by the test rather than by this file.
    with open(os.environ['FAKE_REFUSALS'], 'a') as log:
        print(url, file=log)
    raise SystemExit('unexpected channel fetch before promotion')
source = Path(os.environ['FAKE_REMOTE']) / url.rsplit('/', 1)[-1]
if source.exists():
    shutil.copy(source, dest)
    print('200', end='')
else:
    print('404', end='')
''')
        curl.chmod(0o755)
        cargo = self.bin / 'cargo'
        cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$FAKE_CARGO_ARGS"\nexit "${FAKE_CARGO_EXIT:-0}"\n')
        cargo.chmod(0o755)
        self.env = {**os.environ, 'PATH': str(self.bin) + ':' + os.environ['PATH'],
                    'FAKE_REMOTE': str(self.remote), 'FAKE_CARGO_ARGS': str(root / 'cargo-args'),
                    'FAKE_CHANNEL_ENDPOINT': n.ENDPOINT,
                    'FAKE_REFUSALS': str(root / 'refused-urls'),
                    # The bundle output lock lives in the test's own directory, outside the
                    # fixture's Git tree and never in the shared Cargo cache.
                    'RICHOS_CARGO_CACHE_ROOT': self.cargo_cache.name}
        self.assets()

    def test_candidate_engine_pin_and_receipt_keep_the_digest_url(self):
        self.candidate_engine_pin()

    def test_a_stable_build_pins_the_digest_named_engine_too(self):
        """A per-version engine URL needs the public stable release before the compile.

        That is what made a failed stable build spend its version (hunt part 2, section
        05), so a stable build takes the same digest-named engine a nightly candidate does.
        """
        root = self.scripts.parents[2]
        (root / n.MANIFEST).write_text('[package]\nversion = "5.1.0"\n')
        self.config.write_text(n.json_text(
            {"plugins": {"updater": {"endpoints": n.COMPILED_ENDPOINTS["stable"]}}}))
        url = self.candidate_engine_pin()
        self.assertNotIn('/v5.1.0/', url)

    def candidate_engine_pin(self):
        checker = self.scripts.parents[1] / "engine/scripts/named-persons.sh"
        checker.parent.mkdir(parents=True, exist_ok=True)
        checker.write_text("#!/bin/sh\nexit 0\n")
        checker.chmod(0o755)
        generator = self.scripts / "make-engine-asset.sh"
        generator.write_text('''#!/bin/bash
while [ "$#" -gt 0 ]; do
  case "$1" in
    --out) dest="$2"; shift 2 ;;
    --tag) tag="$2"; shift 2 ;;
    *) shift ;;
  esac
done
printf 'fixture engine bytes' > "$dest/richos-engine-1.2.0.tar.gz"
digest=$(shasum -a 256 "$dest/richos-engine-1.2.0.tar.gz" | awk '{print $1}')
cat > "$dest/engine-pin.env" <<EOF
export RICHOS_ENGINE_VERSION=1.2.0
export RICHOS_ENGINE_URL=https://github.com/RichSpark/RichOS/releases/download/$tag/richos-engine-1.2.0.tar.gz
export RICHOS_ENGINE_SHA256=$digest
EOF
''')
        result = subprocess.run(['bash', str(self.scripts / 'make-release.sh'), 'engine',
                                 '--out', str(self.out), '--candidate-engine'],
                                env=self.env, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        archive = self.out / 'richos-engine-1.2.0.tar.gz'
        name, url = n.candidate_engine_asset(n.sha256_file(archive))
        pin = self.out / 'engine-pin.env'
        self.assertIn(f'export RICHOS_ENGINE_URL={url}\n', pin.read_text())
        shutil.copy(archive, self.remote / name)
        verified = self.call('verify-engine')
        self.assertEqual(verified.returncode, 0, verified.stdout + verified.stderr)
        self.assertIn(f'url={url}\n', (self.out / 'engine-published.ok').read_text())
        pin.write_text(pin.read_text().replace(url, url + '-changed'))
        refused = self.call('app')
        self.assertNotEqual(refused.returncode, 0)
        self.assertIn('URL changed after verification', refused.stderr)
        return url

    def assets(self, manifest_version=None):
        values = {'latest.json': n.json_text({'version': manifest_version or self.version}),
                  'RichOS.app.tar.gz': 'fixture artifact', 'RichOS.app.tar.gz.sig': 'fixture signature'}
        sums = ''
        for name, body in values.items():
            (self.remote / name).write_text(body)
            sums += f'{hashlib.sha256(body.encode()).hexdigest()}  {name}\n'
        (self.out / 'SHA256SUMS').write_text(sums)

    def call(self, command, **env):
        return subprocess.run(['bash', str(self.scripts / 'make-release.sh'), command,
                               '--out', str(self.out)], env={**self.env, **env},
                              capture_output=True, text=True)

    def app_source_fixture(self):
        """Reach the real app source guard with a real Git checkout, not a Git stub."""
        root = self.scripts.parents[2]
        checker = root / 'richos/engine/scripts/named-persons.sh'
        checker.parent.mkdir(parents=True, exist_ok=True)
        checker.write_text('#!/bin/sh\nexit 0\n')
        checker.chmod(0o755)
        (self.scripts / 'package-app.sh').write_text(
            '#!/bin/sh\necho "fixture reached packaging" >&2\nexit 23\n')
        # Match the real repository's generated-bytecode exclusions. make-release
        # imports nightly before checking Git status, even in a clean fixture.
        (root / '.gitignore').write_text('/out/\n/remote/\n/bin/\n__pycache__/\n*.pyc\n')
        digest = 'a' * 64
        url = n.candidate_engine_asset(digest)[1]
        (self.out / 'engine-pin.env').write_text(
            f'export RICHOS_ENGINE_VERSION=1.2.0\n'
            f'export RICHOS_ENGINE_SHA256={digest}\n'
            f'export RICHOS_ENGINE_URL={url}\n')
        (self.out / 'engine-published.ok').write_text(f'sha256={digest}\nurl={url}\n')
        self.env = {key: value for key, value in self.env.items()
                    if key not in ('GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME',
                                   'GIT_COMMITTER_EMAIL', 'EMAIL', 'GIT_INDEX_FILE')}
        self.env['TAURI_SIGNING_PRIVATE_KEY_PATH'] = str(root / 'fixture-unused.key')

        def git(*args, input=None):
            return subprocess.run(['git', *args], cwd=root, env=self.env, input=input,
                                  text=True, capture_output=True, check=True).stdout.strip()
        git('init', '-q', '-b', 'main')
        git('config', 'core.hooksPath', os.devnull)
        git('config', 'user.name', 'Fixture')
        git('config', 'user.email', 'fixture@example.invalid')
        git('add', 'richos', '.gitignore')
        git('commit', '-qm', 'release source fixture')
        return root, git

    def test_unpublished_candidate_without_tag_reaches_packaging(self):
        _, git = self.app_source_fixture()
        self.assertEqual(git('tag', '--list'), '')
        result = self.call('app')
        self.assertIn('fixture reached packaging', result.stderr)
        self.assertIn('does not exist locally yet', result.stdout)

    def test_app_accepts_lightweight_and_annotated_tags_at_head(self):
        _, git = self.app_source_fixture()
        tag = 'v' + self.version
        for annotated in (False, True):
            with self.subTest(annotated=annotated):
                git('tag', *(['-a', '-m', 'fixture'] if annotated else []), tag)
                result = self.call('app')
                self.assertIn('fixture reached packaging', result.stderr)
                self.assertIn('which is ' + tag, result.stdout)
                git('tag', '-d', tag)

    def test_app_refuses_a_tag_on_a_different_commit(self):
        root, git = self.app_source_fixture()
        git('tag', 'v' + self.version)
        (root / 'change').write_text('new source')
        git('add', 'change')
        git('commit', '-qm', 'source moved')
        result = self.call('app')
        self.assertEqual(result.returncode, 1)
        self.assertIn("GitHub publishes the TAG's tree", result.stderr)
        self.assertNotIn('fixture reached packaging', result.stderr)

    def test_app_refuses_a_tag_that_does_not_name_a_commit(self):
        _, git = self.app_source_fixture()
        blob = git('hash-object', '-w', '--stdin', input='not a commit')
        git('tag', 'v' + self.version, blob)
        result = self.call('app')
        self.assertEqual(result.returncode, 1)
        self.assertIn('exists but does not name a commit', result.stderr)
        self.assertNotIn('fixture reached packaging', result.stderr)

    def test_app_does_not_mistake_a_branch_for_the_public_tag(self):
        root, git = self.app_source_fixture()
        git('branch', 'v' + self.version)
        (root / 'change').write_text('new source')
        git('add', 'change')
        git('commit', '-qm', 'source moved')
        result = self.call('app')
        self.assertIn('fixture reached packaging', result.stderr)
        self.assertIn('does not exist locally yet', result.stdout)

    def test_app_still_refuses_uncommitted_source(self):
        root, _ = self.app_source_fixture()
        (root / 'change').write_text('uncommitted source')
        result = self.call('app')
        self.assertEqual(result.returncode, 1)
        self.assertIn('working tree has uncommitted changes', result.stderr)
        self.assertNotIn('fixture reached packaging', result.stderr)

    def test_app_accepts_generated_bytecode_but_refuses_uncommitted_source(self):
        root, git = self.app_source_fixture()
        # The real release suppresses bytecode; standalone tests need not. Let the
        # real Python import in make-release create its cache in this fixture.
        self.env.pop('PYTHONDONTWRITEBYTECODE', None)
        self.env.pop('PYTHONPYCACHEPREFIX', None)
        cache = self.scripts / '__pycache__'
        self.assertFalse(cache.exists())
        for state in ('clean', 'cached'):
            with self.subTest(cache=state):
                result = self.call('app')
                self.assertIn('fixture reached packaging', result.stderr)
                self.assertTrue(list(cache.glob('nightly.*.pyc')))
                self.assertEqual(git('status', '--porcelain'), '')
        (root / 'change').write_text('uncommitted source')
        result = self.call('app')
        self.assertEqual(result.returncode, 1)
        self.assertIn('working tree has uncommitted changes', result.stderr)
        self.assertNotIn('fixture reached packaging', result.stderr)

    def test_nightly_plan_requires_nightly_endpoint(self):
        result = self.call('plan')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(n.ENDPOINT, result.stdout)
        self.config.write_text(n.json_text({'plugins': {'updater': {'endpoints': ['https://stable.invalid']}}}))
        result = self.call('plan')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('updater endpoint', result.stderr)

    def test_assets_verified_before_channel_exists(self):
        result = self.call('verify-assets')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        args = Path(self.env['FAKE_CARGO_ARGS']).read_text()
        self.assertIn('verify_update_manifest', args)
        self.assertIn('--locked', args)
        self.assertIn('published bytes, version and updater signature verified', result.stdout)

    def test_the_channel_endpoint_is_a_different_url_from_the_releases_own_manifest(self):
        """Positive control for the test above, and it stopped being free on 2026-09-17.

        Both URLs are now `https://github.com/<repo>/releases/download/<tag>/latest.json`
        and differ only in the tag, where before one was a `raw.githubusercontent.com`
        branch URL. `verify-assets` passing above therefore no longer proves on its own
        that it left the channel alone -- so this proves the fixture still tells them
        apart, by running the one command that DOES fetch the channel and watching it
        be refused by name.
        """
        refusals = Path(self.env['FAKE_REFUSALS'])
        self.assertEqual(self.call('verify-assets').returncode, 0)
        self.assertFalse(refusals.exists(), 'verify-assets must not fetch the channel')
        result = self.call('verify-release')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(refusals.read_text().split(), [n.ENDPOINT],
                         'the channel endpoint, and only it, must be refused by name')

    def test_the_compiled_endpoint_is_read_from_the_publisher_not_respelled(self):
        """make-release.sh compiles the endpoint in; nightly.py publishes to it.

        Two spellings of one URL is a silent mismatch waiting to happen, and the
        endpoint check inside make-release.sh would then be comparing the script
        against itself. `plan` printing exactly `nightly.ENDPOINT` is the evidence
        that the script asks the publisher.
        """
        result = self.call('plan')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f'releases/download/{n.CHANNEL_TAG}/latest.json', result.stdout)
        self.assertNotIn('raw.githubusercontent.com', result.stdout)

    def test_changed_bytes_and_missing_assets_refuse(self):
        (self.remote / 'RichOS.app.tar.gz').write_text('corrupt')
        self.assertNotEqual(self.call('verify-assets').returncode, 0)
        self.assets()
        (self.remote / 'latest.json').unlink()
        self.assertNotEqual(self.call('verify-assets').returncode, 0)

    def test_wrong_version_and_signature_refuse(self):
        self.assets('5.1.0-nightly.20260911.9')
        self.assertNotEqual(self.call('verify-assets').returncode, 0)
        self.assets()
        self.assertNotEqual(self.call('verify-assets', FAKE_CARGO_EXIT='1').returncode, 0)


if __name__ == "__main__":
    # A stopped run removes its fixtures. Python's default SIGTERM ends the process at
    # once, so every TemporaryDirectory above (a remote.git, a repo and a fake nightly
    # tag) stayed in $TMPDIR, and the scratch reaper keeps a tree holding a .git.
    # Reproduced 2026-10-23: TERM after 6 s left tmp*/remote.git, repo and
    # v5.1.0-nightly.20260911.1. Ending as Ctrl-C does lets Python exit normally,
    # which runs each TemporaryDirectory's finalizer.
    import signal
    signal.signal(signal.SIGTERM, signal.default_int_handler)
    signal.signal(signal.SIGHUP, signal.default_int_handler)
    unittest.main(verbosity=2)
