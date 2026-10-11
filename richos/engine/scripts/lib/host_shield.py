#!/usr/bin/env python3
"""host_shield.py: no test run can reach this Mac's power, sleep, lock or display commands.

THE RULE (ceo-decisions §65): no change to the host Mac's display, sleep, lock, session or
input state, ever.

WHY THIS EXISTS (2026-10-11, zach-opus-tooling1). A draft of guard-host-display-power.test.sh
read a fixture through a heredoc inside $(...); bash 3.2 mis-parsed it because of apostrophes
in the fixture, and a backticked power command in the fixture RAN ON THE HOST as the normal
user. It stopped only because a normal user is not allowed to halt the machine. No check
caught it: guard-host-display-power.sh reads the Bash tool call (`bash <suite>`), never what
a script runs inside itself.

WHAT IT DOES. Every test runner starts each suite through `host_shield.py run -- <argv>`:

  1. A shell suite's SOURCE is read first. A line that runs one of the commands below by an
     ABSOLUTE OR RELATIVE PATH (which bypasses PATH), or under sudo/doas (which resets PATH),
     in a form guard-host-display-power.sh refuses, refuses the suite before it runs. So does
     a path to an always-bad command anywhere on a line (an assignment like X=<path> too).
  2. The suite runs with `host-shield/` FIRST ON PATH. That directory holds one stand-in per
     command the guard judges. A stand-in judges its own argv with the GUARD'S OWN judge()
     (read out of guard-host-display-power.sh, so the two cannot drift): a harmless form
     (`pmset -g`, `caffeinate -is`, an osascript that touches nothing) is handed to the real
     command unchanged; a harmful one is logged and exits 1 without doing anything.
  3. When the suite ends, every logged call is printed as `  FAIL  host-shield: ...` and the
     suite fails, whatever it exited with.

A SUITE THAT GENUINELY NEEDS ONE DECLARES IT, in its own source, with a reason:

    # host-shield: names <command>[,<command>...] -- <reason>
        the source NAMES it (a fixture, a refusal message) and never runs it: the source check
        skips it. The stand-in still refuses a call at run time.
    # host-shield: runs <command>[,<command>...] -- <reason>
        the suite RUNS it for real: the stand-in hands it to the real command (and the source
        check skips it). None should; every one is a decision somebody can read.

`--` or an em dash separates the reason, which must be at least ten characters. A declaration
without one declares nothing and refuses the suite, naming the line.

WHAT IT CANNOT SEE, stated rather than implied: a suite that rebuilds PATH from nothing
(`env -i`, a PATH of only system directories) before calling one of these by bare name; a path
held in a variable that was itself assigned from another variable; a library the suite sources
(only the suite file is read); native code calling IOKit or private frameworks directly. The
stand-ins catch every bare-name call through PATH, which is how a shell, Python's subprocess,
Node's child_process and Rust's Command all look a command up.

Usage:
  host_shield.py run -- <argv...>      run one suite (or any check) shielded
  host_shield.py scan <file>...         the source check alone; exit 1 on a finding
  <host-shield>/<command> <args...>     a stand-in (reached through PATH; each one is a
                                       two-line sh script calling `--standin <command>`)
"""
import os
import re
import shlex
import signal
import subprocess
import sys
import tempfile

LIB = os.path.dirname(os.path.realpath(__file__))
SHIELD_DIR = os.path.join(LIB, "host-shield")
GUARD = os.path.normpath(os.path.join(LIB, "..", "hooks", "guard-host-display-power.sh"))
SHELLS = {"sh", "bash", "zsh", "dash", "ksh"}
LOG_ENV = "RICHOS_HOST_SHIELD_LOG"
RUNS_ENV = "RICHOS_HOST_SHIELD_RUNS"
TOOL_ENV = "RICHOS_HOST_SHIELD_TOOL"
# The screen-saver binary is judged by the guard on any PATH naming it, so the source check
# looks for it too. Assembled, because the guard's code surface reads the bare word in a
# literal as a call.
SCREEN_SAVER = "screensaver" + "engine"

# ---------------------------------------------------------------------------------------------
# The guard's analyzer, loaded from the guard itself
# ---------------------------------------------------------------------------------------------
# The rules live in ONE place: the Python inside guard-host-display-power.sh, between its
# `WRAPPERS = {` line and its `def candidates(` line (the shell analyzer: segments, wrappers,
# judge()). A copy here would be a second list to keep in step, which is the defect this
# engine has paid for repeatedly. If the block cannot be found or does not define what is used
# below, every stand-in REFUSES (a call it cannot judge is never assumed harmless) and the
# source check reports the suite as uncheckable.
_GUARD_NAMES = ("judge", "resolve", "split_segments", "strip_comments", "words", "ALWAYS_BAD",
                "ASSIGN_RE")
_G = None


class GuardUnavailable(Exception):
    pass


def guard():
    global _G
    if _G is not None:
        return _G
    try:
        with open(GUARD, encoding="utf-8") as fh:
            lines = fh.read().split("\n")
    except OSError as exc:
        raise GuardUnavailable("cannot read %s: %s" % (GUARD, exc))
    start = next((i for i, l in enumerate(lines) if l.startswith("WRAPPERS = {")), None)
    end = next((i for i, l in enumerate(lines) if l.startswith("def candidates(")), None)
    if start is None or end is None or end <= start:
        raise GuardUnavailable("the shell analyzer block was not found in %s" % GUARD)
    ns = {"os": os, "re": re, "shlex": shlex, "sys": sys, "__name__": "guard_host_display_power"}
    try:
        exec(compile("\n".join(lines[start:end]), GUARD, "exec"), ns)
    except Exception as exc:  # any failure to load means nothing can be judged harmless
        raise GuardUnavailable("the shell analyzer in %s did not load: %s" % (GUARD, exc))
    missing = [n for n in _GUARD_NAMES if n not in ns]
    if missing:
        raise GuardUnavailable("the shell analyzer in %s no longer defines %s" % (GUARD, ", ".join(missing)))
    _G = ns
    return ns


def standin_names():
    """The committed stand-ins: every stand-in file in host-shield/."""
    try:
        return sorted(n for n in os.listdir(SHIELD_DIR)
                      if not n.startswith(".") and is_standin(os.path.join(SHIELD_DIR, n)))
    except OSError:
        return []


# ---------------------------------------------------------------------------------------------
# Declarations
# ---------------------------------------------------------------------------------------------
DECL_RE = re.compile(r"^\s*#\s*host-shield:(.*)$")
DECL_FULL = re.compile(r"^\s*(names|runs)\s+([A-Za-z0-9_.-]+(?:\s*,\s*[A-Za-z0-9_.-]+)*)\s+(?:--|—)\s+(\S.*\S)\s*$")


def declarations(text):
    """({command: kind}, [error lines]) from a suite's own `# host-shield:` lines."""
    decl, errors = {}, []
    for n, line in enumerate(text.split("\n"), 1):
        m = DECL_RE.match(line)
        if not m:
            continue
        full = DECL_FULL.match(m.group(1))
        if not full or len(full.group(3)) < 10:
            errors.append("line %d: %r declares nothing: write `# host-shield: names|runs "
                          "<command>[,<command>] -- <reason of at least ten characters>`" % (n, line.strip()))
            continue
        for cmd in full.group(2).split(","):
            cmd = cmd.strip().lower()
            if decl.get(cmd) != "runs":
                decl[cmd] = full.group(1)
    return decl, errors


# ---------------------------------------------------------------------------------------------
# The source check
# ---------------------------------------------------------------------------------------------
def _trigger_re():
    names = set(standin_names()) | {SCREEN_SAVER}
    return re.compile(r"(?i)\b(?:%s)\b" % "|".join(re.escape(n) for n in sorted(names)))


def scan_text(text, G=None, depth=0):
    """[(line, command word, snippet, why)] for every line that reaches a judged command
    without going through PATH."""
    trig = _trigger_re()
    found = []
    for n, line in enumerate(text.split("\n"), 1):
        if not trig.search(line):
            continue
        G = G or guard()     # loaded only when a line names one: most suites never do
        always = {k.lower() for k in G["ALWAYS_BAD"]}
        for seg in G["split_segments"](G["strip_comments"](line)):
            found.extend((n,) + f for f in _scan_segment(seg, G, always, depth))
    return found


def _scan_segment(seg, G, always, depth):
    out = []
    raw = G["words"](seg)
    if not raw:
        return out
    for w in raw:
        value = w.split("=", 1)[1] if G["ASSIGN_RE"].match(w) else w
        name = os.path.basename(value.rstrip("/")).lower()
        if "/" in value and name in always:
            out.append((name, seg.strip()[:110], "names %s by path: %s" % (value, G["ALWAYS_BAD"].get(name, "always refused"))))
    argv, sudoed = G["resolve"](raw)
    if not argv:
        return out
    base = os.path.basename(argv[0])
    if base in SHELLS and depth < 4:
        for k, w in enumerate(argv[1:], 1):
            if w == "-c" and k + 1 < len(argv):
                for f in scan_text(argv[k + 1], G, depth + 1):
                    out.append(f[1:])
                break
        return out
    if "/" not in argv[0] and not sudoed:
        return out          # a bare name goes through PATH, where the stand-in is
    v = G["judge"](argv, sudoed)
    if v:
        how = "under sudo, which resets PATH" if sudoed and "/" not in argv[0] else "by path, which bypasses PATH"
        out.append((os.path.basename(argv[0]).lower(), " ".join(argv)[:110], "%s (%s)" % (v[1], how)))
    return out


def scan_file(path):
    """(findings, declared, errors) for one suite file. Findings exclude declared commands."""
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError as exc:
        return [], {}, ["cannot read %s: %s" % (path, exc)]
    declared, errors = declarations(text)
    try:
        found = [f for f in scan_text(text) if f[1] not in declared]
    except GuardUnavailable as exc:
        return [], declared, [str(exc)]
    return found, declared, errors


def suite_of(argv):
    """The shell suite a check runs, when it is `bash <file> ...` (or sh/zsh), else None."""
    if len(argv) >= 2 and os.path.basename(argv[0]) in SHELLS and not argv[1].startswith("-") \
            and os.path.isfile(argv[1]):
        return argv[1]
    if argv and argv[0].endswith(".sh") and os.path.isfile(argv[0]):
        return argv[0]
    return None


def refusal_lines(suite, findings, errors):
    lines = []
    for e in errors:
        lines.append("  FAIL  host-shield: %s: %s" % (suite, e))
    for n, _name, snip, why in findings:
        lines.append("  FAIL  host-shield: %s:%d: %s -- %s" % (suite, n, snip, why))
    if lines:
        lines.append("host-shield: REFUSED before it ran: %s reaches a command that changes this Mac's "
                     "power, sleep, lock or display state without going through the stand-ins "
                     "(ceo-decisions §65). Call it by bare name, or declare it: "
                     "`# host-shield: names|runs <command> -- <reason>`." % suite)
    return lines


# ---------------------------------------------------------------------------------------------
# Running a check shielded
# ---------------------------------------------------------------------------------------------
def shield_env(base, log, runs=()):
    env = dict(base)
    env["PATH"] = SHIELD_DIR + os.pathsep + env.get("PATH", "")
    env[LOG_ENV] = log
    env[RUNS_ENV] = " ".join(sorted(runs))
    env[TOOL_ENV] = os.path.realpath(__file__)
    return env


def logged(log):
    """The refused calls a stand-in wrote to `log`, one display line each."""
    try:
        with open(log, encoding="utf-8", errors="replace") as fh:
            rows = fh.read().splitlines()
    except OSError:
        return []
    return [r[len("REFUSED\t"):] for r in rows if r.startswith("REFUSED\t")]


def verdict_lines(calls, label):
    lines = ["  FAIL  host-shield: %s" % c for c in calls]
    if calls:
        lines.append("host-shield: %s called %d command(s) that change this Mac's power, sleep, lock or "
                     "display state; each stand-in refused it and nothing ran on the host. The suite "
                     "is counted as FAILED (ceo-decisions §65)." % (label, len(calls)))
    return lines


def cmd_run(argv):
    if not argv:
        sys.stderr.write("host_shield.py run: nothing to run (usage: host_shield.py run -- <argv...>)\n")
        return 2
    if not standin_names():
        print("  FAIL  host-shield: the stand-in directory %s is missing or empty; refusing to run "
              "%s unshielded." % (SHIELD_DIR, " ".join(argv)), flush=True)
        return 1
    suite = suite_of(argv)
    declared = {}
    if suite:
        findings, declared, errors = scan_file(suite)
        lines = refusal_lines(suite, findings, errors)
        if lines:
            print("\n".join(lines), flush=True)
            return 1
    fd, log = tempfile.mkstemp(prefix="richos-host-shield.", suffix=".log")
    os.close(fd)
    try:
        env = shield_env(os.environ, log, [c for c, k in declared.items() if k == "runs"])
        try:
            proc = subprocess.Popen(argv, env=env)
        except OSError as exc:
            print("host-shield: could not start %s: %s" % (argv[0], exc), flush=True)
            return 127
        forward = lambda sig, _frame: proc.send_signal(sig)
        for s in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
            signal.signal(s, forward)
        rc = proc.wait()
        if rc < 0:
            rc = 128 - rc
        calls = logged(log)
    finally:
        try:
            os.unlink(log)
        except OSError:
            pass
    if calls:
        print("\n".join(verdict_lines(calls, suite or argv[0])), flush=True)
        return rc if rc not in (0, 2) else 1
    return rc


def cmd_scan(paths):
    bad = 0
    for p in paths:
        findings, _declared, errors = scan_file(p)
        lines = refusal_lines(p, findings, errors)
        if lines:
            bad = 1
            print("\n".join(lines))
    return bad


# ---------------------------------------------------------------------------------------------
# The stand-in
# ---------------------------------------------------------------------------------------------
STANDIN_MARK = b"# host-shield stand-in"


def is_standin(path):
    try:
        with open(path, "rb") as fh:
            return STANDIN_MARK in fh.read(256)
    except OSError:
        return False


def _real(name):
    """The next `name` on PATH that is not a stand-in (this checkout's or any other's: a
    runner started inside a shielded suite puts its own copy in front)."""
    for d in os.environ.get("PATH", "").split(os.pathsep):
        p = os.path.join(d or ".", name)
        if os.path.isfile(p) and os.access(p, os.X_OK) and not is_standin(p):
            return p
    return None


def _osascript_text(args):
    """The script osascript would run, for judging: its -e lines, else its program file, else
    stdin (returned as bytes too, so it can be handed on unchanged)."""
    if "-e" in args:
        return None, None
    i = 0
    while i < len(args):
        a = args[i]
        if a in ("-l", "-s"):
            i += 2
            continue
        if a.startswith("-") and a != "-":
            i += 1
            continue
        break
    if i < len(args) and args[i] != "-":
        try:
            with open(args[i], "rb") as fh:
                return fh.read(1 << 20).decode("utf-8", "replace"), None
        except OSError:
            return None, None
    data = sys.stdin.buffer.read()
    return data.decode("utf-8", "replace"), data


def _refuse(name, args, why):
    shown = " ".join(shlex.quote(a) for a in [name] + args)
    line = "refused `%s` -- %s (pid %d, in %s); nothing ran on the host" % (shown, why, os.getppid(), os.getcwd())
    sys.stderr.write("host-shield: %s\n" % line)
    log = os.environ.get(LOG_ENV)
    if log:
        try:
            with open(log, "a", encoding="utf-8") as fh:
                fh.write("REFUSED\t%s\n" % line.replace("\n", " "))
        except OSError as exc:
            sys.stderr.write("host-shield: could not write %s: %s\n" % (log, exc))
    return 1


def standin(name, args):
    name = name.lower()
    stdin_bytes = None
    if name not in os.environ.get(RUNS_ENV, "").split():
        try:
            G = guard()
        except GuardUnavailable as exc:
            return _refuse(name, args, "this call could not be judged harmless (%s)" % exc)
        judged = [name] + args
        if name == "osascript":
            text, stdin_bytes = _osascript_text(args)
            if text is not None:
                judged = [name, "-e", text] + args
        v = G["judge"](judged, False)
        if v:
            return _refuse(name, args, "%s: %s" % (v[0], v[1]))
    real = _real(name)
    if real is None:
        sys.stderr.write("%s: command not found\n" % name)
        return 127
    if stdin_bytes is not None:
        return subprocess.run([real] + args, input=stdin_bytes).returncode
    os.execv(real, [name] + args)


def main():
    args = sys.argv[1:]
    if args[:1] == ["--standin"] and len(args) > 1:
        return standin(args[1], args[2:])
    if args[:1] == ["run"]:
        rest = args[1:]
        if rest[:1] == ["--"]:
            rest = rest[1:]
        return cmd_run(rest)
    if args[:1] == ["scan"] and len(args) > 1:
        return cmd_scan(args[1:])
    sys.stderr.write(__doc__.split("Usage:")[1])
    return 2


if __name__ == "__main__":
    sys.exit(main())
