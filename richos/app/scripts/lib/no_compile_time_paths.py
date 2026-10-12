#!/usr/bin/env python3
"""Refuse SOURCE that bakes a build-machine path into the shipped program.

WHY THIS EXISTS, AND WHY IT IS NOT `no_host_paths.py`. That one inspects the built
bundle and is the guarantee; this one reads the source and is the early warning. They
catch the same class of defect at two different costs. On 2026-09-18 the nightly build
of `e6f62448` compiled for the better part of an hour, bundled, and only then refused:

    no-host-paths: ... RichOS.app carries 1 home-directory prefix(es) across 1 file(s)
      /Users/alex   (1 occurrence(s))

The one occurrence was
`/Users/alex/.richos-nightly/source/richos/app/src-tauri`, and it came from a single line
of Rust:

    let from_source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../phone");

`--remap-path-prefix` could not touch it. The flag rewrites the COMPILER's path metadata;
`env!` produces a string literal, which is program data. So the artifact check was the
only thing that could see it, and it saw it at the end of an hour.

This check sees it in under a second, on the source, before anything is compiled. It does
not replace the artifact check and must never be read as doing so: a vendored C library
baking in `__FILE__`, a new dependency shape or a future toolchain would put host paths
back with nothing here to notice. The artifact is still the guarantee.

WHAT IS A FINDING

  1. `env!("CARGO_MANIFEST_DIR")` outside test code. There is no correct use in a shipped
     path: it is the directory the crate was compiled in, on the machine that compiled it.
  2. A string literal containing `/Users/<name>/` outside test code — a hand-written home
     directory, which is the same disclosure by hand.
  3. `env!("OUT_DIR")` that is NOT the argument of `include!`, `include_bytes!` or
     `include_str!`. Those three consume the path at compile time and put only the
     INCLUDED BYTES in the program, which is how the phone app is embedded
     (`src/phone/assets.rs`). Used as a value, `OUT_DIR` is the same leak as (1) wearing a
     different name, so the allowance is exactly as wide as the thing that needs it.

WHAT IS NOT A FINDING, AND WHY EACH EXEMPTION IS NARROW

  * TEST CODE. Anything inside a `#[cfg(test)]` module or function is skipped: it is
    compiled out of the shipped program by the same compiler. `ca.rs`'s word-list test
    reads the phone app's own file through `CARGO_MANIFEST_DIR` and is right to.
  * COMMENTS AND DOC COMMENTS. They do not reach the binary. `activation.rs` documents a
    measurement it took at `/Users/alex/Applications/RichOS.app/...`, which is evidence and
    stays.
  * PLACEHOLDER HOMES: `/Users/you/`, `/Users/example/`. Named for the same reason
    `no_host_paths.py` names them — they are invented fixture paths the product ships
    deliberately, they name no machine, and a check that refuses them is a check people
    learn to override.

Usage:
    no_compile_time_paths.py [ROOT ...] [--allow-home NAME]... [--quiet]
    no_compile_time_paths.py --self-test

Exit: 0 clean, 1 findings, 2 bad usage.
"""
from __future__ import annotations

import argparse
import os
import re
import sys

# Home-directory names that are placeholders rather than a machine.
PLACEHOLDER_HOMES = {"you", "example"}

HOST_PATH = re.compile(r"/(Users|Volumes)/([A-Za-z0-9_.-]+)(?![A-Za-z0-9_.-])")

# Rust accepts any whitespace inside a macro call, so `env!( "OUT_DIR" )` is the same call as
# `env!("OUT_DIR")` and must be found too. Comments are already removed from the scanned
# lines. A call split across lines is still not seen: the scan is per line.
_ENV_CALL = r'\benv\s*!\s*\(\s*"%s"\s*\)'
MANIFEST_DIR_MACRO = re.compile(_ENV_CALL % "CARGO_MANIFEST_DIR")
OUT_DIR_MACRO = re.compile(_ENV_CALL % "OUT_DIR")
OUT_DIR_INCLUDE = re.compile(
    r"\binclude(_bytes|_str)?\s*!\s*\(\s*concat\s*!\s*\(\s*" + _ENV_CALL % "OUT_DIR"
)


class Finding:
    def __init__(self, path: str, line: int, kind: str, text: str) -> None:
        self.path = path
        self.line = line
        self.kind = kind
        self.text = text

    def __str__(self) -> str:
        return f"{self.path}:{self.line}: {self.kind}\n      {self.text.strip()}"


def scan_text(source: str, path: str, allow_homes: set[str]) -> list[Finding]:
    """Findings in one Rust file.

    The scan is a small lexer rather than a set of regular expressions over raw lines,
    because the three things it has to tell apart — code, comments and string literals —
    are exactly the three a regular expression cannot tell apart. A `//` inside a string
    is not a comment; a `/Users/` inside a comment is not program data.
    """
    findings: list[Finding] = []
    i = 0
    n = len(source)
    line = 1
    depth = 0
    # Brace depth at which the current `#[cfg(test)]` item ends, if we are in one.
    test_until: int | None = None
    pending_cfg_test = False
    # SHIPPED CODE, REBUILT LINE BY LINE: every token that is not a comment and not inside
    # test code, string literals included with their quotes. This is the text the macro
    # checks read, and rebuilding it here rather than re-reading the file is the whole
    # point — a line can hold a `#[cfg(test)] fn f() { … }` whose body must be skipped
    # while the rest of the line is not, and a `//` inside a string is not a comment.
    # The first version of this file did re-read the original lines, and its own self-test
    # caught both of those within a minute of being written.
    code_lines: dict[int, str] = {}
    strings: list[tuple[int, str]] = []  # (line, literal contents) outside test code

    def in_test() -> bool:
        return test_until is not None

    def emit(at: int, text: str) -> None:
        if not in_test():
            code_lines[at] = code_lines.get(at, "") + text

    while i < n:
        ch = source[i]

        if ch == "\n":
            line += 1
            i += 1
            continue

        # Comments.
        if source.startswith("//", i):
            end = source.find("\n", i)
            i = n if end == -1 else end
            continue
        if source.startswith("/*", i):
            nesting = 1
            i += 2
            while i < n and nesting:
                if source.startswith("/*", i):
                    nesting += 1
                    i += 2
                elif source.startswith("*/", i):
                    nesting -= 1
                    i += 2
                else:
                    if source[i] == "\n":
                        line += 1
                    i += 1
            continue

        # Raw strings: r"...", r#"..."#, br#"..."#
        raw = re.match(r'(b?r)(#*)"', source[i:])
        if raw and (i == 0 or not (source[i - 1].isalnum() or source[i - 1] == "_")):
            hashes = raw.group(2)
            start_line = line
            opener = i
            i += len(raw.group(0))
            terminator = '"' + hashes
            end = source.find(terminator, i)
            if end == -1:
                end = n
            body = source[i:end]
            line += body.count("\n")
            if not in_test():
                strings.append((start_line, body))
            i = end + len(terminator)
            emit(start_line, source[opener:i])
            continue

        # Ordinary strings, with escapes.
        if ch == '"' or (ch == "b" and source.startswith('b"', i)):
            start_line = line
            opener = i
            i += 2 if ch == "b" else 1
            body_chars: list[str] = []
            while i < n:
                if source[i] == "\\":
                    body_chars.append(source[i : i + 2])
                    if source[i + 1 : i + 2] == "\n":
                        line += 1
                    i += 2
                    continue
                if source[i] == '"':
                    i += 1
                    break
                if source[i] == "\n":
                    line += 1
                body_chars.append(source[i])
                i += 1
            if not in_test():
                strings.append((start_line, "".join(body_chars)))
            emit(start_line, source[opener:i])
            continue

        # Character literals, told apart from lifetimes: `'a'` and `'\n'` are literals,
        # `'a` on its own is a lifetime. Getting this wrong would swallow the rest of the
        # file as a string.
        if ch == "'":
            literal = re.match(r"'(\\.|[^\\'])'", source[i:])
            width = len(literal.group(0)) if literal else 1
            emit(line, source[i : i + width])
            i += width
            continue

        # Attributes: only `#[cfg(test)]` matters, and it arms the NEXT item.
        if source.startswith("#[cfg(test)]", i):
            pending_cfg_test = True
            i += len("#[cfg(test)]")
            continue

        if ch == "{":
            depth += 1
            if pending_cfg_test and test_until is None:
                test_until = depth
                pending_cfg_test = False
            i += 1
            continue
        if ch == "}":
            if test_until is not None and depth == test_until:
                test_until = None
            depth -= 1
            i += 1
            continue
        if ch == ";" and pending_cfg_test:
            # `#[cfg(test)] mod tests;` — a whole file we do not see from here. Nothing to
            # skip locally; disarm so it cannot capture the next item.
            pending_cfg_test = False
            i += 1
            continue

        emit(line, source[i])
        i += 1

    for ln in sorted(code_lines):
        stripped = code_lines[ln]
        raw_line = stripped
        if MANIFEST_DIR_MACRO.search(stripped):
            findings.append(
                Finding(
                    path,
                    ln,
                    'env!("CARGO_MANIFEST_DIR") in shipped code — this is the directory the '
                    "crate was compiled in, on the machine that compiled it, and it reaches the "
                    "customer as a string in the binary",
                    raw_line,
                )
            )
        if OUT_DIR_MACRO.search(stripped) and not OUT_DIR_INCLUDE.search(stripped):
            findings.append(
                Finding(
                    path,
                    ln,
                    'env!("OUT_DIR") used as a value — only `include!`, `include_bytes!` and '
                    "`include_str!` may consume it, because those put the included bytes in the "
                    "program and not the path",
                    raw_line,
                )
            )

    for ln, body in strings:
        for match in HOST_PATH.finditer(body):
            kind, who = match.group(1), match.group(2)
            if kind == "Users" and who in allow_homes:
                continue
            findings.append(
                Finding(
                    path,
                    ln,
                    f"a machine directory in a string literal (/{kind}/{who}) - shipped code must "
                    "not name a machine storage directory",
                    body if len(body) < 160 else body[:157] + "...",
                )
            )

    return findings


def scan_tree(root: str, allow_homes: set[str]) -> tuple[list[Finding], int]:
    findings: list[Finding] = []
    files = 0
    if os.path.isfile(root):
        candidates = [root]
    else:
        candidates = []
        for directory, dirnames, filenames in os.walk(root):
            dirnames[:] = [d for d in dirnames if d not in {"target", "gen", "node_modules"}]
            candidates.extend(
                os.path.join(directory, f) for f in filenames if f.endswith(".rs")
            )
    for path in sorted(candidates):
        files += 1
        with open(path, encoding="utf-8", errors="replace") as handle:
            findings.extend(scan_text(handle.read(), path, allow_homes))
    return findings, files


SELF_TEST_CASES = [
    ("a machine volume literal is refused", 'fn f() { let p = "/Volumes/Fixture/cache"; }', 1), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    ("a bare machine volume is refused", 'fn f() { let p = "/Volumes/Fixture"; }', 1), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default

    (
        "a CARGO_MANIFEST_DIR in shipped code is a finding",
        'fn f() { let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../x"); }',
        1,
    ),
    (
        "the same line inside a cfg(test) module is not",
        '#[cfg(test)]\nmod tests {\n    fn f() { let p = env!("CARGO_MANIFEST_DIR"); }\n}\n',
        0,
    ),
    (
        "the same line inside a cfg(test) function is not",
        '#[cfg(test)]\nfn f() { let p = env!("CARGO_MANIFEST_DIR"); }\n',
        0,
    ),
    (
        "and code AFTER a cfg(test) module is scanned again",
        '#[cfg(test)]\nmod tests {\n    fn t() {}\n}\nfn g() { let p = env!("CARGO_MANIFEST_DIR"); }\n',
        1,
    ),
    (
        "a spaced env! CARGO_MANIFEST_DIR is still a finding",
        'fn f() { let p = env!( "CARGO_MANIFEST_DIR" ); }',
        1,
    ),
    (
        "a spaced env ! CARGO_MANIFEST_DIR is still a finding",
        'fn f() { let p = env ! ( "CARGO_MANIFEST_DIR" ); }',
        1,
    ),
    (
        "a spaced env! OUT_DIR used as a value is still a finding",
        'fn f() { let d = env!( "OUT_DIR" ); }',
        1,
    ),
    (
        "a spaced env! OUT_DIR consumed by include! is not a finding",
        'mod g { include!( concat!( env!( "OUT_DIR" ), "/a.rs" ) ); }',
        0,
    ),
    (
        "a home directory in a string literal is a finding",
        'fn f() { let p = "/Users/alex/ab/richos"; }', # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        1,
    ),
    (
        "a placeholder home is not",
        'fn f() { let p = "/Users/example/Projects/x"; let q = "/Users/you/Desktop"; }',
        0,
    ),
    (
        "a home directory in a comment is not",
        "// measured at /Users/alex/Applications/RichOS.app\nfn f() {}", # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        0,
    ),
    (
        "a home directory in a doc comment is not",
        "/// measured at /Users/alex/Applications/RichOS.app\nfn f() {}", # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        0,
    ),
    (
        "a home directory in a block comment is not",
        "/* measured at /Users/alex/Applications/RichOS.app */\nfn f() {}", # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        0,
    ),
    (
        "OUT_DIR consumed by include! is not a finding",
        'mod generated { include!(concat!(env!("OUT_DIR"), "/phone_app.rs")); }',
        0,
    ),
    (
        "OUT_DIR consumed by include_bytes! is not a finding",
        'static A: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/phone/app.js"));',
        0,
    ),
    (
        "OUT_DIR used as a value IS a finding",
        'fn f() { let d = env!("OUT_DIR"); }',
        1,
    ),
    (
        "a `//` inside a string is not a comment, so what follows is still scanned",
        'fn f() { let u = "https://richos"; let p = env!("CARGO_MANIFEST_DIR"); }',
        1,
    ),
    (
        "a lifetime is not a character literal, so the rest of the file is still scanned",
        'fn f<\'a>(x: &\'a str) {}\nfn g() { let p = env!("CARGO_MANIFEST_DIR"); }\n',
        1,
    ),
    (
        "a raw string carrying a home directory is a finding",
        'fn f() { let p = r#"/Users/alex/ab"#; }', # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        1,
    ),
    (
        "an escaped quote does not end the string early",
        'fn f() { let s = "a \\" /Users/alex/b"; }', # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        1,
    ),
]


def self_test() -> int:
    failures = 0
    for name, source, expected in SELF_TEST_CASES:
        got = len(scan_text(source, "<self-test>", PLACEHOLDER_HOMES))
        if got == expected:
            print(f"  ok    {name}")
        else:
            failures += 1
            print(f"  FAIL  {name}: expected {expected} finding(s), got {got}")
    print()
    if failures:
        print(f"{failures} of {len(SELF_TEST_CASES)} self-test case(s) failed")
        return 1
    print(f"all {len(SELF_TEST_CASES)} self-test cases pass")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("roots", nargs="*", default=[])
    parser.add_argument("--allow-home", action="append", default=[])
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    roots = args.roots
    if not roots:
        here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        roots = [os.path.join(os.path.dirname(here), "src-tauri", "src")]

    allow_homes = set(PLACEHOLDER_HOMES) | set(args.allow_home)

    findings: list[Finding] = []
    files = 0
    for root in roots:
        if not os.path.exists(root):
            print(f"no-compile-time-paths: {root} does not exist", file=sys.stderr)
            return 2
        found, count = scan_tree(root, allow_homes)
        findings.extend(found)
        files += count

    if findings:
        print(
            f"no-compile-time-paths: {len(findings)} finding(s) in shipped source across "
            f"{files} file(s). A path baked in at compile time reaches the customer as a "
            f"string in the binary, and --remap-path-prefix cannot touch it.\n"
        )
        for finding in findings:
            print(f"  {finding}")
        print(
            "\nFix: resolve it at runtime, or embed the CONTENTS at compile time the way "
            "`app/src-tauri/build.rs`'s `embed_phone` does (include_bytes! keeps the bytes and "
            "drops the path). If the line is test-only, move it inside `#[cfg(test)]`."
        )
        return 1

    if not args.quiet:
        print(
            f"no-compile-time-paths: clean — {files} Rust file(s), no compile-time path and no "
            "home directory in shipped code."
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
