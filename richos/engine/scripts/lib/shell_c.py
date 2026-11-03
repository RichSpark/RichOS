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
from command_reader import commands, SHELLS

MESSAGE = (
    "=== guard-no-shell-c: BLOCKED ===\n"
    "  This command contains `{shell} -c` (an inline shell script). Claude Code cannot check an inline\n"
    "  shell script, so it stops and ASKS THE USER ('This shell -c script ... could not be checked. Do\n"
    "  you want to proceed?'). The user is never the fallback check.\n"
    "  FIX: run the steps as separate commands (one step per Bash call), or write the script to a file\n"
    "  in your scratch folder and run `bash <file>`. Running a script file stays allowed.\n"

)


def verdict(command):
    """Return the shell name for an inline script, otherwise None."""
    for argv in commands(command):
        name = os.path.basename(argv[0])
        if name not in SHELLS:
            continue
        i = 1
        while i < len(argv):
            option = argv[i]
            if option == "--" or not option.startswith("-") or option == "-":
                break  # subsequent words are the script's arguments
            if option.startswith("--"):
                i += 2 if option in ("--rcfile", "--init-file") else 1
                continue
            if "c" in option:
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
