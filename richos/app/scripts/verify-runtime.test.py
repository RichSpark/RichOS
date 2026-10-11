#!/usr/bin/env python3
"""Negative delivery checks use fictional files, never operator state."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("verify_runtime", Path(__file__).with_name("verify-runtime.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class RuntimeInventoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="richos runtime inventory ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "runtime"
        (self.root / "bin").mkdir(parents=True)
        self.sources = Path(self.temp.name) / "sources.json"
        self.sources.write_text('{"synthetic":true}')
        (self.root / "runtime-sources.json").write_bytes(self.sources.read_bytes())
        for name in ("python3", "node", "git", "jq", "whisper-cli", "ffmpeg", "ffprobe"):
            path = self.root / "bin" / name
            path.write_text("#!/bin/sh\nexit 0\n")
            path.chmod(0o755)
        self.manifest = {"schema": 1, "platform": "aarch64-apple-darwin", "versions": {}, "links": {},
            "files": {str(p.relative_to(self.root)): hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in self.root.rglob("*") if p.is_file()}}
        self.save()

    def save(self):
        (self.root / "delivery.json").write_text(json.dumps(self.manifest))

    def test_complete_delivery_passes(self):
        module.verify(self.root, self.sources)

    def test_unlisted_private_record_refuses(self):
        (self.root / "fictional-private-record.json").write_text('{"fictional":true}')
        with self.assertRaisesRegex(ValueError, "unexpected"):
            module.verify(self.root, self.sources)

    def test_changed_binary_refuses(self):
        (self.root / "bin/git").write_text("#!/bin/sh\nexit 1\n")
        with self.assertRaisesRegex(ValueError, "changed"):
            module.verify(self.root, self.sources)

    def test_escaping_symlink_refuses_even_if_declared(self):
        (self.root / "escape").symlink_to(self.sources)
        self.manifest["links"]["escape"] = str(self.sources)
        self.save()
        with self.assertRaisesRegex(ValueError, "invalid runtime link"):
            module.verify(self.root, self.sources)

    def test_changed_source_recipe_refuses(self):
        self.sources.write_text('{"synthetic":false}')
        with self.assertRaisesRegex(ValueError, "public recipe"):
            module.verify(self.root, self.sources)

    def test_runtime_from_another_recipe_names_what_differs_and_the_right_runtime(self):
        # Nightly 51 (2026-10-11): the launch passed --runtime-dir ~/.richos-nightly/runtime, the
        # pre-keying cache built from a recipe without ffmpeg, ffprobe and whisper-cpp, and the
        # refusal said only "do not match". The refusal now names the differing sources and the
        # recipe-keyed runtime directory (lib/runtime_cache.py) that matches this recipe.
        (self.root / "runtime-sources.json").write_text(json.dumps(
            {"schema": 1, "sources": {"git": {"v": 1}, "node": {"v": 1}, "old": {"v": 1}}}))
        self.sources.write_text(json.dumps(
            {"schema": 1, "sources": {"git": {"v": 1}, "node": {"v": 2}, "ffmpeg": {"v": 1}, "whisper-cpp": {"v": 1}}}))
        keyed = "runtime-" + hashlib.sha256(self.sources.read_bytes()).hexdigest()[:12]
        with self.assertRaises(ValueError) as refused:
            module.verify(self.root, self.sources)
        message = str(refused.exception)
        self.assertIn("public recipe", message)
        self.assertIn("missing: ffmpeg, whisper-cpp", message)
        self.assertIn("extra: old", message)
        self.assertIn("changed: node", message)
        self.assertIn(keyed, message)

    def test_nonexecutable_runtime_refuses(self):
        (self.root / "bin/node").chmod(0o644)
        with self.assertRaisesRegex(ValueError, "not executable"):
            module.verify(self.root, self.sources)

    def test_runtime_without_the_speech_decoder_refuses(self):
        # Voice mode on a user's Mac has no other whisper-cli; a runtime without it never ships.
        (self.root / "bin/whisper-cli").chmod(0o644)
        with self.assertRaisesRegex(ValueError, "not executable: whisper-cli"):
            module.verify(self.root, self.sources)

    def test_runtime_without_the_video_tools_refuses(self):
        # Rich's PATH holds the runtime and system folders only; a runtime without ffmpeg or
        # ffprobe would leave him unable to watch a video, so it never ships.
        for name in ("ffmpeg", "ffprobe"):
            (self.root / "bin" / name).chmod(0o644)
            with self.assertRaisesRegex(ValueError, f"not executable: {name}"):
                module.verify(self.root, self.sources)
            (self.root / "bin" / name).chmod(0o755)

    def test_invalid_member_name_refuses(self):
        self.manifest["files"]["../outside"] = "0" * 64
        self.save()
        with self.assertRaisesRegex(ValueError, "member name"):
            module.verify(self.root, self.sources)


class TrackedRecipeTests(unittest.TestCase):
    """The tracked recipe itself: every source pinned, and the speech decoder at the reference
    version the decode settings were measured on."""
    recipe = json.loads(Path(__file__).with_name("runtime-sources.json").read_text())

    def test_every_source_is_an_https_url_with_a_sha256_pin(self):
        for name, source in self.recipe["sources"].items():
            self.assertTrue(source["url"].startswith("https://"), name)
            self.assertRegex(source["sha256"], r"^[0-9a-f]{64}$", name)

    def test_ffmpeg_corresponding_source_is_pinned(self):
        # GPLv3: the runtime carries directions to the exact source of the static ffmpeg build,
        # so every archive in them is an https URL with a sha256 pin, ffmpeg's own at the same version.
        ffmpeg = self.recipe["sources"]["ffmpeg"]
        self.assertEqual(ffmpeg["version"], self.recipe["sources"]["ffprobe"]["version"])
        self.assertTrue(ffmpeg["license_url"].startswith("https://"))
        self.assertRegex(ffmpeg["license_sha256"], r"^[0-9a-f]{64}$")
        rows = ffmpeg["corresponding_source"]
        self.assertEqual(len({row["name"] for row in rows}), len(rows))
        self.assertIn(("ffmpeg", ffmpeg["version"]), [(row["name"], row["version"]) for row in rows])
        for row in rows:
            self.assertTrue(row["url"].startswith("https://"), row["name"])
            self.assertRegex(row["sha256"], r"^[0-9a-f]{64}$", row["name"])

    def test_whisper_cpp_is_the_reference_build_version(self):
        pins = Path(__file__).resolve().parents[2] / "engine/voice/models/model-pins.json"
        reference = json.loads(pins.read_text())["toolchain"]["whisperCppVersion"]
        whisper = self.recipe["sources"]["whisper-cpp"]
        self.assertEqual(whisper["version"], reference)
        self.assertIn(f"/v{reference}.tar.gz", whisper["url"])


if __name__ == "__main__":
    unittest.main()
