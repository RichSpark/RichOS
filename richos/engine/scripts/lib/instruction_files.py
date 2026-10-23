"""A connected repository's own instruction file (`CLAUDE.md`, `AGENTS.md`), chosen by
Claude Code's own rule, for the workers and reviewers that work in that repository.

richos-hq docs/plans/2026-10-21-folder-instruction-files.md, section 5.3, option D. The file is
never moved, changed or copied into the app's storage, and never given to Rich or Stu. `prepare`
(mega-lander/app.py) puts it in a worker's or reviewer's brief below the duty, and the second
review (second_review.py) reads it by the same rule, so the worker and both reviewers see one text.

THE RULE IS CLAUDE CODE'S, read off https://code.claude.com/docs/en/memory on 2026-10-21 (Claude
Code 2.1.306), for a session started at the repository's root:
  * `CLAUDE.md` and `.claude/CLAUDE.md` load, and `CLAUDE.local.md` is appended after them;
  * `AGENTS.md` and `.claude/AGENTS.md` load only when none of those three exist ("Count, so
    Claude reads them instead of AGENTS.md: a CLAUDE.md, .claude/CLAUDE.md, or CLAUDE.local.md").
A file is given in full, never cut: half a rule is a wrong rule. `@path` imports and a
subfolder's own file are the worker's to read, as its duty says; they are not expanded here.

NEVER THROUGH STU. The section is kept beside the work receipt and joins the worker's prompt only
when its dispatch is admitted; the payload Stu reads carries PLACEHOLDER in its place.

WHERE EACH FILE IS READ. The committed files come from the commit the agent starts at (a
worker's workspace is a `git worktree` at that commit, so it is the file the agent can open).
`CLAUDE.local.md` is normally gitignored and so is not in a workspace at all; it is read from
the connected main checkout, where the user keeps it, and from the commit only when it is
committed and absent there.
"""
import os
import subprocess

HEADING = "## The repository's own instructions"
# Where the section goes in a brief Stu dispatches. Stu reads and resubmits the prepared payload,
# and the file is never Rich's or Stu's to read, so the payload carries this line instead; the
# app puts the section in its place when the dispatch is admitted (app.py `dispatch_intent`).
PLACEHOLDER = "(The repository's own instruction file is put here when the teammate starts.)"
CLAUDE_FILES = ("CLAUDE.md", ".claude/CLAUDE.md")
LOCAL_FILE = "CLAUDE.local.md"
AGENTS_FILES = ("AGENTS.md", ".claude/AGENTS.md")
_LINK_HOPS = 8


def choose(committed, checkout):
    """[(path relative to the root, text, where)] in Claude Code's order, or [] when the
    repository has none. `committed(rel)` and `checkout(rel)` return the file's text or None;
    `where` is "commit" or "checkout"."""
    picked = []
    for rel in CLAUDE_FILES:
        text = committed(rel)
        if text is not None:
            picked.append((rel, text, "commit"))
    local = checkout(LOCAL_FILE)
    if local is not None:
        picked.append((LOCAL_FILE, local, "checkout"))
    else:
        local = committed(LOCAL_FILE)
        if local is not None:
            picked.append((LOCAL_FILE, local, "commit"))
    if picked:
        return picked
    return [(rel, text, "commit") for rel in AGENTS_FILES for text in [committed(rel)] if text is not None]


def from_disk(root):
    """A reader of files under `root` as they are on disk (a symbolic link is followed, as
    Claude Code follows it). A file that is not there is None; any other failure to read it
    raises, because a silently missing instruction file is the defect this module removes."""
    def read(rel):
        try:
            with open(os.path.join(root, rel), "rb") as f:
                return f.read().decode("utf-8", "replace")
        except (FileNotFoundError, NotADirectoryError, IsADirectoryError):
            return None
    return read


def _git(repo, *args):
    return subprocess.run(["git", "-c", "core.hooksPath=/dev/null", "-C", str(repo), *args],
                          capture_output=True, timeout=120)


def at_commit(repo, commit):
    """A reader of files as they are committed at `commit` in `repo`. A link inside the
    repository is followed within the commit; a link out of it is read where it points on
    this Mac, which is what an agent opening the file would read."""
    def read(rel, hops=0):
        listing = _git(repo, "ls-tree", "-z", commit, "--", rel)
        if listing.returncode != 0:
            raise ValueError("The repository's own instruction file %s could not be read at %s in %s: %s"
                             % (rel, commit[:12], repo, listing.stderr.decode("utf-8", "replace").strip()[-500:]))
        entry = listing.stdout.rstrip(b"\0")
        if not entry:
            return None
        meta, _, _name = entry.partition(b"\t")
        mode, kind, obj = meta.split()
        if kind != b"blob":
            return None
        blob = _git(repo, "cat-file", "blob", obj.decode())
        if blob.returncode != 0:
            raise ValueError("The repository's own instruction file %s could not be read at %s in %s"
                             % (rel, commit[:12], repo))
        if mode == b"120000":
            target = os.path.normpath(os.path.join(os.path.dirname(rel), blob.stdout.decode("utf-8", "replace")))
            if hops < _LINK_HOPS and not os.path.isabs(target) and target != ".." and not target.startswith("../"):
                return read(target, hops + 1)
            return from_disk(repo)(rel)
        return blob.stdout.decode("utf-8", "replace")
    return read


def for_repository(repo, commit):
    """The instruction files an agent starting at `commit` in the connected main checkout
    `repo` is given."""
    return choose(at_commit(repo, commit), from_disk(repo))


def quoted(text):
    """The text with every line quoted by `> `. A brief's control lines (`cross-repo-worktree:`,
    `main-checkout-run:`, ...) are read by the spawn guards wherever a line starts with one, and
    a repository's file is not the coordinator's to sign for; quoted, no line of it can act as
    one, and every word of it is still there."""
    return "\n".join(("> " + line) if line else ">" for line in text.rstrip("\n").split("\n"))


def section(entries):
    """The brief section for [(repo, commit, files)], files as `choose` returns them: one fixed
    heading, then each file in full. Empty when no repository has one."""
    blocks = []
    for repo, commit, files in entries:
        for rel, text, where in files:
            origin = ("as committed at %s" % commit[:12] if where == "commit"
                      else "from the connected folder itself (it is not committed, so it is not in your workspace)")
            blocks.append("`%s`, %s. Each line below is quoted with `> `; the file itself has no such prefix.\n\n%s"
                          % (os.path.join(str(repo), rel), origin, quoted(text)))
    return (HEADING + "\n\n" + "\n\n".join(blocks)) if blocks else ""

