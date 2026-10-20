#!/usr/bin/env python3
"""One engine-side triage decision for one recorded work event.

Axel/Wes PRD 2026-10-20 section 4. A tool-free Haiku call decides whether a
work event warrants both advisors: routine, inspect or uncertain, with a short
evidence-linked reason, plus the event's relation to the existing work round
only when no existing classification covers it. A malformed answer, provider
failure, oversized input or missing provider is recorded as `unavailable`,
which every caller routes to the same bounded inspection as `inspect`, never
to routine. This is the only triage implementation: the terminal adapter
(recovery_events.py) and the app adapter call it; neither carries its own
prompt, model call or parser.

CALLING CONTRACT (terminal and app; the app calls it through its existing
engine-command path, which passes arguments and no stdin):

    scripts/recovery.sh triage --input <absolute JSON file>
                               [--claude <claude executable>] [--accounts <claude-accounts.json>]

Input, schema 1 (every field required; lists may be empty):
    event                 {id, kind: dispatch|send|result, at, worker, outcome, text}
                          id is the source's own stable identity (tool-use id,
                          review id); outcome is what happened (accepted, sent,
                          passed, changes-requested ...)
    original_requests     [str]  the user's own words, oldest first
    brief                 str    the worker's current assignment
    preceding             [{at, kind, text}]  earlier lead/worker activity, oldest first
    round                 {current: int|null, classified_relation: str|null, references: [str]}
    standing_instructions [{source, text}]  cited lines in force at the event
    gaps                  [str]  evidence the adapter could not supply
    ask_relation          bool   true only for an event no existing
                                 classification covers (a lead send); false
                                 for a dispatch (Pierce or the app issuer
                                 classified it) and for a handover/review result

Output: exactly one JSON object on stdout and exit 0 whenever a decision
exists, including `unavailable`:
    {schema: 1, input: <sha256 of input, prompt and model>, event: <event id>,
     decision: routine|inspect|uncertain|unavailable, reason: str,
     references: [str], relation: initial|correction|continuation|exploration|uncertain|null,
     model: str, tokens: {...}|null, cost_usd: float|null, elapsed_seconds: float|null,
     error: str, reused: bool}
Exit 2, with the reason on stderr, only when the input file is unreadable or
not schema 1; the caller treats that as unavailable too.

Identical input reuses its recorded result (one active triage per input, held
by a kernel lock), so a redelivered event or a repeated provider outage costs
no second model call. Results are kept beside the review ledger under
recovery/triage/, the existing bounded consultation storage.
"""
import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time

import recovery as R
import second_review as SR

ENGINE = Path(__file__).resolve().parents[2]
PROMPT = ENGINE / "recovery-profiles" / "triage.md"
MODEL = "haiku"
MAX_INPUT = 96 * 1024
LIMIT_SECONDS = 120
LOCK_SECONDS = 150
KINDS = ("dispatch", "send", "result")
DECISIONS = ("routine", "inspect", "uncertain")
RELATIONS = ("initial", "correction", "continuation", "exploration", "uncertain")


def schema(ask_relation):
    fields = {"decision": {"enum": list(DECISIONS)},
              "reason": {"type": "string", "minLength": 1, "maxLength": 600},
              "references": {"type": "array", "maxItems": 8, "items": {"type": "string", "minLength": 1, "maxLength": 120}}}
    if ask_relation:
        fields["relation"] = {"enum": list(RELATIONS)}
    return dict(type="object", additionalProperties=False, required=list(fields), properties=fields)


def validate(value):
    """The input contract above. Raises ValueError naming the first problem."""
    def strings(items):
        return isinstance(items, list) and all(isinstance(i, str) for i in items)
    if not isinstance(value, dict) or value.get("schema") != 1:
        raise ValueError("triage input must be a schema 1 object")
    event = value.get("event")
    if (not isinstance(event, dict) or not isinstance(event.get("id"), str) or not event["id"]
            or event.get("kind") not in KINDS or not all(isinstance(event.get(k), str) for k in ("at", "worker", "outcome", "text"))):
        raise ValueError("triage event needs id, kind (dispatch, send or result), at, worker, outcome and text")
    if not strings(value.get("original_requests")) or not isinstance(value.get("brief"), str):
        raise ValueError("triage input needs original_requests and brief")
    if not isinstance(value.get("preceding"), list) or not all(
            isinstance(p, dict) and isinstance(p.get("text"), str) for p in value["preceding"]):
        raise ValueError("triage preceding activity must be objects with text")
    round_ = value.get("round")
    if not isinstance(round_, dict) or not strings(round_.get("references", [])):
        raise ValueError("triage round must be an object")
    if not isinstance(value.get("standing_instructions"), list) or not all(
            isinstance(s, dict) and isinstance(s.get("source"), str) and isinstance(s.get("text"), str)
            for s in value["standing_instructions"]):
        raise ValueError("triage standing instructions must be cited source lines")
    if not strings(value.get("gaps")) or not isinstance(value.get("ask_relation"), bool):
        raise ValueError("triage input needs gaps and ask_relation")
    return value


def identity(value, prompt):
    return hashlib.sha256(json.dumps([value, hashlib.sha256(prompt.encode()).hexdigest(), MODEL],
                                     sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def argv(cli, shape, prompt):
    # Tool-free and settings-free: no tools, hooks, plugins, MCP or session.
    return [cli, "--print", "--safe-mode", "--restricted", "--model", MODEL, "--system-prompt", prompt,
            "--tools", "", "--allowedTools", "", "--permission-mode", "dontAsk", "--strict-mcp-config",
            "--mcp-config", '{"mcpServers":{}}', "--no-session-persistence", "--output-format", "json",
            "--json-schema", json.dumps(shape)]


def parse(answer, ask_relation):
    """(decision, reason, references, relation) or ValueError. Never routine by default."""
    if not isinstance(answer, dict):
        raise ValueError("triage returned no structured answer")
    problems = SR.shape_problems(answer, schema(ask_relation))
    if problems:
        raise ValueError("triage answer differs from its schema: %s" % "; ".join(problems[:3]))
    if not answer["reason"].strip() or len(answer["reason"]) > 600:
        raise ValueError("triage reason is empty or longer than 600 characters")
    if len(answer["references"]) > 8 or not all(r.strip() and len(r) <= 120 for r in answer["references"]):
        raise ValueError("triage references must be at most eight short input locations")
    return (answer["decision"], answer["reason"].strip(), [r for r in answer["references"]],
            answer.get("relation") if ask_relation else None)


@contextlib.contextmanager
def claimed(path):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(path, os.O_CREAT | os.O_RDWR, 0o600)
    until = time.monotonic() + LOCK_SECONDS
    try:
        while True:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic() >= until:
                    raise R.Unavailable("another triage of this exact input did not finish")
                time.sleep(0.05)
        yield
    finally:
        os.close(fd)


def triage(value, claude="", account=None):
    """The decision for one validated input. Always returns a result dict."""
    validate(value)
    prompt = PROMPT.read_text()
    ident = identity(value, prompt)
    folder = R.root() / "triage"
    stored = folder / (ident + ".json")
    with claimed(folder / (ident + ".lock")):
        try:
            previous = json.loads(stored.read_text())
            if previous.get("input") == ident:
                return dict(previous, reused=True)
        except (OSError, ValueError):
            pass
        result = dict(schema=1, input=ident, event=value["event"]["id"], decision="unavailable", reason="",
                      references=[], relation=None, model=MODEL, tokens=None, cost_usd=None,
                      elapsed_seconds=None, error="", reused=False, at=time.time(),
                      prompt_sha256=hashlib.sha256(prompt.encode()).hexdigest())
        body = json.dumps(value, ensure_ascii=False, sort_keys=True)
        cli = SR.find_claude(claude)
        held, reading = SR.quota_hold(str(ENGINE))
        if len(body.encode()) > MAX_INPUT:
            result["error"] = "triage input exceeds %d bytes; the adapter must bound it by reference" % MAX_INPUT
        elif not cli:
            result["error"] = "Claude executable unavailable for triage"
        elif held:
            result["error"] = "triage follows the existing quota hold: %s" % reading
        else:
            with tempfile.TemporaryDirectory(prefix="recovery-triage-") as tmp:
                out, err = os.path.join(tmp, "stdout.json"), os.path.join(tmp, "stderr.log")
                rc, elapsed = SR.run_bounded(argv(cli, schema(value["ask_relation"]), prompt), tmp, body,
                                             out, err, LIMIT_SECONDS, SR.claude_env(account))
                answer, facts = SR.read_claude_result(out)
                result.update(elapsed_seconds=round(elapsed, 3), tokens=facts.get("tokens"),
                              cost_usd=facts.get("cost_usd"), model=facts.get("model") or MODEL)
                try:
                    if rc != 0:
                        raise ValueError("triage provider ended with exit %s: %s" % (
                            rc, Path(err).read_text(errors="replace")[-300:].strip()))
                    decision, reason, references, relation = parse(answer, value["ask_relation"])
                    result.update(decision=decision, reason=reason, references=references, relation=relation)
                except (OSError, ValueError) as error:
                    result["error"] = str(error)
        R.F.write_json_atomic(str(stored), result)
        return result


def main(argv_=None):
    ap = argparse.ArgumentParser(description="Triage one recorded work event for Axel and Wes.")
    ap.add_argument("--input", required=True)
    ap.add_argument("--claude", default="")
    ap.add_argument("--accounts", default="")
    args = ap.parse_args(argv_)
    if not os.path.isabs(args.input):
        raise ValueError("triage --input must be an absolute path")
    value = json.loads(Path(args.input).read_text())
    account = SR.account_in_use(args.accounts) if args.accounts else None
    print(json.dumps(triage(value, args.claude, account), sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print("recovery triage: %s" % error, file=sys.stderr)
        raise SystemExit(2)
