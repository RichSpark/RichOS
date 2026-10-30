#!/usr/bin/env python3
"""toolchain.py: NOTHING AN AGENT OR A TEST RUNS REACHES AN APPLE DEVELOPER-TOOLS STUB WITHOUT A
USABLE TOOLCHAIN BEHIND IT.

WHY (2026-10-29). At 22:01:06 a process under a proof run executed an /usr/bin command-line
stub (make) that could not use any developer directory, and the stub asked macOS to install the
Command Line Tools: "The make command requires the command line developer tools" sat on the CEO's
screen until he saw it at 22:17. This Mac had three working toolchains. ceo-decisions §65:
nothing an agent runs changes the host's screen state. The CEO's rules (2026-10-29): a regular
user is usually not a developer and has no developer tools; nothing RichOS runs may ever pop that
dialog; when real work needs a tool that is genuinely absent the agent's work pauses and the user
is asked in the conversation, once; a Mac that has the tool always finds it.

THE STUBS. /usr/bin/make, git, cc, clang, python3, swift, xcrun, ... are one small launcher
(they share an inode). It looks for a developer directory (DEVELOPER_DIR, else the
`xcode-select` link, else the defaults) and runs the real tool from it; when it finds none it
asks launchd to start "Install Command Line Developer Tools", which opens the dialog. So the only
safe test of "would this command open the dialog" is the one done here, WITHOUT RUNNING THE
STUB: read the same places from the file system.

RESOLUTION, read-only, nothing executed (developer_dir):
  1. DEVELOPER_DIR, when it names a directory with usr/bin
  2. the target of /var/db/xcode_select_link (Xcode on another volume included)
  3. /Applications/Xcode.app/Contents/Developer
  4. /Library/Developer/CommandLineTools
and a tool is present when <dir>/usr/bin/<tool> or the default toolchain's usr/bin/<tool> exists.

THREE USES.
  check     PreToolUse[Bash] rule (guard-toolchain.sh), in the desktop app's leases and the
            operator's sessions: a command that would run a stub (by bare name found first in
            /usr/bin on PATH, or by its /usr/bin path) is let through when the tool resolves, and
            otherwise stopped BEFORE the stub runs. The refusal names the tool, why it was
            needed and what installing it involves, ends with a machine-readable
            `RICHOS-MISSING-TOOL {json}` line, and tells the agent to pause and report so the
            lead asks the user. `xcode-select --install` is refused too: it opens the dialog.
  harness   host_shield.py (every suite and build the test runners start): a usable developer
            directory is passed through as DEVELOPER_DIR (a sanitized `env -i` environment
            keeps finding it); with none, a stand-in for each stub is put on PATH just ahead of
            /usr/bin, so a call by name ends with exit 127 and a plain line in the log, never a
            dialog. Earlier PATH entries (Homebrew git, a bundled python) still win.
  asks      a small ledger (asks.json) so the same missing tool is asked about ONCE: a tool
            found by a real check is recorded and never asked about; a tool already asked about
            is not asked about again while it is still missing.

NOTHING HERE INSTALLS ANYTHING OR CHANGES xcode-select. RICHOS_TOOLCHAIN_ROOT prefixes every
system path (tests build whole fake Macs under it); RICHOS_TOOLCHAIN_STATE names the ledger's
folder. It is a text check and leaks (a script FILE that calls make, a build tool that calls a
stub by absolute path): the harness half covers what runs under the test runners, the guard the
agent's own command line. Any error in the check is a pass. Suite: toolchain.test.sh.
"""
import json
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

# The stub names: every command macOS ships in /usr/bin as the developer-tools launcher.
SHIMS = frozenset("""
make gnumake gcc g++ cc c++ c89 c99 cpp clang clang++ ld lldb gdb as ar nm ranlib strip lipo
libtool otool size strings dsymutil dwarfdump install_name_tool codesign_allocate cmpdylib
segedit unifdef unifdefall indent ctags mig rpcgen bison yacc flex lex m4 gm4 gperf
git git-receive-pack git-shell git-upload-archive git-upload-pack scalar svn
xcrun xcodebuild xcode-select-tool xctrace xed actool ibtool agvtool swift swiftc
python3 pip3 pydoc3 2to3 python3-config
""".split())
TOOLCHAIN = "Toolchains/XcodeDefault.xctoolchain/usr/bin"


def _root():
    return os.environ.get("RICHOS_TOOLCHAIN_ROOT", "").rstrip("/")


def shim_dir():
    return _root() + "/usr/bin"


def usable(path):
    return bool(path) and os.path.isdir(os.path.join(path, "usr", "bin"))


def developer_dir(env):
    """-> (directory or None, source). Reads the file system only; runs nothing."""
    given = (env.get("DEVELOPER_DIR") or "").strip()
    if given and usable(given):
        return given, "DEVELOPER_DIR"
    link = _root() + "/var/db/xcode_select_link"
    try:
        target = os.readlink(link)
        if not os.path.isabs(target):
            target = os.path.join(os.path.dirname(link), target)
        elif _root() and not target.startswith(_root() + "/"):
            target = _root() + target
        if usable(target):
            return target, "xcode-select"
    except OSError:
        pass
    for default in ("/Applications/Xcode.app/Contents/Developer", "/Library/Developer/CommandLineTools"):
        if usable(_root() + default):
            return _root() + default, "default"
    return None, None


# Tools that ARE /usr/bin's own resolver, with no copy inside a developer directory: xcrun finds
# its tool in whichever usable developer directory it is given (review 2026-10-30: every `xcrun`
# command was refused on a Mac with Xcode, whose Developer/usr/bin has no xcrun). It opens the
# install dialog only where no developer directory is usable, which developer_dir() decides.
RESOLVERS = frozenset({"xcrun"})


def tool_path(directory, name):
    if not directory:
        return None
    if name in RESOLVERS and usable(directory):
        return os.path.join(shim_dir(), name)
    for sub in ("usr/bin", TOOLCHAIN):
        candidate = os.path.join(directory, sub, name)
        if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return candidate
    return None


def stub_name(word, env):
    """The stub `word` would execute, or None (another tool earlier on PATH, or not a stub)."""
    stubs = shim_dir()
    if "/" in word:
        if os.path.dirname(os.path.normpath(word)) == stubs and os.path.basename(word) in SHIMS:
            return os.path.basename(word)
        return None
    if word not in SHIMS:
        return None
    for entry in (env.get("PATH") or "").split(os.pathsep):
        if not entry:
            continue
        candidate = os.path.join(entry, word)
        if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return word if os.path.normpath(entry) == stubs else None
    return None


# ---------------------------------------------------------------------------------------------
# The ask ledger: one ask per missing tool
# ---------------------------------------------------------------------------------------------
def state_dir(env):
    if env.get("RICHOS_TOOLCHAIN_STATE"):
        return env["RICHOS_TOOLCHAIN_STATE"]
    if env.get("RICHOS_APP_STATE"):
        return os.path.join(env["RICHOS_APP_STATE"], "toolchain")
    return os.path.join(os.path.expanduser("~"), "Library", "Application Support", "RichOS", "toolchain")


def _ledger(env):
    path = os.path.join(state_dir(env), "asks.json")
    try:
        with open(path, encoding="utf-8") as fh:
            data = json.load(fh)
        if isinstance(data, dict) and isinstance(data.get("tools"), dict):
            return path, data
    except (OSError, ValueError):
        pass
    return path, {"tools": {}}


def _save(path, data):
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        tmp = "%s.%d.tmp" % (path, os.getpid())
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(data, fh, indent=1, sort_keys=True)
        os.replace(tmp, path)
    except OSError:
        pass


def record_found(env, name, where):
    path, data = _ledger(env)
    row = data["tools"].get(name)
    if row and row.get("state") == "found" and row.get("path") == where:
        return
    data["tools"][name] = {"state": "found", "path": where, "at": int(time.time())}
    _save(path, data)


def record_missing(env, name):
    """-> True when this is a NEW ask (the user has not been asked about `name` yet)."""
    path, data = _ledger(env)
    row = data["tools"].get(name)
    if row and row.get("state") == "asked":
        return False
    data["tools"][name] = {"state": "asked", "at": int(time.time())}
    _save(path, data)
    return True


# ---------------------------------------------------------------------------------------------
# The command check
# ---------------------------------------------------------------------------------------------
def _words():
    from host_sleep import _strip_prefix  # noqa: E402  (one prefix stripper for the Bash rules)
    from unguarded_rm import tokenize  # noqa: E402
    return _strip_prefix, tokenize


SHELLS = {"sh", "bash", "zsh", "dash", "ksh", "fish"}


def _expand(value, env):
    """$NAME and ${NAME} in a command-local assignment, from the environment it is made in."""
    return re.sub(r"\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?", lambda m: env.get(m.group(1), ""), value)


def effective(words, env):
    """-> (argv from the command word, the environment that command runs with).

    The shared prefix stripper drops `NAME=value` assignments and `env` options; this keeps them
    (review 2026-10-29: `env -i PATH=/usr/bin:/bin git status` was resolved with the hook's own
    PATH, where the app's bundled git comes first, and passed, while the shell ran Apple's stub).
    `env -i` / `env -` clears the environment; `env -u NAME` unsets; `env -P path` (BSD) names the
    search path; an environment without PATH searches the system default /usr/bin:/bin."""
    from host_sleep import WRAPPERS, _OPTS_WITH_ARG, _POSITIONAL_VALUE, _ENV_ASSIGN, _unquote  # noqa: E402
    env = dict(env)
    search = None
    i = 0
    while i < len(words):
        w = _unquote(words[i])
        if _ENV_ASSIGN.match(w):
            k, _, v = w.partition("=")
            env[k] = _expand(v, env)
            i += 1
            continue
        base = os.path.basename(w)
        if base not in WRAPPERS:
            break
        i += 1
        opts = _OPTS_WITH_ARG.get(base, set())
        while i < len(words):
            t = _unquote(words[i])
            if base == "env":
                if _ENV_ASSIGN.match(t):
                    k, _, v = t.partition("=")
                    env[k] = _expand(v, env)
                    i += 1
                    continue
                if t in ("-i", "-", "--ignore-environment"):
                    env = {}
                    i += 1
                    continue
                if t == "-u" and i + 1 < len(words):
                    env.pop(_unquote(words[i + 1]), None)
                    i += 2
                    continue
                if t.startswith("--unset="):
                    env.pop(t.split("=", 1)[1], None)
                    i += 1
                    continue
                if t == "-P" and i + 1 < len(words):
                    search = _unquote(words[i + 1])
                    i += 2
                    continue
            if t == "--":
                i += 1
                break
            if t.startswith("-") and len(t) > 1:
                i += 2 if (t in opts and "=" not in t) else 1
                continue
            break
        if base in _POSITIONAL_VALUE and i < len(words):
            i += 1  # the duration
    argv = [_unquote(w) for w in words[i:]]
    lookup = dict(env)
    if search is not None:
        lookup["PATH"] = search
    elif "PATH" not in lookup:
        lookup["PATH"] = os.pathsep.join([_root() + "/usr/bin", _root() + "/bin"])
    return argv, lookup


def _scan(text, env, found, depth=0):
    if depth > 6:
        return
    _strip, tokenize = _words()
    cmds, nested = tokenize(text)
    env = dict(env)
    for words in cmds:
        argv, local = effective(words, env)
        if not argv or argv[0] == "export":
            # `PATH=/usr/bin:/bin; git status` and `export PATH=...`: the assignment lasts for the
            # rest of the text, so later commands resolve with it.
            for a in (words if not argv else argv[1:]):
                a = a.strip("'\"")
                if re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", a):
                    k, _, v = a.partition("=")
                    env[k] = _expand(v.strip("'\""), env)
            continue
        name = os.path.basename(argv[0])
        if name in SHELLS:
            for k, a in enumerate(argv[1:], 1):
                if re.fullmatch(r"-[A-Za-z]*c[A-Za-z]*", a) and k + 1 < len(argv):
                    _scan(argv[k + 1], local, found, depth + 1)
                    break
            continue
        if name in ("ssh", "scp", "sftp") or re.search(r"(?:^|/)testvm/", argv[0]):
            continue  # runs in the guest, whose tools are its own
        if name == "xcode-select" and any(a in ("--install", "-install") for a in argv[1:]):
            found.append(("install", "xcode-select", " ".join(argv)[:120], local))
            continue
        # Resolved in the command's OWN environment: its PATH picks the executable and its
        # DEVELOPER_DIR picks the toolchain (review 2026-10-29, `env -i PATH=/usr/bin:/bin git`).
        stub = stub_name(argv[0], local)
        if stub:
            found.append(("stub", stub, " ".join(argv)[:120], local))
    for body in nested:
        _scan(body, env, found, depth + 1)


INSTALL_INVOLVES = ("Apple's free \"Command Line Tools for Xcode\" (a download from Apple of roughly 1 to 2 GB "
                    "that takes a few minutes; macOS asks the user to agree to the license and may ask for an "
                    "administrator password), or a full Xcode from the App Store")

REFUSAL = """MISSING TOOL (guard-toolchain): this command needs `{tool}`, which on a Mac comes with Apple's command line developer tools, and this Mac has no developer tools RichOS can use. Nothing ran, and no macOS install dialog was opened.
  command : {command}
  tool    : {tool}
  needed  : to run the command above
  install : {involves}

What to do: stop this step and report the missing tool to whoever gave you the work. {ask}
Do not run the command again, do not run `xcode-select --install` yourself, and do not look for a
way around this that runs the tool: on a Mac without developer tools every such way opens the
macOS install dialog on the user's screen.
RICHOS-MISSING-TOOL {payload}
"""
ASK_NEW = ("The lead asks the user in the conversation, in plain words, whether to install it; the "
           "work on this step waits for the answer.")
ASK_DONE = ("The user has ALREADY been asked about `{tool}` (do not ask again); wait for the answer, "
            "or carry on with work that does not need it.")
MISCONFIGURED = """REFUSED (guard-toolchain): DEVELOPER_DIR is set to `{given}`, which is not a developer directory, so `{tool}` would ask macOS to install the developer tools although this Mac has them at `{found}`. Nothing ran.
  Fix: run the command with DEVELOPER_DIR unset, or with DEVELOPER_DIR={found}.
"""
INSTALL_REFUSAL = """REFUSED (guard-toolchain): `{command}` opens the macOS install dialog on the user's screen. Nothing ran.
An agent never starts an install. If work needs the developer tools, stop and report it; the lead asks
the user in the conversation, and the user installs them (or answers no).
"""


def verdict(command, env):
    """-> (exit code, message). 0 passes, 2 refuses."""
    found = []
    _scan(command, env, found)
    for kind, tool, shown, _local in found:
        if kind == "install":
            return 2, INSTALL_REFUSAL.format(command=shown)
    for kind, tool, shown, local in found:
        directory, source = developer_dir(local)
        where = tool_path(directory, tool)
        if where:
            given = (local.get("DEVELOPER_DIR") or "").strip()
            if given and not usable(given):
                return 2, MISCONFIGURED.format(given=given, tool=tool, found=directory)
            record_found(env, tool, where)
            continue
        new = record_missing(env, tool)
        payload = json.dumps({"tool": tool, "ask": new, "command": shown,
                              "needed": "to run the command", "install": INSTALL_INVOLVES,
                              "developer_dir": directory}, sort_keys=True)
        return 2, REFUSAL.format(tool=tool, command=shown, involves=INSTALL_INVOLVES,
                                 ask=(ASK_NEW if new else ASK_DONE.format(tool=tool)), payload=payload)
    return 0, ""


TRIGGER = re.compile(r"(?<![\w.-])(?:%s)(?![\w-])|xcode-select" % "|".join(re.escape(s) for s in sorted(SHIMS)))


def check(payload_text, env=None):
    """PreToolUse[Bash]: 2 with the message on stderr, else 0. Unreadable input passes."""
    env = os.environ if env is None else env
    if env.get("RICHOS_TOOLCHAIN_CALLER_PATH") is not None:
        # The hook puts its own python first on PATH; the command runs with the caller's PATH.
        env = dict(env, PATH=env["RICHOS_TOOLCHAIN_CALLER_PATH"])
        env.pop("RICHOS_TOOLCHAIN_CALLER_PATH", None)
    try:
        payload = json.loads(payload_text)
    except ValueError:
        return 0
    if not isinstance(payload, dict) or payload.get("tool_name") not in (None, "Bash"):
        return 0
    command = (payload.get("tool_input") or {}).get("command") or ""
    if not isinstance(command, str) or not TRIGGER.search(command):
        return 0
    try:
        rc, message = verdict(command, env)
    except Exception:  # any error in the check is a pass
        return 0
    if rc:
        sys.stderr.write(message)
    return rc


# ---------------------------------------------------------------------------------------------
# The harness half
# ---------------------------------------------------------------------------------------------
STANDIN = """#!/bin/sh
echo "toolchain: \\`{name}\\` skipped: this Mac has no Apple developer tools, and no install prompt was opened (scripts/lib/toolchain.py)" >&2
exit 127
"""


def harness_env(env, standin_dir):
    """`env` with a usable DEVELOPER_DIR passed through, or stub stand-ins ahead of /usr/bin."""
    env = dict(env)
    directory, _source = developer_dir(env)
    if directory:
        env["DEVELOPER_DIR"] = directory
        return env
    env.pop("DEVELOPER_DIR", None)
    try:
        os.makedirs(standin_dir, exist_ok=True)
        for name in sorted(SHIMS):
            target = os.path.join(standin_dir, name)
            with open(target, "w", encoding="utf-8") as fh:
                fh.write(STANDIN.format(name=name))
            os.chmod(target, 0o755)
    except OSError:
        return env
    parts = (env.get("PATH") or "").split(os.pathsep)
    stubs = shim_dir()
    out, placed = [], False
    for entry in parts:
        if not placed and entry and os.path.normpath(entry) == stubs:
            out.append(standin_dir)
            placed = True
        out.append(entry)
    if not placed:
        out.append(standin_dir)
    env["PATH"] = os.pathsep.join(out)
    return env


def main(argv):
    if argv and argv[0] == "check":
        return check(sys.stdin.read())
    if argv and argv[0] == "resolve":
        directory, source = developer_dir(os.environ)
        tools = argv[1:] or ["make"]
        print(json.dumps({"developer_dir": directory, "source": source,
                          "tools": {t: tool_path(directory, t) for t in tools}}, sort_keys=True))
        return 0 if directory else 1
    if len(argv) >= 3 and argv[0] == "harness-env" and argv[1] == "--standins":
        print(json.dumps(harness_env(os.environ, argv[2]), sort_keys=True))
        return 0
    sys.stderr.write("usage: toolchain.py check < payload.json | resolve [tool...] | "
                     "harness-env --standins <dir>\n")
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
