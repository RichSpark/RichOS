#!/usr/bin/env python3
"""operator_fences.py: the land lease, the Git fence and the merge-ownership record,
and the second review's land rule (no work lands without a passing review of
exactly its tip, CEO §113; its section below says where it applies).

Spec: richos-hq docs/plans/2026-09-24-operator-back-end-spec-r3.md (Sage, e1, e2,
e6, e7, e8), with Frank's final check G1-G12 applied on top of it
(docs/plans/2026-09-24-operator-back-end-spec-r3-frank-check.md). The as-built
record, including every deviation from r3, is richos-hq
docs/verification/2026-09-24-operator-fences/README.md.

WHY ONE FILE. The same code runs in three places:
  * from the engine, as `land-lease.sh` and `operator-fences.sh`;
  * from the engine, inside Claude Code hooks (the turn-end release, the early
    Bash check, the unfinished-land guard's ownership question);
  * inside Git, as the fence step of the `reference-transaction` launcher. For
    that one, `operator-fences.sh install` COPIES this file into
    <common>/hooks/operator-fences/, the same way install-ref-forensics.sh copies
    the recorder: a Git hook must not depend on where an engine checkout happens
    to be standing today. `operator-fences.sh status` compares the copy's digest
    with this file, so a stale copy is reported, never silently trusted.
So the file imports nothing outside the standard library at module level. The
one engine import (mega-lander/workspaces.py, for the lease's keying rule) is
made only by the admin and lease commands, which always run from the engine.

WHAT NEVER HAPPENS HERE. Nothing is ever signaled. Every process identity is a
pid PLUS its start time, read from the kernel (libproc on macOS, /proc on
Linux), so a recycled pid is never mistaken for the holder. Ancestry decides
ownership; a process name never does. The argv of the writing Git process is
read in exactly two places (G4's pack-refs, and `git worktree add`), and only to
PASS an update, never to select anything.

THE SWITCH. Every decision here is reachable only when the repository's
launcher says OPERATOR_FENCES_STATE="on" (G12). With it off, the lease commands
are no-ops that say so, the hooks return at once, and the launcher never calls
this file at all.
"""
import calendar
import ctypes
import errno
import fcntl
import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import time

MAIN = "refs/heads/main"
MAIN_SYMREF = "ref:" + MAIN
MARKER = "richos-operator-fence-launcher"
EXIT_HELD = 75
DEFAULT_WAIT = 90.0     # G7: the Claude Code Bash tool's own default is 120 s
MAX_WAIT = 540.0        # e1: one call, its start-up and its report fit in 600 s
DEFAULT_TTL = 1800.0    # e1: a non-Claude lease lives 30 minutes past each acquire
CHAIN_DIR = "reference-transaction.d"
RECORDER_SLOT = "20-ref-transaction-forensics.sh"
REPOSITORY_SLOT = "50-repository-hook"
_ZERO = re.compile(r"^0+$")
_CONF = re.compile(r'^OPERATOR_FENCES_([A-Z_]+)="([^"]*)"\s*$')


def is_zero(value):
    return bool(_ZERO.match(value or ""))


# ---------------------------------------------------------------------------
# processes, read from the kernel in process
# ---------------------------------------------------------------------------

class _BSDInfo(ctypes.Structure):
    # struct proc_bsdinfo, <sys/proc_info.h>; 136 bytes, asserted at load.
    _fields_ = [("flags", ctypes.c_uint32), ("status", ctypes.c_uint32), ("xstatus", ctypes.c_uint32),
                ("pid", ctypes.c_uint32), ("ppid", ctypes.c_uint32), ("uid", ctypes.c_uint32),
                ("gid", ctypes.c_uint32), ("ruid", ctypes.c_uint32), ("rgid", ctypes.c_uint32),
                ("svuid", ctypes.c_uint32), ("svgid", ctypes.c_uint32), ("rfu", ctypes.c_uint32),
                ("comm", ctypes.c_char * 16), ("name", ctypes.c_char * 32), ("nfiles", ctypes.c_uint32),
                ("pgid", ctypes.c_uint32), ("pjobc", ctypes.c_uint32), ("tdev", ctypes.c_uint32),
                ("tpgid", ctypes.c_uint32), ("nice", ctypes.c_int32), ("start_sec", ctypes.c_uint64),
                ("start_usec", ctypes.c_uint64)]


_LIB = False
_SZOMB = 5


def _libproc():
    global _LIB
    if _LIB is False:
        _LIB = None
        if sys.platform == "darwin":
            try:
                lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
                if ctypes.sizeof(_BSDInfo) == 136:
                    _LIB = lib
            except OSError:
                _LIB = None
    return _LIB


def _linux_btime():
    try:
        with open("/proc/stat") as fh:
            for line in fh:
                if line.startswith("btime "):
                    return int(line.split()[1])
    except (OSError, ValueError):
        pass
    return 0


def proc(pid, precise=False):
    """{pid, ppid, pgid, start, zombie} for a live pid, else None. `start` is
    whole seconds since the epoch, which is the resolution the platform's own
    session record (`procStart`) carries. Verification's `precise=True` returns
    the kernel generation string instead, without changing session/fence callers.
    """
    try:
        pid = int(pid)
    except (TypeError, ValueError):
        return None
    if pid <= 0:
        return None
    lib = _libproc()
    if lib is not None:
        info = _BSDInfo()
        n = lib.proc_pidinfo(pid, 3, ctypes.c_uint64(0), ctypes.byref(info), ctypes.sizeof(info))
        if n != ctypes.sizeof(info):
            return None
        start = ("darwin:%d:%06d" % (info.start_sec, info.start_usec)) if precise else int(info.start_sec)
        return {"pid": pid, "ppid": int(info.ppid), "pgid": int(info.pgid),
                "start": start, "zombie": int(info.status) == _SZOMB}
    try:
        with open("/proc/%d/stat" % pid) as fh:
            raw = fh.read()
        rest = raw[raw.rindex(")") + 2:].split()
        ticks = os.sysconf("SC_CLK_TCK")
        if precise:
            with open("/proc/sys/kernel/random/boot_id") as boot:
                start = "linux:%s:%s" % (boot.read().strip(), rest[19])
        else:
            start = int(_linux_btime() + int(rest[19]) // ticks)
        return {"pid": pid, "ppid": int(rest[1]), "pgid": int(rest[2]),
                "start": start, "zombie": rest[0] == "Z"}
    except (OSError, ValueError, IndexError):
        return None


def proc_path(pid):
    lib = _libproc()
    if lib is not None:
        buf = ctypes.create_string_buffer(4096)
        if lib.proc_pidpath(int(pid), buf, 4096) > 0:
            return os.path.realpath(buf.value.decode("utf-8", "replace"))
        return ""
    try:
        return os.path.realpath(os.readlink("/proc/%d/exe" % int(pid)))
    except OSError:
        return ""


def proc_argv(pid):
    """The argv of a process, split. Read with `ps` (the kernel's KERN_PROCARGS2
    layout is not worth a second parser for the two rare questions that ask)."""
    try:
        r = subprocess.run(["ps", "-o", "command=", "-p", str(int(pid))], capture_output=True,
                           text=True, timeout=10, env=dict(os.environ, LC_ALL="C"))
    except (OSError, subprocess.TimeoutExpired, ValueError):
        return []
    return r.stdout.strip().split()


def alive(pid, start):
    p = proc(pid)
    return bool(p) and not p["zombie"] and int(p["start"]) == int(start)


def ancestors(pid=None):
    """[(pid, start)] from `pid`'s parent (default: this process's parent) up
    to, not including, pid 1."""
    out, seen = [], set()
    cur = proc(pid if pid is not None else os.getpid())
    while cur and cur["ppid"] > 1 and cur["ppid"] not in seen:
        seen.add(cur["ppid"])
        cur = proc(cur["ppid"])
        if not cur:
            break
        out.append((cur["pid"], cur["start"]))
    return out


def is_ancestor(pid, start, chain=None):
    chain = ancestors() if chain is None else chain
    return any(p == int(pid) and s == int(start) for p, s in chain)


def lstart_epoch(text):
    """`procStart` ("Thu Sep 24 17:49:33 2026", UTC, as ps prints it under
    TZ=UTC0) to epoch seconds, or None."""
    try:
        return calendar.timegm(time.strptime(" ".join(str(text).split()), "%a %b %d %H:%M:%S %Y"))
    except (ValueError, OverflowError):
        return None


# ---------------------------------------------------------------------------
# small utilities
# ---------------------------------------------------------------------------

def now():
    return time.time()


def iso(t=None):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now() if t is None else t))


def age_text(seconds):
    seconds = max(0, int(seconds))
    if seconds < 90:
        return "%d s" % seconds
    minutes = seconds // 60
    if minutes < 90:
        return "%d min" % minutes
    return "%d h %d min" % (minutes // 60, minutes % 60)


def read_json(path):
    try:
        with open(path, encoding="utf-8") as fh:
            value = json.loads(fh.read(65536))
        return value if isinstance(value, dict) else None
    except (OSError, ValueError):
        return None


def write_json_atomic(path, value):
    os.makedirs(os.path.dirname(path), mode=0o700, exist_ok=True)
    tmp = "%s.%d.tmp" % (path, os.getpid())
    with open(tmp, "w", encoding="utf-8") as fh:
        fh.write(json.dumps(value, sort_keys=True) + "\n")
        fh.flush()
        os.fsync(fh.fileno())
    os.replace(tmp, path)


def registry_entities(reg):
    """{main checkout: the entity its fence was installed from, or "" when the
    entry does not record one} for every repository in the fence registry
    `reg` (operator_fences_admin.read_registry(); the land command reads it,
    mega-lander/workspaces.py _registry_entity_map).

    An entry the installer writes now carries its own "entity". AN ENTRY FROM
    BEFORE THAT carries only common, chain and installed, and its entity is
    UNKNOWN (""): it is never inferred from the top-level entity or the
    `installed` times (review rv-20261009T040812Z-9efed042-d03e, finding 1).
    The earlier installer overwrote the top-level entity at every install and
    left it unchanged at uninstall, and its times have one-second resolution,
    so neither establishes which entity installed an entry; inferring credited
    the wrong one. The land command refuses a repository whose entry is
    unknown, naming the `operator-fences.sh install` that records it."""
    repos = reg.get("repositories") if isinstance(reg, dict) else None
    if not isinstance(repos, dict):
        return {}
    out = {}
    for main, entry in repos.items():
        v = entry.get("entity") if isinstance(entry, dict) else None
        out[main] = v if isinstance(v, str) and v.strip() else ""
    return out


def registry_reviewed(reg):
    """{main checkout} of every repository whose fence registry entry records,
    from its declaration at `operator-fences.sh install`, that it requires a
    second review ("reviewed": true). The land command requires a review for
    such a repository whatever its declarations say now, until the next
    install records otherwise (CEO §113). An entry without the field (written
    before install recorded it) records nothing; the declarations decide."""
    repos = reg.get("repositories") if isinstance(reg, dict) else None
    if not isinstance(repos, dict):
        return set()
    return {main for main, entry in repos.items() if isinstance(entry, dict) and entry.get("reviewed") is True}


def append_jsonl(path, value):
    try:
        os.makedirs(os.path.dirname(path), mode=0o700, exist_ok=True)
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(value, sort_keys=True) + "\n")
    except OSError:
        pass


def git(cwd, *args, **kw):
    env = dict(os.environ, GIT_OPTIONAL_LOCKS="0", LC_ALL="C")
    for name in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR"):
        if kw.get("keep_git_env"):
            break
        env.pop(name, None)
    try:
        r = subprocess.run(["git", "-C", cwd] + list(args), capture_output=True, text=not kw.get("raw"),
                           timeout=kw.get("timeout", 60), env=env)
    except (OSError, subprocess.TimeoutExpired) as error:
        return 127, (b"" if kw.get("raw") else ""), str(error)
    return r.returncode, r.stdout, r.stderr


def file_digest(path):
    try:
        with open(path, "rb") as fh:
            return hashlib.sha256(fh.read()).hexdigest()
    except OSError:
        return ""


# ---------------------------------------------------------------------------
# the launcher: the single runtime source of the switch and the lease home (G10, G12)
# ---------------------------------------------------------------------------

def read_launcher(path):
    """The OPERATOR_FENCES_* assignments of a launcher, or None when the file is
    absent or not ours."""
    try:
        # errors="replace": a hook that is not UTF-8 is not ours (Frank's #13).
        with open(path, encoding="utf-8", errors="replace") as fh:
            text = fh.read(65536)
    except OSError:
        return None
    if MARKER not in text:
        return None
    conf = {"LAUNCHER": path}
    for line in text.splitlines():
        m = _CONF.match(line)
        if m:
            conf[m.group(1)] = m.group(2)
    return conf


def repo_paths(repo):
    """(main checkout, common dir, this checkout's git dir) for a path inside a
    repository, all absolute and realpath'd, or None."""
    rc, out, _ = git(repo, "rev-parse", "--path-format=absolute", "--show-toplevel",
                     "--git-common-dir", "--absolute-git-dir")
    lines = out.splitlines()
    if rc != 0 or len(lines) < 3:
        rc, out, _ = git(repo, "rev-parse", "--path-format=absolute", "--git-common-dir", "--absolute-git-dir")
        lines = out.splitlines()
        if rc != 0 or len(lines) < 2:
            return None
        lines = [""] + lines
    common = os.path.realpath(lines[1])
    gitdir = os.path.realpath(lines[2])
    main = os.path.dirname(common) if os.path.basename(common) == ".git" else ""
    return {"top": os.path.realpath(lines[0]) if lines[0] else "", "common": common,
            "gitdir": gitdir, "main": main}


def launcher_for(repo):
    paths = repo_paths(repo)
    if not paths:
        return None, None
    return read_launcher(os.path.join(paths["common"], "hooks", "reference-transaction")), paths


def fenced(conf):
    return bool(conf) and conf.get("STATE") == "on"


class Files(object):
    """Every per-repository file the fence and the lease share, named from the
    launcher's baked home and key and never from the caller's environment."""

    def __init__(self, conf):
        self.home = conf["HOME"]
        self.key = conf["KEY"]
        self.lease = os.path.join(self.home, self.key + ".lease")
        self.lock = os.path.join(self.home, self.key + ".lock")
        self.in_progress = os.path.join(self.home, "in-progress", self.key + ".json")
        self.restore_intent = os.path.join(self.home, "restore-intents", self.key + ".json")
        self.refusals = os.path.join(self.home, "fence-refusals.jsonl")
        self.preserved = os.path.join(self.home, "preserved")
        # The second review's ledger, for a repository in SECOND_REVIEW_REPOS
        # only ("" otherwise, and in a launcher installed before it existed).
        self.reviews = conf.get("REVIEWS") or ""
        self.engine = conf.get("ENGINE") or ""
        # The repository name a refusal record carries (cmd_fence writes the same value).
        self.repo = conf.get("REPO") or conf.get("COMMON") or ""


def declared_holders(conf):
    """{kind: executable} from the launcher's OPERATOR_FENCES_HOLDERS line,
    which the installer copies from the entity's LAND_LEASE_HOLDERS."""
    out = {}
    for item in (conf.get("HOLDERS") or "").split():
        kind, _, exe = item.partition("=")
        if kind and exe:
            out[kind] = os.path.realpath(exe)
    return out


# ---------------------------------------------------------------------------
# who is calling: a Claude session or a declared holder, found by ancestry (e1, F1)
# ---------------------------------------------------------------------------

def sessions_dir():
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude")
    return os.path.join(base, "sessions")


def claude_session(chain=None):
    """The nearest ancestor that the platform's own session record names, with
    its recorded start time matching the kernel's. Returns a holder dict or
    None. `CLAUDE_PID`, when set, must agree."""
    chain = ancestors() if chain is None else chain
    sd = sessions_dir()
    for pid, start in chain:
        rec = read_json(os.path.join(sd, "%d.json" % pid))
        if not rec or str(rec.get("pid")) != str(pid):
            continue
        recorded = lstart_epoch(rec.get("procStart"))
        if recorded is None or abs(recorded - start) > 1:
            continue
        claimed = (os.environ.get("CLAUDE_PID") or "").strip()
        if claimed and claimed != str(pid):
            return {"error": "this shell says it belongs to Claude process %s, but the nearest session "
                             "record in its own ancestry is process %d" % (claimed, pid)}
        return {"kind": "claude", "pid": pid, "start": start, "session_id": str(rec.get("sessionId") or ""),
                "entrypoint": str(rec.get("entrypoint") or ""), "cwd": str(rec.get("cwd") or "")}
    return None


# ---------------------------------------------------------------------------
# a declared holder that an app update moved inside its own bundle
# ---------------------------------------------------------------------------
# On 2026-09-30 21:58 the ChatGPT app updated itself and moved Codex from
# ChatGPT.app/Contents/Resources/codex to
# ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex.
# The declared path stopped existing, no ancestor matched it, and every Codex
# land in a fenced checkout was refused until the declaration was edited and
# the launchers reinstalled by hand. A vendor app moves its own files on its
# own schedule, so one file path is not an identity that survives an update.
#
# THE IDENTITY USED WHEN THE DECLARED PATH MATCHES NO ANCESTOR, all four parts
# required, the last two read from the kernel and the bundle's signature:
#   1. the ancestor's executable lies inside the declared path's app bundle
#      (the outermost `<name>.app` directory of the declared path);
#   2. it has the declared executable's file name (`codex`), so the app's other
#      programs (ChatGPT itself, its helpers and renderers) never pass;
#   3. the kernel says the running process is validly code-signed (CS_VALID) and
#      names its signing team;
#   4. that team is the team that signed the bundle itself.
# An unsigned, ad hoc or platform-signed program has no team, so a copy of bash
# renamed `codex` and dropped inside the bundle still does not pass. The exact
# declared path stays the first test, and the only one off macOS.

_CS_VALID = 0x1
_CS_OPS_STATUS = 0
_CS_OPS_TEAMID = 14
_LIBSYSTEM = False
_BUNDLE_TEAMS = {}


def _libsystem():
    global _LIBSYSTEM
    if _LIBSYSTEM is False:
        _LIBSYSTEM = None
        if sys.platform == "darwin":
            try:
                lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib", use_errno=True)
                lib.csops.argtypes = [ctypes.c_int, ctypes.c_uint, ctypes.c_void_p, ctypes.c_size_t]
                lib.csops.restype = ctypes.c_int
                _LIBSYSTEM = lib
            except (OSError, AttributeError):
                _LIBSYSTEM = None
    return _LIBSYSTEM


def signing_team(pid):
    """The code-signing team of a RUNNING process as the kernel holds it, or ""
    when the process is not validly signed by a team (unsigned, ad hoc, a
    platform binary, gone, or not macOS)."""
    lib = _libsystem()
    if lib is None:
        return ""
    try:
        pid = int(pid)
    except (TypeError, ValueError):
        return ""
    status = ctypes.c_uint32(0)
    if lib.csops(pid, _CS_OPS_STATUS, ctypes.byref(status), 4) != 0 or not status.value & _CS_VALID:
        return ""
    buf = ctypes.create_string_buffer(256)
    if lib.csops(pid, _CS_OPS_TEAMID, buf, 256) != 0:
        return ""
    # A blob: 4-byte type, 4-byte big-endian length (header included), then the
    # NUL-terminated team identifier.
    raw = buf.raw
    length = int.from_bytes(raw[4:8], "big")
    team = raw[8:max(8, min(length, len(raw)))].split(b"\0")[0]
    return team.decode("ascii", "replace").strip()


def file_signing_team(path):
    """The signing team recorded in an executable or bundle on disk, or "".
    Displays the signature (`codesign -d`); it does not re-hash the bundle,
    which on a 4,000-file app takes seconds."""
    if sys.platform != "darwin" or not path or not os.path.exists(path):
        return ""
    try:
        r = subprocess.run(["/usr/bin/codesign", "-d", "--verbose=2", path], capture_output=True,
                           text=True, timeout=20, env=dict(os.environ, LC_ALL="C"))
    except (OSError, subprocess.TimeoutExpired):
        return ""
    for line in (r.stderr + r.stdout).splitlines():
        if line.startswith("TeamIdentifier="):
            team = line.split("=", 1)[1].strip()
            return "" if team in ("", "not set") else team
    return ""


def bundle_team(bundle):
    if bundle not in _BUNDLE_TEAMS:
        _BUNDLE_TEAMS[bundle] = file_signing_team(bundle)
    return _BUNDLE_TEAMS[bundle]


def app_bundle(path):
    """The outermost `<name>.app` directory that contains `path`, or ""."""
    parts = (path or "").split(os.sep)
    for i, part in enumerate(parts[:-1]):
        if part.endswith(".app") and len(part) > len(".app"):
            return os.sep.join(parts[:i + 1])
    return ""


def moved_holder_path(declared, pid):
    """The executable of `pid` when it stands for `declared` after an update
    moved it inside the same app bundle (the four parts above), else ""."""
    bundle = app_bundle(declared)
    if not bundle:
        return ""
    path = proc_path(pid)
    if not path.startswith(bundle + os.sep) or os.path.basename(path) != os.path.basename(declared):
        return ""
    team = signing_team(pid)
    if not team or team != bundle_team(bundle):
        return ""
    return path


def bundle_candidates(declared, limit=20000):
    """Executables on disk that would pass as `declared` once running: same file
    name, inside the same bundle, signed by the bundle's team. Read by
    `operator-fences.sh status`, so a moved holder is named before anyone's
    land needs it."""
    bundle = app_bundle(declared)
    team = bundle_team(bundle) if bundle else ""
    if not bundle or not team:
        return []
    name, found, seen = os.path.basename(declared), [], 0
    for dirpath, _dirs, files in os.walk(bundle):
        seen += len(files)
        if seen > limit:
            break
        if name in files:
            full = os.path.join(dirpath, name)
            if os.path.isfile(full) and os.access(full, os.X_OK) and file_signing_team(full) == team:
                found.append(os.path.realpath(full))
    return sorted(set(found))


def declared_holder(kind, conf, chain=None, holder_pid=None):
    exe = declared_holders(conf).get(kind)
    if not exe:
        return {"error": "no holder of kind %r is declared for this repository (LAND_LEASE_HOLDERS)" % kind}
    chain = ancestors() if chain is None else chain
    if holder_pid is not None:
        found = [(p, s) for p, s in chain if p == int(holder_pid)]
        if not found:
            return {"error": "--holder-pid %s is not an ancestor of this command, so it cannot be its holder"
                             % holder_pid}
        chain = found
    for pid, start in chain:
        if proc_path(pid) == exe:
            return {"kind": kind, "pid": pid, "start": start, "executable": exe}
    # The exact path matched nobody: an update may have moved the holder inside
    # its bundle. Asked only now, so the common case never runs codesign.
    for pid, start in chain:
        moved = moved_holder_path(exe, pid)
        if moved:
            return {"kind": kind, "pid": pid, "start": start, "executable": moved, "declared": exe}
    gone = "" if os.path.exists(exe) else (
        "; that file no longer exists (an app update may have moved it), and no ancestor is a program of the "
        "same name inside %s signed by the same team. `operator-fences.sh status` names what is there"
        % (app_bundle(exe) or "its app bundle"))
    return {"error": "no ancestor of this command is the declared %s executable (%s)%s" % (kind, exe, gone)}


def caller_identity(conf, chain=None):
    """Who is behind this Git call, for a record: a Claude session, else any
    declared holder, else None. Never used to authorize anything."""
    chain = ancestors() if chain is None else chain
    who = claude_session(chain)
    if who and "error" not in who:
        return who
    for kind in sorted(declared_holders(conf)):
        who = declared_holder(kind, conf, chain)
        if who and "error" not in who:
            return who
    return None


def holder_label(holder):
    if not holder:
        return "an unknown holder"
    kind = holder.get("kind")
    if kind == "claude":
        title = _app_conversation_title(holder)
        if title:
            return "the conversation %s" % title
        if holder.get("entrypoint") == "cli":
            return "your terminal"
        return "a Claude session (process %s)" % holder.get("pid")
    return "%s (process %s)" % ((kind or "a holder").capitalize(), holder.get("pid"))


def _app_conversation_title(holder):
    """The app's claim record lists each lead with its conversation's title
    (spec r3 (e) the claim, item 3). Read only; absent until the app side lands."""
    rec = read_json(os.path.join(os.path.dirname(sessions_dir()), "state", "operator-lead.json"))
    for lead in (rec or {}).get("leads") or []:
        try:
            if int(lead.get("pid")) == int(holder.get("pid")) and lead.get("title"):
                return '"%s"' % str(lead["title"])[:120]
        except (TypeError, ValueError):
            continue
    return ""


# ---------------------------------------------------------------------------
# the lease record
# ---------------------------------------------------------------------------

def read_lease(files):
    return read_json(files.lease)


def same_holder(a, b):
    return bool(a and b) and int(a.get("pid", -1)) == int(b.get("pid", -2)) \
        and int(a.get("start", -1)) == int(b.get("start", -2))


def lease_state(lease):
    """'none' | 'live' | 'expired' (a live non-Claude holder past its time) |
    'dead' (the holder's pid no longer runs with its recorded start)."""
    if not lease or not isinstance(lease.get("holder"), dict):
        return "none"
    h = lease["holder"]
    if not alive(h.get("pid"), h.get("start", -1)):
        return "dead"
    if h.get("kind") != "claude" and lease.get("expires") and now() > float(lease["expires"]):
        return "expired"
    return "live"


def authorized(files, chain=None):
    """G1/e2: the caller holds the lease when a lease exists whose holder is
    alive and is an ancestor of this process, pid and start time both.

    A non-Claude lease past its time STILL authorizes its own holder: e1 says it
    "stays held" until it is taken at rest or taken over (G5). Only once another
    holder has it does the old holder's next move meet a refusal."""
    lease = read_lease(files)
    if lease_state(lease) not in ("live", "expired"):
        return False, lease
    h = lease["holder"]
    return is_ancestor(h["pid"], h["start"], chain), lease


# ---------------------------------------------------------------------------
# the land commands a lease is held for (2026-10-11)
# ---------------------------------------------------------------------------
# A LEASE IS NOT RELEASED UNDER A LAND THAT IS STILL RUNNING. Twice on
# 2026-10-11 the lead's background script took the lease (ACQUIRED at 17:18,
# RENEWED at 14:51) and ran `workspaces.sh merge` in the same script, and the
# lead's own turn ended a second later. Its turn-end release (cmd_turn_end)
# looked at the repository, found it at rest (14:51:29: the merge had not
# started; 17:18:11: the merge had written ORIG_HEAD at 17:18:10 but not yet
# its index, AUTO_MERGE at 17:18:12) and deleted the lease. The fence then
# refused the merge's ORIG_HEAD at 14:51:30, and the move of main at 17:23:27
# after a five-minute gate, with "The land lease for this repository is free."
# (ref-transactions.jsonl, fence-refusals.jsonl and the lead's transcript.)
# "At rest" is a snapshot of the repository; it cannot see a land between two
# of its steps. So the lease names the command each land runs in, the holder's
# own child on that command's ancestry (a Claude Bash call's `zsh -c`, alive
# until the call ends, foreground or background), and the turn-end release
# keeps the lease while any of them still runs.

def land_process(holder, chain=None):
    """{pid, start} of the command this land runs in: the holder's own child on
    this process's ancestry (this process itself when the holder is its parent),
    or None when the holder is not an ancestor."""
    me = proc(os.getpid())
    full = ([(me["pid"], me["start"])] if me else []) + list(ancestors() if chain is None else chain)
    try:
        hp, hs = int(holder["pid"]), int(holder["start"])
    except (KeyError, TypeError, ValueError):
        return None
    for i, (p, s) in enumerate(full):
        if p == hp and s == hs:
            return {"pid": full[i - 1][0], "start": full[i - 1][1]} if i > 0 else None
    return None


def running_lands(lease):
    """The land commands recorded on `lease` that still run: [{pid, start}]."""
    out = []
    for e in (lease or {}).get("land_processes") or []:
        try:
            entry = {"pid": int(e["pid"]), "start": int(e["start"])}
        except (KeyError, TypeError, ValueError):
            continue
        if entry not in out and alive(entry["pid"], entry["start"]):
            out.append(entry)
    return out


def _land_processes(previous, holder, chain=None):
    """The running land commands of the same holder carried over, plus this one."""
    out = running_lands(previous) if previous and same_holder(previous.get("holder"), holder) else []
    me = land_process(holder, chain)
    if me and me not in out:
        out.append(me)
    return out


def register_land(repo):
    """Called by a land (workspaces.sh merge) before its first write: when this
    process holds the lease of `repo`, the command it runs in is added to the
    lease, so the holder's turn end cannot release the lease under it, whichever
    call took the lease. True when recorded; never raises."""
    try:
        conf, paths = launcher_for(repo)
        if not fenced(conf) or not paths:
            return False
        files = Files(conf)
        chain = ancestors()
        flock = _Flock(files.lock)
        deadline = time.monotonic() + 10
        while not flock.try_take():
            if time.monotonic() >= deadline:
                return False
            time.sleep(0.1)
        try:
            held, lease = authorized(files, chain)
            if not held:
                return False
            lease["land_processes"] = _land_processes(lease, lease["holder"], chain)
            write_json_atomic(files.lease, lease)
            return True
        finally:
            flock.release()
    except Exception:              # noqa: BLE001  a record that fails leaves the land as it was before
        return False


# ---------------------------------------------------------------------------
# at rest (e6)
# ---------------------------------------------------------------------------

def in_progress_files(gitdir):
    found = []
    for name, what in (("MERGE_HEAD", "merge in progress"), ("CHERRY_PICK_HEAD", "cherry-pick in progress"),
                       ("REVERT_HEAD", "revert in progress"), ("rebase-merge", "rebase in progress"),
                       ("rebase-apply", "rebase or am in progress")):
        if os.path.exists(os.path.join(gitdir, name)):
            found.append((name, what))
    return found


def head_value(gitdir, name):
    try:
        with open(os.path.join(gitdir, name), encoding="utf-8") as fh:
            return fh.read().split()[0].strip()
    except (OSError, IndexError):
        return ""


def at_rest(main_checkout, gitdir):
    """(True, [], []) or (False, [reason, ...], [dirty path, ...])."""
    reasons, dirty = [], []
    for _name, what in in_progress_files(gitdir):
        reasons.append(what)
    rc, out, _ = git(main_checkout, "diff", "--name-only", "--diff-filter=U")
    if rc == 0 and out.strip():
        reasons.append("unmerged paths")
    rc, out, _ = git(main_checkout, "status", "--porcelain", "--untracked-files=no")
    if rc != 0:
        reasons.append("git status failed")
    else:
        dirty = [line[3:] for line in out.splitlines() if line.strip()]
        if dirty:
            reasons.append("uncommitted changes (%s)" % ", ".join(dirty[:6] + (["..."] if len(dirty) > 6 else [])))
    rc, out, _ = git(main_checkout, "rev-list", "--count", "main@{upstream}..main")
    if rc != 0:
        reasons.append("main has no upstream, so 'pushed' cannot be decided")
    else:
        try:
            n = int(out.strip())
        except ValueError:
            n = -1
        if n:
            reasons.append("%s commit%s not pushed" % (n, "" if n == 1 else "s"))
    return (not reasons), reasons, dirty


# ---------------------------------------------------------------------------
# preservation before a takeover or an abort (G5 point 2)
# ---------------------------------------------------------------------------

def preserve(files, paths, why):
    """The directory the repository's state was preserved in, BEFORE a takeover
    or an abort. Whether it is complete is preservation_problems(target).

    A Git view that FAILED is never stored as the view. It used to be: the
    exit status was ignored and stderr was written as the patch, so an abort
    went ahead with "fatal: ..." as the only copy of a merge's resolutions
    (hunt 2026-09-29 part 5, finding 60). A failed view is written beside the
    others as <name>.error, and why.json lists every failure, so the caller
    can refuse the destructive step and the directory still says what it
    managed to keep."""
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    target = os.path.join(files.preserved, "%s-%s-%d" % (files.key, stamp, os.getpid()))
    os.makedirs(target, mode=0o700, exist_ok=True)
    main, gitdir = paths["main"], paths["gitdir"]
    problems = []
    for name, args in (("diff-binary-HEAD.patch", ["diff", "--binary", "HEAD"]),
                       ("status-porcelain-v2.txt", ["status", "--porcelain=v2"]),
                       ("ls-files-unmerged.txt", ["ls-files", "-u"])):
        rc, out, err = git(main, *args)
        if rc != 0:
            detail = (err or out).strip()
            problems.append("git %s failed (exit %s): %s" % (" ".join(args), rc, detail[:300] or "no message"))
            with open(os.path.join(target, name + ".error"), "w", encoding="utf-8") as fh:
                fh.write(detail + "\n")
            continue
        with open(os.path.join(target, name), "w", encoding="utf-8") as fh:
            fh.write(out)
    for name in ("MERGE_HEAD", "MERGE_MSG", "CHERRY_PICK_HEAD", "REVERT_HEAD"):
        src = os.path.join(gitdir, name)
        if os.path.isfile(src):
            try:
                with open(src, "rb") as a, open(os.path.join(target, name), "wb") as b:
                    b.write(a.read())
            except OSError as error:
                problems.append("%s could not be copied (%s)" % (name, error))
    write_json_atomic(os.path.join(target, "why.json"), {"why": why, "at": iso(), "repository": main,
                                                         "complete": not problems, "problems": problems})
    return target


def preservation_problems(target):
    """[] when `target` holds a complete preservation, else what is missing.
    Read back from the directory rather than trusted from the call, so the
    answer is the one a person recovering from it would get."""
    if not target or not os.path.isdir(target):
        return ["nothing was preserved (%s)" % (target or "no directory")]
    rec = read_json(os.path.join(target, "why.json"))
    if not rec:
        return ["the preservation record %s could not be read" % os.path.join(target, "why.json")]
    if rec.get("complete") is not True:
        return list(rec.get("problems") or ["the preservation record does not say it is complete"])
    return []


# ---------------------------------------------------------------------------
# the second review: no work lands without a passing review of exactly its tip
# ---------------------------------------------------------------------------
# Ruling §113 (2026-10-08): "A regular RichOS user can never be expected
# anything even remotely close to that. So, this all must be completely
# automated." Slice 3 of richos-hq docs/plans/2026-10-09-automatic-second-
# review-and-t3-ideas.md (§2.4, §2.5, §4 row 3) with Sage's check (§2 catches 1
# and 8): scripts/second-review.sh writes the verdicts, review-watch starts
# them, and THIS refuses a move of main that lands work whose tip has no
# passing verdict. It lives here, in the fence, because the fence sees every
# move of main whoever makes it: `workspaces.sh merge`, a plain `git merge`, a
# fast-forward, and Rich's land of a codex/ branch (a plain merge in the main
# checkout, which never enters the workspace registry). workspaces.py's
# merge_and_land asks the same functions BEFORE it merges, so its refusal comes
# before git's merge gate runs, and also with the fence switched off.
#
# WHICH REPOSITORIES: only those listed in the entity's SECOND_REVIEW_REPOS.
# `operator-fences.sh install` bakes the review ledger's path into the launcher
# (OPERATOR_FENCES_REVIEWS) for a listed repository and leaves it empty for any
# other, exactly as it copies LAND_LEASE_HOLDERS; `status` reports a launcher
# that disagrees with the declaration.
#
# WHICH VERDICT COUNTS: the NEWEST handover or by-hand verdict whose tip is
# exactly the commit landed, and it must say `passed`. A mid-job verdict never
# counts (its reviewer was told to list what is not yet claimed instead of
# reporting it). A commit is matched by its full ID alone, never by the path
# the review ran in: the ID names the content and its whole history, and the
# path is often a workspace that no longer exists.

REVIEW_LAND_KINDS = ("handover", "manual")
REVIEW_MID_JOB = ("long-job", "quiet")


def review_ledger_default():
    """The path second_review.py's state_root() gives, plus reviews.jsonl."""
    d = (os.environ.get("SECOND_REVIEW_STATE_DIR") or "").strip()
    if not d:
        base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude")
        d = os.path.join(base, "state")
    return os.path.join(d, "reviews.jsonl")


# ONE READER, AND IT ASKS THE SHELL (reviews rv-20261009T050654Z-e21238ff-7685
# and rv-20261009T052655Z-daf09e19-0bc6). review_repos is the only code that
# reads SECOND_REVIEW_REPOS: `operator-fences.sh install` (operator_fences_admin.py),
# the land check (workspaces.py) and review-watch (review_watch.py) all call it.
# It parses no shell: a reader of lines took a key split by a backslash-newline
# for no assignment, and an assignment inside a skipped `if` or a heredoc for the
# value. orchestration.config is a shell file the engine already sources (the
# hooks do), so review_repos sources it in a clean bash, the way
# disk-watchdog.test.sh (W22d) reads DISK_CONSUMER_CANDIDATES, and takes the
# value bash sets. Then it checks what bash returned: unset is no value; set, it
# must be names of letters, digits, `.`, `_` and `-` separated by spaces (each
# the main checkout's folder name). A bash that exits non-zero (a syntax error,
# a last command that failed, an `exit`), does not finish within
# REVIEW_READ_SECONDS, cannot be run, or prints anything besides the value is an
# error: install refuses before it writes, and the land refuses.
REVIEW_KEY = "SECOND_REVIEW_REPOS"
REVIEW_READ_SECONDS = 10
_REVIEW_NAMES = re.compile(r'[A-Za-z0-9._ -]*')
_REVIEW_OUTPUT = re.compile(r'read\|(set|)\|(.*)\|done', re.S)
# `|| exit` exits with the failed `.`'s own status. What the config prints while
# it is sourced goes nowhere, so the one printf is all bash prints unless the
# config left something behind (an EXIT trap, a redirect), and the strict match
# of _REVIEW_OUTPUT refuses that.
_REVIEW_READ = ('. "$1" >/dev/null </dev/null || exit\n'
                'printf "read|%s|%s|done" "${SECOND_REVIEW_REPOS+set}" "${SECOND_REVIEW_REPOS-}"')


def review_repos(config):
    """([name, ...], "") for the orchestration.config at path `config`: the
    value of SECOND_REVIEW_REPOS once bash has sourced it, [] when it is set
    and empty; (None, "") when bash leaves it unset; (None, why) when bash
    cannot source it cleanly or the value is not names."""
    config = os.path.abspath(config)
    env = {"HOME": os.environ.get("HOME") or os.path.expanduser("~"),
           "PATH": os.environ.get("PATH") or "/usr/bin:/bin", "LC_ALL": "C"}
    if os.environ.get("TMPDIR"):
        env["TMPDIR"] = os.environ["TMPDIR"]
    try:
        # Its own process group, so a timeout stops whatever the config started
        # too (the group of the pid captured here, never a name match).
        p = subprocess.Popen(["bash", "--noprofile", "--norc", "-c", _REVIEW_READ, "_", config],
                             cwd=os.path.dirname(config), env=env, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    except OSError as exc:
        return None, "bash could not be run to source it (%s)" % (exc.strerror or exc)
    try:
        out, err = p.communicate(timeout=REVIEW_READ_SECONDS)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(p.pid, 9)
        except OSError:
            pass
        p.stdout.close()
        p.stderr.close()
        p.wait()
        return None, "bash did not finish sourcing it within %s seconds" % REVIEW_READ_SECONDS
    if p.returncode != 0:
        lines = [x.strip() for x in err.decode("utf-8", "replace").splitlines() if x.strip()]
        return None, "sourcing it in bash exits %d%s" % (p.returncode, ": " + lines[-1][:200] if lines else "")
    m = _REVIEW_OUTPUT.fullmatch(out.decode("utf-8", "replace"))
    if not m:
        return None, "sourcing it in bash printed something besides the value (%r)" % out[:120]
    if not m.group(1):
        return None, ""
    if not _REVIEW_NAMES.fullmatch(m.group(2)):
        return None, ("bash reads it as %r, which is not names of letters, digits, '.', '_' and '-' separated "
                      "by spaces" % m.group(2)[:120])
    return m.group(2).split(), ""


def review_listed(names, main):
    """Is the main checkout `main` listed in `names` (review_repos)?"""
    return os.path.basename(os.path.realpath(main or "")) in (names or ())


# RELEASE NOTES NEED NO SECOND REVIEW (CEO 2026-10-23: any review of a
# release-notes job is useless). Work whose changed files are all release
# notes lands without one, and review-watch starts none.
RELEASE_NOTES = ("docs/CHANGELOG-NIGHTLY.md", "docs/CHANGELOG.md", "docs/release-notes.json")


def notes_only(cwd, base, tip, keep_git_env=False):
    """Does `tip` bring in over `base` (git diff base...tip, renames split into
    a delete and an add) changes to release notes and to nothing else? False
    when it changes nothing or git cannot say."""
    rc, out, _e = git(cwd, "diff", "--no-renames", "--name-only", "-z", "%s...%s" % (base, tip),
                      keep_git_env=keep_git_env)
    files = [f for f in out.split("\0") if f] if rc == 0 else []
    return bool(files) and all(f in RELEASE_NOTES for f in files)


def landed_tips(cwd, old, new, keep_git_env=False):
    """The commits whose work a move of main from `old` to `new` lands; each
    needs a passing review of exactly itself. [] when the move lands nothing.

      * a delete, or a rollback (main goes back to a commit it already
        held): [] - the lease rule is the fence for those;
      * a creation of main where there is none (`old` all zeros, as after a
        deletion): [new], the whole move, because nothing says main ever held
        it (review rv-20261009T025426Z-15dae5ca-3cc1, finding 1). The engine's
        own create-only restore never gets here: fence_decide lets it through
        before it asks. A caller whose Git handed it an all-zero old value for
        a main that exists (`git update-ref` with no expected old value) passes
        the real current main as `old` instead;
      * a replacement (main moves to a commit that holds work main did not
        hold, and is not a fast-forward): [new], the whole move (review
        rv-20261009T023055Z-b51ebb97-bc65, finding 1: a `git reset --hard` to
        an unreviewed divergent branch was taken for a rollback);
      * merge commits on main's own line whose files are exactly the clean
        merge of their two parents: each parent after the first that main did
        not already hold (the branch tip the merge brings in);
      * a merge with changes of its own (a conflict resolved by hand, a file
        added while merging, more than two parents), wherever it was made:
        [new], the whole move, because no branch review saw those changes
        (the same review, finding 2);
      * a fast-forward to work made elsewhere, or any commit on main's line
        that is not a merge and changes a file: [new], the whole move;
      * a commit on main's line that changes no file lands nothing."""
    if is_zero(new) or old == new:
        return []
    if is_zero(old):
        return [new]
    rc, _o, err = git(cwd, "merge-base", "--is-ancestor", old, new, keep_git_env=keep_git_env)
    if rc not in (0, 1):
        raise RuntimeError("git merge-base --is-ancestor %s %s failed: %s" % (old[:12], new[:12], err.strip()))
    if rc == 1:
        rc, _o, err = git(cwd, "merge-base", "--is-ancestor", new, old, keep_git_env=keep_git_env)
        if rc not in (0, 1):
            raise RuntimeError("git merge-base --is-ancestor %s %s failed: %s" % (new[:12], old[:12], err.strip()))
        return [] if rc == 0 else [new]
    rc, out, err = git(cwd, "rev-list", "--first-parent", "--parents", "%s..%s" % (old, new),
                       keep_git_env=keep_git_env)
    if rc != 0:
        raise RuntimeError("git rev-list %s..%s failed: %s" % (old[:12], new[:12], err.strip()))
    line = [ln.split() for ln in out.splitlines() if ln.strip()]
    if not line:
        return []
    if len(line[-1]) < 2 or line[-1][1] != old:
        return [new]
    tips = []
    for commit in line:
        c, parents = commit[0], commit[1:]
        if len(parents) == 1:
            rc, out, _e = git(cwd, "rev-parse", c + "^{tree}", parents[0] + "^{tree}", keep_git_env=keep_git_env)
            trees = out.split()
            if rc == 0 and len(trees) == 2 and trees[0] == trees[1]:
                continue
            return [new]
        if not clean_merge(cwd, c, parents, keep_git_env):
            return [new]
        for p in parents[1:]:
            rc, _o, _e = git(cwd, "merge-base", "--is-ancestor", p, old, keep_git_env=keep_git_env)
            if rc != 0 and p not in tips:
                tips.append(p)
    return tips


def clean_merge(cwd, commit, parents, keep_git_env=False):
    """Are the files of the merge `commit` exactly what Git's own merge of its
    two parents makes? Then it brings in its second parent's work and nothing
    else, wherever and whenever it was made. A merge Git could not make by
    itself (a conflict resolved by hand), one with a file changed while
    merging, or one with more than two parents is not."""
    if len(parents) != 2:
        return False
    rc, out, err = git(cwd, "merge-tree", "--write-tree", parents[0], parents[1], keep_git_env=keep_git_env)
    if rc not in (0, 1):
        raise RuntimeError("git merge-tree %s %s failed: %s" % (parents[0][:12], parents[1][:12], err.strip()))
    if rc == 1:
        return False
    merged = (out.split() or [""])[0]
    rc, tree, err = git(cwd, "rev-parse", commit + "^{tree}", keep_git_env=keep_git_env)
    if rc != 0:
        raise RuntimeError("git rev-parse %s^{tree} failed: %s" % (commit[:12], err.strip()))
    return bool(merged) and merged == tree.strip()


def read_reviews(ledger):
    rows = []
    try:
        with open(ledger, encoding="utf-8") as fh:
            for raw in fh:
                try:
                    r = json.loads(raw)
                except ValueError:
                    continue
                if isinstance(r, dict) and r.get("verdict"):
                    rows.append(r)
    except OSError:
        pass
    return rows


def review_repo_resolved(repo):
    """(the canonical identity of a repository for matching verdicts, whether Git
    resolved it). The identity is the real path of its main checkout; Git decides
    it (the common git directory), so every linked worktree of a repository has
    the identity of that repository while an independent clone stays distinct. A
    path Git cannot resolve (gone, moved, or not a repository) is its own real
    path and is reported as unresolved. The common directory is stored and compared
    exactly as Git reports it, never rewritten (a bare clone at `storage` and a
    separate-git-dir clone at `storage/.git` are two repositories)."""
    real = os.path.realpath(repo or "")
    resolved = False
    if repo and os.path.isdir(real):
        code, out, _e = git(real, "rev-parse", "--git-common-dir", raw=True)
        common = os.fsdecode(out[:-1] if out.endswith(b"\n") else out)
        if code == 0 and common:
            real = os.path.realpath(os.path.join(real, common))
            resolved = True
    return real, resolved


def review_repo_identity(repo):
    return review_repo_resolved(repo)[0]


def review_row_belongs(row, ident):
    """Whether a verdict row counts for the repository `ident`. A row stores the
    identity second-review resolved when it WROTE the review (`repo_id`), so a
    worktree that is later moved or removed still resolves. A row without it
    (written before that) is resolved from its path; if the path no longer
    resolves, its repository is unknown, so it counts against a land whatever its
    verdict (see review_row_standing)."""
    if not ident or not row.get("repo"):
        return False
    stored = row.get("repo_id")
    if stored:
        return stored == ident
    found, resolved = review_repo_resolved(row["repo"])
    if resolved:
        return found == ident
    return True


def review_row_standing(row):
    """The row as it counts: an unresolved legacy row (no stored identity, path no
    longer resolves) blocks like a changes-requested, even if it says passed."""
    if row.get("repo_id") or review_repo_resolved(row.get("repo"))[1] or row.get("verdict") != "passed":
        return row
    return dict(row, verdict="changes-requested", unresolved_legacy_pass=True)


def review_record(row):
    try:
        with open(os.path.join(row.get("record") or "", "verdict.json"), encoding="utf-8") as f:
            record = json.load(f)
        if not isinstance(record, dict):
            return {}
        return record
    except (OSError, ValueError, TypeError):
        return {}


def open_review_findings(rows):
    """Finding identity survives duplicate reviews. Only witnessed resolutions
    remove it. A same-source withdrawal must be an explicit independent recheck
    carrying the original finding; a later PASS cannot erase another review.
    """
    opened, by_id, seen = {}, {r.get("id"): r for r in rows}, set()
    for row in rows:
        if row.get("id") and row["id"] in seen:
            continue
        seen.add(row.get("id"))
        record = review_record(row)
        answer = record.get("answer") or {}
        parent = by_id.get(row.get("recheck_of"))
        parent_record = review_record(parent) if parent else {}
        parent_ids = {"%s#%d" % (parent.get("id"), n) for n, _ in enumerate(
                      (parent_record.get("answer") or {}).get("findings") or [], 1)} if parent else set()
        parent_ids.update(e.get("id") for e in parent_record.get("earlier_findings_in") or [])
        witnessed = {e.get("id"): e for e in record.get("earlier_findings_in") or []}
        for e in answer.get("earlier_findings") or []:
            fid = e.get("id")
            if fid not in opened or fid not in witnessed:
                continue
            old_tip = opened[fid].get("source_tip")
            same_source = old_tip == row.get("tip")
            allowed = not same_source or (parent and parent.get("tip") == old_tip and fid in parent_ids)
            if e.get("status") in ("fixed", "withdrawn") and e.get("note", "").strip() and allowed:
                opened.pop(fid)
            elif e.get("status") == "still-open":
                if e.get("blocks") is not False:
                    opened[fid]["blocks"] = True
                elif e.get("note", "").strip() and allowed:
                    opened[fid]["blocks"] = False
                if opened[fid].get("blocks") is False and witnessed[fid].get("filed"):
                    opened[fid]["filed"] = True
        findings = answer.get("findings") or []
        for n, finding in enumerate(findings, 1):
            if not isinstance(finding, dict):
                continue
            opened["%s#%d" % (row.get("id"), n)] = dict(finding,
                blocks=finding.get("blocks", row.get("verdict") == "changes-requested"), source_tip=row.get("tip"),
                filed=row.get("verdict") == "passed" and finding.get("blocks") is False)
        if row.get("verdict") == "changes-requested" and not record:
            opened["%s#unavailable" % row.get("id")] = {
                "priority": 1, "blocks": True, "title": "Blocking review details unavailable",
                "files": [], "evidence": "Recover the original review record: %s" % row.get("record"),
                "fixture": "", "source_tip": row.get("tip")}
    return opened


def _own_history(repo, other, tip):
    """Whether a review of the commit `other` belongs to the history of `tip`:
    the same commit, or one `tip` was built on. A work key can be shared by
    branches that never met (a manual review's key is its base, so two
    branches cut from the same main commit share it, 2026-10-15), and another
    branch's open findings never decide this one. When Git cannot tell (no
    repository, an unknown commit), the row still counts: a check that cannot
    decide refuses."""
    if not other or other == tip:
        return True
    if not repo:
        return True
    code, _out, _err = git(repo, "merge-base", "--is-ancestor", other, tip)
    return code != 1


def review_deferrals_path(ledger):
    """review-deferrals.jsonl, beside the review ledger (second_review.py writes it)."""
    return os.path.join(os.path.dirname(ledger), "review-deferrals.jsonl")


def read_deferrals(ledger):
    """{finding id: the row that filed it as a follow-up}. The lander's ruling on
    a blocking finding (ruling §116: "Rich applies the same test"), written by
    `second-review.sh --follow-up`; a later `--reopen` row of the same finding
    undoes it, so the later row wins."""
    out = {}
    try:
        with open(review_deferrals_path(ledger), encoding="utf-8") as fh:
            for raw in fh:
                try:
                    r = json.loads(raw)
                except ValueError:
                    continue
                if not isinstance(r, dict) or not r.get("finding"):
                    continue
                if r.get("action") == "follow-up":
                    out[r["finding"]] = r
                elif r.get("action") == "reopen":
                    out.pop(r["finding"], None)
    except OSError:
        pass
    return out


def review_of(rows, tip, repo, deferred=None):
    """(the verdict row that decides `tip` in `repo`, the newest mid-job row on
    it). A row of another repository, or with no repository, never counts: the
    ledger is shared, and independent clones or forks can hold the same commit.

    `deferred` (read_deferrals) is applied last: a finding the lander filed as a
    follow-up does not block, whatever a later review says of it. When every
    blocking finding in the history was filed so, the changes-requested row
    stands as passed (never an unresolved legacy row); the row then carries the
    rulings it rests on in `deferred`."""
    ident = review_repo_identity(repo) if repo else ""
    mine = [review_row_standing(r) for r in rows if review_row_belongs(r, ident)]
    land = [r for r in mine if r.get("tip") == tip and r.get("trigger") in REVIEW_LAND_KINDS]
    mid = [r for r in mine if r.get("tip") == tip and r.get("trigger") in REVIEW_MID_JOB]
    effective = dict(land[-1]) if land else None
    if effective:
        # Preserve this source's job history, including findings raised at an
        # earlier tip. Later source resolutions cannot certify an older source.
        end = max(i for i, r in enumerate(mine) if r.get("tip") == tip and r.get("trigger") in REVIEW_LAND_KINDS)
        works = {r.get("work") for r in land if r.get("work")}
        history = [r for r in mine[:end+1] if (r.get("work") in works and _own_history(repo, r.get("tip"), tip)) or
                   (r.get("tip") == tip and r.get("trigger") in REVIEW_LAND_KINDS)]
        opened = open_review_findings(history)
        applied = {}
        for fid, ruling in (deferred or {}).items():
            if fid in opened and review_row_belongs(ruling, ident):
                if opened[fid].get("blocks") is not False:
                    applied[fid] = ruling
                opened[fid] = dict(opened[fid], blocks=False, filed=True)
        blocked = {k: f for k, f in opened.items() if f.get("blocks") is not False}
        if blocked:
            effective.update(verdict="changes-requested", blocking=len(blocked),
                             effective_findings=blocked)
        elif applied and effective.get("verdict") == "changes-requested" \
                and not effective.get("unresolved_legacy_pass"):
            effective.update(verdict="passed", blocking=0)
        if applied:
            effective["deferred"] = applied
    return effective, (mid[-1] if mid else None)


def review_blocking_ids(row):
    """The ids of the findings that hold `row`, as `--follow-up` takes them."""
    if row.get("effective_findings"):
        return list(row["effective_findings"])
    answer = review_record(row).get("answer") or {}
    return ["%s#%d" % (row.get("id"), n) for n, f in enumerate(answer.get("findings") or [], 1)
            if isinstance(f, dict) and f.get("blocks", row.get("verdict") == "changes-requested")]


def review_findings(row, limit=6):
    """[(priority, title, files)] of a verdict, from its record."""
    if row.get("effective_findings"):
        return [(f.get("priority"), f.get("title"), ", ".join(f.get("files") or []))
                for f in list(row["effective_findings"].values())[:limit]]
    try:
        with open(os.path.join(row.get("record") or "", "verdict.json"), encoding="utf-8") as fh:
            answer = (json.load(fh) or {}).get("answer") or {}
    except (OSError, ValueError, AttributeError):
        return []
    out = []
    for f in (answer.get("findings") or [])[:limit]:
        if isinstance(f, dict):
            out.append((f.get("priority"), " ".join(str(f.get("title") or "").split())[:140],
                        ", ".join(str(x) for x in (f.get("files") or []))[:100]))
    return out


def review_gaps(ledger, tips, repo):
    """{tip: why} for every tip with no passing verdict; {} when all pass."""
    rows = read_reviews(ledger)
    deferred = read_deferrals(ledger)
    gaps = {}
    for tip in tips:
        row, mid = review_of(rows, tip, repo, deferred)
        if row and row.get("verdict") == "passed":
            recovery_home = os.path.join(os.path.dirname(ledger), "recovery")
            if os.path.isdir(recovery_home):
                try:
                    here = os.path.dirname(os.path.realpath(__file__))
                    if here not in sys.path:
                        sys.path.insert(0, here)
                    import recovery
                    work_keys = {r.get("work") for r in rows if r.get("tip") == tip and
                                 review_row_belongs(r, review_repo_identity(repo)) and r.get("work")}
                    blocked = [recovery.landing_gap(repo, work, ledger) for work in work_keys]
                    blocked = [reason for reason in blocked if reason]
                    if blocked:
                        gaps[tip] = blocked
                except (OSError, ValueError, ImportError) as error:
                    gaps[tip] = ["recovery acceptance cannot be established: %s" % error]
            continue
        if row is None:
            why = ["no second review of %s has a verdict yet" % tip[:12]]
            if mid:
                why[0] += (" (a mid-job review %s said %s; only the review at its handover counts)"
                           % (mid.get("id"), mid.get("verdict")))
        else:
            why = ["the second review %s of %s says %s: %s finding(s), %s P1, by %s (%s). Record: %s" % (
                row.get("id"), tip[:12], row.get("verdict"), row.get("findings"), row.get("p1"),
                row.get("reviewer"), row.get("reviewer_model"), os.path.join(row.get("record") or "?", "verdict.json"))]
            ids = review_blocking_ids(row)
            if ids:
                why += ["  Blocking finding id(s): %s" % ", ".join(ids)]
            why += ["  P%s %s (%s)" % f for f in review_findings(row)]
        gaps[tip] = why
    return gaps


def review_refusal_text(repo, gaps, engine, in_git=False):
    lines = ["=== SECOND REVIEW: refused in %s ===" % repo,
             "  Work lands only with a passing second review of exactly its tip (CEO §113)."]
    for tip in sorted(gaps):
        lines += ["  " + w for w in gaps[tip]]
    if in_git:
        # Measured: Git asks this fence after it has written the merged files.
        lines += ["  Main did not move, but Git had already staged the merged files in the main checkout:",
                  "  a refused merge is left in progress (end it with: git merge --abort) and a refused",
                  "  fast-forward leaves them staged (git reset --hard HEAD). You hold the land lease for both."]
    lines += [
        "  The way through: review-watch starts the review by itself when a teammate hands over",
        "  (and at Codex's READY entry); its verdict reaches the lead. For changes requested, start a",
        "  fresh continuation with the findings as its input; its own handover is reviewed again.",
        "  Or, when no user hits a blocking finding in normal use of this work (ruling §116), the lead files",
        "  it as a follow-up and the work lands: %s/scripts/second-review.sh --follow-up <finding id>"
        % (engine or "<engine>"),
        "  --why '<who rules, and why>' (undone with --reopen <finding id>).",
        "  To review a tip now: %s/scripts/second-review.sh --repo %s --tip <commit> --words-file <request>"
        % (engine or "<engine>", repo),
        "  Only the repositories in SECOND_REVIEW_REPOS (orchestration.config) are refused."]
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# the fence (e2, F2, G1, G3, G4; Frank §4 (B) HEAD)
# ---------------------------------------------------------------------------

def current_main(common):
    """C: what refs/heads/main resolves to right now: the loose ref if present,
    else its packed-refs entry, else None. At `prepared` the loose ref still
    holds the old value (measured by Frank on both gits)."""
    try:
        with open(os.path.join(common, "refs", "heads", "main"), encoding="utf-8") as fh:
            value = fh.read().strip()
        if value and not value.startswith("ref:"):
            return value
    except OSError:
        pass
    try:
        with open(os.path.join(common, "packed-refs"), encoding="utf-8") as fh:
            for line in fh:
                parts = line.split()
                if len(parts) == 2 and parts[1] == MAIN and not line.startswith(("#", "^")):
                    return parts[0]
    except OSError:
        pass
    return None


def writer_git_argv(chain):
    """argv of the nearest ancestor whose executable is git, skipping git's own
    global options, so argv[0] is the subcommand."""
    for pid, _start in chain:
        argv = proc_argv(pid)
        if not argv or os.path.basename(argv[0]) != "git":
            continue
        rest, i = argv[1:], 0
        while i < len(rest):
            a = rest[i]
            if a in ("-C", "-c", "--git-dir", "--work-tree", "--namespace", "--exec-path") and i + 1 < len(rest):
                i += 2
                continue
            if a.startswith("-"):
                i += 1
                continue
            break
        return rest[i:]
    return []


def rewrote_before_fence(argv):
    """Does this Git command rewrite the index and the working tree BEFORE its
    first reference-transaction call (its ORIG_HEAD `prepared`)? Measured on
    both gits (richos-hq docs/verification/2026-09-24-operator-fences/probes and
    Frank's re-check §1): reset other than --soft, every --abort, rebase and
    stash do; merge and pull are stopped before the tree changes."""
    if not argv:
        return False
    if argv[0] == "reset":
        return "--soft" not in argv[1:]
    return argv[0] in ("rebase", "stash") or "--abort" in argv[1:]


def index_residue(cwd):
    """[{path, head, index}] where the index differs from HEAD, read-only
    (`git diff --cached --raw`). This is the residue a refused writer that
    rewrote the index first leaves behind (Frank's Fix 2 point 3)."""
    rc, out, _ = git(cwd, "diff", "--cached", "--raw", "--no-renames", "--no-abbrev", "-z", "HEAD",
                     keep_git_env=True)
    parts = out.split("\0") if rc == 0 else []
    found = []
    for i in range(0, len(parts) - 1, 2):
        meta = parts[i].split()
        if len(meta) >= 4 and parts[i + 1]:
            found.append({"path": parts[i + 1], "head": meta[2], "index": meta[3]})
    return found[:200]


def _refusals_of(files, head, started_at):
    """Refusal records of ONE unfinished operation, newest first: this
    repository, carrying its head, and no older than its head file. The same
    commit can be merged or cherry-picked again later, so an older refusal of
    the same head never describes a newer operation (measured in F13)."""
    out = []
    if not head:
        return out
    for rec in reversed(_read_refusals(files)):
        if rec.get("repository") != files.repo:
            continue
        if head not in (rec.get("merge_head"), rec.get("cherry_pick_head"), rec.get("revert_head")):
            continue
        try:
            if started_at is None or float(rec.get("epoch")) < started_at - 0.5:
                continue
        except (TypeError, ValueError):
            continue
        out.append(rec)
    return out


def _cap(text):
    return text[:1].upper() + text[1:]


def _writer_text(rec):
    argv = rec.get("writer_argv") or []
    caller = rec.get("caller") or {}
    return "a refused `git %s` from %s at %s" % (
        " ".join(argv[:8]) or "(argv not recorded)",
        holder_label(caller) if caller.get("pid") else "a caller with no recorded identity", rec.get("at", "?"))


def discarded_operation(files, gitdir):
    """Fix 1 (Frank's re-check §2, case G): (kind, head, refusal) when the
    merge, cherry-pick or revert in progress here has an ORIG_HEAD refusal on
    record, newer than its head file. That refusal's writer (an --abort or a
    reset) had already rewritten the index, so the staged resolution is gone
    while the head file stays, and concluding it would record an empty-diff
    merge that every "landed" check accepts. It clears itself: a new merge
    writes a newer head file."""
    for kind in ("MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD"):
        head = head_value(gitdir, kind)
        for rec in _refusals_of(files, head, _head_mtime(gitdir, kind)):
            if rec.get("ref") == "ORIG_HEAD":
                return kind, head, rec
    return None


def residue_committed(files, lease, new, cwd):
    """Fix 2 for a holder (Frank's case E): (refusal, [paths]) when the commit
    `new` would record a refused writer's residue: a path whose blob in `new`
    is the one that writer left in the index, not HEAD's. Only refusals newer
    than the holder's last acquire count, because acquire and renew NAME the
    residue (residue_notes), and a holder that has been told and renews may
    commit it deliberately."""
    since = float((lease or {}).get("renewed_epoch") or 0)
    for rec in reversed(_read_refusals(files)):
        entries = rec.get("residue_paths") or []
        try:
            newer = float(rec.get("epoch")) > since
        except (TypeError, ValueError):
            newer = False
        if rec.get("repository") != files.repo or not entries or not newer:
            continue
        rc, out, _ = git(cwd, "ls-tree", "-r", "-z", "--full-tree", new, "--",
                         *[e["path"] for e in entries], keep_git_env=True)
        if rc != 0:
            continue
        blobs = {}
        for item in out.split("\0"):
            meta, _tab, path = item.partition("\t")
            if path and len(meta.split()) >= 3:
                blobs[path] = meta.split()[2]
        hit = [e["path"] for e in entries if e["index"] != e["head"]
               and blobs.get(e["path"], "0" * len(e["index"])) == e["index"]]
        if hit:
            return rec, hit
    return None


def route_refused_review(ledger, tips, repo):
    """The shared landing backstop requests applicability review without ever
    changing the effective outcome. Host watcher consumes durable requests.
    """
    library = os.path.dirname(os.path.realpath(__file__))
    if library not in sys.path:
        sys.path.insert(0, library)
    import recovery
    source = recovery.rows(ledger)
    for tip in tips:
        qualified = [r for r in source if r.get("tip")==tip and review_row_belongs(r, review_repo_identity(repo))]
        for work in sorted({r["work"] for r in qualified if r.get("work")}):
            findings = [fid for fid,f in recovery.source_findings(source, repo, work, tip, ledger).items() if f.get("blocks")]
            if findings:
                recovery.route_request(repo, work, tip, findings, "refused-landing", ledger)


def fence_decide(lines, common, gitdir, files, chain, argv_of_writer=None, enforce_lease=True):
    """[(line, why, detail)] of refused updates, [] when everything passes.

    `lines` are (old, new, ref). Almost every refusal is a missing lease, and
    every rule below decides whether an update NEEDS one (detail None). Two
    refuse even the lease holder, because the index under it was already
    rewritten by a refused writer (Frank's Fixes 1 and 2); their detail is the
    text that says so."""
    main_worktree = os.path.realpath(gitdir) == os.path.realpath(common)
    held = None
    lease = None
    writer = None
    refused = []

    def lease_held():
        nonlocal held, lease
        if held is None:
            held, lease = authorized(files, chain)
        return held

    def writer_argv():
        nonlocal writer
        if writer is None:
            writer = argv_of_writer() if argv_of_writer else writer_git_argv(chain)
        return writer

    for old, new, ref in lines:
        need, why = False, ""
        if ref == MAIN:
            current = current_main(common)
            if not is_zero(new):
                if current is not None and new == current:
                    need = False              # packing, or a rewrite to the same value (F2)
                elif current is None and _restore_intent_matches(files, new, chain):
                    need = False              # the engine's create-only restore (G3)
                else:
                    need, why = True, "a move of main"
            else:
                if writer_argv()[:1] == ["pack-refs"]:
                    need = False              # the second half of packing (G4)
                else:
                    need, why = True, "a deletion of main"
        elif ref == "ORIG_HEAD" and main_worktree:
            # G1 as measured, not as first claimed: a merge or pull is stopped
            # here before the tree changes, but a reset, an --abort, a rebase or
            # a stash has already rewritten the index and the tree by now.
            need, why = True, ("ORIG_HEAD in the main checkout (a merge, pull, reset, rebase, stash or "
                               "--abort; only the ref is protected here)")
        elif ref == "HEAD" and main_worktree:
            leaves_main = (new.startswith("ref:") and new != MAIN_SYMREF) or \
                          (not new.startswith("ref:") and not is_zero(new))
            if leaves_main and writer_argv()[:2] != ["worktree", "add"]:
                need, why = True, "switching the main checkout away from main"
        if need and enforce_lease and not lease_held():
            refused.append(((old, new, ref), why, None))
        elif need and ref == MAIN and main_worktree and not is_zero(new):
            detail = _holder_refusal(files, gitdir, lease, new)
            if detail:
                refused.append(((old, new, ref), "a move of main over a refused writer's residue", detail))
        if need and ref == MAIN and files.reviews and not is_zero(new):
            # Lease or no lease: the second review is not the lease's question.
            # Git hands an all-zero old value for an update with no expected
            # old value (`git update-ref refs/heads/main <tip>`) even when main
            # exists, so the move is judged from the main there really is
            # (review rv-20261009T025426Z-15dae5ca-3cc1, finding 1).
            moved_from = current if is_zero(old) and current is not None else old
            gaps = move_review_gaps(files, moved_from, new, keep_git_env=True)
            if gaps:
                refused.append(((old, new, ref), REVIEW_WHY,
                                review_refusal_text(files.repo, gaps, files.engine, in_git=True)))
    return refused


REVIEW_WHY = "a move of main that lands work with no passing second review of its tip"


def move_review_gaps(files, old, new, keep_git_env=False):
    """{tip: why} for the move of a land target from `old` to `new`: the one
    question the fence asks of a move of main and review-check asks before a
    land's checks. A check that cannot decide refuses, and says so."""
    try:
        tips = [t for t in landed_tips(os.getcwd(), old, new, keep_git_env=keep_git_env)
                if not notes_only(os.getcwd(), old, t, keep_git_env=keep_git_env)]
        return review_gaps(files.reviews, tips, files.repo)
    except Exception as error:  # noqa: BLE001: a check that cannot decide refuses, and says so
        return {new: ["the second review of this move could not be checked (%s: %s), so it is refused"
                      % (error.__class__.__name__, error)]}


# THE REVIEW IS ASKED BEFORE THE LAND'S CHECKS, ON EVERY ENTRANCE (verification
# reliability PRD, R8 and A9, 2026-10-20). On 2026-10-19 a plain `git merge` of
# v1.2.1 into main ran its whole land check and was then refused here, at the
# move of main, because its exact tip had no review; `workspaces.sh merge`
# already asked first (_review_check). autocheck's LAND path now asks this
# command before it runs a check: the installed copy is brought current first,
# exactly as `fence prepared` does, so the answer is the one the fence will
# give. It asks it of any land target autocheck lands onto, a release branch
# named by the lander included, which this hook never sees (the launcher runs
# the fence for refs/heads/main only), so main and a release branch are held to
# the same review. It decides nothing about the lease, and writes no refusal
# record: nothing has moved and no writer was refused.
def cmd_review_check(argv):
    """`review-check <launcher> <old> <new>`, run in the checkout being landed
    into: exit 0 when every tip the move from `old` to `new` lands has a
    passing second review, or when this repository's launcher carries no
    review ledger; exit 1, with the refusal, otherwise."""
    if len(argv) != 3:
        sys.stderr.write("usage: operator_fences.py review-check <launcher> <old> <new>\n")
        return 2
    conf = read_launcher(argv[0])
    if not conf or not conf.get("REVIEWS"):
        return 0
    try:
        stale = bring_self_current(conf)
    except OSError as error:
        stale = []
        sys.stderr.write("operator fence: the installed fence differs from the engine's and could not be "
                         "brought current (%s); it decides as installed. Run operator-fences.sh install "
                         "--repo %s.\n" % (error, conf.get("REPO") or ""))
    if stale:
        os.execv(sys.executable, [sys.executable, conf["PROGRAM"], "review-check"] + list(argv))
    files = Files(conf)
    gaps = move_review_gaps(files, argv[1], argv[2])
    if not gaps:
        return 0
    try:
        route_refused_review(files.reviews, list(gaps), files.repo)
    except (OSError, ValueError):
        pass  # Refusal is still real; routing availability is reported by the watcher.
    sys.stderr.write(review_refusal_text(files.repo, gaps, files.engine) + "\n")
    return 1


def _holder_refusal(files, gitdir, lease, new):
    """The text refusing the LEASE HOLDER's move of main (Fixes 1 and 2), or ''."""
    found = discarded_operation(files, gitdir)
    if found:
        kind, head, rec = found
        return ("%s had already discarded the staged resolution of the %s in progress here (%s %s) before "
                "the fence refused it: Git rewrites the index first. Committing now would record that "
                "operation without its changes, and every 'landed' check would accept it. Run "
                "land-lease.sh abort-orphan (it preserves first), then start it again."
                % (_cap(_writer_text(rec)), _KIND_WORD.get(kind, "operation"), kind, head))
    found = residue_committed(files, lease, new, os.getcwd())
    if found:
        rec, hit = found
        return ("%s had already rewritten the index and the working tree before the fence refused it, and this "
                "commit would record that rewrite: %s. Restore those paths first: git restore "
                "--source=HEAD --staged --worktree -- %s. Only if you have checked them and mean to commit "
                "exactly that, renew the lease (land-lease.sh acquire names them) and commit again."
                % (_cap(_writer_text(rec)), ", ".join(hit[:8]), " ".join(shlex.quote(p) for p in hit[:8])))
    return ""


_KIND_WORD = {"MERGE_HEAD": "merge", "CHERRY_PICK_HEAD": "cherry-pick", "REVERT_HEAD": "revert"}


def _restore_intent_matches(files, new, chain):
    intent = read_json(files.restore_intent)
    if not intent or intent.get("ref") != MAIN or intent.get("value") != new:
        return False
    try:
        return is_ancestor(int(intent["pid"]), int(intent["start"]), chain)
    except (KeyError, TypeError, ValueError):
        return False


def fence_refusal_text(conf, files, refused, lease, repo, rewrote=None):
    engine = conf.get("ENGINE") or "<engine>"
    reviews = [detail for _l, why, detail in refused if detail and why == REVIEW_WHY]
    if reviews:
        others = [r for r in refused if r[1] != REVIEW_WHY]
        tail = ["  Also: " + fence_refusal_text(conf, files, others, lease, repo, rewrote)] if others else []
        return "\n".join(reviews[:1] + tail + [
            "  Review and recovery acceptance stays active while lease fencing is off."])
    held_over = [detail for _l, _w, detail in refused if detail]
    if held_over:
        return "\n".join(["=== OPERATOR FENCE: refused in %s, even for the lease holder ===" % repo]
                         + ["  " + d for d in held_over]
                         + ["  If this refusal is a defect in the fence itself, Rich turns it off with one",
                            "  command: %s/scripts/operator-fences.sh off --repo %s --reason '<verified operator reason>'" % (engine, shlex.quote(repo))])
    state = lease_state(lease)
    if state == "none":
        held = "The land lease for this repository is free."
    else:
        h = lease.get("holder") or {}
        held = "The land lease is held by %s, taken %s ago%s." % (
            holder_label(h), age_text(now() - float(lease.get("acquired_epoch") or now())),
            {"dead": " (its holder has ended)", "expired": " (past its time)"}.get(state, ""))
    what = "; ".join(sorted(set(why for _l, why, _d in refused)))
    already = []
    if rewrote:
        # Fix 2 point 2: the refusal says what had already happened.
        already = [
            "  Already done, and not undone by this refusal: `git %s` had already rewritten the" % " ".join(rewrote[:8]),
            "  index and the working tree before Git asked the fence (measured on both gits). HEAD",
            "  and main did not move. Nobody commits on top of this tree: it would record that rewrite.",
            "  `land-lease.sh status --repo %s` names the paths it left." % repo]
    return "\n".join([
        "=== OPERATOR FENCE: refused in %s ===" % repo,
        "  What: %s." % what] + already + [
        "  Why: this process does not hold the land lease. %s" % held,
        "  The way through: take the lease first, then run the same command again:",
        "      %s/scripts/land-lease.sh acquire --repo %s" % (engine, repo),
        "    (Codex adds --holder codex.) Exit 75 means another holder is landing:",
        "    run it again. Wait for it to exit; it can take 90 seconds.",
        "  Abort a merge only if you started it.",
        "  Never get around this with `git -c core.hooksPath=...` or a direct ref write.",
        "  If this refusal is a defect in the fence itself, Rich turns it off with one",
        "  command: %s/scripts/operator-fences.sh off --repo %s --reason '<verified operator reason>'" % (engine, shlex.quote(repo)),
    ])


PROGRAM_FILES = ("operator_fences.py", "recovery.py", "machine_settings.py")   # what install copies beside the hook


def current_program(conf):
    """{name: bytes} of the fence a land into this repository is judged by, or
    None when any file of it cannot be read: the engine's, and when the engine
    lies inside this repository, the one committed on main (which a move of main
    has not moved yet in `prepared`), never the main checkout's files, which a
    plain `git merge` has already replaced with the incoming work's."""
    engine = conf.get("ENGINE") or ""
    top = os.path.realpath(conf.get("REPO") or "/nonexistent")
    found = {}
    for name in PROGRAM_FILES:
        path = os.path.realpath(os.path.join(engine, "scripts", "lib", name))
        if engine and path.startswith(top + os.sep):
            rc, data, _err = git(top, "cat-file", "blob", "%s:%s" % (MAIN, os.path.relpath(path, top)), raw=True)
            if rc != 0:
                return None
        else:
            try:
                with open(path, "rb") as fh:
                    data = fh.read()
            except OSError:
                return None
        found[name] = data
    return found


def bring_self_current(conf):
    """A STALE INSTALLED FENCE NEVER JUDGES A LAND (2026-10-19), whichever way
    the land comes: `workspaces.sh merge` or a plain `git merge`. Run as the
    launcher's installed copy, this replaces every installed file that differs
    from current_program() (each by rename) and returns their names, [] when
    the copy is current, cannot be compared, or this is not the installed copy."""
    program = conf.get("PROGRAM") or ""
    if not program or os.path.realpath(program) != os.path.realpath(__file__):
        return []
    want = current_program(conf)
    if not want:
        return []
    folder = os.path.dirname(program)
    stale = [n for n in PROGRAM_FILES
             if hashlib.sha256(want[n]).hexdigest() != file_digest(os.path.join(folder, n))]
    for name in stale:
        dest = os.path.join(folder, name)
        tmp = "%s.%d.tmp" % (dest, os.getpid())
        with open(tmp, "wb") as fh:
            fh.write(want[name])
        if name == "operator_fences.py":
            os.chmod(tmp, 0o755)
        os.replace(tmp, dest)
    return stale


def cmd_fence(argv):
    """Called by the launcher: `fence <launcher> <phase>` with the transaction
    on stdin. Exit 0 pass, 1 refuse. In `committed` it only records."""
    if len(argv) < 2:
        return 2
    conf = read_launcher(argv[0])
    phase = argv[1]
    if not conf or (not fenced(conf) and not conf.get("REVIEWS")):
        return 0
    if phase == "prepared":
        try:
            stale = bring_self_current(conf)
        except OSError as error:
            stale = []
            sys.stderr.write("operator fence: the installed fence differs from the engine's and could not be "
                             "brought current (%s); it decides as installed. Run operator-fences.sh install "
                             "--repo %s.\n" % (error, conf.get("REPO") or ""))
        if stale:
            # The same process (so the same ancestry) and the same unread stdin, now running the current
            # fence, which finds itself current and decides.
            os.execv(sys.executable, [sys.executable, conf["PROGRAM"], "fence"] + list(argv))
    files = Files(conf)
    lines = []
    for raw in sys.stdin.read().splitlines():
        parts = raw.split(" ", 2)
        if len(parts) == 3:
            lines.append(tuple(parts))
    common = conf.get("COMMON") or ""
    rc, out, _ = git(os.getcwd(), "rev-parse", "--absolute-git-dir", keep_git_env=True)
    gitdir = os.path.realpath(out.strip()) if rc == 0 and out.strip() else common
    chain = ancestors()
    if phase == "prepared":
        refused = fence_decide(lines, common, gitdir, files, chain, enforce_lease=fenced(conf))
        if not refused:
            return 0
        lease = read_lease(files)
        repo = conf.get("REPO") or common
        who = caller_identity(conf, chain) or {}
        argv = writer_git_argv(chain)
        rewrote = None
        for (old, new, ref), why, detail in refused:
            rec = {
                "at": iso(), "epoch": now(), "repository": repo, "ref": ref, "old": old, "new": new, "why": why,
                "caller": {k: who.get(k) for k in ("kind", "pid", "start", "session_id") if k in who},
                "writer_pid": chain[1][0] if len(chain) > 1 else None, "writer_argv": argv[:24],
                "merge_head": head_value(gitdir, "MERGE_HEAD"),
                "cherry_pick_head": head_value(gitdir, "CHERRY_PICK_HEAD"),
                "revert_head": head_value(gitdir, "REVERT_HEAD")}
            if ref == "ORIG_HEAD" and rewrote_before_fence(argv):
                # Fix 2 point 3: what the refused writer had already left in the index.
                rewrote = argv
                rec["residue_paths"] = index_residue(os.getcwd())
                rec["residue"] = bool(rec["residue_paths"])
            append_jsonl(files.refusals, rec)
        if any(why==REVIEW_WHY for _line,why,_detail in refused):
            try:
                route_refused_review(files.reviews, [line[1] for line,why,_detail in refused if why==REVIEW_WHY], repo)
            except (OSError, ValueError):
                pass  # Refusal is still real; routing availability is reported by the watcher.
        sys.stderr.write(fence_refusal_text(conf, files, refused, lease, repo, rewrote) + "\n")
        return 1
    if phase == "committed" and os.path.realpath(gitdir) == os.path.realpath(common):
        started = [(r, n) for _o, n, r in lines if r in ("ORIG_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD")
                   and not is_zero(n)]
        if started:
            who = caller_identity(conf, chain)
            try:
                write_json_atomic(files.in_progress, {
                    "at": iso(), "epoch": now(), "repository": conf.get("REPO") or common,
                    "refs": {r: n for r, n in started},
                    "owner": {k: who.get(k) for k in ("kind", "pid", "start", "session_id", "entrypoint")
                              if k in who} if who else None})
            except OSError:
                pass
    return 0


# ---------------------------------------------------------------------------
# whose unfinished merge is this? (e7 item 2, G6)
# ---------------------------------------------------------------------------

def merge_owner(conf, paths, chain=None, consider_lease=True):
    """The ownership verdict for an in-progress merge, cherry-pick or revert in
    a fenced main checkout, as a dict:
      verdict  mine | other-holder | refused-mine | refused-other |
               owner-ended | owner-live | unknown | none
      label, head, kind
    With consider_lease (the turn-end guard's question), a live lease decides
    first: `mine` when this session holds it, `other-holder` otherwise. Without
    it (abort-orphan's question, asked BY the lease holder), only the positive
    records decide who started it (G6): a refusal carrying this head, or the
    in-progress record the launcher wrote at the start."""
    chain = ancestors() if chain is None else chain
    files = Files(conf)
    gitdir = paths["gitdir"]
    kinds = [n for n, _w in in_progress_files(gitdir) if n in ("MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD")]
    rc, out, _ = git(paths["main"], "diff", "--name-only", "--diff-filter=U")
    unmerged = rc == 0 and bool(out.strip())
    if not kinds and not unmerged:
        return {"verdict": "none"}
    kind = kinds[0] if kinds else ""
    head = head_value(gitdir, kind) if kind else ""
    me = caller_identity(conf, chain)
    if consider_lease:
        lease = read_lease(files)
        if lease_state(lease) in ("live", "expired"):
            h = lease["holder"]
            if is_ancestor(h["pid"], h["start"], chain):
                return {"verdict": "mine", "kind": kind, "head": head, "label": holder_label(h)}
            return {"verdict": "other-holder", "kind": kind, "head": head, "label": holder_label(h)}
    rec = read_json(files.in_progress)
    owner = (rec or {}).get("owner")
    starter = owner if owner and _record_matches(rec, kind, head, gitdir) else None
    # A refusal describes THIS operation only if it came after the head file
    # appeared (_refusals_of). And it ORPHANS the operation only when it really
    # does (Fix 3): an ORIG_HEAD refusal, whose writer had already rewritten the
    # index (Fix 1's case), or a refusal of the operation's own recorded
    # starter, which cannot conclude what it started. Another writer's refused
    # `git commit` during a healthy, resolved merge by the holder orphans nothing.
    for ref_rec in _refusals_of(files, head, _head_mtime(gitdir, kind)):
        caller = ref_rec.get("caller") or {}
        rewrote = ref_rec.get("ref") == "ORIG_HEAD"
        if not rewrote and not same_holder(caller, starter):
            continue
        mine = bool(me) and caller.get("pid") == me["pid"] and caller.get("start") == me["start"]
        return {"verdict": "refused-mine" if mine else "refused-other", "kind": kind, "head": head,
                "label": holder_label(caller) if caller else "an unknown caller",
                "rewrote": _writer_text(ref_rec) if rewrote else ""}
    if starter:
        if not alive(owner.get("pid"), owner.get("start", -1)):
            return {"verdict": "owner-ended", "kind": kind, "head": head, "label": holder_label(owner)}
        if me and same_holder(me, owner):
            return {"verdict": "mine", "kind": kind, "head": head, "label": holder_label(owner)}
        return {"verdict": "owner-live", "kind": kind, "head": head, "label": holder_label(owner)}
    return {"verdict": "unknown", "kind": kind, "head": head, "label": "an unrecorded starter"}


def _record_matches(rec, kind, head, gitdir):
    """Does the in-progress record describe THIS unfinished operation?

    A cherry-pick or revert records its own head, so the match is exact. A merge
    writes no MERGE_HEAD ref (measured, both gits: MERGE_HEAD is a plain file),
    only ORIG_HEAD a moment before, so a merge matches an ORIG_HEAD record
    written within 120 s before MERGE_HEAD appeared. An older record is some
    earlier reset or merge, and matching it would name the wrong starter."""
    refs = rec.get("refs") or {}
    appeared = _head_mtime(gitdir, kind)
    try:
        written = float(rec.get("epoch"))
    except (TypeError, ValueError):
        return False
    if appeared is None:
        return False
    if kind in ("CHERRY_PICK_HEAD", "REVERT_HEAD"):
        # The record is written in the same second the head file appears.
        return bool(head) and refs.get(kind) == head and abs(appeared - written) <= 5.0
    if kind == "MERGE_HEAD" and "ORIG_HEAD" in refs:
        return -5.0 <= appeared - written <= 120.0
    return False


def _head_mtime(gitdir, kind):
    try:
        return os.path.getmtime(os.path.join(gitdir, kind)) if kind else None
    except OSError:
        return None


def _read_refusals(files):
    out = []
    try:
        with open(files.refusals, encoding="utf-8") as fh:
            for line in fh.readlines()[-500:]:
                try:
                    out.append(json.loads(line))
                except ValueError:
                    continue
    except OSError:
        pass
    return out


def cmd_merge_owner(argv):
    repo = argv[argv.index("--repo") + 1] if "--repo" in argv else os.getcwd()
    conf, paths = launcher_for(repo)
    if not fenced(conf) or not paths or not paths["main"]:
        print(json.dumps({"verdict": "off"}))
        return 0
    main_paths = repo_paths(paths["main"])
    print(json.dumps(merge_owner(conf, main_paths)))
    return 0


# ---------------------------------------------------------------------------
# the lease commands (e1, F1, F3, G5, G7, G10)
# ---------------------------------------------------------------------------

def _engine_root():
    return os.path.realpath(os.path.join(os.path.dirname(os.path.realpath(__file__)), "..", ".."))


def _workspaces():
    sys.path.insert(0, os.path.join(_engine_root(), "mega-lander"))
    try:
        import workspaces as W          # noqa: E402  (engine-only import)
    finally:
        sys.path.pop(0)
    return W


def _default_home():
    return _workspaces().land_locks_dir()


def keyed_paths(main_checkout):
    """The lease's keying rule is workspaces.py's, and only workspaces.py's:
    land_lock_path(), with the `.lock` suffix. Imported, never copied."""
    lock = _workspaces().land_lock_path(main_checkout)
    return os.path.dirname(lock), os.path.basename(lock)[:-len(".lock")]


def _parse(argv, flags, valued):
    opts = {}
    i = 0
    while i < len(argv):
        a = argv[i]
        if a in valued and i + 1 < len(argv):
            opts[a] = argv[i + 1]
            i += 2
        elif a in flags:
            opts[a] = True
            i += 1
        else:
            raise SystemExit("land-lease: unknown argument %r" % a)
    return opts


def _say(text):
    sys.stdout.write(text.rstrip("\n") + "\n")
    sys.stdout.flush()


def _lease_context(opts):
    repo = opts.get("--repo") or os.getcwd()
    conf, paths = launcher_for(repo)
    if not paths or not paths["main"]:
        raise SystemExit("land-lease: %s is not inside a repository with a main checkout" % repo)
    if not fenced(conf):
        return None, None, None, paths
    try:
        home, key = keyed_paths(paths["main"])
    except Exception as error:    # noqa: BLE001  (any failure here is a refusal, said out loud)
        raise SystemExit("land-lease: cannot verify the lease home (%s); refusing rather than guessing" % error)
    if os.path.realpath(home) != os.path.realpath(conf.get("HOME", "")) or key != conf.get("KEY"):
        raise SystemExit(
            "land-lease: REFUSED (G10). This command resolves the lease to %s/%s, but the fence installed in "
            "%s reads %s/%s. Two callers with different homes would each see their own lease as free. Run it "
            "without RICHOS_LAND_LOCKS_DIR or CLAUDE_CONFIG_DIR overriding the home the installer used."
            % (home, key, paths["main"], conf.get("HOME"), conf.get("KEY")))
    return conf, Files(conf), repo, repo_paths(paths["main"])


def _identify(opts, conf, taking=False):
    """The caller as a lease holder, or {"error": ...}. With `taking` (acquire and
    takeover, never release), a holder the operator claim bars is refused too."""
    who = _identify_holder(opts, conf)
    if taking and who and "error" not in who:
        barred = _claim_bars(who)
        if barred:
            return {"error": barred}
    return who


def _claim_bars(holder):
    """r3 (e) item 7, asked by the lease itself: while the RichOS app runs his team
    (its claim is live, or the claim is unreadable), his terminal takes no land
    lease, whether a Bash command or a program asks (commit-ceo-inputs.py does).
    The rule lives in operator_leads.lease_refusal; it is imported only here, from
    the engine, never from the copy of this file a launcher runs."""
    if holder.get("kind") != "claude":
        return ""
    sys.path.insert(0, os.path.dirname(os.path.realpath(__file__)))
    try:
        import operator_leads   # noqa: E402  (engine-only import)
    finally:
        sys.path.pop(0)
    return operator_leads.lease_refusal(holder)


def _identify_holder(opts, conf):
    chain = ancestors()
    kind = opts.get("--holder")
    if kind:
        who = declared_holder(kind, conf, chain, opts.get("--holder-pid"))
    else:
        if opts.get("--holder-pid"):
            return {"error": "--holder-pid needs --holder <kind>"}
        who = claude_session(chain)
        if who is None:
            who = {"error": "this command runs under no Claude session with a session record, and no "
                            "--holder <kind> was given. A Claude session is found from its own ancestry; "
                            "a declared non-Claude holder (for example Codex) passes --holder <kind>."}
    return who


def _state_text(paths):
    ok, reasons, _dirty = at_rest(paths["main"], paths["gitdir"])
    return "at rest" if ok else "; ".join(reasons)


def _wait_budget(opts):
    raw = opts.get("--wait") or (os.environ.get("LAND_LEASE_WAIT") or "").strip()
    try:
        wait = float(raw) if raw else DEFAULT_WAIT
    except ValueError:
        raise SystemExit("land-lease: the wait must be a number of seconds")
    if wait < 0 or wait > MAX_WAIT:
        raise SystemExit("land-lease: REFUSED: a wait of %s s is above the %d s ceiling. One call, its start-up and "
                         "its report must fit inside the 600-second command cap; run the command again instead "
                         "of waiting longer in one call." % (raw, int(MAX_WAIT)))
    return wait


def _ttl():
    raw = (os.environ.get("LAND_LEASE_TTL") or "").strip()
    try:
        return float(raw) if raw else DEFAULT_TTL
    except ValueError:
        return DEFAULT_TTL


class _Flock(object):
    def __init__(self, path):
        self.path = path
        self.fh = None

    def try_take(self):
        os.makedirs(os.path.dirname(self.path), mode=0o700, exist_ok=True)
        if self.fh is None:
            self.fh = os.fdopen(os.open(self.path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600), "r+")
        try:
            fcntl.flock(self.fh, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return True
        except OSError as error:
            if error.errno in (errno.EAGAIN, errno.EACCES):
                return False
            raise

    def release(self):
        if self.fh is not None:
            try:
                fcntl.flock(self.fh, fcntl.LOCK_UN)
            finally:
                self.fh.close()
                self.fh = None


def _new_lease(holder, repo_main, previous=None, preserved=None, renewing=None):
    """`renewing`: the same holder's lease being renewed, whose running land
    commands are carried over (a renewal from a second call must not drop the
    first call's land that is still running)."""
    t = now()
    lease = {"schema": 1, "repository": repo_main, "holder": holder, "acquired": iso(t),
             "acquired_epoch": t, "renewed_epoch": t, "expires": None,
             "land_processes": _land_processes(renewing, holder)}
    if holder.get("kind") != "claude":
        lease["expires"] = t + _ttl()
    if previous:
        lease["took_from"] = previous.get("holder")
    if preserved:
        lease["preserved"] = preserved
    return lease


def _held_line(lease, state, paths, wait, allow_takeover, repo):
    h = lease.get("holder") or {}
    age = age_text(now() - float(lease.get("acquired_epoch") or now()))
    tail = {"dead": ", and its holder has ended", "expired": ", held past its time"}.get(state, "")
    line = ("land-lease: HELD. %s holds the land lease for %s, taken %s ago%s; %s. Waited %d s. "
            "Run this command again." % (holder_label(h), repo, age, tail, _state_text(paths), int(wait)))
    if allow_takeover:
        line += (" Its holder can no longer finish it, so it can also be taken over: "
                 "land-lease.sh takeover --repo %s (the state is preserved first)." % repo)
    return line


def cmd_acquire(opts):
    conf, files, repo, paths = _lease_context(opts)
    if conf is None:
        _say("land-lease: operator fences are off for %s. There is no lease to take and none is needed."
             % paths["main"])
        return 0
    holder = _identify(opts, conf, taking=True)
    if not holder or "error" in holder:
        _say("land-lease: REFUSED. %s" % (holder or {}).get("error", "no holder could be identified"))
        return 2
    wait = _wait_budget(opts)
    deadline = time.monotonic() + wait
    flock = _Flock(files.lock)
    last = None
    try:
        while True:
            if flock.try_take():
                try:
                    lease = read_lease(files)
                    state = lease_state(lease)
                    if state == "none" or same_holder(lease["holder"], holder):
                        renewed = state != "none"
                        new = _new_lease(holder, paths["main"], renewing=lease if renewed else None)
                        if renewed:
                            new["acquired"] = lease.get("acquired")
                            new["acquired_epoch"] = lease.get("acquired_epoch")
                        write_json_atomic(files.lease, new)
                        _say("land-lease: %s the land lease for %s as %s."
                             % ("RENEWED" if renewed else "ACQUIRED", paths["main"], holder_label(holder)))
                        _report_state(conf, files, paths)
                        return 0
                    if state in ("dead", "expired"):
                        rest, _r, _d = at_rest(paths["main"], paths["gitdir"])
                        if rest:
                            write_json_atomic(files.lease, _new_lease(holder, paths["main"], previous=lease))
                            _say("land-lease: ACQUIRED the land lease for %s as %s (the previous holder, %s, %s, "
                                 "and the repository was at rest)."
                                 % (paths["main"], holder_label(holder), holder_label(lease["holder"]),
                                    "had ended" if state == "dead" else "was past its time"))
                            return 0
                    last = (lease, state)
                finally:
                    flock.release()
            if time.monotonic() >= deadline:
                break
            time.sleep(min(2.0, max(0.05, deadline - time.monotonic())))
    finally:
        flock.release()
    if last is None:
        _say("land-lease: HELD. Another land of %s (the engine's own integrate) holds the land lock; waited %d s. "
             "Run this command again." % (paths["main"], int(wait)))
        return EXIT_HELD
    lease, state = last
    _say(_held_line(lease, state, paths, wait, state in ("dead", "expired"), paths["main"]))
    return EXIT_HELD


def _report_in_progress(conf, paths):
    verdict = merge_owner(conf, paths, consider_lease=False)
    if verdict.get("verdict") in (None, "none", "mine"):
        return
    head = verdict.get("head") or "(no head recorded)"
    what = {"MERGE_HEAD": "a merge", "CHERRY_PICK_HEAD": "a cherry-pick",
            "REVERT_HEAD": "a revert"}.get(verdict.get("kind"), "an unfinished operation")
    if verdict["verdict"] in ("owner-ended", "refused-mine", "refused-other"):
        if verdict["verdict"] == "owner-ended":
            why = "its owner has ended"
        elif verdict.get("rewrote"):
            why = ("%s had already rewritten its index before the fence refused it, so its staged resolution "
                   "is gone. Do not commit on top of it" % verdict["rewrote"])
        else:
            why = "the fence refused its own starter's move"
        _say("land-lease: %s is in progress in %s (%s), started by %s, and it can never finish: %s. Preserve and "
             "abort it with: land-lease.sh abort-orphan --repo %s, and name %s in your land's report."
             % (what, paths["main"], head, verdict.get("label"), why, paths["main"], head))
    else:
        _say("land-lease: %s is in progress in %s (%s), started by %s, who may still be finishing it. "
             "Leave it: never abort a merge you did not start." % (what, paths["main"], head, verdict.get("label")))


def _main_moved_at(common):
    """When main last moved: its reflog's mtime, else the loose ref's, else packed-refs'."""
    for rel in (("logs", "refs", "heads", "main"), ("refs", "heads", "main"), ("packed-refs",)):
        try:
            return os.path.getmtime(os.path.join(common, *rel))
        except OSError:
            continue
    return 0.0


def residue_notes(files, paths):
    """Fix 2 point 4: each refused writer that rewrote the index and tree first,
    newer than main's last move, whose paths are still dirty in the checkout.
    Named by acquire, renew, takeover and status; `acquire` of a free lease
    checked nothing before (Frank's re-check §2)."""
    moved = _main_moved_at(paths["common"])
    recent = []
    for rec in reversed(_read_refusals(files)):
        try:
            newer = float(rec.get("epoch")) > moved
        except (TypeError, ValueError):
            newer = False
        if rec.get("repository") == files.repo and rec.get("residue") and newer:
            recent.append(rec)
    if not recent:
        return []
    _ok, _reasons, dirty = at_rest(paths["main"], paths["gitdir"])
    dirty = set(dirty)
    notes = []
    for rec in recent:
        still = [e["path"] for e in rec.get("residue_paths") or [] if e.get("path") in dirty]
        if still:
            notes.append("land-lease: %s rewrote the index and the working tree of %s before the fence refused it; "
                         "HEAD and main did not move. These paths still carry it: %s. If you did not make these "
                         "changes, stop and report them, and never commit on top of them. Restore them with: "
                         "git restore --source=HEAD --staged --worktree -- %s"
                         % (_writer_text(rec), paths["main"], ", ".join(still[:12]),
                            " ".join(shlex.quote(p) for p in still[:12])))
    return notes


def _report_state(conf, files, paths):
    _report_in_progress(conf, paths)
    for note in residue_notes(files, paths):
        _say(note)


def cmd_release(opts):
    conf, files, repo, paths = _lease_context(opts)
    if conf is None:
        _say("land-lease: operator fences are off for %s. There is no lease to release." % paths["main"])
        return 0
    holder = _identify(opts, conf)
    if not holder or "error" in holder:
        _say("land-lease: REFUSED. %s" % (holder or {}).get("error", "no holder could be identified"))
        return 2
    flock = _Flock(files.lock)
    deadline = time.monotonic() + 30
    while not flock.try_take():
        if time.monotonic() >= deadline:
            _say("land-lease: the land lock stayed busy for 30 s; the lease was not released. Run it again.")
            return EXIT_HELD
        time.sleep(0.2)
    try:
        lease = read_lease(files)
        if not lease:
            _say("land-lease: no lease is held for %s." % paths["main"])
            return 0
        if not same_holder(lease.get("holder"), holder):
            _say("land-lease: the lease for %s is held by %s, not by this caller. It was left untouched."
                 % (paths["main"], holder_label(lease.get("holder"))))
            return 1
        os.unlink(files.lease)
        _say("land-lease: RELEASED the land lease for %s." % paths["main"])
        return 0
    finally:
        flock.release()


def cmd_status(opts):
    repo = opts.get("--repo") or os.getcwd()
    conf, paths = launcher_for(repo)
    if not paths:
        _say("land-lease: %s is not a repository" % repo)
        return 2
    if not fenced(conf):
        _say("land-lease: operator fences are %s for %s." % ("off" if conf else "not installed", paths["main"]))
        return 0
    files = Files(conf)
    lease = read_lease(files)
    state = lease_state(lease)
    main_paths = repo_paths(paths["main"])
    _say("land-lease: repository %s; lease %s%s; %s." % (
        paths["main"], state,
        "" if state == "none" else ", held by %s for %s%s" % (
            holder_label(lease["holder"]), age_text(now() - float(lease.get("acquired_epoch") or now())),
            "" if not lease.get("expires") else ", expires in %s" % age_text(float(lease["expires"]) - now())
            if float(lease["expires"]) > now() else ", past its time"),
        _state_text(main_paths)))
    for note in residue_notes(files, main_paths):
        _say(note)
    return 0


def cmd_takeover(opts):
    """G5: allowed only when the recorded holder is dead, or a non-Claude lease
    is past its time. The state is preserved BEFORE the lease changes hands."""
    conf, files, repo, paths = _lease_context(opts)
    if conf is None:
        _say("land-lease: operator fences are off for %s. There is nothing to take over." % paths["main"])
        return 0
    holder = _identify(opts, conf, taking=True)
    if not holder or "error" in holder:
        _say("land-lease: REFUSED. %s" % (holder or {}).get("error", "no holder could be identified"))
        return 2
    flock = _Flock(files.lock)
    deadline = time.monotonic() + 30
    while not flock.try_take():
        if time.monotonic() >= deadline:
            _say("land-lease: the land lock stayed busy for 30 s. Run it again.")
            return EXIT_HELD
        time.sleep(0.2)
    try:
        lease = read_lease(files)
        state = lease_state(lease)
        if state == "none":
            _say("land-lease: no lease is held for %s; use acquire." % paths["main"])
            return 1
        if state == "live":
            _say("land-lease: REFUSED. %s is alive and within its time, so its lease cannot be taken over. "
                 "Wait for it: land-lease.sh acquire --repo %s." % (holder_label(lease["holder"]), paths["main"]))
            return 1
        kept = preserve(files, paths, "takeover of a lease whose holder %s" % (
            "has ended" if state == "dead" else "is past its time"))
        missing = preservation_problems(kept)
        if missing:
            # The promise is "preserved BEFORE the lease changes hands", so a
            # preservation that failed keeps the lease where it is. A tree at
            # rest needs no takeover (acquire takes a dead holder's lease), so
            # this refuses only where the preservation is what matters.
            _say("land-lease: REFUSED. The land lease for %s was NOT taken over: the repository's state could not "
                 "be preserved first (%s). What could be kept is in %s. Fix the cause and run the takeover again."
                 % (paths["main"], "; ".join(missing), kept))
            return 1
        write_json_atomic(files.lease, _new_lease(holder, paths["main"], previous=lease, preserved=kept))
        _say("land-lease: TOOK OVER the land lease for %s from %s (%s). The repository's state was preserved "
             "first, in %s; name that directory in your land's report. Nothing in the tree was changed."
             % (paths["main"], holder_label(lease["holder"]),
                "its holder had ended" if state == "dead" else "it was past its time", kept))
        _report_state(conf, files, paths)
        return 0
    finally:
        flock.release()


def cmd_abort_orphan(opts):
    """e7 item 3 with G5 and G6: abort a merge, cherry-pick or revert only when
    it can never finish (its recorded owner has ended, or the fence refused it),
    only by the lease holder, and only after preserving it."""
    conf, files, repo, paths = _lease_context(opts)
    if conf is None:
        _say("land-lease: operator fences are off for %s. Nothing is aborted by this command." % paths["main"])
        return 0
    ok, _lease = authorized(files)
    if not ok:
        _say("land-lease: REFUSED. Only the holder of the land lease may abort an orphaned merge. Take it first: "
             "land-lease.sh acquire --repo %s" % paths["main"])
        return 1
    verdict = merge_owner(conf, paths, consider_lease=False)
    v = verdict.get("verdict")
    if v == "none":
        _say("land-lease: nothing is in progress in %s." % paths["main"])
        return 0
    if v not in ("owner-ended", "refused-mine", "refused-other"):
        _say("land-lease: REFUSED. The %s in %s was started by %s (%s). Nothing was touched: never abort a merge "
             "you did not start." % (verdict.get("kind") or "operation", paths["main"], verdict.get("label"), v))
        return 1
    kept = preserve(files, paths, "abort of an orphaned %s (%s)" % (verdict.get("kind"), v))
    missing = preservation_problems(kept)
    if missing:
        # The abort discards the operation's resolutions, and the preservation
        # is the only copy it leaves. Without a complete one, nothing is aborted.
        _say("land-lease: REFUSED. Nothing was aborted: the state of the %s in %s could not be preserved first "
             "(%s). What could be kept is in %s. Fix the cause and run abort-orphan again."
             % (verdict.get("kind") or "operation", paths["main"], "; ".join(missing), kept))
        return 1
    sub = {"MERGE_HEAD": ["merge", "--abort"], "CHERRY_PICK_HEAD": ["cherry-pick", "--abort"],
           "REVERT_HEAD": ["revert", "--abort"]}.get(verdict.get("kind"))
    if not sub:
        _say("land-lease: the state was preserved in %s, but no abort is defined for it." % kept)
        return 1
    rc, out, err = git(paths["main"], *sub)
    if rc != 0:
        _say("land-lease: the abort failed (%s). The state is preserved in %s." % ((err or out).strip()[:300], kept))
        return 1
    try:
        os.unlink(files.in_progress)
    except OSError:
        pass
    _say("land-lease: ABORTED the orphaned %s in %s. Its head was %s: name it in your land's report so the work "
         "can be landed again. The state before the abort is preserved in %s."
         % (verdict.get("kind"), paths["main"], verdict.get("head"), kept))
    return 0


def cmd_lease(argv):
    if not argv:
        _say("usage: land-lease.sh acquire|release|status|takeover|abort-orphan --repo <path> "
             "[--holder <kind> [--holder-pid <pid>]] [--wait <seconds>]")
        return 2
    sub, rest = argv[0], argv[1:]
    opts = _parse(rest, set(), {"--repo", "--holder", "--holder-pid", "--wait"})
    table = {"acquire": cmd_acquire, "release": cmd_release, "status": cmd_status,
             "takeover": cmd_takeover, "abort-orphan": cmd_abort_orphan}
    if sub not in table:
        _say("land-lease: unknown command %r" % sub)
        return 2
    return table[sub](opts)


# ---------------------------------------------------------------------------
# the turn-end release (e6, F3, G5 point 3)
# ---------------------------------------------------------------------------

def cmd_turn_end(argv):
    """Stop hook body. For every lease THIS session holds: at rest, release it;
    otherwise keep it and say so, once, naming what is unfinished and which
    paths are dirty. Never blocks. A lease it does not hold is never touched."""
    try:
        home = _default_home()
    except Exception:              # noqa: BLE001  a Stop hook never fails the turn
        return 0
    try:
        names = [n for n in os.listdir(home) if n.endswith(".lease")]
    except OSError:
        return 0
    if not names:
        return 0
    chain = ancestors()
    notes = []
    for name in sorted(names):
        lease = read_json(os.path.join(home, name))
        if not lease or not isinstance(lease.get("holder"), dict):
            continue
        h = lease["holder"]
        if not is_ancestor(h.get("pid", -1), h.get("start", -1), chain):
            continue
        conf, paths = launcher_for(lease.get("repository") or "/nonexistent")
        if not fenced(conf) or not paths:
            continue
        files = Files(conf)
        if os.path.realpath(files.lease) != os.path.realpath(os.path.join(home, name)):
            continue
        main_paths = repo_paths(paths["main"])
        rest, reasons, dirty = at_rest(main_paths["main"], main_paths["gitdir"])
        # A land this session started (a background script, a merge whose gate
        # runs for minutes) is not at rest however the repository looks between
        # two of its steps (2026-10-11, see land_process). The lease read above
        # is only a snapshot: a land of the same holder can renew it, adding
        # its command, while the repository is checked (review
        # rv-20261011T205852Z-54ae8c6a-eec1). So the release decides from the
        # running lands of the lease it is about to delete, read under the lock.
        lands = []
        if rest:
            flock = _Flock(files.lock)
            if flock.try_take():
                try:
                    current = read_lease(files)
                    if current and same_holder(current.get("holder"), h):
                        lands = running_lands(current)
                        if not lands:
                            os.unlink(files.lease)
                finally:
                    flock.release()
            if not lands:
                continue
        else:
            lands = running_lands(lease)
        if lands:
            reasons = ["its land command (process %s) is still running"
                       % ", ".join(str(e["pid"]) for e in lands)] + reasons
        notes.append("Your land in %s from %s is still holding the lock after %s: %s. Other conversations' "
                     "lands wait until it finishes. If some of those paths are not yours, another writer "
                     "dirtied the tree; say so rather than finishing their work."
                     % (paths["main"], holder_label(h),
                        age_text(now() - float(lease.get("acquired_epoch") or now())), "; ".join(reasons)))
    if notes:
        print(json.dumps({"systemMessage": " ".join(notes)}))
    return 0


# ---------------------------------------------------------------------------
# the early Bash check (e7 item 1, G1 points 2 and 3)
# ---------------------------------------------------------------------------

# The verbs the text check refuses in a fenced main checkout without the lease.
# G1 kept cherry-pick, revert, am, rebase, stash and commit here and moved merge,
# pull and reset to the fence alone. MEASURED 2026-09-24 on Homebrew git 2.52.0
# and Apple git 2.50.1 (scratch repositories, ORIG_HEAD refused): merge, merge
# --no-ff and pull ARE stopped before the tree changes, so they stay with the
# fence. But reset (--hard, --keep, --merge, --mixed) exits 128 with main unmoved
# AFTER rewriting the index and tree, and merge --abort (a reset --merge inside)
# exits 128 AFTER discarding a staged resolution, leaving MERGE_HEAD behind. So
# `reset` and `merge --abort`/`--quit` are back in this list, as r3 e7 first had
# them. As-built deviation from G1, recorded and escalated.
TEXT_VERBS = ("cherry-pick", "revert", "am", "rebase", "stash", "commit", "reset", "merge")
_SEP = re.compile(r"&&|\|\||;|\||\n")


def git_invocations(command, cwd):
    """[(directory, subcommand, args)] for each `git` call in a shell command,
    with `cd <dir>` tracked across segments and `-C <dir>` honored. A text
    match, stated as such: `sh -c`, scripts and variables are not seen."""
    out = []
    here = cwd
    for segment in _SEP.split(command or ""):
        try:
            words = shlex.split(segment, comments=True)
        except ValueError:
            continue
        while words and re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", words[0]):
            words = words[1:]
        if not words:
            continue
        if words[0] == "cd":
            target = words[1] if len(words) > 1 else os.path.expanduser("~")
            here = os.path.normpath(os.path.join(here, os.path.expanduser(target)))
            continue
        if os.path.basename(words[0]) != "git":
            continue
        d, i = here, 1
        while i < len(words):
            a = words[i]
            if a == "-C" and i + 1 < len(words):
                d = os.path.normpath(os.path.join(d, os.path.expanduser(words[i + 1])))
                i += 2
            elif a in ("-c", "--git-dir", "--work-tree", "--namespace") and i + 1 < len(words):
                i += 2
            elif a.startswith("-"):
                i += 1
            else:
                break
        if i < len(words):
            out.append((d, words[i], words[i + 1:]))
    return out


def early_check(payload):
    """(exit code, message). 0 = allow; 2 = refuse."""
    tool_input = payload.get("tool_input") or {}
    command = tool_input.get("command") or ""
    cwd = payload.get("cwd") or os.getcwd()
    chain = None
    for directory, sub, args in git_invocations(command, cwd):
        if sub not in TEXT_VERBS:
            continue
        if sub == "stash" and args[:1] in (["list"], ["show"]):
            continue
        if sub == "merge" and not any(a in ("--abort", "--quit") for a in args):
            continue            # a merge itself is stopped cleanly by the fence (G1, measured)
        if not os.path.isdir(directory):
            continue
        paths = repo_paths(directory)
        # G1 point 3: the target's OWN checkout decides, never a path prefix. A
        # native worktree under .claude/worktrees/ has its own top level.
        if not paths or not paths["main"] or paths["gitdir"] != paths["common"]:
            continue
        conf = read_launcher(os.path.join(paths["common"], "hooks", "reference-transaction"))
        if not fenced(conf):
            continue
        chain = ancestors() if chain is None else chain
        ok, lease = authorized(Files(conf), chain)
        if ok:
            continue
        engine = conf.get("ENGINE") or _engine_root()
        held = "the lease is free" if lease_state(lease) == "none" else \
            "the lease is held by %s" % holder_label((lease or {}).get("holder"))
        return 2, "\n".join([
            "=== OPERATOR FENCE: `git %s` in the main checkout %s needs the land lease ===" % (sub, paths["main"]),
            "  This session does not hold it (%s). The shared checkout is written only by the" % held,
            "  lease holder, so another conversation's land is never interleaved with this command.",
            "  Take it first, then run the command again:",
            "      %s/scripts/land-lease.sh acquire --repo %s" % (engine, paths["main"]),
            "  Exit 75 means another holder is landing: run it again. Never abort a merge you did not start.",
            "  (Early check, a text match. The Git fence itself is the backstop.)"])
    return 0, ""


def cmd_early_check(argv):
    try:
        payload = json.loads(sys.stdin.read() or "{}")
    except ValueError:
        return 0
    if not isinstance(payload, dict) or payload.get("tool_name") not in (None, "Bash"):
        return 0
    try:
        rc, message = early_check(payload)
    except Exception:   # noqa: BLE001  the second line never fails a command; the Git fence decides
        return 0
    if rc:
        sys.stderr.write(message + "\n")
    return rc


# ---------------------------------------------------------------------------
# entry
# ---------------------------------------------------------------------------

def main(argv):
    if not argv:
        sys.stderr.write("usage: operator_fences.py fence|lease|merge-owner|turn-end|early-check|review-check|admin ...\n")
        return 2
    cmd, rest = argv[0], argv[1:]
    if cmd == "fence":
        return cmd_fence(rest)
    if cmd == "lease":
        return cmd_lease(rest)
    if cmd == "merge-owner":
        return cmd_merge_owner(rest)
    if cmd == "turn-end":
        try:
            return cmd_turn_end(rest)
        except Exception as error:   # noqa: BLE001  never fails the turn: the hook announces it
            sys.stderr.write("turn-end failed: %s\n" % error)
            return 3
    if cmd == "early-check":
        return cmd_early_check(rest)
    if cmd == "review-check":
        return cmd_review_check(rest)
    if cmd == "admin":
        sys.path.insert(0, os.path.dirname(os.path.realpath(__file__)))
        import operator_fences_admin as admin   # noqa: E402  (engine-only)
        return admin.main(rest)
    sys.stderr.write("operator_fences.py: unknown command %r\n" % cmd)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
