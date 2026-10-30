#!/usr/bin/env python3
"""host_sleep.py: refuse an agent's Bash command that holds the Mac awake or changes its sleep or
power settings.

The rule (ceo-decisions §65, widened 2026-10-29): nothing an agent runs changes the host Mac's
display, sleep, lock, session or input state. guard-host-display-power.sh refuses the display
forms (`caffeinate -d`/`-u`, every `pmset` write) in the operator's terminal, and it let
`caffeinate -i`/`-s`/`-w` through as the safe way to keep a long job alive. On 2026-10-29 an agent
held `caffeinate -i -w <pid>` on the CEO's Mac for about seven minutes, and the CEO's answer was
that a hook refuses caffeinate from agents. Holding the Mac awake is a sleep-state change: the
sleep setting belongs to the person who uses the Mac, and an agent never overrides it.

What this refuses, wherever the command sits (after && ; | & newline, in ( ), { }, $( ),
backticks, sh -c / bash -c scripts, behind sudo, env, nohup, exec, time, nice, timeout, xargs):
  1. `caffeinate`, every form and every flag, by name or by path (/usr/bin/caffeinate)
  2. `pmset` with anything but `-g` (every other form writes power settings or acts at once:
     sleepnow, displaysleepnow, noidle, schedule, repeat, relative, -a/-b/-c/-u settings)
  3. `systemsetup -set...sleep` (computer, display and hard disk sleep)
  4. `defaults write|delete` of com.apple.PowerManagement
  5. `open -a`/`-b` of a keep-awake app (Amphetamine, Caffeine, KeepingYouAwake, Lungo, Theine)
  6. inline interpreter code (python -c, node -e, osascript -e, ...) that names caffeinate or
     creates an IOKit power assertion (IOPMAssertionCreate..., IOPMAssertionDeclareUserActivity),
     and the same in a heredoc fed to an interpreter; a heredoc fed to a shell is read as commands

What it lets through: `pmset -g ...` (reading), a mention (echo, grep, a commit message, a heredoc
body written to a file, a comment), and anything sent to the test VM: a segment whose command word is ssh/scp/sftp
or a script under a `testvm/` path runs in the guest, whose power state is its own.

IT IS A TEXT MATCH AND LEAKS (a script file that runs caffeinate, eval, a variable holding the
command). Its job is to stop the habitual command, not an adversary. Any error in the check is a
pass. Suite: scripts/hooks/guard-host-sleep.test.sh.
"""
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from unguarded_rm import tokenize  # noqa: E402  (one shell tokenizer for the Bash rules)

_ENV_ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
# Words that come before the command they run. Each may take options; a few take one value.
WRAPPERS = {"sudo", "doas", "env", "nohup", "time", "nice", "stdbuf", "setsid", "command",
            "builtin", "exec", "timeout", "gtimeout", "xargs", "!", "if", "then", "do", "else",
            "elif", "while", "until", "{"}
_OPTS_WITH_ARG = {
    "sudo": {"-u", "-g", "-h", "-p", "-C", "-D", "-R", "-T", "-U"},
    "doas": {"-u", "-C"},
    "nice": {"-n"},
    "env": {"-u", "-C", "-S"},
    "xargs": {"-n", "-I", "-P", "-L", "-d", "-s", "-E", "-a", "-J"},
    "timeout": {"-s", "-k", "--signal", "--kill-after"},
    "gtimeout": {"-s", "-k", "--signal", "--kill-after"},
    "stdbuf": {"-i", "-o", "-e"},
}
# The first positional of these is a value, not the command (`timeout 60 caffeinate`).
_POSITIONAL_VALUE = {"timeout", "gtimeout"}
SHELLS = {"sh", "bash", "zsh", "dash", "ksh", "fish"}
REMOTE = {"ssh", "scp", "sftp"}
INTERPRETERS = re.compile(r"^(?:python[0-9.]*|node|nodejs|deno|bun|ruby|perl|osascript|swift|php|lua)$")
INLINE_FLAGS = {"-c", "-e", "-E", "--eval", "-p", "--print", "-l"}
KEEP_AWAKE_APPS = re.compile(r"amphetamine|caffeine|keepingyouawake|lungo|theine", re.I)
INLINE_CODE = re.compile(r"\bcaffeinate\b|IOPMAssertion(?:Create\w*|DeclareUserActivity)")
SYSTEMSETUP_SLEEP = re.compile(r"^-set(?:computersleep|displaysleep|harddisksleep|sleep)$", re.I)


def _unquote(word):
    if len(word) >= 2 and word[0] == word[-1] and word[0] in "'\"":
        word = word[1:-1]
    return re.sub(r"\\(.)", r"\1", word).replace('"', "").replace("'", "")


def _strip_prefix(words):
    """Drop assignments, keywords and wrappers (with their options) -> argv from the command word."""
    i = 0
    while i < len(words):
        w = words[i]
        if _ENV_ASSIGN.match(w):
            i += 1
            continue
        base = os.path.basename(_unquote(w))
        if base not in WRAPPERS:
            break
        i += 1
        opts = _OPTS_WITH_ARG.get(base, set())
        while i < len(words):
            t = _unquote(words[i])
            if base == "env" and _ENV_ASSIGN.match(t):
                i += 1
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
    return [_unquote(w) for w in words[i:]]


def judge(argv):
    """-> a refusal reason for one resolved command, or None."""
    if not argv:
        return None
    word = argv[0]
    name = os.path.basename(word)
    args = argv[1:]
    shown = " ".join(argv)[:120]
    if name == "caffeinate":
        return ("`%s`: caffeinate creates a power assertion that keeps the Mac from sleeping "
                "(every form: -i and -s and -w included, not only -d and -u)" % shown)
    if name == "pmset":
        if not args or args[0] == "-g":
            return None
        return ("`%s`: every pmset form except `pmset -g` changes power or sleep settings or acts "
                "on them at once" % shown)
    if name == "systemsetup" and any(SYSTEMSETUP_SLEEP.match(a) for a in args):
        return "`%s`: sets the Mac's sleep policy" % shown
    if name == "defaults" and any(a in ("write", "delete") for a in args) and \
            any("com.apple.powermanagement" in a.lower() for a in args):
        return "`%s`: rewrites the Mac's power-management preferences" % shown
    if name == "open":
        for k, a in enumerate(args):
            if re.fullmatch(r"-[A-Za-z]*[ab]", a) and k + 1 < len(args) and KEEP_AWAKE_APPS.search(args[k + 1]):
                return "`%s`: starts an app whose job is to keep the Mac awake" % shown
            if KEEP_AWAKE_APPS.search(a) and a.endswith(".app"):
                return "`%s`: starts an app whose job is to keep the Mac awake" % shown
    if INTERPRETERS.match(name):
        for k, a in enumerate(args):
            if a in INLINE_FLAGS and k + 1 < len(args) and INLINE_CODE.search(args[k + 1]):
                return ("`%s ...`: the inline code starts caffeinate or creates a power assertion "
                        "that keeps the Mac awake" % name)
    return None


def _scan_words(words, findings, depth):
    argv = _strip_prefix(words)
    if not argv:
        return
    name = os.path.basename(argv[0])
    if name in SHELLS:
        for k, a in enumerate(argv[1:], 1):
            if re.fullmatch(r"-[A-Za-z]*c[A-Za-z]*", a) and k + 1 < len(argv):
                _scan_text(argv[k + 1], findings, depth + 1)
                break
        return
    # Sent to the test VM: everything after the command word runs in the guest.
    if name in REMOTE or re.search(r"(?:^|/)testvm/", argv[0]):
        return
    reason = judge(argv)
    if reason:
        findings.append(reason)


def _heredoc_bodies(text):
    """(command line, body) for each heredoc in `text`."""
    out = []
    for m in re.finditer(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1[^\n]*\n", text):
        start = m.end()
        end = re.search(r"^\s*%s\s*$" % re.escape(m.group(2)), text[start:], re.M)
        body = text[start:start + end.start()] if end else text[start:]
        line_start = text.rfind("\n", 0, m.start()) + 1
        out.append((text[line_start:m.start()], body))
    return out


def _scan_text(text, findings, depth=0):
    if depth > 6:
        return
    cmds, nested = tokenize(text)
    for words in cmds:
        _scan_words(words, findings, depth)
    for body in nested:
        _scan_text(body, findings, depth + 1)
    # A heredoc is input to the command in front of it (review 2026-10-29: `python3 <<EOF` and
    # `bash <<EOF` ran caffeinate). Fed to a shell, its body is commands; fed to an interpreter, it
    # is code. Written to a file or sent to the guest, it is text and passes.
    for head, body in _heredoc_bodies(text):
        head_cmds, _ = tokenize(head)
        for words in head_cmds:
            argv = _strip_prefix(words)
            if not argv:
                continue
            name = os.path.basename(argv[0])
            if name in REMOTE or re.search(r"(?:^|/)testvm/", argv[0]):
                break
            if name in SHELLS and not any(re.fullmatch(r"-[A-Za-z]*c[A-Za-z]*", a) for a in argv[1:]):
                _scan_text(body, findings, depth + 1)
                break
            if INTERPRETERS.match(name) and INLINE_CODE.search(body):
                findings.append("`%s <<...`: the heredoc it reads starts caffeinate or creates a power "
                                "assertion that keeps the Mac awake" % name)
                break


def verdict(command):
    """-> list of refusal reasons (empty when the command passes)."""
    findings = []
    _scan_text(command, findings)
    out = []
    for f in findings:
        if f not in out:
            out.append(f)
    return out


TRIGGER = re.compile(r"caffeinate|pmset|systemsetup|PowerManagement|IOPMAssertion|"
                     r"Amphetamine|Caffeine|KeepingYouAwake|Lungo|Theine", re.I)

REFUSAL = """REFUSED (guard-host-sleep): this command would keep this Mac awake or change its sleep or power settings. Nothing ran.
  - %s

Why: the Mac's sleep and power settings belong to the person who uses it. An agent never holds it
awake and never changes them, not even for a few minutes.

What to do instead:
  * A long command: run it as it is (use run_in_background for a long one). If the Mac sleeps,
    the command pauses and carries on when it wakes; nothing is lost.
  * Work that must not pause: run it in the test VM (ssh <vm> '...' or testvm/<script>), whose
    power state is the guest's own.
  * Reading power state: `pmset -g ...` is allowed.
"""


def check(payload_text):
    """PreToolUse[Bash]: 2 with the refusal on stderr, else 0. Unreadable input passes."""
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
        reasons = verdict(command)
    except Exception:  # any error in the check is a pass
        return 0
    if not reasons:
        return 0
    sys.stderr.write(REFUSAL % "\n  - ".join(reasons))
    return 2


def main(argv):
    if argv and argv[0] == "check":
        return check(sys.stdin.read())
    sys.stderr.write("usage: host_sleep.py check < payload.json\n")
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
