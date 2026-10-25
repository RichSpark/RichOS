#!/usr/bin/env python3
"""
guard-unresolved-claims.py — the analysis half of the Stop-time claim gate.

Called by guard-unresolved-claims.sh, which has already resolved the two roots
and decided that this repository adopted the engine. This file does the
reading, the resolving and the verdict; it never decides whether to run.

WHAT THIS IS FOR
  Every other guard in this engine watches the REPOSITORY. This one watches the
  TURN -- specifically the last thing the orchestrator says before it stops
  talking, which is the one surface on which an untrue statement is free.

THE PREDICATE, AND WHY IT IS THIS ONE
  The first design read prose: it looked for "dispatching" and asked whether an
  Agent call had been made. That is a heuristic over natural language and it was
  measured at 17% precision (1 true positive against 5 false ones across 418
  real turns) -- every false one was the orchestrator correctly describing a
  PAST action, a NEGATED action ("I'm not spawning anything further"), or a past
  incident. A gate at 17% precision is worse than no gate: it teaches the thing
  it governs to route around it, and then it protects nothing.

  So this gate does not detect INTENT. It resolves IDENTIFIERS. An identifier
  either resolves against ground truth or it does not; there is no threshold to
  tune and nothing to interpret.

  The STATE-CLAIM class added on 2026-09-01 keeps that rule and widens the
  question by one step. It reads English only far enough to decide WHICH
  QUESTION TO ASK GIT -- "you said landed, so is this an ancestor of main?" --
  and git answers. The night it was built, three things had been told to the CEO
  as fact in one session: a SHA that was typed rather than read, a merge that a
  PreToolUse guard had refused and that was reported as landed anyway, and a
  value called gone from the whole app when only one of its spellings had been
  replaced. One cause under all three: the report was written from the INTENT of
  a command rather than from the artifact.

    * the fabricated SHA was ALREADY refused here, by the existence check --
      verified, not assumed, before anything was written
    * the unmerged merge is the new blocking class below (state_claims)
    * the value was measured and is NOT enforceable; it reports. The numbers
      and the circular evidence that nearly made it look enforceable are above
      value_absence_claims()

WHY SOME IDENTIFIERS BLOCK AND OTHERS ONLY REPORT
  The property that makes a check safe to block on is not exactness. It is
  MONOTONICITY OF THE GROUND TRUTH.

    * agent names -- ground truth is the spawned-names ledger, the roster and
      the idle/task event logs. detect-nonnative-worktree.sh only ever APPENDS
      to the ledger, and only for an Agent call that actually executed. Nothing
      removes an entry. So within a session the set only grows, and an
      unresolved name can mean exactly one thing: never spawned. BLOCKS.

    * commit SHAs -- ground truth is the object database, which SHRINKS: a
      history rewrite plus gc drops commits that were real when cited. Measured
      cold against today's object DB, 8 of 262 SHA citations failed and all 8
      were casualties of one rewrite the orchestrator itself announced. Adding
      session-wide grounding (below) took that to 0/262. BLOCKS, with grounding.

    * a commit's POSITION -- "landed", "pushed" -- ground truth is the ref
      graph, and reachability from a ref is the monotonic half of it. A rewrite
      strands old commits with no ref pointing at them, so "on a branch that is
      not main" is a state a rewrite cannot manufacture. Requiring it took the
      corpus from 41 fires to 0. BLOCKS. See state_claims().

    * file paths -- ground truth is the filesystem, which shrinks harder: files
      are deleted, renamed, and moved, and reporting a deletion correctly means
      naming a path that no longer resolves. Worse, a relative path is only
      meaningful against a base this process cannot know. Even with grounding,
      15 of 572 path citations failed and they were legitimate: hypothetical
      adopter paths (`~/ab/myrichos`), paths inside a landed-and-removed
      worktree, mockup-round directories relative to an unknown base. 2.6% is
      not zero, so paths REPORT and never block.

GROUNDING -- the relaxation that makes existence-checks safe
  A claim passes if it resolves NOW, *or* if the token appeared anywhere in this
  session's tool traffic. The reasoning: to correctly report that a thing was
  deleted, moved or rewritten, you must have OBSERVED it, and observation means
  tool output. Existence is non-monotonic; observation-within-a-session is
  monotonic, because the transcript only grows. Grounding restores the
  can't-be-wrong property that raw existence checking loses.

  It costs recall -- anything the orchestrator grepped for is thereafter
  citable. That trade is deliberate and it is the right way round: a missed
  check costs one undetected claim, a false alarm costs the whole gate.

CONSERVATISM RULES (all of these make the gate quieter, never louder)
  * a token that is ambiguous is IGNORED, never flagged
  * SHAs are read only from backtick-delimited citations that are ENTIRELY the
    hex token, with at least one digit and one letter -- so UUID fragments,
    tool-use ids and English words never enter
  * the agent-name check is INERT unless this session's own team directory
    exists, because absent ground truth cannot prove absence
  * any resolver error fails OPEN

WHAT IT CANNOT SEE
  * a nameless action claim -- "dispatching it now", with no agent named --
    contains no identifier at all, so this gate passes it. Requiring that such a
    message name nobody was PROPOSED as a gate and MEASURED at 10.3%: 29 of the
    30 prose hits in 3,532 real messages name nobody, so the refinement removes
    one hit in thirty. See nameless_dispatch(). Reported, never enforced.
  * a bare-role claim about a role that HAS been dispatched before -- "Zach is
    building it now" when a zach agent ran earlier in the session for other
    work. That is the 2026-08-31 failure exactly, and the monotonic test passes
    it. Only live_roles() can see it, liveness is not monotonic, and so it
    reports. Stated here rather than discovered later.
  * a claim that is false about the WORLD but true about identifiers ("the
    tests pass" when they do not)
  * anything after the turn ends. This is a point-in-time check: it proves the
    identifier resolved when the claim was made. A later rewrite that kills an
    already-published SHA is a different problem with a different guard.

  * an absence claim in general -- "X is removed", "zero references to Y".
    Grep-backed, it fires on 95 of 109 real literals. Not enforceable, and the
    one narrow version that looked clean turned out to be scoring itself
    against a doc written after the incident. See value_absence_claims().

Exit codes:
  0  nothing unresolved, or the check could not be evaluated
  2  an identifier resolves against nothing, or a state claim contradicts the
     repository
"""

import json
import os
import re
import subprocess
import sys
import time

# --------------------------------------------------------------------------
# extraction
# --------------------------------------------------------------------------

# <role>-<model>-<identifier>, where <model> is one of the four real aliases.
# The middle token is what makes this unambiguous: no English sentence contains
# "word-opus-word" by accident.
AGENT_RE = re.compile(
    r"(?<![A-Za-z0-9_-])"
    r"([a-z][a-z0-9]{1,15})-(fable|opus|sonnet|haiku)-([a-z0-9]{1,12})"
    r"(?![A-Za-z0-9_-])"
)

# A BARE ROLE, USED AS THE SUBJECT OF A PRESENT-PROGRESSIVE VERB.
#
#     "Zach is building it now"        "Dean is writing both hires"
#
# THIS IS THE SHAPE OF THE FAILURE, and it is a far narrower trigger than the
# prose signal's `dispatching|spawning|launching`. Measured over 3,532 real
# final messages it fires 103 times and EVERY ONE of the 103 is a genuine
# reference to a teammate — no ordinary-noun use of "art", "mark", "will" or
# "ray" enters, because the role must be CAPITALIZED and must be the subject of
# a progressive verb. As an extractor it is 103/103.
#
# The role list is DERIVED, never typed: agent definitions on disk, unioned
# with the session roster's own agentType values. A role nobody has defined is
# not a role, and an empty set makes the whole check inert.
PROGRESSIVE = r"(?:is|'s)\s+(?:now\s+|already\s+|still\s+|currently\s+|just\s+)?[a-z]+ing\b"

BACKTICK_RE = re.compile(r"`([^`\n]{1,300})`")
SHA_RE = re.compile(r"^[0-9a-f]{7,40}$")
ABS_PATH_RE = re.compile(
    r"(?<![A-Za-z0-9_./-])((?:~|/(?:Users|home|opt|srv|var|etc))/[A-Za-z0-9._/-]+)"
)
LINE_SUFFIX = re.compile(r":\d+(?::\d+)?$")
GLOB_CHARS = set("*?[]{}$()|<>\"'\\ \t")

MAX_TOKENS = 40  # per class, per turn -- a cap so a pathological message cannot
                 # turn turn-end into a git benchmark


def _backticked(text):
    return [m.group(1).strip() for m in BACKTICK_RE.finditer(text)]


def agent_names(text):
    return sorted({m.group(0) for m in AGENT_RE.finditer(text)})[:MAX_TOKENS]


def roster_roles(entity_root, teams_dir, session_id):
    """Every role this installation actually has. Derived from two surfaces."""
    roles = set()
    d = os.path.join(entity_root, ".claude", "agents")
    try:
        for f in os.listdir(d):
            if f.endswith(".md"):
                stem = f[:-3]
                if re.fullmatch(r"[a-z][a-z0-9]{1,15}", stem):
                    roles.add(stem)
    except OSError:
        pass
    for m in _roster_members(teams_dir, session_id):
        t = str(m.get("agentType") or "")
        if re.fullmatch(r"[a-z][a-z0-9]{1,15}", t):
            roles.add(t)
    roles.discard("team")
    return roles


def role_claims(text, roles):
    """[(role, span)] -- bare-role in-flight claims in this message."""
    if not roles:
        return []
    alt = "|".join(sorted(re.escape(r.capitalize()) for r in roles))
    rx = re.compile(r"(?<![A-Za-z0-9_-])(%s)(?![A-Za-z0-9_-])\s+%s" % (alt, PROGRESSIVE))
    out, seen = [], set()
    for m in rx.finditer(text):
        role = m.group(1).lower()
        if role in seen:
            continue
        seen.add(role)
        out.append((role, m.group(0)))
    return out[:MAX_TOKENS]


def sha_claims(text):
    out = set()
    for tok in _backticked(text):
        if not SHA_RE.match(tok):
            continue
        # Both a digit and a letter. Kills pure-decimal ("1234567") and the
        # handful of all-hex English words ("deadbeef", "facade").
        if not (any(c.isdigit() for c in tok) and any(c.isalpha() for c in tok)):
            continue
        out.add(tok)
    return sorted(out)[:MAX_TOKENS]


# --------------------------------------------------------------------------
# STATE CLAIMS -- "it landed", "it's pushed" -- and the SHA that carries them
# --------------------------------------------------------------------------
#
# WHAT THIS ADDS THAT sha_claims ABOVE DOES NOT
#   sha_claims asks whether a commit EXISTS. That is what caught the fabricated
#   hash on 2026-09-01 -- an invented 40-hex that names no object anywhere. It
#   is NOT what catches the other failure of the same night: a real commit,
#   sitting on a teammate branch, reported to the CEO as landed on main. The
#   merge had been chained behind another command in one Bash call, a PreToolUse
#   guard refused the whole call, so the merge never executed -- and the report
#   was written from the INTENT of the command rather than from the repository.
#   Every identifier in it resolved. The CEO found out by opening the file and
#   getting ERR_FILE_NOT_FOUND.
#
#   So the predicate here is not existence but POSITION: a commit claimed to be
#   integrated must be an ancestor of an integration branch, and a commit
#   claimed to be published must be reachable from a remote ref.
#
# WHY THIS ONE CAN BLOCK WHEN THE PROSE SIGNALS BELOW CANNOT
#   It is not reading English for intent. It reads English only to decide WHICH
#   QUESTION TO ASK OF GIT, and git answers. The sentence has to contain a
#   landing word AND a hex token; everything after that is `merge-base
#   --is-ancestor`, which has no threshold and nothing to interpret.
#
# THE ONE RELAXATION THAT MAKES IT SAFE -- reachability, not ancestry alone
#   A history rewrite leaves the old commits in the object DB, reachable from
#   nothing. Reporting "X landed at `abc123`" about a pre-rewrite commit is
#   honest and forever unprovable, and a naive ancestry test flags every one of
#   them. Measured: 41 fires across the corpus, ALL of them casualties of one
#   repository's rewrite, none of them a false report.
#
#   So a commit reachable from NO REF AT ALL is SILENT. The gate fires only when
#   the commit is alive on some branch and that branch is not the integration
#   branch -- which is exactly the shape of the 2026-09-01 failure, and is a
#   shape a rewrite cannot produce. That single condition took the corpus from
#   41 fires to 0.
#
# MEASURED -- 2,276 turns / 2,206 final messages, 79 transcripts, every
# non-scratchpad orchestrator project on this machine. Numbers and method:
# scripts/hooks/state-claims.corpus.md.
#   landing/push claim sentences carrying a SHA      573
#   SHA citations inside them                        658
#   fires against the repositories as they stand       0
#   fires when each turn is replayed against the
#     repository AS IT STOOD at that turn's clock      1   (0.15% of citations,
#                                                           0.045% of turns)
# The one is a claim made 19 minutes before its commit reached master; by this
# engine's own doctrine that claim was premature rather than true, and it is
# still counted as a false positive because counting it the other way would be
# grading my own work.

STATE_INTEGRATED = re.compile(
    r"\b(landed|merged|is\s+(?:now\s+)?(?:in|on)\s+main|in\s+main|on\s+main)\b",
    re.I)
STATE_PUBLISHED = re.compile(r"\bpushed\b", re.I)
SENTENCE_SPLIT = re.compile(r"(?<=[.!?;])\s+|\n+")
BARE_HEX_RE = re.compile(r"(?<![0-9A-Za-z_-])([0-9a-f]{7,40})(?![0-9A-Za-z_-])")

MAX_STATE_CLAIMS = 12  # this class costs git calls; the cap is a time budget


def _hex_tokens(sentence):
    """Every SHA-shaped token in one sentence, backticked or bare.

    Bare tokens are admitted HERE and nowhere else, because the surrounding
    sentence has already asserted a landing -- the context that makes a raw hex
    string a commit citation rather than a coincidence. The digit-and-letter
    rule is the same one sha_claims uses, for the same reason.
    """
    out, seen = [], set()
    for tok in _backticked(sentence) + [m.group(1) for m in
                                        BARE_HEX_RE.finditer(sentence)]:
        tok = tok.strip()
        if not SHA_RE.match(tok):
            continue
        if not (any(c.isdigit() for c in tok) and any(c.isalpha() for c in tok)):
            continue
        if tok in seen:
            continue
        seen.add(tok)
        out.append(tok)
    return out


# --------------------------------------------------------------------------
# POLARITY -- because a guard that cannot parse "not" cannot judge a claim
# --------------------------------------------------------------------------
#
# ON 2026-09-03 THIS GATE BLOCKED A TRUE STATEMENT, which is notable because
# catching false ones is its entire job. The reply said a commit was NOT on the
# remote; the repository agreed exactly; the gate read the word `pushed`,
# ignored the negation, called it a contradiction and stopped the turn. It fired
# the same way on 2026-09-05 on "committed on the branch, not landed, main is
# still at <sha>" -- a sentence whose whole content is that the commit had not
# landed.
#
# The extractor asks git ONE question -- "you said landed, so is this an
# ancestor of main?" -- and that question is only the right one if the sentence
# ASSERTED the landing. When the sentence asserts the opposite, the same git
# answer means the claim was TRUE, and blocking on it is a guard punishing
# accuracy.
#
# THE RULE IS DELIBERATELY LOCAL, AND THAT IS THE WHOLE SAFETY ARGUMENT.
# Polarity is decided from the CLAUSE the state word sits in -- never from the
# sentence, never from the paragraph -- and a negation cue only counts when it
# sits BEFORE the state word in that clause. So a sentence with no negation cue
# in the governing clause behaves exactly as it did before this existed: same
# extraction, same question, same refusal. Nothing gets quieter by accident.
#
# MEASURED over every non-scratchpad orchestrator transcript on this machine
# (docs/verification/claim-polarity-2026-09-05/), 960 state claims in 770
# messages:
#
#     949  positive    -> unchanged: the repository is asked, and a
#                        contradiction still BLOCKS
#      11  negated     -> report-only, and all 11 were read by hand: every one
#                        is a genuine statement of absence ("not pushed",
#                        "nothing landed", "haven't landed it yet", "never be
#                        pushed", "hadn't merged")
#       0  unreadable
#
# THE FIRST DRAFT WAS WRONG IN THE DANGEROUS DIRECTION and the measurement is
# what caught it. It called polarity unreadable whenever a negation cue appeared
# ANYWHERE in the sentence, which demoted 113 perfectly positive claims to
# report-only -- every "landed, no worktrees, no live agents" status line Rich
# writes. A rule that quiet would have been indistinguishable from switching the
# gate off, and it looked entirely reasonable until it was counted.
#
# `not only` / `not just` / `not merely` are emphatic POSITIVES and are masked
# before the cue search, or "not only landed but pushed" would read as a denial.
NEG_CUE = re.compile(
    r"\b(?:not|never|no|nor|neither|nothing|none|without|cannot)\b|n[\u2019']t\b",
    re.I)
NEG_EMPHATIC = re.compile(r"\bnot\s+(?:only|just|merely|simply)\b", re.I)
# Clause boundaries. Sentence terminators and colons are included because
# SENTENCE_SPLIT requires whitespace after the terminator and markdown does not
# supply it -- "...never the model.** Landed `228ccf5`" is one "sentence" to the
# splitter, and without this boundary its `never` would govern a landing three
# words later. That single addition moved four of fifteen readings from wrong to
# right when it was measured.
CLAUSE_SPLIT = re.compile(
    r"[.!?:,]|\s+--\s+|\s+\u2014\s+|\bbut\b|\band\b|\bwhile\b|\bthough\b"
    r"|\balthough\b|\bbecause\b|\bso\b", re.I)


def claim_polarity(sentence, pos):
    """'positive' | 'negated' | 'unreadable' for the state word at <pos>."""
    start = 0
    end = len(sentence)
    for m in CLAUSE_SPLIT.finditer(sentence):
        if m.end() <= pos:
            start = m.end()
        elif m.start() >= pos:
            end = m.start()
            break
    clause = sentence[start:end]
    rel = pos - start
    if NEG_CUE.search(NEG_EMPHATIC.sub(" ", clause[:rel])):
        return "negated"
    # A cue AFTER the state word in the same clause -- "on main not staging" --
    # has a scope this cannot decide. Report, never block: the fail-open-loud
    # shape the rest of the engine uses for an undecidable premise.
    if NEG_CUE.search(NEG_EMPHATIC.sub(" ", clause[rel:])):
        return "unreadable"
    return "positive"


def state_claims(text):
    """[(kind, sha, sentence, polarity)]; kind is 'integrated' or 'published'."""
    out, seen = [], set()
    for s in SENTENCE_SPLIT.split(text):
        s = s.strip()
        if not s:
            continue
        mi = STATE_INTEGRATED.search(s)
        mp = STATE_PUBLISHED.search(s)
        if not (mi or mp):
            continue
        for tok in _hex_tokens(s):
            # "landed" wins when a sentence says both, because it is the
            # stronger claim and the one that was false on 2026-09-01.
            kind = "integrated" if mi else "published"
            if (kind, tok) in seen:
                continue
            seen.add((kind, tok))
            out.append((kind, tok, s[:400],
                        claim_polarity(s, (mi or mp).start())))
            if len(out) >= MAX_STATE_CLAIMS:
                return out
    return out


def path_claims(text):
    cands = list(_backticked(text))
    cands += [m.group(1) for m in ABS_PATH_RE.finditer(text)]
    out = set()
    for tok in cands:
        tok = LINE_SUFFIX.sub("", tok.strip().rstrip(",.;:"))
        if "/" not in tok or len(tok) < 3:
            continue
        if any(c in GLOB_CHARS for c in tok):
            continue
        if "://" in tok or tok.startswith("//"):
            continue
        segs = [s for s in tok.split("/") if s]
        if not segs:
            continue
        # A purely numeric segment means this is a ratio or a label, not a
        # path: "16/16", "C_turbo_rep1/2/3".
        if any(s.isdigit() for s in segs):
            continue
        last = segs[-1]
        ext = last.rsplit(".", 1)[1] if ("." in last and not last.startswith(".")) else ""
        if not (tok.startswith("/") or tok.startswith("~") or tok.endswith("/")
                or (ext and len(ext) <= 5)):
            continue
        out.add(tok)
    return sorted(out)[:MAX_TOKENS]


# --------------------------------------------------------------------------
# ground truth
# --------------------------------------------------------------------------

def name_history(teams_dir, session_id=None):
    """Union of every present session team dir: ledger + roster + event logs.

    Deliberately unioned across ALL sessions rather than scoped to this one.
    A wider set can only make the gate quieter, and the orchestrator does refer
    to teammates from earlier sessions by their exact identifier.

    With `session_id`, only THIS session's team dir is read: the bare-role
    "never dispatched in this session" test must not be cleared by a role
    spawned in another session (hunt part 3, finding 21).
    """
    hist = set()
    if not os.path.isdir(teams_dir):
        return hist
    try:
        entries = os.listdir(teams_dir)
    except OSError:
        return hist
    for d in entries:
        if session_id and d != "session-" + session_id[:8]:
            continue
        td = os.path.join(teams_dir, d)
        if not os.path.isdir(td):
            continue
        try:
            with open(os.path.join(td, "spawned-names.log"), encoding="utf-8") as f:
                hist.update(x.strip() for x in f if x.strip())
        except Exception:
            pass
        try:
            with open(os.path.join(td, "config.json"), encoding="utf-8") as f:
                for m in (json.load(f).get("members") or []):
                    if isinstance(m, dict) and m.get("name"):
                        hist.add(m["name"])
        except Exception:
            pass
        for logname in ("idle-events.jsonl", "task-events.jsonl"):
            try:
                with open(os.path.join(td, logname), encoding="utf-8") as f:
                    for line in f:
                        try:
                            t = json.loads(line).get("teammate")
                        except Exception:
                            continue
                        if isinstance(t, str) and t:
                            hist.add(t)
            except Exception:
                pass
    hist.discard("team-lead")
    return hist


TERMINAL_STATUS = {
    "shutdown", "shutdown_request", "shutdown_approved", "completed",
    "complete", "done", "terminated", "dead", "exited", "killed", "removed",
    "gone",
}


def _roster_members(teams_dir, session_id):
    if not session_id:
        return []
    p = os.path.join(teams_dir, "session-" + session_id[:8], "config.json")
    try:
        with open(p, encoding="utf-8") as f:
            return json.load(f).get("members") or []
    except Exception:
        return []


def live_roles(teams_dir, session_id):
    """Roles with a teammate running RIGHT NOW, or None if unknowable.

    NOT MONOTONIC, and that is why nothing blocks on it. A teammate completes,
    and the set shrinks; the existing agent-name check blocks precisely because
    its ground truth only ever grows. This answers the question "is building it
    NOW" actually asks, and it is REPORTED rather than enforced until its false
    -positive rate has been measured on real traffic -- which it cannot be
    today, because past rosters are not retained.
    """
    members = _roster_members(teams_dir, session_id)
    if not members:
        return None
    live = set()
    for m in members:
        name = str(m.get("name") or "")
        role = str(m.get("agentType") or "") or name.split("-")[0]
        if not role or role == "team-lead" or name == "team-lead":
            continue
        cwd = str(m.get("cwd") or "")
        if "/.claude/worktrees/" in cwd:
            if os.path.isdir(cwd):
                live.add(role)
            continue
        if str(m.get("status") or "").lower() in TERMINAL_STATUS:
            continue
        live.add(role)
    return live


def repo_roots(entity_root, extra):
    """Repositories whose object DBs a SHA may legitimately live in.

    The entity itself, its siblings (an orchestrator routinely cites commits in
    a neighboring checkout), and anything the entity's config names. Bounded
    and cheap; a wider set only makes the gate quieter.
    """
    roots, seen = [], set()

    def add(p):
        if p and os.path.isdir(p) and p not in seen:
            if os.path.exists(os.path.join(p, ".git")):
                seen.add(p)
                roots.append(p)

    add(entity_root)
    parent = os.path.dirname(os.path.abspath(entity_root))
    try:
        for d in sorted(os.listdir(parent))[:60]:
            add(os.path.join(parent, d))
    except OSError:
        pass
    for p in (extra or "").split():
        add(os.path.expanduser(p))
    return roots


def resolve_shas(shas, roots):
    """Return the subset that names a real object in at least one repository."""
    found = set()
    for root in roots:
        todo = [s for s in shas if s not in found]
        if not todo:
            break
        try:
            p = subprocess.run(
                ["git", "-C", root, "cat-file", "--batch-check"],
                # Each token PEELED TO A COMMIT: a commit, or a tag that
                # points (through any chain of tags) at one. A blob, a tree,
                # or a tag pointing at a blob is not a commit (hunt part 3,
                # finding 20; the tag case is the v3 re-check).
                input="".join(t + "^{commit}\n" for t in todo),
                capture_output=True, text=True, timeout=10,
            )
        except Exception:
            continue  # fail OPEN: a resolver error never invents a violation
        # --batch-check emits one line per input, in order. A missing object
        # echoes the INPUT token; a resolved one echoes the FULL oid, so pair
        # by position rather than by the first column.
        for tok, line in zip(todo, p.stdout.splitlines()):
            parts = line.split()
            # An ambiguous short SHA names real objects; it is not invented.
            if (len(parts) > 1 and parts[1] == "commit") or "ambiguous" in parts:
                found.add(tok)
    return found


# --------------------------------------------------------------------------
# ground truth for STATE CLAIMS -- position, not existence
# --------------------------------------------------------------------------

# HEAD is in this list DELIBERATELY, and it loosens the check: in a checkout
# sitting on a feature branch, a commit on that branch reads as integrated. That
# is the QUIET direction, which is the right one for a blocking gate -- it costs
# recall in a checkout that is not on its integration branch, and it never
# invents a refusal in one that is. Rich lands from the main checkout, where
# HEAD is main; on 2026-09-01 richos-hq's HEAD was main and the gate fires.
INTEGRATION_REFS = ("main", "master", "origin/main", "origin/master", "HEAD")


def _recorded_integration_refs(root):
    """THE ONE ANSWER FIRST, asked of mega-lander/workspaces.py (point 14):
    "Every part of the system that needs to know whether work has landed asks
    the same question: is it in the branch recorded for this work? None of them
    is allowed its own answer, and none of them assumes main."

    The tuple above is this gate's own answer, and on a repository whose work
    integrates on a dev branch it is the WRONG one in the loud direction: a
    commit merged onto that dev branch is not in `main`, so a truthful
    "integrated" claim was refused. The recorded branch goes first.

    Where nothing is recorded this returns (), and the caller then ABSTAINS
    into "unknown" -- this gate's own silent verdict -- rather than falling back
    to main. Loading the library by path is a duplicated LOADER, not a
    duplicated ANSWER."""
    lib = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                       "..", "mega-lander", "workspaces.py")
    try:
        import importlib.util
        spec = importlib.util.spec_from_file_location("workspaces_answer", lib)
        ws = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ws)
        branch, _tip, why_not = ws.integration_for(root)
    except Exception:
        return ()
    if why_not or not branch:
        return ()
    remote = "origin/" + branch
    return (branch, remote)

# A wall-clock budget for the whole class. This gate runs at Stop with a 20s
# hook timeout, and `for-each-ref --contains` over a big repository is not free.
# When the budget is spent the remaining claims resolve to "unknown", which is
# the SILENT verdict -- running out of time can never manufacture a refusal.
STATE_BUDGET_SECONDS = 8.0


def _git(root, args, timeout=10):
    try:
        return subprocess.run(["git", "-C", root] + args,
                              capture_output=True, text=True, timeout=timeout)
    except Exception:
        return None  # fail OPEN, everywhere in this section


def _reachable_from(root, sha, prefix):
    """First ref under `prefix` that contains sha, or ''. """
    # 6s, not more: the budget below is checked BEFORE a call, so the hook's
    # worst case is the budget plus one call, and the registered Stop timeout is
    # 20s. A guard that overruns its own hook timeout is a guard that does not
    # run.
    p = _git(root, ["for-each-ref", "--contains", sha,
                    "--format=%(refname:short)", prefix], timeout=6)
    if p is None or p.returncode != 0:
        return ""
    for line in p.stdout.splitlines():
        line = line.strip()
        if line:
            return line
    return ""


def state_verdict(kind, sha, roots, deadline):
    """('ok'|'unknown', ...) or ('violation', repo, ref) -- see the header.

    integrated: ok when sha is an ancestor of an integration ref anywhere.
                violation when it is alive on some branch and on no
                integration branch.
    published:  ok when sha is reachable from any remote-tracking ref.
                violation when it is reachable only from a local branch.

    Everything else -- object absent, object dangling, git unreadable, budget
    spent -- is 'unknown' and says nothing. An unresolvable SHA is already the
    business of the existence check above; a dangling one is the history-rewrite
    case the header explains.
    """
    held_by = []
    for root in roots:
        if time.monotonic() > deadline:
            return ("unknown",)
        p = _git(root, ["cat-file", "-t", sha], timeout=5)
        if p is None or p.returncode != 0 or p.stdout.strip() != "commit":
            continue
        held_by.append(root)
        if kind == "integrated":
            recorded = _recorded_integration_refs(root)
            if not recorded:
                # ABSTAIN: no branch is recorded as the one this work
                # integrates on, so "integrated" has nothing to be measured
                # against. "unknown" is this gate's silent verdict, so the
                # abstention costs recall and never manufactures a refusal --
                # which is the only direction an unanswerable question may be
                # rounded in.
                return ("unknown",)
            for ref in recorded:
                if time.monotonic() > deadline:
                    return ("unknown",)
                q = _git(root, ["merge-base", "--is-ancestor", sha, ref],
                         timeout=8)
                if q is None:
                    return ("unknown",)
                if q.returncode == 0:
                    return ("ok",)
        else:
            if time.monotonic() > deadline:
                return ("unknown",)
            if _reachable_from(root, sha, "refs/remotes/"):
                return ("ok",)
    # It is nowhere, so the existence check owns it.
    if not held_by:
        return ("unknown",)
    # It exists and is not where the claim says. Say so ONLY if it is alive on
    # a ref -- a commit reachable from nothing is a rewrite casualty and this
    # gate has nothing truthful to say about it.
    for root in held_by:
        if time.monotonic() > deadline:
            return ("unknown",)
        ref = _reachable_from(root, sha, "refs/heads/")
        if not ref and kind == "integrated":
            ref = _reachable_from(root, sha, "refs/remotes/")
        if ref:
            return ("violation", root, ref)
    return ("unknown",)


def resolve_path(tok, bases):
    if tok.startswith("~"):
        return os.path.exists(os.path.expanduser(tok))
    if tok.startswith("/"):
        return os.path.exists(tok)
    for base in bases:
        if os.path.exists(os.path.join(base, tok)):
            return True
        if os.path.exists(os.path.join(base, "engine", tok)):
            return True
        if os.path.exists(os.path.join(base, "richos", "engine", tok)):
            return True
    return False


# --------------------------------------------------------------------------
# the turn's own tool traffic (grounding)
# --------------------------------------------------------------------------

def _flatten(content):
    if isinstance(content, str):
        return content
    parts = []
    if isinstance(content, list):
        for b in content:
            if not isinstance(b, dict):
                continue
            if b.get("type") == "text":
                parts.append(b.get("text", ""))
            elif b.get("type") == "tool_use":
                parts.append(json.dumps(b.get("input", {})))
            elif b.get("type") == "tool_result":
                parts.append(_flatten(b.get("content")))
    return "\n".join(parts)


class Observed:
    """Which of a fixed set of strings the session's records ever contained.

    Stands in for the whole-session text blob this module used to build: the
    only thing ever asked of that blob was `needle in blob`, so the set of
    needles is registered first and one streaming pass records the hits.
    """

    def __init__(self, needles=()):
        self._pending = {n for n in needles if n}
        self._hit = set()

    def feed(self, text):
        if not self._pending:
            return
        for n in [n for n in self._pending if n in text]:
            self._pending.discard(n)
            self._hit.add(n)

    def __contains__(self, needle):
        if not needle:
            return False
        if needle in self._hit:
            return True
        if needle in self._pending:
            return False
        raise KeyError("needle was not registered before the transcript pass: %r" % needle)


def read_transcript(path, prompt_id=None, needles=()):
    """Session-wide tool traffic, plus the tool-call names of THIS TURN ONLY.

    The two scopes are deliberately different and conflating them is a bug that
    silently disarms the reporting layer. GROUNDING wants everything the session
    ever observed, because a citation recalled from an earlier turn is still
    grounded. The "did this turn actually call Agent?" question wants only this
    turn, because the orchestrator spawns agents constantly and a session-wide
    answer is "yes" forever after the first spawn.

    The turn is scoped by promptId, using the binary's own semantic: "UUID
    correlating a user prompt with all subsequent events until the next prompt."
    Assistant records carry no promptId, so the turn is the file-order span from
    the first record bearing this prompt_id to the end of the file (at Stop time
    there is no next prompt yet).

    Absent a prompt_id the turn cannot be scoped, so the window opens at the
    first record and every tool call in the session counts as this turn's. That
    is the QUIET direction: the only consumer is "did this turn call Agent?",
    and answering yes suppresses a report. Scoping to nothing would answer no
    and manufacture one. When a reporting layer cannot tell, it says nothing.

    At Stop time the transcript already holds every tool_use and tool_result of
    the turn; it does NOT yet hold the final assistant text (verified against
    2.1.251 -- the last assistant record at Stop is the tool_use). That is why
    the message itself comes from last_assistant_message and only the tool
    traffic comes from here.

    NO SIZE CAP (2026-10-10): the file is streamed one line at a time and never
    held whole. Because grounding is a yes/no question about a few known
    strings, the caller names those strings (`needles`) up front and the single
    pass records which of them any record contains. The answer comes back as an
    Observed, whose `in` operator is that record. A needle that was not named
    raises KeyError rather than answering, so a forgotten one fails loudly in
    the test suite and not silently in a session.
    """
    seen = Observed(needles)
    calls, refused = [], set()
    # No prompt_id -> no turn boundary -> count everything (see docstring:
    # the wide answer is the quiet one for the only question asked of it).
    turn_started = prompt_id is None
    if not path or not os.path.isfile(path):
        return seen, []
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    rec = json.loads(line)
                except Exception:
                    continue
                if prompt_id and rec.get("promptId") == prompt_id:
                    turn_started = True
                msg = rec.get("message") or {}
                content = msg.get("content")
                if isinstance(content, list):
                    for b in content:
                        if isinstance(b, dict) and b.get("type") == "tool_use":
                            if turn_started:
                                calls.append((b.get("id"), b.get("name", "")))
                        elif (isinstance(b, dict) and b.get("type") == "tool_result"
                              and b.get("is_error") is True and b.get("tool_use_id")):
                            refused.add(b["tool_use_id"])
                flat = _flatten(content)
                if flat:
                    seen.feed(flat)
    except OSError:
        return Observed(needles), []
    # A call whose result is an error was refused or failed: a refused Agent
    # dispatched nobody, so it is not this turn's tool (hunt part 3 v3, 19).
    tools = [name for cid, name in calls if not (cid and cid in refused)]
    return seen, tools


# --------------------------------------------------------------------------
# reporting layer -- never blocks
# --------------------------------------------------------------------------

# --- the third failure of 2026-09-01, and why it is a REPORT ---------------
#
# "`#8F7030` is gone from the entire app" was written after replacing that one
# spelling. The decimal `rgb(143,112,48)` -- the same value -- was still sitting
# in a test. The claim was about a VALUE and the check had been about a STRING.
#
# THE OBVIOUS GATE WAS BUILT AND MEASURED AND IT CRIES WOLF. Take any removal
# sentence, take the token it cites, grep for it: across the corpus that fires
# on 95 of 109 literals (87%). Every kind of false positive is there -- the doc
# that records the removal, the guard that now bans the token, the claim scoped
# to one directory grepped repo-wide, the token that is an ordinary word.
#
# SHARPENING IT TO "A SPELLING THE CLAIM DID NOT NAME" LOOKED CLEAN AND WAS NOT.
# On the two value-absence sentences in the whole corpus it fired once, on the
# real failure, and stayed silent on the other. That reads like a perfect gate
# until you check the hit: the surviving `rgb(143,112,48)` it found is in
# femcboost's CLAUDE.md, which DOCUMENTS THIS VERY INCIDENT. The one true
# positive was matched against a record written after the fact -- circular, and
# worth nothing as evidence.
#
# The number that decides it is the risky half measured on its own. Across all
# 41 value citations in the corpus, an alternate spelling survives somewhere for
# 22 of them (54%). So this predicate is held back from firing by nothing but
# the rarity of an English absence word next to a value -- and "no longer",
# "removed" and "gone" sit next to color values in design talk constantly.
# Blocking on that is a wolf-crier waiting for its first design round.
#
# So it REPORTS, and the report is still worth having: it names the surviving
# spelling and the file, which is the one fact the 2026-09-01 turn did not have.
# The numbers are here so nobody promotes it later without re-measuring.

ABSENCE_RE = re.compile(
    r"\b(removed|gone|deleted|eliminated|zero\s+references|"
    r"no\s+(?:more\s+)?references|no\s+longer|nothing\s+left|nowhere)\b", re.I)
HEX_VALUE_RE = re.compile(r"#([0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b")
MAX_ABSENCE_VALUES = 4


def _value_spellings(hexv):
    """Every way the same value can be written. Deterministic, not heuristic."""
    h = hexv.lower()
    if len(h) == 3:
        h = "".join(c * 2 for c in h)
    r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
    out = ["#" + h, "#" + h.upper()]
    for sep in (", ", ",", " "):
        out.append("rgb(%d%s%d%s%d)" % (r, sep, g, sep, b))
        out.append("rgba(%d%s%d%s%d" % (r, sep, g, sep, b))
    return out


def value_absence_claims(text):
    """[(hexvalue, [spellings the message did NOT name])]"""
    out, seen = [], set()
    low = text.lower()
    for s in SENTENCE_SPLIT.split(text):
        s = s.strip()
        if not s or not ABSENCE_RE.search(s):
            continue
        for m in HEX_VALUE_RE.finditer(s):
            v = m.group(1)
            if v.lower() in seen:
                continue
            seen.add(v.lower())
            alts = [sp for sp in _value_spellings(v) if sp.lower() not in low]
            if alts:
                out.append((v, alts))
            if len(out) >= MAX_ABSENCE_VALUES:
                return out
    return out


def surviving_spellings(claims, root, deadline):
    """[(value, spelling, file)] -- an unnamed spelling still in the tree."""
    found = []
    for v, alts in claims:
        for sp in alts:
            if time.monotonic() > deadline:
                return found
            p = _git(root, ["grep", "-I", "-F", "-l", "--", sp], timeout=5)
            if p is None:
                return found  # fail OPEN
            if p.returncode == 0 and p.stdout.strip():
                found.append((v, sp, p.stdout.splitlines()[0].strip()))
                break
    return found


INFLIGHT_RE = re.compile(r"\b(dispatching|spawning|kicking off|launching)\b", re.I)


def prose_signal(message, turn_tools):
    """An in-flight dispatch claim in a turn with no Agent call.

    MEASURED AT 17% PRECISION on 418 real turns (1 true, 5 false). It is here
    to be LOGGED, never to block, and this docstring exists so nobody promotes
    it later without re-measuring. Every false positive was the orchestrator
    describing a past action, a negated action, or a past incident.
    """
    if not INFLIGHT_RE.search(message):
        return None
    if "Agent" in turn_tools:
        return None
    for s in re.split(r"(?<=[.!?])\s+", message):
        if INFLIGHT_RE.search(s):
            return s.strip()[:300]
    return None


def nameless_dispatch(message, prose):
    """The prose signal fired and the message named no agent identifier.

    MEASURED AND REJECTED AS A GATE. Over 4,134 real turns (3,532 with a final
    message) the prose signal fired 30 times and 29 of those named nobody, so
    this refinement removes ONE HIT IN THIRTY -- it is not a filter. Hand-
    adjudicating the 29: 3 true, 1 borderline, 25 false, i.e. 10.3% precision
    against the 17% it was proposed to replace.

    The reason is structural rather than tunable: the prose signal's false
    positives are negations ("I'm not dispatching anything"), quotations of
    this very header, past tense ("spawning the duplicate at all") and
    hypotheticals -- and NONE OF THOSE NAME AN AGENT EITHER. "Names nobody" and
    "is a false alarm" are very nearly the same set.

    Kept as a report, with the number in this docstring, so nobody promotes it
    later without re-measuring.
    """
    return bool(prose and not AGENT_RE.search(message))


# --- the 2026-09-06 failure: "clean and pushed" over six unlanded branches ---
#
# A session ended reporting "Everything is clean and pushed" while SIX finished
# branches sat outside main, one of them the fix for the CEO's own complaint
# that the app steals keyboard focus. Both checks it ran were green and both
# were CORRECT -- the working tree was clean, and main did match origin/main.
#
#     NEITHER OF THOSE TWO FACTS CAN SEE AN UNLANDED BRANCH, AND BETWEEN THEM
#     THEY ARE THE ENTIRE VOCABULARY OF THE WORD "CLEAN".
#
# This is the SHA-FREE sibling of the state claims above. Those need a hex token
# to have anything to ask git about; the sentence that did the damage cites no
# commit at all, which is precisely why nothing caught it. So the question here
# is asked of the REPOSITORY SET rather than of a citation: is any branch
# holding work main does not have, with nothing live standing over it?
#
# IT REPORTS AND IT MUST NEVER BLOCK, and that is not a preference. This guard
# has three recorded false-positive incidents -- g11, g12, g13 in the CEO's
# backlog -- and two of them are the same defect twice: it blocked a TRUE
# statement because it could not read a negation, and it blocked a commit whose
# MESSAGE merely described a forbidden command. A blocking guard with a
# false-positive class gets waived on the day, and habitual waiving is how a
# defense decays into a formality. Blocking is a decision for later, backed by a
# measured false-positive rate that does not exist yet.
#
# MEASURED -- 161 non-scratchpad transcripts on this machine, 5,683 assistant
# messages, 2026-09-06:
#
#   first draft (scope word + landing word in one sentence)
#       202 messages (3.55%), and 45 sampled and hand-read: 31 true / 14 false
#       = 68.9% precision. The false ones were nearly all one of two genres --
#       the orchestrator QUOTING this failure ("the same kind of claim as
#       'clean and pushed'"), and the contrastive idiom "everything else".
#
#   shipped (quotation spans masked, `everything else` excluded)
#       171 messages (3.01%), 45 sampled and hand-read: 37 true / 8 false
#       = 82.2% precision.
#
# Masking quotations is what moved it, and it moved it 13 points. The remaining
# false ones are future conditionals ("once everything is landed"), rules being
# described rather than states being asserted, and one "everything from Clark's
# landed brief" where `landed` modifies a noun.
#
# 82% is the EXTRACTION rate, not the report rate. A report needs the claim AND
# a branch actually holding unlanded work with nothing live on it, so the
# composite is far rarer -- and on the day it fires, it fires on the sentence
# that was about to be sent.
COMPLETE_QUOTED = re.compile(r"`[^`]*`|“[^”]*”|‘[^’\n]{0,300}’|\"[^\"]{0,300}\"")
# A markdown blockquote is somebody else's words, whole lines of them, so it is
# dropped before the sentences are split rather than masked inside one.
COMPLETE_BLOCKQUOTE = re.compile(r"(?m)^[ \t]*>.*$")
# A CONDITION OR A FORECAST IS NOT A REPORT (2026-10-25). "Nightly 75 builds
# once all the work you named has landed." was read as positive, exactly like
# "All the work has landed.", and the operator was told YOU CALLED IT DONE every
# turn it was repeated. The header above already named future conditionals as
# the residual false class; this is that class, read the way polarity is: from
# the CLAUSE the completion word sits in, and only from a cue BEFORE it. So a
# claim with no such cue in its own clause reads exactly as it did before, and
# "everything landed once the merge finished" (cue after the word) still counts.
COMPLETE_CONDITION = re.compile(
    r"\b(?:if|once|when|whenever|until|unless|after|before|as\s+soon\s+as"
    r"|provided|will|shall|would|going\s+to|about\s+to)\b|[’']ll\b", re.I)
# "once again", "at once": the adverb, not the conjunction.
COMPLETE_NOT_CONDITION = re.compile(r"\bonce\s+(?:again|more)\b|\bat\s+once\b", re.I)
COMPLETE_SCOPE = re.compile(
    r"\b(?:"
    r"everything(?!\s+else)"
    r"|every(?:\s+\w+){0,2}\s+branch(?:es)?"
    r"|all\s+(?:of\s+it|the\s+work|work|branches|the\s+branches)"
    r"|all\s+(?:\w+\s+){0,2}repos?(?:itor(?:y|ies))?"
    r"|nothing\s+(?:left|outstanding|pending|remaining|unmerged|unlanded)"
    r"|no\s+(?:unmerged|unlanded|outstanding|open)\s+\w+"
    r"|(?:repos?|repositor(?:y|ies)|trees?|working\s+trees?)\s+(?:is|are)\s+clean"
    r"|clean\s+and\s+(?:pushed|landed|merged)"
    r")\b", re.I)
COMPLETE_STATE = re.compile(
    r"\b(?:clean|landed|merged|pushed|finished|done|completed?|shipped"
    r"|wrapped\s+up|up\s+to\s+date|in\s+sync)\b", re.I)
# The scope word and the completion word have to be in the same sentence AND
# near each other. Without the window, "all three repos" at the start of a long
# paragraph-sentence pairs with a `done` eighty words later that is about
# something else entirely.
COMPLETE_WINDOW = 120
MAX_COMPLETE_CLAIMS = 4


def conditional_completion(sentence, pos):
    """True when the completion word at <pos> sits in a clause that a condition
    or a forecast governs ("once all the work has landed", "will be merged").
    The clause is cut exactly as claim_polarity cuts it."""
    start, end = 0, len(sentence)
    for m in CLAUSE_SPLIT.finditer(sentence):
        if m.end() <= pos:
            start = m.end()
        elif m.start() >= pos:
            end = m.start()
            break
    before = COMPLETE_NOT_CONDITION.sub(" ", sentence[start:min(pos, end)])
    return bool(COMPLETE_CONDITION.search(before))


def completeness_claims(text):
    """[(sentence, polarity)] -- SHA-free assertions that the work is all in.

    Quotation spans are BLANKED before matching, not stripped, so every offset
    still lines up with the original sentence and `claim_polarity` reads the
    same clause a person would. That one change is worth 13 points of precision,
    because the orchestrator quotes this exact failure constantly.

    Polarity is claim_polarity's, plus 'conditional' for a positive reading
    that a condition or forecast governs (conditional_completion). Only
    'positive' is ever reported.
    """
    out = []
    text = COMPLETE_BLOCKQUOTE.sub("", text)
    for s in SENTENCE_SPLIT.split(text):
        s = s.strip()
        if not s or len(s) > 600:
            continue
        probe = COMPLETE_QUOTED.sub(lambda m: " " * (m.end() - m.start()), s)
        ms = COMPLETE_SCOPE.search(probe)
        if not ms:
            continue
        for mt in COMPLETE_STATE.finditer(probe):
            if abs(mt.start() - ms.start()) <= COMPLETE_WINDOW:
                pol = claim_polarity(probe, mt.start())
                if pol == "positive" and conditional_completion(probe, mt.start()):
                    pol = "conditional"
                out.append((s[:400], pol))
                break
        if len(out) >= MAX_COMPLETE_CLAIMS:
            break
    return out


def operator_line(text):
    """Put ONE line where the operator will actually read it.

    Every other report in this guard's observation block goes to stderr, and
    scripts/lib/stop-hook-notice.sh measured what that is worth on the Stop
    event: a hook's stderr AND its plain stdout are filed into the transcript
    as a `hook_success` attachment and shown to the operator NOWHERE. Only
    {"systemMessage": ...} on stdout reaches him.

    THIS ONE CLASS GETS THE OPERATOR CHANNEL AND THE OTHERS DO NOT, and the
    difference is the failure being caught. A path that did not resolve is a
    footnote in a report he is reading anyway. A sentence telling him the work
    is all in, WHILE it is not, is the report -- and by the time he could read
    a transcript attachment he has already believed it.

    It is NOT de-duplicated, deliberately, and the reasoning is the one
    scripts/lib/unevaluated-notice.sh sets out for the PreToolUse case: a
    stand-down is a persistent CONDITION and belongs on the state-change
    channel, but this is a property of ONE TURN -- this sentence, asserted now.
    Suppressing the second occurrence would not de-duplicate a condition, it
    would decline to mention the second false claim. The Stop notice
    (notice-unlanded-branches.sh) is the state-change half of the same fact and
    goes quiet after the first turn, which is why this half must not.
    """
    sys.stdout.write(json.dumps({"suppressOutput": True,
                                 "systemMessage": text}) + "\n")


def unlanded_sweep(entity_root, session_id):
    """(result, error) from scripts/lib/unlanded-branches.py, loaded by path.

    Loaded rather than duplicated: the Stop notice and this guard MUST agree
    about what counts as unlanded work, and two implementations of one predicate
    is how "it was quiet at the turn end" and "it is loud in the report" become
    two different answers.

    Returns (None, reason) on any failure. A reason is never swallowed -- the
    caller prints it, because a sub-check that did not run must not be reported
    in the same words as one that ran and found nothing.
    """
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                        "..", "lib", "unlanded-branches.py")
    if not os.path.isfile(path):
        return None, "scripts/lib/unlanded-branches.py is missing at %s" % path
    try:
        import importlib.util
        spec = importlib.util.spec_from_file_location("richos_unlanded", path)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        # Ledger path left to the module, which reads RICHOS_WORKTREE_LEDGER --
        # the same override every other reader of that file uses.
        return mod.sweep(entity_root, session_id, None,
                         os.environ.get("UNLANDED_BRANCHES_EXTRA_REPOS", "")), ""
    except Exception as exc:  # never wedge a turn over a sub-check
        return None, "the sweep raised %s: %s" % (type(exc).__name__, exc)


# --------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------

def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0  # unparseable payload never wedges turn-end

    if payload.get("stop_hook_active"):
        # Already blocked once this turn. The binary caps repeats
        # (CLAUDE_CODE_STOP_HOOK_BLOCK_CAP) but a gate that re-blocks its own
        # retry is a gate that can strand a session, so it stands down itself.
        return 0

    message = payload.get("last_assistant_message") or ""
    if not message.strip():
        return 0

    entity_root = os.environ.get("RICHOS_CLAIMS_ENTITY_ROOT") or payload.get("cwd") or os.getcwd()
    teams_dir = os.environ.get("RICHOS_CLAIMS_TEAMS_DIR") or os.path.expanduser("~/.claude/teams")
    extra_repos = os.environ.get("RICHOS_CLAIMS_EXTRA_REPOS", "")
    session_id = payload.get("session_id") or ""

    names = agent_names(message)
    shas = sha_claims(message)
    paths = path_claims(message)
    roles = roster_roles(entity_root, teams_dir, session_id)
    rclaims = role_claims(message, roles)
    sclaims = state_claims(message)

    # Every string the grounding checks below will ask about, named before the
    # one streaming pass over the transcript (see read_transcript).
    needles = set(names) | set(shas) | set(paths)
    needles |= {role + "-" for role, _span in rclaims}
    needles |= {os.path.basename(p.rstrip("/")) for p in paths}
    blob, turn_tools = read_transcript(payload.get("transcript_path"),
                                       payload.get("prompt_id"), needles)

    # --- BLOCKING: agent names ---------------------------------------------
    # Inert unless THIS session's team directory exists. Absent ground truth
    # cannot prove absence, and a guard that guesses is not a guard.
    unresolved_names = []
    names_evaluable = bool(session_id) and os.path.isdir(
        os.path.join(teams_dir, "session-" + session_id[:8])
    )
    hist = name_history(teams_dir) if names_evaluable else set()
    if names_evaluable and names:
        for n in names:
            if n not in hist and n not in blob:
                unresolved_names.append(n)

    # --- BLOCKING: a bare-role in-flight claim about a role NEVER DISPATCHED -
    # Same ground truth, same monotonicity, same zero-false-positive argument
    # as the name check above: the ledger only ever grows, so "no agent of this
    # role has ever been spawned in this session" cannot become false later.
    #
    # It is deliberately the NARROW half. Over 3,532 real messages it fires
    # ZERO times -- every bare-role claim in the corpus had a prior spawn of
    # that role -- so it ships costing nothing and guarding the case where a
    # role is named that was never dispatched at all. It does NOT catch the
    # 2026-08-31 failure, and that is stated rather than glossed: `Zach is
    # building it now` was false, but `zach-opus-uici1` had been spawned in
    # that same session hours earlier, so the monotonic test passes it. The
    # test that catches it is liveness, which is not monotonic and therefore
    # reports instead.
    #
    # Suppressed entirely when the message names an identifier (then the name
    # check above is the authority) or when this turn actually called Agent.
    undispatched_roles = []
    stale_roles = []
    if names_evaluable and rclaims and not names and "Agent" not in turn_tools:
        live = live_roles(teams_dir, session_id)
        here = name_history(teams_dir, session_id)
        for role, span in rclaims:
            ever = any(n.startswith(role + "-") for n in here) or (role + "-") in blob
            if not ever:
                undispatched_roles.append((role, span))
            elif live is not None and role not in live:
                stale_roles.append((role, span))

    # --- BLOCKING: commit SHAs ---------------------------------------------
    unresolved_shas = []
    if shas:
        roots = repo_roots(entity_root, extra_repos)
        if roots:
            live = resolve_shas(shas, roots)
            for s in shas:
                if s not in live and s not in blob:
                    unresolved_shas.append(s)

    # --- BLOCKING: state claims -- landed / merged / on main / pushed -------
    # DELIBERATELY NOT GROUNDED. Grounding is the right relaxation for an
    # EXISTENCE question, because to report a deletion honestly you must have
    # observed the thing. It is exactly wrong here: `git log` on the unmerged
    # branch prints the SHA, so the token is in this session's tool output in
    # the very case the gate exists to catch. Grounding this check would disarm
    # it against the 2026-09-01 failure and nothing else.
    #
    # POLARITY DECIDES WHICH ANSWER IS THE BAD ONE, and it is decided before
    # git is asked rather than after. A POSITIVE claim is contradicted by
    # "violation"; a NEGATED claim -- "not pushed", "nothing landed" -- is
    # contradicted by "ok", and that contradiction is REPORTED rather than
    # blocked, because understating what landed has never been the failure this
    # engine is protecting against. An UNREADABLE one never blocks at all.
    bad_states = []
    state_reports = []
    if sclaims:
        roots = repo_roots(entity_root, extra_repos)
        deadline = time.monotonic() + STATE_BUDGET_SECONDS
        for kind, sha, sentence, pol in sclaims:
            v = state_verdict(kind, sha, roots, deadline)
            if pol == "positive":
                if v[0] == "violation":
                    bad_states.append((kind, sha, sentence, v[1], v[2]))
            elif pol == "negated":
                if v[0] == "ok":
                    state_reports.append((kind, sha, sentence, "negated"))
            else:
                if v[0] == "violation":
                    state_reports.append((kind, sha, sentence, "unreadable"))

    # --- REPORTING: paths and prose ----------------------------------------
    bases = [entity_root] + [os.path.dirname(os.path.abspath(entity_root))]
    bases += repo_roots(entity_root, extra_repos)
    unresolved_paths = []
    for p in paths:
        if resolve_path(p, bases):
            continue
        base = os.path.basename(p.rstrip("/"))
        if p in blob or (base and base in blob):
            continue
        unresolved_paths.append(p)
    prose = prose_signal(message, turn_tools)
    nameless = nameless_dispatch(message, prose)

    # --- REPORTING: a value claimed gone, surviving in another spelling ----
    absence_reports = []
    vclaims = value_absence_claims(message)
    if vclaims:
        for v, sp, fp in surviving_spellings(
                vclaims, entity_root,
                time.monotonic() + STATE_BUDGET_SECONDS):
            absence_reports.append((v, sp, fp, entity_root))

    # --- REPORTING: "everything is clean and pushed" over unlanded branches --
    # Gated on the CLAIM, so the sweep's ~0.5s of git only happens on the 3% of
    # turns that assert completeness. NEGATED claims are silent for the same
    # reason they are silent above: understating what landed has never been the
    # failure this engine protects against, and "nothing has landed yet" being
    # true is not a finding.
    complete_reports = []
    complete_unevaluated = ""
    cclaims = [(s, pol) for s, pol in completeness_claims(message)
               if pol == "positive"]
    if cclaims:
        swept, why = unlanded_sweep(entity_root, session_id)
        if swept is None:
            # NOT silence. A turn asserting completeness while the check that
            # could contradict it did not run is exactly the state this whole
            # class exists to stop being invisible.
            complete_unevaluated = why
        elif swept.get("findings"):
            complete_reports.append((cclaims[0][0], swept))

    # --- observation record -------------------------------------------------
    # Written for EVERY turn, blocked or not, so the reporting layer is a record
    # rather than a rumour. Identifiers only -- never the message, never the
    # operator's words. This file is state, not source.
    record = {
        "session": session_id,
        "prompt_id": payload.get("prompt_id"),
        "tokens": {"name": len(names), "sha": len(shas), "path": len(paths),
                   "state": len(sclaims)},
        "unresolved": {
            "name": unresolved_names,
            "sha": unresolved_shas,
            "path": unresolved_paths,
        },
        "names_evaluable": names_evaluable,
        "prose_signal": prose,
        "nameless_dispatch": nameless,
        "role_claims": [r for r, _ in rclaims],
        "undispatched_roles": [r for r, _ in undispatched_roles],
        "stale_roles": [r for r, _ in stale_roles],
        "state_claims": [{"kind": k, "sha": sh, "polarity": pol}
                         for k, sh, _, pol in sclaims],
        "state_reports": [{"kind": k, "sha": sh, "why": w}
                          for k, sh, _, w in state_reports],
        "bad_state_claims": [{"kind": k, "sha": sh, "repo": rp, "ref": rf}
                             for k, sh, _, rp, rf in bad_states],
        "value_absence": [{"value": v, "surviving": sp, "file": fp}
                          for v, sp, fp, _ in absence_reports],
        "completeness_claims": [{"polarity": pol} for _, pol
                                in completeness_claims(message)],
        "unlanded_branches": [{"repo": f["label"], "branch": f["branch"],
                               "commits": f["commits"], "tip": f["tip"]}
                              for _, sw in complete_reports
                              for f in sw["findings"]],
        "unlanded_unevaluated": complete_unevaluated,
        "verdict": "block" if (unresolved_names or unresolved_shas
                               or undispatched_roles or bad_states) else "pass",
    }
    try:
        state = os.path.join(entity_root, ".claude", "state")
        os.makedirs(state, exist_ok=True)
        with open(os.path.join(state, "claim-checks.jsonl"), "a", encoding="utf-8") as f:
            f.write(json.dumps(record) + "\n")
    except Exception:
        pass  # the log is a convenience; losing it never changes the verdict

    # --- verdict ------------------------------------------------------------
    if not (unresolved_names or unresolved_shas or undispatched_roles
            or bad_states):
        if (unresolved_paths or prose or stale_roles or absence_reports
                or state_reports or complete_reports or complete_unevaluated):
            lines = ["=== claim check: PASSED, with observations (not blocking) ==="]
            for p in unresolved_paths:
                lines.append("  path cited but unresolved and ungrounded: %s" % p)
            for role, span in stale_roles:
                lines.append("  in-flight claim about a teammate that is NOT currently running:")
                lines.append("    \"%s\" -- no live %s in this session's roster." % (span, role))
                lines.append("    Name the agent (%s-<model>-<id>) or put the sentence in the past tense." % role)
            if prose:
                lines.append("  in-flight dispatch claim with no Agent call this turn:")
                lines.append("    %s" % prose)
                lines.append("  (this signal measures 17% precision -- it is logged, never enforced)")
                if nameless:
                    lines.append("  ...and it named no agent identifier (10.3% precision -- also never enforced)")
            for kind, sha, sentence, why in state_reports:
                if why == "negated":
                    lines.append("  you said this had NOT %s, and the repository says it HAS:"
                                 % ("landed" if kind == "integrated" else "been pushed"))
                else:
                    lines.append("  a %s claim whose polarity could not be read, and the"
                                 % ("landing" if kind == "integrated" else "push"))
                    lines.append("  repository does not agree with the positive reading:")
                lines.append("    \"%s\"" % sentence.replace("\n", " ").strip()[:200])
                lines.append("    (%s -- reported, never enforced: over-stating is the failure"
                             % sha)
                lines.append("     this gate blocks on, and understating is not)")
            for v, sp, fp, _root in absence_reports:
                lines.append("  a value you said was gone survives in a spelling you did not name:")
                lines.append("    you wrote #%s; %s is the same value and is still in %s" % (v, sp, fp))
                lines.append("    (grep every spelling before saying gone -- this fires on 54% of value")
                lines.append("     citations in the corpus, so it is reported and never enforced)")
            for sentence, sw in complete_reports:
                lines.append("  YOU CALLED IT DONE, AND %d BRANCH(ES) HOLD WORK MAIN DOES NOT HAVE:"
                             % sw["n_findings"])
                lines.append("    \"%s\"" % sentence.replace("\n", " ").strip()[:200])
                for f in sw["findings"][:6]:
                    lines.append("      %s %s — %d commit(s) ahead of %s, tip %s, %s days old%s"
                                 % (f["label"], f["branch"], f["commits"],
                                    f["trunk"], f["tip"],
                                    f["age_days"] if f["age_days"] is not None else "?",
                                    (", " + f["teammate"]) if f["teammate"] else ""))
                if sw["n_findings"] > 6:
                    lines.append("      (+%d more — scripts/unlanded-branches-lint.sh)"
                                 % (sw["n_findings"] - 6))
                lines.append("    A clean working tree and main == origin/main are BOTH TRUE while")
                lines.append("    these exist, and neither can see them. That is the whole of the")
                lines.append("    2026-09-06 failure: six finished branches outside main, two green")
                lines.append("    checks, and a correct answer to a question nobody asked.")
                lines.append("    (reported, never enforced — this guard has three recorded")
                lines.append("     false-positive incidents and a blocking gate with a")
                lines.append("     false-positive class gets waived, which is how a defense dies)")
            if complete_unevaluated:
                lines.append("  a completeness claim was made and the unlanded-branch check DID NOT RUN:")
                lines.append("    %s" % complete_unevaluated)
                lines.append("    Do not read this as agreement. Run scripts/unlanded-branches-lint.sh.")
            lines.append("  record: .claude/state/claim-checks.jsonl")

            # THE ONE LINE THAT LEAVES THE TRANSCRIPT. See operator_line().
            for sentence, sw in complete_reports:
                named = "; ".join(
                    "%s %s (%d commit%s)" % (f["label"], f["branch"],
                                             f["commits"],
                                             "" if f["commits"] == 1 else "s")
                    for f in sw["findings"][:4])
                extra = ("" if sw["n_findings"] <= 4
                         else " (+%d more)" % (sw["n_findings"] - 4))
                operator_line(
                    "YOU CALLED IT DONE AND IT IS NOT — \"%s\" — while %d "
                    "branch(es) hold work main does not have: %s%s. A clean "
                    "working tree and main == origin/main are both TRUE while "
                    "these exist and neither can see them. "
                    "scripts/unlanded-branches-lint.sh "
                    "(hook: scripts/hooks/guard-unresolved-claims.sh)"
                    % (sentence.replace("\n", " ").strip()[:160],
                       sw["n_findings"], named, extra))
            if complete_unevaluated:
                operator_line(
                    "A COMPLETENESS CLAIM WENT OUT UNCHECKED: this turn said "
                    "the work is all in, and the check that could contradict "
                    "it did not run (%s). Do not read this as agreement; run "
                    "scripts/unlanded-branches-lint.sh. "
                    "(hook: scripts/hooks/guard-unresolved-claims.sh)"
                    % complete_unevaluated[:200])
            sys.stderr.write("\n".join(lines) + "\n")
        return 0

    out = ["=== A CLAIM IN THE FINAL MESSAGE CONTRADICTS THE REPOSITORY — TURN BLOCKED ==="]
    if unresolved_names:
        out.append("")
        out.append("  Agent name(s) named in your reply that were never spawned:")
        for n in unresolved_names:
            out.append("      %s" % n)
        out.append("")
        out.append("  The spawned-names ledger only ever grows, and only an Agent call")
        out.append("  that actually executed writes to it. So this name has no spawn")
        out.append("  behind it. Either make the call now, or remove the name — a")
        out.append("  specific identifier in a report is a claim that it exists.")
    if unresolved_shas:
        out.append("")
        out.append("  Commit SHA(s) cited that name no object in any repository here,")
        out.append("  and that never appeared in this session's tool output:")
        for s in unresolved_shas:
            out.append("      %s" % s)
        out.append("")
        out.append("  Verify with `git cat-file -e <sha>` and cite the real one, or")
        out.append("  drop the citation. An unverifiable SHA in a report makes every")
        out.append("  other SHA in the report worth re-checking.")
    if undispatched_roles:
        out.append("")
        out.append("  You said a teammate is working, and named a ROLE rather than an agent:")
        for role, span in undispatched_roles:
            out.append("      \"%s\"" % span)
        out.append("")
        out.append("  No agent of that role has been spawned in this session at all, so")
        out.append("  there is nobody the sentence could be about. The ledger only grows")
        out.append("  and only a real Agent call writes to it, so this cannot be a stale")
        out.append("  reading.")
        out.append("")
        out.append("  Make the dispatch now, or name the agent in the checkable form")
        out.append("  (<role>-<model>-<identifier>) so the claim resolves against the")
        out.append("  roster instead of against a word.")
    for kind, sha, sentence, repo, ref in bad_states:
        out.append("")
        if kind == "integrated":
            out.append("  You said this LANDED, and the repository says it did not:")
        else:
            out.append("  You said this was PUSHED, and the repository says it was not:")
        out.append("")
        out.append("      \"%s\"" % sentence.replace("\n", " ").strip())
        out.append("")
        if kind == "integrated":
            out.append("  %s is reachable from %s in %s," % (sha, ref, repo))
            out.append("  and it is NOT an ancestor of main/master there. The commit exists;")
            out.append("  it is sitting on a branch. A merge that was refused, or that never")
            out.append("  ran because it was chained behind a command that was, leaves exactly")
            out.append("  this state — and the report gets written from the command you typed")
            out.append("  rather than from the repository.")
            out.append("")
            out.append("  Run the merge, or say where the work actually is. Confirm with:")
            out.append("      git -C %s merge-base --is-ancestor %s "
                       "$(engine/mega-lander/workspaces.sh integration-branch --repo %s | cut -f1)"
                       % (repo, sha, repo))
        else:
            out.append("  %s is reachable from %s in %s and from no remote-tracking" % (sha, ref, repo))
            out.append("  ref, so it is committed locally and has not left this machine.")
            out.append("")
            out.append("  Push it, or drop the word. Confirm with:")
            out.append("      git -C %s branch -r --contains %s" % (repo, sha))
    out.append("")
    out.append("  Fix the reply and finish the turn. Do not weaken or unwire this hook.")
    out.append("(hook: scripts/hooks/guard-unresolved-claims.sh)")
    sys.stderr.write("\n".join(out) + "\n")
    return 2


if __name__ == "__main__":
    sys.exit(main())
