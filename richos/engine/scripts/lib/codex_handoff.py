"""Read a pending terminal handoff without running the author's shell command."""

import ast
from collections import defaultdict, deque
from dataclasses import dataclass
import os
from pathlib import Path
import re
import shlex

from codex_channels import rich_task
from command_reader import ASSIGNMENT, SHELLS, _redirect_parts, _unwrap, _words
from host_sleep import heredocs
from unguarded_rm import tokenize


DEFAULT_OUTBOX = "~/.richos-coordination/rich-codex/to-codex.md"
BRIEF = re.compile(r"""(?<![\w./-])(?:[~/][^\s`"'<>()[\]]*/)?(?:[\w.-]+/)*docs/briefs/[^\s`"'<>()[\]]+?\.md\b""")
ENTRY = re.compile(r"^## \d{4}-\d{2}-\d{2}T\S+[ \t]+", re.M)
SHELL_VARIABLE = re.compile(r"\$\{([A-Za-z_]\w*)\}|\$([A-Za-z_]\w*)")


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
    return (cwd / Path(word).expanduser()).resolve()


def shell_value(word, variables):
    """Resolve simple, previously assigned shell variables as literal data."""
    word = SHELL_VARIABLE.sub(lambda match: variables.get(match[1] or match[2], match[0]), word)
    if "$" in word or "`" in word:
        return None
    return word


def shell_path(word, cwd, variables):
    value = shell_value(word, variables)
    return path_at(value, cwd) if value is not None else None


def shell_targets(raw, cwd, variables):
    """Find output redirections, including ones attached to a word."""
    redirect = None
    for word in raw:
        for operator, part in _redirect_parts(word):
            if operator:
                redirect = part
            elif redirect:
                if redirect in (">", ">>"):
                    for value in shlex.split(part):
                        yield shell_path(value, cwd, variables)
                redirect = None


def python_name(node):
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        return python_name(node.value) + "." + node.attr
    return ""


def python_path(node, variables):
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return Path(node.value)
    if isinstance(node, ast.Name):
        return variables.get(node.id)
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Div):
        left, right = python_path(node.left, variables), python_path(node.right, variables)
        return left / right if left is not None and right is not None else None
    if isinstance(node, ast.Call):
        name = python_name(node.func)
        if name in ("Path.home", "pathlib.Path.home"):
            return Path.home()
        if name in ("Path", "pathlib.Path", "os.path.expanduser") and node.args:
            return python_path(node.args[0], variables)
    return None


def python_writes(source, cwd, outbox):
    """Recognize open and Path writes in inline Python without executing it."""
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return False
    variables = {}
    operations = sorted((node for node in ast.walk(tree) if isinstance(node, (ast.Assign, ast.Call))),
                        key=lambda node: (node.lineno, node.col_offset))
    for node in operations:
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name):
                    variables[target.id] = python_path(node.value, variables)
            continue
        mode = None
        if python_name(node.func) in ("open", "io.open") and node.args:
            path = python_path(node.args[0], variables)
            mode = node.args[1] if len(node.args) > 1 else None
        elif isinstance(node.func, ast.Attribute) and node.func.attr in ("open", "write_text", "write_bytes"):
            path = python_path(node.func.value, variables)
            if node.func.attr != "open" and path is not None and path_at(path, cwd) == outbox:
                return True
            mode = node.args[0] if node.args else None
        else:
            continue
        mode = next((arg.value for arg in node.keywords if arg.arg == "mode"), mode)
        if (isinstance(mode, ast.Constant) and isinstance(mode.value, str) and set(mode.value) & set("awx+")
                and path is not None and path_at(path, cwd) == outbox):
            return True
    return False


def documents_by_command(command):
    documents = defaultdict(deque)
    for head, body in heredocs(command):
        rows, _ = tokenize(head)
        if rows:
            key = (*rows[-1], "<<")
            quoted_head = re.compile(re.escape(head) + r"<<-?[ \t]*(['\"])[A-Za-z_][A-Za-z0-9_]*\1[^\n]*\n")
            quoted = any(command.startswith(body, match.end()) for match in quoted_head.finditer(command))
            documents[key].append((body, quoted or not re.search(r"[$`\\]", body)))
    return documents


def shell_handoff(command, cwd, outbox, depth=0, inherited=None):
    inherited = inherited or {}
    if depth > 8 or (outbox.name not in command and "dispatch" not in command and
                     not any(outbox.name in value for value in inherited.values())):
        return None
    rows, nested = tokenize(command)
    documents = documents_by_command(command)
    variables = {"HOME": str(Path.home()), **inherited}
    entries = []
    for row in rows:
        words = _words(row)
        for word in words[1:] if words[:1] == ["export"] else words:
            if not ASSIGNMENT.match(word):
                break
            name, value = word.split("=", 1)
            resolved = shell_value(value, variables)
            if resolved is None:
                variables.pop(name, None)
            else:
                variables[name] = resolved
        argv = _unwrap(words)
        program = Path(argv[0]).name if argv else ""
        if program in SHELLS and len(argv) > 1 and Path(argv[1]).name == "codex-channel.sh":
            argv, program = argv[1:], "codex-channel.sh"
        if program == "codex-channel.sh" and argv[1:3] == ["dispatch", "--locator"] and len(argv) == 4:
            locator = shell_path(argv[3], cwd, variables)
            if locator is not None:
                try:
                    path, task = rich_task(locator)
                except (OSError, ValueError):
                    pass  # The channel helper itself refuses a missing or invalid task.
                else:
                    return Handoff(path, task["text"])
        group = documents.get(tuple(row))
        body, literal = group.popleft() if group else ("", False)
        if outbox in shell_targets(row, cwd, variables):
            if program == "cat" and len(argv) == 1 and literal:
                entries.append(body)
            else:
                return Handoff(outbox, command, shell=True)
        if program == "tee" and any(shell_path(word, cwd, variables) == outbox for word in argv[1:]):
            return Handoff(outbox, command, shell=True)
        inline = argv[argv.index("-c") + 1] if "-c" in argv[:-1] else ""
        if re.fullmatch(r"python(?:3(?:\.\d+)?)?", program) and python_writes(body or inline, cwd, outbox):
            return Handoff(outbox, command, shell=True)
        if program in SHELLS or program == "eval":
            script = " ".join(argv[1:]) if program == "eval" else body or inline
            if shell_handoff(script, cwd, outbox, depth + 1, variables):
                return Handoff(outbox, command, shell=True)
        if program == "cd" and len(argv) == 2:
            cwd = shell_path(argv[1], cwd, variables) or cwd
    if any(shell_handoff(body, cwd, outbox, depth + 1, variables) for body in nested):
        return Handoff(outbox, command, shell=True)
    if entries:
        return Handoff(outbox, "\n".join(entries))
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
