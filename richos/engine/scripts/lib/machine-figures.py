#!/usr/bin/env python3
"""machine-figures.py: ONE MACHINE'S OWN FIGURES DO NOT GO INTO PUBLIC CODE.

THE GAP. A branch passed its review and still put one Mac's measurements into the public
repository: a production comment with the host's wired-memory figures, and test fixtures named
for "this Mac" and a nightly number carrying its total RAM, its free memory and its private memory
floor. A review judges behavior; the publication guard checked names and private paths. Nothing
looked for a machine's own measured values. This does, at `git commit`, in a repository that
declares a publication boundary (guard-publication-commits.sh runs it beside the device-identifier
scan).

WHAT IT FLAGS, in the lines a commit ADDS (a line already in HEAD is never judged again):
  ram     the host's own RAM size, read at run time (`sysctl -n hw.memsize`): its byte count,
          its size in MB (with a memory word on the line: mb, gb, ram, mem, total, free,
          floor, wired, avail, limit), or "<N> GB" / "<N> GiB" / "<N>G"
  limit   a value from the machine's PRIVATE limits file, read at run time
          (RICHOS_LIMITS_FILE, else ~/Library/Application Support/RichOS/limits.env): a number of
          three digits or more, with a memory word on the line
  label   an incident label in a comment or a test file: "nightly NN" (nightly 80, nightly_80,
          nightly-80), the fixture-name form this_mac / this-mac, and "this Mac" in a comment
          or a test file when the same line carries a measurement (a number with a unit) or a
          nightly label

WHAT IT LEAVES ALONE: "this Mac" that means the user's Mac (a string the app shows, "only a
restart of this Mac does", and a comment with no figure, "changes this Mac's display"); documents
(.md, .txt, .log, .rst and everything under docs/), where a dated record is the point; and
synthetic values (a fixture whose total is not this host's RAM passes).

NO VALUE IS WRITTEN HERE. Every figure it looks for is read from the host and the private file
when it runs, so this file and its suite carry none.

THE FOUR VERDICTS (first line of output, tab-separated), as device-identifiers.py:
  CLEAN   nothing matched
  FOUND   following lines are `<path>:<line><TAB><rule><TAB><what>`
  BROKEN  a staged blob could not be read, or the manifest could not be; the caller refuses

Usage:
  machine-figures.py --scan-manifest <manifest> --repo <repo>   rows: path<TAB>blob[<TAB>...]
  machine-figures.py --facts                                     what it would look for (counts)
Suite: scripts/lib/machine-figures.test.sh.
"""
import collections
import os
import re
import subprocess
import sys

SELF = {"machine-figures.py", "machine-figures.test.sh"}
DOC_EXT = {".md", ".markdown", ".txt", ".log", ".rst", ".csv"}
MEMORY_WORD = re.compile(r"mb|mib|gb|gib|ram|mem|total|free|floor|wired|avail|limit", re.I)
MEASUREMENT = re.compile(r"\b\d+(?:\.\d+)?\s?(?:[KMGT]i?B|[KMGT]\b|%|ms\b|GHz|cores?\b)", re.I)
# Alphanumeric lookarounds, not \b: an underscore is a word character, so \b never fires inside
# the identifiers these are for (test_nightly_80_..., floor_for_this_mac).
NIGHTLY = re.compile(r"(?<![A-Za-z0-9])nightly[\s_-]?\d{2,}(?!\d)", re.I)
THIS_MAC = re.compile(r"(?<![A-Za-z0-9])this\s+mac(?![A-Za-z0-9])", re.I)
THIS_MAC_NAME = re.compile(r"(?<![A-Za-z0-9])this[_-]mac(?![A-Za-z0-9])", re.I)
TEST_PATH = re.compile(r"(?:^|/)(?:tests?|fixtures?|testdata)/|\.test\.|_test\.|(?:^|/)test_|\.spec\.", re.I)
COMMENT = re.compile(r"(?:^|\s)(?:#|//|--|/\*|\*)\s?(.*)$")


def limits_path():
    env = os.environ.get("RICHOS_LIMITS_FILE")
    if env:
        return env
    return os.path.join(os.path.expanduser("~"), "Library", "Application Support", "RichOS", "limits.env")


def host_memsize():
    """The host's RAM in bytes, or None. RICHOS_MACHINE_FIGURES_MEMSIZE stands in (tests)."""
    forced = os.environ.get("RICHOS_MACHINE_FIGURES_MEMSIZE", "").strip()
    if forced.isdigit():
        return int(forced)
    try:
        out = subprocess.run(["/usr/sbin/sysctl", "-n", "hw.memsize"], capture_output=True, text=True, timeout=10)
        if out.returncode == 0 and out.stdout.strip().isdigit():
            return int(out.stdout.strip())
    except (OSError, subprocess.SubprocessError):
        pass
    try:
        with open("/proc/meminfo", encoding="utf-8") as fh:
            for line in fh:
                if line.startswith("MemTotal:"):
                    return int(line.split()[1]) * 1024
    except (OSError, ValueError, IndexError):
        pass
    return None


def limit_values():
    """Numbers of three digits or more from the private limits file (KEY=VALUE lines)."""
    values = set()
    try:
        with open(limits_path(), encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line or line.startswith("#") or "=" not in line:
                    continue
                value = line.split("=", 1)[1].strip().strip("'\"")
                if re.fullmatch(r"\d{3,}", value):
                    values.add(value)
    except OSError:
        pass
    return values


class Facts:
    def __init__(self):
        size = host_memsize()
        self.ram_bytes = str(size) if size else None
        self.ram_mb = str(size // (1024 * 1024)) if size else None
        self.ram_gb = str(size // (1024 ** 3)) if size else None
        self.limits = limit_values()

    def ram_findings(self, line):
        out = []
        if not self.ram_bytes:
            return out
        if re.search(r"(?<!\d)%s(?!\d)" % self.ram_bytes, line):
            out.append(("ram", "this host's RAM size in bytes"))
        elif re.search(r"(?<![\d.])%s(?![\d.])" % self.ram_mb, line) and MEMORY_WORD.search(line):
            out.append(("ram", "this host's RAM size in MB"))
        elif re.search(r"(?<![\d.])%s\s?(?:GB|GiB|G)\b" % self.ram_gb, line):
            out.append(("ram", "this host's RAM size in GB"))
        return out

    def limit_findings(self, line):
        if not MEMORY_WORD.search(line):
            return []
        for value in sorted(self.limits):
            if re.search(r"(?<![\d.])%s(?![\d.])" % value, line):
                return [("limit", "a value from this machine's private limits file")]
        return []


def label_findings(line, is_test):
    comment = COMMENT.search(line)
    judged = []
    if comment:
        judged.append(comment.group(1))
    if is_test:
        judged.append(line)
    out = []
    for text in judged:
        if NIGHTLY.search(text):
            out.append(("label", "an incident label (%s)" % NIGHTLY.search(text).group(0)))
            break
        if is_test and THIS_MAC_NAME.search(text):
            out.append(("label", "a fixture named for one machine (%s)" % THIS_MAC_NAME.search(text).group(0)))
            break
        if THIS_MAC.search(text) and MEASUREMENT.search(text):
            # In a test file, "this Mac" inside a quoted string is a user-facing string under test.
            bare = re.sub(r"\"[^\"]*\"|'[^']*'", "", text) if text is line else text
            if THIS_MAC.search(bare):
                out.append(("label", "\"this Mac\" beside a measurement: a description of one machine"))
                break
    return out


def is_document(path):
    lower = path.lower()
    return lower.startswith("docs/") or "/docs/" in lower or os.path.splitext(lower)[1] in DOC_EXT


def added_lines(repo, path, text):
    """(line number, line) for each line of `text` not already in HEAD's copy of `path`."""
    old = ""
    if repo:
        try:
            out = subprocess.run(["git", "-C", repo, "show", "HEAD:%s" % path], capture_output=True, timeout=60)
            if out.returncode == 0:
                old = out.stdout.decode("utf-8", "replace")
        except (OSError, subprocess.SubprocessError):
            old = ""
    remaining = collections.Counter(old.splitlines())
    for n, line in enumerate(text.splitlines(), 1):
        if remaining[line] > 0:
            remaining[line] -= 1
            continue
        yield n, line


def scan(rows, repo, facts=None):
    facts = facts or Facts()
    found, unread = [], []
    for path, blob in rows:
        if os.path.basename(path) in SELF or is_document(path):
            continue
        try:
            with open(blob, "rb") as fh:
                text = fh.read().decode("utf-8", "replace")
        except OSError:
            unread.append(path)
            continue
        is_test = bool(TEST_PATH.search(path))
        for n, line in added_lines(repo, path, text):
            hits = facts.ram_findings(line) or facts.limit_findings(line) or label_findings(line, is_test)
            for rule, what in hits:
                found.append("%s:%d\t%s\t%s" % (path, n, rule, what))
    return found, unread


def scan_manifest(manifest, repo):
    rows = []
    try:
        with open(manifest, encoding="utf-8") as fh:
            for row in fh:
                parts = row.rstrip("\n").split("\t")
                # The guard's third column is "identity" for a binary blob: no text lines to judge.
                if len(parts) >= 2 and parts[0] and not (len(parts) > 2 and parts[2] == "identity"):
                    rows.append((parts[0], parts[1]))
    except OSError as err:
        print("BROKEN\tthe manifest could not be read (%s)" % err)
        return 0
    found, unread = scan(rows, repo)
    if found:
        print("FOUND")
        for line in found:
            print(line)
    elif unread:
        print("BROKEN\t%d staged file(s) could not be read, so they were not checked: %s"
              % (len(unread), ", ".join(unread[:5])))
    else:
        print("CLEAN")
    return 0


def main(argv):
    if len(argv) == 5 and argv[1] == "--scan-manifest" and argv[3] == "--repo":
        return scan_manifest(argv[2], argv[4])
    if len(argv) == 2 and argv[1] == "--facts":
        f = Facts()
        print("ram: %s; limits file: %s (%d value(s))" % ("read" if f.ram_bytes else "unknown",
                                                          limits_path(), len(f.limits)))
        return 0
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
