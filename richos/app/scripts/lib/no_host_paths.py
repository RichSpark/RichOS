#!/usr/bin/env python3
"""Refuse a bundle that carries the building machine's home directory.

WHY THIS EXISTS. Rust bakes the compile-time path of every crate into the binary as
panic-location and debug metadata. For a registry dependency that path is
`$CARGO_HOME/registry/src/...`; for a path dependency outside the package directory it
is the absolute path of the source tree. MEASURED on the release build kept at
`~/.richos-signing/rebuild-survival/builds/build-1/RichOS.app`: 472 distinct
`/Users/<builder>/.cargo/...` paths and 23 distinct
`/Users/<builder>/ab/richos/.worktrees/<agent>/app/crates/...` paths, all readable with
`strings`. The bundle inspector saw none of it, because none of it is a file — it is
metadata inside the executable, exactly like the 19.89 MiB of embedded test screenshots
that `build.rs` now stages away.

That is a stranger reading the builder's account name, home-directory layout, repository
location and internal working-branch names out of an application they installed. It is
not cosmetic; the CEO's ruling that personal paths in the REPOSITORY are cosmetic says
nothing about the artifact, and nobody had ever checked the artifact.

WHAT FIXES IT. `--remap-path-prefix`, exported by `package-app.sh` before the release
build. Cargo's own `trim-paths` profile key would be the tidier route and is NOT
available: cargo 1.95.0 refuses it with "the package requires the Cargo feature called
`trim-paths`, but that feature is not stabilized in this version of Cargo". That was
established by running it, not by reading about it.

AND ONE THING THE REMAP CANNOT FIX, which is why the check is on the artifact rather than
on the flags. `--remap-path-prefix` rewrites the compiler's path metadata; a path a MACRO
produced as a string literal is program data, and the flag does not touch it. Exactly one
such path survived the remap: `tauri::generate_context!()` emits
`context.with_config_parent("<CARGO_MANIFEST_DIR>")` when the `tauri` crate's
`custom-protocol` feature is off. That feature is now a default of `richos-tauri`
(`app/src-tauri/Cargo.toml`), so the literal is no longer generated at all. If someone
removes that default, no flag and no script notices — THIS check does, on the built
artifact, exactly as it did on 2026-09-04.

WHAT THIS SCRIPT IS FOR. The remap is the fix; this is the proof. A flag that silently
stops applying — a new dependency shape, a vendored C library baking in `__FILE__`, a
future toolchain — would put the paths straight back with nothing to notice it. So the
guarantee is checked on the built bundle, after it is produced and before it is signed,
in the same posture `build.rs` takes toward the staged frontend.

PLACEHOLDERS ARE NOT FINDINGS. `/Users/you/...` and `/Users/example/...` are invented
fixture paths that the product deliberately ships (`app/ui/mock.js`,
`EntityRegistry::FIXTURE_COMPANIES`). They name no machine and they are allowed by name,
so that the check refusing them would be a bug rather than a judgment call.

NEITHER ARE THE PRODUCT'S OWN ASSET KEYS. `app/ui/home/` holds four files, so the
embedded frontend carries the asset keys `/home/field-data.js`, `/home/field-engine.js`,
`/home/field-prep.js` and `/home/field-ref.js`. Those are URL paths inside the
application. The first version of this check read them as `/home/<user>` and refused a
correct release build over them — MEASURED at 9a7799e, where it reported five prefixes
and only one was real. A check that cries wolf four times out of five is a check people
learn to override, which is the failure this file exists to prevent.

So a finding now requires a CHILD COMPONENT: `<root>/<name>/`. That is what a leaked
home directory always looks like, because what leaks is a path INTO the home directory —
a registry source file, a crate root, a manifest directory. `/home/field-data.js` names a
file sitting directly in `/home`, which is not a home directory and never was one.
Narrowing a check is how checks quietly stop working, so the narrowing is paired with an
exact rule: the `--home` value itself is a finding wherever it appears, with a child
component or without one, so the plainest possible leak cannot slip through the gap the
narrowing opens.

The residual is deliberate and fails SAFE. If `app/ui/home/` ever grows a subdirectory,
its asset key gains a child component and this check will refuse the build. That is the
right direction to fail in: name it with `--allow`, and do not widen the rule.

Usage:
    no_host_paths.py <bundle-or-file> [--home DIR] [--allow NAME]... [--quiet]
    no_host_paths.py --self-test

Exit: 0 clean, 1 findings, 2 bad usage.
"""
from __future__ import annotations

import argparse
import ast
import os
import re
import json
import hashlib
from pathlib import Path
import sys

# Home-directory shapes on the platforms this project builds for. `/home/` is here even
# though RichOS ships macOS first, because the Windows companion and any future Linux
# build have the identical defect and a check that only knows one platform grows a hole
# the day the second one ships.
HOME_ROOTS = ("/Users/", "/home/", "/var/root/")

# Fixture user names that are deliberately shipped. Anything NOT on this list is a
# finding, so a new real name cannot be quietly absorbed.
DEFAULT_ALLOWED = frozenset({"you", "example", "x", "jordan", "user", "someone"})

# THE BUILD PATHS OF A PREBUILT THIRD-PARTY LIBRARY, which name its builder and never a
# RichOS machine. Rich's voice (`richos-voice` `pocket.rs`, 2026-10-14) links ONNX Runtime
# statically from the archive `ort-sys` pins by sha256; that archive was compiled by pyke's
# `ort-artifacts` GitHub Actions workflow, and ONNX Runtime's error messages carry its
# source paths under the runner's account (549 strings in the first build that linked it).
# Exactly this prefix is accepted, so `/Users/runner/anything-else` is still a finding, and
# the `--home` rule still refuses this machine's own home whatever its name.
UPSTREAM_BUILD_PREFIXES = ("/Users/runner/work/ort-artifacts/",)  # machine-path-exempt: a third-party CI build path, matched to accept it, never storage

# A printable run, the way `strings` defines one.
RUN = re.compile(rb"[\x20-\x7e]{6,}")

# `<root>/<name>` followed by a `/`. The lookahead is load-bearing twice over: it demands
# the child component that makes this a home-directory path rather than a file that merely
# lives under `/home`, and it stays OUT of group(0) so the reported prefix is still the
# thing an operator greps for.
VOLUME = re.compile(r"(?<!/System)/Volumes/[A-Za-z0-9_.-]+(?![A-Za-z0-9_.-])")
REVIEWED_REASON = re.compile(r"machine-path-exempt:\s*\S[^\r\n]*")
TEXT_SUFFIXES = {'.py', '.sh', '.rs', '.js', '.mjs', '.ts', '.swift', '.kt', '.json', '.md', '.txt', '.toml', '.config', '.yml', '.yaml'}

PATH = re.compile(
    r"(?:" + "|".join(re.escape(r) for r in HOME_ROOTS) + r")([A-Za-z0-9_.-]+)(?=/)"
)


def _exact_home(home: str | None):
    """The `--home` value on its own terms, child component or not.

    Paired with PATH's lookahead so that narrowing the heuristic cannot open a hole: a
    binary carrying nothing but the bare string `/Users/alex` is still refused.
    """
    if not home:
        return None
    return re.compile(re.escape(home.rstrip("/")) + r"(?![A-Za-z0-9_.-])")


def runs(data: bytes):
    for m in RUN.finditer(data):
        yield m.group(0).decode("ascii")


def scan_bytes(data: bytes, allowed, home: str | None = None):
    """Return {offending_prefix: count} for one blob."""
    hits: dict[str, int] = {}
    exact = _exact_home(home)
    for text in runs(data):
        reported: set[int] = set()
        for m in VOLUME.finditer(text):
            hits[m.group(0)] = hits.get(m.group(0), 0) + 1
        for m in PATH.finditer(text):
            if m.group(1) in allowed or text.startswith(UPSTREAM_BUILD_PREFIXES, m.start()):
                continue
            hits[m.group(0)] = hits.get(m.group(0), 0) + 1
            reported.add(m.start())
        if exact:
            for m in exact.finditer(text):
                # Same offset means the heuristic already counted it; counting it twice
                # would inflate a real finding, and an inflated number is a number nobody
                # can check.
                if m.start() in reported:
                    continue
                hits[m.group(0)] = hits.get(m.group(0), 0) + 1
    return hits


def files_under(target: str):
    if os.path.isfile(target):
        yield target
        return
    for root, _dirs, names in os.walk(target):
        for name in sorted(names):
            path = os.path.join(root, name)
            if os.path.isfile(path) and not os.path.islink(path):
                yield path


def operational_text(text, path):
    """Only source that can prescribe runtime paths, not docs or test examples."""
    parts = Path(path).parts
    name = Path(path).name
    if name in {'proof-inputs.json', 'verification-dependencies.json', 'verification-input-qualifications.json'}:
        return ''
    if any(p.lower() in {'docs', 'test', 'tests', 'testvm', 'fixtures', 'examples', 'validation'} for p in parts):
        return ''
    if '.test.' in name or '.mutation.' in name or name.endswith(('.md', '.patch', '-fixture.sh', '_fixture.py')):
        if 'skills' not in parts and 'agents' not in parts:
            return ''
    if name == "runtime-sources.json":
        try:
            recipe = json.loads(text)
        except ValueError:
            return text
        recipe.pop("upstream_path_exemptions", None)  # diagnostic prefixes, not storage instructions
        return json.dumps(recipe)
    if path.endswith('.py'):
        try:
            nodes = list(ast.walk(ast.parse(text)))
        except SyntaxError:
            return text
        lines = text.splitlines(keepends=True)
        for node in reversed(nodes):
            if isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant) and isinstance(node.value.value, str):
                for row in range(node.lineno - 1, node.end_lineno):
                    raw = lines[row].encode('utf-8')
                    start = node.col_offset if row == node.lineno - 1 else 0
                    end = node.end_col_offset if row == node.end_lineno - 1 else len(raw.rstrip(b'\r\n'))
                    lines[row] = (raw[:start] + b' ' * (end - start) + raw[end:]).decode('utf-8')
        text = ''.join(lines)
    return text


def scan(target: str, allowed, home: str | None, reviewed_text=False, upstream_policy=None, runtime_root=None):
    """Return a list of (file, prefix, count, is_this_machine)."""
    findings = []
    upstream = {}
    if upstream_policy is not None:
        if runtime_root is None or not Path(runtime_root).resolve().is_relative_to(Path(target).resolve()):
            raise ValueError("upstream runtime root must be inside the scanned artifact")
        for row in upstream_policy.get("upstream_path_exemptions", []):
            member = Path(row["member"])
            if member.is_absolute() or ".." in member.parts or not row.get("reason") or not re.fullmatch(r"[0-9a-f]{64}", row.get("sha256", "")):
                raise ValueError("invalid upstream artifact exception")
            if not isinstance(row.get("prefixes"), list) or not all(isinstance(v, str) and v.startswith(("/Users/", "/Volumes/", "/home/")) for v in row["prefixes"]):
                raise ValueError("invalid upstream metadata prefixes")
            upstream[str((Path(runtime_root) / member).resolve())] = row
    home = home.rstrip("/") if home else None
    for path in files_under(target):
        try:
            with open(path, "rb") as fh:
                data = fh.read()
        except OSError as exc:  # unreadable member is a finding about the check, not the app
            findings.append((path, f"<unreadable: {exc}>", 1, False))
            continue
        counts = {}
        text = None
        if reviewed_text and os.path.splitext(path)[1] in TEXT_SUFFIXES and b"\x00" not in data:
            try:
                text = operational_text(data.decode("utf-8"), os.path.relpath(path, target) if os.path.isdir(target) else os.path.basename(path))
            except UnicodeDecodeError:
                pass
        if text is None:
            counts = scan_bytes(data, allowed, home)
        else:
            previous = ""
            for line in text.splitlines():
                exempt = REVIEWED_REASON.search(line) or REVIEWED_REASON.search(previous)
                previous = line
                if exempt:
                    continue
                row = scan_bytes(line.encode("utf-8"), allowed, home)
                if line.lstrip().startswith(("#", "//")) and os.path.splitext(path)[1] not in {'.md', '.txt'}:
                    row = {}
                for key, count in row.items():
                    counts[key] = counts.get(key, 0) + count
        exception = upstream.get(str(Path(path).resolve()))
        if exception and hashlib.sha256(data).hexdigest() == exception["sha256"]:
            accepted = {prefix: counts.pop(prefix) for prefix in exception["prefixes"] if prefix in counts}
            if accepted:
                print("no-host-paths: pinned upstream metadata exception: " + str(exception["member"]) + ": " + exception["reason"], file=sys.stderr)
        for prefix, count in sorted(counts.items()):
            findings.append((path, prefix, count, bool(home) and prefix == home))
    return findings


def report(findings, target, quiet=False):
    if not findings:
        if not quiet:
            print(f"no-host-paths: clean — no home-directory path in {target}")
        return 0

    by_prefix: dict[str, int] = {}
    for _path, prefix, count, _mine in findings:
        by_prefix[prefix] = by_prefix.get(prefix, 0) + count

    print("", file=sys.stderr)
    print(
        f"no-host-paths: {target} carries {len(by_prefix)} home-directory prefix(es) "
        f"across {len({f[0] for f in findings})} file(s). A stranger who installs this "
        "can read them with `strings`.",
        file=sys.stderr,
    )
    print("", file=sys.stderr)
    for prefix in sorted(by_prefix):
        print(f"  {prefix}   ({by_prefix[prefix]} occurrence(s))", file=sys.stderr)
    print("", file=sys.stderr)
    print(
        "Fix: build through richos/app/scripts/package-app.sh, which exports\n"
        "--remap-path-prefix for $HOME, $CARGO_HOME and the source tree before\n"
        "`cargo tauri build`. If this fired on a build that DID go through it, the\n"
        "remap missed a new path shape — find it with:\n"
        f"\n    strings -n 8 <executable> | grep -o '{HOME_ROOTS[0]}[^\" ]*' | sort -u\n"
        "\nOverride, deliberately and on the record, with RICHOS_ALLOW_HOST_PATHS=1.",
        file=sys.stderr,
    )
    return 1


SELF_TEST_CASES = [
    # (blob, allowed, home, expect_findings)
    (b"nothing to see here at all", DEFAULT_ALLOWED, None, False),
    (b"/System/Volumes/Data", DEFAULT_ALLOWED, None, False),
    (b"/Volumes/Fixture/cache", DEFAULT_ALLOWED, None, True), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    (b"/Volumes/Fixture", DEFAULT_ALLOWED, None, True), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    (b"machine-path-exempt: fixture\x00/Volumes/Fixture/cache", DEFAULT_ALLOWED, None, True),
    (b"/Users/you/Projects/northwind is a fixture", DEFAULT_ALLOWED, None, False),
    (b"/Users/example/Projects/harbor is a fixture", DEFAULT_ALLOWED, None, False),
    (b"/Users/alex/.cargo/registry/src/anyhow/src/error.rs", DEFAULT_ALLOWED, None, True), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    (b"/home/builder/src/richos/app/crates/richos-core/src/loro.rs", DEFAULT_ALLOWED, None, True),
    (b"/var/root/secret/thing.rs", DEFAULT_ALLOWED, None, True),
    # Short runs are invisible to `strings` and must be invisible here too, so the
    # check describes the same surface an attacker actually reads.
    (b"\x00\x01/Us\x00", DEFAULT_ALLOWED, None, False),
    # A real name added to --allow is not a finding; that is what the flag is for.
    (b"/Users/ci-runner/work/x.rs", DEFAULT_ALLOWED | {"ci-runner"}, None, False), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    # ONNX Runtime's own build paths (UPSTREAM_BUILD_PREFIXES) are accepted, and only those.
    (b"/Users/runner/work/ort-artifacts/ort-artifacts/onnxruntime/core/framework/x.cc", DEFAULT_ALLOWED, None, False), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    (b"/Users/runner/work/other/x.cc", DEFAULT_ALLOWED, None, True), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    # THE PRODUCT'S OWN ASSET KEYS. All four files in app/ui/home/ are embedded in the
    # binary under these keys. This check refused a correct release build over them until
    # the child component became a requirement; if these cases ever fail again, the
    # release is blocked by a false positive.
    (b"/home/field-data.js", DEFAULT_ALLOWED, None, False),
    (b"/home/field-engine.js", DEFAULT_ALLOWED, None, False),
    (b"/home/field-prep.js", DEFAULT_ALLOWED, None, False),
    (b"/home/field-ref.js", DEFAULT_ALLOWED, None, False),
    # ...and the same shape one level down IS a finding, because that is the shape of a
    # leaked home directory. This is the cost of the narrowing, stated rather than hidden.
    (b"/home/field/data.js", DEFAULT_ALLOWED, None, True),
    # THE HOLE THE NARROWING WOULD OTHERWISE OPEN. A bare home directory has no child
    # component, so only the exact `--home` rule catches it.
    (b"built by someone at /Users/alex", DEFAULT_ALLOWED, "/Users/alex", True), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    (b"built by someone at /Users/alex", DEFAULT_ALLOWED, None, False), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    # A longer name that merely starts with the home directory is a different account,
    # and it is a finding on its own terms rather than as this machine's home.
    (b"/Users/alexandra/work/x.rs", DEFAULT_ALLOWED, "/Users/alex", True), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    # The two rules overlap on every ordinary leak; the count must not double.
    (b"/Users/alex/.cargo/x.rs", DEFAULT_ALLOWED, "/Users/alex", True), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
]


def self_test() -> int:
    bad = 0
    for i, (blob, allowed, home, expect) in enumerate(SELF_TEST_CASES, 1):
        got = bool(scan_bytes(blob, allowed, home))
        ok = got == expect
        bad += not ok
        print(f"{'ok  ' if ok else 'FAIL'} case {i}: expected findings={expect}, got={got}")

    # Counted ONCE, not once by each rule. An inflated number is a number nobody can check.
    counts = scan_bytes(b"/Users/alex/.cargo/x.rs", DEFAULT_ALLOWED, "/Users/alex") # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    ok = counts == {"/Users/alex": 1} # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    bad += not ok
    print(f"{'ok  ' if ok else 'FAIL'} an overlapping finding is counted once: {counts}")

    # The bare-home rule must not fire on a DIFFERENT account whose name merely starts
    # with this one; that finding belongs to the heuristic and names the longer prefix.
    counts = scan_bytes(b"/Users/alexandra/work/x.rs", DEFAULT_ALLOWED, "/Users/alex") # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    ok = counts == {"/Users/alexandra": 1} # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    bad += not ok
    print(f"{'ok  ' if ok else 'FAIL'} a longer account name is reported as itself: {counts}")

    # The remap the fix installs must produce something this check calls clean.
    remapped = b"/cargo/registry/src/index.crates.io-1949cf8/anyhow-1.0.104/src/error.rs"
    ok = not scan_bytes(remapped, DEFAULT_ALLOWED)
    bad += not ok
    print(f"{'ok  ' if ok else 'FAIL'} remapped path is clean")

    return 1 if bad else 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(add_help=True, description=__doc__.splitlines()[0])
    ap.add_argument("target", nargs="?", help="a .app bundle, a directory, or a file")
    ap.add_argument("--home", default=os.environ.get("HOME"), help="this machine's home directory")
    ap.add_argument("--allow", action="append", default=[], metavar="NAME",
                    help="an additional user name to treat as a placeholder")
    ap.add_argument("--quiet", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--upstream-policy", help="pinned upstream metadata exceptions in the runtime recipe")
    ap.add_argument("--upstream-runtime-root", help="runtime subdirectory inside this artifact")
    ap.add_argument("--reviewed-text", action="store_true", help="allow line-local reviewed fixture/history reasons in UTF-8 source text only")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()
    if not args.target:
        ap.print_usage(sys.stderr)
        return 2
    if not os.path.exists(args.target):
        print(f"no-host-paths: {args.target} does not exist", file=sys.stderr)
        return 2

    allowed = DEFAULT_ALLOWED | set(args.allow)
    policy = json.loads(Path(args.upstream_policy).read_text()) if args.upstream_policy else None
    return report(scan(args.target, allowed, args.home, args.reviewed_text, policy, args.upstream_runtime_root), args.target, args.quiet)


if __name__ == "__main__":
    sys.exit(main())
