"""Read a pending terminal handoff without running the author's shell command."""

from dataclasses import dataclass
import os
from pathlib import Path
import re
import shlex

from command_reader import _redirect_parts, commands
from host_sleep import heredocs
from unguarded_rm import tokenize


DEFAULT_OUTBOX = "~/.richos-coordination/rich-codex/to-codex.md"
BRIEF = re.compile(r"""(?<![\w./-])(?:[~/][^\s`"'<>()[\]]*/)?(?:[\w.-]+/)*docs/briefs/[^\s`"'<>()[\]]+?\.md\b""")
ENTRY = re.compile(r"^## \d{4}-\d{2}-\d{2}T\S+[ \t]+", re.M)


@dataclass(frozen=True)
class Handoff:
    path: Path
    text: str
    shell: bool = False

    def status_only(self):
        if self.shell or BRIEF.search(self.text):
            return False
        headings = list(ENTRY.finditer(self.text))
        return (bool(headings) and not self.text[:headings[0].start()].strip()
                and all(self.text.startswith("STATUS:", heading.end()) for heading in headings))


def path_at(word, cwd):
    # Expand only the conventional home spelling. Shell code is never evaluated.
    word = re.sub(r"^\$(?:HOME(?=/)|\{HOME\}(?=/))", lambda _: str(Path.home()), word)
    return (cwd / Path(word).expanduser()).resolve()


def shell_targets(raw, cwd):
    """Find literal output redirections, including ones attached to a word."""
    redirect = None
    for word in raw:
        for operator, part in _redirect_parts(word):
            if operator:
                redirect = part
            elif redirect:
                if redirect in (">", ">>") and "$(" not in part and "`" not in part:
                    for value in shlex.split(part):
                        yield path_at(value, cwd)
                redirect = None


def shell_handoff(command, cwd, outbox, depth=0):
    if outbox.name not in command or depth > 8:
        return None
    rows, nested = tokenize(command)
    # For the usual quoted cat heredoc, review exactly the Markdown it writes.
    if rows and not nested and command.count(outbox.name) == 1:
        documents = heredocs(command)
        if len(documents) == 1:
            head, body = documents[0]
            header_rows, _ = tokenize(head)
            argv = list(commands(head))
            quoted = re.search(re.escape(head) + r"<<-?[ \t]*(['\"])[A-Za-z_][A-Za-z0-9_]*\1", command)
            if (header_rows and argv and argv[-1] == ["cat"]
                    and outbox in shell_targets(header_rows[-1], cwd)
                    and (quoted or not re.search(r"[$`\\]", body))):
                return Handoff(outbox, body)
    if any(outbox in shell_targets(row, cwd) for row in rows):
        return Handoff(outbox, command, shell=True)
    for argv in commands(command):
        if Path(argv[0]).name == "tee" and any(
                path_at(word, cwd) == outbox for word in argv[1:] if not word.startswith("-")):
            return Handoff(outbox, command, shell=True)
        if Path(argv[0]).name in ("bash", "sh", "zsh", "eval"):
            # Nested shell redirects are already visible in the tokenizer for
            # substitutions, but a literal -c argument is an ordinary word.
            script = (" ".join(argv[1:]) if argv[0] == "eval" else
                      argv[argv.index("-c") + 1] if "-c" in argv[:-1] else "")
            if script and script != command and shell_handoff(script, cwd, outbox, depth + 1):
                return Handoff(outbox, command, shell=True)
    if any(shell_handoff(body, cwd, outbox, depth + 1) for body in nested):
        return Handoff(outbox, command, shell=True)
    # The lead also uses Python's literal open(path, mode). Read-only mentions
    # of the channel in commands and in shell output are common.
    for match in re.finditer(
            r"""\bopen\(\s*(?:os\.path\.expanduser\(\s*)?(['"])([^'"\n]+)\1\s*\)?\s*,\s*['"][awx][bt+]*['"]""",
            command):
        if path_at(match[2], cwd) == outbox:
            return Handoff(outbox, command, shell=True)
    return None


def pending_handoff(payload):
    tool = payload.get("tool_name")
    if tool not in ("Bash", "Write", "Edit", "MultiEdit"):
        return None
    # This is the terminal coordination channel. The desktop hook has a
    # separate Agent dispatcher and company-scoped read roots.
    if os.environ.get("RICHOS_APP_STATE"):
        return None
    cwd = Path(payload.get("cwd") or os.getcwd())
    outbox = path_at(os.environ.get("RICHOS_PIERCE_CODEX_OUTBOX", DEFAULT_OUTBOX), cwd)
    data = payload.get("tool_input") or {}
    if tool == "Bash":
        handoff = shell_handoff(data.get("command", ""), cwd, outbox)
    elif data.get("file_path") and path_at(data["file_path"], cwd) == outbox:
        if tool == "Write":
            text = data["content"]
            # Count only new content in a Write that preserves the whole file.
            if outbox.is_file() and text.startswith(previous := outbox.read_text()):
                text = text[len(previous):]
        elif tool == "Edit":
            old, text = data["old_string"], data["new_string"]
            if text.startswith(old):
                text = text[len(old):]
        else:
            text = "\n".join(edit["new_string"] for edit in data["edits"])
        handoff = Handoff(outbox, text)
    else:
        return None
    return handoff if handoff and handoff.text.strip() and not handoff.status_only() else None
