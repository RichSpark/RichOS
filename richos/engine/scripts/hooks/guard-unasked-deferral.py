#!/usr/bin/env python3
"""guard-unasked-deferral.py — the analysis half of the UNASKED-DEFERRAL notice.

===========================================================================
THE DEFECT, WITH ITS SPECIMEN
===========================================================================
2026-08-31. The CEO asked, twice, when a durable fix for a defect would be
installed and running. The orchestrator's answer the second time, verbatim from
its own turn (transcript 042f3850, `final_text`):

    "I'm deliberately not spawning a fifth agent for it right now. Four are
     running, you're waiting to restart, and adding a fifth pushes that further
     out for a guard that isn't the thing you're waiting on. It goes in with
     the dialect guard's land — next spawn after these finish."

Every clause of that reasoning was the orchestrator's; every consequence of it
was the CEO's. He was not asked. He was INFORMED OF A DECISION ALREADY TAKEN,
and had to ask a third time, angrily, before the work started. Third occurrence
that day.

THE SIN IS NOT DEFERRING. Deferral is often right — sequencing, collisions, a
genuinely better order. The sin is deferring SILENTLY AND UNILATERALLY:
converting a decision that belongs to the CEO into a plan announced to him.

This is the CONVERSE of the CEO-ask gate (guard-ceo-ask-first.sh /
notice-ceo-unasked.sh), and is not covered by it. That pair ensures a PREPARED
question actually gets put to him. This one covers the case where there is no
ambiguity about what he wants — he asked directly — and the orchestrator
schedules it for later on its own authority. Nothing checked for that.

===========================================================================
WHY IT KEYS ON THE ORCHESTRATOR'S OWN LANGUAGE, NOT THE CEO'S INTENT
===========================================================================
Deciding whether a CEO message "was a request" is a hard classification with no
good answer, and a guard built on it would be noise. So this one never looks at
his message at all.

It keys on the ORCHESTRATOR'S OWN DEFERRAL LANGUAGE, which is a closed, listable
set of constructions, is the orchestrator's prose rather than his, and is exactly
what appears when the defect occurs.

===========================================================================
THE MEASUREMENT, AND THE CONSTRUCTIONS REFUSED BECAUSE OF IT
===========================================================================
CORPUS: 2,198 real orchestrator turns — every non-sidechain turn in the five
main-session transcript directories on this machine (femcboost, richos,
richos-engine, prospects, deeply), final assistant text plus the tool names used
in that turn, which is exactly what a Stop hook can see. The specimen turn is in
it. Extraction and scoring: scripts/hooks/unasked-deferral.corpus.md records the
method so the number can be reproduced rather than believed.

SHIPPED SET — 3 families, 6 patterns. Over the corpus:

    construction matched          13 turns
      discharged, AskUserQuestion  0
      discharged, CEO's hand        9
      discharged, Agent spawn       1
    FIRES                           3   — all 3 genuine unilateral deferrals

PRECISION 3/3 = 100%, n = 3. THE SMALL n IS THE POINT AND NOT A BOAST: this
defect is rare in the record and loud when it happens, so the only useful
tuning target was zero false alarms. The number that was actually worked for is
the REFUSED list below.

RECALL IS THE HONEST WEAK HALF. Hand-reading every construction match found 5
turns I judge genuine unilateral deferrals; it fires on 3. One is lost to the
Agent-spawn discharge (below), one to the `postponing-it` family being dropped.
A guard that catches three in five of an unnoticed habit is worth having; a
guard claiming to catch all five would be lying.

REFUSED, WITH THE MEASURED REASON — every one of these was proposed, run over
the same corpus, hand-read, and dropped:

  "deliberately not"  (bare)            23 hits, 4 genuine (17%). The dominant
                                        use is a wiki heading, "Deliberately NOT
                                        open", and "text deliberately not meant
                                        to be read". Worse than the 10.3% rule
                                        this engine already rejected once. Kept
                                        only as an optional adverb INSIDE the
                                        first-person patterns.
  "I'll X once Y completes"              9 hits, 0 genuine. It is progress
                                        reporting — "I'll land Mark's branch
                                        once it comes back green".
  "when it's done" / "when they're done" 12 hits, 0 genuine. Every single one is
                                        "I'll report when it's done".
  "after these/those land"              14 hits, 3 genuine (21%). Overwhelmingly
                                        roadmap narration — "After those land:
                                        wave 2 is Mark aligning validators".
                                        This is the brief's own example of
                                        pipeline prose that must not fire. Kept
                                        ONLY inside "next <spawn|pass|...>
                                        after", which is 3 hits and precise.
  "wait until" / "waiting for"          39 hits. Mostly the CEO waiting, a
                                        process waiting, or a run waiting.
  "queued / parked / deferred"          32 hits. Mostly nouns describing a state
                                        the CEO had already ruled on.
  "I'm holding/queueing/parking <X>"     9 hits, 4 genuine (44%) — and this one
                                        was WRITTEN, MEASURED AND THEN DELETED.
                                        Its false alarms are pipeline holds that
                                        are plainly correct ("I'm holding the
                                        merge until all three report", "I'm
                                        holding the land until his tests confirm
                                        green"), and 3 of its 4 genuine hits are
                                        already caught by chose-not-to-start-now.
                                        It bought one extra catch for a false
                                        alarm on ordinary sequencing. Dropped.
  "in the same commit/merge as"          1 hit, 0 genuine — "the off switch ships
                                        in the same commit as the splash",
                                        describing a bundle that already exists.
                                        `commit`/`merge` dropped from that
                                        family; `pass`/`land`/`spawn`/`round`
                                        kept.
  "rather than <gerund>"  (bare)         6 hits, 3 genuine. The others are design
                                        choices — "extending the CSV rather than
                                        adding a second ledger". Kept ONLY with a
                                        first-person subject AND a now-marker,
                                        which is what separates a postponement
                                        from a choice.
  "X goes in with Y"  (bare)             3 hits, 1 genuine. "the fix goes in with
                                        the next pilot restart" was describing a
                                        checklist inside a document. Kept ONLY
                                        when the object is a unit of
                                        orchestration work — a land, pass, spawn,
                                        batch, round, merge, deploy or commit.

A SHORT PRECISE LIST BEATS A LONG NOISY ONE, and the refused list is deliberately
longer than the shipped one.

===========================================================================
QUOTED IS NOT USED
===========================================================================
Backtick spans, fenced blocks and double-quoted spans are stripped before
matching. This is not a convenience: two of the corpus's construction hits are
the orchestrator DESCRIBING THIS VERY GUARD to the CEO and quoting its trigger
phrases back at him. A guard that fires on its own documentation is a guard that
gets switched off. Stripping quotations is also the honest reading — a
construction being quoted is not a construction being used.

===========================================================================
WHAT DISCHARGES A DEFERRAL
===========================================================================
Per the rule, three things, checked in this order:

  1. AskUserQuestion in the same turn — the deferral was put to him as a choice.
  2. THE CEO'S HAND IS VISIBLE IN THE TEXT — either the turn hands the decision
     back in prose ("your call", "say the word", "want me to?"), or it cites his
     instruction ("you killed it", "those are orders", "until you rule").
     This is the approximation of "an explicit CEO instruction to defer, already
     on the record", and it is the single highest-value discharge in the corpus:
     of the 13 turns matching a construction, 9 carry it, and reading all 9
     confirms every one either offers him the choice or obeys an order he gave
     ("You killed the gate judgment, so I'm not spawning anything further").
     Without this discharge the guard would fire 12 times at 4/12 = 33%.
  3. An Agent spawn in the same turn. THIS IS THE KNOWN BLIND SPOT and it is
     stated here rather than engineered around: the rule wants "a spawn that
     starts the deferred thing", and a Stop hook cannot tell which thing a spawn
     starts. So a turn that spawns one agent while unilaterally deferring
     another passes. MEASURED COST: exactly one genuine deferral in the corpus
     — "Understood, the PR is authorized. I'm sequencing it deliberately rather
     than firing it off now" — is silenced by it, because that turn spawned an
     unrelated agent. The specimen turn does not spawn (tools: Bash only).

===========================================================================
THE LIMIT, STATED RATHER THAN ENGINEERED AROUND
===========================================================================
An orchestrator can phrase around any word list, and this one is 3 families
wide. THIS GUARD CATCHES THE NATURAL PHRASING OF AN UNNOTICED HABIT, which is
the actual failure mode — the specimen was written without any awareness that it
was taking the CEO's decision. It does not and cannot catch deliberate evasion.
Pretending otherwise would make it a checkbox.

REPORTS, NEVER BLOCKS. The harm is a decision already announced; the value is
that the CEO sees it named in the same turn instead of having to ask a third
time. A blocking guard on PROSE would wedge the session and get switched off,
which is the failure mode that produced this in the first place.

===========================================================================
FAMILY D, AND WHY A MATCH IS ONLY A QUESTION TO THE LEAD (2026-10-24)
===========================================================================
On 2026-09-27 15:30Z a reply said "Stream B is on hold, deliberately.", no
pattern matched it, and his order sat held for weeks. So family D adds the hold
words (scripts/lib/hold_words.py): "on hold", "hold off", "parked", "until I
say", "wait for my go". A reply that says the work is held cannot be starting
it, so an Agent spawn in the same turn does not discharge family D.

A text match is not evidence that authorized unfinished work exists: on
2026-10-23 "Then I parked everything behind the nightly and the stable." (a
retrospective explanation) became a durable hold, and denials became startup
stalls (richos-hq docs/prds/2026-10-23-orchestration-reliability.md, R1). So a
finding opens nothing. It goes to the LEAD once per quote, through the Stop
hook's additionalContext (the model channel; a systemMessage reaches only the
person), so the lead checks the original request and records the real work
(`workspaces.sh hold <name> --what ... --until <end>`), starts it, puts it to
him, or drops the match. The continuation that answers it is never read again
by this detector: a diagnostic and its follow-up are not fresh input.
"""

import json
import os
import re
import sys
import types

# ---------------------------------------------------------------------------
# QUOTATION STRIPPING — see "QUOTED IS NOT USED" above.
# ---------------------------------------------------------------------------
_FENCE = re.compile(r"```.*?```", re.S)
_TICK = re.compile(r"`[^`\n]*`")
# Straight and curly double quotes, single line only: an unbalanced quote must
# not swallow the rest of the message.
_DQUOTE = re.compile(r"[\"“][^\"“”\n]{0,400}[\"”]")


def strip_quoted(text):
    """Blank out quoted/technical spans, preserving offsets so match positions
    still point into the original string."""
    def blank(m):
        return " " * (m.end() - m.start())
    out = _FENCE.sub(blank, text)
    out = _TICK.sub(blank, out)
    out = _DQUOTE.sub(blank, out)
    return out


# ---------------------------------------------------------------------------
# THE CONSTRUCTIONS — three families, each measured on the corpus above.
# ---------------------------------------------------------------------------

# Verbs that name STARTING A PIECE OF WORK. Deliberately not "considering",
# "thinking", "assuming" — this family is about work not being begun.
_ACT = (r"(?:spawn|dispatch|start|launch|open|fir|build|writ|add|fix|land|do|"
        r"run|touch|merg|deploy|implement|wir|ship|creat|patch|chang)"
        r"(?:ing|e|ping|ning|ting|ding)?")

# A now-marker: the word that turns "I am not doing X" (a scope statement) into
# "I am not doing X YET" (a postponement). Without one, "I'm not adding a second
# ledger" is a design decision, and the corpus is full of those.
_NOW = (r"(?:right now|just now|now|yet|today|at the moment|for now|for the "
        r"moment|this turn|this pass|in this pass|immediately|before (?:that|"
        r"these|those))")

# An object that names no work, only "nothing more": "I'm not starting anything
# alongside that" (2026-10-23) says how much runs at once and holds no piece of
# work, so the hold it opened ("I'm not starting") could never end by that work
# running. "I'm not spawning anything for it" still names the work and still fires,
# and so does work named after the qualifier: "I'm not spawning anyone else for the
# sign-in review right now" (second review rv-20261023T204623Z-591cbe58-c2b4 #1). Only
# a clause that ENDS there, at most "that"/"it" and a now-marker later, names nothing.
_MORE = r"(?:else|new|alongside|in\s+parallel|on\s+the\s+side|on\s+top)"
_NOTHING_MORE = (r"(?!\s+(?:anything|anyone|anybody)(?:\s+" + _MORE + r")+"
                 r"(?:\s+(?:of\s+|with\s+|to\s+)?(?:that|this|it|them|those|these))?"
                 r"(?:\s+" + _NOW + r")?"
                 r"\s*(?:[.!?;:,)\n]|$|\s+(?:and|but|while|until|because|since|so|as)\b))")

CONSTRUCTIONS = [
    # ---------------------------------------------------------------- family A
    # FIRST-PERSON REFUSAL TO START WORK.
    #   "I'm deliberately not spawning a fifth agent for it right now"  <- specimen
    #   "I'm not fixing these right now"
    # The subject must be I/I'm — the corpus's false alarms ("that's me not
    # writing down what you said", "a partial not starting with the GGML magic")
    # all have a different subject.
    ("not-starting-it-now",
     r"\b(?:I'm|I am|I'll|I will)\s+(?:\w+\s+){0,3}?not\s+" + _ACT +
     r"\b" + _NOTHING_MORE + r"[^.\n]{0,80}?\b" + _NOW + r"\b"),

    # The same act, with the postponement carried by the verb instead of a
    # now-marker: "I'm not spawning a fourth agent", "I'm not dispatching
    # anything". Restricted to the DISPATCH verbs, because those are the ones
    # that can only mean "I am not starting this", and to a short window so a
    # long sentence cannot drag an unrelated negation in.
    ("not-dispatching",
     r"\b(?:I'm|I am)\s+(?:\w+\s+){0,2}?not\s+"
     r"(?:spawning|dispatching|launching|kicking off|starting)\b" + _NOTHING_MORE),

    # ---------------------------------------------------------------- family B
    # BUNDLING THE ITEM INTO A LATER BATCH — the specimen's second half.
    #   "It goes in with the dialect guard's land"
    #   "next spawn after these finish"
    # `commit`/`merge` are deliberately absent from the second pattern: the one
    # corpus hit for those was describing a bundle that already existed.
    ("bundled-into-later-batch",
     r"\b(?:it|that|this|which|the fix|the guard|the change)\s+"
     r"(?:goes|go|will go|can go|would go|rides|ships|lands)\s+in\s+with\b"
     r"[^.\n]{0,60}?\b(?:land|lands|landing|pass|spawn|batch|round|merge|"
     r"deploy|commit)\b"),
    ("bundled-into-later-batch",
     r"\bin\s+the\s+same\s+(?:pass|land|spawn|round|batch)\s+as\b"),
    ("next-batch-after-these",
     r"\bnext\s+(?:spawn|pass|round|land|batch|build)\s+after\b"),

    # ---------------------------------------------------------------- family C
    # AN EXPLICIT FIRST-PERSON POSTPONEMENT VERB.
    #   "I'm serializing it behind Norm rather than starting it now"
    #   "I'm sequencing it deliberately rather than firing it off now"
    ("chose-not-to-start-now",
     r"\b(?:I'm|I am)\s+[^.\n]{0,60}?\brather\s+than\s+" + _ACT +
     r"\b[^.\n]{0,40}?\b" + _NOW + r"\b"),
    ("id-rather-wait",
     r"\bI'd\s+rather\s+(?:wait|hold off|hold it|not start|not spawn|"
     r"not do (?:it|that|this))\b"),
]

_COMPILED = [(name, re.compile(pat, re.I)) for name, pat in CONSTRUCTIONS]

# ---------------------------------------------------------------- family D
# THE HOLD WORDS (see A DEFERRAL OPENS A HOLD above). One list, shared.
HELD = "held-in-a-reply"
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))
import hold_words  # noqa: E402
# The hold words a sentence does not deny: "Nothing of yours is on hold." holds nothing.
_COMPILED.append((HELD, types.SimpleNamespace(search=hold_words.find)))


def find_deferrals(text):
    """Return [(construction_name, matched_text), ...] for text, quotations
    stripped. Deduplicated by construction name — one fire per family."""
    if not text:
        return []
    scan = strip_quoted(text)
    seen = set()
    out = []
    for name, rx in _COMPILED:
        m = rx.search(scan)
        if m and name not in seen:
            seen.add(name)
            out.append((name, m.group(0).strip()))
    return out


# ---------------------------------------------------------------------------
# THE DISCHARGES
# ---------------------------------------------------------------------------
# THE CEO'S HAND, VISIBLE IN THE TEXT. Two shapes, both meaning the decision is
# his and he knows it:
#   HANDS IT BACK  — "your call", "say the word", "want me to?"
#   CITES HIS ORDER — "you killed it", "those are orders", "until you rule"
# Measured: 9 of the corpus's 21 construction-matching turns carry one, and all
# 9 read correctly on inspection.
_HANDBACK = re.compile(
    r"\b(?:"
    r"your call|your choice|your decision|your ruling|your say-so|"
    r"yours to call|yours to make|up to you|"
    r"say the word|"
    r"tell me (?:which|if|when|what|whether)|you'll tell me|"
    r"want me to\b|do you want|would you (?:like|rather|prefer)|"
    r"shall I\b|should I\b|"
    r"if you want|whenever you want|when you're ready|"
    r"until you (?:rule|say|decide|tell)|"
    r"the next move is yours|nothing (?:happens|moves) until you|"
    r"(?:name|pick|choose) (?:the|which|one)\b|"
    r"either way[, ][^.\n]{0,40}\byou\b|"
    r"say\s+[“\"][^\"”\n]{1,40}[\"”]\s+and\b"
    r")",
    re.I,
)


# HIS ORDER, CITED: the decision is already his and made, so it discharges the deferral
# and opens no hold. A hand-back (above) is a choice put to him and not yet made.
_HIS_ORDER = re.compile(r"\b(?:those are orders|you (?:killed|stopped|told me to stop))\b", re.I)


def discharge(text, tools, hits=()):
    """Return the name of the thing that discharges a deferral in this turn, or
    None if nothing does. A spawn never discharges a reply that says the work
    is held (family D): that work is the one not started."""
    if text and _HIS_ORDER.search(text):
        return "his-order"
    if "AskUserQuestion" in tools:
        return "askuserquestion"
    if text and _HANDBACK.search(text):
        return "handback"
    if "Agent" in tools and not any(h[0] == HELD for h in hits):
        return "agent-spawn"
    return None


def _sentence(text, scan, start, end):
    """The sentence (or line) of `text` around [start, end)."""
    a = max(scan.rfind(".", 0, start), scan.rfind("\n", 0, start)) + 1
    b = min([i for i in (scan.find(".", end), scan.find("\n", end)) if i >= 0] or [len(scan)])
    return " ".join(text[a:b + 1].split())


def held_pieces(text, hits):
    """[(construction, quote)]: one per held piece of work in the reply (second review
    rv-20261022T013056Z-12c77c90-1291, finding 3: "Spelling work is on hold. The sign-in fix
    is parked." holds two). Family D gives one per sentence holding work; the other families
    their own quote. The same sentence is one piece."""
    scan = strip_quoted(text)
    out, seen = [], set()
    for name, quoted in hits:
        quotes = [_sentence(text, scan, m.start(), m.end()) or quoted
                  for m in hold_words.held(scan)] if name == HELD else [quoted]
        for q in quotes:
            q = q if len(q) <= 120 else q[:117] + "..."
            if q not in seen:
                seen.add(q)
                out.append((name, q))
    return out


# ---------------------------------------------------------------------------
# HOOK ENTRY POINT
# ---------------------------------------------------------------------------
# Reads the Stop payload on stdin, writes a systemMessage notice to stdout if a
# deferral in this turn was never put to the CEO, and ALWAYS exits 0.
#
# The de-duplication ledger and the state-change semantics belong to
# scripts/lib/stop-hook-notice.sh, which is bash; the wrapper owns those. This
# module's only output is the sentence, on stdout, one line, or nothing.

MARKER = "DEFERRAL CHECK (unverified"
CONTINUED = "\0continued"


TAIL_CHUNK = 256 * 1024
TAIL_BUDGET = 4 * 1024 * 1024   # a turn whose opening prompt is further back is UNKNOWN, never guessed
_READ = {"bytes": 0}            # bytes read by the last _tools_of_turn (the suite bounds it)


def _walk_turn(lines):
    """(tools, reached_prompt) for the newest records in `lines`, walked backward
    to the real user prompt that opened this turn."""
    tools = set()
    # A call whose result is an error was refused or failed: a refused Agent
    # started nothing and discharges nothing (hunt part 3 v3, finding 19).
    # Walking backward, a call's result is seen before the call.
    refused = set()
    for line in reversed(lines):
        try:
            rec = json.loads(line)
        except Exception:
            continue
        if not isinstance(rec, dict) or rec.get("isSidechain"):
            continue
        att = rec.get("attachment") if rec.get("type") == "attachment" else None
        if isinstance(att, dict) and att.get("type") == "hook_additional_context" \
                and any(MARKER in str(c) for c in (att.get("content") or [])):
            tools.add(CONTINUED)     # this turn is the continuation of our own diagnostic
            continue
        msg = rec.get("message") or {}
        content = msg.get("content")
        if rec.get("type") == "user":
            if isinstance(content, list) and all(
                isinstance(b, dict) and b.get("type") == "tool_result"
                for b in content
            ):
                for b in content:
                    if b.get("is_error") is True and b.get("tool_use_id"):
                        refused.add(b["tool_use_id"])
                continue  # tool results are not a prompt
            return tools, True  # the real prompt that opened this turn
        if rec.get("type") == "assistant" and isinstance(content, list):
            for b in content:
                if isinstance(b, dict) and b.get("type") == "tool_use":
                    if b.get("id") and b["id"] in refused:
                        continue
                    tools.add(b.get("name") or "")
    return tools, False


def _tools_of_turn(payload):
    """The tool names used in the CURRENT turn.

    Read from the transcript, walking BACKWARD from the end to the last real
    user prompt. Verified in guard-unresolved-claims.sh's header against the
    shipping binary: at Stop time the transcript already holds this turn's
    tool_use records but NOT the final assistant text — which is why the text
    comes from `last_assistant_message` and only the tool traffic from here.

    ONLY THE CURRENT TURN IS READ (orchestration reliability R7: no full-history
    work in a routine Stop; the lead's transcript was 10.7 MB on 2026-10-24). The
    tail is read in a window that doubles from TAIL_CHUNK until it holds the
    prompt that opened the turn. A turn whose opening prompt is not found within
    TAIL_BUDGET, or not at all, is UNKNOWN (None): the honest not-checked path,
    never a tool set guessed from part of the history.
    """
    path = payload.get("transcript_path") or ""
    if not path or not os.path.isfile(path):
        return None  # UNKNOWN — never the same as "no tools used"
    _READ["bytes"] = 0
    try:
        with open(path, "rb") as fh:
            size = fh.seek(0, os.SEEK_END)
            window = TAIL_CHUNK
            while True:
                start = max(0, size - window)
                fh.seek(start)
                data = fh.read(size - start)
                _READ["bytes"] += len(data)
                lines = data.decode("utf-8", "replace").splitlines()
                if start:
                    lines = lines[1:]            # the first line is cut by the window
                tools, reached = _walk_turn(lines)
                if reached:
                    return tools
                if not start or window >= TAIL_BUDGET:
                    return None
                window = min(window * 2, TAIL_BUDGET)
    except Exception:
        return None


def main():
    # EXIT CODES — three of them now, and the third one is the whole point of
    # this block.
    #
    #   0  CHECKED, and nothing fired. The turn's text was read and it does not
    #      defer, or it defers and a discharge is present.
    #   3  A deferral fired. The JSON finding is on stdout.
    #   4  NOT CHECKED. The predicate could not be evaluated at all; the reason
    #      is one word on stdout.
    #
    # 4 exists because 0 used to mean both of the first and the last, and the
    # wrapper, having no way to tell them apart, told the operator "This turn's
    # text was checked" over a payload that carried no turn text. That sentence
    # was not merely uninformative, it was FALSE, and it is the one place in the
    # 2026-09-05 forty-hook survey where a degraded payload produced an
    # affirmative false statement rather than silence. A check reporting green
    # over nothing is the failure this whole engine is built against; it does
    # not get to live inside the guard that reports on unasked decisions.
    #
    # stop_hook_active is NOT one of the 4 cases. It is a deliberate stand-down
    # on a turn whose text a sibling has already spoken about, so the text WAS
    # checked; re-announcing on a block re-fire is what case 7c forbids.
    try:
        payload = json.load(sys.stdin)
    except Exception:
        print("payload-unreadable")
        return 4
    if not isinstance(payload, dict):
        print("payload-unreadable")
        return 4
    # Never re-fire into a block loop: if a sibling Stop hook blocked the turn,
    # this one has already spoken about the same text.
    if payload.get("stop_hook_active"):
        return 0

    text = payload.get("last_assistant_message") or ""
    if not isinstance(text, str) or not text.strip():
        print("no-turn-text")
        return 4

    hits = find_deferrals(text)
    if not hits:
        return 0

    tools = _tools_of_turn(payload)
    if tools is None:
        # The transcript could not be read. Two of the three discharges depend
        # on it, so FIRING here would blame a turn that may well have asked, and
        # that remains wrong. But this turn's text DOES carry a deferral
        # construction and nothing was able to decide whether the choice was put
        # to him — so the accusation is withheld and the NOT-CHECKED state is
        # reported instead. Reporting 0 here used to make the wrapper announce
        # that the text had been checked, which is the same false reassurance in
        # a rarer costume.
        print("transcript-unreadable")
        return 4

    if CONTINUED in tools:
        return 0          # the reply answering our own diagnostic is never fresh detector input
    asked = discharge(text, tools, hits)
    if asked is not None:
        return 0          # put to him, a spawn that starts it (never family D), or his own order
    # REPORT ONLY (R1): nothing is recorded. The lead is asked once to check it.
    name, quoted = held_pieces(text, hits)[0]
    print(json.dumps({
        "construction": name,
        "quote": quoted,
        "all": [h[0] for h in hits],
    }))
    return 3  # 3 = "a deferral fired"; the wrapper turns it into the notice


if __name__ == "__main__":
    sys.exit(main())
