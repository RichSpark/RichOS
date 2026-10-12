#!/usr/bin/env python3
"""Generated launchers must work within the real packaged probe's no-fork policy."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
import probe_packaged

spec = importlib.util.spec_from_file_location("runtime_builder", HERE / "build-runtimes.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class Launchers(unittest.TestCase):
    def test_python_and_git_launchers_need_no_fork(self):
        with tempfile.TemporaryDirectory(prefix="runtime launchers with spaces ") as temporary:
            root = Path(temporary).resolve()
            runtime = root / "engine/runtime"
            for part in ("bin", "node/bin", "node/lib", "python/bin", "git/bin"):
                (runtime / part).mkdir(parents=True, exist_ok=True)
            (root / "home").mkdir()
            for name in ("python/bin/python3", "git/bin/git"):
                target = runtime / name
                target.write_text('#!/bin/sh\nprintf "%s\\n" "${PYTHONDONTWRITEBYTECODE:-unset}" "$@"\n')
                target.chmod(0o755)
            builder.prepare_runtime_launchers(runtime, {})
            self.assertEqual(probe_packaged.run(root, [runtime / "bin/python3", "a b", "*"]).splitlines(),
                             ["1", "-B", "a b", "*"])
            self.assertEqual(probe_packaged.run(root, [runtime / "bin/git", "a b", "*"]).splitlines(),
                             ["unset", "a b", "*"])

    def test_generated_node_launcher_needs_no_fork(self):
        with tempfile.TemporaryDirectory(prefix="runtime launcher with spaces ") as temporary:
            root = Path(temporary).resolve()
            runtime = root / "engine/runtime"
            for part in ("bin", "node/bin", "node/lib"):
                (runtime / part).mkdir(parents=True, exist_ok=True)
            (root / "home").mkdir()
            target = runtime / "node/bin/node"
            target.write_text('#!/bin/sh\nprintf "%s\\n" "$OPENSSL_MODULES" "$@"\n')
            target.chmod(0o755)
            builder.prepare_portable_node(runtime, {})
            wrapper = runtime / "bin/node"
            for command in (str(wrapper), "./engine/runtime/bin/node", "node"):
                with self.subTest(command=command):
                    lines = probe_packaged.run(root, [command, "argument with spaces", "*"]).splitlines()
                    self.assertEqual(lines[1:], ["argument with spaces", "*"])
                    self.assertEqual((root / lines[0]).resolve(), runtime / "node/lib/ossl-modules")
            # An operator's explicit module directory keeps precedence over the default.
            custom = root / "custom"
            custom.write_text('#!/bin/sh\nexport OPENSSL_MODULES="explicit module path"\nexec "' + str(wrapper) + '" "$@"\n')
            custom.chmod(0o755)
            self.assertEqual(probe_packaged.run(root, [custom]).strip(), "explicit module path")
            # Keep the original failure as a control: a launcher that forks is refused.
            wrapper.write_text('#!/bin/sh\nexec "$(dirname "$0")/../node/bin/node" "$@"\n')
            with self.assertRaisesRegex(ValueError, "fork: Operation not permitted"):
                probe_packaged.run(root, [wrapper])


if __name__ == "__main__":
    unittest.main()
