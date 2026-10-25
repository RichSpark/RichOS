#!/usr/bin/env python3
"""ass-kicker.py — THE ANALYSIS HALF of the Stop-time check that a
turn's REPORT agrees with the turn's ACTIONS, in three arms:

  ARM 1  STATED, NOT TAKEN   the text states an action; the tool calls do not
                             contain it.
  ARM 2  THE TURN THAT STOPS a teammate's completion arrived in this turn, and
                             the turn ends having started nothing and declared
                             nothing.
  ARM 3  RECORDED, NOT       the text claims something was recorded, saved,
         WRITTEN             written down, logged, filed or made a rule, and
                             the turn wrote it nowhere durable. An ALERT to
                             the lead, never a refusal.

===========================================================================
ARM 3 — RECORDED, NOT WRITTEN (added 2026-10-02)
===========================================================================
The CEO, 2026-10-02, after "Recorded as a standing rule": recorded WHERE???
— "Get a hook added that HITS YOU WITH A HAMMER every time you claim to have
'recorded' something without actually doing it." The turn behind that claim
wrote one file, in the lead's private memory directory, through
`cd …/memory && cat > feedback_….md`, and the claim was a bulleted line.

THE CLAIM is read from the reply with list items INCLUDED (ARM 1 skips them;
this claim lives in them): a first-person act ("I've saved your ruling"), a
subjectless participle ("Recorded as the first job for the next session"), a
pronoun or rule-noun passive ("It's recorded", "Your ruling is recorded as
§59"), or a rule claim ("now a standing rule", "now on record"). Excluded, each
for a measured reason: denials, questions, conditionals, modals, quotes,
reported speech, other people's acts ("He …, and recorded the anomaly"),
result clauses (", so the gap is written down"), habitual mechanisms ("it's
logged every time"), media senses (a screen recording), noun uses ("Recorded
numbers:"), acknowledgments ("Noted that …") and references to earlier turns
("the thing I recorded an hour ago").

THE EVIDENCE is this turn's own tool calls: Write / Edit / MultiEdit /
NotebookEdit targets, and Bash write targets (redirects, tee, sed -i, python
writes) resolved against the command's own `cd` and shell variables; commits;
engine record commands (escalation ledger, acknowledgement logs, workspace
registry) with the entry ids on their command lines; ECS checkpoints; in the
app, a team save by the path it answers and a job report that answers
recorded: true, with its obligation id as the entry (2026-10-24). Each
target is classified: the memory directory and ECS are PRIVATE; scratch and
temp are nothing; a git work tree file is a REPOSITORY record; anything else
durable is a FILE record.

THE ALERT (CEO, 2026-10-02, replacing the refusal this arm first shipped as:
"check whether or not a 'recorded' claim was followed by the appropriate action
and then issue an alert if it wasn't", then "I don't need to see it. YOU need to
see it and do your job."). Per claim, the turn either wrote it to a durable
record or it did not:
  * a repository or durable file, an engine record command's entry, or any
    write at all: backed, SILENT. No SHA recital, no "where" recital;
  * "in my private notes" and the turn wrote the memory directory: backed;
  * the turn wrote ONLY a private note (memory or ECS) and the reply does not
    say "in my private notes", or points at "my notes" without saying private:
    ALERT;
  * the turn wrote nothing at all: ALERT;
  * a citation of an existing record ("it is written in", "already recorded"):
    not an act of this turn, SILENT.
The alert is one stdout line, RICHOS_RECORD_ALERT<TAB>text, which the wrapper
turns into a Stop `additionalContext` (the lead's own context). It is never a
refusal, never a block, and never a user-visible message. It runs on the
re-fire too, because the re-fire carries the reply that ends the turn. The three
misread classes the corpus replay found (a past confession, a teammate's "saved",
a description of an existing document) are not claims.

MEASURED: every Stop point in the governed project's lead transcripts (72
sessions, 2026-08-07 to 2026-10-02), replayed through this file by
ass-kicker/tests/record-claims.replay.py. Counts and the adjudication are in
ass-kicker/docs/stated-actions.corpus.md (ARM 3); the excerpts, which quote the
lead's replies and the CEO's questions, are in the private record only.

WHAT ARM 3 CANNOT SEE: a claim made in an intermediate text block of a turn
(only the final message is read, like ARMS 1 and 2); whether the record says
what the claim says it does; a session the workspace gate holds in recovery
(every Stop guard stands down there, by that gate's own contract); a write by
a command shape none of the parsers know.

Called by ass-kicker.sh, which has already resolved the two roots,
read orchestration.config and decided that this repository adopted the engine.
This file reads the turn, reconciles the report against the tool calls, and
returns the verdict. It never decides whether to run.

===========================================================================
THE DEFECT, WITH THE DATE AND THE SENTENCES
===========================================================================
2026-09-02, one session, seven instances of one failure: the lead WRITES A
SENTENCE DESCRIBING AN ACTION AND TREATS HAVING WRITTEN IT AS HAVING DONE IT.
Two of the seven share an exact machine signature and are ARM 1:

    17:49  "Zach builds it tomorrow."        turn's tool calls: Bash x3, no Agent
    18:29  "Frank breaks it first, and I     turn's tool calls: Bash x7, no Agent
            want him attacking ..."

The CEO's next message asked, in blunt terms, where Frank was this time — and
after the apology — "I said 'Frank breaks it first' and then wrote a status
report instead of spawning him" — his reply was that this class of failure will
never end, will it?"*

The same day he sent six messages of one other shape — "No Frank this time?",
"Next Sage.", then a blunter repeat of the same question — each one restarting work
that had paused because a teammate returned and the lead answered it with a
REPORT and no dispatch. That is ARM 2. His question, verbatim, was "when will
this pattern STOP?". There was already a prose rule for it (the working record:
"A land ends by STARTING the top unblocked item, then reports") and there was
already a GUARD for it — guard-idle-land.sh — and the guard's own observation
log shows why it let all three of tonight's turns through:

    finishes: ["Design the elimination of the worktree class"]  dispatched: 0
        verdict: backlog-empty   rows: 41   free: 0
    finishes: ["Break the elimination design"]                  dispatched: 0
        verdict: backlog-empty   rows: 41   free: 0
    finishes: ["Lease-based worktree design"]                   dispatched: 0
        verdict: backlog-empty   rows: 41   free: 0

Its term 4 asks the BACKLOG whether there is something to start, and the next
step after a returned design — stress-test it — is not a backlog row. So the
gate saw the finish, saw no dispatch, and stood down on a table. ARM 2 is that
gate's first three terms with the fourth replaced by a DECLARATION: a stop
after a completion passes only if the lead SAYS, in the documented form, why
this is one of the three legitimate stops. An implicit stop is refused.

The same day's measured lesson: every rule left as PROSE was broken, and every
rule with a GUARD caught the lead — six refusals that evening, from five
guards. So both arms BLOCK. A notice that names the failure and lets the turn
end is the same defect as a reaper that saw 14 unlanded commits and printed
CLEAN.

===========================================================================
ARM 1 — THE SIGNATURE, stated precisely
===========================================================================
    the assistant's final text for a turn states an action, and that turn's
    tool calls do not contain it.

Both halves are read from structure. The tool calls come from the transcript,
scoped to THIS TURN by prompt_id exactly the way turn-manifest.py scopes them
(that module is IMPORTED for the call list rather than re-derived — the
sibling that re-derived it collected session-wide and was silently inverted
for weeks). The stated action is read from `last_assistant_message`.

The prose half is a heuristic over English, and the engine has been here
before: guard-unresolved-claims.py measured `dispatching|spawning|launching`
at 17% precision and refused to enforce it. So the trigger here is NOT a word.
It is a SENTENCE SHAPE, fitted to the two real failures and then measured
against every real turn on disk, and only the arms that fired on NOTHING ELSE
are allowed to block. The rest report, with their rate written down.

Corpus: every orchestrator transcript in ~/.claude/projects/-Users-alex-ab-
femcboost/ (19 sessions, 1,268 turns, 1,213 with a final message, 2026-07-27
to 2026-09-02), replayed by prompt_id span exactly as the Stop payload would
present each turn, through THIS file. Method and adjudication:
ass-kicker/docs/stated-actions.corpus.md.

ARM 1a — ROLE ACT  (BLOCKS — 2 fires over 1,213 messages, both the defect)
    A clause whose SUBJECT is a roster role, whose VERB is present-simple
    third person, and whose OBJECT is a pronoun or a determiner phrase:

        Zach builds it tomorrow.
        Frank breaks it first[, and I want him ...]

    and the turn contains no Agent call for that role and no SendMessage to a
    teammate. Before its exclusions this shape fired 15 times; 13 were false,
    and every false one belonged to one of four classes, each excluded for a
    stated reason rather than by tuning:

      REPORT VERBS     "Frank recommends the hybrid", "Sage rates this the
                       biggest risk", "Sage comes back with", "Echo continues
                       on". Verbs of saying, judging, returning and continuing
                       describe a running or finished agent's OUTPUT, not an
                       act the lead has to take. That class is liveness, and
                       liveness is owned (report-only, on measured grounds) by
                       guard-agent-state-claims.py.
      LIST ITEMS       "1. Zach builds it — in flight now", "- Dean fixes
                       Sterling's definition", "5. ... Iris builds each". A
                       bulleted or numbered line is a plan or a status table.
                       Both real failures were plain sentences in prose. Lines
                       that open with a list marker are not scanned.
      BARE OBJECTS     "Art designs Bootstrap components", "Clark researches
                       what an art director is". A description of a role's
                       craft takes a bare noun; an announced act takes a
                       pronoun or a determiner ("it", "the design", "this").
      CEO PROPOSALS    "Say go and I'll start the hire", "say the word and
                       I'll dispatch it". A clause conditioned on the CEO's
                       word is a proposal, and ending a turn on a proposal is
                       legitimate. Also: an AskUserQuestion call this turn
                       exempts the whole turn, the same term guard-idle-land
                       uses.

    After the exclusions: 2 fires, 2 genuine, 0 false. Recall is deliberately
    the price — "Frank breaks the elimination design" with no determiner is
    not caught, and that is stated here rather than discovered later.

ARM 1b — FIRST-PERSON DISPATCH  (BLOCKS — 0 fires, 0 false, guards a case the
                                  corpus does not contain)
    "I'm dispatching Frank", "I'll spawn Zach now", "Dispatching frank-opus-x1"
    — first person, present or future, a dispatch verb, and a ROLE or AGENT
    NAME as the object — in a turn with no matching Agent call. This is the
    17%-precision word family narrowed three ways: it must be first person
    (past incidents and quotations drop out), it must name who (the "names
    nobody" refinement measured at 10.3% is inverted — a dispatch that names
    nobody is not scanned), and it must not be conditioned on the CEO's word.
    All 10 raw fires in the corpus were "Say the word and I'll dispatch it";
    after the proposal exclusion, none. It ships blocking on the precedent of
    the never-dispatched-role arm: zero cost, and a positive probe in the
    suite proves it can fire.

ARM 1c — ROLE FUTURE  (REPORTS — 2 fires, 2 false, 0% precision)
    "Sage will come back with the actual count", "Sage will fold all of it in
    and I'll land it". Both were predictions about an agent already running.
    Kept as a report with the number, so nobody promotes it without
    re-measuring.

REFUSED OUTRIGHT — measured and not shipped in any form:
    "Landed / merged / pushed" with no git merge or push this turn: 12 fires,
    every one a cross-turn reference ("Landed at `13e3f00ac`" about an earlier
    turn's merge) or a noun ("Landing rule enforced per merge"). The SHA-
    bearing version of this claim is already owned by guard-unresolved-claims
    against the repository, which is the right ground truth for it.
    "Running now: X" with no tool call: 5 fires, all status about work
    dispatched in an earlier turn or the phrase "running tally". Liveness
    class again.

WHAT SATISFIES A STATED ACTION — the quiet direction everywhere
  * an Agent call this turn whose subagent_type is the role, or whose name
    starts with "<role>-"; an Agent call whose input carries neither field
    satisfies every role (a call the guard cannot read is a call, not an
    absence);
  * a SendMessage this turn to anyone but the lead — a resumed teammate is a
    dispatch, and its `to` may be an opaque agent id that cannot be joined to
    a role here, so any teammate message satisfies every role;
  * an AskUserQuestion this turn — the CEO is being asked, and the turn may
    end on his answer.

===========================================================================
ARM 2 — THE TURN THAT STOPS, stated precisely
===========================================================================
    the turn's window holds a host-written <task-notification> saying an
    Agent FINISHED (status completed), AND the turn started nothing (no Agent
    call, no backgrounded tool call), AND nothing is owed to the CEO (no
    AskUserQuestion), AND the reply carries no valid `stop-declared:` line.

    IN THE APP, Stu's own work tools answer the "started nothing" term too
    (2026-10-24, nightly 73 and 74 walks, where both of his truthful replies
    were refused): a `complete` that closed the selected obligation means
    nothing of that obligation is pending, which answers for every completion
    except one whose work receipt names another, still open job, and a `team save` of a new teammate
    means the app resumes the job with a team_reloaded message. Either discharges the arm, read from the
    tool's answer, never from the reply. See app_job_outcome().

    THE TERM THAT USED TO SAY "no hold or end-of-day in his own words" IS GONE
    (2026-09-20, CEO ruling §68). No hook reads his sentences to decide
    anything. What it excused is still excusable, through the declaration.

EVERY ONE OF THOSE TERMS IS guard-idle-land.py's, BY IMPORT. The completion
signal is its agent_finishes() — the host's own summary shape, `<status>
completed</status>` required, a killed agent and a finished shell excluded by
the summary text itself. The declaration is its stop_declaration(), with its
three cases, its six-word and thirty-character floors, and its code-span
strip. Nothing here re-derives a term that gate already measured, and there
is exactly one vocabulary for "this stop is legitimate" in the engine:

    stop-declared: nothing-unblocked    — everything unblocked is genuinely done
    stop-declared: ceo-owns-it          — he stopped this, or his answer IS the deliverable
    stop-declared: waiting-on-teammate  — a teammate is running and the next step needs it

What ARM 2 does NOT import is term 4, the backlog. That is the whole
difference, and it is the reason the three turns above went green.

WHY THIS DOES NOT DOUBLE-FIRE WITH guard-idle-land.sh: the sibling refuses
only when the backlog has a free row; this arm refuses only when there is no
declaration. On a turn where both hold, both refuse, the refusals name the
same three declared cases, and one line satisfies both — a declaration is a
declaration, not a token per gate.

MEASURED (same corpus, same replay, through this file — the corpus file
carries the per-turn adjudication):
    turns whose window carries an Agent-finished notice     (see corpus.md)
      started something (Agent / backgrounded)                silent, correct
      put something to the CEO / held / off duty              silent, correct
      declared                                                silent, shown
      NOTHING — report and stop                               REFUSED
    Every refusal in the corpus was read by hand and every one was a turn the
    operator had to restart by hand. The number is in the corpus file, and
    the three named turns of 2026-09-02 are among them.

===========================================================================
WHAT THIS CANNOT SEE — stated here, not discovered later
===========================================================================
  * a claim about STATE that no tool call established ("three repos clean and
    pushed", "the scheme is untouched"). Five of the day's seven instances.
    Different signature, no monotonic ground truth for most of it; a stretch
    goal, not this file.
  * an announced act whose object is a bare noun, or that sits in a list.
  * a role the entity has not defined. Roles are DERIVED from the entity's
    .claude/agents/*.md and the session roster; an empty set makes ARM 1a
    inert (recorded as such), never guessed.
  * a turn with no final text at all. There is no report to reconcile.
  * anything after the turn ends. Point-in-time, like its siblings.

===========================================================================
IT CANNOT FIRE ON ITSELF, AND IT FAILS OPEN
===========================================================================
The refusal this file prints reaches the model on stderr and is never part of
`last_assistant_message`. The offending clause is printed inside double
quotes, so a reply that pastes it back has it stripped as quoted speech, and
code spans, fenced blocks (including ```ecs``` records, which carry the
sentence as before="…" text), blockquotes and headings are stripped before
any sentence is read. A reply that explains "I wrote 'Frank breaks it first'
and did not spawn him" carries the sentence only inside quotes and only as
reported speech; both defenses stop it. The declaration line the refusal
prints is indented under a code-span strip for the same reason, exactly as
guard-idle-land.py does it.

Every error path returns 0. A Stop guard that fails closed refuses to let the
session end, and the wrapper explains why that is worse than the defect.

Exit codes:
  0  nothing stated-but-untaken, no undeclared stop after a completion, and
     every record claim backed; or exempt, not evaluable, report-only, or any
     error at all
  2  BLOCKED — the final message states an action this turn did not take, or
     a teammate finished this turn and the turn ends having started nothing
     and declared nothing, or the message claims a record this turn did not
     write or does not locate
"""

import importlib.util
import json
import os
import re
import sys

_HERE = os.path.abspath(os.path.join(os.path.dirname(__file__), "../scripts/hooks"))


def _load(name, filename):
    """A sibling analyzer, imported rather than copied: a second reader of the
    turn boundary is a second place for the session-wide-scope bug to come
    back, and a second copy of the declaration vocabulary is a fork."""
    try:
        spec = importlib.util.spec_from_file_location(
            name, os.path.join(_HERE, filename))
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        return mod
    except Exception:
        return None


# --------------------------------------------------------------------------
# text preparation — everything that is not the assistant's own plain prose
# is removed BEFORE any sentence is read
# --------------------------------------------------------------------------

FENCE_RE = re.compile(r"```.*?```", re.S)
CODE_SPAN_RE = re.compile(r"`[^`\n]*`")
DQUOTE_RE = re.compile(r"[\"“][^\"”\n]{0,300}[\"”]")
SQUOTE_RE = re.compile(r"(?<![A-Za-z])['‘][^'’\n]{2,300}['’](?![A-Za-z])")
EMPH_RE = re.compile(r"[*_]{1,3}")
BLOCKQUOTE_LINE_RE = re.compile(r"^[ \t]*>")
HEADING_LINE_RE = re.compile(r"^[ \t]*#{1,6}[ \t]")
# A list marker may sit inside emphasis: "**1. Zach builds it — in flight
# now.**" is a numbered line. The replay found it because the emphasis strip
# ran AFTER the line filter, and the line was scanned as prose.
LIST_LINE_RE = re.compile(r"^[ \t]*[*_]{0,3}(?:[-*+•]|\d{1,3}[.)])[ \t]")
TABLE_LINE_RE = re.compile(r"^[ \t]*\|")
SENTENCE_SPLIT_RE = re.compile(r"(?<=[.!?])\s+|\n+")
# Clauses split on coordination and semicolons ONLY. A dash or a colon opens
# an appositive that modifies the clause before it ("Zach builds it — in
# flight now"), and splitting there would lose the very words that mark the
# clause as a present-state claim rather than an announcement.
CLAUSE_SPLIT_RE = re.compile(r",\s+(?:and|but|so|then|which|while)\b|;\s+")


def clean(message):
    """The assistant's own prose, line by line, with everything quoted,
    fenced, coded, listed, tabulated or headed removed."""
    text = FENCE_RE.sub(" ", message)
    kept = []
    for line in text.split("\n"):
        if (BLOCKQUOTE_LINE_RE.match(line) or HEADING_LINE_RE.match(line)
                or LIST_LINE_RE.match(line) or TABLE_LINE_RE.match(line)):
            continue
        kept.append(line)
    text = "\n".join(kept)
    text = CODE_SPAN_RE.sub(" ", text)
    text = DQUOTE_RE.sub(" ", text)
    text = SQUOTE_RE.sub(" ", text)
    text = EMPH_RE.sub("", text)
    return text


def sentences(message):
    for s in SENTENCE_SPLIT_RE.split(clean(message)):
        s = s.strip()
        if s:
            yield s


def clauses(sentence):
    for c in CLAUSE_SPLIT_RE.split(sentence):
        c = c.strip().strip(",;:")
        if c:
            yield c


# --------------------------------------------------------------------------
# the roster — derived, never typed
# --------------------------------------------------------------------------

ROLE_SHAPE = re.compile(r"[a-z][a-z0-9]{1,15}")
LEAD_ROLES = {"team-lead", "team", "rich"}


def roster_roles(entity_root, teams_dir, session_id):
    roles = set()
    d = os.path.join(entity_root or "", ".claude", "agents")
    try:
        for f in os.listdir(d):
            if f.endswith(".md") and ROLE_SHAPE.fullmatch(f[:-3]):
                roles.add(f[:-3])
    except OSError:
        pass
    if session_id and teams_dir:
        p = os.path.join(teams_dir, "session-" + session_id[:8], "config.json")
        try:
            with open(p, encoding="utf-8") as fh:
                for m in (json.load(fh).get("members") or []):
                    t = str((m or {}).get("agentType") or "")
                    if ROLE_SHAPE.fullmatch(t):
                        roles.add(t)
        except Exception:
            pass
    return roles - LEAD_ROLES


# --------------------------------------------------------------------------
# ARM 1a — ROLE ACT
# --------------------------------------------------------------------------

# Verbs of SAYING, JUDGING, RETURNING, CONTINUING, THINKING and BEING. A clause
# built on one of these reports what an agent produced or is, which is the
# liveness class and not an act the lead has to take. The set is classes with
# members, not words that happened to misfire.
REPORT_VERBS = {
    # saying / judging
    "says", "reports", "recommends", "rates", "confirms", "finds", "flags",
    "notes", "agrees", "argues", "concludes", "warns", "admits", "claims",
    "states", "writes", "answers", "asks", "tells", "explains", "describes",
    "calls", "names", "lists", "shows", "suggests", "proposes", "objects",
    "disagrees", "pushes", "backs", "corrects", "judges", "scores", "grades",
    # returning / continuing / finishing
    "comes", "returns", "delivers", "continues", "finishes", "completes",
    "hands", "lands", "goes", "gets", "arrives", "responds",
    # thinking / wanting
    "thinks", "believes", "suspects", "expects", "wants", "needs", "prefers",
    "knows", "understands", "sees", "means", "assumes", "considers", "likes",
    # being / having
    "is", "has", "owns", "keeps", "stays", "holds", "remains", "seems",
    "looks", "sounds", "feels", "exists", "lives", "sits", "stands", "does",
    "covers", "handles", "supports", "guides", "governs", "carries", "works",
}

OBJECT = (r"(?:it|this|that|them|him|her|both|everything|"
          r"the|these|those|his|its|their|my|our|your|each\s+of)\b")


def role_act_re(roles):
    """The subject is the FIRST word of the clause: a capitalized roster role.
    Not a role mentioned mid-clause ("I want Frank to break it"), which is
    intent about a person and a different sentence."""
    alt = "|".join(re.escape(r.capitalize()) for r in sorted(roles))
    return re.compile(r"^(?P<role>" + alt + r")\s+(?P<verb>[a-z]+s)\s+" + OBJECT, re.S)


def role_future_re(roles):
    alt = "|".join(re.escape(r.capitalize()) for r in sorted(roles))
    return re.compile(r"^(?P<role>" + alt + r")\s+(?:will|'ll|is\s+going\s+to)\s+(?P<verb>[a-z]+)\b", re.I)


# A clause that is conditioned, negated, reported, modal, in progress, past, or
# a question is not an announcement of an act about to be taken.
SUBORD_RE = re.compile(r"\b(once|when|whenever|after|if|unless|until|till|while|"
                       r"before|as\s+soon\s+as|assuming|provided|whether|"
                       r"in\s+case|so\s+that|because|since|the\s+moment)\b", re.I)
NEG_RE = re.compile(r"\b(not|never|no|neither|nor|nothing|nobody|without|"
                    r"isn't|doesn't|won't|can't|cannot|don't|didn't|wasn't)\b|n't\b", re.I)
REPORTED_RE = re.compile(r"\b(I\s+(?:said|told|wrote|reported|claimed|announced|promised|"
                         r"typed|narrated)|you\s+(?:said|asked|told|wrote)|"
                         r"(?:he|she|they)\s+(?:said|asked|wrote)|saying|"
                         r"instead\s+of|rather\s+than|before\s+saying)\b", re.I)
MODAL_RE = re.compile(r"\b(would|could|should|might|may|can|used\s+to)\b", re.I)
PROGRESS_RE = re.compile(r"\b(now(?!\s+that)|currently|already|still|running|"
                         r"underway|in\s+flight|in\s+progress|mid-\w+)\b", re.I)
QUESTION_RE = re.compile(r"\?\s*$")
# A proposal put to the CEO. The turn may end on it; it is his call.
PROPOSAL_RE = re.compile(r"\b(say\s+the\s+word|say\s+go|on\s+your\s+word|your\s+call|"
                         r"want\s+me\s+to|shall\s+I|should\s+I|do\s+you\s+want|"
                         r"if\s+you\s+(?:want|prefer|say|would|'d)|"
                         r"which\s+(?:do|would)\s+you|up\s+to\s+you)\b", re.I)
PAST_RE = re.compile(r"\b(yesterday|earlier|last\s+(?:night|week|session|turn)|"
                     r"this\s+morning|ago)\b", re.I)


def ends_on_ceo(message):
    """The reply's LAST sentence is a question or a proposal to the CEO."""
    last = ""
    for s in sentences(message):
        last = s
    return bool(last) and bool(QUESTION_RE.search(last) or PROPOSAL_RE.search(last))


def clause_excluded(sentence, clause, progress=True):
    """Two scopes, deliberately. A subordinator or a progress marker binds the
    clause it sits in ("Frank breaks it first, and I want him attacking
    whether ..." — the `whether` belongs to the second clause). A negation, a
    modal, a past marker, reported speech, a proposal or a question colors the
    whole sentence ("Frank breaks it, but not tonight" is a deferral, not an
    announcement), so those read the sentence."""
    if SUBORD_RE.search(clause) or (progress and PROGRESS_RE.search(clause)):
        return True
    if (NEG_RE.search(sentence) or MODAL_RE.search(sentence) or PAST_RE.search(sentence)
            or REPORTED_RE.search(sentence) or PROPOSAL_RE.search(sentence)
            or QUESTION_RE.search(sentence)):
        return True
    return False


_FUTURE_REPORT = {v[:-1] for v in REPORT_VERBS if v.endswith("s")} | {"be", "have", "come", "go", "get"}


def role_act_claims(message, roles):
    """[(who, clause, arm)] — ARM 1a (blocking) and ARM 1c (reporting)."""
    if not roles:
        return []
    act = role_act_re(roles)
    fut = role_future_re(roles)
    out = []
    for s in sentences(message):
        for c in clauses(s):
            m = act.match(c)
            if m and m.group("verb").lower() not in REPORT_VERBS:
                if not clause_excluded(s, c):
                    out.append((m.group("role").lower(), c[:200], "role-act"))
                continue
            m = fut.match(c)
            if m and m.group("verb").lower() not in _FUTURE_REPORT:
                if not clause_excluded(s, c):
                    out.append((m.group("role").lower(), c[:200], "role-future"))
    return out


# --------------------------------------------------------------------------
# ARM 1b — FIRST-PERSON DISPATCH
# --------------------------------------------------------------------------

AGENT_NAME = r"[a-z][a-z0-9]{1,15}-(?:fable|opus|sonnet|haiku)-[a-z0-9]{1,12}"
DISPATCH_VERB = r"(?:dispatch|spawn|launch|brief|re-?spawn|send\s+in|kick\s+off|start|fire\s+up|bring\s+in)"
DISPATCH_GERUND = (r"(?:dispatching|spawning|launching|briefing|re-?spawning|sending\s+in|"
                   r"kicking\s+off|starting|firing\s+up|bringing\s+in)")


def first_person_dispatch_re(roles):
    """Two shapes: "I'm dispatching Frank" / "I'll spawn frank-opus-x1", and
    the sentence-initial gerund "Dispatching Frank now". Both must NAME who —
    a dispatch that names nobody is not scanned (the "names nobody" refinement
    measured at 10.3% is inverted here on purpose)."""
    alt = "|".join(re.escape(r.capitalize()) for r in sorted(roles)) if roles else None
    who = r"(?P<name>" + AGENT_NAME + r")" + (r"|(?P<role>" + alt + r")" if alt else "")
    return re.compile(
        r"(?:\b(?:I'm|I\s+am|I'll|I\s+will)\s+(?:now\s+|just\s+|about\s+to\s+)?" + DISPATCH_VERB + r"(?:ing)?"
        r"|^(?:Now\s+)?" + DISPATCH_GERUND + r")\s+"
        r"(?:a\s+|the\s+|another\s+|a\s+fresh\s+|him\s+|her\s+)?(?:" + who + r")",
        re.I)


def dispatch_claims(message, roles):
    rx = first_person_dispatch_re(roles)
    out = []
    for s in sentences(message):
        for c in clauses(s):
            m = rx.search(c)
            if not m:
                continue
            # "now" is the natural word in "I'm dispatching Frank now" — for
            # this arm it is the point, not a liveness claim. The other
            # exclusions (conditional, negated, reported, proposal, question)
            # apply unchanged.
            if clause_excluded(s, c, progress=False):
                continue
            who = (m.group("name") or "").lower() or ((m.groupdict().get("role") or "").lower())
            if who:
                out.append((who, c[:200], "first-person-dispatch"))
    return out


# --------------------------------------------------------------------------
# the turn's own tool traffic (ARM 1) — via turn-manifest.py
# --------------------------------------------------------------------------

def turn_calls(tm, transcript, prompt_id):
    """([(id, name)], {id: input}, error). The manifest module scopes the
    turn; the inputs of Agent / SendMessage / AskUserQuestion calls are read
    here by id so a stated role can be matched to a real spawn."""
    if tm is None:
        return [], {}, "turn-manifest.py could not be loaded"
    calls, _results, _examined, err = tm.read_turn(transcript, prompt_id)
    if err:
        return [], {}, err
    wanted = {tid for tid, name in calls if name in ("Agent", "SendMessage", "AskUserQuestion")}
    inputs = {}
    if wanted:
        try:
            with open(transcript, encoding="utf-8", errors="replace") as fh:
                for line in fh:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        rec = json.loads(line)
                    except Exception:
                        continue
                    content = (rec.get("message") or {}).get("content")
                    if not isinstance(content, list):
                        continue
                    for b in content:
                        if isinstance(b, dict) and b.get("type") == "tool_use" and b.get("id") in wanted:
                            inputs[b["id"]] = b.get("input") if isinstance(b.get("input"), dict) else {}
        except OSError:
            pass
    return calls, inputs, None


def satisfied(who, arm, calls, inputs):
    """Did this turn perform a dispatch that discharges `who` (a role or an
    agent name)? Quiet direction throughout: an unreadable call counts.

    ARM 1a (role-act) is discharged by ANY Agent call this turn. The sentence
    is a statement about the sequence of work, and a turn that dispatched
    something has the sequence in motion — measured: role-matching this arm
    fired on "Sage is designing that now ... Zach builds it." in the turn
    that spawned Sage, which is a plan in motion, not the defect.
    ARM 1b (first-person dispatch) names WHO is being dispatched, so only a
    call for that role or name discharges it."""
    for tid, name in calls:
        if name == "AskUserQuestion":
            return "ceo-asked"
        if name == "Agent":
            inp = inputs.get(tid)
            if arm != "first-person-dispatch" or inp is None:
                return "agent-call"
            stype = str(inp.get("subagent_type") or "").lower()
            aname = str(inp.get("name") or "").lower()
            if not stype and not aname:
                return "agent-call"
            if who == aname or stype == who or aname.startswith(who + "-"):
                return "agent-call"
        if name == "SendMessage":
            inp = inputs.get(tid) or {}
            to = str(inp.get("to") or "").lower()
            if not to or to not in ("team-lead", "main", "lead"):
                return "teammate-message"
    return None


# --------------------------------------------------------------------------
# ARM 2 — THE TURN THAT STOPS — every term is guard-idle-land.py's, by import
# --------------------------------------------------------------------------

APP_COMPLETE = "mcp__richos_work__complete"
APP_TEAM = "mcp__richos_work__team"
TASK_ID_RE = re.compile(r"<task-id>\s*([^<\s]+)\s*</task-id>")


def finished_agents(idle, notices):
    """[(provider agent id, title)] for this turn's Agent completions, both
    read from the host's own notice (its <task-id> is the agent id the app
    records on the teammate's work receipt). One entry per agent id: titles are
    the caller's own descriptions, need not be unique, and are for display only
    (review rv-20261025T001232Z-00aed5ab-1fe4)."""
    out, seen = [], set()
    for text in notices or []:
        for part in text.split("<task-notification>")[1:]:
            part = "<task-notification>" + part
            titles, m = idle.agent_finishes([part]), TASK_ID_RE.search(part)
            if titles and m and m.group(1) not in seen:
                seen.add(m.group(1))
                out.append((m.group(1), titles[0]))
    return out


def receipt_obligations(agent_ids):
    """{agent id: obligation id} from the app's own work receipts, for the given
    agents only. The app keeps this conversation's receipts in
    <RICHOS_APP_STATE>/work-receipts/sha256(entity, thread), and its workspace
    registry, which it sets as RICHOS_WORKSPACES_DIR, in
    <RICHOS_APP_STATE>/workspaces/<the same hash> (mega-lander/app.py folder()
    and read_scope()). One folder is listed and its receipts read until every
    agent is found; nothing else is opened. {} outside the app or when the
    folder cannot be read; an agent no receipt names is simply absent."""
    state = (os.environ.get("RICHOS_APP_STATE") or "").strip()
    registry = (os.environ.get("RICHOS_WORKSPACES_DIR") or "").strip()
    if not agent_ids or not state or os.path.dirname(registry) != os.path.join(state, "workspaces"):
        return {}
    root = os.path.join(state, "work-receipts", os.path.basename(registry))
    try:
        names = sorted(n for n in os.listdir(root) if n.endswith(".json"))
    except OSError:
        return {}
    found = {}
    for name in names:
        path = os.path.join(root, name)
        try:
            if os.path.islink(path) or os.path.getsize(path) > 1024 * 1024:
                continue
            with open(path, encoding="utf-8") as fh:
                rec = json.load(fh)
        except (OSError, ValueError):
            continue
        if isinstance(rec, dict) and rec.get("agent_id") in agent_ids:
            found[rec["agent_id"]] = (rec.get("request") or {}).get("obligation_id")
            if len(found) == len(agent_ids):
                break
    return found


def app_job_outcome(idle, transcript, prompt_id, agents):
    """What the app's own work tools answered this turn about Stu's job, as
    (verdict, detail), or None. Read from the tools' answers, never the reply:
      ('closed', ...)   `complete` answered obligation_closed: true, and no
                        completion that arrived this turn is of a teammate
                        whose work receipt names an obligation still open
                        (receipt_obligations: the notice's agent id, the
                        receipt's obligation). The app closes an obligation
                        only when none of its own receipts is still preparing,
                        running or unknown and every final worker of it is
                        landed (mega-lander/app.py completion_evidence), so
                        nothing of THAT obligation is pending. It says nothing
                        about the lease, which goes on to its next job, or
                        about any other obligation: a completion whose
                        receipt names another job keeps the check, and the
                        refusal names only those (second review
                        rv-20261024T230343Z-5ff59ca2-e966: notes.md's reviewer
                        and more.md's worker finished, only notes.md was
                        closed, and "both are landed and closed" went through).
                        A completion whose receipt cannot be read (no receipt
                        names the agent, or the hook runs outside the app) is
                        not guessed at: it does not keep the check, and the
                        kept stop record (stated-actions.jsonl) lists it as an
                        owner not read. That records the unknown; it does not
                        settle it.
      ('started', ...)  `team save` added a NEW teammate. The app registers
                        him when this turn ends and then sends Stu the
                        team_reloaded message, which starts his next turn
                        (work_host.rs reload_team_after_turn), so the next
                        step is already in motion.
    A refused call counts for nothing, a replaced teammate gets no reload, and
    a land (`integrate`) is not a close. Nightly 73 and 74 walks: both
    truthful replies were refused without this."""
    try:
        offset = idle.turn_start_offset(transcript, prompt_id)
        wanted, answers, started = {}, {}, False
        with open(transcript, "rb") as fh:
            fh.seek(offset)
            for raw in fh:
                try:
                    rec = json.loads(raw)
                except ValueError:
                    continue
                if not isinstance(rec, dict):
                    continue
                if rec.get("promptId") == prompt_id:
                    started = True
                if not started:
                    continue
                content = (rec.get("message") or {}).get("content")
                if not isinstance(content, list):
                    continue
                for b in content:
                    if not isinstance(b, dict):
                        continue
                    if b.get("type") == "tool_use" and b.get("name") in (APP_COMPLETE, APP_TEAM):
                        wanted[b.get("id")] = (b.get("name"), b.get("input") or {})
                    elif b.get("type") == "tool_result" and b.get("tool_use_id") in wanted:
                        answers[b["tool_use_id"]] = _app_answer(b)
        closed = set()
        for tid, answer in answers.items():
            name, inp = wanted[tid]
            if name == APP_COMPLETE and answer.get("obligation_closed") is True:
                # The answer's id only: the app selects the carried obligation
                # before it looks at the argument (mega-lander/app.py complete).
                closed.add(answer.get("obligation_id"))
        if closed:
            owners = receipt_obligations({a for a, _t in agents})
            open_ = list(dict.fromkeys(t for a, t in agents if a in owners and owners[a] not in closed))
            unread = list(dict.fromkeys(t for a, t in agents if a not in owners))
            if not open_:
                return ("closed", "the app closed obligation %s this turn%s"
                        % (", ".join(sorted(str(o) for o in closed if o)),
                           "; owner not read for: %s" % ", ".join(unread) if unread else ""))
    except (OSError, AttributeError):
        return None
    for tid, answer in answers.items():
        name, inp = wanted[tid]
        if (name == APP_TEAM and isinstance(inp, dict)
                and inp.get("action") == "save" and answer.get("replaced") is False
                and answer.get("registration_boundary") == "next_provider_connection"):
            return ("started", "a new teammate is saved; the app reloads the team and resumes the job")
    if closed:
        return ("open", open_)
    return None


def stop_after_completion(idle, transcript, prompt_id, message):
    """None when ARM 2 has nothing to say, else a dict:
         finishes   titles of the agents whose completion arrived this turn
         verdict    'started' | 'ceo-owed' | 'closed' | 'declared' | 'undeclared-stop'
         detail     what discharged it, or the declaration, or the problem
    """
    if idle is None:
        return {"finishes": [], "verdict": "unavailable",
                "detail": "guard-idle-land.py could not be loaded"}
    turn = idle.read_turn(transcript, prompt_id)
    if turn is None:
        return None
    finishes = idle.agent_finishes(turn.get("notices"))
    if not finishes:
        return None
    if "Agent" in turn["tools"]:
        return {"finishes": finishes, "verdict": "started", "detail": "Agent"}
    if turn.get("backgrounded"):
        return {"finishes": finishes, "verdict": "started", "detail": "backgrounded tool call"}
    if "AskUserQuestion" in turn["tools"]:
        return {"finishes": finishes, "verdict": "ceo-owed", "detail": "AskUserQuestion"}
    job = app_job_outcome(idle, transcript, prompt_id, finished_agents(idle, turn.get("notices")))
    if job and job[0] == "open":
        finishes = job[1]
    elif job:
        return {"finishes": finishes, "verdict": job[0], "detail": job[1]}
    # NO HOLD TERM (2026-09-20, CEO ruling §68). This used to call
    # idle.hold_signal(turn["said"]) -- two regexes over the CEO's own typed
    # messages -- and excuse the turn on a match. Both the predicate and the
    # `said` collection it fed on are deleted from guard-idle-land.py; a hook
    # does not decide what he meant. A turn he stopped is declared, below.
    decl = idle.stop_declaration(message)
    if decl and decl.get("ok"):
        return {"finishes": finishes, "verdict": "declared", "detail": decl}
    return {"finishes": finishes, "verdict": "undeclared-stop", "detail": decl}


# --------------------------------------------------------------------------
# ARM 3 — RECORDED, NOT WRITTEN
# --------------------------------------------------------------------------
#
# The claim is read from the reply; the evidence is read from this turn's own
# tool calls. Both are structure-first: a claim is a SENTENCE SHAPE (first
# person, a subjectless participle, or a pronoun passive) over one closed verb
# family, and the evidence is the list of files this turn's Write / Edit / Bash
# calls actually wrote, classified by WHERE they are.
#
# LIST ITEMS ARE SCANNED HERE, unlike ARM 1. The case that built this arm was a
# bullet: "- **Recorded as a standing rule:** every phone-testing instruction
# names the command to use." (2026-10-02). A plan is written as a list; so is a
# report of what was done, and that is where this claim lives.

REC_VERB = (r"(?:recorded|saved|logged|documented|filed|captured|written\s+down|"
            r"noted(?=\s+(?:in|as|where|under|down)\b))")
# What may follow a sentence-initial "Recorded" when it is the verb and not an
# adjective. "Recorded as a rule", "Recorded.", "Recorded the lesson so ...",
# "Saved your choice" are claims; "Recorded creation", "Recorded numbers:" and
# "Saved work:" are noun phrases (all three measured on the corpus).
_FOLLOW = (r"(?=\s*(?:$|[.,:;!—–-]|\(|\s(?:as|so|in|into|to|at|and|that|it|this|these|the|for|with|including|"
           r"under|here|there|both|now|against|because|on|rather|but|separately|word|your|his|her|their|"
           r"my|its|all|each|every|alongside|together|prominently|verbatim|plainly|deliberately|"
           r"properly|exactly|first|too|also|them|him|a|an)\b))")

# "Recorded as a standing rule: ...", "Approved and recorded.", "Done —
# documented, filed, pushed.", "Noted where it'll bind next time.", "Saved."
ARM3_SUBJECTLESS_RE = re.compile(
    r"^(?:(?:and|also|then|now|both|all|this\s+is)\s+)?"
    r"(?:(?:approved|ruled|withdrawn|done|fixed|corrected|updated|accepted|understood|"
    r"agreed|confirmed|held|dispatched|started|sent|rule|ruling|decision)\s*(?:,|and|—|–|-|:)?\s*)*"
    r"(?:now\s+)?" + REC_VERB + _FOLLOW, re.I)

# "I've also saved your ruling", "I recorded it", "we've written it down",
# "I've made it a standing rule", "I'm recording this as a rule".
ARM3_FIRST_RE = re.compile(
    r"\b(?:I|we)(?:'ve|\s+have|\s+had)?(?:\s+(?:also|now|just|already|then|both))*\s+"
    r"(?:recorded|saved|logged|documented|filed|captured|"
    r"noted\s+(?:(?:it|this|them)\b|down\b|in\b|as\b|where\b)|"
    r"wrote\s+(?:it|this|that|them|these|those|both|each)\s+down|"
    r"written\s+(?:it|this|that|them|these|those|both)\s+down|"
    r"made\s+(?:it|this|that)\s+a\s+(?:standing\s+|permanent\s+|hard\s+)?rule|"
    r"put\s+(?:it|this|that)\s+(?:in|into|on)\s+(?:the\s+|your\s+|my\s+)?"
    r"(?:record|memory|notes|rules|wiki|decisions|claude\.md)|"
    r"added\s+(?:it|this|that|them)\s+to\s+(?:the\s+|your\s+|my\s+)?"
    r"(?:record|memory|notes|rules|wiki|decisions|claude\.md|open[- ]items))\b"
    r"|\bI'?m\s+(?:recording|saving|logging|documenting|filing|"
    r"writing\s+(?:it|this|that)\s+down)\b", re.I)

# The passive, with a subject that is the lead's own record or the thing he
# was told: "It's recorded", "That's filed as row 3.20", "Your ruling is
# recorded as §59", "it is written in the three places I read". Any other
# subject reads as a description of somebody else's text ("his answer is
# written in the vocabulary of what he probed", "7 test files are written in a
# form this check can't read") and is not scanned, unless "now" makes it an
# announcement ("Isaac's fix is now recorded as merging").
_SUBJ = (r"(?:it|this|that|both|these|they|all\s+(?:two|three|four|five)|"
         r"(?:the|your|my|this|that)\s+(?:rule|ruling|rulings|decision|order|correction|answer|words|"
         r"spec|lesson|failure(?:\s+type)?|type|finding|plan|choice|question))")
# "written in" is a claim only before a place: "written in the three places I
# read", "written into PRD 5", "written into CLAUDE.md". "It's written in his
# first person" is about voice, not a record.
_WRITTEN_IN = (r"written(?=\s+(?:in|into|on)\s+(?:the|your|my|our|both|all|two|three|four|"
               r"CLAUDE|PRD|§|[\w-]+\.md)\b)")
ARM3_PASSIVE_RE = re.compile(
    r"^(?:(?:and|but)\s+)?" + _SUBJ +
    r"\s*(?:is|'s|are|'re|was|were|has\s+been|have\s+been)\s+(?:now\s+|also\s+|already\s+)?"
    r"(?:" + REC_VERB + r"|" + _WRITTEN_IN + r")\b", re.I)
# The same passive with no "now", followed by a place ("it's recorded in two
# places", "it is written in the three places I read"), cites a record rather
# than announcing one: it must NAME the place.
ARM3_CITES_RE = re.compile(r"\s+(?:in|into|under|at|on)\b", re.I)
ARM3_PASSIVE_NOW_RE = re.compile(
    r"\b(?:is|'s|are|'re|has\s+been|have\s+been)\s+now\s+" + REC_VERB + r"\b"
    r"|^" + REC_VERB + r"\s+now\b", re.I)

# "recorded as a standing rule", "it's now a standing rule", "now the ruling on
# record". A rule claim is the strongest claim of the family: a rule is a
# mechanism or a line in the rules file, never a note.
ARM3_RULE_RE = re.compile(
    r"\b(?:recorded|saved|filed|logged)\s+(?:it\s+|this\s+|that\s+)?as\s+(?:a\s+|the\s+|my\s+)?"
    r"(?:standing\s+|new\s+|permanent\s+|hard\s+)?(?:rule|ruling|doctrine)\b"
    r"|\b(?:is|'s)\s+now\s+(?:a\s+|the\s+)?(?:standing\s+)?(?:rule|ruling)\b"
    r"|\bnow\s+(?:the\s+\w+\s+)?on\s+record\b", re.I)

# A record said to exist from BEFORE this turn ("already recorded", "recorded
# earlier", "on Sept 20"). It cannot be checked against this turn's calls, so
# it is held to the other half only: it must say WHERE.
ARM3_PAST_RE = re.compile(r"\b(?:earlier|previously|yesterday|last\s+(?:night|week|session|time)|"
                          r"this\s+morning|ago|since\s+then|on\s+(?:sept?\.?|september|"
                          r"oct\.?|october|aug\.?|august)\s+\d|on\s+\d{4}-\d\d-\d\d)\b", re.I)
ARM3_ALREADY_RE = re.compile(r"\balready\b", re.I)
ARM3_NOW_RE = re.compile(r"\bnow\b", re.I)
# Governs the verb only when it comes BEFORE it in the clause: "nothing was
# recorded" is a denial, while "it's now recorded as a standing rule, not a
# note" is the claim at full strength.
ARM3_NEG_RE = re.compile(r"\b(?:not|never|nothing|nobody|no\s+one|neither|nor|without)\b|n't\b", re.I)
ARM3_MODAL_RE = re.compile(r"\b(?:would|could|should|might|may|must|can|will|shall|'ll|going\s+to|"
                           r"to\s+be|needs?\s+to|has\s+to|have\s+to|want\s+to)\b", re.I)
ARM3_SUBORD_RE = re.compile(r"\b(?:once|when|whenever|if|unless|until|till|before|after|"
                            r"as\s+soon\s+as|whether|in\s+case|the\s+moment|how)\b", re.I)
ARM3_REPORTED_RE = re.compile(r"\b(?:I\s+(?:said|told|wrote|claimed|announced|promised|typed|reported)|"
                              r"you\s+(?:said|asked|told|wrote)|(?:he|she|they)\s+(?:said|asked|wrote)|"
                              r"as\s+I\s+(?:noted|said|mentioned)|(?:before|by|without)\s+saying|"
                              r"instead\s+of)\b", re.I)
# A mechanism, not an act: "it's logged every time it's used".
ARM3_HABITUAL_RE = re.compile(r"\b(?:every\s+time|each\s+time|whenever|automatically|always)\b", re.I)
# The other senses of the verbs. A screen recording, a voice clip, a saved
# second: none of them is a claim about the record.
ARM3_MEDIA_RE = re.compile(r"\b(?:video|screen|screenshots?|audio|clip|mp4|mov|microphone|mic|voice|"
                           r"podcast|calls?|recordings?|recorder|footage|camera|frames?)\b", re.I)
ARM3_SAVED_QTY_RE = re.compile(r"\bsaved\s+(?:you\s+|us\s+|me\s+|him\s+)?(?:\d|a\s+(?:round|day|minute|second|lot|"
                               r"few)|an?\s+hour|time|minutes|hours|seconds|money|tokens|nothing|"
                               r"everything\s+from)", re.I)
ARM3_LOGGED_IN_RE = re.compile(r"\blogged\s+(?:in|out|into|on)\b", re.I)
# The three misread classes the corpus replay found (stated-actions.corpus.md):
# a past CONFESSION ("I recorded it wrong."), a TEAMMATE's version-control
# "saved" ("saved their work"), and a DESCRIPTION of an existing document
# ("Documented, never fixed."). None of them claims a record made this turn.
ARM3_CONFESS_RE = re.compile(r"\b(?:wrong(?:ly)?|incorrectly|falsely|mistakenly|by\s+mistake|inaccurately|"
                             r"untruthfully|misrecorded|misfiled|too\s+early|prematurely)\b", re.I)
ARM3_OTHERS_SAVED_RE = re.compile(r"\b(?:saved|filed|logged|captured|recorded)\s+(?:their|his|her|its)\s+"
                                  r"(?:work|edits?|changes?|progress|branch|commits?|state|session)\b"
                                  r"|\b(?:saved|logged|filed)\s+(?:work|edits|changes|progress)\b", re.I)
ARM3_DESCRIBES_RE = re.compile(r"\bdocumented\s*,?\s*(?:and\s+)?(?:never|not|but\s+not|yet\s+to|still|"
                               r"nowhere|unfixed|unresolved)\b"
                               r"|\b(?:recorded|saved|logged|filed|captured)\s*,?\s*(?:and\s+)?(?:never|but\s+never|"
                               r"but\s+not|yet\s+to|still\s+(?:not|open|unfixed))\b", re.I)
# A sentence whose subject is somebody else: in "He nearly wrote five notes,
# caught it, and recorded the anomaly", the bare "recorded" of the third
# clause is HIS act, not the lead's.
_NOT_THIRD = {"i", "i've", "i'm", "i'll", "we", "we've", "it", "it's", "this", "that", "that's", "these",
              "both", "all", "and", "also", "then", "now", "so", "but", "your", "my", "our", "here",
              "there", "yes", "no", "understood", "one", "two", "three", "every", "first", "next"}


def third_subject(sentence):
    """True when the sentence's subject is somebody else: He, She, They, a
    capitalized name ("Echo had also started unsaved edits"), or "The
    engineer". Participles ("Recorded, and ...", "Dispatched, and ...") and
    the lead's own pronouns are not."""
    m = re.match(r"([A-Za-z][\w']*)(?:\s+(\w+))?", sentence)
    if not m:
        return False
    w = m.group(1)
    lw = w.lower()
    if lw in ("he", "she", "they", "each", "everyone", "nobody", "someone"):
        return True
    if lw == "the":
        return (m.group(2) or "").lower() not in ("rule", "ruling", "decision", "correction", "lesson", "type")
    if lw in _NOT_THIRD or lw.endswith("ed") or lw.endswith("ing") or not w[0].isupper():
        return False
    return True
PRIVATE_NOTES_RE = re.compile(r"\bprivate\s+(?:memory\s+|follow-?up\s+)?notes?\b", re.I)
# "Noted." as a promise or follow-up, and "it's in my (private) notes" (CEO
# 2026-10-21: "the 'noted' and 'private notes' need to be added to that hook").
# Neither is a record until a committed repository file holds it, so the
# claim is judged as kind "note": only a named, committed repository file
# backs it; the memory directory and ECS do not. "Noted that ..." stays an
# acknowledgment (R25), and a bare "Noted." with no later work promised is one too.
NOTED_ONLY_RE = re.compile(r"^(?:(?:ok(?:ay)?|got\s+it|understood)\s*[,.—–-]?\s+)?noted\s*[.!]?\s*$", re.I)
NOTED_LEAD_RE = re.compile(r"^noted\s*(?:[,:—–-]\s*|\s+(?=(?:for|as|on|in|re)\b))(?!that\b)(\S.*)$", re.I)
FUTURE_RE = re.compile(r"\b(?:goes?\s+in|going\s+in|will|I'll|we'll|it'll|after|once|next|later|then|when|"
                       r"before|follow-?ups?|queued|on\s+the\s+list|after\s+that)\b|'ll\b", re.I)
NOTES_CLAIM_RE = re.compile(r"\b(?:it|this|that|they|these)(?:'s|\s+is|\s+are|'re)\s+(?:now\s+|also\s+|just\s+|only\s+)?"
                            r"(?:in|on)\s+(?:my|the)\s+(?:private\s+)?(?:follow-?up\s+)?notes?\b", re.I)
FOLLOWUPS_FILE = "richos-hq/docs/plans/follow-ups.md"
# "That's on the follow-up list", "added to the follow-ups", "going on the
# follow-up list" (CEO 2026-10-23: "So, it wasn't on your fucking follow-up
# list?"). Backed only by a committed change to FOLLOWUPS_FILE in this turn.
FOLLOWUP_LIST_CLAIM_RE = re.compile(r"\b(?:on|added\s+(?:\w+\s+)?to)\s+(?:my|the)\s+"
                                    r"(?:follow-?up\s+list|follow-?ups(?:\s+list)?)\b", re.I)
# A completed preface before the claim's clause ("After checking the release,
# I've added it ...") neither denies nor conditions the claim.
FOLLOWUP_PREFACE_RE = re.compile(r"\b(?:after|before|once|when|since)\s+\w+ing\b[^,]*,", re.I)
RULE_NOUN_RE = re.compile(r"\b(?:rules?|rulings?|lessons?|doctrine|standing\s+(?:instruction|order)s?)\b", re.I)
NOTES_POINTER_RE = re.compile(r"\b(?:in|to|into)\s+(?:my\s+|the\s+)?(?:notes|memory)\b", re.I)
# From the corpus (ass-kicker/docs/stated-actions.corpus.md, ARM 3).
CEO_ASKED_WHERE = ("13 times between 2026-09-08 and 2026-10-02, and the replay finds 90 turns "
                   "claiming a record that was only a private note or nothing at all")
# Clause boundaries for ARM 3 carry their connector, so a result clause
# (", so the gap is written down") can be told from a coordinated act
# (", and recorded it").
ARM3_CLAUSE_RE = re.compile(r"(,\s+(?:and|but|so|then|which|while)\b|;\s+)")
ARM3_LABEL_RE = re.compile(r"^[A-Z][\w' -]{0,40}:\s+")


def clean_keep_lists(message):
    """clean(), except that a list item is SCANNED with its marker removed.
    Fences, code spans, quotes, blockquotes, headings and tables still go."""
    text = FENCE_RE.sub(" ", message)
    kept = []
    for line in text.split("\n"):
        if BLOCKQUOTE_LINE_RE.match(line) or HEADING_LINE_RE.match(line) or TABLE_LINE_RE.match(line):
            continue
        m = LIST_LINE_RE.match(line)
        if m:
            line = line[m.end():]
        kept.append(line)
    text = "\n".join(kept)
    text = CODE_SPAN_RE.sub(" ", text)
    text = DQUOTE_RE.sub(" ", text)
    text = SQUOTE_RE.sub(" ", text)
    text = EMPH_RE.sub("", text)
    return text


def _before(rx, clause, pos):
    return bool(rx.search(clause[:pos]))


def _own_clause_before(sentence, pos):
    """The claim's own clause up to pos: the text after the last ARM 3 clause
    boundary, with a completed preface ("After checking X,") removed."""
    start = 0
    for m in ARM3_CLAUSE_RE.finditer(sentence, 0, pos):
        start = m.end()
    return FOLLOWUP_PREFACE_RE.sub(" ", sentence[start:pos])


def _arm3_clauses(sentence):
    """[(connector, clause)] — connector is '' for the first clause."""
    parts = ARM3_CLAUSE_RE.split(sentence)
    out = [("", parts[0])]
    for i in range(1, len(parts) - 1, 2):
        out.append((parts[i].strip(" ,;").lower(), parts[i + 1]))
    return [(k, c.strip().strip(",;:")) for k, c in out if c.strip()]


def record_claims(message):
    """[(kind, sentence)] — kind is 'act' (a record made in this turn) or
    'state' (a record said to exist already), 'note' or 'list' (said to be on
    the follow-up list). One entry per sentence."""
    out = []
    sents = [x.strip() for x in SENTENCE_SPLIT_RE.split(clean_keep_lists(message))]
    for idx, s in enumerate(sents):
        if not s or QUESTION_RE.search(s) or ARM3_REPORTED_RE.search(s):
            continue
        note = None
        if NOTED_ONLY_RE.match(s):
            nxt = sents[idx + 1] if idx + 1 < len(sents) else ""
            if nxt and FUTURE_RE.search(nxt) and not QUESTION_RE.search(nxt):
                note = "%s %s" % (s, nxt)
        else:
            ml = NOTED_LEAD_RE.match(s)
            if ml and FUTURE_RE.search(ml.group(1)):
                note = s
        if note is None:
            mn = NOTES_CLAIM_RE.search(s)
            if mn and not (_before(ARM3_NEG_RE, s, mn.start()) or _before(ARM3_MODAL_RE, s, mn.start())
                           or _before(ARM3_SUBORD_RE, s, mn.start())):
                note = s
        if note is not None:
            out.append(("note", note[:300]))
            continue
        ml = FOLLOWUP_LIST_CLAIM_RE.search(s)
        own = _own_clause_before(s, ml.start()) if ml else ""
        if ml and not (ARM3_NEG_RE.search(own) or ARM3_MODAL_RE.search(own) or ARM3_SUBORD_RE.search(own)):
            out.append(("list", s[:300]))
            continue
        third = third_subject(s)
        kind = None
        for i, (conn, c) in enumerate(_arm3_clauses(s)):
            if conn in ("so", "which", "while"):
                continue
            m = ARM3_FIRST_RE.search(c) or ARM3_RULE_RE.search(c) or ARM3_PASSIVE_NOW_RE.search(c)
            if not m and not (i > 0 and third):
                m = ARM3_SUBJECTLESS_RE.search(c) or ARM3_PASSIVE_RE.search(c)
                # A short label in front ("First question: it is written in
                # the three places I read") hides the clause's real start.
                lab = ARM3_LABEL_RE.match(c)
                if not m and lab:
                    c = c[lab.end():]
                    m = ARM3_SUBJECTLESS_RE.search(c) or ARM3_PASSIVE_RE.search(c)
            if not m:
                continue
            pos = m.start()
            if (_before(ARM3_NEG_RE, c, pos) or _before(ARM3_MODAL_RE, c, pos)
                    or _before(ARM3_SUBORD_RE, c, pos) or ARM3_HABITUAL_RE.search(c)):
                continue
            if re.search(r"record", m.group(0), re.I) and ARM3_MEDIA_RE.search(s):
                continue
            if ARM3_SAVED_QTY_RE.search(c) or ARM3_LOGGED_IN_RE.search(c):
                continue
            if (ARM3_CONFESS_RE.search(c[m.start():]) or ARM3_OTHERS_SAVED_RE.search(c)
                    or ARM3_DESCRIBES_RE.search(s)):
                continue
            # The tense is read from the clause, never the sentence: "I've
            # recorded Quint's workspace as merging: his record is already
            # merged" is an act of this turn. "Now" makes it an act; a past
            # marker ("the thing I recorded an hour ago") makes it a reference
            # to an earlier turn, which this arm does not judge; "already", or
            # a passive that points at a place, makes it a citation.
            stop = re.search(r"[:—–;(]", c[m.end():])
            seg = c[:m.end() + stop.start()] if stop else c
            if ARM3_NOW_RE.search(seg):
                k = "act"
            elif ARM3_PAST_RE.search(seg):
                continue
            elif ARM3_ALREADY_RE.search(seg):
                k = "state"
            elif m.re is ARM3_PASSIVE_RE and ARM3_CITES_RE.match(c, m.end()):
                k = "state"
            else:
                k = "act"
            kind = "act" if (kind == "act" or k == "act") else "state"
        if kind and PRIVATE_NOTES_RE.search(s) and not RULE_NOUN_RE.search(s):
            # "Saved it in my private notes" is a note claim like "Noted.":
            # saying "private" no longer excuses a memory-only write (CEO
            # 2026-10-21). A RULE keeps its own, stricter judgment.
            kind = "note"
        if kind:
            out.append((kind, s[:300]))
    return out


# --- what the turn actually wrote ------------------------------------------

MEMORY_DIR_RE = re.compile(r"/\.claude/projects/[^/]+/memory(?:/|$)")
SCRATCH_RE = re.compile(r"^(?:/tmp/|/private/tmp/|/private/var/folders/|/var/folders/|/dev/)|/scratchpad(?:/|$)")
WRITE_TOOLS = ("Write", "Edit", "MultiEdit", "NotebookEdit")
# A Bash command's write targets, by the shapes the lead actually uses:
# `cat > f <<EOF`, `>> f`, `tee [-a] f`, `sed -i … f`, python's open(p, "w").
# EVERY QUANTIFIER IS BOUNDED. A Bash command can carry a megabyte with no
# whitespace in it, and an unbounded `[^'"\s]+` path scan over that is
# quadratic: the first corpus replay sat on one 10 MB transcript for over ten
# minutes before the bounds went in.
# Not `>=`: awk's `NR>=593` is a comparison, and reading it as a write once
# produced a "file" named =593 that excused a false claim in the replay.
REDIRECT_RE = re.compile(r"(?<![0-9&<>|=!])>{1,2}(?!=)[ \t]{0,8}(['\"]?)([^\s'\";|&<>()=`]{1,400})\1")
TEE_RE = re.compile(r"\btee[ \t]+(?:-a[ \t]+)?(['\"]?)([^\s'\";|&<>()`]{1,400})\1")
CD_RE = re.compile(r"(?:^|[;&|\n(])[ \t]{0,8}cd[ \t]+(['\"]?)([^\s'\";|&<>()]{1,400})\1")
PY_PATH_RE = re.compile(r"(['\"])((?:/|~/)[^'\"\s]{1,400}?\.[A-Za-z0-9]{1,8})\1")
ASSIGN_RE = re.compile(r"(?:^|[;&|\n(]|\s)([A-Za-z_]\w{0,40})=(['\"]?)([^\s'\";|&<>()$]{1,400})\2")
VAR_RE = re.compile(r"\$\{([A-Za-z_]\w{0,40})\}|\$([A-Za-z_]\w{0,40})")
SED_PATH_RE = re.compile(r"\s((?:[\w.~-]{1,100}/){0,20}[\w-]{1,100}(?:\.[\w-]{1,40}){0,4}\."
                         r"(?:md|json|jsonl|txt|toml|ya?ml|sh|py|config))\b")
PY_WRITE_RE = re.compile(r"open\([^)]*['\"][wa]\+?['\"]|\.write_text\(|\.write\(|json\.dump\(")
SED_I_RE = re.compile(r"\bsed\s+-i\b")
COMMIT_RE = re.compile(r"\bgit\b(?:\s+-[Cc]\s+\S+)*\s+(?:commit|merge|cherry-pick|revert|am)\b(?![^;&|\n]*--dry-run)")
# Engine commands whose whole job is to write a durable record outside any
# repository: the escalation ledger, the exemption and acknowledgement logs,
# and the workspace registry ("I've recorded zach-sonnet-rest6's branch as
# waiting on zach-sonnet-rest7"). The reply says WHERE by naming the entry the
# command wrote: an id or a workspace name taken from the command line.
RECORD_SCRIPT_RE = re.compile(r"\b(?:escalate\.sh\s+(?:raise|ack)|ceo-ruled-exempt\.sh|stop-work-ack\.sh|"
                              r"inflight-ack\.sh|workspaces\.sh\s+(?:wait|merging|merge|land|discard|"
                              r"register|shelve|park|block|hold|note|start|started))\b")
RECORD_TOKEN_RE = re.compile(r"\b[A-Za-z0-9][A-Za-z0-9._-]{5,80}\b")
# The Executive Continuity System checkpoint: typed turn state in a private
# shadow store the CEO never reads. Like the memory directory, it is a private
# note, and a claim backed only by it must say so, or name it.
ECS_RE = re.compile(r"\becs\b[^\n;|&]{0,80}\bcheckpoint", re.I)
ECS_NAMED_RE = re.compile(r"\b(?:ECS|checkpoint|end-of-turn\s+record|continuity\s+(?:record|system))\b", re.I)
SHA_RE = re.compile(r"\b[0-9a-f]{7,40}\b")


def _resolve(path, base):
    path = os.path.expanduser(path)
    if not os.path.isabs(path):
        path = os.path.join(base or "/", path)
    path = os.path.normpath(path)
    return "/" + path.lstrip("/") if path.startswith("//") else path


def bash_writes(command, cwd):
    """{targets, committed, record_tokens, ecs} for one Bash command. Relative
    targets are resolved against the last `cd` before them — the 2026-10-02
    memory write was `cd …/memory && cat > feedback_….md`, and a reader that
    took the bare name would have filed it under the working directory."""
    targets = []
    # `M=…/memory; cat > "$M/feedback_….md"`: the 2026-09-08 memory write took
    # this shape, and a target that starts with `$` is a path once expanded.
    assigns = {m.group(1): m.group(3) for m in ASSIGN_RE.finditer(command)}

    def expand(t):
        return VAR_RE.sub(lambda v: assigns.get(v.group(1) or v.group(2), v.group(0)), t)

    command = expand(command)
    cds = [(m.start(), m.group(2)) for m in CD_RE.finditer(command)]

    def base_at(pos):
        b = cwd
        for p, d in cds:
            if p < pos:
                b = _resolve(d, b)
        return b

    for rx in (REDIRECT_RE, TEE_RE):
        for m in rx.finditer(command):
            t = m.group(2)
            if (t.startswith("&") or t in ("/dev/null", "-") or t.startswith("$")
                    or not re.search(r"[A-Za-z0-9]", os.path.basename(t))):
                continue
            targets.append(_resolve(t, base_at(m.start())))
    if PY_WRITE_RE.search(command) or SED_I_RE.search(command):
        for m in PY_PATH_RE.finditer(command):
            targets.append(_resolve(m.group(2), cwd))
        if SED_I_RE.search(command):
            for m in SED_PATH_RE.finditer(command):
                targets.append(_resolve(m.group(1), base_at(m.start())))
    tokens = []
    for m in RECORD_SCRIPT_RE.finditer(command):
        line = command[m.start():].split("\n", 1)[0]
        tokens.extend(t for t in RECORD_TOKEN_RE.findall(line[m.end() - m.start():])
                      if re.search(r"[-\d]", t) and not t.startswith("-"))
    return {"targets": targets, "committed": bool(COMMIT_RE.search(command)),
            "record_tokens": tokens, "record_script": bool(RECORD_SCRIPT_RE.search(command)),
            "ecs": bool(ECS_RE.search(command))}


def classify_target(path):
    """'memory' | 'scratch' | 'repo' | 'file'. A repository file is one that
    git would commit: inside a work tree and not ignored."""
    if MEMORY_DIR_RE.search(path):
        return "memory"
    # A git work tree is checked BEFORE the scratch prefixes: a repository that
    # happens to live under the temp directory is still a repository, and a
    # scratch file is never inside one.
    # The walk ends when the parent stops changing, never on a spelling of the
    # root: normpath keeps a leading "//", dirname("//") is "//", and a loop
    # that waited for "/" hung the first corpus replay on exactly that path.
    d = os.path.dirname(path)
    while True:
        if os.path.exists(os.path.join(d, ".git")):
            try:
                import subprocess
                r = subprocess.run(["git", "-C", d, "check-ignore", "-q", path],
                                   capture_output=True, timeout=5)
                return "file" if r.returncode == 0 else "repo"
            except Exception:
                return "repo"
        parent = os.path.dirname(d)
        if not parent or parent == d:
            return "scratch" if SCRATCH_RE.search(path) else "file"
        d = parent


APP_REPORT = "mcp__richos_work__report"


def _app_answer(block):
    """An app tool's JSON answer from its tool_result block, or {} when the
    call was refused or the answer is not a JSON object."""
    if not isinstance(block, dict) or block.get("is_error") is True:
        return {}
    content = block.get("content")
    if isinstance(content, list):
        content = "".join(x.get("text", "") for x in content if isinstance(x, dict))
    try:
        answer = json.loads(content)
    except (TypeError, ValueError):
        return {}
    return answer if isinstance(answer, dict) else {}


def turn_writes(tm, transcript, prompt_id, cwd):
    """What this turn wrote, from its own calls. None plus a reason when the
    turn cannot be read. turn-manifest.py no longer caps the transcript size
    (it finds the turn from the end of the file), so a long session is read
    like any other. The read happens only on a turn that made a claim."""
    if tm is None:
        return None, "turn-manifest.py could not be loaded"
    calls, results, _examined, err = tm.read_turn(transcript, prompt_id)
    if err:
        return None, err
    ids = {tid for tid, name in calls if name in WRITE_TOOLS or name in ("Bash", APP_REPORT)}
    inputs = {}
    if ids:
        try:
            with open(transcript, encoding="utf-8", errors="replace") as fh:
                for line in fh:
                    if "tool_use" not in line:
                        continue
                    try:
                        rec = json.loads(line)
                    except Exception:
                        continue
                    content = (rec.get("message") or {}).get("content")
                    if not isinstance(content, list):
                        continue
                    for b in content:
                        if isinstance(b, dict) and b.get("type") == "tool_use" and b.get("id") in ids:
                            inputs[b["id"]] = b.get("input") if isinstance(b.get("input"), dict) else {}
        except OSError as exc:
            return None, "the transcript could not be read (%s)" % exc
    w = {"targets": [], "committed": False, "record_script": False, "record_tokens": [],
         "ecs": False, "traffic": ""}
    traffic = []
    for tid, name in calls:
        inp = inputs.get(tid) or {}
        if name in WRITE_TOOLS:
            p = inp.get("file_path") or inp.get("notebook_path")
            if p:
                w["targets"].append(_resolve(str(p), cwd))
        elif name == "Bash":
            cmd = str(inp.get("command") or "")
            traffic.append(cmd)
            b = bash_writes(cmd, cwd)
            w["targets"].extend(b["targets"])
            w["committed"] = w["committed"] or b["committed"]
            w["record_script"] = w["record_script"] or b["record_script"]
            w["record_tokens"].extend(b["record_tokens"])
            w["ecs"] = w["ecs"] or b["ecs"]
        elif name in (APP_TEAM, APP_REPORT):
            # The app's own record tools, counted only by their confirmed
            # answer (nightly 74 walk on f7db19c03, Stu rows 141-169): a team
            # save writes team/<name>.md and answers with that path; a job
            # report answers recorded: true. A refused or unreadable answer
            # counts for nothing.
            answer = _app_answer(results.get(tid))
            if name == APP_TEAM and isinstance(answer.get("saved"), str):
                w["targets"].append(answer["saved"])
            elif name == APP_REPORT and answer.get("recorded") is True:
                w["record_script"] = True
                if isinstance(inp.get("obligation_id"), str):
                    w["record_tokens"].append(inp["obligation_id"])
        res = results.get(tid)
        if res is not None:
            traffic.append(tm.flatten(res.get("content")))
    seen = set()
    w["targets"] = [(p, classify_target(p)) for p in w["targets"] if not (p in seen or seen.add(p))]
    w["traffic"] = "\n".join(traffic)
    return w, None


LOCATION_RE = re.compile(
    r"(?:[\w.~-]{0,100}/[\w.@-]{1,100})"
    r"|\b[\w-]{1,100}(?:\.[\w-]{1,40}){0,4}\.(?:md|json|jsonl|txt|toml|ya?ml|sh|py|rs|kt|swift|config|log|html|csv)\b"
    r"|§\s*\d+"
    r"|\besc-\d{8}T\d{6}Z-[0-9a-f]{4,16}\b"
    r"|\b(?:row|entry|decision|ruling|section|item|point)\s+[A-Z]?\d+(?:\.\d+)*\b", re.I)


def _named(path, raw):
    low = raw.lower()
    base = os.path.basename(path).lower()
    stem = os.path.splitext(base)[0]
    if len(base) < 4 or not re.search(r"[a-z0-9]", base):
        return False
    if base in low:
        return True
    return len(stem) >= 4 and re.search(r"(?<![\w-])" + re.escape(stem) + r"(?![\w-])", low) is not None


def raw_window(raw, sentence):
    """The raw text a claim sentence came from: its own line, plus the list
    its colon introduces. The cleaned sentence lost its code spans (two spaces
    where each was), so the line is found by the words before the first gap."""
    probe = re.split(r"\s{2,}", sentence.strip())[0][:60].strip()
    if len(probe) < 8:
        probe = sentence.strip()[:30]
    lines = raw.split("\n")
    for i, line in enumerate(lines):
        if probe and probe in EMPH_RE.sub("", line):
            out = [line]
            if line.rstrip().endswith(":"):
                for nxt in lines[i + 1:i + 12]:
                    if not nxt.strip() and len(out) > 1:
                        break
                    out.append(nxt)
            return "\n".join(out)
    return sentence


def note_window(raw, sentence):
    """The paragraph a note claim sits in: the file and commit that back a
    claim are named WITH it, never in another paragraph of the reply."""
    first = re.split(r"\s{2,}", sentence.strip())[0]
    probes = [first[:40].strip(), re.split(r"[(`]", first)[0][:40].strip()]
    if ". " in first:
        probes.append(first.split(". ", 1)[1][:40].strip())
    for probe in probes:
        if len(probe) >= 8:
            for para in re.split(r"\n\s*\n", raw):
                if probe in EMPH_RE.sub("", para).replace("`", ""):
                    return para
    return sentence


GIT_ADD_RE = re.compile(r"\bgit\s+(?:-C\s+\S+\s+)?add\b([^\n;&|]*)")
GIT_COMMIT_ALL_RE = re.compile(r"\bgit\s+(?:-C\s+\S+\s+)?commit\b[^\n;&|]*\s-[a-z]*a")


def _staged_in_commit(path, traffic):
    """True unless the turn staged explicit paths and this file is not one of
    them. Without a `git add` in view (commit -a, staging done elsewhere) the
    commit flag stands. `traffic` is lowercase."""
    adds = GIT_ADD_RE.findall(traffic)
    if not adds or GIT_COMMIT_ALL_RE.search(traffic):
        return True
    p = path.lower()
    for a in adds:
        toks = a.split()
        if any(t in ("-a", "--all", "-u", "--update") for t in toks):
            return True
        for t in toks:
            t = t.strip("'\"")
            if t.startswith("-") or not t:
                continue
            if t in (".", "./", "*") or t in p or p in t:
                return True
    return False


# AN EXISTING RECORD IS LOOKED UP, NOT RE-DEMANDED (orchestration reliability R6/A7,
# richos-hq docs/prds/2026-10-23-orchestration-reliability.md). A reply saying an item is
# already on the follow-up list and citing the commit that added it is checked against
# that commit, never alerted as "in no committed repository file". Two traps: code spans
# are stripped before claims are read, so the path and SHA must come from the raw reply;
# and a commit made in an earlier turn counts, not only one this turn's traffic carried.
#
# WHAT THE LOOKUP ESTABLISHES, AND WHAT IT DOES NOT. When the claim's paragraph names the
# follow-up list and cites a commit, the commit is resolved in the repositories the
# workspace registry knows (and the reply's own checkout). Observable facts only: the
# commit exists, it changed the follow-up list, and a line it added is still on the list.
# Those facts make the claim CITED: no alert. They do NOT establish that the line is this
# item (a reply citing an "Investigate X" row while saying "Delete X" is cited too); no
# wording is compared, by design, so nothing here is ever labeled verified. A cited commit
# that is absent, does not change the list, or whose added lines are all gone is NOT
# VERIFIED, said as such and never as "no record exists"; nothing cited and nothing written
# is the missing-record alert as before.
FOLLOWUPS_BASENAME = "follow-ups.md"


def _record_repos(cwd):
    """Repositories to resolve a cited commit in: the workspace registry's (repos.json,
    workspaces.state_dir's rule) and the checkout the reply was written in."""
    base = (os.environ.get("RICHOS_WORKSPACES_DIR") or "").strip() or os.path.join(
        (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude"),
        "state", "workspaces")
    repos = []
    try:
        with open(os.path.join(base, "repos.json"), encoding="utf-8") as fh:
            repos = [r for r in (json.load(fh) or {}).get("repos", []) if isinstance(r, str)]
    except (OSError, ValueError, AttributeError):
        pass
    return [r for r in dict.fromkeys(repos + ([cwd] if cwd else [])) if os.path.isdir(r)]


def existing_record(window, repos):
    """('cited', sha, repo) | ('unverified', sha, why) | None (the paragraph names no
    follow-up list or cites no commit). `window` is the RAW paragraph (code spans intact)."""
    import subprocess
    if FOLLOWUPS_BASENAME not in window:
        return None
    shas = [x for x in SHA_RE.findall(window.lower()) if re.search(r"[a-f]", x) and re.search(r"\d", x)]
    if not shas:
        return None
    why = "commit %s was not found in any repository the registry knows" % shas[0]
    for sha in shas:
        for repo in repos:
            def git(*args):
                r = subprocess.run(["git", "-C", repo] + list(args), capture_output=True, text=True, timeout=5)
                return r.returncode, r.stdout
            try:
                if git("cat-file", "-e", sha + "^{commit}")[0] != 0:
                    continue
                _rc, names = git("show", "--name-only", "--format=", sha)
                files = [n for n in names.splitlines() if n.endswith(FOLLOWUPS_BASENAME)]
                if not files:
                    why = "commit %s (in %s) does not change the follow-up list" % (sha, repo)
                    continue
                _rc, diff = git("show", "--format=", "--unified=0", sha, "--", files[0])
                added = [ln[1:] for ln in diff.splitlines() if ln.startswith("+") and not ln.startswith("+++")]
                rc, head = git("show", "HEAD:" + files[0])
                if rc == 0 and any(ln.strip() and ln in head.splitlines() for ln in added):
                    return "cited", sha, repo
                why = "commit %s (in %s) changed the follow-up list, but no line it added is still there" % (sha, repo)
            except (OSError, subprocess.SubprocessError):
                why = "commit %s could not be read" % sha
    return "unverified", shas[0], why


def judge_record_claims(claims, writes, raw, repos=(), cited_out=None):
    """[(sentence, code, problem)] for every claim this turn does not back.
    code is one of: nothing, private, where, sha, state, followup, unverified.
    Pure but for `repos`: claims from record_claims(), writes from turn_writes(),
    raw = the reply, repos = where a cited follow-up commit is looked up;
    `cited_out` collects the claims an earlier cited record backs."""
    failing = []
    low = raw.lower()
    concrete = bool(LOCATION_RE.search(raw))
    traffic = writes["traffic"].lower()
    # A SHA counts only when this turn's own traffic carries it: the commit
    # line, a push range, a log the turn read. A typed SHA is not evidence.
    cited = [s for s in SHA_RE.findall(low)
             if re.search(r"[a-f]", s) and re.search(r"\d", s) and s in traffic]
    targets = writes["targets"]
    durable = [(p, k) for p, k in targets if k in ("repo", "file")]
    memory = [p for p, k in targets if k == "memory"]
    named = [(p, k) for p, k in durable if _named(p, raw)]
    entry_named = any(t.lower() in low for t in writes.get("record_tokens") or [])

    def wrote():
        parts = []
        if durable:
            parts.append("to a record: " + ", ".join(p for p, _k in durable))
        if writes["committed"]:
            parts.append("a commit")
        if writes["record_script"]:
            parts.append("an entry through an engine record command")
        priv = [os.path.basename(p) for p in memory] + (["an ECS checkpoint"] if writes.get("ecs") else [])
        if priv:
            parts.append("privately only: " + ", ".join(priv))
        return "; ".join(parts) or "nothing"

    for kind, sentence in claims:
        if kind == "note":
            # A promise or follow-up said to be "noted", or "in my notes": only
            # a committed repository file the reply names backs it. The
            # memory directory and ECS do not.
            win = note_window(raw, sentence)
            wl = win.lower()
            here = [s for s in cited if s in wl]
            repo_named = [p for p, k in durable if k == "repo" and _named(p, win)]
            found = None if (here and writes["committed"] and any(_staged_in_commit(p, traffic)
                                                                 for p in repo_named)) \
                else (existing_record(win, repos) if repos else None)
            if found and found[0] == "cited":
                if cited_out is not None:
                    cited_out.append({"sentence": sentence, "commit": found[1], "repo": found[2]})
                continue
            if found:
                failing.append((sentence, "unverified", found[2]))
                continue
            if not (here and writes["committed"]
                    and any(_staged_in_commit(p, traffic) for p in repo_named)):
                failing.append((sentence, "followup", "a promise or follow-up is called noted or in the "
                                                      "notes, and this turn wrote %s. Only a committed "
                                                      "repository file the reply names, with its commit, "
                                                      "backs it" % wrote()))
            continue
        if kind == "list":
            # "On the follow-up list": only a committed change to the list itself.
            on_list = [p for p, k in durable if k == "repo"
                       and (p == FOLLOWUPS_FILE or p.endswith("/" + FOLLOWUPS_FILE))]
            if not (writes["committed"] and any(_staged_in_commit(p, traffic) for p in on_list)):
                found = existing_record(note_window(raw, sentence), repos) if repos else None
                if found and found[0] == "cited":
                    if cited_out is not None:
                        cited_out.append({"sentence": sentence, "commit": found[1], "repo": found[2]})
                    continue
                if found:
                    failing.append((sentence, "unverified", found[2]))
                    continue
                failing.append((sentence, "followup", "an item is said to be on the follow-up list, and "
                                                      "this turn wrote %s. Only a committed change to %s "
                                                      "backs it" % (wrote(), FOLLOWUPS_FILE)))
            continue
        if kind == "state":
            # A citation names its place where it is made, not somewhere else
            # in the reply: "it is written in the three places I read: the
            # rules file, your decisions page, and my memory" named no file,
            # and a path three paragraphs down does not change that.
            win = raw_window(raw, sentence)
            here = bool(LOCATION_RE.search(win)) or any(s in win.lower() for s in cited)
            if not (here or (writes["record_script"] and entry_named)):
                failing.append((sentence, "state", "it says the record already exists and names no "
                                                   "place for it: no path, no file, no section, no commit"))
            continue
        # A RULE, lesson or standing instruction is never a private note
        # (CEO 2026-10-02: "I've saved the rule in my private notes" ...
        # followed by nothing): saying "private notes" does not excuse it when
        # the turn's only writes are the memory directory. Memory holds no
        # rules; a rule is a line in CLAUDE.md or the decisions record.
        if memory and PRIVATE_NOTES_RE.search(sentence) and RULE_NOUN_RE.search(sentence) \
                and not (durable or writes["committed"] or writes["record_script"]):
            failing.append((sentence, "rule", "the reply says a rule or lesson was saved, and this turn "
                                                 "wrote only %s. A private note is not a rule: put it in "
                                                 "CLAUDE.md or richos-hq/wiki/ceo-decisions.md, committed, "
                                                 "in this turn" % wrote()))
            continue
        # Said plainly to be private, and it is: that is the truth, told.
        if (memory and PRIVATE_NOTES_RE.search(sentence)) or \
                (writes.get("ecs") and ECS_NAMED_RE.search(sentence)):
            continue
        # The claim itself points at the memory directory ("saved this as a
        # rule in my notes", "recorded in memory as standing doctrine"): that
        # is where it is, whatever else the turn wrote, and it must be said in
        # plain words. Measured: the CEO answered the first with "so what???".
        if NOTES_POINTER_RE.search(sentence):
            failing.append((sentence, "private", "the claim points at your notes or memory, which is "
                                                 "%s, and does not say \"in my private notes\". A "
                                                 "private note is not a record and not a rule"
                            % ("what this turn wrote there: " + ", ".join(os.path.basename(p) for p in memory)
                               if memory else "a place this turn did not even write")))
            continue
        if named:
            if any(k == "repo" for _p, k in named) and not cited:
                failing.append((sentence, "sha", "it names %s, a repository file, and no commit this "
                                                 "turn produced; commit it and give the SHA"
                                % ", ".join(os.path.basename(p) for p, _k in named)))
            continue
        if writes["record_script"] and entry_named:
            continue
        # A commit this turn produced or read, cited in the reply alongside a
        # place: the reply says where, which is the standard. (A claim that
        # points at "my notes" was judged above and never reaches this line,
        # so an unrelated commit elsewhere cannot excuse a note.)
        if cited and (concrete or durable or writes["committed"]):
            continue
        if durable or writes["committed"] or writes["record_script"]:
            if writes["committed"] and concrete and not cited:
                failing.append((sentence, "sha", "it names a place and no commit this turn produced; "
                                                 "give the SHA. This turn wrote %s" % wrote()))
            else:
                failing.append((sentence, "where", "the reply does not say WHERE. This turn wrote %s, "
                                                   "and the reply names none of it" % wrote()))
            continue
        if memory or writes.get("ecs"):
            failing.append((sentence, "private", "this turn wrote %s, and the reply does not say "
                                                 "\"in my private notes\". A private note is not a "
                                                 "record and not a rule" % wrote()))
            continue
        failing.append((sentence, "nothing", "this turn wrote NOTHING to any record: no Write, no "
                                             "Edit, no file written by a command, no commit"))
    return failing


def record_claim_verdict(payload, message):
    """None when the reply claims no record. Otherwise a dict:
         claims   [(kind, sentence)]
         failing  [(sentence, problem)] — non-empty means REFUSE
         written  [(path, class)] this turn wrote, for the refusal
         verdict  'backed' | 'refused' | 'unscoped' | 'unavailable: …' |
                  'cited, content not checked' (only cited earlier records back the claims:
                  their commit and file are checked, their wording is not)
         cited    [{sentence, commit, repo}] for those
    Every failure to READ the turn lets it end (the arm fails open like its
    siblings); a turn that was read and does not back its claim is refused."""
    try:
        claims = record_claims(message)
    except Exception:
        return None
    if not claims:
        return None
    v = {"claims": claims, "failing": [], "written": [], "verdict": "backed"}
    prompt_id = payload.get("prompt_id")
    if not prompt_id:
        v["verdict"] = "unscoped"
        return v
    tm = _load("turn_manifest_arm3", "turn-manifest.py")
    cwd = payload.get("cwd") or os.getcwd()
    try:
        writes, err = turn_writes(tm, payload.get("transcript_path"), prompt_id, cwd)
    except Exception as exc:
        writes, err = None, "the turn could not be read (%s)" % exc
    if writes is None:
        v["verdict"] = "unavailable: " + str(err)
        return v
    v["written"] = writes["targets"]
    cited = []
    v["failing"] = judge_record_claims(claims, writes, message, _record_repos(cwd), cited)
    if cited:
        v["cited"] = cited
    if v["failing"]:
        v["verdict"] = "refused"
    elif cited:
        v["verdict"] = "cited, content not checked"
    return v


def _arm3_log(payload, v):
    try:
        root = os.environ.get("RICHOS_SA_ENTITY_ROOT") or payload.get("cwd") or os.getcwd()
        state = os.path.join(root, ".claude", "state")
        os.makedirs(state, exist_ok=True)
        with open(os.path.join(state, "stated-actions.jsonl"), "a", encoding="utf-8") as f:
            f.write(json.dumps({"session": payload.get("session_id"), "prompt_id": payload.get("prompt_id"),
                                "refire": True, "arm3": {"claims": len(v["claims"]),
                                                         "failing": len(v["failing"]),
                                                         "verdict": v["verdict"],
                                                         "cited": [c["commit"] for c in v.get("cited", [])]}}) + "\n")
    except Exception:
        pass


ALERT_CODES = ("nothing", "private", "rule", "followup", "unverified")


def unbacked_claims(v):
    """The claims the turn wrote nothing for: no durable write at all
    ("nothing"), or only a private note the reply never called one
    ("private"). A claim backed by a durable write is never alerted on, whether
    or not the reply says where or cites a commit; a citation of an existing
    record ("state") is not an act of this turn."""
    if not v or v.get("verdict") != "refused":
        return []
    return [(s, c, p) for s, c, p in v["failing"] if c in ALERT_CODES]


def arm3_alert_line(v):
    """THE ALERT: one line (the host prefixes every line of a notice), never a
    refusal. Names the claim and says nothing was written. '' when none."""
    bad = unbacked_claims(v)
    if not bad:
        return ""
    parts = []
    for sentence, code, _p in bad[:3]:  # noqa: B007 (_p: the unverified alert quotes it)
        one = " ".join(sentence.split())[:160]
        if code == "rule":
            what = ("this turn wrote only a private memory note, and a rule is never a private note: "
                    "memory holds no rules")
            rule_tail = (". Lead: put the rule in CLAUDE.md or richos-hq/wiki/ceo-decisions.md, "
                         "committed, in this turn")
        elif code == "followup":
            what = ("a promise or follow-up called noted, or said to be on the follow-up list, is in no "
                    "committed repository file the reply "
                    "names (memory and ECS do not count). Lead: write it to %s, commit it, and name "
                    "the file and the commit" % FOLLOWUPS_FILE)
        elif code == "unverified":
            what = ("the record it cites is NOT VERIFIED: %s (not a finding that no record exists). "
                    "Lead: check the commit you cited" % _p)
        elif code == "private":
            what = ("this turn wrote only a private memory note, and the reply never said "
                    "\"in my private notes\"")
        else:
            what = "this turn wrote NOTHING to any durable record"
        parts.append("the reply says: \"%s\" and %s" % (one.replace('"', "'"), what))
    more = " (+%d more)" % (len(bad) - 3) if len(bad) > 3 else ""
    if any(c == "rule" for _s, c, _p in bad[:3]):
        return "RECORD ALERT: " + " | ".join(parts) + more + rule_tail + "."
    if all(c == "unverified" for _s, c, _p in bad[:3]):
        return "RECORD ALERT: " + " | ".join(parts) + more + "."      # never a demand to write it again
    return ("RECORD ALERT: " + " | ".join(parts) + more +
            ". Lead: write it to the record now (a repository file, committed), or say it was not recorded.")


# --------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------

ALERT_PREFIX = "RICHOS_RECORD_ALERT\t"   # stdout line the hook wrapper turns into a notice


def emit_alert(arm3):
    """Print the alert line for the wrapper. Never blocks, never changes an
    exit code: ARM 3 is an alert, not a refusal."""
    line = arm3_alert_line(arm3)
    if line:
        sys.stdout.write(ALERT_PREFIX + line + "\n")
    return bool(line)


def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0

    message = payload.get("last_assistant_message") or ""
    if not message.strip():
        return 0

    # ARM 3 is an ALERT (CEO 2026-10-02: "check whether or not a 'recorded'
    # claim was followed by the appropriate action and then issue an alert if
    # it wasn't"). It never refuses and never blocks. It runs on the re-fire
    # too, because the re-fire carries the final reply, which is the one that
    # reaches him.
    arm3 = record_claim_verdict(payload, message)
    if payload.get("stop_hook_active"):
        if arm3 is not None:
            _arm3_log(payload, arm3)
            emit_alert(arm3)
        return 0

    entity_root = (os.environ.get("RICHOS_SA_ENTITY_ROOT") or payload.get("cwd") or os.getcwd())
    teams_dir = os.environ.get("RICHOS_SA_TEAMS_DIR") or os.path.expanduser("~/.claude/teams")
    enforce = os.environ.get("RICHOS_SA_ENFORCE", "1") != "0"
    session_id = payload.get("session_id") or ""
    prompt_id = payload.get("prompt_id")
    transcript = payload.get("transcript_path")

    roles = roster_roles(entity_root, teams_dir, session_id)
    claims = role_act_claims(message, roles) + dispatch_claims(message, roles)

    record = {
        "session": session_id,
        "prompt_id": prompt_id,
        "roles": len(roles),
        "claims": [{"who": w, "arm": a} for w, _c, a in claims],
        "unmet": [],
        "reported": [],
        "stop": None,
        "scoped": bool(prompt_id),
        "verdict": "pass",
    }

    def log():
        try:
            state = os.path.join(entity_root, ".claude", "state")
            os.makedirs(state, exist_ok=True)
            with open(os.path.join(state, "stated-actions.jsonl"), "a", encoding="utf-8") as f:
                f.write(json.dumps(record) + "\n")
        except Exception:
            pass

    # Without a prompt_id the turn cannot be scoped. The wide answer would be
    # a FALSE one (attributing the session's calls to this turn) and the
    # narrow one would manufacture a refusal. Say nothing; record it.
    if not prompt_id:
        record["verdict"] = "unscoped"
        log()
        return 0

    # --- ARM 1 -------------------------------------------------------------
    unmet, reported = [], []
    calls = []
    if claims and ends_on_ceo(message):
        # The reply's last sentence puts something to the CEO. A plan stated
        # above a question is a proposal, and the turn may end on his answer
        # — the same term AskUserQuestion carries, read from the reply.
        # Measured: "Art reads the loro convos himself ... Want me to post
        # that prompt before it goes?" — a plan awaiting his word.
        record["arm1"] = "ceo-proposal"
        claims = []
    if claims:
        tm = _load("turn_manifest", "turn-manifest.py")
        calls, inputs, err = turn_calls(tm, transcript, prompt_id)
        if err:
            record["arm1"] = "unavailable: " + err
        else:
            for who, clause, arm in claims:
                if satisfied(who, arm, calls, inputs):
                    continue
                (reported if arm == "role-future" else unmet).append((who, clause, arm))
    record["unmet"] = [{"who": w, "arm": a} for w, _c, a in unmet]
    record["reported"] = [{"who": w, "arm": a} for w, _c, a in reported]
    record["calls"] = [n for _t, n in calls]

    # --- ARM 2 -------------------------------------------------------------
    idle = _load("guard_idle_land", "guard-idle-land.py")
    stop = stop_after_completion(idle, transcript, prompt_id, message)
    if stop is not None:
        record["stop"] = {"finishes": stop["finishes"], "verdict": stop["verdict"]}
        if stop["verdict"] == "closed":
            # what discharged it, including any completion whose owner was not read
            record["stop"]["detail"] = str(stop["detail"])[:500]
        if stop["verdict"] == "declared":
            d = stop["detail"]
            record["stop"]["declared"] = d.get("case")
            # The wrapper shows a declared stop to the operator, always — a
            # justification filed where nobody reads it is a flag with a
            # longer spelling. Same line shape as guard-idle-land.sh reads.
            sys.stdout.write("RICHOS_STOP_DECLARED\t%s\t%s\t%s\n"
                             % (d.get("case"), d.get("why", ""), stop["finishes"][0]))

    undeclared = stop is not None and stop["verdict"] == "undeclared-stop"

    if arm3 is not None:
        record["arm3"] = {"claims": len(arm3["claims"]), "failing": len(arm3["failing"]),
                          "verdict": arm3["verdict"], "cited": [c["commit"] for c in arm3.get("cited", [])]}
    # The alert is spoken once, on the reply that ends the turn: when ARM 1 or 2
    # is about to refuse this reply, the re-fire carries the one that counts.
    if arm3 is not None and not unmet and not undeclared:
        emit_alert(arm3)

    if not unmet and not undeclared:
        if reported:
            record["verdict"] = "report"
            log()
            lines = ["=== stated-action check: PASSED, with an observation (not blocking) ==="]
            for who, clause, _arm in reported:
                lines.append("  a role is said to be about to act, and this turn called no Agent for it:")
                lines.append("    \"%s\"" % clause)
            lines.append("  (this arm measured 0 true in 2 fires on the corpus -- logged, never enforced)")
            lines.append("  record: .claude/state/stated-actions.jsonl")
            sys.stderr.write("\n".join(lines) + "\n")
        else:
            log()
        return 0

    record["verdict"] = "block" if enforce else "report-only"
    log()

    tally = {}
    for _t, n in calls:
        tally[n] = tally.get(n, 0) + 1
    did = ", ".join("%s x%d" % (k, v) for k, v in tally.items()) or "nothing at all"

    out = ["=== THE REPORT DOES NOT MATCH THE TURN — TURN BLOCKED ==="]
    for who, clause, arm in unmet:
        out.append("")
        if arm == "role-act":
            out.append("  You wrote, as a plain statement of what happens:")
        else:
            out.append("  You wrote, in the first person, that you are dispatching:")
        out.append("")
        out.append("      \"%s\"" % clause)
        out.append("")
        out.append("  and this turn's tool calls contain no Agent call for %s and no message" % who)
        out.append("  to a teammate. What the turn actually called: %s." % did)
        out.append("")
        out.append("  A sentence describing an act is not the act. Seven times on 2026-09-02")
        out.append("  the report was written from the intention, and twice the CEO had to ask")
        out.append("  where the teammate was. Either")
        out.append("      make the Agent call now, in this turn, and then finish; or")
        out.append("      write what is TRUE: the dispatch has not happened, and why, or the")
        out.append("      choice it waits on, put to the CEO as a question.")
        out.append("  The sentence in the report must describe what was done.")
    if undeclared:
        out.append("")
        out.append("  A TEAMMATE FINISHED THIS TURN and the turn is ending having started nothing:")
        for t in stop["finishes"]:
            out.append("      Agent \"%s\" finished" % t)
        out.append("")
        out.append("  No Agent call, no backgrounded command, no question to the CEO, and no")
        out.append("  declaration. (His own words are NOT read here and never will be — if he")
        out.append("  stopped this turn, say so yourself in a `stop-declared: ceo-owns-it`")
        out.append("  line.) The report IS the stopping — and six")
        out.append("  times on 2026-09-02 he had to send \"No Frank this time?\" to restart work")
        out.append("  that should never have paused. guard-idle-land let those turns through")
        out.append("  because its backlog had no free row; the next step after a returned")
        out.append("  design is not a backlog row, and that is why this arm asks YOU instead.")
        out.append("")
        out.append("  Either START the next thing now (an Agent call in this turn), or DECLARE")
        out.append("  the stop, in the reply, in this exact form:")
        out.append("")
        out.append("           stop-declared: <case> — <why, in a full sentence>")
        out.append("")
        cases = getattr(idle, "DECLARED_CASES", {}) if idle else {}
        for name in sorted(cases):
            out.append("           %-20s %s" % (name, cases[name]))
        out.append("")
        out.append("  At least %d words and %d characters of reason. A bare marker exempts"
                   % (getattr(idle, "MIN_DECLARATION_WORDS", 6), getattr(idle, "MIN_DECLARATION_CHARS", 30)))
        out.append("  nothing, and the declaration is SHOWN to the CEO, unverified, every time.")
        problem = (stop.get("detail") or {}).get("problem") if isinstance(stop.get("detail"), dict) else ""
        if problem:
            out.append("")
            out.append("  A declaration was present and REJECTED: %s" % problem)
    out.append("")
    out.append("  Arms 1 and 2 stand down on the re-fire, so they refuse a turn at most")
    out.append("  once. Fix the reply and finish the turn. Do not weaken or unwire this hook.")
    out.append("(hook: scripts/hooks/ass-kicker.sh)")
    sys.stderr.write("\n".join(out) + "\n")
    return 2 if enforce else 0


if __name__ == "__main__":
    sys.exit(main())
