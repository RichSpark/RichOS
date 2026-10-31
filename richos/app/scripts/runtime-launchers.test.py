#!/usr/bin/env python3
"""Generated launchers must work within the real packaged probe's no-fork policy, and every
entry in the runtime's bin must be a Mach-O executable or a symlink to one, never a `#!` script
(macOS leaks kernel memory for a script exec'd from a script; runtime-launcher.c)."""
import importlib.util
import os
from pathlib import Path
import shutil
import tempfile
import unittest
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
import probe_packaged

spec = importlib.util.spec_from_file_location("runtime_builder", HERE / "build-runtimes.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


# Read here, not from the builder, so the test does not trust the code it checks.
MACHO = (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xfe\xed\xfa\xcf")


def not_macho(bin_dir):
    """Entries of bin_dir that are not a Mach-O file or a symlink to one inside the runtime."""
    root = bin_dir.parent.resolve()
    found = []
    for entry in sorted(bin_dir.iterdir()):
        target = entry.resolve()
        if not target.is_relative_to(root) or not target.is_file():
            found.append(f"{entry.name} (no file inside the runtime)")
            continue
        with target.open("rb") as handle:
            head = handle.read(4)
        if head not in MACHO:
            found.append(f"{entry.name} ({head[:2]!r}...)" if head[:2] == b"#!" else f"{entry.name} (magic {head.hex()})")
    return found


def fake_runtime(root):
    """A runtime tree with stand-in targets and the already-Mach-O tools, launchers not yet made."""
    runtime = root / "engine/runtime"
    for part in ("bin", "node/bin", "node/lib", "python/bin", "git/bin"):
        (runtime / part).mkdir(parents=True, exist_ok=True)
    (root / "home").mkdir(exist_ok=True)
    for name in ("python/bin/python3", "git/bin/git", "node/bin/node"):
        target = runtime / name
        target.write_text('#!/bin/sh\nprintf "%s\\n" "${PYTHONDONTWRITEBYTECODE:-unset}" "${OPENSSL_MODULES:-unset}" "$@"\n')
        target.chmod(0o755)
    for name in ("jq", "ffmpeg", "ffprobe", "whisper-cli"):   # Mach-O upstream tools: a system executable stands in
        shutil.copyfile("/usr/bin/true", runtime / "bin" / name)
        (runtime / "bin" / name).chmod(0o755)
    return runtime


class EveryEntryIsMachO(unittest.TestCase):
    def test_built_bin_holds_no_script(self):
        with tempfile.TemporaryDirectory(prefix="runtime bin entries ") as temporary:
            runtime = fake_runtime(Path(temporary).resolve())
            builder.prepare_runtime_launchers(runtime, {})
            self.assertEqual(sorted(p.name for p in (runtime / "bin").iterdir()),
                             ["ffmpeg", "ffprobe", "git", "jq", "node", "python3", "whisper-cli"])
            self.assertEqual(not_macho(runtime / "bin"), [])

    def test_delivered_runtime_bin_holds_no_script(self):
        delivered = os.environ.get("RICHOS_RUNTIME_DIR")
        if not delivered or not (Path(delivered) / "bin").is_dir():
            self.skipTest("RICHOS_RUNTIME_DIR names no runtime; the built bin is checked above")
        self.assertEqual(not_macho(Path(delivered) / "bin"), [])

    def test_the_check_refuses_a_script_and_an_escaping_link(self):
        # Positive probes for this test's own reader and for the builder's script_entries, so
        # an empty list above is not a check that cannot fail.
        with tempfile.TemporaryDirectory(prefix="runtime bin probes ") as temporary:
            runtime = fake_runtime(Path(temporary).resolve())
            bin_dir = runtime / "bin"
            (bin_dir / "python3").write_text("#!/bin/sh\nexec true\n")
            (bin_dir / "outside").symlink_to("/usr/bin/true")
            (bin_dir / "inside").symlink_to("jq")
            self.assertEqual([entry.split(" ")[0] for entry in not_macho(bin_dir)], ["outside", "python3"])
            self.assertEqual(builder.script_entries(bin_dir), ["outside", "python3"])


class PythonBytecode(unittest.TestCase):
    """2026-10-29: a child that rebuilt its environment ran the runtime's python/bin/python3 with
    neither -B nor PYTHONDONTWRITEBYTECODE and wrote 36-47 caches into the CEO's installed engine,
    and every Python without -B rewrote the delivered encodings caches relocation made stale.
    prepare_python_bytecode delivers sitecustomize.py (writing off for every start) and makes every
    delivered cache, sitecustomize's own included, a hash-checked one. The interpreter here stands in
    for the runtime's; build-runtimes.test.sh B7 runs the same check on a built runtime."""

    def test_delivered_caches_survive_relocation_and_sitecustomize_turns_writing_off(self):
        import py_compile, struct, subprocess
        with tempfile.TemporaryDirectory(prefix="runtime python bytecode ") as temporary:
            runtime = Path(temporary).resolve() / "engine/runtime"
            library = runtime / "python/lib/python3.99"
            (library / "probe_pkg").mkdir(parents=True)
            (runtime / "python/bin").mkdir(parents=True)
            (library / "os.py").write_text("# the standard library's marker file\n")
            interpreter = runtime / "python/bin/python3"
            interpreter.write_text('#!/bin/sh\nexec "%s" "$@"\n' % os.path.realpath(sys.executable))
            interpreter.chmod(0o755)
            source = library / "probe_pkg/__init__.py"
            source.write_text("VALUE = 1\n")
            delivered = Path(py_compile.compile(str(source), doraise=True,
                                                invalidation_mode=py_compile.PycInvalidationMode.TIMESTAMP))
            builder.prepare_python_bytecode(runtime)
            caches = sorted(p for p in (runtime / "python").rglob("*.pyc"))
            self.assertEqual(sorted(p.name.split(".")[0] for p in caches), ["__init__", "sitecustomize"], caches)
            for cache in caches:
                self.assertEqual(struct.unpack("<I", cache.read_bytes()[4:8])[0], 3, f"{cache.name} is hash-checked")
            # Relocation: the installer gives every source a new mtime.
            later = source.stat().st_mtime + 3600
            os.utime(source, (later, later))
            os.utime(library / "sitecustomize.py", (later, later))
            before = {p: p.read_bytes() for p in caches}
            env = {"PATH": "/usr/bin:/bin", "HOME": temporary}
            result = subprocess.run([sys.executable, "-c",
                                     f"import sys; sys.dont_write_bytecode = False; sys.path.insert(0, {str(library)!r}); "
                                     f"import probe_pkg; print(probe_pkg.VALUE)"],
                                    env=env, capture_output=True, text=True, check=True)
            self.assertEqual(result.stdout.strip(), "1")
            self.assertEqual({p: p.read_bytes() for p in caches}, before, "a delivered cache was rewritten")
            self.assertEqual(sorted((runtime / "python").rglob("*.pyc")), caches, "a cache was added")
            ran = subprocess.run([sys.executable, "-c", "import runpy, sys; sys.dont_write_bytecode = False; "
                                  f"runpy.run_path({str(library / 'sitecustomize.py')!r}); print(sys.dont_write_bytecode)"],
                                 env=env, capture_output=True, text=True, check=True)
            self.assertEqual(ran.stdout.strip(), "True")
            self.assertTrue(delivered.is_file())

    def test_an_upstream_sitecustomize_is_refused_not_replaced(self):
        with tempfile.TemporaryDirectory(prefix="runtime python bytecode ") as temporary:
            library = Path(temporary) / "runtime/python/lib/python3.99"
            (library / "site-packages").mkdir(parents=True)
            (library / "os.py").write_text("")
            (library / "site-packages/sitecustomize.py").write_text("UPSTREAM = True\n")
            with self.assertRaisesRegex(RuntimeError, "already ships a sitecustomize.py"):
                builder.prepare_python_bytecode(Path(temporary) / "runtime")


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
