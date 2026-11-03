"""Read command words for G10 guards. Never execute or expand shell input.

Use the engine's shell tokenizer for comments, heredocs and substitutions.
Wrappers and Git global options are consumed before inspecting a command's verb.
This is a literal command reader, not a shell evaluator.
"""
import os
import re
import shlex

from host_sleep import _OPTS_WITH_ARG, _POSITIONAL_VALUE, WRAPPERS, heredocs
from unguarded_rm import tokenize, _match_paren

ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
SHELLS = {"bash", "sh", "zsh", "dash", "ksh"}
GIT_VALUES = {"-C", "-c", "--git-dir", "--work-tree", "--namespace", "--config-env", "--exec-path"}
COMMAND_WRAPPERS = WRAPPERS | {"flock", "watch", "arch", "unbuffer", "script", "parallel"}
EXTRA_VALUES = {
    "sudo": {"--user", "--group", "--host", "--prompt", "--close-from", "--chdir",
             "--chroot", "--command-timeout", "--other-user"},
    "flock": {"-w", "--timeout", "-E", "--conflict-exit-code"},
    "watch": {"-n", "--interval"},
    "script": {"-t"},
    "parallel": {"-j", "--jobs", "-a", "--arg-file", "-S", "--sshlogin", "-I", "--replace"},
}
SSH_VALUES = {"-B", "-b", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L",
              "-l", "-m", "-O", "-o", "-p", "-Q", "-R", "-S", "-W", "-w"}


def _consumes_next(option, values):
    if option in values:
        return True
    if option.startswith("--"):
        return False
    for index, flag in enumerate(option[1:], 1):
        if "-" + flag in values:
            return index == len(option) - 1
    return False


def ssh_command(argv):
    """The remote shell text, for the shell rule only; remote Git is not local Git."""
    i = 1
    while i < len(argv) and argv[i].startswith("-"):
        if argv[i] == "--":
            i += 1
            break
        i += 2 if _consumes_next(argv[i], SSH_VALUES) else 1
    return " ".join(argv[i + 1:])  # skip the destination


def _redirect_parts(word):
    """Split only unquoted redirects, keeping literal words and substitutions intact."""
    start, i, quote = 0, 0, None
    while i < len(word):
        char = word[i]
        if char == "\\" and quote != "'":
            i += 2
            continue
        if quote == "'":
            if char == quote:
                quote = None
        elif word.startswith("$(", i) or (quote is None and word[i:i + 2] in ("<(", ">(")):
            i = _match_paren(word, i)
            continue
        elif char == "`":
            i += 1
            while i < len(word) and word[i] != "`":
                i += 2 if word[i] == "\\" else 1
        elif quote:
            if char == quote:
                quote = None
        elif char in "'\"":
            quote = char
        elif char in "<>":
            if start < i:
                yield False, word[start:i]
            end = i + 1
            while end < len(word) and word[end] in "<>":
                end += 1
            if end < len(word) and word[end] == "&":
                end += 1
            yield True, word[i:end]
            start = i = end
            continue
        i += 1
    if start < len(word):
        yield False, word[start:]


def _words(raw):
    """Unquote words and remove redirects, including redirects joined to words."""
    result = []
    skip = False
    for word in raw:
        attached = None
        for redirect, part in _redirect_parts(word):
            if skip:
                skip = False
            elif redirect:
                if attached and result and (attached.isdigit() or attached == "&"):
                    result.pop()
                elif attached and result and attached.endswith("&") and not attached.endswith("\\&"):
                    result[-1] = result[-1][:-1]
                skip = part != "<<"  # tokenizer already consumed the heredoc delimiter
            else:
                result.extend(shlex.split(part))
            attached = None if redirect else part
    return result


def _unwrap(words):
    i = 0
    while i < len(words):
        word = words[i]
        if ASSIGNMENT.match(word):
            i += 1
            continue
        base = os.path.basename(word)
        if base not in COMMAND_WRAPPERS:
            break
        i += 1
        values = _OPTS_WITH_ARG.get(base, set()) | EXTRA_VALUES.get(base, set())
        while i < len(words):
            word = words[i]
            if base == "command" and word in ("-v", "-V"):
                return []  # lookup only
            if base == "script" and word in ("-c", "--command"):
                return _unwrap(shlex.split(words[i + 1]) + words[i + 2:])
            if base == "env" and (word in ("-S", "--split-string") or word.startswith("--split-string=")):
                value = word.split("=", 1)[1] if "=" in word else words[i + 1]
                rest = words[i + (1 if "=" in word else 2):]
                return _unwrap(shlex.split(value) + rest)
            if base == "env" and ASSIGNMENT.match(word):
                i += 1
            elif word == "--":
                i += 1
                break
            elif word.startswith("-") and word != "-":
                i += 2 if _consumes_next(word, values) else 1
            else:
                break
        if base in _POSITIONAL_VALUE or base in ("flock", "script"):
            i += 1
    return words[i:]


def _shell_stdin(argv):
    i = 1
    while i < len(argv):
        option = argv[i]
        if option == "--":
            return i + 1 == len(argv)
        if not option.startswith(("-", "+")) or option in ("-", "+"):
            return False
        if not option.startswith("--"):
            if option.startswith("-") and "c" in option:
                return False
            if "s" in option:
                return True
        i += 2 if option in ("--rcfile", "--init-file") or option.endswith(("o", "O")) else 1
    return True


def commands(text, depth=0):
    """Yield argv for literal commands and executable substitutions, not printed text."""
    if depth > 8:
        return
    raw_commands, nested = tokenize(text)
    for raw in raw_commands:
        try:
            argv = _unwrap(_words(raw))
        except (ValueError, IndexError):
            continue
        if not argv:
            continue
        yield argv
        if os.path.basename(argv[0]) == "eval":
            yield from commands(" ".join(argv[1:]), depth + 1)
        if os.path.basename(argv[0]) == "find":
            for k, arg in enumerate(argv):
                if arg in ("-exec", "-execdir", "-ok", "-okdir"):
                    end = next((j for j in range(k + 1, len(argv)) if argv[j] in (";", "+")), len(argv))
                    try:
                        child = _unwrap(argv[k + 1:end])
                    except (ValueError, IndexError):
                        continue
                    if child:
                        yield child
    for body in nested:
        yield from commands(body, depth + 1)
    # Here-document data is executable only when it is stdin to a shell.
    for head, body in heredocs(text):
        for raw in tokenize(head)[0]:
            try:
                argv = _unwrap(_words(raw))
            except (ValueError, IndexError):
                continue
            if argv and os.path.basename(argv[0]) in SHELLS and _shell_stdin(argv):
                yield from commands(body, depth + 1)


def git_commands(text):
    """Yield (subcommand, arguments) only for Git command words."""
    for argv in commands(text):
        if os.path.basename(argv[0]) != "git":
            continue
        i = 1
        while i < len(argv) and argv[i].startswith("-"):
            if argv[i] == "--":
                i += 1
                break
            i += 2 if argv[i] in GIT_VALUES else 1
        if i < len(argv):
            yield argv[i], argv[i + 1:]
