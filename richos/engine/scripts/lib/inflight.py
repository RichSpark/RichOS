#!/usr/bin/env python3
"""inflight.py — THE PREDICATE. Who is in flight, who is behind, who was told,
and who has proved they heard it.

===========================================================================
WHAT THIS ANSWERS, AND WHY IT IS ONE FILE
===========================================================================
Landing moves `main`. Every teammate still working was cut from an older base
and is now silently behind. `rich-lander/SKILL.md` §8b is the written step that
says to sweep them; this file is the machine underneath it, and it is a single
file for the reason `row-currency.py` is: a predicate kept in two copies is the
defect class this engine keeps finding in itself.

Four consumers, one predicate:

    scripts/hooks/guard-inflight-notify.sh   BLOCKS a push that leaves a live
                                             teammate behind and un-notified.
    scripts/hooks/notice-inflight-acks.sh    Stop-hook notice: notified, but
                                             no ack, past the timeout.
    scripts/inflight-notify.sh               the by-hand runner (status /
                                             check / waive / acks).
    scripts/inflight-ack.sh                  the teammate's ack writer, which
                                             uses only the ack-path rule here.

===========================================================================
THE THREE FACTS, AND WHERE EACH ONE COMES FROM
===========================================================================
1. WHO IS IN FLIGHT — `git worktree list --porcelain`, in the repository being
   landed. Not a roster, not the SendMessage agent list: a worktree is a thing
   on disk with a branch and a base, and it is the only in-flight record that
   survives the lead's context.

2. WHO WAS TOLD — an append-only ledger written by a PostToolUse[SendMessage]
   hook, in the LEAD's own execution, at the moment the tool call resolves.
   This is the same trick worker-updated-handoff.sh uses and it is the whole
   reason the send half can be guaranteed at all: the witness observes the
   SEND, not the delivery, so it is true even when the mailbox drops the
   message. The lander cannot satisfy it by remembering to write a file — the
   only way to produce the record is to actually call SendMessage.

   IT IS NOT FRAUD-PROOF, and that is stated rather than implied: anyone with
   Bash can append a line to a JSONL file. The failure being engineered out is
   FORGETTING, which is what happened twice on 2026-08-30. Fabrication is a
   different act and this guard does not pretend to stop it.

3. WHO HEARD IT — an append-only row in ~/.claude/state/inflight-acks.jsonl,
   written by the teammate. A reply the lead may never receive proves nothing
   to the lead; a record the lead can read proves it, and it proves it after
   the session that sent the message is gone.

   IT LIVED IN THE TEAMMATE'S WORKTREE UNTIL 2026-09-05, AND THAT IS WHERE IT
   DIED. Both governed repositories gitignore `.claude/*`; the harness
   auto-cleans an isolation worktree that is UNCHANGED at completion; and a
   gitignored write does not make a tree changed. So an agent whose only writes
   were acks had its worktree, and every ack in it, deleted the moment it
   finished — and an ack that was written, confirmed and then deleted reads at
   the 30-minute timeout exactly like an ack that was never written. The
   worktree file is still written and still read, because every ack already on
   disk is one; it is now the readable mirror and the ledger is the record. See
   ack_status() for exactly which parts a machine checks and which part it
   cannot, and orphan_ledger_acks() for the case the whole change is about.

AND, UNDERNEATH ALL THREE, WHO A TEAMMATE IS — scripts/lib/teammate-identity.py,
imported rather than re-derived. Facts 2 and 3 are joined by NAME, and until
2026-08-31 this file resolved that name from `worker-events.jsonl`'s
`agent_type`, which is the ROLE (`zach`), while the witness recorded
SendMessage's `to`, which is the mandatory unique name (`zach-opus-s1`). Two
notices that were genuinely sent, witnessed and logged were reported as
OWED-NO-NOTICE and waived through, because one half of the engine wrote `zach`
and the other wrote `zach-opus-s1`. There is now ONE definition of a teammate's
name and both halves import it.

===========================================================================
LIVENESS — TWO KINDS OF WORKTREE, TWO DIFFERENT ANSWERS
===========================================================================
NATIVE (`.claude/worktrees/agent-<id>`): the harness locks it while the agent
runs, and the lock line carries a pid. That is the authoritative signal the
rest of the engine already uses (remove-agent-worktree.sh). locked + live pid
= LIVE; locked + dead pid = stale lock, not live; unlocked = not live.

HAND-ROLLED (any other registered worktree — the shape this whole operation
actually runs in: /Users/alex/ab/richos-wt/<name>): NO LOCK IS EVER TAKEN, so
the lock signal says nothing at all. Absence of a lock here is absence of
evidence, and the doctrine is explicit that an agent is never inferred dead
from filesystem quiet. So a hand-rolled worktree is PRESUMED LIVE — when it
names a teammate (a teammate-shaped basename, or a registered workspace's
cc/<name> branch). One that names nobody is not a teammate and is owed nothing.

That presumption is deliberately the over-blocking direction, and it is bounded
by two things that cost the lander seconds rather than minutes: a worktree
whose branch is already contained in the tip is discharged automatically (it
has landed — there is nothing to tell it), and anything else can be waived with
a recorded reason. An unbounded presumption with no way out would be the
"guard that wedges the session" this was explicitly told not to build.

===========================================================================
WHAT IS OWED — AND WHY RELEVANCE IS NOT FILTERED
===========================================================================
A live worktree is owed a notice when the tip contains commits its base does
not. NOT "when the moved files overlap its own files".

That is not laziness, it is the second of the two failures §8b was written
from. The library agent did not TOUCH the twelve new design variations; it
CONSUMED them, and shipped 7 of 19 because nobody told it they existed. A
path-overlap filter would have cleared that agent as unaffected. Overlap is
computed and REPORTED, because it separates "will hit a merge conflict" from
"may be building on a stale fact", and the second is the one that needs a human
sentence rather than a diff. It is never used to excuse a notice.
"""

import calendar
import json
import os
import re
import subprocess
import sys
import time

DEFAULT_ACK_TIMEOUT_MIN = 30


def _identity_module():
    """scripts/lib/teammate-identity.py — THE definition of a teammate's name.

    Loaded by path because the filename is hyphenated. If it is missing this
    predicate degrades to the role, which is exactly the 2026-08-31 defect, so
    the absence is RECORDED in the assessment rather than swallowed."""
    try:
        import importlib.util as ilu
        here = os.path.dirname(os.path.abspath(__file__))
        spec = ilu.spec_from_file_location(
            "teammate_identity", os.path.join(here, "teammate-identity.py"))
        mod = ilu.module_from_spec(spec)
        spec.loader.exec_module(mod)
        return mod
    except Exception:
        return None


IDENTITY = _identity_module()

# Tokens that carry no identity: the model alias in <role>-<model>-<id> names,
# and dates in hand-rolled worktree names. Stripped before name matching.
_NOISE_TOKENS = {"fable", "opus", "sonnet", "haiku", "wt", "worktree", "agent"}
_DATE_RE = re.compile(r"^\d{4}-?\d{2}-?\d{2}$")
_SHA_RE = re.compile(r"\b[0-9a-f]{7,40}\b")
_NATIVE_ID_RE = re.compile(r"^a(?P<name>[a-z0-9][a-z0-9-]*?)-[0-9a-f]{12,}$")

ACK_DIR_REL = os.path.join(".claude", "inflight-acks")


def norm(path):
    """A path in the one form two of these comparisons can agree on.

    macOS hands /var/folders/... to a caller and /private/var/folders/... back
    out of git, because /var is a symlink. abspath does not resolve that, so a
    waiver recorded against a worktree failed to match the same worktree ten
    lines later — caught by inflight-notify.test.sh case 5j, not by reading."""
    if not path:
        return ""
    return os.path.realpath(os.path.abspath(path)).rstrip("/")
ACK_IMPACTS = ("conflict", "stale-record", "grew-scope", "none")
ACK_MIN_DETAIL = 40


# --------------------------------------------------------------------------
# git
# --------------------------------------------------------------------------
def git(repo, *args, check=False):
    try:
        res = subprocess.run(
            ["git", "-C", repo] + list(args),
            capture_output=True, text=True, timeout=30,
        )
    except Exception as exc:
        if check:
            raise RuntimeError("git %s failed: %s" % (" ".join(args), exc))
        return ""
    if res.returncode != 0:
        if check:
            raise RuntimeError(
                "git %s exited %d: %s"
                % (" ".join(args), res.returncode, (res.stderr or "").strip()[:200])
            )
        return ""
    return res.stdout


def git_checked(repo, *args):
    """(ok, stdout). Unlike git(), a FAILURE IS NOT AN EMPTY ANSWER.

    git() returns "" for a command that ran and printed nothing AND for a
    command that could not run, which is how a failed worktree inventory became
    "no peers" and a failed merge-base became "not behind" (hunt P5-08). The
    callers that decide a verdict from the answer use this and carry the
    failure out as an error instead."""
    try:
        res = subprocess.run(["git", "-C", repo] + list(args),
                             capture_output=True, text=True, timeout=30)
    except Exception:
        return False, ""
    return res.returncode == 0, res.stdout


def ancestor_state(repo, a, b):
    """True / False / None. None = git could not answer (neither 0 nor 1), which
    is_ancestor() folds into False and so into "behind" or "not landed"."""
    try:
        res = subprocess.run(
            ["git", "-C", repo, "merge-base", "--is-ancestor", a, b],
            capture_output=True, text=True, timeout=30,
        )
    except Exception:
        return None
    if res.returncode == 0:
        return True
    if res.returncode == 1:
        return False
    return None


# AN ANSWER ABOUT TWO COMMITS NEVER CHANGES, SO IT IS ASKED ONCE (2026-10-22). The Stop notice
# asked git the same questions about every worktree at every turn end: whether its head
# contains or trails the tip, their merge-base, the commits and paths between. With 50
# worktrees that was 16.4 CPU-s a turn (Hank's after-measurement, richos-hq
# docs/research/2026-10-22-mac-system-cpu-after.md), and between two lands every answer was
# the one it gave last turn. Each is a fact about full commit ids, so it is kept in the
# repository's own git directory and reused while those ids are the ones asked about. Only a
# definite answer is kept: a question git could not answer is asked again next time, so a
# failure is never remembered as a fact. A worktree whose head moved, or a new tip, asks anew.
GIT_MEMO_NAME = "richos-inflight-answers.json"
_FULL_SHA = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")


class GitMemo:
    """What one assessment asked git, kept for the next. Only the answers this assessment used
    are written back, so the file holds one assessment's worth (about 7 MB for richos's 50
    worktrees on 2026-10-22, most of it moved-path listings) and never grows with the history
    of tips."""
    def __init__(self, root):
        common = git(root, "rev-parse", "--path-format=absolute", "--git-common-dir").strip()
        self.path = os.path.join(common, GIT_MEMO_NAME) if common else ""
        self.entries, self.used, self.dirty = {}, set(), False
        try:
            with open(self.path) as fh:
                loaded = json.load(fh)
            if isinstance(loaded, dict) and isinstance(loaded.get("entries"), dict):
                self.entries = loaded["entries"]
        except (OSError, ValueError):
            pass

    def ask(self, key, shas, compute, definite):
        """compute()'s answer, reused for `key` when every id in `shas` is a full commit id."""
        keep = bool(self.path) and all(_FULL_SHA.match(s or "") for s in shas)
        name = "\t".join(key)
        if keep and name in self.entries:
            self.used.add(name)
            return self.entries[name]
        answer = compute()
        if keep and definite(answer):
            self.entries[name] = answer
            self.used.add(name)
            self.dirty = True
        return answer

    def save(self):
        if not self.path or not (self.dirty or set(self.entries) != self.used):
            return
        tmp = "%s.%d.tmp" % (self.path, os.getpid())
        try:
            with open(tmp, "w") as fh:
                json.dump({"schema": 1, "entries": {k: self.entries[k] for k in sorted(self.used)}}, fh)
            os.replace(tmp, self.path)
        except OSError:
            try:
                os.unlink(tmp)
            except OSError:
                pass


def main_checkout(repo):
    """The TRUE main checkout for this repository, from any worktree inside it.

    Same resolution resolve-main-checkout.sh uses: --git-common-dir points at
    the main checkout's .git regardless of which worktree asked."""
    common = git(repo, "rev-parse", "--path-format=absolute", "--git-common-dir").strip()
    if not common:
        return os.path.abspath(repo)
    return os.path.dirname(common.rstrip("/"))


def is_ancestor(repo, a, b):
    try:
        res = subprocess.run(
            ["git", "-C", repo, "merge-base", "--is-ancestor", a, b],
            capture_output=True, text=True, timeout=30,
        )
    except Exception:
        return False
    return res.returncode == 0


def list_worktrees(repo, strict=False):
    """Parse `git worktree list --porcelain` into dicts.

    Fields per entry: path, head, branch, detached, locked (the raw lock line
    or None), prunable.

    strict=True raises RuntimeError when git could not list them, instead of
    returning the same empty list as a repository with no teammates."""
    if strict:
        ok, out = git_checked(repo, "worktree", "list", "--porcelain")
        if not ok:
            raise RuntimeError("git worktree list failed in %s" % repo)
    else:
        out = git(repo, "worktree", "list", "--porcelain")
    entries = []
    cur = None
    for line in out.splitlines():
        if line.startswith("worktree "):
            if cur is not None:
                entries.append(cur)
            cur = {"path": line[len("worktree "):], "head": "", "branch": "",
                   "detached": False, "locked": None, "prunable": False}
        elif cur is None:
            continue
        elif line.startswith("HEAD "):
            cur["head"] = line[len("HEAD "):].strip()
        elif line.startswith("branch "):
            cur["branch"] = line[len("branch "):].strip().replace("refs/heads/", "", 1)
        elif line.startswith("detached"):
            cur["detached"] = True
        elif line.startswith("locked"):
            cur["locked"] = line
        elif line.startswith("prunable"):
            cur["prunable"] = True
    if cur is not None:
        entries.append(cur)
    return entries


# --------------------------------------------------------------------------
# RETIRED WORKSPACES ARE NOT TEAMMATES
# --------------------------------------------------------------------------
# A worktree used to leave `git worktree list` when its teammate finished:
# remove-agent-worktree.sh unlocked the registration and pruned it. Since
# 6472bb60 ("Preserve Claude-owned native cleanup and prohibit bulk
# registration pruning") it does not. Retirement now RENAMES the directory to
# <parent>/.richos-retired/<base>.richos-retired-ws-<16 hex>-<stamp>Z and runs
# `git worktree repair` on the new path instead of pruning, deliberately, so
# that no automatic helper ever bulk-prunes a registration out from under a
# checkout Claude Code owns. The bytes and the registration both survive.
#
# The consequence for THIS module is that a finished teammate's quarantine
# keeps appearing in `git worktree list`, and everything here is driven by that
# list. A hand-rolled worktree is PRESUMED LIVE (correctly — no lock is ever
# taken on one, so quiet is not death), so a retired quarantine read as a
# worktree is read as a LIVE TEAMMATE: it inflates live_count, it is owed a
# notice it can never be sent, it can never ack, and the Stop hook chases the
# operator about it forever. That is the false chase this whole row exists to
# prevent, arriving through a new door.
#
# So a quarantine is skipped, exactly as every other scanner in this engine
# already skips it — reap-stale-worktrees.sh (`*.richos-terminal-*` and
# `*/.richos-retired/*`) and detect-nonnative-worktree.sh (`.*` and
# `*.richos-retired-*`). This module was the consumer that convention was never
# extended to.
#
# NOT a heuristic on a name: both forms are minted by the engine and carry
# unforgeable material — 16 hex of workspace id plus a UTC stamp for the
# retirement quarantine (workspace-retire.py QUARANTINE_RE), the session
# prefix plus the agent id for the terminal quarantine
# (worktree-transactions.quarantine_path). A teammate cannot accidentally be
# named one.
#
# The teammate's ACK is not lost by skipping: the durable ledger row outside
# every repository is the authority (see read_ack_ledger), and it is what
# reports the teammate under ACKED, WORKTREE GONE. Crediting an ack read out of
# a quarantine would resurrect exactly the destroyed-file evidence path the
# ledger replaced, and would silently defeat the positive probe that proves the
# ledger is doing the work.
QUARANTINE_DIRNAME = ".richos-retired"
RETIRED_QUARANTINE_RE = re.compile(
    r"\.richos-retired-ws-[0-9a-f]{16}-\d{8}T\d{6}(\.\d{6})?Z$")
TERMINAL_QUARANTINE_RE = re.compile(r"\.richos-terminal-[0-9a-f]{8}-[A-Za-z0-9_-]{6,64}$")


def retired_workspace(path):
    """True when `path` is a retired/terminal workspace quarantine rather than
    a teammate's workspace. Returns the reason, or "" — never a bare bool, so
    a caller can say WHY it skipped something."""
    p = (path or "").rstrip("/")
    base = os.path.basename(p)
    if RETIRED_QUARANTINE_RE.search(base):
        return "retirement quarantine (workspace-retire.py)"
    if TERMINAL_QUARANTINE_RE.search(base):
        return "terminal quarantine (worktree-transactions.py)"
    if QUARANTINE_DIRNAME in p.split(os.sep):
        return "inside %s/" % QUARANTINE_DIRNAME
    return ""


def pid_alive(pid):
    try:
        os.kill(int(pid), 0)
    except (OSError, ValueError):
        return False
    return True


# --------------------------------------------------------------------------
# identity
# --------------------------------------------------------------------------
def name_tokens(text, strip_digits=True):
    """Identity tokens of a teammate name or a worktree basename.

    strict (strip_digits=False):
        'zach-opus-ackguard1'       -> {'zach', 'ackguard1'}
    loose (strip_digits=True):
        'zach-opus-ackguard1'       -> {'zach', 'ackguard'}
        'zach-ackguard-2026-08-30'  -> {'zach', 'ackguard'}

    Dates and model aliases are dropped from both, because neither carries
    identity. The digits are the difference, and BOTH readings are needed:
    a hand-rolled worktree is named for a date while its teammate is named
    'ackguard1', so only the loose reading joins them — but this house's own
    naming rule turns 'mark-sonnet-f1' into 'mark-sonnet-f2' for the next
    teammate, and under the loose reading those two are the same person. That
    collision is not hypothetical; it is the convention. So the strict reading
    is tried first and the loose one only breaks a tie nothing else could."""
    text = (text or "").lower()
    text = re.sub(r"\d{4}-\d{2}-\d{2}", " ", text)
    parts = [p for p in re.split(r"[^a-z0-9]+", text) if p]
    toks = set()
    for p in parts:
        if _DATE_RE.match(p) or p.isdigit():
            continue
        if strip_digits:
            p = re.sub(r"\d+[a-z]?$", "", p)
        if len(p) < 3 or p in _NOISE_TOKENS:
            continue
        toks.add(p)
    return toks


def identity_index(teams_dir, transcript_path="", session_id=""):
    """The shared identity index, or an empty one that says why it is empty.

    Delegated in full to scripts/lib/teammate-identity.py. Kept as a wrapper so
    that a missing module degrades to "no names, and here is the reason"
    instead of silently reverting to the role — the failure mode this whole
    change exists to remove."""
    if IDENTITY is None:
        return {"names": {}, "sources": {}, "roles": {}, "spawned": [],
                "tried": ["scripts/lib/teammate-identity.py — MISSING, so no "
                          "exact name join was possible at all"],
                "found": []}
    try:
        return IDENTITY.identity_index(teams_dir, transcript_path, session_id)
    except Exception as exc:
        return {"names": {}, "sources": {}, "roles": {}, "spawned": [],
                "tried": ["scripts/lib/teammate-identity.py raised %s" % exc],
                "found": []}


def resolve_worktree_identity(wt_path, index, branch=""):
    """(kind, agent_id, name, role, how) for a worktree path and its branch.

    NAME and ROLE are different fields and are never conflated again. `name` is
    the unique spawn name SendMessage addresses; `role` is the agent_type, and
    a role is not an address. When no exact source resolves the name, `name` is
    EMPTY and `how` says which sources were tried — an honest unresolved beats
    a role wearing a name's field, which is what produced two waivers on
    2026-08-31."""
    base = os.path.basename(wt_path.rstrip("/"))
    if base.startswith("agent-"):
        agent_id = base[len("agent-"):]
        name = index["names"].get(agent_id, "")
        role = index["roles"].get(agent_id, "")
        if name:
            return "native", agent_id, name, role, index["sources"].get(agent_id, "")
        # The agent id itself sometimes embeds the name (a<name>-<hex>). Exact,
        # and it costs nothing to read when the ledgers are silent.
        m = _NATIVE_ID_RE.match(agent_id)
        if m:
            return "native", agent_id, m.group("name"), role, "agent-id"
        return "native", agent_id, "", role, "unresolved"
    # A hand-rolled worktree IS named for its teammate by convention — that is
    # the only naming rule it has — so a basename of the enforced spawn shape
    # is the name, not a guess about one.
    if IDENTITY is not None and IDENTITY.looks_like_teammate_name(base):
        return "hand-rolled", "", base, IDENTITY.role_of(base), "worktree basename"
    # A registered workspace may sit under any directory name (--dir), but its
    # branch is always cc/<teammate name>, minted at registration.
    cc_name = branch[len("cc/"):] if (branch or "").startswith("cc/") else ""
    if IDENTITY is not None and IDENTITY.looks_like_teammate_name(cc_name):
        return "hand-rolled", "", cc_name, IDENTITY.role_of(cc_name), "cc/ branch"
    return "hand-rolled", "", "", "", "unresolved"


def credit_notices(worktrees, notices, tip, index=None):
    """Attach each notice to at most ONE live worktree.

    THE ADDRESS IS THE JOIN. A notice records who it was addressed to; a
    worktree resolves to the set of things that address it (its unique spawn
    name, its agent id, its directory). Equality between the two is the whole
    rule, and it is exact.

    The witness now also records `to_agent_id` — resolved at send time, in the
    lead's own execution, from the same identity module — so the first reading
    does not depend on the debt side being able to resolve the name at all.
    Two independent exact paths to the same fact; either one suffices.

    THE ADDRESS BOOK HAS TWO ENTRIES PER TEAMMATE, because SendMessage takes
    two forms and both are legal: the unique spawn name, and — its own
    documentation says so, "use the raw agentId when the agent has no name" —
    the bare agent id. The lead used the SECOND form on 2026-09-01, and the
    fixed guard immediately reported OWED-NO-NOTICE against a notice that had
    been sent, witnessed and logged, because a HAND-ROLLED worktree in another
    repository has no agent id of its own to compare against. Same defect
    class, one layer over: the halves agreed about the teammate and disagreed
    about which of its two legal addresses counts.

    So the notice's recipient is expanded through the identity index BEFORE
    matching — an id to its name, a name to its id — and either form finds the
    same teammate. The expansion is the exact index, so it adds no guess.

    NO ROLE FALLBACK. `zach-opus-s1` is not credited to a worktree known only
    as `zach`: three Zachs ran at once on the day of the defect, and a
    role-prefix credit would report a teammate as told when a different
    teammate was told. When nothing resolves, the debt stands and the report
    names the sources that came up empty.

    AMBIGUITY CREDITS NOTHING — if a notice's recipient matches two live
    worktrees, neither is credited."""
    tip = (tip or "").lower()
    names = (index or {}).get("names", {}) or {}
    for wt in worktrees:
        wt["notice"] = None
    for note in notices:
        if not any(tip.startswith(tok) and len(tok) >= 7 for tok in note.get("sha_tokens", [])):
            continue
        to = note.get("to", "") or ""
        if not to:
            continue
        # 1. THE WITNESS'S OWN RESOLUTION, joined on agent id. Independent of
        #    whatever the debt side can resolve now.
        to_aid = (note.get("to_agent_id") or "").strip()
        if not to_aid and names and to in names:
            to_aid = to                      # addressed by raw agent id
        hits = [w for w in worktrees if to_aid and w.get("agent_id") == to_aid]
        # 2. The address set: unique spawn name, agent id, directory name —
        #    against BOTH legal forms of the recipient's address.
        if len(hits) != 1:
            forms = {to}
            if to_aid:
                forms.add(to_aid)
                forms.add("agent-" + to_aid)
                if names.get(to_aid):
                    forms.add(names[to_aid])
            hits = [w for w in worktrees
                    if forms & set(w.get("addresses", ()))]
        # 3/4. The token readings, unchanged, for names that predate the
        #    enforced <role>-<model>-<id> shape.
        if len(hits) != 1:
            strict = name_tokens(to, strip_digits=False)
            hits = [w for w in worktrees
                    if strict and len(strict & w["identity_tokens_strict"]) >= 2]
        if len(hits) != 1:
            loose = name_tokens(to)
            hits = [w for w in worktrees
                    if loose and len(loose & w["identity_tokens"]) >= 2]
        if len(hits) != 1:
            continue  # zero matches, or ambiguous — credit nothing
        wt = hits[0]
        prev = wt.get("notice")
        if prev is None or note.get("timestamp", "") > prev.get("timestamp", ""):
            wt["notice"] = note


# --------------------------------------------------------------------------
# ledgers
# --------------------------------------------------------------------------
def read_jsonl(path):
    rows = []
    if not path or not os.path.isfile(path):
        return rows
    try:
        with open(path, "r", encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    rows.append(json.loads(line))
                except Exception:
                    continue
    except Exception:
        pass
    return rows


def notice_ledger_path(teams_dir):
    return os.path.join(teams_dir, "inflight-notices.jsonl") if teams_dir else ""


def waiver_ledger_path(teams_dir):
    return os.path.join(teams_dir, "inflight-waivers.jsonl") if teams_dir else ""


def sha_tokens(text):
    """Every 7-40 char lowercase hex run in a message body.

    Deliberately the ONLY thing extracted from a message besides its length and
    digest. A commit SHA is a public identifier, not a credential, so recording
    it does not reopen the privacy question worker-updated-handoff.sh settled
    when it refused to log message bodies."""
    return sorted({m.group(0) for m in _SHA_RE.finditer((text or "").lower())})


# --------------------------------------------------------------------------
# the ack artifact
# --------------------------------------------------------------------------
def ack_slug(name):
    """A teammate's name as a filename component. Anything that is not a
    filename character becomes a dash; the name itself is repeated INSIDE the
    file, so nothing depends on this being reversible."""
    s = re.sub(r"[^A-Za-z0-9._-]+", "-", (name or "").strip()).strip("-.")
    return s or "unnamed"


def ack_path(worktree, tip, teammate=""):
    """Where THIS teammate's ack for this tip lives.

    KEYED ON THE TEAMMATE AS WELL AS THE SHA, and that is the whole of row g6.
    It used to be `<sha12>.ack` — the sha and nothing else — so when two
    teammates both acknowledged the same land, which is the CORRECT behavior
    because both were notified and both answered, their branches carried two
    different files at ONE path and the merge was an add/add conflict. It
    happened twice on 2026-09-01. A lander in a hurry resolves that with
    `--ours` and silently destroys the evidence that a teammate acknowledged a
    land, after which the witness ledger and the worktree disagree for a reason
    nobody can reconstruct.

    An ack is per-teammate-per-SHA. The multi-teammate case is the NORMAL case;
    it merely took until the fifth concurrent teammate of the night for the
    filename to say so."""
    stem = (tip or "")[:12]
    if teammate:
        return os.path.join(worktree, ACK_DIR_REL, "%s.%s.ack" % (stem, ack_slug(teammate)))
    return os.path.join(worktree, ACK_DIR_REL, "%s.ack" % stem)


def ack_paths(worktree, tip):
    """EVERY ack record for this tip in this worktree, oldest naming scheme
    first.

    Both shapes are read, deliberately and permanently: `<sha12>.ack` is what
    every ack already on main is called, and orphaning those would delete the
    only evidence that those teammates ever answered. A reader that handled one
    record was the other half of the same defect — it could not have reported
    two even once the writer stopped colliding."""
    d = os.path.join(worktree, ACK_DIR_REL)
    stem = (tip or "")[:12]
    if not stem:
        return []
    found = []
    legacy = os.path.join(d, "%s.ack" % stem)
    if os.path.isfile(legacy):
        found.append(legacy)
    try:
        for name in sorted(os.listdir(d)):
            if name == "%s.ack" % stem:
                continue
            if name.startswith(stem + ".") and name.endswith(".ack"):
                p = os.path.join(d, name)
                if os.path.isfile(p):
                    found.append(p)
    except Exception:
        pass
    return found


def parse_ack(path):
    fields = {}
    try:
        with open(path, "r", encoding="utf-8") as fh:
            raw = fh.read()
    except Exception:
        return None, ""
    for line in raw.splitlines():
        m = re.match(r"^([a-z_]+):\s*(.*)$", line.strip())
        if m:
            fields.setdefault(m.group(1), m.group(2).strip())
    return fields, raw


def _check_ack_fields(fields, tip, wt, paths_present=()):
    """THE machine-checkable part of an ack, wherever the ack came from.

    Factored out of _ack_record so the durable ledger row and the worktree file
    are held to ONE definition of a valid ack. Two spellings of "valid" is how
    a mechanism ends up green on one side and red on the other for the same
    fact.

    Returns (problems, notes). A PROBLEM invalidates the ack. A NOTE records
    something a machine could not re-check and says why — never a silent pass,
    because a check that cannot run and reports nothing is indistinguishable
    from a check that ran and found nothing, which is the exact class this row
    belongs to."""
    problems = []
    notes = []
    got_sha = (fields.get("sha") or "").lower()
    if got_sha != tip:
        problems.append(
            "sha: is %r, must be the full 40-char tip %r"
            % (got_sha or "<missing>", tip))
    impact = fields.get("impact") or ""
    if impact not in ACK_IMPACTS:
        problems.append(
            "impact: is %r, must be one of %s"
            % (impact or "<missing>", "/".join(ACK_IMPACTS)))
    detail = fields.get("detail") or ""
    if len(detail) < ACK_MIN_DETAIL:
        problems.append(
            "detail: %d chars, needs >= %d in the teammate's own words"
            % (len(detail), ACK_MIN_DETAIL))
    paths_field = fields.get("paths") or ""
    if not paths_field:
        problems.append("paths: missing — list affected paths, or the word 'none'")
    elif paths_field.strip() != "none":
        moved = set(wt.get("moved_paths", []))
        present_at_write = set(paths_present or ())
        wt_here = os.path.isdir(wt["path"])
        for p in paths_field.split():
            if p in moved:
                continue
            if wt_here and os.path.exists(os.path.join(wt["path"], p)):
                continue
            if p in present_at_write:
                # THE ONE CHECK THAT CANNOT BE REDONE. `paths` is deliberately
                # the field that can only be filled by looking at the
                # teammate's own workspace — so once that workspace is gone,
                # nobody can re-verify it, ever. The writer checked it at the
                # only moment it was checkable and recorded the result; this
                # says so out loud rather than passing quietly or failing an
                # ack for the crime of having been written by an agent that
                # subsequently finished.
                notes.append(
                    "paths: %r was verified present in %s at ack time; that "
                    "worktree is gone now, so this reader could not re-check it"
                    % (p, wt["path"]))
                continue
            problems.append(
                "paths: %r is neither in the moved changeset nor present in "
                "this worktree — it cannot have been read off either" % p)
    return problems, notes


def _ack_record(path, tip, wt):
    """One ack file, checked, and attributed to a teammate.

    ATTRIBUTION FAILS SAFE. A record is treated as this worktree's unless there
    is POSITIVE evidence it belongs to somebody else — an explicit `teammate:`
    that is none of this worktree's addresses, or a `worktree:` naming a
    different directory that still exists. Every ack written before this file
    existed carries neither claim, so all of them keep counting; a renamed or
    moved worktree keeps counting too. Only a record that says out loud that it
    came from elsewhere is set aside, which is exactly the case a merge
    produces."""
    fields, _raw = parse_ack(path)
    fields = fields or {}
    problems, notes = _check_ack_fields(
        fields, tip, wt, (fields.get("paths_present") or "").split())
    detail = fields.get("detail", "")

    teammate = fields.get("teammate", "")
    claimed_wt = fields.get("worktree", "")
    addresses = set(a.lower() for a in (wt.get("addresses") or []) if a)
    addresses.add(os.path.basename(wt["path"].rstrip("/")).lower())
    if teammate:
        own = teammate.lower() in addresses
    elif claimed_wt and norm(claimed_wt) != norm(wt["path"]) and os.path.isdir(claimed_wt):
        own = False
    else:
        own = True

    age = None
    try:
        age = int(time.time() - os.path.getmtime(path))
    except Exception:
        pass
    return {"path": path, "source": "worktree-artifact",
            "teammate": teammate, "worktree": claimed_wt,
            "problems": problems, "notes": notes,
            "verified": not problems, "detail": detail,
            "own": own, "age_sec": age}


# --------------------------------------------------------------------------
# THE DURABLE ACK LEDGER — row 3.19
# --------------------------------------------------------------------------
# THE ARTIFACT ABOVE LIVES IN A DISPOSABLE TREE, AND THE TREE IS DISPOSED OF.
#
# The harness auto-cleans a native isolation worktree that is UNCHANGED at
# completion, and a gitignored write does not make a tree changed. Both
# repositories this engine governs ignore `.claude/*` — femcboost since it
# adopted the engine, and richos since c19cd83 on 2026-09-02, which UNTRACKED
# all 31 acks that had been committed there because they carry the operator's
# absolute home paths, teammate names and session ids and that repository is
# the open-source launch target. So an agent whose only writes were acks has
# its worktree, and every ack in it, deleted the moment it finishes.
#
# That is not a theory. echo-opus-529 reported three acks by path on
# 2026-09-05 — 361590f, 363b0f8, c92488d — and named, in its own handoff, the
# ignore rule that doomed them. All three are absent from the whole of
# /Users/alex/ab today. zach-opus-not1 hit the other half of it: after its
# worktree was cleaned, inflight-ack.sh REFUSED its next ack outright with
# "worktree does not exist", so it could no longer answer at all.
#
# An ack that was written, confirmed and then deleted reads exactly like an ack
# that was never written. That is this project's recurring failure class —
# nothing distinguishes "worked" from "never ran" — and the fix is the one the
# rest of this engine already uses for facts that must outlive the thing that
# produced them: an append-only ledger outside every repository, every worktree
# and every session, beside ~/.claude/state/worktree-ledger.jsonl and
# ~/.claude/state/escalations.jsonl.
#
# WHY NOT UN-IGNORE THE DIRECTORY INSTEAD (row 3.19's option 2): it fixes one
# repository and leaves the mechanism repository-dependent, which IS the
# defect; and it is refuted by a commit LATER than the row, because putting
# those files back under version control re-publishes the operator's home paths
# into the v1 launch target. WHY NOT HAVE A HOOK WRITE IT (option 3): the
# question is WHERE the evidence lives, not WHO writes it. A hook writing into
# the same disposable tree would evaporate identically.

ACK_LEDGER_EVENT = "InflightAck"


def ack_ledger_path():
    """The ONE ack ledger. Overridable for tests and for a non-standard home.

    Same shape and the same override discipline as escalations.ledger_path(),
    deliberately: a second spelling of "where durable state lives" is how two
    halves of one mechanism end up reading different files."""
    p = os.environ.get("RICHOS_INFLIGHT_ACK_LEDGER")
    if p:
        return os.path.abspath(os.path.expanduser(p))
    return os.path.join(os.path.expanduser("~"), ".claude", "state",
                        "inflight-acks.jsonl")


def read_ack_ledger(path=None):
    """(rows, malformed_count, error) — every InflightAck row, in file order.

    UNREADABLE IS NOT EMPTY, and MISSING IS NOT UNREADABLE. A reader that
    returned [] for a ledger it could not open would report "nobody acked" over
    a broken disk, which is the false-green this row exists to remove. The
    error is carried out and rendered rather than swallowed."""
    path = path or ack_ledger_path()
    if not os.path.exists(path):
        return [], 0, ""
    rows, bad = [], 0
    try:
        with open(path, "r", encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    d = json.loads(line)
                except Exception:
                    bad += 1
                    continue
                if isinstance(d, dict) and d.get("event") == ACK_LEDGER_EVENT:
                    rows.append(d)
    except Exception as exc:
        return None, bad, "%s: %s" % (path, exc)
    return rows, bad, ""


def ack_ledger_rows_for_tip(rows, tip, repo_root):
    """Rows that could possibly be about THIS land of THIS repository.

    The sha is matched on the full 40 characters — a short sha is not checkable
    against a tip, which is why the writer refuses one. The repository is
    matched when the row records one; a row that records none is kept, because
    it can still only be credited by a teammate/worktree match below, and that
    match is exact."""
    tip = (tip or "").lower()
    out = []
    for r in rows or ():
        if (r.get("sha") or "").lower() != tip:
            continue
        rrepo = r.get("repo") or ""
        if rrepo and repo_root and norm(rrepo) != norm(repo_root):
            continue
        out.append(r)
    return out


def _ledger_row_is_own(row, wt):
    """Does this ledger row belong to THIS worktree's teammate?

    ATTRIBUTION IS POSITIVE HERE, the opposite of _ack_record's fail-safe, and
    the difference is deliberate. An ack FILE is found inside one worktree, so
    its location is already evidence and the only question is whether a merge
    carried somebody else's in. A LEDGER ROW sits in a machine-wide file next
    to every other teammate's, so location proves nothing: crediting one
    without a positive match would let any teammate's ack discharge everybody
    else's debt, which is the "a store that says acked for everybody" failure —
    worse than the bug it replaces."""
    addresses = set()
    for a in (wt.get("addresses") or ()):
        if a:
            addresses.add(a.lower())
    addresses.add(os.path.basename(wt["path"].rstrip("/")).lower())
    teammate = (row.get("teammate") or "").strip().lower()
    if teammate and teammate in addresses:
        return True
    claimed = row.get("worktree") or ""
    if claimed and norm(claimed) == norm(wt["path"]):
        return True
    return False


def _age_of_iso(ts):
    """Seconds since a UTC ISO stamp, or None.

    READ AS UTC WITH calendar.timegm, NEVER time.mktime(...) - time.timezone. mktime assumes
    LOCAL time and time.timezone is the STANDARD offset, so under daylight saving every stamp
    read an hour old: measured 2026-09-26 in BST, a notice sent seconds earlier read "60 min
    since the notice (OVERDUE)", against a 30-minute ack timeout. containers.py fixed the same
    defect on 2026-09-13 (containers.test.py C10); this file kept the old reading at both of
    its age sites, which now share this one."""
    if not ts:
        return None
    try:
        sent = calendar.timegm(time.strptime(ts[:19], "%Y-%m-%dT%H:%M:%S"))
        return int(time.time() - sent)
    except Exception:
        return None


def _ledger_record(row, tip, wt):
    """One ledger row, checked by the same rules as the file it mirrors."""
    fields = {
        "sha": (row.get("sha") or "").lower(),
        "impact": row.get("impact") or "",
        "detail": row.get("detail") or "",
        "paths": row.get("paths") or "",
        "teammate": row.get("teammate") or "",
        "worktree": row.get("worktree") or "",
    }
    present = row.get("paths_present") or []
    problems, notes = _check_ack_fields(fields, tip, wt, present)
    return {"path": "%s (row %s)" % (ack_ledger_path(), row.get("timestamp", "")),
            "source": "durable-ledger",
            "teammate": fields["teammate"], "worktree": fields["worktree"],
            "problems": problems, "notes": notes,
            "verified": not problems, "detail": fields["detail"],
            "own": _ledger_row_is_own(row, wt),
            "age_sec": _age_of_iso(row.get("timestamp", "")),
            "row": row}


def orphan_ledger_acks(rows, tip, repo_root, worktrees):
    """Acks whose teammate has NO registered worktree left.

    THIS IS THE WHOLE POINT OF THE ROW. When the harness removes a worktree it
    also deregisters it, so the teammate stops appearing in `git worktree list`
    entirely and drops out of every per-worktree loop below. Before this
    existed, an ack in that state did not read as invalid — it did not read AT
    ALL, and the difference between "this teammate complied" and "this teammate
    never existed" was nothing at all. They are listed separately rather than
    folded in, because a teammate that has finished is not owed a chase, and
    saying which is which is the point."""
    out = []
    for r in ack_ledger_rows_for_tip(rows, tip, repo_root):
        if any(_ledger_row_is_own(r, wt) for wt in worktrees):
            continue
        out.append({
            "teammate": r.get("teammate") or "",
            "worktree": r.get("worktree") or "",
            "worktree_present": bool(r.get("worktree")) and os.path.isdir(r["worktree"]),
            "branch": r.get("branch") or "",
            "impact": r.get("impact") or "",
            "detail": r.get("detail") or "",
            "paths": r.get("paths") or "",
            "timestamp": r.get("timestamp") or "",
        })
    return out


def ack_status(wt, tip, notices_ts, worker_updates, timeout_min, ledger_rows=()):
    """Did this teammate prove it holds the new fact?

    THREE CHANNELS, AND THEY PROVE DIFFERENT AMOUNTS.

    DURABLE — a row in ~/.claude/state/inflight-acks.jsonl, written by the same
    command that writes the artifact, in a file that is outside every
    repository, every worktree and every session. It is listed first because it
    is the only one of the three that is still there after the teammate's
    worktree has been removed, which — as row 3.19 records — is the normal end
    of a teammate's life rather than an edge case. It carries the identical
    fields and is held to the identical checks; the only thing it cannot do is
    re-check `paths` against a workspace that no longer exists, and where that
    bites it SAYS SO instead of passing quietly.

    PRIMARY-BY-HISTORY — the artifact at <worktree>/.claude/inflight-acks/<tip12>.ack,
    written by the teammate (scripts/inflight-ack.sh writes it). Independent of
    the mailbox in both directions, survives the session, and the lead verifies
    it with stat + read. Machine-checkable:
        sha:     must equal the full 40-char tip EXACTLY. A teammate that does
                 not hold the new fact cannot produce it.
        impact:  one of conflict / stale-record / grew-scope / none — §8b's own
                 three questions plus "not affected", forcing a choice rather
                 than a nod.
        detail:  >= 40 characters of the teammate's own words.
        paths:   repo-relative paths, each of which must EXIST in that
                 teammate's worktree or in the moved changeset — the one field
                 that cannot be filled by copying the notice, because it is
                 about the teammate's own workspace.

    WHAT NO MACHINE HERE CAN CHECK, stated plainly rather than dressed up: the
    `detail` line is checked for LENGTH and for not being a copy of anything in
    the notice. Whether it is CORRECT — whether the teammate actually
    understood which of its assumptions broke — is comprehension, and a string
    match is not comprehension. The verifier prints `detail` and labels it
    HUMAN JUDGMENT REQUIRED. A guard that scored that line would be inventing a
    reading it did not do.

    SECONDARY — a witnessed reply: a WorkerUpdated row in worker-events.jsonl
    from this teammate, after the notice, with the tip's short SHA in its
    `summary`. That hook fires inside the SENDER's execution when its tool call
    resolves, so it witnesses the send even when delivery drops — a durable
    substrate, not a mailbox read. It is secondary because that log stores only
    the 200-character summary, so it can carry a token but never a restatement.
    It proves DELIVERY AND READING. It does not prove content."""
    tip = (tip or "").lower()
    short = tip[:12]
    paths = ack_paths(wt["path"], tip)
    out = {"state": "none", "verified": False, "problems": [], "notes": [],
           "detail": "",
           "path": ack_path(wt["path"], tip, wt.get("resolved_name") or ""),
           "records": [], "channel": "", "age_sec": None, "overdue": False,
           "sources": []}

    records = [_ack_record(p, tip, wt) for p in paths]
    # THE DURABLE HALF. Only rows this teammate positively owns — see
    # _ledger_row_is_own for why attribution here is positive rather than
    # fail-safe. A ledger row is added even when an artifact is present: the
    # two are written together, and a case where one survives and the other
    # does not is exactly the thing worth being able to see.
    records += [_ledger_record(r, tip, wt)
                for r in ledger_rows if _ledger_row_is_own(r, wt)]

    if records:
        srcs = sorted({r.get("source", "") for r in records})
        out["sources"] = srcs
        out["state"] = ("ledger" if srcs == ["durable-ledger"]
                        else "artifact" if srcs == ["worktree-artifact"]
                        else "artifact+ledger")
        out["channel"] = " + ".join(srcs)
        out["records"] = records
        own = [r for r in records if r["own"]]
        chosen = own or []
        if chosen:
            best = ([r for r in chosen if r["verified"]] or chosen)[0]
            out["path"] = best["path"]
            out["problems"] = best["problems"]
            out["notes"] = best.get("notes", [])
            out["verified"] = best["verified"]
            out["detail"] = best["detail"]
            out["age_sec"] = best["age_sec"]
        else:
            # RECORDS ARE HERE, BUT NONE OF THEM IS THIS TEAMMATE'S. That is
            # what a merge carrying somebody else's ack into this worktree looks
            # like, and crediting it would let one teammate's answer discharge
            # another's debt. Named, never silently counted.
            out["path"] = records[0]["path"]
            out["problems"] = [
                "no ack here was written by this teammate — the %d record(s) "
                "present belong to: %s"
                % (len(records),
                   ", ".join(r["teammate"] or os.path.basename(r["path"])
                             for r in records))]
            out["verified"] = False
            out["detail"] = ""
            out["age_sec"] = records[0]["age_sec"]
        return out

    for row in worker_updates:
        if row.get("agent_type") != wt.get("resolved_name"):
            continue
        if notices_ts and row.get("timestamp", "") <= notices_ts:
            continue
        summary = (row.get("summary") or "").lower()
        if short[:7] and short[:7] in summary:
            out["state"] = "witnessed"
            out["channel"] = "worker-events WorkerUpdated"
            out["verified"] = True
            out["detail"] = row.get("summary") or ""
            return out

    if notices_ts:
        # The ledger writes UTC; compare in UTC (see _age_of_iso).
        age = _age_of_iso(notices_ts)
        if age is not None:
            out["age_sec"] = age
            out["overdue"] = age > timeout_min * 60
    return out


# --------------------------------------------------------------------------
# the assessment
# --------------------------------------------------------------------------
def _recorded_integration_tip(root):
    """(tip, how) — the tip of the branch RECORDED as the one this body of work
    integrates on, asked of mega-lander/workspaces.py.

    Loading the library by path is a duplicated LOADER, not a duplicated
    ANSWER: there is one implementation of the question.

    This one does NOT hard-abstain, and the difference is deliberate. A land
    notice is about a commit the caller just made and normally names; the
    fallback exists only for a bare call with no tip, where returning nothing
    would silence the notice entirely. So it falls back to HEAD and RECORDS
    that it did, in a field the caller reports -- a named guess is a different
    thing from an unnamed one."""
    lib = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "mega-lander", "workspaces.py")
    try:
        import importlib.util
        spec = importlib.util.spec_from_file_location("workspaces_answer", lib)
        ws = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ws)
        branch, tip, why_not = ws.integration_for(root)
        if not why_not and tip:
            return tip, "the branch recorded for this body of work (%s)" % branch
        why = why_not or "the recorded branch has no tip"
    except Exception as exc:
        why = "the branch this work integrates on could not be read: %s" % exc
    return (git(root, "rev-parse", "HEAD").strip(),
            "the main checkout's HEAD, because %s" % why)


def assess(repo, tip=None, teams_dir="", timeout_min=DEFAULT_ACK_TIMEOUT_MIN,
           session_id="", transcript_path=""):
    root = main_checkout(repo)
    if not tip:
        # THE ONE ANSWER, ASKED OF THE LIBRARY (point 14). Every worktree below
        # gets `landed` from `is_ancestor(head, tip)`, so `tip` IS this file's
        # answer to "has this work landed". It used to be the main checkout's
        # HEAD -- "whatever main happens to have", which point 14 names as the
        # thing that stops "landed" meaning anything. It is now the tip of the
        # branch RECORDED for this body of work; where nothing is recorded it
        # falls back to HEAD and SAYS SO in the assessment rather than passing
        # the guess off as the answer.
        tip, tip_source = _recorded_integration_tip(root)
    else:
        tip_source = "given by the caller"
    tip = (tip or "").lower()

    # The team directory is resolved HERE when the caller did not name one, so
    # that the runner, the guard and the Stop notice cannot disagree about
    # which ledger they are talking about.
    teams_dir_source = os.environ.get("INFLIGHT_TEAMS_DIR_SOURCE") or "given by the caller"
    if not teams_dir and IDENTITY is not None:
        try:
            teams_dir, teams_dir_source = IDENTITY.resolve_teams_dir(session_id)
        except Exception:
            teams_dir, teams_dir_source = "", "resolution raised"

    notices = [r for r in read_jsonl(notice_ledger_path(teams_dir))
               if r.get("event") == "InflightNotice"]
    waivers = [r for r in read_jsonl(waiver_ledger_path(teams_dir))
               if r.get("event") == "InflightWaiver"]
    updates = [r for r in read_jsonl(os.path.join(teams_dir, "worker-events.jsonl"))
               if r.get("event") == "WorkerUpdated"] if teams_dir else []
    index = identity_index(teams_dir, transcript_path, session_id)

    # THE DURABLE ACK LEDGER, read ONCE for the whole assessment. Outside every
    # repository, worktree and session, so it is the only ack source that is
    # still readable after a teammate's worktree has been removed — which is
    # how every teammate ends. An unreadable ledger is carried out as an ERROR
    # and rendered; it is never allowed to look like "nobody acked".
    _ledger_rows, _ledger_bad, _ledger_err = read_ack_ledger()
    ledger_tip_rows = ack_ledger_rows_for_tip(_ledger_rows or [], tip, root)

    result = {
        "repo": os.path.abspath(repo),
        "main_checkout": root,
        "tip": tip,
        "tip_source": tip_source,
        "ack_ledger": ack_ledger_path(),
        "ack_ledger_error": _ledger_err,
        "ack_ledger_malformed": _ledger_bad,
        "ack_ledger_rows_for_tip": len(ledger_tip_rows),
        "orphan_acks": [],
        "teams_dir": teams_dir,
        "teams_dir_source": teams_dir_source,
        "notice_ledger": notice_ledger_path(teams_dir),
        "waiver_ledger": waiver_ledger_path(teams_dir),
        "ack_timeout_min": timeout_min,
        "identity_sources_tried": index.get("tried", []),
        "identity_sources_found": index.get("found", []),
        "worktrees": [],
        "retired_workspaces": [],
        "blocking": [],
        "unacked": [],
        "live_count": 0,
    }
    if not tip:
        result["error"] = "could not resolve a tip commit for %s" % repo
        return result

    try:
        inventory = list_worktrees(root, strict=True)
    except RuntimeError as exc:
        # NO PEERS IS AN ANSWER; "git would not say" IS NOT. Fail closed: main()
        # exits 2 on an error and guard-inflight-notify.sh refuses on it.
        result["error"] = "the worktree inventory could not be read (%s), so who is in flight is UNKNOWN" % exc
        return result
    memo = GitMemo(root)
    for entry in inventory:
        path = entry["path"]
        if norm(path) == norm(root):
            continue  # the main checkout is not a teammate
        why_retired = retired_workspace(path)
        if why_retired:
            # A finished teammate's quarantine, still registered since 6472bb60.
            # Recorded rather than dropped silently: a skip nobody can see is a
            # teammate nobody can see.
            result["retired_workspaces"].append({"path": path, "reason": why_retired})
            continue
        kind, agent_id, name, role, how = resolve_worktree_identity(path, index, entry["branch"])
        base = os.path.basename(path.rstrip("/"))
        # EVERY EXACT WAY THIS WORKTREE CAN BE ADDRESSED. A notice is credited
        # on equality with one of these and on nothing looser; the role is
        # deliberately absent, because a role is not an address.
        addresses = {a for a in (name, base, agent_id,
                                 ("agent-" + agent_id) if agent_id else "") if a}
        wt = {
            "path": path,
            "branch": entry["branch"],
            "head": entry["head"],
            "kind": kind,
            "agent_id": agent_id,
            "resolved_name": name,
            "role": role,
            "name_source": how,
            "addresses": sorted(addresses),
            "identity_tokens": name_tokens(name or base),
            "identity_tokens_strict": name_tokens(name or base, strip_digits=False),
            "present": os.path.isdir(path),
        }

        # --- liveness (see the module header) ---------------------------
        if kind == "native":
            lock = entry["locked"]
            if not lock:
                wt["liveness"] = "not-live (native worktree, unlocked)"
                wt["live"] = False
            else:
                m = re.search(r"\(pid\s+(\d+)", lock)
                if not m:
                    wt["liveness"] = "not-live (locked, but the lock line carries no pid)"
                    wt["live"] = False
                elif pid_alive(m.group(1)):
                    wt["liveness"] = "LIVE (locked, pid %s running)" % m.group(1)
                    wt["live"] = True
                else:
                    wt["liveness"] = "not-live (stale lock, pid %s is dead)" % m.group(1)
                    wt["live"] = False
        elif not name:
            # The nightly's frozen source, a release check, a codex/ branch, the
            # lead's own checkout: nothing names a teammate here, so nobody here
            # can be told, ack or be stopped (2026-10-24).
            wt["liveness"] = "not a teammate (hand-rolled checkout that resolves to no teammate name)"
            wt["live"] = False
        else:
            wt["liveness"] = "presumed LIVE (hand-rolled worktree — no lock is ever taken here, so quiet is not death)"
            wt["live"] = True
        if not wt["present"]:
            wt["liveness"] = "not-live (registered, but the directory is gone)"
            wt["live"] = False

        head = entry["head"] or wt["branch"]
        wt["scan_gap"] = ""
        ancestor = lambda a, b: memo.ask(("is-ancestor", a, b), (a, b),
                                         lambda: ancestor_state(root, a, b), lambda v: v is not None)
        asked = lambda key, shas, *args: memo.ask(key, shas, lambda: git_checked(root, *args),
                                                  lambda v: bool(v[0]))
        landed_state = ancestor(head, tip) if head else False
        behind_state = ancestor(tip, head) if head else False
        if landed_state is None or behind_state is None:
            wt["scan_gap"] = "git could not say whether %s contains or trails %s" % (head[:12], tip[:12])
        contained = bool(head) and landed_state is True
        wt["landed"] = contained
        wt["behind"] = bool(head) and behind_state is not True

        wt["base"] = ""
        wt["moved_shas"] = []
        wt["moved_paths"] = []
        wt["own_paths"] = []
        wt["overlap"] = []
        if wt["behind"] and head:
            ok_b, out_b = asked(("merge-base", tip, head), (tip, head), "merge-base", tip, head)
            base = out_b.strip() if ok_b else ""
            wt["base"] = base
            if not base:
                # Behind, but the point it diverged from cannot be named (git
                # failed, or there is no common ancestor). That is NOT "nothing
                # moved under it": the verdict below would say CLEAN-NOT-BEHIND.
                wt["scan_gap"] = "the merge-base of %s and %s could not be computed, so what moved under it is UNKNOWN" % (tip[:12], head[:12])
            else:
                ok1, shas = asked(("rev-list", base, tip), (base, tip), "rev-list", "%s..%s" % (base, tip))
                ok2, moved = asked(("diff", base, tip), (base, tip), "diff", "--name-only", base, tip)
                ok3, own = asked(("diff", base, head), (base, head), "diff", "--name-only", base, head)
                if not (ok1 and ok2 and ok3):
                    wt["scan_gap"] = "git could not list the commits or paths that moved since %s" % base[:12]
                wt["moved_shas"] = shas.split()
                wt["moved_paths"] = sorted(set(moved.split("\n")) - {""})
                wt["own_paths"] = sorted(set(own.split("\n")) - {""})
                wt["overlap"] = sorted(set(wt["moved_paths"]) & set(wt["own_paths"]))

        result["worktrees"].append(wt)
    memo.save()

    live = [w for w in result["worktrees"] if w["live"]]
    result["live_count"] = len(live)
    credit_notices(live, notices, tip, index)

    for wt in result["worktrees"]:
        wt.setdefault("notice", None)
        wt["waiver"] = None
        for w in waivers:
            if w.get("tip", "").lower() != tip:
                continue
            if norm(w.get("worktree", "")) == norm(wt["path"]):
                wt["waiver"] = w
        wt["identity_tokens"] = sorted(wt["identity_tokens"])
        wt["identity_tokens_strict"] = sorted(wt["identity_tokens_strict"])

        if not wt["live"]:
            wt["verdict"] = "NOT-LIVE"
            # A FINISHED TEAMMATE IS NOT OWED A CHASE, BUT ITS ANSWER STILL
            # COUNTS. Before the durable ledger this branch left `ack` at the
            # "n/a" default, so the moment a teammate stopped running — or its
            # directory was removed under it — every ack it had ever written
            # stopped being readable through this predicate. Nothing here goes
            # into `blocking` or `unacked`; the verdict is unchanged. The ack
            # is simply no longer thrown away.
            wt["ack"] = ack_status(wt, tip, "", updates, timeout_min,
                                   ledger_tip_rows)
        elif wt["scan_gap"]:
            wt["verdict"] = "UNVERIFIED"    # never a CLEAN-* verdict on a question git did not answer
        elif wt["landed"]:
            wt["verdict"] = "CLEAN-LANDED"
        elif not wt["behind"] or not wt["moved_shas"]:
            wt["verdict"] = "CLEAN-NOT-BEHIND"
        elif wt["waiver"]:
            wt["verdict"] = "WAIVED"
        elif not wt["notice"]:
            wt["verdict"] = "OWED-NO-NOTICE"
            result["blocking"].append(wt["path"])
        else:
            ack = ack_status(wt, tip, wt["notice"].get("timestamp", ""), updates,
                             timeout_min, ledger_tip_rows)
            wt["ack"] = ack
            if ack["verified"]:
                wt["verdict"] = "NOTIFIED-ACKED"
            elif ack["state"] in ("artifact", "ledger", "artifact+ledger"):
                wt["verdict"] = "NOTIFIED-ACK-INVALID"
                result["unacked"].append(wt["path"])
            else:
                wt["verdict"] = "NOTIFIED-NO-ACK"
                result["unacked"].append(wt["path"])
        wt.setdefault("ack", {"state": "n/a", "verified": False, "problems": [],
                              "notes": [], "detail": "", "path": "",
                              "records": [], "channel": "", "sources": [],
                              "age_sec": None, "overdue": False})

    # ACKS WITH NO WORKTREE LEFT AT ALL. `git worktree list` stops naming a
    # worktree the moment it is removed, so its teammate never enters the loop
    # above and — before the ledger — its ack did not read as invalid, it did
    # not read at all. This is row 3.19's exact observed failure: echo-opus-529
    # wrote three acks and finished, and the evidence went with the directory.
    result["orphan_acks"] = orphan_ledger_acks(
        _ledger_rows or [], tip, root, result["worktrees"])
    # Only a LIVE worktree's gap matters: a finished teammate's verdict is
    # NOT-LIVE whatever git says, and one odd leftover must not refuse every
    # push. A live teammate git could not be asked about is a debt nobody can
    # see, so it is an error (exit 2, which the guard refuses on).
    gaps = ["%s: %s" % (w["path"], w["scan_gap"])
            for w in result["worktrees"] if w["live"] and w["scan_gap"]]
    if gaps:
        result["error"] = ("the sweep could not finish reading git, so the verdict for %d "
                           "worktree question(s) is UNKNOWN: %s" % (len(gaps), "; ".join(gaps)))
    return result


# --------------------------------------------------------------------------
# rendering
# --------------------------------------------------------------------------
def render_text(res):
    out = []
    a = out.append
    a("in-flight sweep — %s" % res["main_checkout"])
    a("  tip            : %s" % res["tip"])
    a("  tip is         : %s" % res.get("tip_source", "(not recorded)"))
    a("  ack timeout    : %s min" % res["ack_timeout_min"])
    a("  notice ledger  : %s" % (res["notice_ledger"] or "<no team dir resolved>"))
    a("  ack ledger     : %s (%d row(s) for this tip)"
      % (res.get("ack_ledger", ""), res.get("ack_ledger_rows_for_tip", 0)))
    # AN UNREADABLE LEDGER IS NOT AN EMPTY ONE. Said on screen, because the
    # whole point of this ledger is that it distinguishes "nobody acked" from
    # "the evidence is gone", and a reader that swallowed its own read error
    # would put those two back together again.
    if res.get("ack_ledger_error"):
        a("      ** THE ACK LEDGER COULD NOT BE READ: %s" % res["ack_ledger_error"])
        a("         Every ack below is therefore reported from the worktree files")
        a("         ALONE, which is the state row 3.19 exists to fix. Fix the ledger.")
    if res.get("ack_ledger_malformed"):
        a("      ** %d malformed line(s) in the ack ledger were SKIPPED — not counted as acks."
          % res["ack_ledger_malformed"])
    a("  resolved by    : %s" % res.get("teams_dir_source", ""))
    a("  identity from  : %s"
      % ("; ".join(res.get("identity_sources_found") or [])
         or "NOTHING RESOLVED — sources tried: %s"
            % "; ".join(res.get("identity_sources_tried") or ["<none>"])))
    if res.get("error"):
        a("  ERROR: %s" % res["error"])
        return "\n".join(out)
    if not res["worktrees"]:
        a("  NO teammate worktrees registered — nothing is in flight, nothing is owed.")
        return "\n".join(out)
    for wt in res["worktrees"]:
        a("")
        a("  %-22s %s" % (wt["verdict"], wt["path"]))
        a("      branch     : %s @ %s" % (wt["branch"] or "(detached)", (wt["head"] or "")[:12]))
        a("      teammate   : %s (%s)%s"
          % (wt["resolved_name"] or "<unresolved>", wt["name_source"],
             ("   role: %s" % wt["role"]) if wt.get("role") else ""))
        if not wt["resolved_name"]:
            a("                   NO EXACT NAME JOIN. A notice is addressed to the")
            a("                   unique spawn name, so nothing can be credited to this")
            a("                   worktree until one of these resolves it:")
            for src in (res.get("identity_sources_tried") or []):
                a("                     - %s" % src)
        a("      liveness   : %s" % wt["liveness"])
        if wt["behind"] and wt["moved_shas"]:
            a("      base       : %s" % wt["base"][:12])
            a("      moved      : %d commit(s), %d path(s)%s"
              % (len(wt["moved_shas"]), len(wt["moved_paths"]),
                 ("; OVERLAPS %d of its own file(s): %s"
                  % (len(wt["overlap"]), " ".join(wt["overlap"][:6]))) if wt["overlap"] else ""))
        if wt.get("notice"):
            a("      notified   : %s  (to %r)" % (wt["notice"].get("timestamp", ""), wt["notice"].get("to", "")))
        if wt.get("waiver"):
            a("      WAIVED     : %s" % wt["waiver"].get("reason", ""))
        ack = wt.get("ack") or {}
        if ack.get("state") in ("artifact", "ledger", "artifact+ledger"):
            a("      ack        : %s  %s  [%s]"
              % (ack["path"], "VERIFIED" if ack["verified"] else "INVALID",
                 ack.get("channel", "")))
            # EVERY record, not just the one that decided. Two teammates
            # acknowledging one land is the normal case, and a reader that
            # printed one of them would hide the very collision row g6 is about.
            recs = ack.get("records") or []
            if len(recs) > 1:
                a("      records    : %d for this tip in this worktree" % len(recs))
                for r in recs:
                    a("        %-9s %-16s %s  %s"
                      % ("(this)" if r.get("own") else "(another)",
                         r.get("source", ""),
                         os.path.basename(r["path"]),
                         "verified" if r.get("verified") else "INVALID"))
            for p in ack.get("problems", []):
                a("        - %s" % p)
            # A CHECK THAT COULD NOT RUN NAMES ITSELF. It never reads as a
            # silent pass, because a silent pass is the failure class this
            # whole row belongs to.
            for n in ack.get("notes", []):
                a("        ~ COULD NOT RE-CHECK: %s" % n)
            if ack.get("detail"):
                a("        detail (HUMAN JUDGMENT REQUIRED — no machine here reads it for correctness):")
                a("          %s" % ack["detail"])
        elif ack.get("state") == "witnessed":
            a("      ack        : witnessed reply (%s) — proves delivery+reading, NOT content" % ack["channel"])
        elif ack.get("state") == "none" and wt.get("notice"):
            age = ack.get("age_sec")
            a("      ack        : NONE%s" % (" — %d min since the notice%s"
              % (age // 60, " (OVERDUE)" if ack.get("overdue") else "") if age is not None else ""))
    # ACKS THAT OUTLIVED THEIR WORKTREE. Printed as their own section because
    # they answer a question no per-worktree line can: this teammate complied,
    # and then it finished and its workspace was removed. Silence here used to
    # be indistinguishable from a teammate that never acked at all.
    orphans = res.get("orphan_acks") or []
    if orphans:
        a("")
        a("  ACKED, WORKTREE GONE — %d record(s) from the durable ledger" % len(orphans))
        a("  These teammates answered for this tip and no longer have a registered")
        a("  worktree. They are NOT owed a chase. Before the ledger this section was")
        a("  empty for the same reason it is not empty now: the evidence was deleted")
        a("  with the directory, and nothing said so.")
        for o in orphans:
            a("    %s  (%s)" % (o["teammate"] or "<unnamed>", o["timestamp"]))
            a("        worktree : %s%s"
              % (o["worktree"] or "<none recorded>",
                 "" if o["worktree_present"] else "   [GONE]"))
            a("        impact   : %s   paths: %s" % (o["impact"], o["paths"]))
            a("        detail (HUMAN JUDGMENT REQUIRED):")
            a("          %s" % o["detail"])
    a("")
    a("  live worktrees: %d   blocking: %d   notified-but-unacked: %d   acked-worktree-gone: %d"
      % (res["live_count"], len(res["blocking"]), len(res["unacked"]), len(orphans)))
    return "\n".join(out)


def main(argv):
    import argparse
    ap = argparse.ArgumentParser(prog="inflight.py")
    ap.add_argument("--repo", required=True)
    ap.add_argument("--tip", default="")
    ap.add_argument("--teams-dir", default="")
    ap.add_argument("--timeout-min", type=int, default=DEFAULT_ACK_TIMEOUT_MIN)
    ap.add_argument("--session", default="")
    ap.add_argument("--transcript", default="")
    ap.add_argument("--format", choices=("json", "text"), default="text")
    args = ap.parse_args(argv)

    res = assess(args.repo, args.tip, args.teams_dir, args.timeout_min,
                 args.session, args.transcript)
    if args.format == "json":
        print(json.dumps(res, indent=2, sort_keys=True))
    else:
        print(render_text(res))
    if res.get("error"):
        return 2
    return 1 if res["blocking"] else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
