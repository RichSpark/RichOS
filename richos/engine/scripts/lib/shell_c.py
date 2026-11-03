#!/usr/bin/env python3
"""shell_c.py: refuse, BEFORE Claude Code asks the user, a Bash command that wraps an inline script
in `bash -c`, `sh -c` or `zsh -c`.

The CEO, 2026-10-09, saw "This shell -c script runs rm and could not be checked. Do you want to
proceed?" for a teammate's command, hit yes, and wrote: "I hope RichOS app users won't be bothered by
this." (The same prompt reached him on 2026-10-07 from the lead's own commands.) Claude Code cannot
check an inline shell script, so it asks the person. A PreToolUse refusal runs first, so the agent
gets the rewrite and the person never sees a prompt.

Refuse actual shell command words with a -c option before their script operand.
Printed text, comments, heredoc data and options belonging to another command pass.
Like the other shell guards, this reads literal syntax without evaluating it.
Shared by the operator-install rule (scripts/hooks/guard-no-shell-c.sh) and by the RichOS app's hook
(mega-lander/app.py validate_shell_target).
"""
import json
import re
import sys

import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from command_reader import commands, SHELLS, GIT_VALUES, ssh_command

TEXT_COMMANDS = {"echo", "printf", "grep", "egrep", "fgrep", "rg", "true", ":"}
SHELL_WORD = re.compile(r"(?<![\w.\-])(bash|sh|zsh|dash|ksh)(?![\w.\-])")
OPTION_WORD = re.compile(r"(?<![^\s'\"`;|&(){}<>])-(?!-)[A-Za-z0-9-]+")
GIT_DATA = {"-m", "--message", "-F", "--file", "--author", "--date", "--trailer",
            "--grep", "-G", "-S"}


def _git_action_args(argv):
    """Remove positively identified message/search data, retaining other arguments."""
    index = 1
    while index < len(argv) and argv[index].startswith("-"):
        index += 2 if argv[index] in GIT_VALUES else 1
    if index >= len(argv) or argv[index] not in {"commit", "merge", "tag", "notes", "log", "show", "grep", "diff"}:
        return argv
    if index < len(argv) and argv[index] == "grep" and not any(
            arg.startswith("--open-files-in-pager") for arg in argv[index + 1:]):
        return argv[:index]  # the built-in search consumes patterns as data
    out, skip = [], False
    for arg in argv:
        if skip:
            skip = False
        elif arg in GIT_DATA:
            skip = True
        elif arg.split("=", 1)[0] in GIT_DATA and "=" in arg:
            continue
        elif re.match(r"^-[A-Za-z]*?m", arg):
            skip = arg.endswith("m")  # -am TEXT or -mTEXT
        else:
            out.append(arg)
    return out


def _unknown_shell(argv):
    """Keep the old conservative refusal where an action cannot be classified.

    A new wrapper must not become a bypass. Only known print/search operations
    and message operands earn the mention exemption.
    """
    text = " ".join(argv)
    for shell in SHELL_WORD.finditer(text):
        if any("c" in option.group(0) for option in OPTION_WORD.finditer(text, shell.end())):
            return shell.group(1)
    return None

MESSAGE = (
    "=== guard-no-shell-c: BLOCKED ===\n"
    "  This command contains `{shell} -c` (an inline shell script). Claude Code cannot check an inline\n"
    "  shell script, so it stops and ASKS THE USER ('This shell -c script ... could not be checked. Do\n"
    "  you want to proceed?'). The user is never the fallback check.\n"
    "  FIX: run the steps as separate commands (one step per Bash call), or write the script to a file\n"
    "  in your scratch folder and run `bash <file>`. Running a script file stays allowed.\n"

)


def verdict(command, depth=0):
    """Return the shell name for an inline script, otherwise None."""
    if depth > 8:
        return None
    for argv, environment in commands(command, with_environment=True):
        name = os.path.basename(argv[0])
        mention = name in TEXT_COMMANDS and not (
            name == "rg" and any(arg.startswith("--pre") for arg in argv[1:]))
        if not mention:
            unknown = _unknown_shell(environment)
            if unknown:
                return unknown
        if name == "ssh":
            nested = verdict(ssh_command(argv), depth + 1)
            if nested:
                return nested
            continue
        if name not in SHELLS:
            if name == "eval" or mention:
                continue
            unknown = _unknown_shell(_git_action_args(argv) if name == "git" else argv)
            if unknown:
                return unknown
            continue
        i = 1
        while i < len(argv):
            option = argv[i]
            if option == "--" or not option.startswith(("-", "+")) or option in ("-", "+"):
                break  # subsequent words are the script's arguments
            if option.startswith("--"):
                i += 2 if option in ("--rcfile", "--init-file") else 1
                continue
            if option.startswith("-") and "c" in option:
                return name
            i += 2 if option.endswith(("o", "O")) else 1
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
