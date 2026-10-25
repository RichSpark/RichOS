#!/usr/bin/env python3
"""mechanical-findings.py — A DEFECT THE TREE CAN SHOW YOU BECOMES A ROW
                            WITHOUT A PERSON RETYPING IT.

Read scripts/lib/mechanical-findings.sh first; it carries the argument and the
wiring. This file carries the mechanism, because the mechanism has to be exact.

===========================================================================
THE ONE SENTENCE
===========================================================================
    A CHECK THAT CANNOT RUN, A SUITE THAT IS SKIPPED, A HOOK NOBODY TESTS —
    THESE ARE FACTS ABOUT THE TREE. A FACT ABOUT THE TREE IS FOUND BY
    READING THE TREE, AND ONCE FOUND IT IS WRITTEN DOWN WHERE EVERY OTHER
    PIECE OF OPEN WORK IS WRITTEN DOWN, UNDER THE SAME RULES, BY THE SAME
    MACHINE THAT FOUND IT.

On 2026-09-02 an audit found eight real defects in twenty minutes that six
weeks of attention had missed. One of them — the only automated suite over the
"no wrong numbers" data contract, red and skipped in CI since 2026-07-19 — sat
under a comment reading "Tracked separately", tracked in none of the three
queue files. The finding was mechanical. The tracking was a person. The person
is the part that failed, three times: nothing asked for the audit, nothing
turned the finding into a row, and nothing noticed the row was never started.

This file is the middle link. It reads the tree, produces findings with a
stable identity, and appends rows to the working record in the record's own
format — a warrant and all — so the row goes stale, closes and is refused by
exactly the rules every hand-written row already lives under.

===========================================================================
WHAT IS A FINDING HERE — mechanical only, and the boundary is the CEO's
===========================================================================
Every class below is a statement a machine can make about bytes with no
opinion attached. Whether the defect MATTERS, whether the fix is worth doing,
whether the row should be closed as won't-fix — none of that is decided here,
and a class that needed a judgment to fire would be the wrong class.

  ci-excluded-suite   a test file whose basename appears, on a non-comment
                      line of a CI workflow, within a few lines of the word
                      "skip". The 2026-07-19 shape, exactly:
                          if [ "$name" = "client-data-check.test.sh" ]; then
                            echo "SKIP  $t"

  unrun-harness       a *.mutation.sh named on NO non-comment line of any
                      other script or workflow in its repository. The
                      engine's own runner globs *.test.sh, so a harness that
                      no suite invokes by name is run by nobody — seven of
                      thirteen were, on the day this was written, and the
                      comment "that discovery never saw it" sat beside one
                      of the six that were.

  untested-hook       a hook registered in a hook registry (hooks.json or a
                      .claude/settings*.json) that no test file names on a
                      non-comment line — where a file that names half or
                      more of the registered hooks is a registry checker,
                      not a test of this hook, and does not count. That
                      threshold is DERIVED per repository, never typed.

===========================================================================
IDENTITY — the same defect on three runs is one row, not three
===========================================================================
A finding's key is  <class>:<prefix>/<path>  — the class and the subject's
declared-root-relative path, nothing else. It is written INTO the row as
`finding:<key>` where a reader can see what the machine thinks the identity
is, and it is what the next sweep looks for before writing anything. Not a
sidecar ledger: two ledgers written by one writer and read by nobody are how
the 2026-09-02 waiver count reached 251.

A row that already carries the key, in ANY state, is never written again.
Three things follow, and each is REPORTED rather than acted on:

  KNOWN               the row exists and is open. Nothing to do here; the
                      unstarted-row sweep names it every turn until somebody
                      starts it. That is a different hook's job and it
                      already exists.
  GONE                the row exists and is open, and this sweep no longer
                      produces its finding. The defect was fixed, moved, or
                      deleted. The row is NOT touched — closing it means
                      somebody reads the sentence and decides it is true —
                      but it is named, so it does not sit there forever
                      describing a defect that is not there.
  CLOSED-BUT-PRESENT  the row says CLOSED and the sweep still produces the
                      finding. Two statements about one fact disagree; a
                      human resolves it.

A renamed subject is a NEW finding and its old row goes stale through the
warrant — `path`@`oid` no longer exists — which is the row-currency contract
doing precisely what it was built to do.

===========================================================================
WHAT IS NEVER DONE HERE
===========================================================================
  * No existing row is edited, re-stamped, closed or deleted. The record's
    own contract says there is deliberately no re-stamp command, and a writer
    that could touch old rows would be that command wearing a new name.
  * Nothing is written when the record cannot be parsed, when its governed
    section has no table to append to, when a stamp cannot be minted, or
    when another writer holds the lock. Each is reported, never worked
    around.
  * Nothing decides importance. The row says what was seen, where, and that
    a machine wrote it.

===========================================================================
INPUT — one JSON job on argv[1] (or "-" for stdin)
===========================================================================
    {
      "record_label":   "wiki/open-items.md",
      "record_path":    "/abs/path/to/wiki/open-items.md",
      "row_sections":   ["3"],
      "status_tokens":  ["OPEN", "BUILT", "BOUNDED", "BLOCKED-ON-RICH", "CLOSED"],
      "terminal_tokens":["CLOSED"],
      "roots":          {"richos": "/abs", "femcboost": "/abs"},   # on disk
      "absent_roots":   {"prefix": "../declared"},               # not here
      "today":          "2026-09-02",
      "hook_name":      "notice-mechanical-findings.sh",
      "write":          true | false,
      "lock_dir":       "/abs/path/to/lock"                       # mkdir lock
    }

OUTPUT — tab-separated lines on stdout. First line is the verdict:

    CLEAN     <subjects-checked>
    FINDINGS  <total> <new> <known> <gone> <contradictions> <written>
    BROKEN    <reason>

then, in a stable order:

    CLASS   <class>  <subjects-checked>  <findings>        the positive probe
    F       <NEW|KNOWN|GONE|CLOSED-BUT-PRESENT>  <key>  <row-id or ->  <sentence>
    WROTE   <row-id>  <key>  <line-number>
    EXEMPT  <key>  <declared reason>                       honored, never silent
    SKIP    <key>  <reason>                                could not pin, etc.
    NOTE    <text>

EXIT  0 always, unless the job itself is unreadable (2). The verdict is the
      product; a non-zero exit would make "there are findings" and "the sweep
      is broken" the same signal to every caller.
"""

import importlib.util
import json
import os
import re
import subprocess
import sys
import time

_HERE = os.path.dirname(os.path.abspath(__file__))

CLASSES = ("ci-excluded-suite", "unrun-harness", "untested-hook")

# The identity token as it appears inside a row. Backticked so it survives
# markdown, prefixed so a grep for it finds only machine-written rows.
FINDING_RE = re.compile(r"`finding:(?P<key>[^`\s]+)`")

# What counts as a TEST FILE, by name alone. Broad on purpose: the check that
# uses it asks "does ANY test name this hook?", and a narrow definition would
# fail toward a false finding, which is the loud direction — but a hook tested
# only by a Python suite under tests/ must not be reported as untested.
TEST_NAME_RE = re.compile(r"(\.test\.|\.mutation\.|^test_|_test\.|\.spec\.)")
TEST_DIR_RE = re.compile(r"(^|/)(tests?|__tests__|spec)(/|$)")

# The registration surfaces, and the one regex every engine parser of them
# already uses (scripts/lib/registered-hooks.sh). Same regex, so this cannot
# disagree with the probe about what is registered.
REGISTRY_RE = re.compile(r"(^|/)(hooks/hooks\.json|\.claude/settings(\.local)?\.json)$")
HOOK_CMD_RE = re.compile(r"scripts/hooks/([A-Za-z0-9._+-]+\.sh)")

WORKFLOW_RE = re.compile(r"(^|/)\.github/workflows/[^/]+\.ya?ml$")
MUTATION_RE = re.compile(r"\.mutation\.sh$")
SKIP_WORD_RE = re.compile(r"\bskip", re.I)

# Paths never scanned: other people's code, and worktree copies of this tree.
IGNORE_RE = re.compile(r"(^|/)(node_modules|\.worktrees|\.claude/worktrees|vendor|target|dist|build)(/|$)")

# A DECLARED exemption, in the source, where a reviewer sees it — the same
# discipline dialect-guard.sh uses (`dialect-exempt: <reason>`). A finding a
# person has judged deliberate would otherwise return as a fresh row every time
# its closed row left the page. A BARE marker exempts nothing: the reason is
# the declaration, and every exemption is reported, never silent.
EXEMPT_RE = re.compile(r"finding-exempt:\s*(?P<why>.*)")
EXEMPT_MIN_REASON = 4    # characters; "finding-exempt: no" is not a reason

SKIP_WINDOW = 8          # lines after a suite's name in which "skip" counts
LOCK_STALE_SECONDS = 120
STAMP_LEN = 12

GIT = "git"


def out(*cells):
    sys.stdout.write("\t".join(str(c) for c in cells) + "\n")


def fail(reason):
    out("BROKEN", reason)
    sys.exit(0)


# ===========================================================================
# THE RECORD'S GRAMMAR — borrowed, never copied
# ===========================================================================
def load_row_currency():
    """row-currency.py owns the row shape, the warrant and `identity()`. This
    file parses rows through it and mints stamps through it, so the rows it
    writes are checked by the same code that wrote their warrants."""
    p = os.path.join(_HERE, "row-currency.py")
    if not os.path.isfile(p):
        fail("scripts/lib/row-currency.py is missing at %s. The record's row "
             "grammar and the warrant's object-id stamp are defined there; "
             "this writer refuses to carry a second copy of either." % p)
    try:
        spec = importlib.util.spec_from_file_location("row_currency", p)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
    except Exception as exc:
        fail("scripts/lib/row-currency.py could not be loaded (%s)." % exc)
    for name in ("parse_record", "warrant_of", "span_text", "identity",
                 "STATUS_RE", "ABSENT"):
        if not hasattr(mod, name):
            fail("scripts/lib/row-currency.py no longer exposes %s; the row "
                 "grammar this writer depends on has moved." % name)
    return mod


# ===========================================================================
# READING THE TREE — at HEAD, so a dirty working copy cannot change a finding
# ===========================================================================
def vcs(root, args, timeout=30):
    try:
        p = subprocess.run([GIT, "-C", root] + args, capture_output=True,
                           text=True, timeout=timeout)
    except Exception:
        return None
    if p.returncode != 0:
        return None
    return p.stdout


def tracked_paths(root):
    o = vcs(root, ["ls-tree", "-r", "HEAD", "--name-only"])
    if o is None:
        return None
    return [p for p in o.split("\n") if p and not IGNORE_RE.search(p)]


def show(root, path):
    o = vcs(root, ["show", "HEAD:%s" % path])
    return o if o is not None else ""


def grep_refs(root, needles, pathspecs):
    """-> [(path, lineno, text)] for every line at HEAD under <pathspecs>
    containing any needle. One call per root, not one per needle."""
    if not needles or not pathspecs:
        return []
    args = ["grep", "-n", "-I", "-F"]
    for n in needles:
        args += ["-e", n]
    args += ["HEAD", "--"] + list(pathspecs)
    o = vcs(root, args)
    if not o:
        return []
    refs = []
    for line in o.split("\n"):
        if not line:
            continue
        m = re.match(r"^HEAD:(?P<p>[^:]+):(?P<n>\d+):(?P<t>.*)$", line)
        if not m:
            continue
        refs.append((m.group("p"), int(m.group("n")), m.group("t")))
    return refs


def is_comment(text):
    s = text.lstrip()
    return s.startswith("#") or s.startswith("//")


def is_test_file(path):
    base = os.path.basename(path)
    return bool(TEST_NAME_RE.search(base) or TEST_DIR_RE.search(os.path.dirname(path) + "/"))


def exemption_in(lines):
    """-> the declared reason, or ''. A marker with no reason is no marker."""
    for line in lines:
        m = EXEMPT_RE.search(line)
        if not m:
            continue
        why = m.group("why").strip()
        if len(why) >= EXEMPT_MIN_REASON:
            return why
    return ""


# ===========================================================================
# THE CLASSES
# ===========================================================================
def find_ci_excluded_suites(prefix, root, paths, census):
    """A test file named on a non-comment workflow line with 'skip' nearby."""
    suites = [p for p in paths if is_test_file(p)]
    workflows = [p for p in paths if WORKFLOW_RE.search(p)]
    census["ci-excluded-suite"] = len(suites) if workflows else 0
    findings = []
    if not suites or not workflows:
        return findings
    by_base = {}
    for s in suites:
        by_base.setdefault(os.path.basename(s), []).append(s)
    for wf in workflows:
        lines = show(root, wf).split("\n")
        for i, line in enumerate(lines):
            if is_comment(line):
                continue
            for base, sp in by_base.items():
                if base not in line:
                    continue
                window = lines[i:i + SKIP_WINDOW + 1]
                if not any(SKIP_WORD_RE.search(w) for w in window):
                    continue
                exempt = exemption_in(window)
                for s in sp:
                    findings.append({
                        "class": "ci-excluded-suite",
                        "key": "ci-excluded-suite:%s/%s" % (prefix, s),
                        "prefix": prefix, "root": root,
                        "subject": s, "evidence": [wf],
                        "line": i + 1, "exempt": exempt,
                        "headline": "A test suite is excluded from CI by name: `%s/%s` is skipped in `%s/%s` (line %d)."
                                    % (prefix, s, prefix, wf, i + 1),
                        "sentence": "The workflow names this suite and skips it, so every invariant the suite asserts is unverified in CI for as long as the exclusion stands, and a note beside an exclusion is not a row anywhere.",
                    })
    seen = set()
    uniq = []
    for f in findings:
        if f["key"] in seen:
            continue
        seen.add(f["key"])
        uniq.append(f)
    return uniq


def find_unrun_harnesses(prefix, root, paths, census):
    """A *.mutation.sh named on no non-comment line of any OTHER .sh/.yml."""
    harnesses = [p for p in paths if MUTATION_RE.search(p)]
    census["unrun-harness"] = len(harnesses)
    findings = []
    if not harnesses:
        return findings
    needles = sorted({os.path.basename(h) for h in harnesses})
    refs = grep_refs(root, needles, ["*.sh", "*.yml", "*.yaml", "*.bash"])
    named = {}
    for path, n, text in refs:
        if is_comment(text) or IGNORE_RE.search(path):
            continue
        for needle in needles:
            if needle in text:
                named.setdefault(needle, set()).add(path)
    for h in harnesses:
        base = os.path.basename(h)
        callers = {p for p in named.get(base, set()) if p != h}
        if callers:
            continue
        findings.append({
            "class": "unrun-harness",
            "key": "unrun-harness:%s/%s" % (prefix, h),
            "prefix": prefix, "root": root,
            "subject": h, "evidence": [],
            "exempt": exemption_in(show(root, h).split("\n")),
            "headline": "A mutation harness is run by nothing: `%s/%s` is named on no non-comment line of any script or workflow in its repository."
                        % (prefix, h),
            "sentence": "The properties it proves load-bearing are proven only when somebody runs it by hand; a runner that discovers suites by one glob never sees it.",
        })
    return findings


def find_untested_hooks(prefix, root, paths, census):
    """A registered hook script that no non-omnibus test file names."""
    registries = [p for p in paths if REGISTRY_RE.search(p)]
    tests = [p for p in paths if is_test_file(p)]
    registered = {}   # hook basename -> (hook path, registry path)
    for reg in registries:
        content = show(root, reg)
        for name in sorted(set(HOOK_CMD_RE.findall(content))):
            candidates = [p for p in paths if p.endswith("scripts/hooks/" + name)]
            if not candidates:
                continue   # registered but absent: the probe's BR4 finding, not this one
            regdir = os.path.dirname(reg)
            anchor = regdir.rsplit("/", 1)[0] if "/" in regdir else ""
            best = None
            for c in candidates:
                if anchor == "" or c.startswith(anchor + "/"):
                    best = c
                    break
            if best is None:
                best = candidates[0]
            registered.setdefault(name, (best, reg))
    census["untested-hook"] = len(registered)
    findings = []
    if not registered:
        return findings
    needles = sorted(registered)
    refs = grep_refs(root, needles, tests) if tests else []
    named = set()   # hook basenames some test file names on a non-comment line
    for path, n, text in refs:
        if is_comment(text):
            continue
        for needle in needles:
            if needle in text:
                named.add(needle)
    for name in needles:
        if name in named:
            continue
        hook_path, reg = registered[name]
        findings.append({
            "class": "untested-hook",
            "key": "untested-hook:%s/%s" % (prefix, hook_path),
            "prefix": prefix, "root": root,
            "subject": hook_path, "evidence": [reg],
            "exempt": exemption_in(show(root, hook_path).split("\n")),
            "headline": "A registered hook is named by no test: `%s/%s` is wired in `%s/%s` and no test file in its repository names it on a non-comment line."
                        % (prefix, hook_path, prefix, reg),
            "sentence": "Nothing has ever been shown to fail because this hook stopped working, so its green means nothing.",
        })
    return findings


# ===========================================================================
# THE RECORD — what is already written down
# ===========================================================================
def existing_findings(rc, text, row_sections, status_tokens, terminal):
    """-> ({key: {"id", "token", "closed"}}, items)

    A record the landing guard would refuse is not one to append to, and a
    governed section that parses to no rows is an UNREAD section, not an
    empty one — the same rule unstarted-rows.py applies. Both are BROKEN."""
    items, violations, seen = rc.parse_record(text, row_sections)
    if violations:
        fail("the record does not parse cleanly (%s: %s); nothing is appended to a "
             "record the landing guard would refuse." % (violations[0][1], violations[0][2][:120]))
    if not any(it.get("governed") for it in items):
        fail("section %s of the record parsed to ZERO rows. That is not an empty "
             "section, it is an unread one, and nothing is appended to it."
             % "/".join(row_sections))
    found = {}
    for it in items:
        if not it.get("governed"):
            continue
        body = rc.span_text(it)
        for m in FINDING_RE.finditer(body):
            key = m.group("key")
            tok = ""
            w = rc.warrant_of(it)
            if w:
                sm = rc.STATUS_RE.match(w)
                if sm:
                    tok = sm.group("tok")
            found.setdefault(key, {"id": it["id"], "token": tok,
                                   "closed": tok in terminal})
    return found, items


def has_row(known, key):
    """The one dedup predicate, used at sweep time AND under the lock. One
    function, so the property 'the same defect is one row' lives in one place
    and a mutation harness can remove it in one place."""
    return key in known


def next_ids(items, section, count):
    """The next <section>.<N> ids, above every numeric id already in it."""
    top = 0
    for it in items:
        if it["section"] != section:
            continue
        m = re.match(r"^(\d+)\.(\d+)", it["id"])
        if m and int(m.group(2)) > top:
            top = int(m.group(2))
    return ["%s.%d" % (section, top + i + 1) for i in range(count)]


def insertion_line(items, section):
    """1-based line number AFTER which new rows go: the section's last table row."""
    last = None
    for it in items:
        if it["section"] == section and it.get("shape") == "table":
            last = it
    if last is None:
        return None
    return last["line0"] + len(last["span"]) - 1


def mint_warrant(rc, f):
    """-> (warrant, None) or (None, reason). Every stamp comes from the same
    identity() the landing guard reads, so the row is born current."""
    stamps = []
    for rel in [f["subject"]] + list(f.get("evidence") or []):
        oid = rc.identity(f["root"], "HEAD", rel)
        if oid is None or oid == rc.ABSENT:
            return None, "could not identify `%s/%s` at HEAD of %s" % (f["prefix"], rel, f["root"])
        stamps.append("`%s/%s`@`%s`" % (f["prefix"], rel, oid[:STAMP_LEN]))
    return "**State:** `OPEN` — " + ", ".join(stamps), None


def row_text(f, rid, warrant, today, hook_name):
    return ("| %s | **%s** %s Written by the mechanical sweep (`%s`) on %s, not by a person: "
            "it is a fact about the tree at HEAD, not a judgment about importance. "
            "`finding:%s` | %s |"
            % (rid, f["headline"], f["sentence"], hook_name, today, f["key"], warrant))


# ===========================================================================
# THE LOCK — two sessions may both reach this file; one writes at a time
# ===========================================================================
def take_lock(lock_dir):
    if not lock_dir:
        return True, ""
    parent = os.path.dirname(lock_dir)
    try:
        os.makedirs(parent, exist_ok=True)
    except Exception as exc:
        return False, "lock directory %s cannot be created (%s)" % (parent, exc)
    for attempt in range(2):
        try:
            os.mkdir(lock_dir)
            return True, ""
        except FileExistsError:
            try:
                age = time.time() - os.stat(lock_dir).st_mtime
            except Exception:
                age = 0
            if age > LOCK_STALE_SECONDS and attempt == 0:
                try:
                    os.rmdir(lock_dir)
                except Exception:
                    pass
                continue
            return False, "another writer holds %s (age %ds)" % (lock_dir, int(age))
        except Exception as exc:
            return False, "lock %s cannot be taken (%s)" % (lock_dir, exc)
    return False, "lock %s could not be taken" % lock_dir


def release_lock(lock_dir):
    if not lock_dir:
        return
    try:
        os.rmdir(lock_dir)
    except Exception:
        pass


# ===========================================================================
def main():
    if len(sys.argv) < 2:
        sys.stderr.write("usage: mechanical-findings.py <job.json | ->\n")
        sys.exit(2)
    try:
        raw = sys.stdin.read() if sys.argv[1] == "-" else open(sys.argv[1], encoding="utf-8").read()
        job = json.loads(raw)
    except Exception as exc:
        sys.stderr.write("mechanical-findings.py: unreadable job: %s\n" % exc)
        sys.exit(2)

    rc = load_row_currency()

    record_path = job.get("record_path") or ""
    record_label = job.get("record_label") or record_path
    row_sections = [str(s) for s in (job.get("row_sections") or [])]
    status_tokens = job.get("status_tokens") or []
    terminal = set(job.get("terminal_tokens") or [])
    roots = job.get("roots") or {}
    absent = job.get("absent_roots") or {}
    today = job.get("today") or time.strftime("%Y-%m-%d")
    hook_name = job.get("hook_name") or "notice-mechanical-findings.sh"
    write = bool(job.get("write"))
    lock_dir = job.get("lock_dir") or ""

    if not row_sections:
        fail("no governed section declared, so there is nowhere a finding could be written.")
    section = row_sections[0]
    if not roots:
        fail("the record declares no artifact root that is on this machine, so there is no tree to read.")

    try:
        with open(record_path, encoding="utf-8") as fh:
            text = fh.read()
    except Exception as exc:
        fail("the record %s could not be read (%s)." % (record_label, exc))

    # --- sweep ---------------------------------------------------------------
    census = {}
    findings = []
    notes = []
    for prefix in sorted(roots):
        root = roots[prefix]
        paths = tracked_paths(root)
        if paths is None:
            notes.append("root '%s' (%s) has no readable HEAD; it was not swept." % (prefix, root))
            continue
        c = {}
        findings += find_ci_excluded_suites(prefix, root, paths, c)
        findings += find_unrun_harnesses(prefix, root, paths, c)
        findings += find_untested_hooks(prefix, root, paths, c)
        for k, v in c.items():
            census[k] = census.get(k, 0) + v
    for prefix, spec in sorted(absent.items()):
        notes.append("declared root '%s' (%s) is not on this machine and was not swept." % (prefix, spec))

    subjects = sum(census.get(k, 0) for k in CLASSES)
    if subjects == 0:
        fail("every class checked ZERO subjects across %d root(s) — no test file, no mutation harness, no registry was found. That is not a clean tree; it is an unread one." % len(roots))

    # A declared exemption is honored and REPORTED. It never becomes a row and
    # it never disappears from the output: silence is how an exemption would
    # decay into a rumor.
    exempted = [f for f in findings if f.get("exempt")]
    findings = [f for f in findings if not f.get("exempt")]

    # --- against the record --------------------------------------------------
    known, items = existing_findings(rc, text, row_sections, status_tokens, terminal)
    produced = {f["key"]: f for f in findings}

    rows_out = []
    new = []
    for f in findings:
        k = f["key"]
        if has_row(known, k):
            if known[k]["closed"]:
                rows_out.append(("CLOSED-BUT-PRESENT", k, known[k]["id"], f["headline"]))
            else:
                rows_out.append(("KNOWN", k, known[k]["id"], f["headline"]))
        else:
            new.append(f)
            rows_out.append(("NEW", k, "-", f["headline"]))
    gone = []
    for k, info in sorted(known.items()):
        cls = k.split(":", 1)[0]
        if cls not in CLASSES:
            notes.append("row %s carries `finding:%s`, a class this sweep does not run; left alone." % (info["id"], k))
            continue
        if k in produced or info["closed"]:
            continue
        gone.append((k, info["id"]))
        rows_out.append(("GONE", k, info["id"],
                         "the sweep no longer produces this finding; the row still describes it as open"))

    # --- write ---------------------------------------------------------------
    written = []
    skips = []
    if write and new:
        ok, why = take_lock(lock_dir)
        if not ok:
            notes.append("WRITE REFUSED: %s. Nothing was appended; the findings above are still findings." % why)
        else:
            try:
                # Re-read under the lock: another writer may have appended
                # between the sweep and now, and a key written twice is the
                # one thing this file exists not to do.
                with open(record_path, encoding="utf-8") as fh:
                    text2 = fh.read()
                known2, items2 = existing_findings(rc, text2, row_sections, status_tokens, terminal)
                at = insertion_line(items2, section)
                if at is None:
                    notes.append("WRITE REFUSED: section %s of %s has no table row to append after." % (section, record_label))
                else:
                    pending = []
                    for f in new:
                        if has_row(known2, f["key"]):
                            continue
                        warrant, why = mint_warrant(rc, f)
                        if warrant is None:
                            skips.append((f["key"], why))
                            continue
                        pending.append((f, warrant))
                    ids = next_ids(items2, section, len(pending))
                    lines = text2.split("\n")
                    new_lines = [row_text(f, rid, w, today, hook_name)
                                 for (f, w), rid in zip(pending, ids)]
                    if new_lines:
                        lines[at:at] = new_lines
                        tmp = record_path + ".mechanical-findings.tmp"
                        with open(tmp, "w", encoding="utf-8") as fh:
                            fh.write("\n".join(lines))
                        os.replace(tmp, record_path)
                        for i, ((f, w), rid) in enumerate(zip(pending, ids)):
                            written.append((rid, f["key"], at + i + 1))
            finally:
                release_lock(lock_dir)
    elif new and not write:
        for f in new:
            warrant, why = mint_warrant(rc, f)
            if warrant is None:
                skips.append((f["key"], why))

    # --- report --------------------------------------------------------------
    n_new = sum(1 for r in rows_out if r[0] == "NEW")
    n_known = sum(1 for r in rows_out if r[0] == "KNOWN")
    n_contra = sum(1 for r in rows_out if r[0] == "CLOSED-BUT-PRESENT")
    total = len(findings)
    if total == 0 and not gone:
        out("CLEAN", subjects)
    else:
        out("FINDINGS", total, n_new, n_known, len(gone), n_contra, len(written))
    for cls in CLASSES:
        out("CLASS", cls, census.get(cls, 0), sum(1 for f in findings if f["class"] == cls))
    order = {"NEW": 0, "CLOSED-BUT-PRESENT": 1, "GONE": 2, "KNOWN": 3}
    for state, key, rid, sentence in sorted(rows_out, key=lambda r: (order[r[0]], r[1])):
        out("F", state, key, rid, sentence)
    for rid, key, line in written:
        out("WROTE", rid, key, line)
    for f in sorted(exempted, key=lambda f: f["key"]):
        out("EXEMPT", f["key"], f["exempt"])
    for key, why in skips:
        out("SKIP", key, why)
    for n in notes:
        out("NOTE", n)
    sys.exit(0)


if __name__ == "__main__":
    main()
