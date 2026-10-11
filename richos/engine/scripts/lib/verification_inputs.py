"""Parse verification inputs as data. Never source config or run hook commands.

The supported shell subset is deliberately finite: one assignment per line,
quoted/bare strings and parameter paths. Unsupported syntax carries a reason
for conservative selection instead of inventing an empty dependency set.
"""
import json
import math
import hashlib
import os
from pathlib import Path
import re
import subprocess
import shlex

ENVIRONMENT = {"HOME", "TMPDIR", "CLAUDE_CONFIG_DIR"}
NAME = re.compile(r"[A-Za-z_][A-Za-z_0-9]*")
ASSIGNMENT = re.compile(r"^\s*([A-Za-z_][A-Za-z_0-9]*)=(.*)$")


class Unsupported(ValueError):
    pass


class Snapshot:
    """An explicit committed, staged or working version of a selector input."""
    def __init__(self, root, revision):
        self.root = Path(root)
        self.revision = revision
        repository = subprocess.run(["git", "-C", str(root), "rev-parse", "--git-dir"], capture_output=True)
        if repository.returncode:
            raise Unsupported("cannot read Git checkout: " + str(root))
        if revision not in ("WORKTREE", "INDEX"):
            result = subprocess.run(["git", "-C", str(root), "rev-parse", "--verify", revision + "^{commit}"],
                                    capture_output=True, text=True)
            if result.returncode:
                raise Unsupported("cannot resolve input revision: " + revision)
            self.revision = result.stdout.strip()

    def read(self, path):
        path = Path(path)
        if path.is_absolute() or ".." in path.parts:
            raise Unsupported("input path must be repository-relative: " + str(path))
        if self.revision == "WORKTREE":
            try:
                return (self.root / path).read_text(errors="surrogateescape")
            except FileNotFoundError:
                return None
        spec = ("" if self.revision == "INDEX" else self.revision) + ":" + path.as_posix()
        inventory = (["ls-files", "--stage", "-z", "--", path.as_posix()] if self.revision == "INDEX"
                     else ["ls-tree", "-z", self.revision, "--", path.as_posix()])
        exists = subprocess.run(["git", "-C", str(self.root), *inventory], capture_output=True)
        if exists.returncode:
            raise Unsupported("cannot inspect input inventory: " + spec)
        if not exists.stdout:
            return None
        return subprocess.check_output(["git", "-C", str(self.root), "show", spec], text=True, errors="surrogateescape")


def revisions(root, range_text):
    """Match Git's two-dot/three-dot diff semantics before reading assignments."""
    if "..." in range_text:
        left, right = range_text.split("...", 1)
        base = subprocess.run(["git", "-C", str(root), "merge-base", left or "HEAD", right or "HEAD"],
                              capture_output=True, text=True)
        if base.returncode:
            raise Unsupported("cannot resolve merge base: " + range_text)
        return base.stdout.strip(), right or "HEAD"
    if ".." in range_text:
        left, right = range_text.split("..", 1)
        return left or "HEAD", right or "HEAD"
    return range_text + "^1", range_text


class Value:
    def __init__(self, text, names):
        self.text, self.names, self.position = text, names, 0
        self.references = set()

    def parse(self, end=None):
        parts, literal = [], []

        def flush():
            if literal:
                parts.append(("text", "".join(literal)))
                literal.clear()

        while self.position < len(self.text):
            ch = self.text[self.position]
            if ch == end:
                self.position += 1
                flush()
                return tuple(parts)
            if end is None and ch.isspace():
                break
            if ch == "`":
                raise Unsupported("backtick execution")
            if ch == "\\":
                self.position += 1
                if self.position == len(self.text):
                    raise Unsupported("multiline escape")
                escaped = self.text[self.position]
                if end == '"' and escaped not in '$`"\\':
                    literal.append("\\")
                literal.append(escaped)
                self.position += 1
            elif ch == "$":
                flush()
                parts.append(self.parameter())
            elif ch == "'" and end != '"':
                last = self.text.find("'", self.position + 1)
                if last < 0:
                    raise Unsupported("multiline or unclosed single quote")
                literal.append(self.text[self.position + 1:last])
                self.position = last + 1
            elif ch == '"' and end != '"':
                self.position += 1
                # Merge quoted literals with adjacent bare literals so changing
                # only quoting cannot manufacture a changed value.
                for kind, *data in self.parse('"'):
                    if kind == "text":
                        literal.append(data[0])
                    else:
                        flush()
                        parts.append((kind, *data))
            elif ch in ";|&()<>" and end != '"':
                raise Unsupported("shell control or command substitution")
            elif ch in "{}" and end is None:
                raise Unsupported("unsupported brace syntax")
            else:
                literal.append(ch)
                self.position += 1
        if end is not None:
            raise Unsupported("multiline or unclosed " + end)
        flush()
        rest = self.text[self.position:].strip()
        if rest and not rest.startswith("#"):
            raise Unsupported("multiple words or executable syntax after assignment")
        return tuple(parts)

    def parameter(self):
        self.position += 1
        braced = self.text[self.position:self.position + 1] == "{"
        if braced:
            self.position += 1
        match = NAME.match(self.text, self.position)
        if match is None:
            raise Unsupported("command, arithmetic or unsupported parameter expansion")
        name = match.group()
        self.position = match.end()
        if name not in ENVIRONMENT | self.names:
            raise Unsupported("undeclared environment or config reference: " + name)
        if name in self.names:
            self.references.add(name)
        default = None
        if braced:
            if self.text[self.position:self.position + 1] == "}":
                self.position += 1
            elif self.text[self.position:self.position + 2] == ":-":
                self.position += 2
                default = self.parse("}")
            else:
                raise Unsupported("unsupported expansion of " + name)
        return ("parameter", name, default)


def config(text):
    assignments = {}
    for number, line in enumerate(text.splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        match = ASSIGNMENT.fullmatch(line)
        if not match:
            raise Unsupported(f"line {number}: not a single assignment")
        name, value = match.groups()
        if name in assignments:
            raise Unsupported(f"line {number}: duplicate assignment {name}")
        assignments[name] = (number, value)
    result = {}
    for name, (number, value) in assignments.items():
        parser = Value(value, set(assignments))
        try:
            parsed = parser.parse()
            if any(assignments[reference][0] >= number for reference in parser.references):
                raise Unsupported("forward/self key reference would read undeclared inherited environment")
        except Unsupported as exc:
            raise Unsupported(f"line {number}, {name}: {exc}") from None
        result[name] = {"value": parsed, "references": parser.references}
    return result


def config_change(before, after):
    result = {"content": before != after, "presence": (before is None) != (after is None),
              "keys": [], "fallback": None}
    try:
        old, new = config(before or ""), config(after or "")
    except Unsupported as exc:
        result["fallback"] = str(exc)
        return result
    names = set(old) | set(new)
    changed = {name for name in names if old.get(name) != new.get(name)}
    # A value inherited through another key is an input even if that assignment
    # itself did not change. Follow both versions, including deleted aliases.
    while True:
        expanded = changed | {name for name in names if
            (old.get(name, {}).get("references", set()) | new.get(name, {}).get("references", set())) & changed}
        if expanded == changed:
            break
        changed = expanded
    result["keys"] = sorted(changed)
    return result


def hook_entries(text, settings=False):
    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('duplicate object key: ' + key)
            result[key] = value
        return result

    def invalid_constant(value):
        raise ValueError('non-JSON numeric constant: ' + value)

    try:
        document = json.loads(text, object_pairs_hook=unique_object,
                              parse_constant=invalid_constant)
    except (TypeError, ValueError) as exc:
        raise Unsupported("hooks JSON is invalid: " + str(exc)) from None
    if (not isinstance(document, dict) or (not settings and set(document) - {"description", "hooks"})
            or not isinstance(document.get("hooks"), dict)):
        raise Unsupported("unknown hooks document structure")
    entries = {}
    for event, groups in document["hooks"].items():
        if not isinstance(groups, list):
            raise Unsupported("hook event is not an ordered group list: " + event)
        for group_index, group in enumerate(groups):
            if (not isinstance(group, dict) or set(group) - {"matcher", "hooks"}
                    or not isinstance(group.get("hooks"), list)
                    or not isinstance(group.get("matcher", ""), str)):
                raise Unsupported("unknown hook group structure: " + event)
            for index, command in enumerate(group["hooks"]):
                if (not isinstance(command, dict) or command.get("type") != "command"
                        or set(command) - {"type", "command", "timeout", "async", "statusMessage"}
                        or not isinstance(command.get("command"), str)):
                    raise Unsupported("unknown hook command structure: " + event)
                if ("timeout" in command and type(command["timeout"]) not in (int, float)
                        or isinstance(command.get("timeout"), float) and not math.isfinite(command["timeout"])
                        or "async" in command and type(command["async"]) is not bool
                        or "statusMessage" in command and not isinstance(command["statusMessage"], str)):
                    raise Unsupported("unsupported hook command metadata: " + event)
                entries[(event, group_index, index)] = {
                    "matcher": group.get("matcher"), **command}
    return entries


def hooks_change(before, after):
    result = {"content": before != after, "commands": [], "events": [], "metadata": False, "fallback": None}
    try:
        old, new = hook_entries(before), hook_entries(after)
    except Unsupported as exc:
        result["fallback"] = str(exc)
        return result
    changed = {key for key in old.keys() | new.keys() if old.get(key) != new.get(key)}
    result["commands"] = sorted({version[key]["command"] for key in changed
                                  for version in (old, new) if key in version})
    result["metadata"] = json.loads(before)["hooks"] != json.loads(after)["hooks"]
    old_events, new_events = json.loads(before)["hooks"], json.loads(after)["hooks"]
    result["events"] = sorted(event for event in old_events.keys() | new_events.keys()
                              if old_events.get(event) != new_events.get(event))
    return result


def hook_command_path(command):
    """Resolve the shipped literal command grammar without executing the shell.

    Inline echo hooks have no script consumer. Shell pipelines, substitutions
    and dynamic arguments retain a named conservative fallback.
    """
    if re.fullmatch(r"echo\s+'[^']*'", command):
        return None
    try:
        words = shlex.split(command)
    except ValueError as exc:
        raise Unsupported('unparseable hook command: ' + str(exc)) from None
    if (len(words) < 2 or words[0] not in ('bash', 'python3')
            or not re.fullmatch(r'\$\{CLAUDE_PLUGIN_ROOT\}/[A-Za-z0-9_./+-]+', words[1])
            or any(not re.fullmatch(r'[A-Za-z0-9_./+-]+', arg) for arg in words[2:])):
        raise Unsupported('unsupported hook command: ' + command)
    path = words[1].removeprefix('${CLAUDE_PLUGIN_ROOT}/')
    if '..' in Path(path).parts:
        raise Unsupported('hook command escapes the engine: ' + command)
    return path


def hook_reader(row, read):
    """Validate a reviewed manifest-reader contract against all its helpers."""
    if (not isinstance(row, dict) or not row.get('evidence') or not row.get('sources')
            or not isinstance(row.get('commands'), list)):
        raise Unsupported('hook reader has no qualified command/input contract')
    if 'all_events' in row and type(row['all_events']) is not bool:
        raise Unsupported('hook inventory scope must be a boolean')
    for path, digest in row['sources'].items():
        if Path(path).is_absolute() or '..' in Path(path).parts:
            raise Unsupported('non-relative hook reader: ' + path)
        content = read(path)
        if content is None or hashlib.sha256(content.encode(errors='surrogateescape')).hexdigest() != digest:
            raise Unsupported('changed hook reader requires qualification: ' + path)
    return row


class Dependencies:
    """Content-bound read/call contracts, traversed through fixture transports.

    `edges` are executed/source relationships, not every filename a copy command
    mentions. A transport has a presence dependency and follows its actual
    downstream readers. Replacement fixture configs sever value dependencies;
    individual overrides mask only their named keys. Unqualified readers remain
    visible on their own consumer, never as an invented whole-file read.
    """
    def __init__(self, root, declaration, read=None):
        self.root, self.declaration = Path(root), declaration
        self.read = read or (lambda path: (self.root / path).read_text())
        self.contents = {}
        if declaration.get("schema") != 1:
            raise ValueError("unsupported verification dependency schema")

    def node(self, name):
        row = self.declaration["nodes"].get(name)
        if not row:
            raise Unsupported("unqualified reader " + name)
        source = Path(row["source"])
        if source.is_absolute() or ".." in source.parts:
            raise Unsupported("non-relative dependency " + str(source))
        try:
            if source not in self.contents:
                self.contents[source] = self.read(str(source))
            content = self.contents[source]
            if content is None:
                raise FileNotFoundError(source)
            content = content.encode(errors="surrogateescape")
        except OSError:
            raise Unsupported("missing dependency " + str(source)) from None
        if hashlib.sha256(content).hexdigest() != row["sha256"]:
            raise Unsupported("changed reader requires input qualification: " + str(source))
        if not row.get("evidence"):
            raise Unsupported("reader has no qualification evidence: " + str(source))
        for key, allowed in row.get("config_literals", {}).items():
            if key not in self.declaration["config_keys"] or not isinstance(allowed, list):
                raise Unsupported("invalid config command binding: " + name)
            config_path = Path("orchestration.config")
            if config_path not in self.contents:
                try:
                    self.contents[config_path] = self.read(str(config_path))
                except FileNotFoundError:
                    self.contents[config_path] = None
                except OSError:
                    raise Unsupported("unreadable config command binding: " + key) from None
            values = config(self.contents[config_path] or "")
            value = values.get(key)
            if value is None:
                literal = None
            elif all(part[0] == "text" for part in value["value"]):
                literal = "".join(part[1] for part in value["value"])
            else:
                raise Unsupported("nonliteral config command binding: " + key)
            if literal not in allowed:
                raise Unsupported("changed config command binding: " + key)
        inventory = row.get("source_inventory")
        if inventory is not None:
            if (not isinstance(inventory, dict) or set(inventory) != {"directory", "members"}
                    or not isinstance(inventory["directory"], str)
                    or not isinstance(inventory["members"], list)
                    or any(not isinstance(member, str) or not member or member in (".", "..", "__pycache__")
                           or Path(member).name != member for member in inventory["members"])
                    or len(set(inventory["members"])) != len(inventory["members"])):
                raise Unsupported("invalid source inventory: " + name)
            relative = Path(inventory["directory"])
            if relative.is_absolute() or ".." in relative.parts or not relative.parts:
                raise Unsupported("invalid source inventory directory: " + name)
            try:
                members = list((self.root / relative).iterdir())
                actual = sorted(member.name for member in members if member.name != "__pycache__")
                if any(member.is_symlink() for member in members):
                    raise Unsupported("redirected source inventory: " + name)
            except OSError:
                raise Unsupported("missing source inventory: " + name) from None
            if actual != sorted(inventory["members"]):
                raise Unsupported("changed source inventory: " + name)
        for external in row.get("external", []):
            relative = Path(external.get("path", ""))
            if (external.get("root") not in ("HOME", "environment", "repository") or relative.is_absolute()
                    or ".." in relative.parts or not relative.parts
                    or not external.get("evidence")):
                raise Unsupported("invalid external reader binding: " + name)
            if external["root"] == "HOME":
                base = Path.home()
            elif external["root"] == "repository":
                # The selector root is <repository>/richos/engine.
                base = self.root.parent.parent
            else:
                variable = external.get("variable", "")
                default = external.get("default", "")
                engine_default = external.get("default_engine", False)
                if (not NAME.fullmatch(variable) or not isinstance(default, str)
                        or type(engine_default) is not bool or (engine_default and default)):
                    raise Unsupported("invalid external environment binding: " + name)
                value = os.environ.get(variable)
                base = (self.root if value is None else Path(value)) if engine_default else Path(value or default)
                if not base.is_absolute():
                    raise Unsupported("relative external environment binding: " + name)
            path = base / relative
            if external.get("optional_file") and not path.is_file():
                continue
            # The reviewed fixture takes its literal fallback when this exact
            # optional executable is unavailable. Otherwise bind its bytes.
            if external.get("optional_executable") and not os.access(path, os.X_OK):
                continue
            try:
                digest = hashlib.sha256(path.read_bytes()).hexdigest()
            except OSError:
                raise Unsupported("missing external reader: " + str(path)) from None
            if digest != external.get("sha256"):
                raise Unsupported("changed external reader: " + str(path))
        # Known direct parameter accesses cannot be omitted from the contract.
        # This check is a floor, not a claim of arbitrary shell interpretation.
        text = "\n".join(line for line in content.decode().splitlines() if not line.lstrip().startswith("#"))
        if row.get("region"):
            start, end = row["region"]
            if text.count(start) != 1 or text.count(end) != 1 or text.index(end) <= text.index(start):
                raise Unsupported("dependency region is ambiguous: " + name)
            text = text.split(start, 1)[1].split(end, 1)[0]
        known = set(self.declaration["config_keys"])
        direct = set(re.findall(r"\$\{?([A-Za-z_][A-Za-z_0-9]*)", text)) & known
        declared = set(row.get("keys", []))
        literals = row.get('literal_keys', {})
        if not isinstance(literals, dict) or any(not reason for reason in literals.values()):
            raise Unsupported('literal config-key references require evidence: ' + name)
        if not row.get("whole") and not direct <= declared | set(literals):
            raise Unsupported("omitted known key reads in " + str(source) + ": " + ", ".join(sorted(direct - declared)))
        # A narrow independently derived floor catches concrete shell calls
        # whose repository path is literal. Other aliases/call shapes still
        # require the content-bound review, not a claim of shell interpretation.
        calls = set(re.findall(
            r'\b(?:bash|python3)\s+["\']?\$(?:ENGINE_ROOT|\{ENGINE_ROOT\})/([^"\'\s]+\.(?:sh|py))', text))
        def exists(path):
            try:
                return self.read(path) is not None
            except FileNotFoundError:
                return False
        calls = {path for path in calls if exists(path)}
        targets = {self.declaration["nodes"].get(edge["to"], {}).get("source", edge["to"].split("#")[0])
                   for edge in row.get("edges", [])}
        if not calls <= targets:
            raise Unsupported("omitted known execute edges in " + str(source) + ": "
                              + ", ".join(sorted(calls - targets)))
        for alias, key in row.get("indirect", {}).items():
            assignment = re.search(r"(?m)^\s*" + re.escape(alias) + r"=['\"]?([A-Za-z_][A-Za-z_0-9]*)['\"]?\s*(?:#.*)?$", text)
            if not assignment or assignment[1] != key or key not in declared:
                raise Unsupported("unqualified indirect key " + alias + " in " + str(source))
        if (re.search(r"\beval\s|\$\{!", text) and not row.get("indirect")
                and not row.get("nonconfig_eval")):
            raise Unsupported("unqualified dynamic evaluation in " + str(source))
        return row

    def closure(self, name, active=()):
        """Finite dependency fixed point, preserving override and fixture edges.

        Read each reachable node once per invocation. Keep one witnessed path
        per originating requirement instead of enumerating every route through
        shared readers. Nothing is cached across input observations.
        """
        empty = {"keys": {}, "whole": [], "presence": [], "fallback": []}
        if name in active:
            return empty
        states, parents, pending = {}, {}, [name]
        blocked = set(active)
        while pending:
            current = pending.pop()
            if current in states:
                continue
            states[current] = {}
            if current in blocked:
                continue
            try:
                row = self.node(current)
            except Unsupported as exc:
                states[current][("fallback", "", current)] = str(exc)
                continue
            for key in row.get("keys", []):
                states[current][("keys", key, current)] = current + ": " + row["evidence"]
            for category in ("whole", "presence", "fallback"):
                if row.get(category):
                    states[current][(category, "", current)] = current + ": " + row[category]
            for edge in row.get("edges", []):
                target = edge["to"]
                parents.setdefault(target, []).append((current, edge))
                if target not in states:
                    pending.append(target)
        # New facts propagate to predecessors until no requirement is added.
        # Cycles cannot invent new facts or growing strings of explanations.
        from collections import deque
        queue = deque(states)
        queued = set(states)
        while queue:
            child = queue.popleft()
            queued.discard(child)
            for parent, edge in parents.get(child, []):
                changed = False
                for fact, reason in states[child].items():
                    category, key, _origin = fact
                    if edge.get("fixture") and category != "fallback":
                        continue
                    if category == "keys" and key in edge.get("overrides", []):
                        continue
                    if fact not in states[parent]:
                        states[parent][fact] = parent + " -> " + reason
                        changed = True
                if changed and parent not in queued:
                    queue.append(parent)
                    queued.add(parent)
        for (category, key, _origin), reason in states[name].items():
            if category == "keys":
                empty["keys"].setdefault(key, []).append(reason)
            else:
                empty[category].append(reason)
        return empty

    def config_units(self, change, inventory):
        selected = {}
        for unit in inventory:
            node = self.declaration["units"].get(unit, unit)
            closure = self.closure(node)
            reasons = []
            if change["content"]:
                reasons.extend("unresolved dependency: " + reason for reason in closure["fallback"])
                reasons.extend("whole-content check: " + reason for reason in closure["whole"])
                if change["fallback"] and (closure["keys"] or closure["whole"] or closure["presence"]):
                    reasons.append("config syntax fallback: " + change["fallback"])
                for key in change["keys"]:
                    reasons.extend("changed key " + key + ": " + reason for reason in closure["keys"].get(key, []))
            if change["presence"]:
                reasons.extend("config existence: " + reason for reason in closure["presence"])
                reasons.extend("config existence for reader: " + reason for reasons_for_key in closure["keys"].values()
                               for reason in reasons_for_key)
            if reasons:
                selected[unit] = sorted(set(reasons))
        return selected
