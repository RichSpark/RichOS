#!/usr/bin/env python3
"""
failure-type.py — WHEN HE PUTS "TYPE" AND "FAILURE" TOGETHER, THE ANSWER COMES
                  FROM THE FAILURE REGISTER, BY NUMBER, AND IS RECORDED.

The analysis half of two hooks:

    scripts/hooks/failure-type-lookup.sh        UserPromptSubmit   `inject`
    scripts/hooks/guard-failure-type-answer.sh  Stop, BLOCKING     `stop`

===========================================================================
THE RULE, AND WHOSE IT IS
===========================================================================
The operator asked, in capitals, what must ALWAYS happen whenever he mentions
the words "type" and "failure" close to each other, and when that would start
happening with guaranteed reliability. It had been promised the same morning,
in a sentence, and broken the same day, because a sentence is not a mechanism.
The rule this file enforces:

    1. The lead READS the register (the type catalog) before answering.
    2. The reply NAMES each matching type as `Type <N>: <its heading,
       verbatim>`, where N exists in the register; or says it is a new type.
    3. The instance is RECORDED in the register and COMMITTED, in the same
       turn: a new type as the next free number, a repeat as a
       `### 5.<n> Type <N>, again` entry.

The register's location is DECLARED, never guessed: `FAILURE_TYPE_REGISTER` in
the entity's orchestration.config, absolute or relative to the entity root.

===========================================================================
WHAT THIS READS OF HIS WORDS, AND WHY THAT IS NOT INFERRING WHAT HE MEANT
===========================================================================
This engine's standing ruling is that a hook may check what the lead DID and
never decides what the operator MEANT. This file reads his message for two
WORDS, because the trigger is his own and he stated it in terms of words: "the
words TYPE and FAILURE close to each other". Nothing here classifies intent,
tone or topic. The predicate is lexical and literal: an inflection of *type*
(type, types, typed, typing) and one of *failure* (failure, failures), in
either order, at most WINDOW words apart. Its whole effect is to require a
lookup and a record; it never decides which type applies, which is the one
judgment the gate leaves with the lead and states that it leaves there.

Measured before the window was chosen, over every transcript on the machine
the rule was written on (2026-09-26): 2,645 messages whose origin is the
operator, 36 containing both words, and in all 36 the words are one or two
apart. Every window from two upward selects the same 36; six is headroom for
phrasings not yet seen, not a fitted number. The corpus beside the suite is
synthetic (the engine is public); his real lines are in the private record.

HIS TEXT ONLY. The words in a hook's output, a tool result, the lead's own
reply, a teammate's message, another session's message, a task notification or
pasted material never trigger anything. At UserPromptSubmit the payload holds
only the text, so machine envelopes are stripped by shape (the list below). At
Stop the transcript holds each record's `origin`, and the predicate is the one
scripts/lib/left-off.py derived and measured for "which messages are his",
imported rather than copied.

===========================================================================
THE STOP GATE — THREE CHECKS, EACH READ FROM AN ARTIFACT
===========================================================================
  (a) READ       a Read or Grep of the register, or a Bash command that runs a
                 reader (cat, grep, sed, awk, head, tail, python...) naming it,
                 in THIS turn's tool traffic, whose result is not an error. Any
                 copy of the register counts (a worktree holds one).
  (b) NAMED      the reply (every assistant text block of the turn, plus the
                 final message from the payload) contains `Type <N>` followed by
                 that type's heading, verbatim up to formatting (bold, backticks,
                 line breaks, a final period), for an N the register has. Or it
                 says "new type" AND (c) found a `Type <M>:` heading ADDED this
                 turn. A number alone is not enough: a heading copied out of the
                 register is the evidence that the register, not recall,
                 answered.
  (c) COMMITTED  a commit in the register's repository, on ANY ref, whose
                 committer time is at or after the turn's first record, and
                 which changes the register file.

"The turn" is scoped by prompt_id, the way turn-manifest.py scopes it: from the
first record carrying the payload's prompt_id to the end of the file.

===========================================================================
IT DOES NOT STAND DOWN ON THE RE-FIRE, AND THAT IS A DELIBERATE EXCEPTION
===========================================================================
Most Stop guards in this engine return 0 when `stop_hook_active` is true, so
each refuses a turn at most once. That is a pattern, not a rule:
guard-workspace-gate.sh keeps refusing. This gate keeps refusing too, because
each of its three steps is something the lead can do inside the same turn, in
under a minute. The bound is the host's: after CLAUDE_CODE_STOP_HOOK_BLOCK_CAP
consecutive refusals (default 8, read from the 2.1.283 binary) the host ends the
turn itself.

AND WHEN THE HOST DOES END IT, THE DEBT IS NOT FORGIVEN. An unpaid obligation
is written to the entity's state and carried into the NEXT turn, which is
refused for it whatever his next message says, until a turn reads, names and
commits. That is the answer to "the cap makes it less than 100%": the cap can
end a turn, it cannot make the record stop being owed.

===========================================================================
WHAT IT CANNOT GUARANTEE — stated so it is never read as more
===========================================================================
  * That the type named is the RIGHT type. That is judgment; the gate checks
    that a real type was named from the register and a record was committed.
  * That the commit records THIS instance. It checks that the register changed
    in a commit made during the turn, not what the change says.
  * A second session committing to the same register during the same turn
    would satisfy (c) for both.
  * Hooks load at session start: nothing here acts in a session that began
    before it was installed.

Usage:
    failure-type.py inject            UserPromptSubmit payload on stdin
    failure-type.py stop              Stop payload on stdin
    failure-type.py classify          one message per line on stdin -> 0/1 lines
    failure-type.py classify-json     one JSON string on stdin -> 0/1
    failure-type.py classify-corpus F  a private corpus (JSON lines with
                                       "text" and "want") -> summary, exit 1 on
                                       any disagreement

Environment (set by the wrappers):
    RICHOS_FT_ENTITY_ROOT   the governed repository
    RICHOS_FT_REGISTER      FAILURE_TYPE_REGISTER as declared (may be empty)
    RICHOS_FT_STOOD_DOWN    1 when CHECK_FAILURE_TYPE=0
"""

import hashlib
import importlib.util
import json
import os
import re
import subprocess
import sys
import time
from datetime import datetime, timezone

HOOK_UPS = "(hook: scripts/hooks/failure-type-lookup.sh)"
HOOK_STOP = "(hook: scripts/hooks/guard-failure-type-answer.sh)"

WINDOW = 6
TYPE_WORD = re.compile(r"typ(?:e|es|ed|ing)", re.I)
FAILURE_WORD = re.compile(r"failures?", re.I)
TOKEN = re.compile(r"[A-Za-z]+|\d+")

# THE CHANNEL CAP, READ FROM THE HOST RATHER THAN INHERITED. commit-ceo-inputs.sh
# records an older measurement (8000 characters, 200 lines, over it the whole
# object dropped). In the 2.1.283 binary a hook's additionalContext passes
# through `cae(..., {threshold: CLo})` with `CLo=1e4`: up to 10,000 characters
# it is kept inline; over it the text is written to disk and only its head and
# path reach the model. So the list is budgeted under 10,000 with a margin, and
# anything that does not fit is NAMED as not shown rather than cut silently.
# The line ceiling is the older measurement's, kept because it is the stricter.
CONTEXT_BUDGET = 9600
CONTEXT_MAX_LINES = 180

GIT_TIMEOUT = 10
COMMIT_SKEW_S = 5

# Blocks that are never his words, whatever sits inside them.
ENVELOPE_BLOCKS = re.compile(
    r"<(system-reminder|task-notification|teammate-message|agent-message|"
    r"cross-session-message|local-command-stdout|local-command-stderr|"
    r"command-name|command-message|command-args|pasted_content|"
    r"user-memory-input)\b[^>]*>.*?</\1\s*>",
    re.S | re.I,
)
# Text that opens this way is machine-written as a whole. Kept in step with
# left-off.py's MACHINE_ENVELOPES, which is imported and added below.
MACHINE_PREFIXES = [
    "<task-notification>",
    "<teammate-message",
    "<agent-message",
    "<cross-session-message",
    "Another Claude session sent a message:",
    "Stop hook feedback:",
    "Caveat:",
    "This session is being continued from a previous conversation",
]

READER_VERBS = re.compile(
    r"(?:^|[\s;|&(`$])(?:cat|grep|egrep|fgrep|rg|sed|awk|head|tail|less|more|nl|"
    r"python3?|perl|ruby|bat|view|cut|wc)\b"
)
NEW_TYPE_PHRASE = re.compile(r"\bnew (?:failure )?type\b", re.I)
TYPE_REF = re.compile(r"\bType\s+(\d+)\b\s*[:.—–-]?\s*", re.I)
HEADING = re.compile(r"^#{2,5} Type (\d+): (.+?)\s*$", re.M)
ADDED_TYPE_HEADING = re.compile(r"^\+#{2,5} Type (\d+): ")
BOX_TYPE = re.compile(r"Next free type number\**\s*\|\s*\**\s*(\d+)")
BOX_SECTION = re.compile(r"Next free section number\**\s*\|\s*\**\s*([\d.]+)")
PART5_SECTION = re.compile(r"^(?:#{2,5} |\*Section )5\.(\d+)\b", re.M)


# ===========================================================================
# the shared human predicate — imported, never copied
# ===========================================================================
_LO = None


def left_off():
    global _LO
    if _LO is None:
        here = os.path.dirname(os.path.abspath(__file__))
        spec = importlib.util.spec_from_file_location("richos_left_off", os.path.join(here, "left-off.py"))
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        _LO = mod
        for p in getattr(mod, "MACHINE_ENVELOPES", ()):
            if p not in MACHINE_PREFIXES:
                MACHINE_PREFIXES.append(p)
    return _LO


# ===========================================================================
# the trigger
# ===========================================================================
def his_text(text):
    """His words with every machine-written part removed; "" if none are his."""
    if not isinstance(text, str):
        return ""
    left_off()
    stripped = text.lstrip()
    for prefix in MACHINE_PREFIXES:
        if stripped.startswith(prefix):
            return ""
    return ENVELOPE_BLOCKS.sub(" ", text)


def trigger(text):
    """The snippet where the two words sit together, or "" when they do not."""
    words = [(m.group(0), m.start(), m.end()) for m in TOKEN.finditer(his_text(text))]
    types = [i for i, (w, _s, _e) in enumerate(words) if TYPE_WORD.fullmatch(w)]
    fails = [i for i, (w, _s, _e) in enumerate(words) if FAILURE_WORD.fullmatch(w)]
    for i in types:
        for j in fails:
            if abs(i - j) <= WINDOW:
                lo, hi = min(i, j), max(i, j)
                return " ".join(w for w, _s, _e in words[lo:hi + 1])
    return ""


# ===========================================================================
# the register
# ===========================================================================
def resolve_register(entity_root, declared):
    if not declared:
        return ""
    p = os.path.expanduser(declared)
    if not os.path.isabs(p):
        p = os.path.join(entity_root or os.getcwd(), p)
    return os.path.normpath(p)


def parse_register(text):
    types = {}
    for m in HEADING.finditer(text):
        types.setdefault(int(m.group(1)), m.group(2).strip())
    box_t = BOX_TYPE.search(text)
    box_s = BOX_SECTION.search(text)
    sections = [int(x) for x in PART5_SECTION.findall(text)]
    return {
        "types": types,
        "box_type": int(box_t.group(1)) if box_t else None,
        "box_section": box_s.group(1) if box_s else None,
        "derived_type": (max(types) + 1) if types else 1,
        "derived_section": "5.%d" % ((max(sections) + 1) if sections else 1),
    }


def read_register(path):
    """(parsed, error). An unreadable or headless register is an error."""
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError as exc:
        return None, "the register could not be read at %s (%s)" % (path, exc.strerror or exc)
    parsed = parse_register(text)
    if not parsed["types"]:
        return None, "the register at %s has no `Type <N>: <heading>` heading at all" % path
    return parsed, None


def git(repo, args, timeout=GIT_TIMEOUT):
    try:
        r = subprocess.run(["git", "-C", repo] + args, capture_output=True, text=True, timeout=timeout)
    except Exception:
        return None
    return r.stdout if r.returncode == 0 else None


def repo_of(path):
    top = git(os.path.dirname(path), ["rev-parse", "--show-toplevel"])
    if not top:
        return None, None
    top = top.strip()
    rel = os.path.relpath(os.path.realpath(path), os.path.realpath(top))
    return top, rel


def next_numbers_line(reg):
    bt, dt = reg["box_type"], reg["derived_type"]
    bs, ds = reg["box_section"], reg["derived_section"]
    line = "Next free type number: %s (box) · next free section number: %s (box)" % (
        bt if bt is not None else "NOT IN THE BOX", bs if bs is not None else "NOT IN THE BOX")
    notes = []
    if bt is None or bt != dt:
        notes.append("THE BOX DISAGREES WITH THE HEADINGS: the highest heading is Type %d, so the next "
                     "free type number by the headings is %d" % (dt - 1, dt))
    if bs is None or bs != ds:
        notes.append("the Part 5 sections say the next free section number is %s" % ds)
    if notes:
        line += " — " + "; ".join(notes) + ". Settle it in the register before allocating a number."
    return line


# ===========================================================================
# state: the obligations ledger and the event log
# ===========================================================================
def state_dir(entity_root):
    return os.path.join(entity_root, ".claude", "state", "failure-type")


def session_key(session_id):
    k = re.sub(r"[^A-Za-z0-9-]", "", session_id or "")[:8]
    return k or "nosession"


def pending_path(entity_root, session_id):
    return os.path.join(state_dir(entity_root), "pending.%s.json" % session_key(session_id))


def load_pending(entity_root, session_id):
    try:
        with open(pending_path(entity_root, session_id), encoding="utf-8") as fh:
            data = json.load(fh)
        items = data.get("pending") if isinstance(data, dict) else None
        return [i for i in items if isinstance(i, dict)] if isinstance(items, list) else []
    except (OSError, ValueError):
        return []


# Failures to save the owed-answer record in THIS process. The record is the only
# thing that carries an unpaid obligation to the next turn, so a failed save is
# reported (stderr, and the Stop status line) and never swallowed (P5-48).
SAVE_FAILURES = []


def save_pending(entity_root, session_id, items):
    """True when the record now says what `items` says; False, loudly, when not."""
    path = pending_path(entity_root, session_id)
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        if not items:
            if os.path.exists(path):
                os.remove(path)
            return True
        tmp = "%s.%d" % (path, os.getpid())
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump({"pending": items}, fh)
        os.replace(tmp, path)
        return True
    except OSError as error:
        msg = ("the owed failure-type answer could NOT be saved at %s (%s), so the next turn will not know it "
               "is owed" % (path, error))
        SAVE_FAILURES.append(msg)
        sys.stderr.write("failure-type: %s\n" % msg)
        return False


def log_event(entity_root, record):
    """Best effort. Carries the matched words only, never his whole message."""
    try:
        os.makedirs(state_dir(entity_root), exist_ok=True)
        record = dict(record)
        record["at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        with open(os.path.join(state_dir(entity_root), "events.jsonl"), "a", encoding="utf-8") as fh:
            fh.write(json.dumps(record) + "\n")
    except OSError:
        pass


def text_hash(text):
    return hashlib.sha256((text or "").strip().encode("utf-8", "replace")).hexdigest()[:16]


# ===========================================================================
# inject — UserPromptSubmit
# ===========================================================================
def emit_ups(system, context):
    out = {"hookSpecificOutput": {"hookEventName": "UserPromptSubmit", "additionalContext": context}}
    if system:
        out["systemMessage"] = system
    sys.stdout.write(json.dumps(out) + "\n")


def build_context(snippet, path, repo, reg):
    head = [
        '=== FAILURE-TYPE LOOKUP: HIS MESSAGE PUTS "TYPE" AND "FAILURE" TOGETHER ("%s") ===' % snippet,
        "Before you answer, in THIS turn. The Stop gate refuses to end the turn until all three are true:",
        " 1. READ the register: %s (the Read tool, or grep/sed on it)." % path,
        " 2. NAME each matching type as `Type <N>: <its heading, verbatim>` from the list below, or say it is a new type.",
        " 3. RECORD it and COMMIT in %s: a repeat gets `### 5.<n> Type <N>, again — <date> — <one line>` in Part 5;"
        " a new type gets `### Type <next free>: <description>` in Part 5 plus its index row and catalog entry,"
        " and the box bumped." % (repo or "the register's repository"),
        next_numbers_line(reg),
        "The register's %d types, read from the file just now:" % len(reg["types"]),
    ]
    tail = [HOOK_UPS]
    used = sum(len(x) + 1 for x in head + tail)
    lines_left = CONTEXT_MAX_LINES - len(head) - len(tail) - 1
    shown = []
    numbers = sorted(reg["types"])
    for n in numbers:
        row = "Type %d: %s" % (n, reg["types"][n])
        if used + len(row) + 1 + 160 > CONTEXT_BUDGET or len(shown) >= lines_left:
            break
        shown.append(row)
        used += len(row) + 1
    body = head + shown
    omitted = numbers[len(shown):]
    if omitted:
        body.append("... %d type(s) NOT SHOWN (Type %d to Type %d): the channel holds about %d characters, "
                    "so read them in the register itself." % (len(omitted), omitted[0], omitted[-1], CONTEXT_BUDGET))
    return "\n".join(body + tail)


def cmd_inject(payload):
    entity = os.environ.get("RICHOS_FT_ENTITY_ROOT") or payload.get("cwd") or os.getcwd()
    if payload.get("agent_id"):
        return 0
    prompt = payload.get("prompt")
    if not isinstance(prompt, str):
        return 0
    snippet = trigger(prompt)
    if not snippet:
        return 0
    session = payload.get("session_id") or ""

    if os.environ.get("RICHOS_FT_STOOD_DOWN") == "1":
        emit_ups("FAILURE-TYPE LOOKUP — STOOD DOWN by CHECK_FAILURE_TYPE=0: your message puts \"type\" and "
                 "\"failure\" together and nothing handed the lead the register. %s" % HOOK_UPS,
                 "THE FAILURE-TYPE LOOKUP IS STOOD DOWN (CHECK_FAILURE_TYPE=0). His message puts \"type\" and "
                 "\"failure\" together (\"%s\"). Read the failure register yourself and answer with a type number "
                 "from it. %s" % (snippet, HOOK_UPS))
        log_event(entity, {"event": "stood-down", "session": session_key(session), "words": snippet})
        return 0

    declared = os.environ.get("RICHOS_FT_REGISTER", "")
    path = resolve_register(entity, declared)
    if not path:
        emit_ups("FAILURE-TYPE LOOKUP NOT CONFIGURED for this repository (no FAILURE_TYPE_REGISTER in "
                 "orchestration.config), so no register was handed to the lead. %s" % HOOK_UPS,
                 "THE FAILURE-TYPE LOOKUP IS NOT CONFIGURED here: FAILURE_TYPE_REGISTER is unset in %s, so no "
                 "register was read and nothing will check the answer. His message puts \"type\" and \"failure\" "
                 "together (\"%s\"). %s" % (os.path.join(entity, "orchestration.config"), snippet, HOOK_UPS))
        log_event(entity, {"event": "unconfigured", "session": session_key(session), "words": snippet})
        return 0

    reg, err = read_register(path)
    if err:
        emit_ups("FAILURE-TYPE LOOKUP COULD NOT READ THE REGISTER: %s. The lead was told to read it by hand. %s"
                 % (err, HOOK_UPS),
                 "THE FAILURE-TYPE LOOKUP COULD NOT READ THE REGISTER: %s. His message puts \"type\" and "
                 "\"failure\" together (\"%s\"). Find the register, read it, and answer with `Type <N>: <heading>` "
                 "from it. This is the absence of a check, not a clean one. %s" % (err, snippet, HOOK_UPS))
        log_event(entity, {"event": "cannot-read", "session": session_key(session), "words": snippet})
        pend = load_pending(entity, session)
        pend.append({"source": "UserPromptSubmit", "hash": text_hash(prompt), "words": snippet,
                     "at": time.time()})
        save_pending(entity, session, pend)
        return 0

    repo, _rel = repo_of(path)
    context = build_context(snippet, path, repo, reg)
    emit_ups("FAILURE-TYPE LOOKUP: your message puts \"type\" and \"failure\" together, so the lead was handed "
             "the register's %d types and cannot end this turn until one is named by number and the record is "
             "committed. %s" % (len(reg["types"]), HOOK_UPS), context)
    pend = load_pending(entity, session)
    h = text_hash(prompt)
    if not any(p.get("hash") == h for p in pend):
        pend.append({"source": "UserPromptSubmit", "hash": h, "words": snippet, "at": time.time()})
    save_pending(entity, session, pend)
    log_event(entity, {"event": "injected", "session": session_key(session), "words": snippet,
                       "types": len(reg["types"])})
    return 0


# ===========================================================================
# stop — the gate
# ===========================================================================
def parse_ts(s):
    if not s:
        return None
    try:
        return datetime.fromisoformat(str(s).replace("Z", "+00:00")).timestamp()
    except Exception:
        return None


def turn_start_offset(path, prompt_id, chunk=4 * 1024 * 1024):
    """Byte offset of the first record carrying prompt_id, found by reading
    backward from the END of the file, so a long session costs no more than its
    last turn does (2026-10-10: no hook may skip itself because a transcript is
    large).

    The first record is accepted only when a record of ANOTHER turn precedes it
    in the window already read (or the window reaches the start of the file);
    otherwise the window doubles. When the prompt cannot be located without
    pulling in more than 64 MB, the answer is 0 and the caller's forward
    streaming read from the start takes over, which is the previous behavior.
    """
    needle = prompt_id.encode("utf-8", "replace")

    def pid_of(raw):
        try:
            rec = json.loads(raw)
        except Exception:
            return None
        return rec.get("promptId") if isinstance(rec, dict) else None

    try:
        size = os.path.getsize(path)
        window = chunk
        with open(path, "rb") as fh:
            while True:
                pos = max(0, size - window)
                fh.seek(pos)
                buf = fh.read(size - pos)
                base = 0
                if pos > 0:
                    nl = buf.find(b"\n")
                    base = len(buf) if nl < 0 else nl + 1
                lines, at = [], pos + base
                for ln in buf[base:].split(b"\n"):
                    lines.append((at, ln))
                    at += len(ln) + 1
                first = None
                for idx, (_o, ln) in enumerate(lines):
                    if needle in ln and pid_of(ln) == prompt_id:
                        first = idx
                        break
                if first is not None:
                    if pos == 0:
                        return lines[first][0]
                    for _o, ln in reversed(lines[:first]):
                        if b'"promptId"' in ln:
                            other = pid_of(ln)
                            if other and other != prompt_id:
                                return lines[first][0]
                if pos == 0 or window >= 64 * 1024 * 1024:
                    return 0
                window *= 2
    except OSError:
        return 0


def read_turn(path, prompt_id):
    """(records-in-turn, error). The turn opens on the first record carrying
    prompt_id and runs to the end of the file."""
    if not path or not os.path.isfile(path):
        return None, "the transcript is not a readable file at %s" % path
    try:
        os.path.getsize(path)
    except OSError as exc:
        return None, "the transcript could not be sized (%s)" % exc
    out = []
    in_turn = False
    # No size cap: the turn is located from the end of the file.
    offset = turn_start_offset(path, prompt_id)
    try:
        with open(path, "rb") as fh:
            fh.seek(offset)
            for raw in fh:
                line = raw.decode("utf-8", "replace")
                if not in_turn and prompt_id not in line:
                    continue
                try:
                    rec = json.loads(line)
                except Exception:
                    continue
                if not isinstance(rec, dict):
                    continue
                if not in_turn and rec.get("promptId") == prompt_id:
                    in_turn = True
                if in_turn and not rec.get("isSidechain"):
                    out.append(rec)
    except OSError as exc:
        return None, "the transcript could not be read (%s)" % exc
    if not out:
        return None, "no record in the transcript carries this turn's prompt_id"
    return out, None


def turn_facts(records):
    lo = left_off()
    humans, machines, texts, calls, results = [], [], [], [], {}
    start = None
    for rec in records:
        ts = parse_ts(rec.get("timestamp"))
        if start is None and ts is not None:
            start = ts
        if rec.get("type") == "user":
            verdict, _deg = lo.is_human(rec)
            text, has_result = lo.text_of(rec)
            if verdict:
                humans.append(text)
            elif text and not has_result:
                machines.append(text)
            content = (rec.get("message") or {}).get("content")
            if isinstance(content, list):
                for b in content:
                    if isinstance(b, dict) and b.get("type") == "tool_result" and b.get("tool_use_id"):
                        results[b["tool_use_id"]] = b
        elif rec.get("type") == "assistant":
            content = (rec.get("message") or {}).get("content")
            if isinstance(content, list):
                for b in content:
                    if not isinstance(b, dict):
                        continue
                    if b.get("type") == "text":
                        texts.append(b.get("text") or "")
                    elif b.get("type") == "tool_use":
                        calls.append(b)
    return {"start": start, "humans": humans, "machines": machines, "texts": texts,
            "calls": calls, "results": results}


def path_is_register(p, reg_path, rel):
    if not isinstance(p, str) or not p:
        return False
    p = os.path.normpath(p)
    if p == reg_path:
        return True
    return bool(rel) and (p == rel or p.endswith(os.sep + rel))


def register_read(facts, reg_path, rel):
    base = os.path.basename(reg_path)
    for call in facts["calls"]:
        res = facts["results"].get(call.get("id"))
        if res is None or res.get("is_error"):
            continue
        name = call.get("name")
        inp = call.get("input") or {}
        if name == "Read" and path_is_register(inp.get("file_path"), reg_path, rel):
            return True
        if name == "Grep" and path_is_register(inp.get("path"), reg_path, rel):
            return True
        if name == "Bash":
            if bash_reads_register(inp.get("command") or "", reg_path, rel, base):
                return True
    return False


_READER_WORDS = {"cat", "grep", "egrep", "fgrep", "rg", "sed", "awk", "head", "tail", "less",
                 "more", "nl", "python", "python3", "perl", "ruby", "bat", "view", "cut", "wc"}
_INTERPRETERS = {"python", "python3", "perl", "ruby"}
_WRAPPERS = {"sudo", "env", "command", "time", "nice", "builtin"}


def bash_reads_register(cmd, reg_path, rel, base):
    """True when some simple command in `cmd` runs a READER on the register.

    Looking for the reader verb and the register's name anywhere in the text
    credited `echo cat failure-types.md`, which reads nothing. The command is
    split into simple commands, the command WORD must be a reader, and one of
    its arguments (or the target of a `<` redirect) must be the register. An
    interpreter run with -c/-e carries its program in one argument, so there the
    register's name anywhere inside that argument counts."""
    import shlex
    try:
        lex = shlex.shlex(cmd, posix=True, punctuation_chars=True)
        lex.whitespace_split = True
        tokens = list(lex)
    except ValueError:
        return False
    segments, cur = [], []
    for tok in tokens:
        if tok and set(tok) <= set(";|&()"):
            segments.append(cur)
            cur = []
        else:
            cur.append(tok)
    segments.append(cur)
    for seg in segments:
        words = list(seg)
        while words and (re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", words[0]) or words[0] in _WRAPPERS):
            words.pop(0)
        if not words:
            continue
        verb = os.path.basename(words[0])
        args, redirected = [], False
        for w in words[1:]:
            if w in ("<<", "<<<"):
                redirected = "text"  # a here-document or here-string operand is TEXT on stdin, never a file
                continue
            if w == "<":
                redirected = True
                continue
            if w.startswith(">") or re.match(r"^\d*>", w):
                redirected = None  # an output redirect: its operand is written, not read
                continue
            if redirected is None or redirected == "text":
                redirected = False
                continue
            args.append(w)
        if verb not in _READER_WORDS:
            continue
        files, program, rest = _reader_operands(verb, args)
        for a in files:
            if path_matches_register(a, reg_path, rel, base):
                return True
        if program is not None and (
                any(path_matches_register(p, reg_path, rel, base) for p in _read_targets(program))
                or (_READ_CALL.search(program)
                    and any(path_matches_register(a, reg_path, rel, base) for a in rest))):
            return True
    return False


# Hunt P5-57 (v3): the register's name anywhere in a program that reads SOMETHING
# credited `print('failure-types.md'); open('other-file')`. Only a literal path
# that is itself the argument of a read call counts as the program reading it.
_READ_TARGET = [
    re.compile(r"""(?<![\w.])open\s*\(\s*[rbuf]*(['"])(?P<p>[^'"]+)\1(?!\s*,\s*[rbuf]*['"][^'"]*[wax])"""),
    re.compile(r"""\bPath\s*\(\s*[rbuf]*(['"])(?P<p>[^'"]+)\1\s*\)\s*\.\s*(?:read_text|read_bytes|open|readlines)\b"""),
    re.compile(r"""\b(?:File|IO)\.(?:read|open|foreach|readlines)\s*\(?\s*(['"])(?P<p>[^'"]+)\1"""),
    re.compile(r"""\bfs\.read\w*\s*\(\s*(['"])(?P<p>[^'"]+)\1"""),
    re.compile(r"""(?<![\w.])open\s*\(?\s*(?:my\s+)?[$\w]+\s*,\s*(?:(['"])<\1\s*,\s*)?(['"])<?\s*(?P<p>[^'"]+)\2"""),
]


def _read_targets(program):
    return [m.group("p").strip() for rx in _READ_TARGET for m in rx.finditer(program)]


# An interpreter's one-line program READS a file only when it calls something that
# reads (P5-57). Its text naming the register, as a print string, proves nothing.
_READ_CALL = re.compile(r"open\s*\(|read_text|read_bytes|\.read\s*\(|readlines|readline|"
                        r"File\.(?:read|open|foreach)|IO\.(?:read|readlines|foreach)|"
                        r"fs\.read|readFile|<>|<STDIN>|getline")
_PATTERN_FIRST = {"grep", "egrep", "fgrep", "rg", "sed", "awk"}
_OPT_VALUE = {"grep": {"-m", "-A", "-B", "-C", "--max-count", "--include", "--exclude"},
              "egrep": {"-m", "-A", "-B", "-C"}, "fgrep": {"-m", "-A", "-B", "-C"},
              "rg": {"-m", "-A", "-B", "-C", "-g", "-t", "--glob", "--type", "--max-count"},
              "sed": {"-l"}, "awk": {"-v", "-F"}}
_PROGRAM_FLAGS = {"-e", "--regexp", "--expression"}
_PROGRAM_FILE_FLAGS = {"-f", "--file"}


def _reader_operands(verb, args):
    """(files read, interpreter program text or None, args after the program) for one reader.

    The first positional of grep, sed, awk and rg is a pattern or program, never a
    file, unless -e/-f gave it already; an interpreter run with -c/-e carries its
    program in the next argument. Only the real file operands count as reads."""
    if verb in _INTERPRETERS:
        for i, a in enumerate(args):
            if a in ("-c", "-e", "-E", "-ne", "-pe", "-ane") and i + 1 < len(args):
                prog, rest = args[i + 1], args[i + 2:]
                # perl and ruby treat what follows the program as files; python as argv
                return ([] if verb in ("python", "python3") else rest), prog, rest
        return [a for a in args if not a.startswith("-")], None, []
    if verb not in _PATTERN_FIRST:
        return [a for a in args if not a.startswith("-")], None, []
    files, have_program, skip = [], False, False
    valued = _OPT_VALUE.get(verb, set())
    for a in args:
        if skip:
            skip = False
            continue
        if a in _PROGRAM_FLAGS:
            have_program, skip = True, True
        elif a in _PROGRAM_FILE_FLAGS:
            have_program = True  # the pattern or program FILE is itself read: keep the operand
        elif a in valued:
            skip = True
        elif a.startswith("-") and a != "-":
            continue
        elif not have_program:
            have_program = True  # the pattern or program text
        else:
            files.append(a)
    return files, None, []


def path_matches_register(arg, reg_path, rel, base):
    if not arg or arg.startswith("-"):
        return False
    return path_is_register(arg, reg_path, rel) or os.path.basename(arg) == base


def norm(s):
    s = s.replace("’", "'").replace("‘", "'").replace("“", '"').replace("”", '"')
    s = s.replace("—", "-").replace("–", "-")
    s = re.sub(r"[*_`#>]", "", s)
    s = re.sub(r"\s+", " ", s).strip().lower()
    return s


def named_types(reply, types):
    """([N named with the heading verbatim], [(N, heading) named without it])."""
    verbatim, loose = [], []
    for m in TYPE_REF.finditer(reply):
        n = int(m.group(1))
        heading = types.get(n)
        if heading is None:
            continue
        want = norm(heading).rstrip(".")
        after = norm(reply[m.end():m.end() + len(heading) * 2 + 80])
        if after.startswith(want):
            if n not in verbatim:
                verbatim.append(n)
        elif n not in [x for x, _h in loose]:
            loose.append((n, heading))
    return verbatim, [x for x in loose if x[0] not in verbatim]


def turn_commits(repo, rel, start):
    """[(sha, added-lines)] for commits on any ref, committed at or after the
    turn's start, that change the register."""
    out = git(repo, ["log", "--all", "--max-count=200", "--format=%H %ct", "--", rel])
    if out is None:
        return None
    found = []
    for line in out.splitlines():
        parts = line.split()
        if len(parts) != 2:
            continue
        sha, ct = parts[0], int(parts[1])
        if start is not None and ct < start - COMMIT_SKEW_S:
            continue
        diff = git(repo, ["show", "--format=", "--unified=0", "--no-color", sha, "--", rel]) or ""
        added = [ln for ln in diff.splitlines() if ln.startswith("+") and not ln.startswith("+++")]
        found.append((sha, added))
    return found


def commit_types(repo, rel, commits):
    """Headings as they stand in each of this turn's commits."""
    types = {}
    for sha, _added in commits:
        text = git(repo, ["show", "%s:%s" % (sha, rel)])
        if text:
            for n, h in parse_register(text)["types"].items():
                types.setdefault(n, h)
    return types


def stop_status(kind, detail=""):
    # A quiet "none"/"satisfied" line over a record that failed to save is the
    # silent loss P5-48 names: say it instead. ("blocked" carries the letters of
    # what is missing, and its refusal text already printed the save failure.)
    if SAVE_FAILURES and kind == "cannot":
        detail = (detail + " — AND " if detail else "") + "; ".join(SAVE_FAILURES)
    elif SAVE_FAILURES and kind != "blocked":
        kind, detail = "cannot", "; ".join(SAVE_FAILURES)
    sys.stdout.write("FT\t%s\t%s\n" % (kind, detail.replace("\t", " ").replace("\n", " ")))


def cmd_stop(payload):
    entity = os.environ.get("RICHOS_FT_ENTITY_ROOT") or payload.get("cwd") or os.getcwd()
    if payload.get("agent_id"):
        stop_status("none", "teammate")
        return 0
    session = payload.get("session_id") or ""
    prompt_id = payload.get("prompt_id")
    if not prompt_id:
        stop_status("cannot", "the Stop payload carried no prompt_id, so this turn cannot be told apart from the "
                              "rest of the session and whether it owes a failure-type answer was NOT CHECKED")
        return 0
    records, err = read_turn(payload.get("transcript_path"), prompt_id)
    pending = load_pending(entity, session)
    if records is None:
        if pending:
            stop_status("cannot", "%s, so the failure-type answer owed from an earlier message was NOT CHECKED"
                        % err)
        else:
            stop_status("none", err)
        return 0
    facts = turn_facts(records)

    # Obligations: his words in this turn, plus anything carried over. A
    # carried item the transcript positively shows was machine-written is not
    # his and is dropped.
    machine_hashes = {text_hash(t) for t in facts["machines"]}
    pending = [p for p in pending if p.get("hash") not in machine_hashes]
    owed = []
    for text in facts["humans"]:
        w = trigger(text)
        if w:
            owed.append({"source": "transcript", "hash": text_hash(text), "words": w, "at": time.time()})
    known = {p.get("hash") for p in pending}
    obligations = pending + [o for o in owed if o["hash"] not in known]
    if not obligations:
        save_pending(entity, session, [])
        stop_status("none", "")
        return 0
    words = obligations[-1].get("words") or ""

    declared = os.environ.get("RICHOS_FT_REGISTER", "")
    reg_path = resolve_register(entity, declared)
    if not reg_path:
        save_pending(entity, session, obligations)
        stop_status("cannot", "FAILURE_TYPE_REGISTER is not set in orchestration.config, so whether this turn "
                              "answered his failure-type question from the register was NOT CHECKED")
        return 0
    reg, rerr = read_register(reg_path)
    repo, rel = repo_of(reg_path) if not rerr else (None, None)
    if rerr or not repo:
        save_pending(entity, session, obligations)
        stop_status("cannot", "%s, so whether this turn answered his failure-type question (\"%s\") from the "
                              "register was NOT CHECKED" % (rerr or "the register is not inside a git repository",
                                                           words))
        return 0

    commits = turn_commits(repo, rel, facts["start"])
    if commits is None:
        save_pending(entity, session, obligations)
        stop_status("cannot", "git could not list the register's history in %s, so this turn's failure-type "
                              "record was NOT CHECKED" % repo)
        return 0
    types = dict(reg["types"])
    for n, h in commit_types(repo, rel, commits).items():
        types.setdefault(n, h)
    new_headings = set()
    for _sha, added in commits:
        for ln in added:
            m = ADDED_TYPE_HEADING.match(ln)
            if m:
                new_headings.add(int(m.group(1)))

    reply = "\n".join(facts["texts"] + [payload.get("last_assistant_message") or ""])
    a_ok = register_read(facts, reg_path, rel)
    verbatim, loose = named_types(reply, types)
    says_new = bool(NEW_TYPE_PHRASE.search(reply))
    b_ok = bool(verbatim) or (says_new and bool(new_headings))
    c_ok = bool(commits)

    if a_ok and b_ok and c_ok:
        save_pending(entity, session, [])
        named = ", ".join("Type %d" % n for n in verbatim) or ", ".join("new Type %d" % n for n in sorted(new_headings))
        shas = ", ".join(sha[:7] for sha, _a in commits[:3])
        log_event(entity, {"event": "satisfied", "session": session_key(session), "types": verbatim,
                           "new": sorted(new_headings), "commits": [s for s, _a in commits[:5]]})
        stop_status("satisfied", "FAILURE-TYPE ANSWER: the register was read, %s named from it, and the record "
                                 "committed this turn (%s). %s" % (named, shas, HOOK_STOP))
        return 0

    save_pending(entity, session, obligations)
    lines = [
        "FAILURE-TYPE ANSWER GATE: this turn owes an answer to his message putting \"type\" and \"failure\" "
        "together (\"%s\"), and it cannot end until the register was read, a type from it was named, and the "
        "record was committed." % words,
    ]
    if not a_ok:
        lines.append("  MISSING (a) the register was not read this turn. Read %s (the Read tool, or grep/sed on "
                     "it)." % reg_path)
    if not b_ok:
        why = ("  MISSING (b) the reply names no type from the register. Write `Type <N>: <heading, verbatim>` "
               "for each matching type.")
        if loose:
            why += " Named without the register's heading: " + "; ".join(
                "Type %d, whose heading is \"%s\"" % (n, h) for n, h in loose[:3]) + "."
        if says_new and not new_headings:
            why += (" The reply uses the words \"new type\", but no `Type <M>:` heading was added to the register in "
                    "a commit this turn; a new type is not new until it has its number (next free: %s)."
                    % (reg["box_type"] if reg["box_type"] is not None else reg["derived_type"]))
        lines.append(why)
    if not c_ok:
        lines.append("  MISSING (c) the register has no commit from this turn. Record it (a repeat: `### 5.<n> "
                     "Type <N>, again — <date> — <one line>` in Part 5; a new type: `### Type <M>: <description>` "
                     "in Part 5 with its index row, catalog entry and the box bumped) and commit it in %s." % repo)
    present = [x for x, ok in (("(a) read", a_ok), ("(b) named", b_ok), ("(c) committed", c_ok)) if ok]
    if present:
        lines.append("  Already done: %s." % ", ".join(present))
    lines.append("  Next free type number: %s; next free section number: %s (from the register's box)." % (
        reg["box_type"] if reg["box_type"] is not None else reg["derived_type"],
        reg["box_section"] or reg["derived_section"]))
    lines.append("  This gate does not stand down when it re-fires, and an unpaid answer carries into the next "
                 "turn. %s" % HOOK_STOP)
    sys.stderr.write("\n".join(lines) + "\n")
    missing = "".join(x for x, ok in (("a", a_ok), ("b", b_ok), ("c", c_ok)) if not ok)
    log_event(entity, {"event": "blocked", "session": session_key(session), "missing": missing,
                       "words": words, "refire": bool(payload.get("stop_hook_active"))})
    stop_status("blocked", missing)
    return 2


# ===========================================================================
# classify — the predicate, by hand and over a corpus
# ===========================================================================
def cmd_classify():
    for line in sys.stdin.read().splitlines():
        if not line.strip():
            continue
        print("1" if trigger(line) else "0")
    return 0


def cmd_classify_json():
    try:
        text = json.loads(sys.stdin.read())
    except Exception:
        return 2
    print("1" if trigger(text) else "0")
    return 0


def cmd_classify_corpus(path):
    total = agree = 0
    wrong = []
    try:
        fh = open(path, encoding="utf-8")
    except OSError as exc:
        print("cannot read %s: %s" % (path, exc))
        return 2
    with fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
            except Exception:
                continue
            total += 1
            got = 1 if trigger(row.get("text", "")) else 0
            if got == int(row.get("want", 0)):
                agree += 1
            else:
                wrong.append("%s want %s got %d" % (row.get("id", "?"), row.get("want"), got))
    if total == 0:
        print("the corpus at %s holds no rows: nothing was checked" % path)
        return 1
    print("%d/%d rows classified as recorded%s" % (agree, total, ("; DISAGREE: " + "; ".join(wrong[:10])) if wrong else ""))
    return 0 if not wrong else 1


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    if mode == "classify":
        return cmd_classify()
    if mode == "classify-json":
        return cmd_classify_json()
    if mode == "classify-corpus" and len(sys.argv) > 2:
        return cmd_classify_corpus(sys.argv[2])
    if mode in ("inject", "stop"):
        try:
            payload = json.loads(sys.stdin.read())
        except Exception:
            payload = None
        if not isinstance(payload, dict):
            if mode == "stop":
                stop_status("cannot", "the Stop payload was not readable JSON")
            return 0
        return cmd_inject(payload) if mode == "inject" else cmd_stop(payload)
    sys.stderr.write(__doc__.split("Usage:")[1].split("Environment")[0])
    return 2


if __name__ == "__main__":
    sys.exit(main())
