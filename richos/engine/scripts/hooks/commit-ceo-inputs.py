#!/usr/bin/env python3
"""commit-ceo-inputs.py — THE INGRESS. A FILE HE HANDS OVER ENTERS THE RECORD.

Reads a UserPromptSubmit payload on stdin. Does one job:

    EVERY FILE THIS MESSAGE HANDS OVER THAT THE RECORD IS NOT HOLDING IS
    COMMITTED, UNMODIFIED, RIGHT NOW — UNLESS A SAFETY GATE REFUSES, IN WHICH
    CASE NOTHING IS COMMITTED AND THE REFUSAL IS SAID OUT LOUD.

The wrapper (commit-ceo-inputs.sh) owns the wiring and the notice channel.
This file owns the predicate, the gates, the git plumbing and the refusals.

===========================================================================
THE DEFECT, AND THE DESIGN IT ALREADY KILLED ONCE
===========================================================================
2026-09-05. The CEO wrote a specification, handed it over with "handle this",
and named its path. It was read, verified, turned into four questions and a
dispatch that changed a PUBLIC repository — while the file itself sat
UNTRACKED on his disk. The document driving live changes existed in exactly
one place and nothing but a hard drive was holding it. He noticed. Nothing
else could have.

THE FIRST VERSION OF THIS FILE ONLY NOTICED. It found the untracked file,
told the orchestrator, committed nothing, and explained that committing is a
judgment. He rejected it in one line:

    "'it does not commit for you': Then who commits for me? Santa Claus?"

He is right, and the reason is structural rather than rhetorical. His entire
complaint was that a document went uncommitted because the orchestrator
forgot. A mechanism whose last link is the orchestrator remembering is not a
mechanism; it is the same failure with a reminder attached. So the judgment
does not get reserved for a human step — IT GETS MADE MECHANICAL.

===========================================================================
CAN A HOOK ACTUALLY COMMIT? MEASURED, NOT ASSUMED
===========================================================================
Claude Code 2.1.261, from the shipped binary:

  * a `command` hook is an ordinary spawned process with the payload on stdin.
    It is not sandboxed and there is no restriction on side effects.
  * UserPromptSubmit's default timeout is 30000 ms, overridable per
    registration. A local git commit is milliseconds; the budget is not close.
  * exit 0 -> stdout shown to Claude. exit 2 -> BLOCK PROCESSING AND ERASE THE
    ORIGINAL PROMPT. Other codes -> stderr to the user only.

The precedent is in this engine already: terminalize-agent-worktrees.sh
performs `git update-ref` from a SubagentStop hook, and
snapshot-agent-definitions.sh writes files from SessionStart. Git side effects
from hooks are established practice here, not a new capability being invented.

The measured erase-on-exit-2 behavior is why this hook's only exit code is 0.
A refusal must never cost him the message he typed.

===========================================================================
THE GATES — REUSED, NEVER REWRITTEN
===========================================================================
A commit happens unless one of these refuses. Each is a decision this engine
ALREADY makes somewhere; none is a second implementation.

  1. CREDENTIALS      scripts/hooks/scan-secrets.sh, invoked as a subprocess
                      with a synthesized Write payload. Vendor-prefix keys and
                      entropy-gated literals, findings already redacted so the
                      refusal is not a second leak.

  2. PRIVATE MATERIAL scripts/hooks/guard-publication-writes.sh, same shape.
                      This is the third-party-personal-data gate available
                      TODAY: it refuses private speech, transcripts and
                      identity-declared files entering a publication-bound
                      repository. Row p1's named-person deny-list is being
                      built into that same boundary, so when it lands this gate
                      inherits it WITH NO EDIT HERE — which is exactly why the
                      guard is invoked rather than its library.

                      BEFORE p1 EXISTS, said plainly rather than implied: a
                      third party's name in ordinary prose, in a repository
                      that declares no publication boundary, is not detected by
                      anything. That is a real hole and it is stated here so
                      nobody reads this list as complete.

  3. THE ROOT         A path directly at a repository root is never committed
                      automatically. richos's root is nine entries by the CEO's
                      standing decision, and a repository's front door is a
                      structural choice he reserves. The refusal names the
                      subdirectory it should live in instead.

  4. UNSCANNABLE      A file the content gates cannot read — over the size cap,
                      or not valid UTF-8 — is not committed. "The scanner could
                      not look" is never rounded to "the scanner found
                      nothing"; that rounding is this repository's single
                      most-repeated failure.

  5. NO SEAT          Gates 1 and 2 are run FROM the session's own adopted
                      repository, never from the file's. Without a seat there
                      is nowhere to run them from, nothing is known about the
                      content, and nothing is committed.

                      THE FIRST DRAFT GOT THIS BACKWARDS AND IT WOULD HAVE
                      SHIPPED. It tested the FILE's repository for
                      orchestration.config, which reads as obviously right —
                      and measured against this machine, NEITHER `richos` NOR
                      `richos-hq` carries one at its root; the engine sits one
                      level down, inside richos. So the mechanism would have
                      refused every hand-over in the two repositories his
                      specifications actually live in, and would have given a
                      safety-shaped reason for doing it. Run from the seat, the
                      credential scanner scans a file in ANY repository — which
                      its own header says is deliberate, because a secret in
                      someone else's tree is still a leaked secret — while the
                      publication boundary still reads its declaration out of
                      the file's own tree, where that question belongs.

  6. GIT IS NOT READY Detached HEAD, an unborn branch, a merge/rebase/
                      cherry-pick/revert/bisect in progress, a symlink, a path
                      inside .git, or a path inside a live agent worktree
                      (.claude/worktrees/) — a sealed transaction that belongs
                      to somebody else. Refused, never forced.

  7. A PASTE          More than MAX_COMMITS paths in one message is a paste,
                      not a handover. Nothing is committed and the count is
                      reported.

`outside every repository` is not a gate refusal, it is a different case: the
file is unheld and there is no obvious repository to hold it, so it is
REPORTED with the question of where it belongs. Committing it somewhere chosen
by a hook would be the hook making the one judgment it genuinely cannot.

===========================================================================
HOW IT COMMITS — HIS FILE IS NEVER TOUCHED, AND NEITHER IS ANYONE'S INDEX
===========================================================================
The naive form is `git add` then `git commit`, and it is wrong twice: it
mutates the real index while an orchestrator may have work staged there, and a
plain `git commit` would sweep that staged work into HIS commit.

So the commit is built through plumbing, from HEAD's tree plus this one blob,
in a THROWAWAY index:

    hash-object -w                      the file's bytes, unmodified
    read-tree HEAD (into a temp index)  everybody else's staged work excluded
    update-index --add --cacheinfo      exactly one path added
    write-tree / commit-tree            a commit containing HEAD + this file
    update-ref <branch> <new> <old>     COMPARE-AND-SWAP, so a concurrent
                                        commit loses the race loudly instead of
                                        being silently clobbered
    update-index --add --cacheinfo      the REAL index, that one path only, so
      (real index, last)                `git status` does not then report his
                                        file as a staged deletion

The working tree is never written. His document is read and never modified —
corrections belong somewhere else, always.

===========================================================================
NEVER THE CONTENT
===========================================================================
The file is read once, to hand to the gates and to hash. Nothing derived from
its bytes is ever printed: not an excerpt, not a first line, not a matched
quote. When the publication gate refuses, only the DETECTOR NAMES travel out
of here — its own refusal text quotes evidence, which is right for a human
reading a terminal and wrong for a notice injected into a transcript.

===========================================================================
EXIT CODES — the wrapper depends on these
===========================================================================
    0   ran; nothing handed over needed holding (or nothing was named)
    3   ran; at least one file COMMITTED
    4   ran; at least one file REFUSED, blocked, or undecided
    1   COULD NOT RUN (unreadable payload, no `prompt`, git absent)

3 and 4 can both apply; 4 wins, because a refusal is the thing that needs a
person. Every code except 1 prints one JSON object on stdout.
"""

import json
import os
import re
import subprocess
import stat
import sys
import time

# A message naming more paths than this is a paste, not a handover.
MAX_CANDIDATES = 60

# More commits than this from one message is refused wholesale (gate 7).
MAX_COMMITS = 10

# Above this the content gates cannot reasonably scan, so nothing is committed.
MAX_FILE_BYTES = 2 * 1024 * 1024

# Per-git-call ceiling. A hung git must not sit on his message.
GIT_TIMEOUT_S = 8

# The content gates get their own budget; they build a corpus and can be slower.
GATE_TIMEOUT_S = 10

# THE WHOLE RUN'S BUDGET, and it exists because he is waiting behind it. This
# hook sits between him pressing enter and the model seeing his message, so a
# slow repository must not become a slow product. The event's default timeout is
# 30000 ms and the registration asks for 25; this deadline sits below both so
# the process reports its own overrun instead of being killed mid-file with
# nothing written and nothing said. Files not reached are REPORTED as
# not-examined, never dropped: a file that was skipped for time is not a file
# that was found clean.
DEADLINE_S = 18

# A single monotonic deadline bounds every subprocess, including classification,
# companion gates and commit plumbing. Imported helpers have no run deadline.
_deadline = None


class IngressDeadline(Exception):
    pass


def check_budget():
    if _deadline is not None and time.monotonic() >= _deadline:
        raise IngressDeadline("the ingress ran out of its %ds budget; remaining work was not examined" % DEADLINE_S)


def run_command(*args, timeout, **kwargs):
    check_budget()
    if _deadline is not None:
        timeout = min(timeout, max(0, _deadline - time.monotonic()))
        if timeout <= 0:
            check_budget()
    return subprocess.run(*args, timeout=timeout, **kwargs)


TRAILING = ".,;:!?—-’'\")]}>*"
LEADING = "(“\"'[{<*"

INLINE_CODE = re.compile(r"`([^`\n]{1,400})`")

# Absolute and tilde tokens. The character after the root MUST be path-ish;
# that single lookahead is what keeps `1 / 2`, a bare `/`, a markdown rule and
# a regex like `s/a/b/` out of the candidate list entirely.
PATH_TOKEN = re.compile(
    r"(?<![A-Za-z0-9_/~.\-])"
    r"(~?/[A-Za-z0-9._~\-]"
    r"(?:\\ |[^\s`'\"<>|])*)"
)

QUOTED_PATH = re.compile(r"""["']\s*(~?/[^"'\n]{1,400}?)\s*["']""")

GIT_IN_PROGRESS = (
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
    "rebase-apply",
    "rebase-merge",
)


# ---------------------------------------------------------------------------
# EXTRACTION
# ---------------------------------------------------------------------------
def candidates(text):
    """Every path-shaped string in the message, with the source that found it.

    Three sources and no fourth: absolute tokens, `~`-rooted tokens, and the
    content of an inline code span. BARE WORDS ARE NEVER GUESSED AT — not
    `spec`, not `the plan`, not a naked `notes.md`. That is not politeness: a
    mechanism that fires on ordinary English fires on every message, and one
    that fires on every message gets switched off, after which it protects
    nothing at all. A missed handover costs one file. A mechanism nobody
    tolerates costs all of them, permanently.
    """
    # Automated task completion payloads are transport metadata, not handovers.
    text = re.sub(r"<(task-notification|system-reminder)\b[^>]*>.*?</\1>", "", text, flags=re.S)
    found = []
    seen = set()

    def add(raw, source):
        raw = raw.strip()
        if raw and raw not in seen:
            seen.add(raw)
            found.append((raw, source))

    for m in QUOTED_PATH.finditer(text):
        add(m.group(1), "quoted")

    for m in INLINE_CODE.finditer(text):
        inner = m.group(1).strip()
        # One token is a path. Two is a command line — from which the scanner
        # below still lifts any absolute path, which is the right split.
        if inner and not re.search(r"\s", inner.replace("\\ ", "")):
            add(inner, "backtick")

    for m in PATH_TOKEN.finditer(text):
        add(m.group(1), "absolute" if m.group(1).startswith("/") else "tilde")

    return found


def is_handover_file(path):
    """Exclude directories, devices, OS tools and Claude operational state.

    A symlink to a regular file, including a dangling link, still reaches the
    existing refusal gates. A symlink to a directory is not a file handover.
    This predicate is also used to recheck historical notice entries.
    """
    if os.path.isdir(path):
        return False
    try:
        mode = os.lstat(path).st_mode
    except FileNotFoundError:
        return False
    except OSError:
        return True  # Unknown is left to the existing visible refusal path.
    if not (stat.S_ISREG(mode) or stat.S_ISLNK(mode)):
        return False
    if any(path == root or path.startswith(root + "/")
           for root in ("/bin", "/sbin", "/usr/bin", "/usr/sbin", "/dev", "/proc", "/sys")):
        return False
    claude = os.path.realpath(os.path.join(os.path.expanduser("~"), ".claude"))
    if path in {os.path.join(claude, name) for name in ("settings.json", "settings.local.json")}:
        return False
    if any(path.startswith(os.path.join(claude, name) + "/")
           for name in ("state", "teams", "projects")):
        return False
    if re.search(r"/(?:private/)?tmp/claude-[^/]+/.+/tasks/[^/]+\.output$", path):
        return False
    return True


def physical(p):
    """One spelling for one file, so every later comparison asks one question.

    `git rev-parse --show-toplevel` answers PHYSICALLY. A path the CEO typed,
    or one that arrived through the payload's cwd, is whatever spelling he was
    handed — and on macOS `/tmp` and `/var` are symbolic links, so the two
    disagree routinely. Then `os.path.relpath(file, repo)` returns something
    beginning `../../../private/var/...` and `git update-index --cacheinfo`
    refuses it with a bare rc=128.

    THAT IS NOT HYPOTHETICAL: it is what a replay of the 2026-09-05 incident
    did on the first run, in a sandbox reached through /var, while the test
    suite was green because its own sandbox path had already been physicalized
    by `pwd -P`. The engine has the same fix in pb_physical for the same
    reason.

    The DIRECTORY is resolved and the basename is kept, never the whole path:
    resolving the last component would silently turn a symbolic link into its
    target, and a link is something this hook refuses rather than follows.
    """
    d = os.path.dirname(p) or "/"
    b = os.path.basename(p)
    try:
        rd = os.path.realpath(d)
    except OSError:
        return p
    return os.path.join(rd, b) if b else rd


def resolve(raw, cwd):
    """One raw candidate -> an absolute path that exists on disk, or None.

    Trailing punctuation is stripped one character at a time, longest form
    tried first, so `see /Users/alex/spec.md.` finds the file while a file that
    genuinely ends in a dot still wins on the first attempt.
    """
    forms = []
    s = raw
    while s and s[0] in LEADING:
        s = s[1:]
    forms.append(s)
    while s and s[-1] in TRAILING:
        s = s[:-1]
        if s:
            forms.append(s)

    for form in forms:
        p = form.replace("\\ ", " ")
        if p.startswith("~"):
            p = os.path.expanduser(p)
            if p.startswith("~"):
                continue
        if not os.path.isabs(p):
            if not cwd:
                continue
            p = os.path.join(cwd, p)
        p = os.path.abspath(p)
        # lexists, not exists: a dangling symlink he hands over is a real path
        # in a real state, and exists() would call it absent.
        if os.path.lexists(p):
            return physical(p)
    return None


# ---------------------------------------------------------------------------
# GIT
# ---------------------------------------------------------------------------
def git(args, cwd, timeout=GIT_TIMEOUT_S, stdin_bytes=None):
    """(rc, stdout, undecided_reason). A non-empty third element is never a pass."""
    try:
        r = run_command(
            ["git", "--no-optional-locks"] + args,
            cwd=cwd,
            input=stdin_bytes,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
    except IngressDeadline as exc:
        return (124, "", str(exc))
    except FileNotFoundError:
        return (127, "", "git-absent")
    except subprocess.TimeoutExpired:
        return (124, "", "git-timeout")
    except OSError as exc:
        return (125, "", "git-oserror:%s" % exc.__class__.__name__)
    return (r.returncode, r.stdout.decode("utf-8", "replace").strip(), "")


def classify(path):
    """(state, repo_root, detail). Never opens the file.

    States: tracked-clean, tracked-modified, ignored, outside-repo, untracked,
    indeterminate. Only `untracked` is something the record is not holding.
    """
    base = path if os.path.isdir(path) else os.path.dirname(path) or "/"
    if not os.path.isdir(base):
        return ("indeterminate", "", "containing directory vanished")

    rc, top, undecided = git(["rev-parse", "--show-toplevel"], base)
    if undecided:
        return ("indeterminate", "", undecided)
    if rc != 0 or not top:
        return ("outside-repo", "", "")

    rc, out, undecided = git(["ls-files", "--", path], top)
    if undecided:
        return ("indeterminate", top, undecided)
    if rc != 0:
        return ("indeterminate", top, "ls-files rc=%d" % rc)
    if out:
        rc, status, undecided = git(
            ["status", "--porcelain", "--untracked-files=no", "--", path], top
        )
        if undecided:
            return ("indeterminate", top, undecided)
        if rc != 0:
            return ("indeterminate", top, "status rc=%d" % rc)
        return ("tracked-modified" if status else "tracked-clean", top, "")

    rc, _o, undecided = git(["check-ignore", "-q", "--", path], top)
    if undecided:
        return ("indeterminate", top, undecided)
    if rc == 0:
        return ("ignored", top, "")
    if rc == 1:
        return ("untracked", top, "")
    return ("indeterminate", top, "check-ignore rc=%d" % rc)


def companion_files(path, repo):
    """Untracked, unignored files that belong with `path`, as absolute paths.

    One rule: an input under `wiki/raw/` ends its stem in an id (the video id
    in `...-watch-this-D8PikZ1KhUo.md`), and the run that produced it wrote
    `wiki/raw/assets/<id>-<anything>/`. Those folders are its companions.
    """
    rel = os.path.relpath(path, repo)
    parts = rel.split(os.sep)
    if len(parts) != 3 or parts[:1] != ["wiki"] or parts[1] != "raw":
        return []
    stem = os.path.splitext(parts[2])[0]
    vid = stem.rsplit("-", 1)[-1]
    if not re.fullmatch(r"[A-Za-z0-9_]{6,}", vid):
        return []
    assets = os.path.join(repo, "wiki", "raw", "assets")
    if not os.path.isdir(assets):
        return []
    out = []
    for name in sorted(os.listdir(assets)):
        check_budget()
        if not name.startswith(vid + "-"):
            continue
        d = os.path.join(assets, name)
        if os.path.islink(d) or not os.path.isdir(d):
            continue
        rc, listing, undecided = git(
            ["ls-files", "--others", "--exclude-standard", "-z", "--", d], repo)
        if undecided or rc != 0:
            continue
        out += [os.path.join(repo, f) for f in listing.split("\0") if f]
    return out


# ---------------------------------------------------------------------------
# GATES
# ---------------------------------------------------------------------------
def gate_git_ready(path, repo):
    """Gate 6. Returns a refusal string, or "" when git is in a fit state."""
    if os.path.islink(path):
        return ("it is a symbolic link, not a document — a link commits a "
                "pointer, and what it points at may live anywhere")
    if os.path.isdir(path):
        return ("it is a directory — an ingress commit takes one named file, "
                "never a tree whose contents nobody enumerated")

    rc, gitdir, undecided = git(["rev-parse", "--absolute-git-dir"], repo)
    if undecided or rc != 0 or not gitdir:
        return "the repository's git directory could not be resolved (%s)" % (
            undecided or "rc=%d" % rc
        )
    for marker in GIT_IN_PROGRESS:
        if os.path.exists(os.path.join(gitdir, marker)):
            return ("a %s is in progress in this repository — committing into "
                    "the middle of one is how a half-finished operation gets "
                    "buried" % marker)

    rc, _b, undecided = git(["symbolic-ref", "--quiet", "HEAD"], repo)
    if undecided:
        return "HEAD could not be read (%s)" % undecided
    if rc != 0:
        return ("HEAD is detached — a commit here would belong to no branch "
                "and would be lost by the next checkout")

    rc, _h, undecided = git(["rev-parse", "--verify", "HEAD"], repo)
    if undecided or rc != 0:
        return ("the branch has no commits yet — there is no parent to build "
                "on, and inventing the first commit of a repository is not an "
                "ingress hook's call")

    rel = os.path.relpath(path, repo)
    if rel.split(os.sep)[0] == ".git":
        return "it is inside .git — repository internals are not documents"
    if ".claude%sworktrees%s" % (os.sep, os.sep) in rel + os.sep:
        return ("it is inside a live agent worktree — that is a sealed "
                "transaction belonging to another agent, and this hook does "
                "not write into one")
    if os.sep not in rel:
        return ("it sits at the repository ROOT. A root entry is the "
                "repository's front door and a standing CEO decision (richos "
                "is nine entries by his deliberate design), so it is never "
                "added automatically — move it into a subdirectory such as "
                "docs/ and hand it over again")
    return ""


def read_text(path):
    """(text, refusal). Gate 4: unscannable is refused, never assumed clean."""
    check_budget()
    try:
        size = os.path.getsize(path)
    except OSError as exc:
        return (None, "it could not be measured (%s)" % exc.__class__.__name__)
    if size > MAX_FILE_BYTES:
        return (None, "it is %d bytes, over the %d-byte ceiling the content "
                      "gates can scan — and an unscanned commit is not on the "
                      "table" % (size, MAX_FILE_BYTES))
    try:
        with open(path, "rb") as fh:
            raw = fh.read()
    except OSError as exc:
        return (None, "it could not be read (%s)" % exc.__class__.__name__)
    try:
        return (raw.decode("utf-8"), "")
    except UnicodeDecodeError:
        return (None, "it is not valid UTF-8 text, so neither the credential "
                      "scanner nor the publication boundary can read it — and "
                      "a gate that cannot look never reports nothing found")


def run_gate(script, repo, path, text, seat):
    """Invoke an existing guard as a subprocess. (verdict, detail).

    verdict is "pass", "refuse" or "undecided". The guard is handed a
    synthesized PreToolUse Write payload — the exact shape it already reads.

    RICHOS_ENTITY_ROOT IS THE SEAT, NOT THE FILE'S REPOSITORY, AND THAT IS THE
    WHOLE POINT. The first draft pinned it to the repository enclosing the
    file, which reads as obviously right and is wrong in the field: measured
    against this machine, NEITHER `richos` NOR `richos-hq` carries
    orchestration.config at its root — the engine sits one level down, inside
    richos — so an adoption test against the file's own repository would have
    refused every hand-over in the two repositories the CEO's specifications
    actually live in, while reporting a safety reason for it.

    Handing the guards the SEAT is not a workaround; it is the behavior they
    are built around and their own headers describe. scan-secrets.sh scans a
    file outside its seat DELIBERATELY — "a secret written into someone else's
    repository is still a leaked secret" — announces the jurisdiction
    difference, and re-resolves its thresholds from the file's own governing
    root when there is one. guard-publication-writes.sh gives the seat NO VETO
    at all: it loads the declaration out of the FILE's repository and stands
    down only when that repository declares no publication boundary.

    The seat is adopted by construction — the wrapper has already resolved it
    and would not have reached this code otherwise — so the credential scan
    cannot take its not-adopted stand-down branch, which is the one way a gate
    could have returned 0 without looking at anything.
    """
    if not os.path.isfile(script):
        return ("undecided", "gate script missing at %s" % script)
    payload = json.dumps(
        {
            "session_id": "ingress",
            "cwd": repo,
            "hook_event_name": "PreToolUse",
            "tool_name": "Write",
            "tool_input": {"file_path": path, "content": text},
        }
    )
    env = dict(os.environ)
    env["RICHOS_ENTITY_ROOT"] = seat
    env.pop("CLAUDE_PROJECT_DIR", None)
    try:
        r = run_command(
            ["bash", script],
            input=payload.encode("utf-8"),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=repo,
            env=env,
            timeout=GATE_TIMEOUT_S,
        )
    except subprocess.TimeoutExpired as exc:
        return ("undecided", "gate timed out after %gs" % exc.timeout)
    except OSError as exc:
        return ("undecided", "gate could not run (%s)" % exc.__class__.__name__)
    if r.returncode == 0:
        return ("pass", "")
    if r.returncode == 2:
        return ("refuse", detectors_only(r.stderr.decode("utf-8", "replace")))
    return ("undecided", "gate exited %d" % r.returncode)


def detectors_only(stderr_text):
    """The REASON a gate refused, with every trace of file content removed.

    A guard's refusal is written for a human at a terminal, so it quotes the
    evidence it found. That is right there and wrong here: this text is about
    to be injected into a transcript, and requirement five is that the file's
    content never leaves this process. So only recognized detector labels and
    the guard's own headline travel out.
    """
    labels = []
    for line in stderr_text.splitlines():
        line = line.strip()
        for token in (
            "DECLARED PRIVATE BY IDENTITY",
            "recorded-speech",
            "verbatim-quote",
            "declared-private-file",
            "Publication boundary BLOCKED",
            "SECRET",
            "credential",
        ):
            if token.lower() in line.lower() and token not in labels:
                labels.append(token)
    return ", ".join(labels) if labels else "the gate refused (detail withheld: it quotes file content)"


# ---------------------------------------------------------------------------
# THE COMMIT
# ---------------------------------------------------------------------------
INDEX_STALE = "INDEX-STALE: "


def commit(path, repo, session, stamp, data=None, extras=()):
    """Commit one file, unmodified, via plumbing. (ok, sha_or_reason).

    (False, INDEX_STALE + ...) means the commit WAS made but the real index
    could not be brought in step with it.

    `extras` is [(path, bytes)]: the input's companion files, committed in the
    SAME commit from the bytes the gates checked."""
    rel = os.path.relpath(path, repo)
    # BELT AND BRACES, and it is here because the first field replay produced
    # exactly this state and reported it as a bare `update-index rc=128`. A
    # relative path that climbs out of the repository means the two spellings
    # disagree, which is a defect in this file rather than a fact about his
    # document — so it says that, instead of handing on a git return code
    # nobody can act on.
    if rel.startswith("..") or os.path.isabs(rel):
        return (False, "the file's path and its repository's path are in "
                       "different spellings (%s against %s), so the ingress "
                       "refused rather than committing something it could not "
                       "name" % (path, repo))
    mode = "100755" if os.access(path, os.X_OK) else "100644"
    entries = []   # (mode, blob, rel) for every extra; the main file is appended below

    for xpath, xdata in extras:
        xrel = os.path.relpath(xpath, repo)
        if xrel.startswith("..") or os.path.isabs(xrel):
            return (False, "a companion file's path and its repository's path are in "
                           "different spellings, so nothing was committed")
        rc, xblob, undecided = git(["hash-object", "-w", "--stdin"], repo, stdin_bytes=xdata)
        if undecided or rc != 0 or not xblob:
            return (False, "hash-object failed for a companion file (%s)" % (undecided or "rc=%d" % rc))
        entries.append(("100755" if os.access(xpath, os.X_OK) else "100644", xblob, xrel))

    if data is not None:
        # P3-22: commit the BYTES THE GATES CHECKED, never a re-read of the live
        # file, which can have been replaced since the scan.
        rc, blob, undecided = git(["hash-object", "-w", "--stdin"], repo, stdin_bytes=data)
    else:
        rc, blob, undecided = git(["hash-object", "-w", "--", path], repo)
    if undecided or rc != 0 or not blob:
        return (False, "hash-object failed (%s)" % (undecided or "rc=%d" % rc))
    entries.append((mode, blob, rel))

    rc, old, undecided = git(["rev-parse", "--verify", "HEAD"], repo)
    if undecided or rc != 0 or not old:
        return (False, "HEAD moved out from under the check (%s)" % (undecided or "rc=%d" % rc))

    rc, branch, undecided = git(["symbolic-ref", "--quiet", "HEAD"], repo)
    if undecided or rc != 0 or not branch:
        return (False, "the branch reference could not be read (%s)" % (undecided or "rc=%d" % rc))

    tmp_index = os.path.join(repo, ".git-ingress-index-%d" % os.getpid())
    rc, gitdir, undecided = git(["rev-parse", "--absolute-git-dir"], repo)
    if not undecided and rc == 0 and gitdir:
        tmp_index = os.path.join(gitdir, "ingress-index-%d" % os.getpid())

    env_index = dict(os.environ)
    env_index["GIT_INDEX_FILE"] = tmp_index

    def gi(args):
        try:
            r = run_command(
                ["git", "--no-optional-locks"] + args,
                cwd=repo,
                env=env_index,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=GIT_TIMEOUT_S,
            )
        except Exception as exc:
            return (1, "", exc.__class__.__name__)
        return (r.returncode, r.stdout.decode("utf-8", "replace").strip(), "")

    try:
        rc, _o, err = gi(["read-tree", old])
        if rc != 0:
            return (False, "read-tree failed (%s)" % (err or "rc=%d" % rc))
        for emode, eblob, erel in entries:
            rc, _o, err = gi(
                ["update-index", "--add", "--cacheinfo", "%s,%s,%s" % (emode, eblob, erel)]
            )
            if rc != 0:
                return (False, "update-index failed (%s)" % (err or "rc=%d" % rc))
        rc, tree, err = gi(["write-tree"])
        if rc != 0 or not tree:
            return (False, "write-tree failed (%s)" % (err or "rc=%d" % rc))
    finally:
        try:
            os.unlink(tmp_index)
        except OSError:
            pass

    message = (
        "CEO input captured: %s\n"
        "\n"
        "Handed over in a message and committed unmodified by the ingress\n"
        "hook, so the record holds the document the work is being driven from\n"
        "rather than one hard drive holding it.\n"
        "\n"
        "Handed over: %s\n"
        "Session:     %s\n"
        "Path:        %s\n"
        "Hook:        engine/scripts/hooks/commit-ceo-inputs.sh\n"
    ) % (rel, stamp, session or "unknown", path)
    if extras:
        message += "Companions:  %d file(s) under %s\n" % (
            len(extras), os.path.dirname(os.path.relpath(extras[0][0], repo)))

    try:
        r = run_command(
            ["git", "--no-optional-locks", "commit-tree", tree, "-p", old, "-F", "-"],
            cwd=repo,
            input=message.encode("utf-8"),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=GIT_TIMEOUT_S,
        )
    except Exception as exc:
        return (False, "commit-tree failed (%s)" % exc.__class__.__name__)
    if r.returncode != 0:
        return (False, "commit-tree failed (rc=%d)" % r.returncode)
    new = r.stdout.decode("utf-8", "replace").strip()
    if not new:
        return (False, "commit-tree produced no commit")

    # COMPARE-AND-SWAP. If anything else committed between the read of HEAD and
    # this line, the update is refused rather than clobbering that work — and
    # the refusal is reported, never retried blind.
    rc, _o, undecided = git(
        ["update-ref", "-m", "ingress: CEO input captured", branch, new, old], repo
    )
    if undecided or rc != 0:
        rc2, now_at, _u = git(["rev-parse", "--verify", "--quiet", branch], repo)
        if rc2 == 0 and now_at == old:
            # The branch did NOT move: a reference-transaction hook refused the
            # update (the operator fence, when it is on). Saying "the branch
            # moved" here would be false.
            return (False, "a Git hook refused moving %s (the operator fence, when "
                           "it is on); the branch did not move (%s)"
                           % (branch, undecided or "rc=%d" % rc))
        return (False, "the branch moved while this commit was being built, so "
                       "it was refused rather than overwriting that work "
                       "(%s)" % (undecided or "rc=%d" % rc))

    # The REAL index, that one path only, so `git status` does not now report
    # his file as a staged deletion. Every other staged change is untouched.
    # Its result is NOT discarded: HEAD now holds the input and the real index
    # does not, so the next ordinary commit would record the file's deletion.
    # The capture stands (the commit is made and is not undone), but it is not
    # reported as a clean capture; the failure is named with the repair.
    for emode, eblob, erel in entries:
        rc, _o, undecided = git(
            ["update-index", "--add", "--cacheinfo", "%s,%s,%s" % (emode, eblob, erel)], repo
        )
        if undecided or rc != 0:
            break
    if undecided or rc != 0:
        return (False, INDEX_STALE + (
            "the file IS captured in commit %s, but updating the real index "
            "failed (%s), so `git status` will show it as a staged deletion "
            "and the next ordinary commit would delete it. Repair: git "
            "update-index --add --cacheinfo %s,%s,%s"
            % (new[:12], undecided or "rc=%d" % rc, emode, eblob, erel)))

    return (True, new)


# ---------------------------------------------------------------------------
# THE LAND LEASE — THIS HOOK TAKES IT; IT IS NEVER EXEMPT
# ---------------------------------------------------------------------------
# With the operator fence on, a move of `main` needs the land lease
# (engine/scripts/land-lease.sh). The compare-and-swap above protects against
# `main` moving under this commit, NOT against another lead's land in progress
# in the same checkout: without the lease this commit would move `main` in
# the middle of that merge and add its path to the index that merge commits.
# An exemption would reopen exactly that interleaving (Frank's re-check of the
# operator fences, 2026-09-25, §4). So:
#   * the lease is taken before the commit, waiting at most LEASE_WAIT_S (his
#     prompt waits behind this hook, and only when he hands over a file);
#   * it is released only if THIS hook took it (ACQUIRED), never when the lead
#     already held it (RENEWED), so a lead's land lease is never released in
#     the middle of its land;
#   * when another holder has it, his file is not dropped: it is reported as
#     pending, naming the holder, and every later prompt retries it (without
#     waiting, so a long land does not slow every message down).
# With the switch off, or no launcher, none of this runs.
LEASE_WAIT_S = 10
LAUNCHER_MARKER = "richos-operator-fence-launcher"


def fence_on(repo):
    """True when this repository's operator-fence launcher says the switch is on."""
    rc, common, undecided = git(["rev-parse", "--path-format=absolute", "--git-common-dir"], repo)
    if undecided or rc != 0 or not common:
        return False
    try:
        with open(os.path.join(common, "hooks", "reference-transaction"),
                  encoding="utf-8", errors="replace") as fh:
            text = fh.read(65536)
    except OSError:
        return False
    return LAUNCHER_MARKER in text and \
        re.search(r'(?m)^OPERATOR_FENCES_STATE="on"\s*$', text) is not None


def take_lease(repo, engine_root, wait):
    """('acquired' | 'renewed' | 'off' | 'held' | 'failed', detail)."""
    script = os.path.join(engine_root, "scripts", "land-lease.sh")
    try:
        r = run_command(["bash", script, "acquire", "--repo", repo, "--wait", str(wait)],
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=wait + 5)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return ("failed", "land-lease.sh could not run (%s)" % exc.__class__.__name__)
    out = r.stdout.decode("utf-8", "replace")
    # Anything acquire says after its first line (an unfinished merge, a tree a
    # refused writer rewrote) is relayed, never swallowed: a renewal is how a
    # holder is told, so the lead must see what this renewal said.
    notes = [ln.strip() for ln in out.strip().splitlines()[1:] if ln.strip()]
    if r.returncode == 0 and "ACQUIRED" in out:
        return ("acquired", notes)
    if r.returncode == 0 and "RENEWED" in out:
        return ("renewed", notes)
    if r.returncode == 0 and "fences are off" in out:
        return ("off", "")
    if r.returncode == 75:
        m = re.search(r"HELD\. (.+?) holds the land lease", out)
        return ("held", m.group(1) if m else "another land")
    first = (out.strip() or r.stderr.decode("utf-8", "replace").strip() or "rc=%d" % r.returncode)
    return ("failed", first.splitlines()[0][:300])


def release_lease(repo, engine_root):
    try:
        run_command(["bash", os.path.join(engine_root, "scripts", "land-lease.sh"), "release", "--repo", repo],
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=45)
    except (OSError, subprocess.TimeoutExpired, IngressDeadline):
        pass    # a lease left held at rest is released by the turn-end hook (release-land-leases.sh)


def pending_paths(state_dir):
    """Paths whose latest ledger word is 'pending' (another land held the lease)."""
    latest = {}
    try:
        with open(os.path.join(state_dir, "ceo-inputs.jsonl"), encoding="utf-8") as fh:
            for line in fh:
                try:
                    rec = json.loads(line)
                except ValueError:
                    continue
                for p in rec.get("committed") or []:
                    latest.pop(p, None)
                for r in rec.get("refused") or []:
                    if isinstance(r, dict) and r.get("path"):
                        if r.get("pending"):
                            latest[r["path"]] = True
                        else:
                            latest.pop(r["path"], None)
                for p in rec.get("reported") or []:
                    latest.pop(p, None)
    except OSError:
        return []
    return list(latest)


# ---------------------------------------------------------------------------
# LEDGER
# ---------------------------------------------------------------------------
def ledger_append(state_dir, record):
    """One line per outcome. The DURABLE half of this mechanism.

    Absence of a finding and absence of a check are two different facts, and
    this file is what tells them apart: a run that found nothing still writes a
    line. It also carries every REFUSED and FAILED outcome, which is what the
    Stop-side notice re-reads at the end of every turn so a refusal cannot be
    read once and forgotten.

    Never contains file content. Path, state, outcome, reason.
    """
    if not state_dir:
        return "no state directory"
    try:
        os.makedirs(state_dir, exist_ok=True)
        with open(os.path.join(state_dir, "ceo-inputs.jsonl"), "a", encoding="utf-8") as fh:
            fh.write(json.dumps(record) + "\n")
    except OSError as exc:
        return exc.__class__.__name__
    return ""


def main():
    global _deadline
    _deadline = time.monotonic() + DEADLINE_S
    raw = sys.stdin.read()
    try:
        payload = json.loads(raw)
        if not isinstance(payload, dict):
            raise ValueError("payload is not an object")
    except Exception as exc:
        sys.stderr.write("unreadable payload: %s\n" % exc)
        return 1

    text = payload.get("prompt")
    if not isinstance(text, str):
        # No prompt field means the payload shape moved, or this is not the
        # event this analyzer reads. Either way it is a CANNOT-RUN, never a
        # clean: a process that read nothing must not report nothing found.
        sys.stderr.write("payload carries no string `prompt` field\n")
        return 1

    cwd = payload.get("cwd") if isinstance(payload.get("cwd"), str) else ""
    session = payload.get("session_id") if isinstance(payload.get("session_id"), str) else ""
    engine_root = os.environ.get("RICHOS_ENGINE_ROOT_FOR_GATES", "")
    state_dir = os.environ.get("RICHOS_INGRESS_STATE_DIR", "")
    seat_root = os.environ.get("RICHOS_INGRESS_SEAT_ROOT", "")
    stamp = time.strftime("%Y-%m-%dT%H:%M:%S%z")

    cands = candidates(text)
    # A file an earlier message handed over while another land held the lease
    # is retried now, whatever this message says (see THE LAND LEASE above).
    named = set(c for c, _s in cands)
    cands += [(p, "pending") for p in pending_paths(state_dir) if p not in named] if state_dir else []
    truncated = len(cands) > MAX_CANDIDATES
    cands = cands[:MAX_CANDIDATES]

    committed, refused, undecided, reported, pending, lease_notes = [], [], [], [], [], []
    held = {}
    states = {}
    seen = set()
    unheld = []

    for rawc, source in cands:
        p = resolve(rawc, cwd)
        if p is None or p in seen:
            continue
        if not is_handover_file(p):
            continue
        seen.add(p)
        try:
            check_budget()
            state, repo, detail = classify(p)
        except IngressDeadline as exc:
            undecided.append({"path": p, "why": str(exc)})
            continue
        states[state] = states.get(state, 0) + 1
        if state == "indeterminate":
            undecided.append({"path": p, "why": detail or "unknown"})
        elif state == "outside-repo":
            # Not a gate refusal — a different case. Unheld, with no repository
            # that obviously ought to hold it. Choosing one is the single
            # judgment a hook genuinely cannot make, so it is reported.
            reported.append(
                {
                    "path": p,
                    "why": "it is outside every git repository, so there is no "
                           "repository that obviously ought to hold it — say "
                           "where it belongs and it can be moved and committed",
                }
            )
        elif state == "untracked":
            unheld.append((p, repo, source))

    # Gate 7 — a paste, not a handover.
    if len(unheld) > MAX_COMMITS:
        for p, repo, _s in unheld:
            refused.append(
                {
                    "path": p,
                    "repo": repo,
                    "why": "this message names %d untracked files, over the %d "
                           "an ingress commit will take — that is a paste "
                           "rather than a handover, so nothing was committed"
                           % (len(unheld), MAX_COMMITS),
                }
            )
        unheld = []

    for p, repo, source in unheld:
        lease = "none"
        try:
            check_budget()
            why = gate_git_ready(p, repo)
            if why:
                refused.append({"path": p, "repo": repo, "why": why})
                continue

            # Gate 5 — the gates below are run FROM the seat, and the seat is what
            # makes the credential scanner scan instead of standing down. Without
            # one there is no adopted repository to run them from, so nothing is
            # known about the content and nothing is committed.
            if not seat_root:
                refused.append(
                    {
                        "path": p,
                        "repo": repo,
                        "why": "the ingress could not name an adopted repository "
                               "to run the credential and publication gates from, "
                               "so nothing is known about this file's content — "
                               "and an unscanned automatic commit is not on the "
                               "table",
                    }
                )
                continue

            body, why = read_text(p)
            if body is None:
                refused.append({"path": p, "repo": repo, "why": why})
                continue

            # Companion files go through the same two gates; one refusal and
            # nothing at all is committed.
            extras = []
            for cp in companion_files(p, repo):
                check_budget()
                cbody, cwhy = read_text(cp)
                if cbody is None or os.path.islink(cp):
                    why = cwhy or "a companion is a symbolic link"
                    break
                extras.append((cp, cbody))
            else:
                why = ""
            if why:
                refused.append({"path": p, "repo": repo,
                                "why": "a companion file could not be committed with it (%s)" % why})
                continue

            blocked = False
            for name, script in (
                ("credential scanner", os.path.join(engine_root, "scripts/hooks/scan-secrets.sh")),
                ("publication boundary", os.path.join(engine_root, "scripts/hooks/guard-publication-writes.sh")),
            ):
                check_budget()
                verdict, detail = run_gate(script, repo, p, body, seat_root)
                for cp, cbody in extras:
                    if verdict != "pass":
                        break
                    check_budget()
                    verdict, detail = run_gate(script, repo, cp, cbody, seat_root)
                if verdict == "refuse":
                    refused.append(
                        {
                            "path": p,
                            "repo": repo,
                            "why": "the %s refused it (%s). It is not committed; "
                                   "put it somewhere gitignored, or in the private "
                                   "record, and say so." % (name, detail),
                        }
                    )
                    blocked = True
                    break
                if verdict == "undecided":
                    refused.append(
                        {
                            "path": p,
                            "repo": repo,
                            "why": "the %s could not run (%s), so nothing is known "
                                   "about this file's content — and a gate that "
                                   "could not look never reports nothing found"
                                   % (name, detail),
                        }
                    )
                    blocked = True
                    break
            if blocked:
                continue

            check_budget()
            if fence_on(repo) and git(["symbolic-ref", "--quiet", "HEAD"], repo)[1] == "refs/heads/main":
                if repo in held:
                    lease, detail = "held", held[repo]
                else:
                    wait = 0 if source == "pending" else \
                        max(0, min(LEASE_WAIT_S, int(_deadline - time.monotonic())))
                    lease, detail = take_lease(repo, engine_root, wait)
                if lease == "held":
                    held[repo] = detail
                    pending.append({"path": p, "repo": repo, "pending": True,
                                    "why": "not committed yet: %s is landing in this repository, and a "
                                           "commit onto main needs the land lease. It stays pending and is "
                                           "retried on your next message" % detail})
                    continue
                if lease in ("acquired", "renewed"):
                    lease_notes.extend(n for n in detail if n not in lease_notes)
                if lease == "failed":
                    refused.append({"path": p, "repo": repo, "failed": True,
                                    "why": "the land lease could not be taken, so nothing was committed (%s)"
                                           % detail})
                    continue
            check_budget()
            ok, result = commit(p, repo, session, stamp, data=body.encode("utf-8"),
                             extras=[(cp, cb.encode("utf-8")) for cp, cb in extras])
            if ok:
                committed.append(
                    {"path": p, "repo": repo, "sha": result,
                     "relpath": os.path.relpath(p, repo), "source": source}
                )
            else:
                if result.startswith(INDEX_STALE):
                    why = "THE CAPTURE LEFT THE INDEX STALE: %s" % result[len(INDEX_STALE):]
                else:
                    why = "THE COMMIT ITSELF FAILED: %s" % result
                refused.append({"path": p, "repo": repo, "why": why, "failed": True})
        except IngressDeadline as exc:
            undecided.append({"path": p, "repo": repo, "why": str(exc)})
        finally:
            if lease == "acquired":
                release_lease(repo, engine_root)

    result = {
        "stamp": stamp,
        "session": session,
        "candidates": len(cands),
        "truncated": truncated,
        "states": states,
        "committed": committed,
        "refused": refused,
        "pending": pending,
        "lease_notes": lease_notes[:10],
        "reported": reported,
        "undecided": undecided,
    }

    ledger_err = ledger_append(
        state_dir,
        {
            "ts": stamp,
            "session": session,
            "hook": "commit-ceo-inputs.sh",
            "candidates": len(cands),
            "states": states,
            "committed": [c["path"] for c in committed],
            # A pending file is recorded as refused WITH "pending": true, so the
            # turn-end notice keeps naming it until it is held, and the next
            # prompt's run finds it (pending_paths) and retries it.
            "refused": [{"path": r["path"], "why": r["why"]} for r in refused]
                       + [{"path": r["path"], "why": r["why"], "pending": True} for r in pending],
            "reported": [r["path"] for r in reported],
            "undecided": [u["path"] for u in undecided],
        },
    )
    if ledger_err:
        result["ledger_error"] = ledger_err

    print(json.dumps(result))
    if refused or pending or lease_notes or undecided or reported or ledger_err:
        return 4
    if committed:
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
