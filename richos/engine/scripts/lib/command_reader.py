"""Read command words for G10 guards. Never execute or expand shell input.

Use the engine's shell tokenizer for comments, heredocs and substitutions.
Wrappers and Git global options are consumed before inspecting a command's verb.
This is a literal command reader, not a shell evaluator.
"""
import os
import re
import shlex

from host_sleep import _OPTS_WITH_ARG, _POSITIONAL_VALUE, WRAPPERS, heredocs
from unguarded_rm import tokenize

ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
SHELLS = {"bash", "sh", "zsh", "dash", "ksh"}
GIT_VALUES = {"-C", "-c", "--git-dir", "--work-tree", "--namespace", "--config-env", "--exec-path"}


def _words(raw):
    """Unquote words and remove redirects, including redirects joined to words."""
    result = []
    skip = False
    for word in raw:
        lex = shlex.shlex(word, posix=True, punctuation_chars="<>")
        lex.whitespace_split = True
        lex.commenters = ""
        parts = list(lex)
        for part in parts:
            if skip:
                skip = False
            elif part and set(part) <= set("<>") and part in word and not word.startswith(("'", '"')):
                if result and (result[-1].isdigit() or result[-1] == "&"):
                    result.pop()
                elif result and result[-1].endswith("&"):
                    result[-1] = result[-1][:-1]
                skip = part != "<<"  # tokenizer already consumed the heredoc delimiter
            else:
                result.append(part)
    return result


def _unwrap(words):
    i = 0
    while i < len(words):
        word = words[i]
        if ASSIGNMENT.match(word):
            i += 1
            continue
        base = os.path.basename(word)
        if base not in WRAPPERS:
            break
        i += 1
        values = _OPTS_WITH_ARG.get(base, set())
        if base == "sudo":
            values = values | {"--user", "--group", "--host", "--prompt", "--close-from",
                               "--chdir", "--chroot", "--command-timeout", "--other-user"}
        while i < len(words):
            word = words[i]
            if base == "command" and word in ("-v", "-V"):
                return []  # lookup only
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
                i += 2 if word in values else 1
            else:
                break
        if base in _POSITIONAL_VALUE:
            i += 1
    return words[i:]


def _shell_stdin(argv):
    i = 1
    while i < len(argv):
        option = argv[i]
        if option == "--":
            return i + 1 == len(argv)
        if not option.startswith("-") or option == "-":
            return False
        if not option.startswith("--"):
            if "c" in option:
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
