#!/usr/bin/env python3
"""shell_c.py: refuse, BEFORE Claude Code asks the user, a Bash command that wraps an inline script
in `bash -c`, `sh -c` or `zsh -c`.

The CEO, 2026-10-09, saw "This shell -c script runs rm and could not be checked. Do you want to
proceed?" for a teammate's command, hit yes, and wrote: "I hope RichOS app users won't be bothered by
this." (The same prompt reached him on 2026-10-07 from the lead's own commands.) Claude Code cannot
check an inline shell script, so it asks the person. A PreToolUse refusal runs first, so the agent
gets the rewrite and the person never sees a prompt.

What is refused: ONE plain match over the whole command text, quotes and comments included: a word
named bash, sh, zsh, dash or ksh (with or without a path or quotes) followed, anywhere later in the
command (across lines and separators), by a short-option group containing c (-c, -lc, ...).
`bash test.sh && git diff --cached` passes (--cached is a long option); `bash x.sh | grep -c y` is refused, an accepted false positive; `bash test.sh -c x` is refused, an
accepted false positive. A command that only MENTIONS `bash -c` (a commit message, an echo) is refused too:
an accepted false positive; the message says to put the text in a file (git commit -F <file>).

IT IS A TEXT MATCH AND LEAKS (eval, a script file that itself runs bash -c). Its job is to stop the
habitual command, not to out-think the shell. Any error in the check is a pass.
Shared by the operator-install rule (scripts/hooks/guard-no-shell-c.sh) and by the RichOS app's hook
(mega-lander/app.py validate_shell_target).
"""
import json
import re
import sys

SHELL_WORD = re.compile(r"(?<![\w.\-])(bash|sh|zsh|dash|ksh)(?![\w.\-])")
# An option word starts a word of the command: at the start of the text, or after whitespace, a quote
# or shell punctuation. Never mid-word or after a / (2026-10-24: a session scratch folder of the shape
# /private/tmp/claude-<uid>/-Users-<user>-<project>/... was read as the option group -Users-...).
OPTION_WORD = re.compile(r"(?<![^\s'\"`;|&(){}<>])-[A-Za-z0-9-]*")

MESSAGE = (
    "=== guard-no-shell-c: BLOCKED ===\n"
    "  This command contains `{shell} -c` (an inline shell script). Claude Code cannot check an inline\n"
    "  shell script, so it stops and ASKS THE USER ('This shell -c script ... could not be checked. Do\n"
    "  you want to proceed?'). The user is never the fallback check.\n"
    "  FIX: run the steps as separate commands (one step per Bash call), or write the script to a file\n"
    "  in your scratch folder and run `bash <file>`. Running a script file stays allowed.\n"
    "  This check is a plain text match, so it also refuses a command that only MENTIONS `{shell} -c`\n"
    "  (a commit message, an echo): write that text to a file instead, for example `git commit -F <file>`."
)


def verdict(command):
    """Return the shell name when `command` has a shell word followed, anywhere later (across lines
    and separators), by a short-option group containing c; else None. A group ends at the first
    character that is not a letter, digit or -, so -c</dev/null and -c'x' count; --long options do
    not, and neither does a hyphen inside a path or word (/-Users-sample). Nothing is split into
    segments: a line continuation or &> cannot hide the pair."""
    for m in SHELL_WORD.finditer(command):
        for o in OPTION_WORD.finditer(command, m.end()):
            if re.fullmatch(r"-(?!-)[A-Za-z0-9-]+", o.group(0)) and "c" in o.group(0):
                return m.group(1)
    return None


def refusal(command):
    """The refusal text for `command`, or None to let it through. Never raises."""
    try:
        shell = verdict(command)
    except Exception:
        return None
    return MESSAGE.format(shell=shell) if shell else None


def main(argv):
    try:
        payload = json.loads(sys.stdin.read())
        command = payload.get("tool_input", {}).get("command", "")
        if payload.get("tool_name") != "Bash" or not isinstance(command, str):
            return 0
    except Exception:
        return 0
    text = refusal(command)
    if text:
        sys.stderr.write(text + "\n(hook: scripts/hooks/guard-no-shell-c.sh)\n")
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
