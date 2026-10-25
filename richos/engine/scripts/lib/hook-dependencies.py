#!/usr/bin/env python3
"""hook-dependencies.py — the predicate behind scripts/lib/hook-dependencies.sh.

Prints one engine-relative path per line: every file under scripts/, mega-lander/ or ass-kicker/ that a
registered hook needs, transitively, and that is not itself a registered hook.

    hook-dependencies.py <engine-dir>

The whole decision — what counts as a reference, why a sentence containing a
path is not one, what the anchor is and why a broken deriver must print nothing
— is stated in hook-dependencies.sh. This file is the implementation of it, kept
separate for the reason ceo-todos.sh keeps ceo-todos.py: one predicate, callable
from shell, with no heredoc inside a command substitution (bash 3.2 on macOS
mis-parses that shape, which is how this file came to exist).

Exit codes, identical to the wrapper's contract:
  0  complete closure printed
  1  no such engine directory, or no hooks/hooks.json in it
  2  hooks/hooks.json unparseable, or registering no hook script at all
  4  ANCHOR FAILED — nothing printed
"""
import ast
import json
import os
import re
import sys

SCAFFOLD = (".test.sh", ".test.py", ".mutation.sh")

# A REFERENCE IS A QUOTED TOKEN WHOSE ENTIRE CONTENT IS A PATH. Leading shell
# expansions are stripped first: $VAR, ${VAR}, and $(cd "$(dirname
# "${BASH_SOURCE[0]}")" && pwd), which is how the two handoff hooks reach
# scripts/lib/worktree-ledger.py.
EXPANSION = (r'(?:(?:\$\{?[A-Za-z_][A-Za-z0-9_]*\}?'
             r'|\$\([^()]*(?:\([^()]*\)[^()]*)*\))/)+')
# `manifest` joined this list on 2026-09-15 for the same reason `dict` is on it:
# scripts/hooks/dispatch-pretooluse.manifest is DATA a registered hook reaches
# for and cannot run without, exactly as dialect-guard.sh cannot run without
# dialect-en-US.dict. Left off, every sandbox this engine builds carried a
# dispatcher with no manifest — which fails LOUD (the dispatcher refuses) rather
# than quietly, but fails in ten suites at once for a reason none of them names.
BODY = (r'[A-Za-z0-9._+-]+(?:/[A-Za-z0-9._+-]+)*'
        r'\.(?:sh|py|json|tsv|dict|md|txt|manifest)')
WHOLE_TOKEN = re.compile(r'^' + EXPANSION + r'?' + BODY + r'$')
LEADING_EXPANSION = re.compile(r'^' + EXPANSION)


def quoted_tokens(line):
    """The quoted spans of a line, at the TOP LEVEL of quoting only.

    Scanned rather than regex-matched, because a regex cannot tell a token from
    punctuation inside a sentence. `REST=" (... '$ENGINE_ROOT/scripts/x.sh'
    --session <id>.)"` contains a single-quoted path, and a regex sees a
    perfectly-formed token; a shell sees literal text inside one double-quoted
    string. Measured, not feared: that exact line in session-start-ceo-ask.sh
    and one in engine-status.sh made three advice paths look like dependencies.
    """
    out = []
    i, n = 0, len(line)
    while i < n:
        ch = line[i]
        if ch in '"\'':
            j = i + 1
            buf = []
            while j < n:
                if ch == '"' and line[j] == '\\' and j + 1 < n:
                    buf.append(line[j + 1])
                    j += 2
                    continue
                # A command substitution inside a double-quoted span carries
                # its own quoting, and the shell does not end the span at the
                # quotes inside it. Consumed whole, parens balanced, so that
                # `"$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/x.py"`
                # stays ONE token -- the shape both handoff hooks use to reach
                # scripts/lib/worktree-ledger.py.
                if ch == '"' and line.startswith("$(", j):
                    depth, k = 0, j
                    while k < n:
                        if line[k] == "(":
                            depth += 1
                        elif line[k] == ")":
                            depth -= 1
                            if depth == 0:
                                k += 1
                                break
                        k += 1
                    buf.append(line[j:k])
                    j = k
                    continue
                if line[j] == ch:
                    break
                buf.append(line[j])
                j += 1
            if j < n:
                out.append("".join(buf))
                i = j + 1
                continue
            return out
        i += 1
    return out

ANCHOR = "scripts/lib/resolve-roots.sh"
# Keyed on a USE, not a mention: `resolve_engine_root` is defined in
# resolve-roots.sh and cannot be called without sourcing it, so a hook that
# merely NAMES the library in prose does not enter the expected set. That
# matters -- an anchor with a false-positive class is a check that gets
# waived, and a waived check is the failure this engine keeps finding.
ANCHOR_TEXT = "resolve_engine_root"


def is_scaffold(rel):
    return os.path.basename(rel).endswith(SCAFFOLD)


def in_scope(rel):
    """Code and data under scripts/, mega-lander/ and ass-kicker/. Configuration and record files are the
    consumer's own to synthesize — see the wrapper's SCOPE section."""
    return rel.startswith(("scripts/", "mega-lander/", "ass-kicker/")) and not is_scaffold(rel)


def dispatch_manifest(eng):
    """{chain key: [rule basenames]} from scripts/hooks/dispatch-pretooluse.manifest.

    Empty when the engine has no dispatcher, which is every engine before
    2026-09-15 and every engine that never adopts one.
    """
    out = {}
    path = os.path.join(eng, "scripts/hooks/dispatch-pretooluse.manifest")
    try:
        with open(path, encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if line and not line.startswith("#") and "|" in line:
                    key, module = line.split("|", 1)
                    out.setdefault(key.strip(), []).append(module.strip())
    except OSError:
        pass
    return out


def registered_hooks(eng):
    # THE DISPATCHER'S RULES ARE REGISTERED HOOKS FOR THIS PURPOSE, and getting
    # that wrong has one symptom and it is not a missing name. This function
    # SEEDS the dependency closure, so a rule left out of the seed set takes its
    # whole dependency subtree with it — scripts/lib/inflight.py,
    # scripts/lib/ceo-todos.sh, scripts/hook-registration-completeness.sh — and
    # every sandbox this engine's suites build is then missing what those rules
    # reach for. Measured: with the seed set unexpanded,
    # contract-integrity.test.sh SC1 failed naming
    # guard-hook-registration-commits.sh, which is the exact omission SC1's own
    # header records from 2026-09-14, arriving through a new door.
    try:
        with open(os.path.join(eng, "hooks/hooks.json"), encoding="utf-8") as fh:
            doc = json.load(fh)
    except Exception:
        raise SystemExit(2)
    manifest = dispatch_manifest(eng)
    found = set()
    hooks = doc.get("hooks", {})
    if isinstance(hooks, dict):
        for entries in hooks.values():
            if not isinstance(entries, list):
                continue
            for entry in entries:
                if not isinstance(entry, dict):
                    continue
                for h in entry.get("hooks", []) or []:
                    if not isinstance(h, dict):
                        continue
                    cmd = h.get("command", "")
                    if not isinstance(cmd, str):
                        continue
                    for m in re.findall(
                            r"scripts/hooks/([A-Za-z0-9._+-]+\.(?:sh|py))", cmd):
                        found.add(m)
                    hit = re.search(
                        r"scripts/hooks/dispatch-pretooluse\.sh\s+(\S+)", cmd)
                    if hit:
                        found.update(manifest.get(hit.group(1), []))
    if not found:
        raise SystemExit(2)
    return found


def build_basename_index(eng):
    """One index, so a `.sh` half reaches its `.py` half without guessing."""
    index = {}
    for dirpath, dirnames, filenames in os.walk(eng):
        dirnames[:] = [d for d in dirnames
                       if d not in (".git", "__pycache__", "node_modules")]
        for fn in filenames:
            index.setdefault(fn, []).append(
                os.path.relpath(os.path.join(dirpath, fn), eng))
    return index


def resolve(eng, index, tok, owner_rel):
    owner_dir = os.path.dirname(os.path.join(eng, owner_rel))
    expanded = bool(LEADING_EXPANSION.match(tok))
    body = LEADING_EXPANSION.sub("", tok)
    cands = []
    if body.startswith(("../", "./")):
        cands.append(os.path.join(owner_dir, body))
    elif body.startswith(("scripts/", "hooks/", "mega-lander/", "ass-kicker/")):
        cands.append(os.path.join(eng, body))
        if expanded:
            cands.append(os.path.join(owner_dir, body))
    elif expanded:
        cands.append(os.path.join(owner_dir, body))
        hits = index.get(body, [])
        if len(hits) == 1:
            cands.append(os.path.join(eng, hits[0]))
    for cand in cands:
        cand = os.path.normpath(cand)
        try:
            if os.path.commonpath([cand, eng]) != eng:
                continue
        except ValueError:
            continue
        if os.path.isfile(cand):
            return os.path.relpath(cand, eng)
    # A token that resolves to no file is DROPPED. A hook cannot depend on a
    # file that is not there, and demanding one would be inventing a fact.
    return None


# A PYTHON IMPORT OF AN ENGINE MODULE IS A REFERENCE TOO (2026-09-30). The quoted
# path rule above cannot see `import containers`, so a sandbox built from this
# closure carried workspaces.py and not the module workspaces.py imports. The
# land's container cleanup then failed with "No module named 'containers'" and
# the landed agent kept its workspace (integration section Q, richos 4e73fd89).
# Modules are looked up beside the importing file and in the three directories
# the engine puts on sys.path: scripts/lib, mega-lander and scripts/hooks.
IMPORT_DIRS = ("scripts/lib", "mega-lander", "scripts/hooks", "scripts")

# NEVER COPIED INTO A SANDBOX, BY NAME. testdevices.py lists, boots and deletes
# the Mac's real simulators and emulators. Its own gate (machine_devices_allowed)
# stands down when HOME or the state directories are redirected, but a sandbox
# that redirected only some of them would reach real devices, so the module is
# simply not there. Every caller that imports it (workspaces.collect_test_devices,
# disk-watchdog, scratch-reaper, worker_tokens) wraps the import in try/except and
# degrades to "devices not collected", which is exactly what a sandbox wants.
DEVICE_MODULES = frozenset(("testdevices",))

STDLIB = getattr(sys, "stdlib_module_names", frozenset())


def imported_modules(source):
    """Top-level module names imported anywhere in a Python source, including
    imports inside functions and try blocks. Relative imports are skipped."""
    try:
        tree = ast.parse(source)
    except (SyntaxError, ValueError):
        return set()
    names = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for a in node.names:
                names.add(a.name.split(".")[0])
        elif isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
            names.add(node.module.split(".")[0])
    return names


def import_references(eng, rel, raw):
    found = set()
    owner_dir = os.path.dirname(rel)
    for name in imported_modules(raw.decode("utf-8", "replace")):
        if name in STDLIB or name in DEVICE_MODULES:
            continue
        for d in (owner_dir,) + IMPORT_DIRS:
            cand = os.path.normpath(os.path.join(d, name + ".py"))
            if os.path.isfile(os.path.join(eng, cand)) and cand != rel:
                if not is_scaffold(cand):
                    found.add(cand)
                break
    return found


def references(eng, index, rel):
    path = os.path.join(eng, rel)
    if not os.path.isfile(path):
        return set()
    try:
        with open(path, "rb") as fh:
            raw = fh.read()
    except Exception:
        return set()
    if b"\0" in raw[:4096]:
        return set()
    found = set()
    if rel.endswith(".py"):
        found |= import_references(eng, rel, raw)
    for line in raw.decode("utf-8", "replace").splitlines():
        if line.lstrip().startswith("#"):
            continue
        for tok in quoted_tokens(line):
            if not WHOLE_TOKEN.match(tok):
                continue
            got = resolve(eng, index, tok, rel)
            if got and not is_scaffold(got):
                found.add(got)
    return found


def check_anchor(eng, seeds, direct):
    """Every hook carrying the shared root bootstrap must come out of the
    derivation depending on it. The expected set is taken by a PLAIN TEXT SCAN —
    a weaker, differently shaped predicate that the extraction rules cannot
    break in the same direction."""
    expected, derived = set(), set()
    for seed in sorted(seeds):
        path = os.path.join(eng, seed)
        if not os.path.isfile(path):
            continue
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except Exception:
            continue
        if ANCHOR_TEXT in text:
            expected.add(seed)
        if ANCHOR in direct.get(seed, set()):
            derived.add(seed)
    if not expected:
        sys.stderr.write(
            "hook-dependencies.py: ANCHOR FAILED — no registered hook calls "
            "%s(). Either this is not a richos engine, or the registration "
            "surface was just rewritten wholesale; neither is something to "
            "answer with a list.\n" % ANCHOR_TEXT)
        raise SystemExit(4)
    missed = sorted(expected - derived)
    if missed:
        sys.stderr.write(
            "hook-dependencies.py: ANCHOR FAILED — %d of %d hooks call %s() and "
            "the derivation did not resolve scripts/lib/resolve-roots.sh for: %s\n"
            "  The extraction rules have stopped matching a reference shape "
            "this engine uses. A shorter closure is indistinguishable from a "
            "correct one, so nothing is printed.\n"
            % (len(missed), len(expected), ANCHOR_TEXT,
               ", ".join(os.path.basename(m) for m in missed)))
        raise SystemExit(4)


def main(argv):
    if len(argv) != 2:
        sys.stderr.write("usage: hook-dependencies.py <engine-dir>\n")
        return 1
    eng = os.path.abspath(argv[1])
    if not os.path.isdir(eng) or not os.path.isfile(
            os.path.join(eng, "hooks/hooks.json")):
        return 1

    reg = registered_hooks(eng)
    index = build_basename_index(eng)
    seeds = set("scripts/hooks/" + b for b in reg)

    direct = {}
    seen = set(seeds)
    work = list(seeds)
    while work:
        cur = work.pop()
        refs = references(eng, index, cur)
        direct[cur] = refs
        for ref in refs:
            if ref not in seen:
                seen.add(ref)
                work.append(ref)

    check_anchor(eng, seeds, direct)

    for rel in sorted(x for x in seen - seeds if in_scope(x)):
        print(rel)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
