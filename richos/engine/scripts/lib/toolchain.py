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
            is not asked about again while it is still missing, nor one the user answered.

THE ANSWER AND THE INSTALL (CEO 2026-10-29: ask before installing, unless the user already gave a
general permission; never the same tool installed twice). Rich asks the user from the refusal's
report (his instruction file says how; nothing here writes the question), then runs:
  answer <tool> yes|no   yes: recorded, then `install`; no: recorded, never asked about again
  install <tool>         only with the user's yes for that tool or the general permission;
                         first checks the tool is really missing (present: nothing installed),
                         never starts the installer while an install is already running (one
                         lock, one record), opens Apple's installer (`xcode-select --install`,
                         where the user presses Install) and returns at once so the user can be
                         told; run again, each run waits up to CALL_WAIT seconds for the tool.
                         The agent waits in its own turn: a background command's end cannot
                         continue the work (the app grants actions only inside its own turns)
  permission [yes|no]    the general permission, `INSTALL_MISSING_TOOLS` in the user's own
                         settings file, the orchestration.config in their central folder
                         (RICHOS_USER_FOLDER, from the app; off unless it says yes). With it on,
                         the refusal tells the agent to install, not to ask.
An agent's own `xcode-select --install` is still refused: the install runs only through here.

NOTHING ELSE HERE INSTALLS ANYTHING OR CHANGES xcode-select. RICHOS_TOOLCHAIN_ROOT prefixes every
system path (tests build whole fake Macs under it); RICHOS_TOOLCHAIN_STATE names the ledger's
folder. It is a text check and leaks (a script FILE that calls make, a build tool that calls a
stub by absolute path): the harness half covers what runs under the test runners, the guard the
agent's own command line. Any error in the check is a pass. Suite: toolchain.test.sh.
"""
import json
import os
import re
import subprocess
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
    """-> True when this is a NEW ask (the user has not been asked about `name` yet). A tool the
    user already answered about (yes or no) is never asked about again."""
    path, data = _ledger(env)
    row = data["tools"].get(name)
    if row and row.get("state") in ("asked", "declined", "approved"):
        return False
    data["tools"][name] = {"state": "asked", "at": int(time.time())}
    _save(path, data)
    return True


def tool_state(env, name):
    return (_ledger(env)[1]["tools"].get(name) or {}).get("state")


# ---------------------------------------------------------------------------------------------
# The user's answer, the general permission and the install (CEO 2026-10-29: "If a tool is needed
# for actual work and the tool isn't there, then the user needs to be asked permission for
# installing etc. Unless the user has already given a general permission for installing what's
# needed." and "no user would ever want getting the same fucking tool installed a million times.")
# ---------------------------------------------------------------------------------------------
# The general permission lives in the user's own settings file, the `orchestration.config` of
# their central folder (user_settings.rs: "the user's values for the engine's checks"), which the
# app names to every lease as RICHOS_USER_FOLDER. Off unless that file says yes.
PERMISSION_KEY = "INSTALL_MISSING_TOOLS"
PERMISSION_COMMENT = ("# Install a tool that work needs and this Mac does not have without asking first "
                      "(yes), or ask each time (no). Rich sets it when you tell him.")
# How long an install may take before a second request may start the installer again: the
# Command Line Tools are a download of 1 to 2 GB.
INSTALL_WAIT = 3600
# How long one `install` call waits before it says "still installing, run me again": under Claude
# Code's default Bash timeout of 120 s, so the agent's call is never cut off or moved to the
# background. A background command's end cannot continue the work in a conversation turn: the
# app allows actions only inside a turn it granted (app-engine-hook.py), and the provider reports
# a late ending in a turn of its own (measured on the test VM 2026-10-31, walk-f50bafc2bb48).
CALL_WAIT = 100


def settings_file(env):
    folder = (env.get("RICHOS_USER_FOLDER") or "").strip()
    if folder:
        return os.path.join(folder, "orchestration.config")
    project = (env.get("CLAUDE_PROJECT_DIR") or "").strip()
    return os.path.join(project, "orchestration.config") if project else None


def _assigns(line, key):
    text = line.strip()
    if text.startswith("export "):
        text = text[len("export "):].lstrip()
    return text.startswith(key + "=")


def general_permission(env):
    """The user's stored general permission: the last `INSTALL_MISSING_TOOLS=` line, as a shell
    that sources the file sees it. Absent, unreadable or anything but yes is off."""
    path = settings_file(env)
    try:
        with open(path, encoding="utf-8") as fh:
            lines = [l for l in fh.read().splitlines() if _assigns(l, PERMISSION_KEY)]
    except (OSError, TypeError):
        return False
    if not lines:
        return False
    value = lines[-1].split("=", 1)[1].split(" #", 1)[0].strip().strip("'\"").lower()
    return value in ("yes", "true", "1", "on")


def set_general_permission(env, on):
    """Write the setting into the user's file, keeping every other line as it is. -> the path."""
    path = settings_file(env)
    if not path:
        raise OSError("no settings file: RICHOS_USER_FOLDER and CLAUDE_PROJECT_DIR are both unset")
    try:
        with open(path, encoding="utf-8") as fh:
            lines = fh.read().splitlines()
    except FileNotFoundError:
        lines = []
    line = '%s="%s"' % (PERMISSION_KEY, "yes" if on else "no")
    at = [i for i, l in enumerate(lines) if _assigns(l, PERMISSION_KEY)]
    if at:
        lines[at[0]] = line
        lines = [l for i, l in enumerate(lines) if i not in at[1:]]
    else:
        if lines and lines[-1].strip():
            lines.append("")
        lines += [PERMISSION_COMMENT, line]
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    tmp = "%s.%d.tmp" % (path, os.getpid())
    with open(tmp, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    os.replace(tmp, path)
    return path


def _set_state(env, name, state):
    path, data = _ledger(env)
    data["tools"][name] = {"state": state, "at": int(time.time())}
    _save(path, data)


def _commands(name):
    # Quoted: a fetched engine lives under ~/Library/Application Support (review
    # rv-20261031T104540Z-1d6c4328-0287), and the agent runs these through a shell.
    import shlex
    me = "%s %s" % (shlex.quote(sys.executable), shlex.quote(os.path.abspath(__file__)))
    tool = shlex.quote(name)
    return {"yes_command": "%s answer %s yes" % (me, tool), "no_command": "%s answer %s no" % (me, tool),
            "install_command": "%s install %s" % (me, tool)}


def _locked(env):
    """An exclusive lock on the ledger's folder, so two requests never both start the installer."""
    import fcntl
    os.makedirs(state_dir(env), exist_ok=True)
    fh = open(os.path.join(state_dir(env), "install.lock"), "a")
    fcntl.flock(fh, fcntl.LOCK_EX)
    return fh


def install_running(env, wait=INSTALL_WAIT):
    started = int((_ledger(env)[1].get("install") or {}).get("started") or 0)
    return bool(started) and time.time() - started < wait


def _present(env, name):
    directory, _source = developer_dir(env)
    return directory, tool_path(directory, name)


def _arrived(env, name, where, state, out):
    """`name` is on this Mac. With an install of ours on record, that install has finished; with
    none, it was already here and nothing was installed."""
    record_found(env, name, where)
    path, data = _ledger(env)
    if not data.pop("install", None):
        out.write("ALREADY-INSTALLED `%s` is on this Mac at %s. Nothing was installed.\n" % (name, where))
        return 0
    _save(path, data)
    told = " Tell the user once, in one line, that it was installed." if state != "approved" else ""
    out.write("INSTALLED `%s` at %s. Carry on with the work that needed it.%s\n" % (name, where, told))
    return 0


def install(env, name, wait=CALL_WAIT, poll=5.0, out=sys.stdout):
    """Install the developer tools that carry `name`, ONCE, and only with the user's permission
    (their yes for this tool, or the general permission). -> exit code.
      0  the tool is there (it already was: nothing installed; or the install finished)
      3  no permission: the user has not said yes (ask them first), or said no to this tool
      4  not there yet: this call opened Apple's installer and returns at once, so the user can
         be told to press Install; or it waited `wait` seconds. Run it again to keep waiting: an
         install already running is never started again (for INSTALL_WAIT seconds)
      5  it cannot be installed from here (only the full Xcode app carries it; not macOS; the
         installer refused)"""
    state = tool_state(env, name)
    directory, where = _present(env, name)
    if where:  # really missing? never install what is already here
        return _arrived(env, name, where, state, out)
    if state == "declined":
        out.write("NOT-INSTALLED: the user said no to installing `%s`. Nothing was installed; do not ask "
                  "again.\n" % name)
        return 3
    permitted = general_permission(env)
    if state != "approved" and not permitted:
        out.write("NOT-INSTALLED: the user has not given permission to install `%s`. Nothing was installed. "
                  "Ask the user first (what it is, why the work needs it, what installing involves: %s).\n"
                  % (name, INSTALL_INVOLVES))
        return 3
    if directory:
        out.write("CANNOT-INSTALL: the developer tools are already on this Mac (%s), and `%s` is not part of "
                  "them: it comes only with the full Xcode app, from the App Store (a download of more than "
                  "10 GB). Nothing was installed.\n" % (directory, name))
        return 5
    installer = _root() + "/usr/bin/xcode-select"
    if sys.platform != "darwin" and not _root():
        out.write("CANNOT-INSTALL: `%s` is installed with Apple's developer tools, which exist only on "
                  "macOS. Nothing was installed.\n" % name)
        return 5
    again = _commands(name)["install_command"]
    lock = _locked(env)
    try:
        path, data = _ledger(env)
        if not install_running(env):
            proc = subprocess.run([installer, "--install"], capture_output=True, text=True,
                                  stdin=subprocess.DEVNULL)
            said = (proc.stdout + proc.stderr).strip()
            if proc.returncode != 0 and "already installed" not in said:
                out.write("CANNOT-INSTALL: Apple's installer did not start (exit %d): %s. Nothing was "
                          "installed.\n" % (proc.returncode, said or "no message"))
                return 5
            data["install"] = {"started": int(time.time()), "tool": name}
            _save(path, data)
            out.write("INSTALLING `%s`: Apple's \"Install Command Line Developer Tools\" window is open on "
                      "this Mac. The user presses Install there and agrees to Apple's license; it then "
                      "takes a few minutes. Tell the user that now, then run `%s` until it says INSTALLED: "
                      "each run waits up to %d s and never opens the installer again.\n"
                      % (name, again, wait))
            return 4
        started = int(data["install"]["started"])
    finally:
        lock.close()
    deadline = time.time() + wait
    while True:
        directory, where = _present(env, name)
        if where:
            return _arrived(env, name, where, state, out)
        if time.time() >= deadline:
            out.write("STILL-INSTALLING `%s`: Apple's install, started %d s ago, has not finished. Run `%s` "
                      "again to keep waiting; it does not open the installer again. (If the user closed "
                      "Apple's window, a request after %d minutes from the start opens it again.)\n"
                      % (name, int(time.time() - started), again, INSTALL_WAIT // 60))
            return 4
        time.sleep(poll)


def answer(env, name, said, out=sys.stdout, **kw):
    """The user's answer to the ask about `name`: yes installs it once, no records it so the user
    is never asked about it again."""
    if said == "no":
        _set_state(env, name, "declined")
        out.write("RECORDED: the user said no to installing `%s`. It will not be installed, and the user "
                  "will not be asked about it again; work that needs it stays stopped.\n" % name)
        return 0
    directory, where = _present(env, name)
    if not where:
        _set_state(env, name, "approved")
    return install(env, name, out=out, **kw)


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
    # A heredoc fed to a shell is commands (review 2026-10-30: `bash <<'EOF'` running
    # /usr/bin/make passed). Written to a file or sent to the guest, it is text, and so is any
    # heredoc inside it: only heredocs whose head is at command level are read here.
    from host_sleep import heredocs  # noqa: E402  (one heredoc scan for the Bash rules)
    for head, body in heredocs(text):
        head_cmds, _ = tokenize(head)
        for words in head_cmds:
            argv, local = effective(words, env)
            if not argv:
                continue
            name = os.path.basename(argv[0])
            if name in ("ssh", "scp", "sftp") or re.search(r"(?:^|/)testvm/", argv[0]):
                break
            if name in SHELLS and not any(re.fullmatch(r"-[A-Za-z]*c[A-Za-z]*", a) for a in argv[1:]):
                _scan(body, local, found, depth + 1)
                break


INSTALL_INVOLVES = ("Apple's free \"Command Line Tools for Xcode\" (a download from Apple of roughly 1 to 2 GB "
                    "that takes a few minutes; macOS asks the user to agree to the license and may ask for an "
                    "administrator password), or a full Xcode from the App Store")

REFUSAL = """MISSING TOOL (guard-toolchain): this command needs `{tool}`, which on a Mac comes with Apple's command line developer tools, and this Mac has no developer tools RichOS can use. Nothing ran, and no macOS install dialog was opened.
  command : {command}
  tool    : {tool}
  needed  : to run the command above
  install : {involves}

What to do: {ask}
Do not run the command again, do not run `xcode-select --install` yourself, and do not look for a
way around this that runs the tool: on a Mac without developer tools every such way opens the
macOS install dialog on the user's screen.
RICHOS-MISSING-TOOL {payload}
"""
ASK_NEW = ("Stop this step and report the missing tool to whoever gave you the work. "
           "The lead asks the user in the conversation, in plain words, whether to install it; the "
           "work on this step waits for the answer. On yes: `{yes_command}` (it opens the installer "
           "once), then `{install_command}` until it says INSTALLED; on no: `{no_command}`.")
ASK_DONE = ("Stop this step and report the missing tool to whoever gave you the work. "
            "The user has ALREADY been asked about `{tool}` (do not ask again); wait for the answer, "
            "or carry on with work that does not need it.")
ASK_DECLINED = ("Stop this step and report the missing tool to whoever gave you the work. "
                "The user said NO to installing `{tool}`: do not ask again and do not install it; "
                "carry on with work that does not need it.")
ASK_PERMITTED = ("The user has already given permission to install it, so do not ask and do not stop: "
                 "run `{install_command}` until it says INSTALLED (it does nothing if `{tool}` is "
                 "already there and never opens the installer twice), report that it was installed "
                 "so the user is told once, then run this step again.")
ASK_INSTALLING = ("Stop this step and report the missing tool to whoever gave you the work. "
                  "Apple's developer tools, which carry `{tool}`, are being installed right now: do not "
                  "ask and do not start another install; run this step again once it is installed.")
MISCONFIGURED = """REFUSED (guard-toolchain): DEVELOPER_DIR is set to `{given}`, which is not a developer directory, so `{tool}` would ask macOS to install the developer tools although this Mac has them at `{found}`. Nothing ran.
  Fix: run the command with DEVELOPER_DIR unset, or with DEVELOPER_DIR={found}.
"""
INSTALL_REFUSAL = """REFUSED (guard-toolchain): `{command}` opens the macOS install dialog on the user's screen. Nothing ran.
An agent never starts this install itself. If work needs the developer tools, stop and report it; the
lead asks the user in the conversation, and on the user's yes the install runs through
scripts/lib/toolchain.py (`answer <tool> yes`), once.
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
        commands = _commands(tool)
        state = tool_state(env, tool)
        if state == "declined":
            new, ask = False, ASK_DECLINED
        elif install_running(env):
            new, ask = False, ASK_INSTALLING
        elif state == "approved" or general_permission(env):
            new, ask = False, ASK_PERMITTED
        else:
            new = record_missing(env, tool)
            ask = ASK_NEW if new else ASK_DONE
        fields = {"tool": tool, "ask": new, "command": shown, "needed": "to run the command",
                  "install": INSTALL_INVOLVES, "developer_dir": directory,
                  "declined": state == "declined", "permitted": ask is ASK_PERMITTED}
        fields.update(commands)
        payload = json.dumps(fields, sort_keys=True)
        return 2, REFUSAL.format(tool=tool, command=shown, involves=INSTALL_INVOLVES,
                                 ask=ask.format(tool=tool, **commands), payload=payload)
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
    """`env` with a usable DEVELOPER_DIR passed through, or stub stand-ins ahead of /usr/bin.
    Only on macOS: the stubs are Apple's, and elsewhere /usr/bin/git and make are the real tools,
    which a stand-in ahead of them would hide (review rv-20261030T130236Z-aaf88b85-fcb0)."""
    env = dict(env)
    if sys.platform != "darwin":
        return env
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
    if len(argv) == 3 and argv[0] == "answer" and argv[2] in ("yes", "no"):
        return answer(os.environ, argv[1], argv[2])
    if len(argv) == 2 and argv[0] == "install":
        return install(os.environ, argv[1])
    if argv and argv[0] == "permission" and len(argv) <= 2:
        if len(argv) == 2 and argv[1] in ("yes", "no"):
            try:
                path = set_general_permission(os.environ, argv[1] == "yes")
            except OSError as error:
                sys.stderr.write("the setting could not be saved: %s\n" % error)
                return 1
            print("SAVED in %s: %s" % (path, "missing tools are installed without asking first; the user is "
                                       "told once about each" if argv[1] == "yes" else
                                       "the user is asked before any missing tool is installed"))
            return 0
        if len(argv) == 1:
            print("%s in %s" % ("ON: missing tools are installed without asking first" if general_permission(os.environ)
                                else "OFF: the user is asked before a missing tool is installed",
                                settings_file(os.environ)))
            return 0
    sys.stderr.write("usage: toolchain.py check < payload.json | resolve [tool...] | "
                     "harness-env --standins <dir> | answer <tool> yes|no | install <tool> | "
                     "permission [yes|no]\n")
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
