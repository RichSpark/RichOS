#!/usr/bin/env python3
"""codex_watch.py: CODEX'S MESSAGES TO THE LEAD WAKE THE LEAD, WITH NOBODY STARTING ANYTHING.

===========================================================================
WHAT THIS EXISTS FOR
===========================================================================
Codex writes to the lead through append-only files (each entry starts
`## <UTC time> — <subject>`): `to-rich.md` in each channel folder, beside the
`to-codex.md` the lead answers in. Which folders are channels is the user's
setting, CODEX_CHANNELS in orchestration.config (colon-separated folders, `~`
for the home folder; an empty value watches none). Without it, the
conventional folders:

    ~/.richos-coordination/rich-codex
    ~/.richos-coordination/rich-codex-questions

The lead was supposed to start a background watcher on them at every session
start. On 2026-10-05 it did not, and Codex's 08:11Z "READY TO LAND" entry for
codex/split-proof-evidence (3ec820348) sat unlanded for about five hours,
until the CEO asked. A watcher that must be started by hand is the same defect
one step later, so this one is started by Claude Code itself with every
interactive session: the engine's plugin monitor (monitors/monitors.json), as
quota-watch and stall-watch are.

===========================================================================
WHAT IT DOES
===========================================================================
Every POLL_SECONDS it looks at each channel's size. New bytes are told once
the file has stopped growing for one look (an entry written in two parts is
still one entry), and only up to the last complete line. The new text is split
at its `## ` headers and each entry is printed as ONE block, header plus EVERY
line of it (the old hand watcher printed only the last entry and missed two
requests on 2026-09-27). Each printed block is one notification to the lead.

What was told is kept per session, so a restarted monitor repeats nothing, and
in one shared record (last-told.json), so a NEW session starts where the last
one stopped: an entry written while no session was watching is told at the
next session's first looks. The very first start on a machine starts from the
end of each file; the backlog is not replayed.

It only reads and prints. It never writes to a channel, answers, lands or
messages anything.

===========================================================================
COMMANDS (codex-watch.sh passes --engine-root and --config)
===========================================================================
  --monitor  the plugin monitor's body: runs for the whole session, one per
             session, and ends with its session
  --tick     one look against the session's state, printing what the monitor
             would print (codex-watch.test.sh drives it)
  --host-json  with --monitor: the RichOS app's own child (richos-core
             codex_watch.rs), from the app's launch to its quit: each block as
             one JSON line, which the app gives to Rich in the conversation on
             screen (host_child.py). It needs no --config: the app names the
             channels the user's setting gives in CODEX_WATCH_CHANNELS and its
             own state folder in CODEX_WATCH_STATE_DIR

Settings: CODEX_WATCH_CHANNELS (colon-separated to-rich.md files; the app, and
tests), CODEX_WATCH_STATE_DIR, CODEX_WATCH_POLL_SECONDS (tests only).
"""

import argparse
import os
import shutil
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import host_child  # noqa: E402  (sibling in scripts/lib: the app host's child)
import stall_watch  # noqa: E402  (sibling in scripts/lib: session, lock and JSON helpers)

POLL_SECONDS = 15
STATE_KEEP_SECONDS = 2 * 86400
CONFIG_KEY = "CODEX_CHANNELS"
# The conventional channel folders, used when the user's orchestration.config sets no
# CODEX_CHANNELS. The app's default is the same value (richos-core codex_watch.rs, whose
# test holds the two equal).
DEFAULT_CHANNELS = "~/.richos-coordination/rich-codex:~/.richos-coordination/rich-codex-questions"


def configured_folders(config):
    """The value of the last CODEX_CHANNELS line in `config`, as written (quotes removed), or
    None when the file sets none or cannot be read. Read as text, never executed."""
    value = None
    try:
        with open(config, "r", encoding="utf-8") as fh:
            lines = fh.read().splitlines()
    except (OSError, ValueError):
        return None
    for line in lines:
        line = line.strip()
        if line.startswith("export "):
            line = line[len("export "):].lstrip()
        key, sep, rest = line.partition("=")
        if sep and key == CONFIG_KEY:
            rest = rest.split(" #", 1)[0].strip()
            if len(rest) >= 2 and rest[0] == rest[-1] and rest[0] in "\"'":
                rest = rest[1:-1]
            value = rest
    return value


def channels(config=""):
    """The to-rich.md files watched: CODEX_WATCH_CHANNELS when set (files), else `to-rich.md`
    in each folder of the user's CODEX_CHANNELS, else in each conventional folder."""
    raw = (os.environ.get("CODEX_WATCH_CHANNELS") or "").strip()
    if raw:
        paths = [p for p in raw.split(":") if p]
    else:
        folders = configured_folders(config) if config else None
        if folders is None:
            folders = DEFAULT_CHANNELS
        paths = [os.path.join(f.strip(), "to-rich.md") for f in folders.split(":") if f.strip()]
    return [os.path.abspath(os.path.expanduser(p)) for p in paths]


def channel_name(path):
    """`rich-codex/to-rich.md`: the directory says which channel it is."""
    return os.path.join(os.path.basename(os.path.dirname(path)), os.path.basename(path))


def state_root():
    return (os.environ.get("CODEX_WATCH_STATE_DIR") or "").strip() or os.path.join(
        os.path.expanduser("~"), ".claude", "state", "codex-watch")


def _stat(path):
    """(size, inode) or (None, None) when the file is not there."""
    try:
        st = os.stat(path)
    except OSError:
        return None, None
    return st.st_size, st.st_ino


def _start_offset(path, size, inode, told):
    """Where a session that has never looked at this file starts: where the
    last session's monitor stopped telling, when that is still this file; else
    the end of the file (the first start on a machine replays nothing)."""
    t = told.get(path) or {}
    if t.get("inode") == inode and isinstance(t.get("offset"), int) and 0 <= t["offset"] <= size:
        return t["offset"]
    return size


def _entries(lines):
    """Split lines at `## ` headers: [(header or None, [lines])]."""
    out = []
    for ln in lines:
        if ln.startswith("## ") or not out:
            out.append([ln])
        else:
            out[-1].append(ln)
    return [(grp[0] if grp[0].startswith("## ") else None, grp) for grp in out]


def look_one(path, st, told, now):
    """(blocks to print, new per-file state, offset told up to or None)."""
    size, inode = _stat(path)
    if size is None:
        return [], {"offset": 0, "inode": None, "seen": 0}, None
    if not st or st.get("inode") != inode:
        if st and st.get("inode") is None:
            # The file appeared while this session watched: all of it is new.
            return [], {"offset": 0, "inode": inode, "seen": size}, None
        off = _start_offset(path, size, inode, told)
        return [], {"offset": off, "inode": inode, "seen": size}, None
    off, seen = int(st.get("offset") or 0), int(st.get("seen") or 0)
    if size < off:
        # Shortened in place: nothing in it can be placed, so it is said once
        # and watching goes on from its new end.
        block = ["CODEX-WATCH %s: %s was shortened (%d bytes, was %d); watching from its new end. Read it: %s" % (
            stall_watch.hhmm(now), channel_name(path), size, off, path)]
        return [block], {"offset": size, "inode": inode, "seen": size}, size
    if size == off or size != seen:
        # Nothing new, or it is still growing: wait one look for it to settle.
        return [], {"offset": off, "inode": inode, "seen": size}, None
    try:
        with open(path, "rb") as fh:
            fh.seek(off)
            data = fh.read(size - off)
            # The line number of the first new line, for the lead to find it.
            fh.seek(0)
            first_line = fh.read(off).count(b"\n") + 1
    except OSError:
        return [], {"offset": off, "inode": inode, "seen": size}, None
    end = data.rfind(b"\n") + 1
    if end == 0:
        end = len(data)                    # one settled unterminated line: told, never held forever
    text = data[:end].decode("utf-8", "replace")
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    blocks = []
    n = first_line
    for header, grp in _entries(lines):
        what = ("a new entry" if header else "more lines of the entry above")
        blocks.append(["CODEX-WATCH %s: Codex wrote %s to you in %s (lines %d-%d of %s); every new line follows:" % (
            stall_watch.hhmm(now), what, channel_name(path), n, n + len(grp) - 1, path)] + grp)
        n += len(grp)
    new_off = off + end
    return blocks, {"offset": new_off, "inode": inode, "seen": size}, new_off


def session_dir(sid):
    d = os.path.join(state_root(), sid or "no-session")
    os.makedirs(d, exist_ok=True)
    return d


def tick(sd, now=None, out=None, json_lines=False, config=""):
    """One look: prints each due entry as its own block (as one JSON line with
    json_lines). Returns blocks printed."""
    out = out or sys.stdout
    now = time.time() if now is None else now
    path = os.path.join(sd, "offsets.json")
    told_path = os.path.join(state_root(), "last-told.json")
    state = stall_watch._read_json(path)
    told = stall_watch._read_json(told_path)
    printed = 0
    for ch in channels(config):
        blocks, new, told_to = look_one(ch, state.get(ch), told, now)
        for b in blocks:
            out.write((host_child.line("\n".join(b)) if json_lines else "\n".join(b)) + "\n")
            out.flush()
            printed += 1
        state[ch] = new
        if told_to is not None:
            told = stall_watch._read_json(told_path)
            told[ch] = {"offset": told_to, "inode": new["inode"], "at": int(now)}
            stall_watch._write_json(told_path, told)
    stall_watch._write_json(path, state)
    return printed


def prune_state(keep):
    """Section 54: a session's directory goes once nothing holds its lock and it
    has not been written for two days. Never the one in use."""
    root = state_root()
    try:
        names = os.listdir(root)
    except OSError:
        return
    now = time.time()
    for n in names:
        d = os.path.join(root, n)
        if d == keep or not os.path.isdir(d) or stall_watch.lock_held(os.path.join(d, "monitor.lock")):
            continue
        try:
            newest = max([os.path.getmtime(os.path.join(d, f)) for f in os.listdir(d)] + [os.path.getmtime(d)])
        except OSError:
            continue
        if now - newest > STATE_KEEP_SECONDS:
            shutil.rmtree(d, ignore_errors=True)


def session(engine_root):
    """(session id, claude pid) of the session this runs in, or ('', None)."""
    ws = stall_watch._load("codex_watch_workspaces", os.path.join(engine_root, "mega-lander", "workspaces.py"))
    if ws is None:
        return "", None
    try:
        sid = ws.current_session() or ""
        pid = ws.session_pid(sid) if sid else ws.session_pid()
    except Exception:  # noqa: BLE001: no session is "watch with no end", never a crash
        return "", None
    return sid, pid


def run_loop(engine_root, config):
    sid, spid = session(engine_root)
    sd = session_dir(sid)
    fd = stall_watch._try_lock(os.path.join(sd, "monitor.lock"))
    if fd is None:
        return 0                           # this session is already watched
    prune_state(sd)
    alive = stall_watch.session_alive_check(spid) if spid else None
    poll = stall_watch._env_float("CODEX_WATCH_POLL_SECONDS", POLL_SECONDS)
    try:
        while alive is None or alive():
            tick(sd, config=config)
            stall_watch._nap(poll, alive)
    finally:
        os.close(fd)
    return 0


def run_host_loop():
    """--monitor --host-json: the app's own child (richos-core codex_watch.rs). Its state is the
    app's own folder (CODEX_WATCH_STATE_DIR), so the app's first start replays no backlog, and a
    later start tells what was written while the app was closed."""
    alive = host_child.start()
    if not alive():
        return 0                           # its host is gone already
    sd = session_dir("app-host")
    fd = stall_watch._try_lock(os.path.join(sd, "monitor.lock"))
    if fd is None:
        return 0                           # another copy of the app already watches
    prune_state(sd)
    poll = stall_watch._env_float("CODEX_WATCH_POLL_SECONDS", POLL_SECONDS)
    try:
        while alive():
            tick(sd, json_lines=True)
            stall_watch._nap(poll, alive)
    finally:
        os.close(fd)
    return 0


def main(argv):
    ap = argparse.ArgumentParser(prog="codex-watch.sh")
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--monitor", action="store_true")
    g.add_argument("--tick", action="store_true")
    ap.add_argument("--config", default="")
    ap.add_argument("--engine-root", default="")
    ap.add_argument("--host-json", action="store_true",
                    help="with --monitor: the app host's child; each block as one JSON line")
    a = ap.parse_args(argv)
    if a.host_json and a.monitor:
        return run_host_loop()             # the app names its channels itself
    if not a.config:
        return 0  # a repository that never adopted the engine: nothing to watch, and nothing said
    engine_root = a.engine_root or os.path.dirname(os.path.dirname(HERE))
    if a.tick:
        sid, _pid = session(engine_root)
        tick(session_dir(sid), config=a.config)
        return 0
    return run_loop(engine_root, a.config)


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except KeyboardInterrupt:
        sys.exit(130)
