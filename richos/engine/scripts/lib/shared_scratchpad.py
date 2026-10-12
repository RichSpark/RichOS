#!/usr/bin/env python3
"""shared_scratchpad.py — a teammate never deletes inside the lead session's directory.

WHAT HAPPENED, 2026-10-01. Claude Code gives every session a scratchpad,
<root>/<project-slug>/<session-id>/scratchpad, and tells every IN-PROCESS teammate
of that session that the same directory is its own "session-specific" scratchpad.
It is shared: the lead and every teammate of the session write into one folder.
The working-material rule then told each teammate to "delete your disposable
scratch before reporting". Two teammates did exactly that, as their last step:

  22:08:59Z  echo-opus-hunta       rm -rf <session>/scratchpad/* && ls -A ... | wc -l
  22:34:19Z  quint-sonnet-huntb4   rm -f <session>/scratchpad/*

The first took the lead's briefs/ folder. The second took a live teammate's helper
scripts, its logs and its backup copy of a source file it had just mutated, 12
seconds after that teammate parked them there. Both obeyed the rule they were
given; the rule pointed them at a directory they did not own. (The land sweep was
suspected first and is innocent: ~/.claude/state/scratch-reaper.log names no path
under the session directory for either land that night.)

THE RULE. A call made inside a subagent (its payload carries `agent_id`) may not
delete anything inside a session directory under a Claude scratch root, nor that
directory, nor any directory above it. Not "anything it did not create": in a
folder every teammate writes into, nothing on disk proves which teammate wrote a
file, so no delete there can be shown to be the deleter's own. Nothing is lost by
refusing: the scratch reaper removes the whole session directory once the session
has ended. A teammate's own scratch is the directory returned by machine_settings.py scratch <its-name>, and
deleting there is not this rule's business. The lead's own calls are not refused.

WHAT IT SEES: rm, rmdir, unlink, trash, srm, and find with -delete or an
-exec/-execdir of one of those, after `VAR=value` assignments made earlier in the
same command are substituted and relative paths are resolved against the working
directory (including a `cd` earlier in the command). IT IS A TEXT CHECK AND IT
LEAKS (a script file, `sh -c "$X"`, a Python rmtree): its job is to stop the
habitual cleanup command, not an adversary. An operand it cannot resolve (a
variable it never saw assigned) is let through. Any error in the check is a pass.

Usage:
  shared_scratchpad.py bash-check < payload.json     exit 2 refused (reason on stderr), else 0
  shared_scratchpad.py verdict <command> [cwd]       prints the refusal or "allowed"; exit 1/0
"""

import importlib.util
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

# The shell tokenizer is the one the foreign-app-data rule already uses and tests;
# two tokenizers would disagree about the same command line.
_spec = importlib.util.spec_from_file_location("foreign_app_data", os.path.join(HERE, "foreign_app_data.py"))
_fad = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fad)

DELETERS = {"rm", "rmdir", "unlink", "trash", "srm"}
UUID = r"[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}"
ASSIGN = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)=(.*)$", re.S)
VAR = re.compile(r"\$(?:\{([A-Za-z_][A-Za-z0-9_]*)\}|([A-Za-z_][A-Za-z0-9_]*))")
GLOB = re.compile(r"[*?\[]")
FALLBACK_ROOTS = "/private/tmp/claude-%u /tmp/claude-%u"


def claude_roots():
    """The Claude scratch roots, from the same declaration the scratch reaper reads.

    SCRATCH_CLAUDE_ROOTS in the environment wins (the hook test sets it); otherwise
    the engine's orchestration.config is read for the line; otherwise the shipped
    value. Each is returned real-path resolved, so /tmp and /private/tmp are one root."""
    raw = (os.environ.get("SCRATCH_CLAUDE_ROOTS") or "").strip()
    if not raw:
        cfg = os.path.join(HERE, "..", "..", "orchestration.config")
        try:
            with open(cfg, encoding="utf-8") as fh:
                for line in fh:
                    m = re.match(r'^SCRATCH_CLAUDE_ROOTS="([^"]*)"', line)
                    if m:
                        raw = m.group(1)
        except OSError:
            pass
    raw = raw or FALLBACK_ROOTS
    uid = str(os.getuid())
    out = []
    for r in raw.split():
        real = os.path.realpath(r.replace("%u", uid))
        if real not in out:
            out.append(real)
    return out


def _real(path):
    """realpath of the longest existing prefix, with the rest appended unresolved."""
    path = os.path.normpath(path)
    head, tail = path, []
    while head and not os.path.exists(head) and head != os.path.dirname(head):
        head, base = os.path.dirname(head), os.path.basename(head)
        tail.insert(0, base)
    return os.path.join(os.path.realpath(head), *tail) if tail else os.path.realpath(head)


def protected(path, roots):
    """The reason `path` is the shared session area, or None.

    Inside <root>/<slug>/<session-uuid>, that directory itself, or an ancestor of
    one (the root or a slug directory), for any declared root."""
    p = _real(path)
    for root in roots:
        if p == root or p.startswith(root + os.sep):
            rest = p[len(root):].strip(os.sep)
            parts = rest.split(os.sep) if rest else []
            # The root, or a project directory under it: removing either removes
            # every session directory inside. A loose FILE at that level is not
            # an ancestor of anything and is not this rule's business.
            if not parts or (len(parts) == 1 and os.path.isdir(p)):
                return "%s holds session directories, whose scratchpads are shared" % p
            if len(parts) == 1:
                continue
            if re.fullmatch(UUID, parts[1]):
                return "%s is inside session %s's directory, whose scratchpad the lead and every teammate share" % (p, parts[1])
    return None


def _expand(word, assigned, cwd):
    """The word with known variables and ~ substituted, made absolute; None if unknowable."""
    w = _subst(word, assigned)
    if w is None:
        return None
    if not w.startswith("/"):
        if not cwd:
            return None
        w = os.path.join(cwd, w)
    return w


def _subst(word, assigned):
    """The word with known variables and ~ substituted, NOT made absolute; None if unknowable.

    Hunt P5-83 (v3): an assigned VALUE is a string, not yet a path. Making
    `SID=<uuid>` absolute against the working directory turned the session id
    into /cwd/<uuid>, so `.../$SID/scratchpad` stopped naming the session."""
    def sub(m):
        name = m.group(1) or m.group(2)
        if name in assigned:
            return assigned[name]
        if name in ("HOME",) and os.environ.get(name):
            return os.environ[name]
        return m.group(0)
    w = VAR.sub(sub, word)
    if "$" in w or "`" in w:
        return None
    if w == "~" or w.startswith("~/"):
        w = os.path.expanduser(w)
    return w


def _glob_base(path):
    """For a glob operand, the directory it expands inside; else the path itself."""
    if not GLOB.search(path):
        return path, False
    parts = path.split("/")
    for i, part in enumerate(parts):
        if GLOB.search(part):
            return "/".join(parts[:i]) or "/", True
    return path, True


def _targets(cmd, args):
    """The operands a deleting command removes, or [] when it deletes nothing."""
    if cmd in DELETERS:
        return [a for a in _fad._positionals(args, set()) if a]
    if cmd == "find":
        rest = " ".join(args)
        if "-delete" in args or re.search(r"-exec(dir)?\s+(\S*/)?(rm|rmdir|unlink|trash|srm)\b", rest):
            return _fad._walk_roots(cmd, args) or ["."]
    return []


def verdict(command, cwd=None, roots=None):
    """None when the command may run; otherwise the sentence the refusal prints."""
    if not command:
        return None
    roots = roots if roots is not None else claude_roots()
    assigned, here = {}, cwd
    for seg in _fad._segments(_fad._tokens(_fad._strip_heredocs(command))):
        # Assignments anywhere before the command word (`S=/x; rm -rf $S/*` and
        # `export S=/x`) are remembered for the rest of the line.
        words = [w for w in seg]
        if words and words[0] in ("export", "declare", "local", "readonly"):
            words = words[1:]
        for w in words:
            m = ASSIGN.match(w)
            if not m:
                break
            val = _subst(m.group(2), assigned) if m.group(2) else ""
            if val is not None:
                assigned[m.group(1)] = val
        cmd, args = _fad._command(seg)
        if cmd == "cd":
            target = _expand(args[0], assigned, here) if args else os.path.expanduser("~")
            here = os.path.normpath(target) if target else None
            continue
        for operand in _targets(cmd, args):
            path = _expand(operand, assigned, here)
            if path is None:
                continue
            base, globbed = _glob_base(path)
            why = protected(base, roots)
            if why:
                return ("`%s %s` would delete %s: %s."
                        % (cmd, operand, "everything matching it" if globbed else "it", why))
    return None


REFUSAL = """REFUSED (guard-shared-scratchpad): %s
  The session scratchpad is shared by the lead and every in-process teammate, so
  nothing in it can be shown to be yours, and deleting there destroys their work
  (2026-10-01: two teammates' cleanups took the lead's briefs and a live teammate's
  backup copy of a file it was mutating). Do not delete there at all: the scratch
  reaper removes the whole session directory after the session ends.
  Your own scratch is the directory returned by the engine's
  scripts/lib/machine_settings.py scratch <your-name> command (use it, delete it); only that directory is yours to clean up.
"""


def bash_check(payload_text):
    """PreToolUse[Bash]: 2 with the refusal on stderr, else 0. Unreadable input passes."""
    try:
        payload = json.loads(payload_text)
    except ValueError:
        return 0
    if not isinstance(payload, dict) or payload.get("tool_name") not in (None, "Bash"):
        return 0
    # The lead's own call carries no agent_id; the lead may tidy its own scratchpad.
    if not payload.get("agent_id"):
        return 0
    command = (payload.get("tool_input") or {}).get("command") or ""
    if not isinstance(command, str):
        return 0
    why = verdict(command, cwd=payload.get("cwd"))
    if not why:
        return 0
    sys.stderr.write(REFUSAL % why)
    return 2


def main(argv):
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0
    if argv[0] == "bash-check":
        try:
            return bash_check(sys.stdin.read())
        except Exception:          # any error in the check is a pass, never a block
            return 0
    if argv[0] == "verdict":
        why = verdict(argv[1] if len(argv) > 1 else "", cwd=argv[2] if len(argv) > 2 else None)
        print(why or "allowed")
        return 1 if why else 0
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
