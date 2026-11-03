"""Read-only engine selection. The shell entry point delegates here.

Ordinary paths retain the established sibling/name/section rules. Config values
use content-bound reader closures. An unknown closure is explicit and conservative;
this module does not infer independence from an absent grep match.
"""
import argparse
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys

from verification_inputs import (Dependencies, Snapshot, Unsupported, config, config_change,
                                 hook_command_path, hook_entries, hook_reader, hooks_change, revisions)

SECTIONED = "scripts/hooks/contract-integrity.test.sh"
GLOBAL_CONFIG = "scripts/verification-config.test.sh"
# The suite that checks verification-dependencies.json's reviewed reader pins against the
# readers' bytes. A change to a pinned reader selects it, so a stale pin fails at the land
# that made it stale, not at the next nightly (2026-09-28: 33 sources had gone stale on
# main, one land at a time, because nothing that checks pins was ever selected).
FULL_INPUT_CHECK = "scripts/verification-inputs.test.sh"
PIN_CHECK = "scripts/verification-pins.test.sh"
DEPENDENCY_MAP = "scripts/lib/verification-dependencies.json"


# A HOOK TIMEOUT EDIT SELECTS THAT HOOK'S CONSUMERS, NOT EVERY MANIFEST READER (2026-10-05).
#
# The hold-leak merge (34c54f9f4) changed one hook's timeout from 5 to 86400 in hooks/hooks.json
# and in .claude/settings.local.json. Those two lines alone selected 54 and 29 engine units: a
# timeout is a field of a registration, so the event counted as changed and every inventory
# reader of PreToolUse was selected with all its sections, and every suite whose text names
# settings.local.json was selected by basename. No selected suite read the real value (richos-hq
# docs/operations/2026-10-04-merge-check-speed.md, section 1). So when the ONLY difference
# between the two documents is the `timeout` of one or more registrations (same commands,
# matchers, events, order and every other field), the change selects what the changed commands
# themselves select (their sibling suites and the suites that name them) and the suites that
# declare they read hook timeouts with this line of their own:
TIMEOUT_READER_MARKER = "# verification: reads hook timeouts"


def _without_timeouts(document):
    """The document with every registration's `timeout` removed, or None for an unknown shape."""
    if not isinstance(document, dict) or not isinstance(document.get("hooks"), dict):
        return None
    out = dict(document)
    out["hooks"] = {}
    for event, groups in document["hooks"].items():
        if not isinstance(groups, list):
            return None
        kept = []
        for group in groups:
            if not isinstance(group, dict) or not isinstance(group.get("hooks"), list):
                return None
            kept.append(dict(group, hooks=[{k: v for k, v in command.items() if k != "timeout"}
                                           if isinstance(command, dict) else command
                                           for command in group["hooks"]]))
        out["hooks"][event] = kept
    return out


def timeout_only_commands(before, after, settings=False):
    """The commands whose registrations differ ONLY in `timeout`; None when anything else differs."""
    if not isinstance(before, str) or not isinstance(after, str) or before == after:
        return None
    try:
        old, new = hook_entries(before, settings=settings), hook_entries(after, settings=settings)
        stripped = [_without_timeouts(json.loads(text)) for text in (before, after)]
    except (Unsupported, ValueError):
        return None
    if None in stripped or stripped[0] != stripped[1] or old.keys() != new.keys():
        return None
    changed = sorted({new[key]["command"] for key in new if old[key] != new[key]})
    return changed or None


def settings_command_path(command):
    """An engine-relative script for a settings.local.json command, or None for an inline echo."""
    try:
        words = shlex.split(command)
    except ValueError as exc:
        raise Unsupported("unparseable hook command: " + str(exc)) from None
    if words[:1] in (["bash"], ["python3"]):
        words = words[1:]
    match = re.fullmatch(r"\$\{?CLAUDE_PROJECT_DIR\}?/([A-Za-z0-9_./+-]+)", words[0]) if len(words) == 1 else None
    if match:
        if ".." in Path(match[1]).parts:
            raise Unsupported("hook command escapes the engine: " + command)
        return match[1]
    return hook_command_path(command)


def pinned_sources(declaration):
    """Every engine-relative file whose bytes the dependency map pins."""
    out = set()
    for row in declaration.get("nodes", {}).values():
        if isinstance(row, dict):
            if isinstance(row.get("source"), str):
                out.add(row["source"])
            for ext in row.get("external", []):
                if (isinstance(ext, dict) and ext.get("root") == "environment"
                        and ext.get("default_engine") and isinstance(ext.get("path"), str)):
                    out.add(ext["path"])
    for row in declaration.get("hook_readers", {}).values():
        if isinstance(row, dict) and isinstance(row.get("sources"), dict):
            out.update(row["sources"])
    return out


def sections(text):
    bodies, current = {}, None
    for line in text.splitlines():
        match = re.fullmatch(r"if _section ([A-Za-z0-9_.-]+); then", line)
        if match:
            current = match[1]
            if current in bodies:
                raise Unsupported("duplicate section marker: " + current)
            bodies[current] = []
        elif line.startswith("fi  # _section"):
            current = None
        elif current:
            bodies[current].append(line)
    if not bodies:
        raise Unsupported("sectioned suite has no section markers")
    return {name: "\n".join(lines) for name, lines in bodies.items()}


def validate_config(engine):
    declaration = json.loads((engine / "scripts/lib/verification-dependencies.json").read_text())
    known = set(declaration["config_keys"])
    known.update(key for row in declaration["nodes"].values() for key in row.get("keys", []))
    parsed = config((engine / "orchestration.config").read_text())
    unknown = set(parsed) - known
    if unknown:
        raise Unsupported("unknown config keys require reader declarations: " + ", ".join(sorted(unknown)))
    return len(parsed)


def digest_only_change(before, after):
    """Only existing, well-formed reader digests may differ. Unknown shapes stay full."""
    def unique(pairs):
        out = {}
        for key, value in pairs:
            if key in out:
                raise ValueError("duplicate declaration key")
            out[key] = value
        return out

    def mask(text):
        def invalid_constant(value):
            raise ValueError("invalid declaration constant: " + value)
        document = json.loads(text, object_pairs_hook=unique,
                              parse_constant=invalid_constant)
        if document.get("schema") != 1:
            raise ValueError("unknown declaration schema")
        def pin(value):
            if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
                raise ValueError("invalid reader digest")
            return "$DIGEST"
        for row in document["nodes"].values():
            row["sha256"] = pin(row["sha256"])
            for external in row.get("external", []):
                external["sha256"] = pin(external["sha256"])
        for row in document.get("hook_readers", {}).values():
            row["sources"] = {path: pin(value) for path, value in row["sources"].items()}
        return document

    try:
        return isinstance(before, str) and isinstance(after, str) and mask(before) == mask(after)
    except (ValueError, KeyError, TypeError, AttributeError):
        return False


def validate_pins(engine):
    """Use the selector's existing reader validation, without running parser fixtures."""
    document = json.loads((engine / DEPENDENCY_MAP).read_text())
    graph = Dependencies(engine, document)
    for name, row in document["nodes"].items():
        graph.node(name)
        for edge in row.get("edges", []):
            if edge["to"] not in document["nodes"]:
                raise Unsupported("unqualified reader " + edge["to"] + " referenced by " + name)
    for row in document.get("hook_readers", {}).values():
        hook_reader(row, graph.read)
    return len(document["nodes"]), len(document.get("hook_readers", {}))


class Selection:
    def __init__(self, engine, suites, read, declaration, before=None):
        # `before` reads the base side of the change (None for a bare path list): with it, a
        # path the base had and the selected tree lacks is a removal, never an unmapped file.
        self.engine, self.read, self.declaration, self.before = engine, read, declaration, before
        self.suites = {path: read(path) for path in suites}
        if any(text is None for text in self.suites.values()):
            raise Unsupported("suite disappeared while planning")
        self.bodies = sections(self.suites[SECTIONED]) if SECTIONED in self.suites else {}
        self.units = [unit for suite in self.suites for unit in
                      ([suite + ":" + part for part in self.bodies] if suite == SECTIONED else [suite])]
        self.selected, self.unmapped, self.removed = {}, [], []
        self.pinned = pinned_sources(declaration)

    def add(self, unit, reason):
        self.selected.setdefault(unit, set()).add(reason)

    def suite(self, suite, path, reason, all_sections=False):
        if suite == SECTIONED:
            basename = Path(path).name
            hits = list(self.bodies) if all_sections else [
                part for part, body in self.bodies.items() if basename in body]
            for part in hits or self.bodies:
                why = reason if hits else reason + "; ALL sections: named outside any section body"
                self.add(suite + ":" + part, path + ": " + why)
        else:
            self.add(suite, path + ": " + reason)

    def unchanged_seated_reader(self, suite, before, after):
        """Compare only source-bound registration observations of this suite."""
        row = self.declaration.get('hook_readers', {}).get(suite)
        if not isinstance(row, dict):
            return False
        needles = row.get('seated_text_needles', [])
        events = row.get('seated_events', [])
        no_reads = row.get('seated_reads_nothing', False)
        if (not isinstance(needles, list) or not isinstance(events, list)
                or type(no_reads) is not bool
                or any(not isinstance(n, str) or not n or '\n' in n for n in needles)
                or any(not isinstance(e, str) or not re.fullmatch(r'[A-Za-z][A-Za-z0-9]*', e) for e in events)
                or not (needles or events or no_reads)
                or no_reads and (needles or events or 'seated_dispatch_reader' in row)
                or not isinstance(before, str) or not isinstance(after, str)):
            return False
        try:
            hook_reader(row, self.read)
            if suite not in row['sources']:
                return False
            documents = [json.loads(text) for text in (before, after)]
            if any(not isinstance(document, dict) for document in documents):
                return False
            if events or no_reads:
                for text in (before, after):
                    hook_entries(text, settings=True)
                # Preserve complete groups, ordering and metadata. Also retain
                # legacy top-level event data consumed by the spawn collector.
                for event in events:
                    def observation(document):
                        return (event in document['hooks'], document['hooks'].get(event),
                                event in document, document.get(event))
                    if observation(documents[0]) != observation(documents[1]):
                        return False
            if 'seated_dispatch_reader' in row:
                if row['seated_dispatch_reader'] is not True:
                    return False
                def dispatchers(text):
                    return sorted(entry['command'] for entry in hook_entries(text, settings=True).values()
                                  if 'dispatch-pretooluse' in entry['command'])
                if dispatchers(before) != dispatchers(after):
                    return False
        except (Unsupported, ValueError):
            return False
        return all([line for line in before.splitlines() if needle in line] ==
                   [line for line in after.splitlines() if needle in line]
                   for needle in needles)

    def unchanged_probe_reader(self, suite, before, after):
        """Ignore only unrelated literal oracle rows for qualified text readers."""
        row = self.declaration.get('hook_readers', {}).get(suite)
        if not isinstance(row, dict) or 'probe_registration_needles' not in row:
            return False
        needles = row['probe_registration_needles']
        no_reads = row.get('probe_registration_reads_nothing', False)
        if (not isinstance(needles, list) or type(no_reads) is not bool
                or not (needles or no_reads) or no_reads and needles
                or any(not isinstance(n, str) or not re.fullmatch(r'[A-Za-z0-9_-]+', n) for n in needles)
                or not isinstance(before, str) or not isinstance(after, str)):
            return False
        try:
            hook_reader(row, self.read)
            if suite not in row['sources']:
                return False
            def observed(text):
                blocks = list(re.finditer(r'(?m)^    BR_EXPECTED="\\\n([^"]*)"', text))
                if len(blocks) != 1:
                    raise Unsupported('ambiguous probe registration oracle')
                block = blocks[0]
                lines = block[1].splitlines(keepends=True)
                if not lines or any(not re.fullmatch(r'[A-Za-z0-9_.-]+\.sh\|[A-Za-z][A-Za-z0-9]*\n?', line)
                                    for line in lines):
                    raise Unsupported('nonliteral probe registration oracle')
                retained = ''.join(line for line in lines if any(n in line for n in needles))
                return text[:block.start(1)] + retained + text[block.end(1):]
            return observed(before) == observed(after)
        except Unsupported:
            return False

    def timeout_edit(self, source, commands, resolve, before, after, needle_key):
        """Select a timeout-only edit of `source` (see TIMEOUT_READER_MARKER); False to fall back."""
        try:
            paths = sorted({path for command in commands if (path := resolve(command)) is not None})
        except Unsupported:
            return False
        for path in paths:
            self.ordinary(path)
        for suite, text in self.suites.items():
            if any(line.strip() == TIMEOUT_READER_MARKER for line in text.splitlines()):
                self.suite(suite, source, "declared hook-timeout reader", all_sections=True)
        # A qualified reader's raw-text assertions keep their own rule: a changed line it greps for
        # still selects it, whatever the change means.
        for suite in self.suites:
            row = self.declaration.get("hook_readers", {}).get(suite)
            needles = row.get(needle_key, []) if isinstance(row, dict) else []
            if isinstance(needles, list) and any(
                    isinstance(needle, str) and needle and
                    [line for line in before.splitlines() if needle in line] !=
                    [line for line in after.splitlines() if needle in line] for needle in needles):
                self.suite(suite, source, "changed text consumed by registration assertions", all_sections=True)
        return True

    def ordinary(self, path, before=None, after=None):
        if path == '.claude/settings.local.json':
            commands = timeout_only_commands(before, after, settings=True)
            if commands and self.timeout_edit(path, commands, settings_command_path, before, after,
                                              'seated_text_needles'):
                return
        pin_check = PIN_CHECK if PIN_CHECK in self.units else FULL_INPUT_CHECK
        renewal = (path == DEPENDENCY_MAP and PIN_CHECK in self.units
                   and digest_only_change(before, after))
        if renewal:
            self.add(PIN_CHECK, path + ": existing reader digests renewed; dependency semantics unchanged")
        basename, stem = Path(path).name, str(Path(path).with_suffix(""))
        if path.startswith("voice/") and Path(path).suffix in (".js", ".mjs", ".json", ".sh"):
            self.suite("voice/tests/run.test.sh", path, "voice component inputs")
        if path in self.suites:
            self.suite(path, path, "changed suite")
        for sibling in (stem + ".test.sh", path + ".test.sh", str(Path(stem).parent / "tests" / (Path(stem).name + ".test.sh"))):
            if sibling in self.suites:
                self.suite(sibling, path, "sibling suite")
        for suite, text in self.suites.items():
            if basename in text:
                if renewal and suite == FULL_INPUT_CHECK:
                    continue
                if path == '.claude/settings.local.json' and self.unchanged_seated_reader(suite, before, after):
                    continue
                if path == 'scripts/hooks/contract-integrity-probe.sh' and self.unchanged_probe_reader(suite, before, after):
                    continue
                self.suite(suite, path, "basename dependency (conservative)")
        # A suite claims a path when a reason for it is on record, NOT when this call grew
        # the record: hooks() already runs ordinary() for a newly registered hook script, so
        # the same reason strings are present before the script's own turn and a growth
        # test called a claimed script "named by NO suite".
        matched = any(reason.startswith(path + ": ") and "pinned reader in" not in reason
                      for reasons in self.selected.values() for reason in reasons)
        # A PATH THE CHANGE DELETED has no code left to prove (proof-for.sh's REMOVED rule, hunt
        # 2026-09-29 part 2 section 03): the suites that still name it are selected above, and it
        # is never "named by NO suite". It must have existed on the base side: a path that never
        # existed (a typo, or a path list with no base) stays unmapped. 2026-10-01: four of the
        # seven files refused at main bb112ab68 were absent from the tree the selector read.
        # Each path is listed once, however many roads (hooks() and its own turn) visit it. The
        # selected tree is read only when the answer needs it, as before: with no base side, a
        # path with an executable suffix is never read here.
        if not matched and (Path(path).suffix in (".sh", ".py", ".bash") or (self.read(path) or "").startswith("#!")):
            removed = self.before is not None and self.read(path) is None and self.before(path) is not None
            listed = self.removed if removed else self.unmapped
            if path not in listed:
                listed.append(path)
        # Decided AFTER `matched`: the pin check proves the pin, not the reader's behavior,
        # so it never turns an unmapped executable into a mapped one.
        if path in self.pinned and pin_check in self.units:
            self.add(pin_check, path + ": pinned reader in verification-dependencies.json "
                     "(renew its pin after review, in the same land)")

    def configuration(self, before, after, unknown=False):
        change = config_change(before, after)
        if unknown:
            change.update(content=True, presence=True, fallback="path-only request has no before/after config identity")
        graph = Dependencies(self.engine, self.declaration, read=self.read)
        for unit, reasons in graph.config_units(change, self.units).items():
            for reason in reasons:
                self.add(unit, "orchestration.config: " + reason)
        # Validation remains a genuine whole-config obligation even when no
        # behavior reader consumes a newly introduced setting.
        if change["content"]:
            if GLOBAL_CONFIG not in self.units:
                raise Unsupported("global config validation unit is missing from the target inventory")
            self.add(GLOBAL_CONFIG, "orchestration.config: global syntax and unknown-key validation")

    def hooks(self, before, after, unknown=False):
        change = hooks_change(before, after)
        if unknown:
            change.update(content=True, fallback='path-only request has no before/after hook identity')
        if not change['content']:
            return
        if not unknown and not change['fallback']:
            commands = timeout_only_commands(before, after)
            if commands and self.timeout_edit('hooks/hooks.json', commands, hook_command_path, before, after,
                                              'text_needles'):
                return
        paths = set()
        if not change['fallback']:
            try:
                paths = {path for command in change['commands']
                         if (path := hook_command_path(command)) is not None}
            except Unsupported as exc:
                change['fallback'] = str(exc)
        if change['fallback']:
            for unit in self.units:
                self.add(unit, 'hooks/hooks.json: syntax/command fallback: ' + change['fallback'])
            return
        # A removed command still selects its behavior consumers. Registration
        # and ordering checks below are additional obligations, not substitutes.
        for path in sorted(paths):
            self.ordinary(path)
        contracts = self.declaration.get('hook_readers', {})
        for suite, text in self.suites.items():
            row = contracts.get(suite)
            if row is None:
                if 'hooks.json' in text:
                    self.suite(suite, 'hooks/hooks.json', 'unqualified manifest reader (conservative)',
                               all_sections=True)
                continue
            try:
                row = hook_reader(row, self.read)
                if suite not in row['sources']:
                    raise Unsupported('hook reader omits its own source: ' + suite)
            except Unsupported as exc:
                self.suite(suite, 'hooks/hooks.json', str(exc), all_sections=True)
                continue
            affected = paths & set(row['commands'])
            events = (set(change['events']) if row.get('all_events')
                      else set(change['events']) & set(row.get('events', [])))
            # Existing grep-based registration assertions also inspect raw
            # text. Preserve their behavior on duplicate lines, formatting and
            # description edits, even when parsed hook semantics are unchanged.
            needles = row.get('text_needles', [])
            changed_text = any([line for line in before.splitlines() if needle in line] !=
                               [line for line in after.splitlines() if needle in line]
                               for needle in needles)
            if affected or events or changed_text:
                why = ('changed registration/ordering for ' + ', '.join(sorted(affected))
                       if affected else 'changed event ordering/inventory for ' + ', '.join(sorted(events))
                       if events else 'changed text consumed by registration assertions')
                # A suite-level inventory contract includes shared setup.
                # Basename matches inside a section cannot narrow that scope.
                self.suite(suite, 'hooks/hooks.json', why, all_sections=True)


def git(root, *argv):
    result = subprocess.run(["git", "-C", str(root), *argv], capture_output=True, text=True)
    if result.returncode:
        raise Unsupported("Git input unavailable: " + " ".join(argv) + ": " + result.stderr.strip())
    return result.stdout


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--range")
    parser.add_argument("--base")
    parser.add_argument("--staged", action="store_true")
    parser.add_argument("--working", action="store_true")
    parser.add_argument("--paths-file", type=Path)
    parser.add_argument("--paths", action="append", default=[])
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--explain", action="store_true")
    parser.add_argument("--validate-config", action="store_true")
    parser.add_argument("--validate-pins", action="store_true")
    args = parser.parse_args(argv)
    engine = Path(__file__).resolve().parents[2]
    try:
        if args.validate_config:
            print("PASS: global config syntax and declarations (%d keys)" % validate_config(engine))
            return 0
        if args.validate_pins:
            nodes, hooks = validate_pins(engine)
            print("PASS: reviewed dependency pins and reader floors (%d nodes, %d hook readers)" % (nodes, hooks))
            return 0
        root = Path(git(engine, "rev-parse", "--show-toplevel").strip())
        prefix = engine.relative_to(root).as_posix()
        prefix = "" if prefix == "." else prefix + "/"
        explicit = bool(args.paths or args.paths_file)
        if sum(bool(value) for value in (args.range, args.staged, args.working)) > 1:
            raise Unsupported("choose one of --range, --staged or --working")
        if args.staged:
            old, new = "HEAD", "INDEX"
            diff = ["--cached"]
        elif args.working:
            old, new = args.base or "HEAD", "WORKTREE"
            diff = [old]
        elif args.range or args.base or not explicit:
            old, new = revisions(root, args.range or (args.base or "HEAD^") + "..HEAD")
            diff = [old, new]
        else:
            old, new, diff = None, "WORKTREE", []
        after = Snapshot(root, new)
        read = lambda path: after.read(prefix + str(path))
        if args.paths_file:
            changed = args.paths_file.read_text().splitlines()
        elif args.paths:
            changed = [part for value in args.paths for part in value.split(",")]
        else:
            changed = git(root, "diff", "--name-only", *diff).splitlines()
            if new == "WORKTREE":
                changed += git(root, "ls-files", "--others", "--exclude-standard").splitlines()
        changed = sorted({path.strip() for path in changed if path.strip()})
        if new == "WORKTREE":
            suites = sorted(path.relative_to(engine).as_posix() for path in engine.rglob("*.test.sh") if path.is_file())
        else:
            files = (git(root, "ls-files") if new == "INDEX" else git(root, "ls-tree", "-r", "--name-only", after.revision)).splitlines()
            suites = sorted(path[len(prefix):] for path in files if path.startswith(prefix) and path.endswith(".test.sh"))
        declaration = json.loads((engine / "scripts/lib/verification-dependencies.json").read_text())
        base = Snapshot(root, old) if old else None
        selection = Selection(engine, suites, read, declaration,
                              before=(lambda path: base.read(prefix + str(path))) if base else None)
        for path in changed:
            if not path.startswith(prefix):
                continue
            relative = path[len(prefix):]
            if relative == "orchestration.config":
                before = Snapshot(root, old).read(path) if old else None
                selection.configuration(before, read(relative), unknown=old is None)
            elif relative == 'hooks/hooks.json':
                before = Snapshot(root, old).read(path) if old else None
                selection.hooks(before, read(relative), unknown=old is None)
            elif relative in ('.claude/settings.local.json', 'scripts/hooks/contract-integrity-probe.sh', DEPENDENCY_MAP):
                before = Snapshot(root, old).read(path) if old else None
                selection.ordinary(relative, before, read(relative))
            else:
                selection.ordinary(relative)
        for unit, reasons in sorted(selection.selected.items()):
            print(unit)
            if args.explain:
                for reason in sorted(reasons):
                    print("    %s -> %s" % (reason, unit), file=sys.stderr)
        print("%d changed path(s) -> %d unit(s)" % (len(changed), len(selection.selected)), file=sys.stderr)
        if not selection.selected:
            print("NOTHING TO RUN: no affected suite for these paths.", file=sys.stderr)
        if selection.removed:
            print("%d removed file(s) no suite names, no code left to prove:" % len(selection.removed), file=sys.stderr)
            print("\n".join("    " + path for path in selection.removed), file=sys.stderr)
        if selection.unmapped:
            print("%d changed executable file(s) are named by NO suite:" % len(selection.unmapped), file=sys.stderr)
            print("\n".join("    " + path for path in selection.unmapped), file=sys.stderr)
            if args.strict:
                return 1
        return 0
    except (OSError, ValueError, Unsupported, subprocess.SubprocessError) as exc:
        print("ERROR: ci-affected-units.sh: " + str(exc), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
