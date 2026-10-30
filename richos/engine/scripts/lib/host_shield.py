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
     in a form guard-host-display-power.sh refuses OR the test-run rules below refuse,
     refuses the suite before it runs, inside `if`/`while`/`!` and the other control words
     too. Every physical line is read, and every backslash-continued command is read joined
     as well. So does a path to an always-bad command anywhere on a line, and a path to ANY
     stood-in command assigned to a variable (`POWER=/usr/bin/<it>` then `$POWER ...` would
     bypass PATH and the stand-in alike). An osascript by path whose script is in a file or
     on stdin is refused too: the source check cannot read that script.
     A PYTHON OR NODE suite (`python3 <file>`, `node <file>`, a script started directly, or
     inline `-c`/`-e` code) is read too. No shell parser can tell its data from its calls, so
     ANY absolute path to one of these commands in it, or sudo/doas beside one, refuses it.
     The command line itself is judged as well: a host command started by path, or a shell's
     inline string. Launchers (`env A=1`, `nice`, `timeout 5`...) and the interpreter's own
     options (`bash -e`, `python3 --`) are stepped past to the file they start; a form whose
     script cannot be read (stdin, sudo, a launcher option it does not know) is refused.
  2. The suite runs with `host-shield/` FIRST ON PATH. That directory holds one stand-in per
     command the guard judges. A stand-in judges its own argv with the GUARD'S OWN judge()
     (read out of guard-host-display-power.sh, so the two cannot drift), plus the TEST-RUN
     RULES below: a harmless form (`pmset -g`, an osascript that touches nothing) is handed to
     the real command unchanged; a harmful one is logged and exits 1 without doing anything.
     THE TEST-RUN RULES are stricter than the guard's, because the guard allows a few forms an
     operator legitimately types and a test never needs: every `caffeinate` (it holds the
     host awake, `-is` included); every `open` (it launches or activates an app, a file's
     app or a URL's browser); and any osascript that is not plainly inert: a keystroke or
     key code (modifier or not: it reaches whatever is frontmost on the host), an app set
     frontmost, activated or launched, a shell run from inside it, the Objective-C bridge
     or raw Apple events, a dialog or the clipboard, any `tell` or `Application(...)`, and
     a compiled script it cannot read. A script whose harm cannot be ruled out is refused:
     every word outside a string must be on a short inert list (OSA_INERT_WORDS), so an
     action nobody listed is refused, never assumed harmless.
  3. When the suite ends, every logged call is printed as `  FAIL  host-shield: ...` and the
     suite fails, whatever it exited with.

A SUITE THAT GENUINELY NEEDS ONE DECLARES IT, in its own source, with a reason:

    # host-shield: names <command>[,<command>...] -- <reason>
        the source NAMES it (a fixture, a refusal message) and never runs it: the source check
        skips a path to it in the arguments of a data command (echo, printf, grep, test), and
        nothing else. A call by path or under sudo is still refused before the suite runs, and
        the stand-in still refuses a call at run time.
    # host-shield: runs <command>[,<command>...] -- <reason>
        the suite RUNS it for real: the stand-in hands it to the real command (and the source
        check skips it). None should; every one is a decision somebody can read.

`--` or an em dash separates the reason, which must be at least ten characters. A declaration
without one declares nothing and refuses the suite, naming the line.

WHAT IT CANNOT SEE, stated rather than implied: a suite that rebuilds PATH from nothing
(`env -i`, a PATH of only system directories) before calling one of these by bare name; a path
held in a variable that was itself assigned from another variable; a library the suite sources
or imports (only the suite file is read); a path a Python or Node suite assembles at run time;
`python3 -m <module>` (no file is named); native code calling IOKit or private frameworks
directly. The
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
# Shell control words a command can follow on a line (`if <cmd>; then`, `! <cmd>`); the source
# check steps over them to the command word, as the shell does.
CONTROL_WORDS = {"if", "then", "elif", "else", "while", "until", "do", "!", "time", "{", "}"}
# THE TEST-RUN RULES (see the header): what a stand-in refuses beyond the guard's judge().
# An osascript in a test run is handed on only when it plainly touches nothing outside itself
# (`return 1`). Anything that addresses an app, runs a shell, reaches the Objective-C bridge or
# raw Apple events, or puts something on the screen is refused: the guard's list names what
# is known to be harmful, and in a test run a script whose harm cannot be ruled out is refused
# too (reviews rv-20261011T141442Z-0009fe5a-a898 #2: `activate`, JXA `keyCode(53)` and
# `do shell script "<a host halt>"` were all handed on).
TEST_RUN_OSA = [
    (re.compile(r"\b(?:keystroke|key\s*code)\b", re.IGNORECASE),
     "sends a keystroke to whatever is frontmost on the host"),
    (re.compile(r"\bset\b[^\n]{0,120}\bfrontmost\b|\bfrontmost\s*=\s*true\b", re.IGNORECASE),
     "brings an app to the front on the host, taking the CEO's focus"),
    (re.compile(r"\b(?:activate|launch|reopen)\b", re.IGNORECASE),
     "launches or activates an app on the host, taking the CEO's focus"),
    (re.compile(r"\bdo\s+shell\s+script\b|\bdoShellScript\b", re.IGNORECASE),
     "runs a shell command the stand-ins never see"),
    (re.compile(r"«|\bObjC\b|\$\s*[.(]|\buse\s+framework\b|\b(?:run|load)\s+script\b", re.IGNORECASE),
     "reaches past what the shield can judge (raw Apple events, the Objective-C bridge, a nested script)"),
    (re.compile(r"\bdisplay\s+(?:dialog|alert|notification)\b|\bchoose\s+(?:file|folder|from|application|color)\b"
                r"|\bthe\s+clipboard\b|\bopen\s+location\b", re.IGNORECASE),
     "puts a dialog, a notification or the clipboard in front of the CEO on the host"),
    (re.compile(r"\btell\b|\bapplication\b", re.IGNORECASE),
     "addresses an app on the host; a test never needs to"),
]


def test_run_rule(name, judged):
    """(rule, why) for a call the guard allows and a test run never may, else None."""
    if name == "caffeinate":
        return ("caffeinate", "holds the host awake (any form, -is included); a test never needs it")
    if name == "open":
        # Every form launches or activates something on the host: an app (-a, -b), a file's
        # app, a URL's browser, Finder (-R). Review rv-20261011T173659Z-e08ee8a2-2c75 #2.
        return ("open", "launches or activates an app, a file's app or a URL's browser on the host, "
                        "taking the CEO's focus; a test never needs it")
    if name == "osascript":
        script = " \n ".join(judged[1:])
        if OSA_UNJUDGEABLE in script:
            return ("osascript", "a compiled script cannot be read, so it cannot be judged harmless")
        for rx, why in TEST_RUN_OSA:
            if rx.search(script):
                return ("osascript", why)
        why = osa_not_inert(judged[1:])
        if why:
            return ("osascript", why)
    return None


# THE INERT VOCABULARY. The list above names what is known to be harmful; a script is handed on
# only when every word in it is on this list too, so an action nobody listed (`choose URL`,
# `say`, `beep`, `set volume`) is refused rather than assumed harmless (review
# rv-20261011T192225Z-5fa33e7e-a349 #1: `choose URL` was handed on). String literals and
# comments are data and are not read; every other word must be one of these.
OSA_INERT_WORDS = {
    "applescript": {"return", "log", "true", "false", "and", "or", "not", "is", "equal", "equals",
                    "to", "than", "greater", "less", "mod", "div", "as", "text", "integer", "real",
                    "number", "string", "list", "boolean", "item", "of", "the", "count", "length",
                    "if", "then", "else", "end", "error", "missing", "value", "contains",
                    "linefeed", "tab", "space", "quote"},
    "javascript": {"return", "true", "false", "null", "undefined", "console", "log", "JSON",
                   "stringify"},
}
# The punctuation an inert script may carry once its strings and comments are taken out.
OSA_INERT_CHARS = {"applescript": set("+-*/^&=<>(){},.:¬≠≤≥ \t\r\n"),
                   "javascript": set("+-*/%=<>!&|?:;,.()[]{} \t\r\n")}


def osa_not_inert(args):
    """None when the osascript call's every -e script is plainly inert, else why it is not.
    Anything this cannot read (another language, an option it does not know, a script in a
    file or on stdin, a word or character outside the inert vocabulary) is NOT inert."""
    lang, scripts, i = "applescript", [], 0
    while i < len(args):
        a = args[i]
        if a in ("-e", "-l", "-s"):
            if i + 1 >= len(args):
                return "%s is given no value" % a
            if a == "-e":
                scripts.append(args[i + 1])
            elif a == "-l":
                lang = args[i + 1].lower()
            i += 2
            continue
        if a.startswith("-") and a != "-":
            return "it is given the option %s, which the shield does not know" % a
        break           # the first operand: the arguments the script is handed, which are data
    lang = {"js": "javascript"}.get(lang, lang)
    if lang not in OSA_INERT_WORDS:
        return "its language %s cannot be read, so it cannot be judged harmless" % lang
    if not scripts:
        return "its script is in a file or on stdin that the shield could not read"
    for script in scripts:
        rest = _osa_residue(script, lang)
        if rest is None:
            return "a JavaScript template string can run code, so it cannot be judged harmless"
        odd = sorted({c for c in rest if not (c.isalnum() or c == "_") and c not in OSA_INERT_CHARS[lang]})
        if odd:
            return "it carries %s outside a string, which the shield cannot judge harmless" % " ".join(odd)
        for w in re.findall(r"[A-Za-z_][A-Za-z0-9_]*", rest):
            if (w.lower() if lang == "applescript" else w) not in OSA_INERT_WORDS[lang]:
                return ("it does an action the shield does not recognize as inert (`%s`); a script whose "
                        "harm cannot be ruled out is refused" % w)
    return None


def _osa_residue(text, lang):
    """The script with its string literals and comments taken out (each string becomes `0`).
    None for a JavaScript template string, whose `${...}` runs code."""
    out, i, n = [], 0, len(text)
    quotes = '"' if lang == "applescript" else "\"'"
    at_line_start = True
    while i < n:
        c = text[i]
        if c in quotes:
            j = i + 1
            while j < n and text[j] != c:
                j += 2 if text[j] == "\\" else 1
            out.append(" 0 ")
            i, at_line_start = j + 1, False
            continue
        if lang == "javascript" and c == "`":
            return None
        line_comment = text.startswith("--" if lang == "applescript" else "//", i) \
            or (lang == "applescript" and c == "#" and at_line_start)
        if line_comment:
            while i < n and text[i] not in "\r\n":
                i += 1
            continue
        opener, closer = ("(*", "*)") if lang == "applescript" else ("/*", "*/")
        if text.startswith(opener, i):
            depth, j = 1, i + 2
            while j < n and depth:
                if lang == "applescript" and text.startswith(opener, j):
                    depth, j = depth + 1, j + 2
                elif text.startswith(closer, j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            out.append(" ")
            i = j
            continue
        out.append(c)
        if c in "\r\n":
            at_line_start = True
        elif not c.isspace():
            at_line_start = False
        i += 1
    return "".join(out)


# What _osascript_text returns for a program file it cannot read as text (a compiled .scpt).
OSA_UNJUDGEABLE = "\x00host-shield: unjudgeable compiled script\x00"

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
DECL_RE = re.compile(r"^\s*(?:#|//)\s*host-shield:(.*)$")
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


# Commands whose arguments are only ever data: a path to a host command named after one of
# them (`printf '%s\n' /sbin/<it>`) is a MENTION, the one kind of finding a `names`
# declaration exempts. Anywhere else (`xargs <path>`, `timeout 5 <path>`, the command word)
# the same path may run, and only `runs` exempts it (review rv-20261011T141442Z-0009fe5a-a898 #6).
DATA_COMMANDS = {"echo", "printf", "grep", "egrep", "fgrep", "rg", "test", "[", "[[", ":"}
# ...except an option of one that RUNS its value: ripgrep's `--pre <program>` starts that program
# for every file it searches (review rv-20261011T192225Z-5fa33e7e-a349 #4: `rg --pre /sbin/<it>`
# was a mention that `names` exempted). Such a value is a call, never a mention.
EXEC_OPTIONS = {"rg": {"--pre"}}


def exec_values(argv):
    """{index in argv: the program} for every option value argv's command will RUN."""
    opts = EXEC_OPTIONS.get(os.path.basename(argv[0]) if argv else "", set())
    out = {}
    for k, w in enumerate(argv):
        if w in opts and k + 1 < len(argv):
            out[k + 1] = argv[k + 1]
        elif "=" in w and w.split("=", 1)[0] in opts:
            out[k] = w.split("=", 1)[1]
    return out


def logical_lines(text):
    """[(first line number, text)] for every backslash-continued run of lines, joined as the
    shell joins it (review rv-20261011T141442Z-0009fe5a-a898 #3: `<path> \\` then
    `-e '<a host sleep>'` split the command from its harmful argument). The backslash and the
    newline are removed and NOTHING is put in their place, exactly as the shell (and a Python or
    JavaScript string) does: `/sbin/ha\\` then `lt` is one word (review
    rv-20261011T192225Z-5fa33e7e-a349 #3: a space put between them hid it)."""
    out, buf, at = [], None, 0
    for n, raw in enumerate(text.split("\n"), 1):
        if buf is None:
            buf, at, joined = raw, n, False
        else:
            buf, joined = buf + raw, True
        if buf.endswith("\\") and (len(buf) - len(buf.rstrip("\\"))) % 2 == 1:
            buf = buf[:-1]
            continue
        if joined:
            out.append((at, buf))
        buf = None
    if buf is not None and buf.strip():
        out.append((at, buf))
    return out


def scan_text(text, G=None, depth=0):
    """[(line, command word, snippet, why, kind)] for every line that reaches a judged command
    without going through PATH. Every physical line is read, and so is every backslash-
    continued command joined into one: either one finding refuses. `kind` is "mention" for a
    path in a data command's arguments, else "exec"."""
    trig = _trigger_re()
    found, seen = [], set()
    lines = list(enumerate(text.split("\n"), 1)) + logical_lines(text)
    for n, line in lines:
        if not trig.search(line):
            continue
        G = G or guard()     # loaded only when a line names one: most suites never do
        always = {k.lower() for k in G["ALWAYS_BAD"]}
        for seg in G["split_segments"](G["strip_comments"](line)):
            for f in _scan_segment(seg, G, always, depth):
                if (n, f[0], f[2]) not in seen:     # a line read alone and joined: once
                    seen.add((n, f[0], f[2]))
                    found.append((n,) + f)
    return found


def _scan_segment(seg, G, always, depth):
    out = []
    raw = G["words"](seg)
    if not raw:
        return out
    words = raw
    while words and words[0] in CONTROL_WORDS:
        words = words[1:]
    argv, sudoed = G["resolve"](words)
    data = bool(argv) and not sudoed and os.path.basename(argv[0]) in DATA_COMMANDS
    guarded = {n.lower() for n in standin_names()}
    runs = {len(raw) - len(argv) + k: v for k, v in exec_values(argv).items()}
    for at, w in enumerate(raw):
        assigned = bool(G["ASSIGN_RE"].match(w))
        value = w.split("=", 1)[1] if assigned else runs.get(at, w)
        name = os.path.basename(value.rstrip("/")).lower()
        if "/" not in value:
            continue
        if at in runs and (name in always or name in guarded or name == SCREEN_SAVER.lower()):
            out.append((name, seg.strip()[:110], "%s runs %s for every file it searches, which bypasses PATH "
                        "and the stand-in" % (os.path.basename(argv[0]), value), "exec"))
        elif name in always:
            out.append((name, seg.strip()[:110], "names %s by path: %s" % (value, G["ALWAYS_BAD"].get(name, "always refused")),
                        "mention" if data and not assigned else "exec"))
        elif assigned and name in guarded and value.startswith(("/", "~")):
            # Absolute only: `X="$STUBS/open"` is the suite's own recording stub, not the host's.
            out.append((name, seg.strip()[:110], "assigns the path %s to a variable: calling it through the "
                        "variable bypasses PATH and the stand-in" % value, "exec"))
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
    # A path (or sudo) never reaches a stand-in, so the source check applies the TEST-RUN
    # RULES here as well as the guard's: an absolute caffeinate or osascript keystroke is
    # refused exactly as its bare-name call is (review rv-20261011T141442Z-0009fe5a-a898 #1).
    name = base.lower()
    v = G["judge"](argv, sudoed) or test_run_rule(name, argv)
    if not v and name == "osascript" and "-e" not in argv[1:]:
        v = ("osascript", "its script is in a file or on stdin, which the source check cannot read; "
                          "call it by bare name so the stand-in judges the script")
    if v:
        how = "under sudo, which resets PATH" if sudoed and "/" not in argv[0] else "by path, which bypasses PATH"
        out.append((name, " ".join(argv)[:110], "%s (%s)" % (v[1], how), "exec"))
    return out


def _code_res():
    names = "|".join(re.escape(n) for n in sorted(set(standin_names()) | {SCREEN_SAVER}))
    # An absolute path in a string or an argument list: it starts at the root, right after a
    # quote, a bracket, `=`, a comma or a space, passes through at least one directory (every
    # host command lives in one; a bare `/open ...` is a JavaScript regular expression), and
    # ends at one of those.
    path = re.compile(r"(?i)(?<![^\s\"'`(=,\[])(/(?:[\w.@+-]+/)+(?:%s))(?=$|[\s\"'`),;\]])" % names)
    sudo = re.compile(r"(?i)\b(?:sudo|doas)\b[^\n]{0,80}?\b(%s)\b" % names)
    return path, sudo


def scan_code(text):
    """Findings for a Python or Node source: no shell parser can tell its data from its calls,
    so ANY absolute path to a stood-in command, and any sudo/doas next to one, refuses it
    (review rv-20261011T141442Z-0009fe5a-a898 #4: an absolute halt in `subprocess.run([...])` in
    a Python suite started unchecked). A path assembled at run time is not seen."""
    path, sudo = _code_res()
    found, seen = [], set()
    # A backslash-newline inside a Python or JavaScript string joins it too: read joined as well
    # (a line read alone and joined is reported once).
    for n, line in list(enumerate(text.split("\n"), 1)) + logical_lines(text):
        hits = [(m.group(1), "names %s by path in code, which bypasses PATH and the stand-in; call it "
                 "by bare name" % m.group(1)) for m in path.finditer(line)]
        hits += [(m.group(1), "runs %s under sudo or doas, which resets PATH past the stand-in" % m.group(1))
                 for m in sudo.finditer(line)]
        for what, why in hits:
            if (n, why) not in seen:
                seen.add((n, why))
                found.append((n, os.path.basename(what).lower(), line.strip()[:110], why, "exec"))
    return found


CODE_EXT = {".py": "python", ".js": "node", ".mjs": "node", ".cjs": "node", ".ts": "node"}


def language_of(path, text):
    """shell, python or node: by the file's extension, else its first line."""
    ext = os.path.splitext(path)[1].lower()
    if ext in CODE_EXT:
        return CODE_EXT[ext]
    first = text.split("\n", 1)[0]
    if first.startswith("#!"):
        if re.search(r"\bpython[0-9.]*\b", first):
            return "python"
        if re.search(r"\bnode(?:js)?\b", first):
            return "node"
    return "shell"


def scan_source(text, lang, declared):
    """(findings, errors) for source text in `lang`, without the declared ones."""
    try:
        found = scan_text(text) if lang == "shell" else scan_code(text)
    except GuardUnavailable as exc:
        return [], [str(exc)]
    return [f for f in found if not exempt(f, declared)], []


def scan_file(path, lang=None):
    """(findings, declared, errors) for one suite file, shell, Python or Node. Findings exclude
    declared commands."""
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError as exc:
        return [], {}, ["cannot read %s: %s" % (path, exc)]
    declared, errors = declarations(text)
    found, unavailable = scan_source(text, lang or language_of(path, text), declared)
    if unavailable:
        return [], declared, unavailable
    return found, declared, errors


def exempt(finding, declared):
    """`runs` exempts every finding for its command; `names` only a MENTION (a path in a data
    command's arguments), never a call that could run it past the stand-in."""
    kind = declared.get(finding[1])
    return kind == "runs" or (kind == "names" and finding[4] == "mention")


PYTHON_RE = re.compile(r"^python[0-9.]*$")
NODES = {"node", "nodejs"}
# Interpreter options that take the next word as their value.
PYTHON_VALUE_OPTS = {"-W", "-X", "--check-hash-based-pycs"}
NODE_VALUE_OPTS = {"-r", "--require", "--import", "--loader", "--experimental-loader", "--conditions", "-C",
                   "--input-type", "--title"}


def suite_of(argv):
    """The suite file a check runs: a shell, `python3` or `node` given a file, or the file itself
    started directly. None for any other form."""
    return next((value for kind, value, _lang in sources_of(argv) if kind == "file"), None)


# LAUNCHERS a check may be started under, and how to step past each one's options to the
# command it starts (review rv-20261011T192225Z-5fa33e7e-a349 #2: `env REVIEW=1 python3 <file>`
# started the file unread). {name: (options that take a value, options that take none)}. An
# option not listed here cannot be stepped past, so the check is refused, never started unread.
# sudo and doas are not listed: they reset PATH past every stand-in, so a check started under
# one is refused outright.
LAUNCHERS = {
    "env": ({"-u", "--unset", "-P"}, {"-i", "-0", "-v", "--ignore-environment", "--null", "-"}),
    "nice": ({"-n", "--adjustment"}, set()),
    "nohup": (set(), set()),
    "time": (set(), {"-p", "-l", "-h"}),
    "command": (set(), {"-p"}),
    "exec": ({"-a"}, {"-c", "-l"}),
    "stdbuf": ({"-i", "-o", "-e", "--input", "--output", "--error"}, set()),
    "setsid": (set(), {"-f", "-w", "-c", "--fork", "--wait", "--ctty"}),
    "timeout": ({"-s", "-k", "--signal", "--kill-after"}, {"--foreground", "--preserve-status", "-v",
                                                           "--verbose", "-f", "-p"}),
    "gtimeout": ({"-s", "-k", "--signal", "--kill-after"}, {"--foreground", "--preserve-status", "-v",
                                                            "--verbose"}),
}
ASSIGN_WORD = re.compile(r"^[A-Za-z_][A-Za-z_0-9]*=")
# Options of a shell that take the next word as their value.
SHELL_VALUE_OPTS = {"-o", "+o", "-O", "+O", "--rcfile", "--init-file"}


def unwrap(argv):
    """(argv from the command a launcher chain starts, None) or ([], why it cannot be read)."""
    argv = list(argv)
    while argv:
        base = os.path.basename(argv[0])
        if base in ("sudo", "doas"):
            return [], ("it is started under %s, which resets PATH past the stand-ins" % base)
        if base not in LAUNCHERS:
            return argv, None
        values, flags = LAUNCHERS[base]
        i = 1
        while i < len(argv):
            a = argv[i]
            if a == "--":
                i += 1
                break
            if base == "env" and ASSIGN_WORD.match(a):
                i += 1
                continue
            if a in values:
                i += 2
                continue
            if a in flags or (a.split("=", 1)[0] in values and "=" in a) \
                    or (base == "nice" and re.match(r"^-\d+$", a)) \
                    or (base == "stdbuf" and re.match(r"^-[ioe].+$", a)):
                i += 1
                continue
            if a.startswith("-"):
                return [], ("its launcher %s is given the option %s, which the shield cannot step past "
                            "to the command it starts" % (base, a))
            break
        if base in ("timeout", "gtimeout") and i < len(argv):
            i += 1          # the duration
        argv = argv[i:]
    return argv, None


def _shell_sources(argv):
    """A shell's script: its file operand after its options, or its -c string (a `c` in any
    option cluster: `-ec`). A shell reading stdin cannot be read."""
    i, inline = 1, False
    while i < len(argv):
        a = argv[i]
        if a in ("--", "-"):
            i += 1
            break
        if a in SHELL_VALUE_OPTS:
            i += 2
            continue
        if a.startswith("--"):
            i += 1
            continue
        if a[:1] in "-+" and len(a) > 1:
            if a[0] == "-" and "c" in a[1:]:
                inline = True
            if a[0] == "-" and "s" in a[1:]:
                return [("unreadable", "it starts a shell that reads its commands from stdin", None)]
            i += 2 if ("o" in a[1:] or "O" in a[1:]) else 1
            continue
        break
    if i >= len(argv):
        return [("unreadable", "it starts a shell with no script file, which reads its commands from stdin",
                 None)]
    if inline:
        return [("inline", argv[i], "shell")]
    return [("file", argv[i], None)] if os.path.isfile(argv[i]) else []


def _interpreter_sources(argv, lang):
    """Python's or Node's script: its file operand after its options (`--` included), or its
    inline code (Python's -c, also inside a cluster such as `-Bc`; Node's -e/-p)."""
    values = PYTHON_VALUE_OPTS if lang == "python" else NODE_VALUE_OPTS
    inline = {"-c"} if lang == "python" else {"-e", "--eval", "-p", "--print"}
    i = 1
    while i < len(argv):
        a = argv[i]
        if a in inline:
            return [("inline", argv[i + 1], lang)] if i + 1 < len(argv) else []
        if a == "-m":
            return []
        if a == "-":
            return [("unreadable", "it starts %s reading its program from stdin" % lang, None)]
        if a == "--":
            i += 1
            break
        if a in values:
            i += 2
            continue
        if lang == "python" and re.match(r"^-[A-Za-z]{2,}$", a):
            # A cluster: the first of c, m, W, X takes the rest of it, or the next word.
            for k, ch in enumerate(a[1:], 1):
                if ch in "cmWX":
                    value = a[k + 1:] or (argv[i + 1] if i + 1 < len(argv) else None)
                    if ch == "c":
                        return [("inline", value, lang)] if value is not None else []
                    if ch == "m":
                        return []
                    i += 1 if a[k + 1:] else 2
                    break
            else:
                i += 1
            continue
        if a.startswith("-"):
            i += 1
            continue
        break
    if lang == "node" and "--test" in argv[1:i]:
        # The test runner: every operand is a test file (none: it finds them itself, unread).
        return [("file", f, lang) for f in argv[i:] if os.path.isfile(f)]
    if i >= len(argv):
        return [("unreadable", "it starts %s with no program file, which reads it from stdin" % lang, None)]
    return [("file", argv[i], lang)] if os.path.isfile(argv[i]) else []


def sources_of(argv):
    """[(kind, value, language)] the check will run: ("file", path, lang) for a suite file,
    ("inline", code, lang) for Python's -c, Node's -e/-p or a shell's -c, ("unreadable", why,
    None) for a form whose script cannot be read (stdin, a launcher option it cannot step past,
    sudo). Launchers (env, nice, timeout...) are stepped past first, and every interpreter's
    options. `python3 -m <module>` names no file here: stated in the header."""
    argv, why = unwrap(argv)
    if why:
        return [("unreadable", why, None)]
    if not argv:
        return []
    base = os.path.basename(argv[0])
    if base in SHELLS:
        return _shell_sources(argv)
    if PYTHON_RE.match(base) or base in NODES:
        return _interpreter_sources(argv, "python" if PYTHON_RE.match(base) else "node")
    try:
        with open(argv[0], "rb") as fh:
            # A script started directly (a `#!` line, or a .sh file), never a binary.
            script = fh.read(2) == b"#!" or argv[0].endswith(".sh")
    except OSError:
        script = False
    return [("file", argv[0], None)] if script else []


def direct_findings(argv):
    """Findings for the command line itself: a host command started by path (which never
    reaches a stand-in), or a shell's inline string."""
    line = " ".join(shlex.quote(a) for a in argv)
    if not _trigger_re().search(line):
        return []
    G = guard()
    always = {k.lower() for k in G["ALWAYS_BAD"]}
    return [(0,) + f for f in _scan_segment(line, G, always, 0)]


def refusal_lines(suite, findings, errors):
    lines = []
    for e in errors:
        lines.append("  FAIL  host-shield: %s: %s" % (suite, e))
    for n, _name, snip, why, _kind in findings:
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
    # The developer toolchain (scripts/lib/toolchain.py, 2026-10-29): a suite or build that calls
    # make, git, cc, python3... through an /usr/bin stub finds a usable developer directory via
    # DEVELOPER_DIR even in a sanitized environment; with none on this Mac, the stubs are shadowed
    # by stand-ins that exit 127 with a plain line, so no install dialog ever opens.
    try:
        sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
        import toolchain
        env = toolchain.harness_env(base, log + ".toolchain-standins")
    except Exception:  # the toolchain step never stops a suite from running
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
    declared, lines = {}, []
    for kind, value, lang in sources_of(argv):
        if kind == "file":
            findings, found_declared, errors = scan_file(value, lang)
            declared.update(found_declared)
            lines.extend(refusal_lines(value, findings, errors))
        elif kind == "unreadable":
            lines.extend(refusal_lines("the command line", [], ["%s, so what it runs cannot be checked" % value]))
        else:
            findings, errors = scan_source(value, lang, {})
            lines.extend(refusal_lines("%s (its inline code)" % argv[0], findings, errors))
    try:
        lines.extend(refusal_lines("the command line", direct_findings(argv), []))
    except GuardUnavailable as exc:
        lines.extend(refusal_lines("the command line", [], [str(exc)]))
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
        import shutil
        shutil.rmtree(log + ".toolchain-standins", ignore_errors=True)
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
                data = fh.read(1 << 20)
        except OSError:
            return None, None
        if data.startswith(b"FasdUAS") or b"\x00" in data:
            return OSA_UNJUDGEABLE, None
        return data.decode("utf-8", "replace"), None
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
        v = G["judge"](judged, False) or test_run_rule(name, judged)
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
