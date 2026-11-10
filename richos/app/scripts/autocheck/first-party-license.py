#!/usr/bin/env python3
"""first-party-license.py — RichOS's own material is under the root AGPL-3.0 and no other license.

WHY (2026-11-09/10). RichPass reached public richos carrying its own `LICENSE` (MIT) and a
comment in `RichPass.html` putting its own source under MIT too, both copied from the author's
PassBooster. The CEO never authorized that license for RichPass, and nothing
refused it: it was published. Public main was rebuilt with RichPass under the AGPL-3.0 like the
rest of RichOS (5c3cda46f), and he asked for a guarantee that it never happens again. This check
refuses the next one, at the commit and again at the land, whoever makes it.

WHAT IS REFUSED, in first-party material (anything not registered as third-party):

  - a license file (LICENSE in either spelling, COPYING, UNLICENSE, LICENSE-x.txt, x-LICENSE
    and the like) anywhere but the root, and a root LICENSE that is no longer the GNU AGPL-3.0;
  - an SPDX identifier other than AGPL-3.0-only or AGPL-3.0-or-later;
  - a manifest license field (package.json, Cargo.toml, pyproject.toml) other than those two
    (or npm's "SEE LICENSE IN ..." and "UNLICENSED", which grant no other license);
  - a license tag (@license, License:) or a sentence that declares a file is licensed,
    released or distributed under another license (MIT, BSD, Apache, GPL, MPL, CC and the
    like) or put in the public domain. A sentence that says something is NOT licensed that way
    (the brand exclusion in docs/legal/BRAND-ASSETS.md) is not a declaration.

WHAT PASSES: everything under a path `.richos/vendored-material` records with origin
`third-party`, which keeps its own license; the third-party notices themselves
(docs/legal/THIRD-PARTY-*.md, docs/legal/*PROVENANCE*.md and the registry), whose job is to
state those licenses; lockfiles, which record dependencies' licenses; and, inside a RichOS file,
a third-party notice reproduced between a `BEGIN VENDORED` and an `END VENDORED` line, or after
a line saying the embedded work keeps its own license until that comment closes (how
RichPass.html carries the notices of the code and fonts it embeds), or one line carrying
`third-party-notice: <whose>` (a script that writes a bundled program's notice). A line in such a
region, or carrying that mark, that names a RichOS product (RichOS, RichPass, ...) or
"first-party" is still judged: those hold other people's notices, never RichOS's own license.

  first-party-license.py [--what W]             the commit and land check: what the index
                                                adds to HEAD (autocheck.py runs HEAD's copy)
  first-party-license.py --range A..B           what B adds to A (the push backstop)
  first-party-license.py --all [--rev R]        every tracked file, in the index or at R, as if
                                                it were added now: proves a tree has nothing
                                                this check would refuse

Exit 0: nothing refused. Exit 1: refused, each finding named by file and line with what to do.
Exit 2: the check could not run (an unreadable registry, a git failure); a caller refuses.
Standard library only; it reads git, never the working tree.
"""
import re
import subprocess
import sys

REGISTRY = ".richos/vendored-material"
ROOT_LICENSE = "LICENSE"
AGPL_TITLE = "GNU AFFERO GENERAL PUBLIC LICENSE"
EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
SELF = "richos/app/scripts/autocheck/first-party-license.py"
ALLOWED_IDS = {"agpl-3.0-only", "agpl-3.0-or-later", "agpl-3.0"}

# Files that record other people's licenses by design.
LOCKFILES = {"package-lock.json", "npm-shrinkwrap.json", "yarn.lock", "pnpm-lock.yaml", "Cargo.lock",
             "Podfile.lock", "Gemfile.lock", "poetry.lock", "uv.lock", "composer.lock", "Package.resolved",
             "gradle.lockfile"}


def notices_file(path):
    if path == REGISTRY:
        return True
    head, _, name = path.rpartition("/")
    return head == "docs/legal" and (name.startswith("THIRD-PARTY-") or "PROVENANCE" in name)


# A license file by its name. A code or data file whose name only contains the word
# (dependency-license-inventory.sh, licenses.json) is not one.
LICENSE_NAME = re.compile(r"^(?:(?:un)?licen[cs]e|copying)(?:[._-][^/]*)?$"
                          r"|^[^/]*[._-](?:licen[cs]e|copying)(?:\.(?:txt|md|markdown|rst))?$", re.I)
CODE_SUFFIXES = {"py", "sh", "bash", "zsh", "js", "mjs", "cjs", "ts", "tsx", "jsx", "rs", "swift", "kt", "kts",
                 "java", "go", "rb", "c", "h", "cc", "cpp", "m", "mm", "svelte", "vue", "css", "scss", "html",
                 "json", "toml", "yml", "yaml", "lock", "xml", "plist", "png", "svg"}


def license_file(path):
    name = path.rsplit("/", 1)[-1]
    suffix = name.rsplit(".", 1)[-1].lower() if "." in name else ""
    return suffix not in CODE_SUFFIXES and bool(LICENSE_NAME.match(name))


# Licenses other than RichOS's own. Anchored where they start, so "the GNU Affero General Public
# License" and "AGPL-3.0" never match: only RichOS's license contains "Affero".
OTHER = (r"(?:MIT|X11|ISC|0BSD|BSD|Apache|L?GPL|GPLv?\d|MPL|Mozilla\s+Public|Unlicen[cs]e|CC0|CC[- ]BY"
         r"|Creative\s+Commons|WTFPL|zlib|Boost\s+Software|BSL-1|EPL|Eclipse\s+Public|Artistic|OFL"
         r"|SIL\s+Open\s+Font|PSF|Python\s+Software\s+Foundation|BlueOak|Unicode|PostgreSQL|Ms-PL|CDDL|EUPL"
         r"|Elastic\s+License|SSPL|Server\s+Side\s+Public|BUSL|Business\s+Source"
         r"|(?:GNU\s+)?(?:Lesser\s+|Library\s+)?General\s+Public)\b")
COPULA = r"\b(?:is|are|be|been|being|was|were|remains?|stays?)\s+(?:[\w-]+[\s,]+){0,3}?"
UNDER = r"\s+under\s+(?:the\s+)?(?:terms\s+of\s+(?:the\s+)?)?(?:an?\s+)?(?:GNU\s+)?"
DECLARATIONS = [
    # "<it> is licensed / released / distributed / made available under the <other> License"
    re.compile(COPULA + r"(?:re-?)?(?:licen[cs]ed|released|distributed|made\s+available|available|published"
               r"|offered|provided|covered)" + UNDER + OTHER, re.I),
    # a header that starts with it: "Licensed under the <other> License, Version 2.0"
    re.compile(r"^[\W_]*(?:dual[- ]?|also\s+)?licen[cs]ed" + UNDER + OTHER, re.I),
    # "<it> is <other> licensed", "is BSD-3-Clause licensed"
    re.compile(COPULA + r"(?:an?\s+)?" + OTHER + r"[\w.+-]*[- ]licen[cs]ed\b", re.I),
    # tags and fields: @license <other>, License: <other>, "license": "<other>", license = "<other>"
    re.compile(r"(?:@licen[cs]e\b|\blicen[cs]e\s*[:=]|\"license\"\s*:)\s*[{\[(]?\s*(?:text\s*=\s*)?[\"'`]?\s*" + OTHER,
               re.I),
    # "<it> is released into the public domain"
    re.compile(COPULA + r"(?:released\s+|dedicated\s+|placed\s+)?(?:in|into|to)\s+the\s+public\s+domain\b", re.I),
]
NEGATED = re.compile(r"\b(?:not|never|neither|nor)\b|n't\b", re.I)
SPDX = re.compile(r"SPDX-License-Identifier[:]\s*(.*)", re.I)
SPDX_EXPRESSION = re.compile(r"[A-Za-z0-9.+-]+(?:\s+(?:OR|AND|WITH)\s+[A-Za-z0-9.+-]+)*")
MANIFEST_FIELD = {
    "package.json": re.compile(r"\"license\"\s*:\s*\"([^\"]*)\""),
    "Cargo.toml": re.compile(r"^\s*license\s*=\s*\"([^\"]*)\""),
    "pyproject.toml": re.compile(r"^\s*license\s*=\s*\"([^\"]*)\""),
}
CANDIDATE = re.compile(r"licen[cs]|public\s+domain|SPDX", re.I)

# A RichOS file reproducing other people's notices says where they are.
VENDORED_BEGIN = re.compile(r"\bBEGIN VENDORED\b")
VENDORED_END = re.compile(r"\bEND VENDORED\b")
KEEPS_OWN = re.compile(r"\bkeeps?\s+(?:its|their)\s+own\s+licen[cs]es?\b", re.I)
COMMENT_CLOSE = re.compile(r"-->|\*/")
# One line that states a third party's license (a notice a script writes for a bundled program)
# says so on that line, with whose it is, where a reviewer sees it; a bare marker exempts nothing.
NOTICE_MARK = re.compile(r"\bthird-party-notice:[ \t]*[\w(]")
OURS = re.compile(r"\bRich[A-Z]\w*|\b(?i:first[- ]party)\b")

WHAT_TO_DO = [
    "Everything RichOS writes is under the GNU AGPL-3.0 of the root LICENSE, and nothing in it may",
    "carry or declare another license. For each finding above, one of:",
    "  - it is RichOS's own work: remove the license file or the line (the root LICENSE already",
    "    covers it; a manifest field says AGPL-3.0-only);",
    "  - it is somebody else's work: register it as third-party in the same change, a row in",
    f"    {REGISTRY} (origin third-party, its license, holder and source) and its entry",
    "    in docs/legal/THIRD-PARTY-NOTICES.md. A third-party notice reproduced inside a RichOS file",
    "    goes between a BEGIN VENDORED line and an END VENDORED line, or, for one line, carries",
    "    'third-party-notice: <whose>' (a script writing a bundled program's notice).",
    "Only the CEO authorizes a license for RichOS's own work.",
]


class Broken(Exception):
    pass


def git(*args):
    result = subprocess.run(["git", "-c", "core.quotePath=false", *args], capture_output=True,
                            stdin=subprocess.DEVNULL)
    if result.returncode:
        raise Broken(f"git {' '.join(args)} failed: {result.stderr.decode(errors='replace').strip()}")
    return result.stdout


def unquote(path):
    """A path as git prints it in a patch header (C-quoted when it holds a special byte)."""
    if path.startswith('"') and path.endswith('"'):
        return path[1:-1].encode("latin-1", "backslashreplace").decode("unicode_escape") \
            .encode("latin-1").decode("utf-8", errors="surrogateescape")
    return path


class Blobs:
    """`git cat-file --batch`, one process for every blob read."""

    def __init__(self):
        self.proc = subprocess.Popen(["git", "cat-file", "--batch"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)

    def read(self, spec):
        self.proc.stdin.write(spec.encode("utf-8", errors="surrogateescape") + b"\n")
        self.proc.stdin.flush()
        header = self.proc.stdout.readline().decode(errors="replace").split()
        if len(header) < 3 or header[-1] in ("missing", "ambiguous"):
            return None
        size = int(header[2])
        data = self.proc.stdout.read(size)
        self.proc.stdout.read(1)
        return data if header[1] == "blob" else None

    def close(self):
        self.proc.stdin.close()
        self.proc.wait()


def third_party_paths(text):
    """The paths the registry records as somebody else's work."""
    found = []
    for number, line in enumerate(text.splitlines(), 1):
        if not line.strip() or line.startswith("#") or re.match(r"^[A-Z_]+=", line):
            continue
        fields = line.split("\t")
        if len(fields) != 10:
            raise Broken(f"{REGISTRY} line {number} has {len(fields)} tab-separated fields; an entry has 10")
        if fields[1] == "third-party":
            found += [p.strip().removeprefix("./").rstrip("/") for p in fields[0].split(",") if p.strip()]
    return found


def covered(path, paths):
    return any(path == p or path.startswith(p + "/") for p in paths)


def regions(lines):
    """Line numbers (1-based) inside a reproduced third-party notice in a RichOS file."""
    inside = set()
    start = None
    for number, line in enumerate(lines, 1):
        if VENDORED_BEGIN.search(line):
            start = number
        elif VENDORED_END.search(line) and start is not None:
            inside.update(range(start, number + 1))
            start = None
    for number, line in enumerate(lines, 1):
        if KEEPS_OWN.search(line):
            for end in range(number, len(lines) + 1):
                if COMMENT_CLOSE.search(lines[end - 1]):
                    inside.update(range(number, end + 1))
                    break
    return inside


def judge_line(path, line):
    """What this line declares, or None."""
    name = path.rsplit("/", 1)[-1]
    found = SPDX.search(line)
    if found:
        expression = SPDX_EXPRESSION.match(found.group(1))
        value = expression.group(0) if expression else ""
        if value.lower() not in ALLOWED_IDS:
            return f"an SPDX identifier other than AGPL-3.0 ({value or 'empty'})"
    field = MANIFEST_FIELD.get(name)
    if field and (found := field.search(line)):
        value = found.group(1).strip()
        if value.lower() not in ALLOWED_IDS and value != "UNLICENSED" and not value.startswith("SEE LICENSE IN"):
            return f"a {name} license field other than AGPL-3.0 ({value})"
    for pattern in DECLARATIONS:
        for found in pattern.finditer(line):
            if not NEGATED.search(found.group(0)):
                return "a license declaration other than AGPL-3.0"
    return None


def judge_file(path, data, added):
    """Findings for one file's new content; `added` is the set of added line numbers, or None for all."""
    if data is None or b"\0" in data[:8192]:
        return []
    text = data.decode("utf-8", errors="replace")
    if path == ROOT_LICENSE:
        if AGPL_TITLE not in text[:400]:
            first = text.strip().splitlines()[0] if text.strip() else "(empty)"
            return [(path, 1, "the root LICENSE is no longer the GNU AGPL-3.0", first)]
        return []
    lines = text.splitlines()
    hits = []
    for number, line in enumerate(lines, 1):
        if added is not None and number not in added:
            continue
        if not CANDIDATE.search(line):
            continue
        why = judge_line(path, line[:4000])
        if why:
            hits.append((number, why, line))
    if not hits:
        return []
    exempt = regions(lines)
    return [(path, number, why, line) for number, why, line in hits
            if OURS.search(line) or (number not in exempt and not NOTICE_MARK.search(line))]


def added_lines(patch):
    """{path: set of added line numbers} from a `git diff -U0` patch."""
    files = {}
    current = None
    for raw in patch.decode("utf-8", errors="surrogateescape").splitlines():
        if raw.startswith("+++ "):
            target = raw[4:]
            current = None if target == "/dev/null" else unquote(target).removeprefix("b/")
            if current is not None:
                files.setdefault(current, set())
        elif raw.startswith("@@") and current is not None:
            found = re.match(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@", raw)
            if found:
                start, count = int(found.group(1)), int(found.group(2) if found.group(2) is not None else 1)
                files[current].update(range(start, start + count))
    return files


def changed(args):
    """(paths the change adds or modifies, {path: added lines}) for `git diff <args>`."""
    names = [p for p in git("diff", "--no-renames", "--name-only", "-z", "--diff-filter=d", *args)
             .decode("utf-8", errors="surrogateescape").split("\0") if p]
    patch = git("diff", "--no-renames", "--no-color", "--no-ext-diff", "-U0", "--diff-filter=d",
                "--src-prefix=a/", "--dst-prefix=b/", *args)
    return names, added_lines(patch)


def head_or_empty():
    result = subprocess.run(["git", "rev-parse", "--verify", "-q", "HEAD^{commit}"], capture_output=True, text=True,
                            stdin=subprocess.DEVNULL)
    return result.stdout.strip() if result.returncode == 0 else EMPTY_TREE


def scan(paths, added, side):
    """Every finding in `paths`; `side` is the blob prefix of the new content (":" for the index).
    `added`: {path: added line numbers}, or None to judge every line (--all)."""
    blobs = Blobs()
    try:
        registry = blobs.read(f"{side}{REGISTRY}")
        third = third_party_paths(registry.decode("utf-8", errors="replace")) if registry is not None else []
        findings = []
        for path in paths:
            if covered(path, third) or notices_file(path):
                continue
            if path != ROOT_LICENSE and license_file(path):
                findings.append((path, None, "a license file outside registered third-party material", ""))
                continue
            if path.rsplit("/", 1)[-1] in LOCKFILES:
                continue
            lines = added.get(path) if added is not None else None
            if added is not None and not lines and path != ROOT_LICENSE:
                continue  # only a mode change or a binary file: no added line to read
            findings += judge_file(path, blobs.read(f"{side}{path}"), lines)
        return findings
    finally:
        blobs.close()


def report(what, findings):
    def say(text=""):
        print(text, file=sys.stderr, flush=True)
    say("")
    say(f"=== {what.upper()} REFUSED: first-party material under a license other than RichOS's AGPL-3.0 ===")
    for path, number, why, line in findings:
        say(f"  {path}:{number}: {why}" if number else f"  {path}: {why}")
        if line.strip():
            say(f"      {line.strip()[:160]}")
    say("")
    for line in WHAT_TO_DO:
        say("  " + line)
    say(f"  Recheck: python3 {SELF}  (the whole tree: --all)")
    say("")


def main(argv):
    what, mode, rev, span = "commit", "index", None, None
    args = list(argv)
    while args:
        arg = args.pop(0)
        if arg == "--what" and args:
            what = args.pop(0)
        elif arg == "--range" and args:
            mode, span = "range", args.pop(0)
        elif arg == "--all":
            mode = "all"
        elif arg == "--rev" and args:
            rev = args.pop(0)
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            print(f"first-party-license: unknown argument {arg!r}", file=sys.stderr)
            return 2
    try:
        if mode == "all":
            listing = git("ls-tree", "-r", "-z", "--name-only", rev) if rev else git("ls-files", "-z", "--cached")
            paths = sorted({p for p in listing.decode("utf-8", errors="surrogateescape").split("\0") if p})
            findings = scan(paths, None, f"{rev}:" if rev else ":")
        elif mode == "range":
            base, _, tip = span.partition("..")
            if not base or not tip:
                print(f"first-party-license: --range needs A..B, not {span!r}", file=sys.stderr)
                return 2
            paths, added = changed([base, tip])
            findings = scan(paths, added, f"{tip}:")
        else:
            paths, added = changed(["--cached", head_or_empty()])
            findings = scan(paths, added, ":")
    except Broken as exc:
        print(f"\n=== {what.upper()} REFUSED: the first-party license check could not run ===\n  {exc}\n"
              "  A check that did not run is not a pass. Fix the cause and try again.\n", file=sys.stderr)
        return 2
    if findings:
        report(what, findings)
        return 1
    if mode == "all":
        print(f"first-party-license: {len(paths)} files, nothing refused", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
