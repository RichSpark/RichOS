#!/usr/bin/env python3
"""foreign_app_data.py — RichOS never reads another app's data. The rule, in one place.

WHAT HAPPENED. On 2026-09-24 and 2026-09-25 the CEO kept getting a macOS dialog:
"python3.14" would like to access data from other apps. Don't Allow / Allow. The unified log
names the cause exactly (private record:
richos-hq/docs/verification/2026-09-25-app-data-prompts/README.md). The disk watchdog's
launchd job (com.richos.disk-watchdog) runs Homebrew's python3.14. When an alert was
firing, it ran `du -sk` over every path in DISK_CONSUMER_CANDIDATES to name the biggest
consumers. One of those paths was the per-user folder where macOS keeps other apps' sandboxed
data. The kernel's System Policy stopped `du` at Voice Memos, Mail, Messages, Notes,
Safari, Maps, Home, Stocks and News. tccd then put up the "App Data" prompt, attributed to
the responsible process, python3.14.

WHY CLICKING "ALLOW" NEVER FIXED IT. The same log shows the CEO allowed it at 05:01:42.
The next run prompted again at 05:17:57, with tccd logging "Session scoped auth is invalid
for client". Homebrew's interpreter carries no stable code-signing identity, so macOS
tracks it by path, and a grant to a path-identified client lasts only for that
process. Every 15-minute run is a new process. The prompt cannot be answered away. The
only fix is not to read other apps' data at all.

WHAT MACOS PROTECTS. App Data protection, TCC service kTCCServiceSystemPolicyAppData:
since macOS 14, the per-app and per-daemon container folders under your Library; since
macOS 15, the shared app-group folders as well (the three are PROTECTED_RELATIVE below). Reading inside any of them, or walking a folder that holds them (your home
folder or its Library), is what triggers the prompt. A process started by RichOS.app is
attributed to RichOS, so for a user the prompt would say "RichOS".

THE TWO HALVES OF THE RULE, BOTH HERE:
  entering(path)   runtime: would measuring or walking `path` enter another app's data?
                   Callers that walk a configured list (the disk watchdog) refuse such a
                   path instead of walking it.
  scan(roots)      static: source lines that name those folders, or that start a recursive
                   walk at the home folder or its Library. Used by the engine's and the app's
                   test suites, so a violation fails a suite before it reaches anybody's Mac.

EXEMPTIONS ARE DECLARED, NEVER INFERRED. A line that must name one of these folders (this
file, or a test that builds a fake one in a sandbox) carries
`foreign-app-data-exempt: <reason>` on the same line, where a reviewer can see it. A bare
marker with no reason exempts nothing.

WHAT THE STATIC SCAN CANNOT SEE, stated so it is not over-trusted: a walk whose root comes from a
variable that holds your home folder is data flow, which a line scan cannot follow. The watchdog's
runtime guard covers its own configured list. An agent session's own shell commands are
not source code at all.

Usage:
  foreign_app_data.py scan [--exclude-tests] <file-or-dir>...   exit 0 clean, 1 findings, 2 bad input
  foreign_app_data.py check <path> [--home DIR]                 exit 0 allowed, 1 enters app data
  foreign_app_data.py --self-test
"""

import json
import os
import re
import shlex
import sys
import time

# The per-user folders macOS guards with App Data protection, relative to the home folder.
PROTECTED_RELATIVE = (
    os.path.join("Library", "Containers"),  # foreign-app-data-exempt: the rule's own list of protected folders
    os.path.join("Library", "Group Containers"),  # foreign-app-data-exempt: the rule's own list of protected folders
    os.path.join("Library", "Daemon Containers"),  # foreign-app-data-exempt: the rule's own list of protected folders
)

# THE REST OF THE USER'S PROTECTED DATA (2026-10-29). App Data is one TCC service of many.
# At 08:34:10 `du -sk ~/Library/Caches` (the disk watchdog's attribution walk, run by
# disk-watchdog.test.sh with the real home folder) reached ~/Library/Caches/com.apple.Music,
# sandboxd asked tccd for kTCCServiceSystemPolicyAllFiles and kTCCServiceMediaLibrary, and
# the CEO was asked whether "RichOS.app" may access Apple Music. ~/Library/Caches is
# neither inside nor above a container folder, so the rule above let it through. Desktop,
# Documents and Downloads are their own TCC services; Music, Movies and Pictures hold the
# media and photo libraries; ~/Library holds Mail, Messages, Safari, and Apple's caches.
# A walk of any of them, or of a folder that holds them, is refused. RichOS's OWN folders
# inside ~/Library (OWN_NAMES, one level below a Library area, e.g. Caches/RichOS,
# Application Support/com.richos.app) stay measurable: they are ours.
PROTECTED_USER_DIRS = ("Desktop", "Documents", "Downloads", "Movies", "Music", "Pictures",
                       "Library")
OWN_NAMES = ("RichOS", "com.richos.", "group.com.richos.")

# The per-user Darwin folders under /private/var/folders/<xx>/<id>/: T is every app's
# temporary folder (their TemporaryItems are sandbox-protected; measured 2026-10-29,
# `du(64366) deny(1) file-read-data .../T/com.apple.triald/TemporaryItems`), C is every
# app's cache. Walking T, C or anything above them is refused; a folder RichOS made
# inside T (a test sandbox, the scratch allocator) is not.
_DARWIN_USER = re.compile(
    r"^/private(?:/var(?:/folders(?:/[^/]+(?:/[^/]+(?:/[^/]+)?)?)?)?)?$")
_DARWIN_CACHE = re.compile(r"^/private/var/folders/[^/]+/[^/]+/C(?:/|$)")
_DATA_VOLUME = "/System/Volumes/Data"


def _home(home=None):
    return home or os.environ.get("HOME") or os.path.expanduser("~")


def _homes(home=None):
    """Every home folder the rule protects: the one named (or $HOME) AND the account's own.

    A test that points HOME at a fixture must not make the REAL home folder walkable:
    the account's home from the password database is protected whatever HOME says."""
    out = [os.path.realpath(_home(home))]
    try:
        import pwd
        out.append(os.path.realpath(pwd.getpwuid(os.getuid()).pw_dir))
    except (ImportError, KeyError, OSError):
        pass
    return list(dict.fromkeys(h for h in out if h and h != os.sep))


def protected_roots(home=None):
    return [os.path.join(h, rel) for h in _homes(home) for rel in PROTECTED_RELATIVE]


def _norm(path):
    p = os.path.realpath(os.path.expanduser(path)).rstrip(os.sep) or os.sep
    # The data volume is the same files as / through firmlinks.
    if p == _DATA_VOLUME or p.startswith(_DATA_VOLUME + os.sep):
        p = p[len(_DATA_VOLUME):] or os.sep
    return p


def _own(p, h):
    """True when `p` is at or below a RichOS folder one level inside a Library area."""
    lib = os.path.join(h, "Library") + os.sep
    if not p.startswith(lib):
        return False
    parts = p[len(lib):].split(os.sep)
    return len(parts) >= 2 and parts[1].startswith(OWN_NAMES)


def entering(path, home=None):
    """A sentence saying why walking `path` would enter macOS-protected data, or None.

    Two shapes, both walks `du -sk` or os.walk would make:
      inside   the path is a protected folder or below one: another app's container, a
               user folder macOS guards (Desktop, Documents, Downloads, Movies, Music,
               Pictures, Library), or the per-user Darwin cache folder
      holds    the path is an ancestor of one: the home folder, /Users, /, the per-user
               Darwin temporary folder and those above it
    RichOS's own folders inside ~/Library are allowed. The comparison is on resolved
    paths, so a symlink pointing into protected data is caught.
    """
    if not path:
        return None
    p = _norm(path)
    if p == os.sep:
        return "%s holds every user's data, so walking it walks other apps' data" % path
    if _DARWIN_USER.match(p) or _DARWIN_CACHE.match(p):
        return ("%s is or holds the per-user folders where macOS keeps other apps' "
                "temporary files and caches" % path)
    for h in _homes(home):
        for root in protected_roots(home):
            r = _norm(root)
            if not r.startswith(h + os.sep):
                continue
            if (p == r or p.startswith(r + os.sep)) and not _own(p, h):
                return "%s is inside %s, which macOS protects as another app's data" % (path, r)
        for name in PROTECTED_USER_DIRS:
            r = os.path.join(h, name)
            if (p == r or p.startswith(r + os.sep)) and not _own(p, h):
                return ("%s is inside %s, which macOS protects as the user's own data; "
                        "walking it asks the user for permission" % (path, r))
        if p == h or h.startswith(p + os.sep):
            return "%s holds %s, so walking it walks the user's protected data" % (path, h)
    return None


# ---------------------------------------------------------------------------
# the static scan
# ---------------------------------------------------------------------------

EXEMPT = re.compile(r"foreign-app-data-exempt:\s*\S")

# R1: any spelling of the three folders. The separators cover `Library/Containers`,
# `"Library", "Containers"`, `.join("Library").join("Containers")` and
# `home / "Library" / "Group Containers"`.
R1 = re.compile(
    r"Library(?:[\s/\\\"',()]|\.join\(|\.push\()*(?:Group |Daemon )?Containers\b"
    r"|Group Containers|Daemon Containers")  # foreign-app-data-exempt: the pattern that refuses them

_HOME_TOKEN = r"(?:\"?\$\{?HOME\}?\"?|~)(?:/Library)?/?(?=[\"'\s;|&)`]|$)"
# R2: a recursive walk or measurement that STARTS at the home folder or its Library.
R2_SHELL = re.compile(
    r"(?:^|[\s;|&(`])(?:du|find|tree|rg|ag|tar|zip|"
    r"grep\s+-[A-Za-z]*[rR][A-Za-z]*|ls\s+-[A-Za-z]*R[A-Za-z]*)"
    r"(?:\s+[^\s;|&]+)*?\s+" + _HOME_TOKEN)
_PY_HOME = (r"(?:os\.path\.expanduser\(\s*[\"']~(?:/Library)?/?[\"']\s*\)"
            r"|os\.environ\[\s*[\"']HOME[\"']\s*\]|os\.environ\.get\(\s*[\"']HOME[\"'][^)]*\)"
            r"|str\(\s*Path\.home\(\)\s*\)|Path\.home\(\)(?:\s*/\s*[\"']Library[\"'])?)")
R2_PY = re.compile(
    r"(?:os\.walk|os\.fwalk|shutil\.copytree|shutil\.disk_usage_walk|glob\.glob)\(\s*" + _PY_HOME
    + r"\s*[,)]"
    r"|Path\.home\(\)(?:\s*/\s*[\"']Library[\"'])?\s*\)?\.(?:rglob|walk)\(")
R2_RUST = re.compile(
    r"WalkDir::new\(\s*&?(?:dirs::home_dir\(\)|home_dir\(\)|std::env::var(?:_os)?\(\s*\"HOME\"\s*\))")
# R3: a configured list of walk roots that holds the home folder or its Library bare.
R3_CONFIG = re.compile(
    r"^\s*(?:export\s+)?[A-Z][A-Z0-9_]*(?:CANDIDATES|ROOTS|PATHS|DIRS)\s*=\s*[\"']?[^\n]*?"
    r"(?:^|[\s\"'=])" + _HOME_TOKEN)

SOURCE_SUFFIXES = (".sh", ".bash", ".zsh", ".py", ".rs", ".js", ".mjs", ".cjs", ".ts",
                   ".swift", ".kt", ".config", ".plist", ".toml")
SKIP_DIRS = {".git", "node_modules", "target", "docs", "reference", "fixtures", "__pycache__",
             ".build", "build", "dist"}


def _is_test(path):
    b = os.path.basename(path)
    parts = path.split(os.sep)
    return (".test." in b or b.startswith("test_") or b.endswith("_test.py")
            or b.endswith("_tests.rs") or "tests" in parts or ".mutation." in b)


def _comment_only(line):
    s = line.lstrip()
    return s.startswith(("#", "//", "/*", "* ", "*/", "--")) or s == "*"


def scan_text(text, name="<text>"):
    """[(line_no, rule, line)] for every finding in `text`."""
    out = []
    for i, line in enumerate(text.splitlines(), 1):
        if _comment_only(line) or EXEMPT.search(line):
            continue
        for rule, rx in (("names-app-data", R1), ("walks-home", R2_SHELL),
                         ("walks-home", R2_PY), ("walks-home", R2_RUST),
                         ("walk-root-is-home", R3_CONFIG)):
            if rx.search(line):
                out.append((i, rule, line.strip()))
                break
    return out


def iter_sources(root, exclude_tests):
    if os.path.isfile(root):
        yield root
        return
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if d not in SKIP_DIRS)
        for f in sorted(filenames):
            p = os.path.join(dirpath, f)
            if not (f.endswith(SOURCE_SUFFIXES) or f == "orchestration.config"):
                continue
            if exclude_tests and _is_test(p):
                continue
            yield p


def scan(roots, exclude_tests=False):
    findings = []
    for root in roots:
        for p in iter_sources(root, exclude_tests):
            try:
                with open(p, encoding="utf-8", errors="replace") as fh:
                    text = fh.read()
            except OSError:
                continue
            for n, rule, line in scan_text(text, p):
                findings.append((p, n, rule, line))
    return findings


# ---------------------------------------------------------------------------
# self-test: every refusal has an acceptance beside it
# ---------------------------------------------------------------------------

def self_test():
    # foreign-app-data-exempt: this table IS the list of spellings the scan must refuse
    must_find = [
        'DISK_CONSUMER_CANDIDATES="$TMPDIR $HOME/Library/Containers"',  # foreign-app-data-exempt: self-test case
        'p = os.path.join(home, "Library", "Containers", ident)',  # foreign-app-data-exempt: self-test case
        'let p = home.join("Library").join("Group Containers");',  # foreign-app-data-exempt: self-test case
        'du -sk "$HOME"',
        'du -sh ~/Library',
        'find ~ -name "*.log"',
        'find "$HOME/Library" -type f',
        'grep -rn secret ~',
        'for r, d, f in os.walk(os.path.expanduser("~")):',  # foreign-app-data-exempt: self-test case
        'files = list(Path.home().rglob("*.json"))',  # foreign-app-data-exempt: self-test case
        'for e in WalkDir::new(dirs::home_dir().unwrap()) {',  # foreign-app-data-exempt: self-test case
        'SCAN_ROOTS="$HOME/ab $HOME"', # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        'du -sk "$HOME/ab" "$HOME"', # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        'tar -czf backup.tgz ~',
        'y = "Library/Containers"  # foreign-app-data-exempt:',  # foreign-app-data-exempt: a bare marker must exempt nothing
    ]
    must_pass = [
        'DISK_CONSUMER_CANDIDATES="$TMPDIR /private/tmp $HOME/.claude $HOME/ab $HOME/Library/Caches"', # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        'du -sk "$HOME/ab"', # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        'find ~/ab -name "*.log"', # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        'find "$HOME/Library/Developer/CoreSimulator" -maxdepth 1',
        'p = os.path.join(home, "Library", "Application Support", ident)',
        'for r, d, f in os.walk(os.path.expanduser("~/ab")):', # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
        'home.join("Library").join("Logs").join("RichOS")',
        '# the watchdog never measures other apps\' Containers',
        'x = "Library/Containers"  # foreign-app-data-exempt: names the folder in order to refuse it',
        'cd ~',
        'ls ~',
        'cd ~ && find . -name x',
        'mkdir -p ~/.richos && du -sk ~/.richos',
    ]
    bad = 0
    for s in must_find:
        got = scan_text(s)
        print(("  ok   refused  " if got else "  FAIL missed   ") + s)
        bad += 0 if got else 1
    for s in must_pass:
        got = scan_text(s)
        print(("  ok   allowed  " if not got else "  FAIL refused  ") + s)
        bad += 1 if got else 0
    home = "/Users/someone"
    cases = [
        (home + "/Library/Containers", True),  # foreign-app-data-exempt: self-test case
        (home + "/Library/Group Containers/group.x", True),  # foreign-app-data-exempt: self-test case
        (home + "/Library", True),
        (home, True),
        ("/", True),
        ("/Users", True),
        # 2026-10-29: the walk that asked the CEO about Apple Music, and its kin.
        (home + "/Library/Caches", True),
        (home + "/Library/Caches/com.apple.Music", True),
        (home + "/Library/Developer", True),
        (home + "/Music", True),
        (home + "/Movies", True),
        (home + "/Pictures/Photos Library.photoslibrary", True),
        (home + "/Desktop", True),
        (home + "/Documents/x", True),
        (home + "/Downloads", True),
        ("/private/var/folders/mx/abc123/T", True),
        ("/private/var/folders/mx/abc123", True),
        ("/private/var/folders/mx/abc123/C/com.apple.x", True),
        ("/System/Volumes/Data/Users/someone/Music", True),
        # RichOS's own, and what is not protected at all.
        (home + "/Library/Caches/RichOS", False),
        (home + "/Library/Application Support/com.richos.app/engine-state", False),
        (home + "/Library/Containers/com.richos.app", False),  # foreign-app-data-exempt: self-test case
        (home + "/Library/Logs/RichOS", False),
        (home + "/.claude", False),
        (home + "/ab", False),
        ("/private/var/folders/mx/abc123/T/richos-scratch", False),
        ("/Volumes/E1TB/vm", False), # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
    ]
    for path, want in cases:
        got = entering(path, home=home) is not None
        print(("  ok   " if got == want else "  FAIL ") + "entering(%s) = %s" % (path, got))
        bad += 0 if got == want else 1
    # A FIXTURE HOME DOES NOT UNPROTECT THE REAL ONE: the account's home stays refused.
    real = _homes()[-1]
    for path in (os.path.join(real, "Library", "Caches"), os.path.join(real, "Music"), real):
        got = entering(path, home=home) is not None
        print(("  ok   " if got else "  FAIL ") + "entering(%s, fixture home) = %s" % (path, got))
        bad += 0 if got else 1
    print("%d failure(s)" % bad)
    return 1 if bad else 0


# ---------------------------------------------------------------------------
# the Bash rule: commands an agent composes at runtime (2026-09-25)
# ---------------------------------------------------------------------------
# The static scan cannot see a command an agent types. Inside a RichOS session that command
# runs as a child of RichOS.app, so `find ~ -name x` there makes macOS ask the USER whether
# RichOS may access data from other apps. scripts/hooks/guard-foreign-app-data.sh runs this
# on every Bash call. It refuses exactly two shapes and nothing else:
#   (a) a path under another app's Containers or Group Containers folder, spelled with ~,
#       $HOME, ${HOME} or /Users/<name> (our own com.richos.* containers are allowed);
#   (b) find, du, ls -R, grep -r or rg whose root is the whole home folder, its Library,
#       /Users or / (the folder, `<it>/*`, or `.` while the working directory is one of them).
# Commands that only MENTION a path (echo, printf, git, gh) are not reads. A heredoc body is
# checked only when it feeds an interpreter. `# foreign-app-data-exempt: <reason>` on the
# command passes it and is logged.

BASH_HOME = r"(?:~|\$HOME|\$\{HOME\}|/Users/[^/\s'\"$`]+)"
BASH_CONTAINER = re.compile(
    r"(?:^|(?<=[\s'\"=(:,]))" + BASH_HOME
    + r"/Library/(Containers|Group Containers)(?=$|[/\s'\"`);|&])(?:/([^/\s'\"`);|&]*))?")  # foreign-app-data-exempt: the Bash rule's own pattern
OWN_CONTAINER_PREFIXES = ("com.richos.", "group.com.richos.")
MENTION_ONLY = {"echo", "printf", "git", "gh", ":", "true", "false"}
INTERPRETERS = re.compile(
    r"(?:^|[\s/;&|(])(?:python[0-9.]*|node|bash|sh|zsh|ruby|perl|osascript|swift)(?=\s|$)")
WRAPPERS = {"sudo", "command", "time", "nohup", "exec", "builtin", "$"}
HEREDOC = re.compile(r"<<(-?)\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\2")
REDIRECT = re.compile(r"^\d*(?:>>?|<|>&|<&)")
SEPARATORS = ";&|()\n"


def _strip_heredocs(text):
    """The command text with each heredoc body removed, unless it feeds an interpreter."""
    lines = text.split("\n")
    out, i = [], 0
    while i < len(lines):
        line = lines[i]
        out.append(line)
        i += 1
        m = HEREDOC.search(line)
        if not m:
            continue
        tag, dash = m.group(3), m.group(1) == "-"
        keep = bool(INTERPRETERS.search(line[:m.start()]))
        while i < len(lines):
            body = lines[i]
            i += 1
            if (body.lstrip("\t") if dash else body) == tag:
                break
            if keep:
                out.append(body)
    return "\n".join(out)


def _tokens(text):
    text = text.replace("\\\n", " ")  # a continued line is one command, not two
    lx = shlex.shlex(text, posix=True, punctuation_chars=SEPARATORS)
    lx.whitespace = " \t\r"
    lx.whitespace_split = True
    try:
        return list(lx)
    except ValueError:
        return text.split()


def _segments(tokens):
    seg = []
    for t in tokens:
        if t and all(c in SEPARATORS for c in t):
            if seg:
                yield seg
            seg = []
        else:
            seg.append(t)
    if seg:
        yield seg


def _command(seg):
    """(command basename, its arguments): assignments, wrappers and redirections removed."""
    words, skip = [], False
    for t in seg:
        if skip:
            skip = False
            continue
        if REDIRECT.match(t):
            skip = bool(re.fullmatch(r"\d*(?:>>?|<|>&|<&)", t))
            continue
        words.append(t)
    while words and (re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", words[0]) or words[0] in WRAPPERS):
        words = words[1:]
    if words and os.path.basename(words[0]) == "env":
        words = words[1:]
        while words and ("=" in words[0] or words[0].startswith("-")):
            words = words[1:]
    if words and os.path.basename(words[0]) in ("nice", "timeout"):
        words = words[1:]
        while words and (words[0].startswith("-") or re.fullmatch(r"[0-9.]+[smhd]?", words[0])):
            words = words[1:]
    if not words:
        return "", []
    return os.path.basename(words[0]), words[1:]


def _positionals(args, takes_value):
    """Operands of a command line, skipping options and the values they consume."""
    out, i, ended = [], 0, False
    while i < len(args):
        a = args[i]
        if ended or not a.startswith("-") or a == "-":
            out.append(a)
        elif a == "--":
            ended = True
        elif a.startswith("--"):
            if "=" not in a and a[2:] in takes_value:
                i += 1
        else:
            for j, c in enumerate(a[1:], 1):
                if c in takes_value:
                    if j == len(a) - 1:
                        i += 1
                    break
        i += 1
    return out


def _walk_roots(cmd, args):
    """The roots a recursive walker would start from, or None when it does not recurse."""
    if cmd == "find":
        roots, i = [], 0
        while i < len(args) and args[i] in ("-H", "-L", "-P", "-E", "-X", "-d", "-s", "-x", "-f"):
            if args[i] == "-f" and i + 1 < len(args):
                roots.append(args[i + 1])
                i += 1
            i += 1
        while i < len(args) and not args[i].startswith(("-", "(", "!")):
            roots.append(args[i])
            i += 1
        return roots or ["."]
    if cmd == "du":
        return _positionals(args, {"d", "B", "t", "I", "max-depth", "threshold", "exclude"}) or ["."]
    if cmd == "ls":
        rec = any(a == "--recursive" or (a.startswith("-") and not a.startswith("--") and "R" in a)
                  for a in args)
        return (_positionals(args, set()) or ["."]) if rec else None
    if cmd in ("grep", "egrep", "fgrep"):
        rec = any(a in ("--recursive", "--dereference-recursive", "--directories=recurse")
                  or (a.startswith("-") and not a.startswith("--") and ("r" in a or "R" in a))
                  for a in args)
        if not rec:
            return None
        pos = _positionals(args, set("efmABCdD") | {"regexp", "file", "max-count", "context",
                                                     "include", "exclude", "exclude-dir"})
        by_option = any(a.startswith(("-e", "-f", "--regexp", "--file")) for a in args)
        return pos if by_option else pos[1:]
    if cmd == "rg":
        pos = _positionals(args, set("efgtTmABCjM") | {"regexp", "file", "glob", "type",
                                                       "type-not", "max-count", "context",
                                                       "threads", "max-columns"})
        if "--files" in args or any(a.startswith(("-e", "-f", "--regexp", "--file")) for a in args):
            return pos or ["."]
        return pos[1:] or ["."]
    return None


# HOW DEEP A WALK FROM EACH HOLDER MAY GO BEFORE IT OPENS ANOTHER APP'S CONTAINER. The kernel
# refuses `file-read-data` on the container folder of each app (measured 2026-09-25:
# `du(62614) deny(1) file-read-data .../Containers/com.apple.VoiceMemos`). Listing the
# Containers folder itself names those folders without opening them. From the home folder a
# container folder is an entry at depth 3, and it is opened only to list depth 4. So
# `find ~ -maxdepth 3` never opens one, and `-maxdepth 4` does. The budget for each holder is
# the deepest `-maxdepth` that stays out of every container folder.
_DEPTH_BUDGET = (("/", 5), ("/Users", 4), ("HOME", 3), ("HOME/Library", 2),
                 ("HOME/Library/Containers", 1), ("HOME/Library/Group Containers", 1))  # foreign-app-data-exempt: the holders the Bash rule measures depth from


def _depth_budget(root, cwd, home):
    """The deepest safe -maxdepth for a walk rooted at `root`, or None if it holds no app data."""
    r = root.rstrip("/") or "/"
    glob = 0
    if r.endswith("/*") or r.endswith("/.*"):
        r, glob = (r.rsplit("/", 1)[0] or "/"), 1
    elif r in ("*", ".*"):
        r, glob = ".", 1
    if r in (".", "./"):
        if cwd is None:
            return None
        r = cwd.rstrip("/") or "/"
    h = home.rstrip("/")
    for alias in (r"~", r"\$HOME", r"\$\{HOME\}", r"/Users/[^/\s'\"$`]+", re.escape(h)):
        m = re.match(alias + r"(?=/|$)", r)
        if m:
            r = "HOME" + r[m.end():]
            break
    for holder, budget in _DEPTH_BUDGET:
        if r == holder:
            return budget - glob
    return None


def _max_depth(cmd, args):
    """The walk's -maxdepth (find) or --max-depth (rg), or None when it is unbounded."""
    found = []
    for i, a in enumerate(args):
        nxt = args[i + 1] if i + 1 < len(args) else ""
        if cmd == "find" and a == "-maxdepth" and nxt.isdigit():
            found.append(int(nxt))
        if cmd == "rg":
            if a in ("--max-depth", "-d", "--maxdepth") and nxt.isdigit():
                found.append(int(nxt))
            elif a.startswith("--max-depth=") and a.split("=", 1)[1].isdigit():
                found.append(int(a.split("=", 1)[1]))
    return min(found) if found else None


def bash_verdict(command, cwd=None, home=None):
    """None when the command may run; otherwise the sentence the refusal prints."""
    home = _home(home)
    if not command or EXEMPT.search(command):
        return None
    effective_cwd = cwd
    for seg in _segments(_tokens(_strip_heredocs(command))):
        cmd, args = _command(seg)
        if cmd == "cd":
            target = args[0] if args else home
            target = re.sub(r"^(?:~|\$HOME|\$\{HOME\})", home.rstrip("/"), target)
            if target.startswith("/"):
                effective_cwd = os.path.normpath(target)
            elif target != "-" and effective_cwd:
                # A relative cd moves the working directory the depth rule
                # measures from: `cd Library && find . -maxdepth 3` is a walk
                # of HOME/Library (budget 2), not of HOME (budget 3) (P5-38).
                effective_cwd = os.path.normpath(os.path.join(effective_cwd, target))
            continue
        if cmd not in MENTION_ONLY:
            # A path is a word of its own (or follows `=`/`:` as in --file=PATH). Inside a
            # longer word it is prose (an --tried "…" or --body "…" sentence) unless the
            # word is code handed to an interpreter (python3 -c "open('…')").
            code = bool(INTERPRETERS.search(" " + cmd))
            for t in seg:
                for m in BASH_CONTAINER.finditer(t):
                    if not code and m.start() > 0 and t[m.start() - 1] not in "=:":
                        continue
                    owner = m.group(2) or ""
                    # The folder itself (no app named) is a listing of names, not a read of
                    # any app's data; walking it is the depth rule's business below.
                    if not owner or owner.startswith(OWN_CONTAINER_PREFIXES):
                        continue
                    return ("`%s` reads %s, which is another app's data. macOS would ask the "
                            "user whether RichOS may access data from other apps."
                            % (cmd or "the command", m.group(0)))
        depth = _max_depth(cmd, args)
        # `-xdev` does NOT keep `find /` out of the home folder: measured 2026-09-25,
        # `stat -f %d / /Users /Users/alex` gives one device id for all three (firmlinks).
        for root in _walk_roots(cmd, args) or []:
            budget = _depth_budget(root, effective_cwd, home)
            if budget is None or (depth is not None and depth <= budget):
                continue
            where = root if root.rstrip("/") not in (".", "./*", "*") else \
                "%s (the working directory)" % effective_cwd
            return ("`%s` walks %s%s, which holds other apps' data. macOS would ask the user "
                    "whether RichOS may access data from other apps."
                    % (cmd, where, "" if depth is None else " to depth %d" % depth))
    return None


def bash_check(payload_text, log_path=None):
    """PreToolUse[Bash]: 2 with the refusal on stderr, else 0. Unreadable input passes."""
    try:
        payload = json.loads(payload_text)
    except ValueError:
        return 0
    if not isinstance(payload, dict) or payload.get("tool_name") not in (None, "Bash"):
        return 0
    command = (payload.get("tool_input") or {}).get("command") or ""
    if not isinstance(command, str):
        return 0
    marker = EXEMPT.search(command)
    if marker:
        if log_path:
            try:
                os.makedirs(os.path.dirname(log_path), exist_ok=True)
                with open(log_path, "a", encoding="utf-8") as fh:
                    line = command[marker.start():].split("\n")[0][:200]
                    fh.write("%s\t%s\t%s\n" % (time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                                               payload.get("session_id", ""), line))
            except OSError:
                pass
        return 0
    why = bash_verdict(command, cwd=payload.get("cwd"))
    if not why:
        return 0
    sys.stderr.write(
        "REFUSED (guard-foreign-app-data): %s\n"
        "  Use the specific directory you need instead (for example the repository directory or ~/.claude),\n"
        "  never the whole home folder or another app's container folder.\n"
        "  If it really is needed, add `# foreign-app-data-exempt: <reason>` to the command;\n"
        "  that is logged.\n" % why)
    return 2


def main(argv):
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0
    if argv[0] == "--self-test":
        return self_test()
    if argv[0] == "bash-check":
        base = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.join(_home(), ".claude")
        return bash_check(sys.stdin.read(), os.path.join(base, "state", "foreign-app-data-acks.log"))
    if argv[0] == "bash-verdict":
        why = bash_verdict(argv[1] if len(argv) > 1 else "", cwd=argv[2] if len(argv) > 2 else None)
        print(why or "allowed")
        return 1 if why else 0
    if argv[0] == "check":
        if len(argv) < 2:
            return 2
        home = argv[argv.index("--home") + 1] if "--home" in argv else None
        why = entering(argv[1], home=home)
        if why:
            print(why)
            return 1
        return 0
    if argv[0] == "scan":
        args = argv[1:]
        exclude = "--exclude-tests" in args
        roots = [a for a in args if a != "--exclude-tests"]
        if not roots:
            sys.stderr.write("foreign_app_data.py scan: no roots given\n")
            return 2
        missing = [r for r in roots if not os.path.exists(r)]
        if missing:
            sys.stderr.write("foreign_app_data.py scan: missing root(s): %s — refusing to "
                             "report a clean scan over nothing\n" % " ".join(missing))
            return 2
        findings = scan(roots, exclude_tests=exclude)
        for p, n, rule, line in findings:
            print("%s:%d: [%s] %s" % (p, n, rule, line[:200]))
        if findings:
            print("\n%d line(s) would make macOS ask \"would like to access data from other "
                  "apps\". Walk a narrower folder, or, if the line only names the folder in "
                  "order to refuse it, add `foreign-app-data-exempt: <reason>` to it."
                  % len(findings))
            return 1
        return 0
    sys.stderr.write("unknown command: %s\n" % argv[0])
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
