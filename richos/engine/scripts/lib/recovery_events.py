#!/usr/bin/env python3
"""Terminal event adapter: recorded work events, triage, actual rounds, advisors.

Axel/Wes PRD 2026-10-20 sections 3 and 5. Nothing here observes anything new:
every event is read from a record that already exists, and every decision is
written beside the existing assignment (the registry record) and the existing
recovery job record. There is no new event store, scheduler or approval stage.

Events per governed worker (an implementation worker with a workspace in a
repository the entity's recovery declaration names):

  dispatch  the lead's Agent call for this registered worker, joined by its
            tool-use id to its tool result. Its round relation is the existing
            Pierce or app-issued classification; triage decides only whether
            it warrants inspection.
  send      a successful lead SendMessage to this worker, joined to the send
            hook's ledger row (notice-inflight-sends.sh) by session, recipient,
            body hash and timestamp order. Distinct sends stay distinct even
            when their text is identical. A send the hook did not record is
            still an event (the transcript witnesses it); the missing ledger
            row is reported as a coverage gap, never read as silence.
  result    a review row already written for this worker's job (handover or
            mid-job). It follows its recorded attempt and adds no round.
  refused   an Agent call whose result is an error. It adds no round and gets
            no triage call; it is recorded once in recovery/events.jsonl. A
            spawn.sh preflight refusal makes no Agent call, so no event exists.

Each new dispatch, send or result gets one triage call (recovery_triage.py,
the only triage implementation). A send triaged `correction` adds a round only
once the worker's own transcript shows the message arrived and new work
began. The witnessed start of round 4 or later, a triage decision other than
routine, an uncertain relation and an unwitnessable start each raise one alarm
on the job record, which schedules both advisors through the existing runner.
An alarm that could not be recorded stays pending on the event and is raised
again by the next drain.

The app adapter supplies its own events (its dispatch records, its loaded
instructions and its native message observation) and calls the same triage
command; process() is adapter-neutral given those events.
"""
import argparse
import contextlib
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

import recovery as R

ENGINE = Path(__file__).resolve().parents[2]
REFUSED_EVENT = "dispatch-refused"
MAX_REQUESTS = 12
MAX_REQUEST_BYTES = 24 * 1024
MAX_BRIEF = 12 * 1024
MAX_PRECEDING = 10
MAX_ITEM = 1500
MAX_CITED_LINES = 20
CITATION = re.compile(r"§\s?\d+[a-z]?|\bruling\s+\d+\b", re.I)
RECEIPT_MATCH = 300


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def quiet_projection(rec, mode):
    """The observe/questions-only projection keeps classification beside the
    live assignment; enforce uses the live assignment itself."""
    if R.quiet(mode) and rec.get("recovery_observation_assignment"):
        return rec["recovery_observation_assignment"]
    return rec.get("recovery_assignment") or {}


def chain(ws, rec, mode):
    """Workers joined only by recorded correction relationships, oldest first.
    A similar name or a common Git ancestor never joins work."""
    out, todo, seen = [], [rec], set()
    while todo:
        current = todo.pop()
        if not current or current["key"] in seen:
            continue
        seen.add(current["key"])
        out.append(current)
        assignment = quiet_projection(current, mode)
        if assignment.get("kind", "implementation") == "implementation":
            todo.extend(ws.load_agent(ref) for ref in assignment.get("correction_of") or [])
    return sorted(out, key=lambda r: (str(r.get("spawned_at") or r.get("registered_at") or ""), r["key"]))


def governed(rec, mode, repos):
    assignment = quiet_projection(rec, mode)
    return bool(assignment.get("kind", "implementation") == "implementation" and rec.get("tool_use_id")
                and rec.get("session_id") and rec.get("dispatch_transcript")
                and any(w.get("repo") and os.path.realpath(w["repo"]) in repos for w in rec.get("workspaces") or []))


# ---------------------------------------------------------------------------
# reading existing records
# ---------------------------------------------------------------------------

def _identity_module():
    path = Path(__file__).resolve().parent / "teammate-identity.py"
    spec = importlib.util.spec_from_file_location("teammate_identity_events", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def send_ledger(session):
    """(rows, path, gap): the send hook's own ledger, resolved by the same
    ladder the hook writes with. An absent ledger proves nothing."""
    try:
        team, _how = _identity_module().resolve_teams_dir(session)
    except Exception as error:  # noqa: BLE001 -- a resolver fault is a coverage gap
        team, _how = "", str(error)
    path = os.path.join(team, "inflight-notices.jsonl") if team else os.path.join(
        os.path.expanduser("~"), ".claude", "inflight-notices.jsonl")
    try:
        rows = [r for r in R.rows(path) if r.get("session_id") == session and r.get("event") == "InflightNotice"]
    except (OSError, ValueError) as error:
        return [], path, "send ledger unreadable: %s" % error
    return rows, path, "" if os.path.exists(path) else "send ledger absent at %s" % path


def human_requests(path):
    """[(timestamp, text)]: the user's own words in the lead transcript, oldest
    first, read by Pierce's collector one row at a time. Tool results, task
    notifications and generated summaries are not the user's words."""
    import pierce
    out = []
    with open(path, encoding="utf-8") as stream:
        for text in stream:
            if not text.endswith("\n"):
                break
            if "tool_result" in text or not ("user" in text or "queued_command" in text):
                continue
            try:
                row = json.loads(text)
            except ValueError:
                continue
            if isinstance(row, dict):
                out += [(str(row.get("timestamp") or ""), m) for m in pierce.human_messages([row])]
    return out


def requests_until(messages, at, transcript):
    """The user's words in force at an event: nothing said after it."""
    return bounded_requests([m for stamp, m in messages if not at or stamp <= at], transcript)


def bounded_requests(messages, transcript):
    """Keep the oldest request and the most recent ones; name what was left
    out by reference instead of silently truncating authority."""
    gaps = []
    if len(messages) > MAX_REQUESTS:
        kept = messages[:1] + messages[-(MAX_REQUESTS - 1):]
        gaps.append("%d earlier user messages between the first and the last %d are omitted by bound; all remain in %s"
                    % (len(messages) - len(kept), MAX_REQUESTS - 1, transcript))
    else:
        kept = list(messages)
    kept = [m[-4000:] for m in kept]
    while len(json.dumps(kept).encode()) > MAX_REQUEST_BYTES and len(kept) > 2:
        kept.pop(1)
        gaps.append("an older user message was omitted by size bound; it remains in %s" % transcript)
    return kept, gaps


def cited_instructions(texts, sources):
    """Lines of the standing instructions that the event, brief or preceding
    activity explicitly cite (a section or ruling number), with their source.
    A cited rule that cannot be found is an evidence gap, never presumed."""
    wanted = {re.sub(r"\s+", "", c.lower()) for t in texts for c in CITATION.findall(t or "")}
    lines, gaps = [], []
    for token in sorted(wanted):
        found = False
        for source, body in sources:
            for line in body.splitlines():
                if token in re.sub(r"\s+", "", line.lower()):
                    found = True
                    if len(lines) < MAX_CITED_LINES:
                        lines.append(dict(source=source, text=line.strip()[:600]))
        if not found:
            gaps.append("cited rule %s was not found in the standing instructions in force" % token)
    return lines, gaps


def standing_sources(rec, dispatch_input, entity):
    """The terminal reuses Pierce's instruction collector for the worker's
    definition and inherited project instructions."""
    import pierce
    data = dispatch_input or {}
    payload = dict(tool_input=dict(subagent_type=str(data.get("subagent_type") or rec.get("subagent_type") or "general"),
                                   prompt=str(data.get("prompt") or "")), cwd=entity)
    saved = os.environ.get("CLAUDE_PROJECT_DIR")
    os.environ["CLAUDE_PROJECT_DIR"] = entity
    try:
        text = pierce.instruction_text(payload)
    finally:
        if saved is None:
            os.environ.pop("CLAUDE_PROJECT_DIR", None)
        else:
            os.environ["CLAUDE_PROJECT_DIR"] = saved
    parts = re.split(r"(?m)^(?:Agent definition|Standing instructions) \((.+)\):$", text)
    return [(parts[i], parts[i + 1]) for i in range(1, len(parts) - 1, 2)]


def witnessed_start(transcript, body, after):
    """('witnessed', reference) once the worker's own transcript shows the
    message arrived and a tool call followed it; ('pending', '') before that;
    ('unavailable', why) when the transcript cannot be read."""
    import pierce
    if not transcript:
        return "unavailable", "the worker transcript is not recorded"
    needle = " ".join(body.split())[:RECEIPT_MATCH]
    received = False
    try:
        for row in pierce.transcript_rows(transcript, complete=True):
            stamp = row.get("timestamp")
            if isinstance(stamp, str) and after and stamp < after:
                continue
            if not received:
                attachment = row.get("attachment") if row.get("type") == "attachment" else None
                text = attachment.get("prompt") if isinstance(attachment, dict) and attachment.get("type") == "queued_command" else None
                if text is None and row.get("type") == "user":
                    content = (row.get("message") or {}).get("content")
                    text = content if isinstance(content, str) else "\n".join(
                        p.get("text", "") for p in content or [] if isinstance(p, dict) and p.get("type") == "text")
                received = bool(isinstance(text, str) and needle and needle in " ".join(text.split()))
                continue
            content = (row.get("message") or {}).get("content")
            if row.get("type") == "assistant" and isinstance(content, list) and any(
                    isinstance(p, dict) and p.get("type") == "tool_use" for p in content):
                return "witnessed", "%s#%s@%s" % (transcript, row.get("uuid"), stamp)
    except (OSError, ValueError) as error:
        return "unavailable", "worker transcript unreadable: %s" % error
    return "pending", ""


def lead_events(actions, rec, ledger, used):
    """The dispatch and the successful sends for one registered worker."""
    events, gaps = [], []
    for action in actions:
        data, result = action["input"], action.get("result")
        if action["tool"] == "Agent" and action["id"] == rec.get("tool_use_id"):
            if result is None:
                continue  # its result is not recorded yet
            events.append(dict(id=action["id"], kind="dispatch", at=action["timestamp"] or "", worker=rec["key"],
                               outcome="refused" if result["is_error"] else "accepted", body=str(data.get("prompt") or ""),
                               head="Agent dispatch %s (%s)" % (data.get("name") or "", data.get("description") or ""),
                               result=result["text"][:300]))
        elif (action["tool"] == "SendMessage" and data.get("to") in (rec.get("name"), rec.get("agent_id"))
              and result is not None and not result["is_error"] and isinstance(data.get("message"), str)):
            body = data["message"]
            match = next((n for n, row in enumerate(ledger) if n not in used and row.get("to") == data["to"]
                          and row.get("body_sha256") == sha(body)), None)
            if match is None:
                gaps.append("send %s to %s is in the lead transcript but not in the send ledger" % (action["id"], data["to"]))
            else:
                used.add(match)
            events.append(dict(id=action["id"], kind="send", at=action["timestamp"] or "", worker=rec["key"],
                               outcome="sent", body=body, head="SendMessage to %s (%s)" % (data["to"], data.get("summary") or ""),
                               witness="ledger" if match is not None else "transcript only"))
    return events, gaps


def result_events(rec, work, aliases, repo_ids, review_rows):
    """Review results already recorded for this job; the supervisor observed them."""
    events = []
    for row in review_rows:
        if row.get("work") not in aliases or not row.get("verdict") or not row.get("id") or not any(
                R.F.review_row_belongs(row, ident) for ident in repo_ids):
            continue
        detail = R.F.review_record(row) or {}
        findings = (detail.get("answer") or {}).get("findings") or []
        lines = ["- %s%s" % ("blocking: " if f.get("blocks") else "", f.get("title") or "") for f in findings if isinstance(f, dict)]
        events.append(dict(id="review:" + row["id"], kind="result", at=str(row.get("at") or ""), worker=rec["key"],
                           outcome=str(row["verdict"]), head="Review (%s) of %s for %s" % (row.get("trigger"), str(row.get("tip"))[:12], work),
                           body="\n".join(["verdict: " + str(row["verdict"])] + lines)))
    return events


# ---------------------------------------------------------------------------
# rounds
# ---------------------------------------------------------------------------

def round_sequence(workers):
    """Actual starts of one outcome, oldest first, each numbered. A dispatch
    that started is a start (initial, or corrective by its recorded relation);
    a send counts only as a witnessed corrective start; an exploration or new
    outcome starts again at round 1 while the earlier rounds stay recorded."""
    entries = []
    for worker in workers:
        events = worker.get("recovery_events") or {}
        dispatch = events.get(worker.get("tool_use_id")) or {}
        started = worker.get("agent_id") or dispatch.get("outcome") == "accepted"
        if started and dispatch.get("outcome") != "refused":
            entries.append(dict(id=worker.get("tool_use_id"), at=dispatch.get("at") or str(worker.get("spawned_at") or ""),
                                relation=dispatch.get("relation") or "initial", worker=worker["key"]))
        for eid, event in events.items():
            if event.get("kind") == "send" and event.get("start_state") == "witnessed" and event.get("relation") == "correction":
                entries.append(dict(id=eid, at=event.get("start_at") or event["at"], relation="correction", worker=worker["key"]))
            elif event.get("kind") == "send" and event.get("relation") in ("exploration", "initial"):
                entries.append(dict(id=eid, at=event["at"], relation="exploration", worker=worker["key"]))
    entries.sort(key=lambda e: (e["at"], e["id"]))
    count = 0
    for entry in entries:
        count = 1 if entry["relation"] in ("initial", "exploration") else count + 1
        entry["round"] = count
    return entries


# ---------------------------------------------------------------------------
# processing
# ---------------------------------------------------------------------------

def triage_input(event, worker, relation, brief, requests, gaps, preceding, cited, rounds):
    """The triage command's schema 1 input for one event, bounded by reference."""
    current = rounds[-1]["round"] if rounds else None
    if event["kind"] != "dispatch":
        relation = None
    own_gaps = list(gaps)
    if len(brief.encode()) > MAX_BRIEF:
        own_gaps.append("the brief is longer than %d bytes; its end is kept and the whole brief is the dispatch prompt" % MAX_BRIEF)
        brief = brief.encode()[-MAX_BRIEF:].decode("utf-8", "ignore")
    text = event["head"] + ":\n" + event["body"]
    return dict(schema=1,
                event=dict(id=event["id"], kind=event["kind"], at=event["at"], worker=str(worker.get("name") or worker["key"]),
                           outcome=event["outcome"], text=text[-8000:]),
                original_requests=requests, brief=brief, preceding=preceding[-MAX_PRECEDING:],
                round=dict(current=current, classified_relation=relation, references=[r["id"] for r in rounds][-6:]),
                standing_instructions=cited, gaps=own_gaps, ask_relation=event["kind"] == "send")


def record_event(ws, key, eid, value):
    with ws.Lock():
        fresh = ws.load_agent(key)
        if not fresh:
            return None
        fresh.setdefault("recovery_events", {})[eid] = value
        ws.save_agent(fresh)
        return fresh


def unavailable(event, error):
    return dict(decision="unavailable", reason="", references=[], relation=None, error=str(error), input="",
                model="", tokens=None, cost_usd=None, elapsed_seconds=None, reused=False, event=event["id"])


def raise_alarms(ws, worker, alarms, declaration, engine, summary, schedule):
    """One reconcile per alarm on the job record; advisors through the runner."""
    import recovery_controls as C
    mode = declaration["mode"]
    projected = dict(worker, recovery_assignment=quiet_projection(worker, mode))
    work, aliases, acceptance = C.work(ws, projected)
    repos = sorted({os.path.realpath(w["repo"]) for w in worker.get("workspaces") or [] if w.get("repo")}
                   & {os.path.realpath(p) for p in declaration["repositories"]})
    raised = []
    for eid, signal in alarms:
        try:
            for repo in repos:
                record = R.reconcile(repo, work, aliases, worker["session_id"], acceptance, mode=mode,
                                     premises=declaration.get("wes_policy", "carried"), signal=signal)
                if record:
                    summary["alarms"].append(dict(job=record["id"], event=eid, kind=signal["kind"], round=signal.get("round")))
                    scheduled = schedule(record, engine)
                    if scheduled and scheduled.get("observation_gap"):
                        summary["gaps"].append("advisor scheduling gap for %s: %s" % (record["id"], scheduled["observation_gap"]))
            raised.append(eid)
        except (OSError, ValueError) as error:
            summary["gaps"].append("alarm for %s kept pending: %s" % (eid, error))
    return raised


def process(ws, worker, events, context, declaration, engine, summary, triage, schedule):
    """Adapter-neutral: triage new events of one worker in order, witness
    corrective starts, count rounds across its recorded chain and raise alarms."""
    mode = declaration["mode"]
    known = worker.get("recovery_events") or {}
    for event in sorted(events, key=lambda e: (e["at"], e["id"])):
        entry = dict(known.get(event["id"]) or {})
        if not entry:
            entry = dict(kind=event["kind"], at=event["at"], outcome=event["outcome"], witness=event.get("witness", ""))
            if event["kind"] == "dispatch":
                entry["relation"] = "correction" if quiet_projection(worker, mode).get("correction_of") else "initial"
            if event["outcome"] == "refused":
                entry.update(decision="none", reason="refused before any work started; adds no round", alarm="none")
            else:
                rounds = round_sequence(chain(ws, worker, mode))
                requests, gaps = context["requests"](event)
                value = triage_input(event, worker, entry.get("relation"), context["brief"], requests,
                                     context["gaps"] + gaps, context["preceding"](event), context["cited"](event), rounds)
                try:
                    result = triage(value)
                except Exception as error:  # noqa: BLE001 -- any fault routes to inspection
                    result = unavailable(event, error)
                summary["triage_calls"] += 0 if result.get("reused") else 1
                summary["triage_cost_usd"] += result.get("cost_usd") or 0
                entry.update(decision=result["decision"], reason=result["reason"] or result.get("error", ""),
                             references=result.get("references", []), triage=result.get("input", ""),
                             model=result.get("model"), tokens=result.get("tokens"), cost_usd=result.get("cost_usd"),
                             elapsed_seconds=result.get("elapsed_seconds"),
                             alarm="pending" if result["decision"] != "routine" else "none")
                if event["kind"] == "send":
                    entry["relation"] = result.get("relation") or ("uncertain" if result["decision"] == "unavailable" else None)
                    if entry["relation"] == "uncertain":
                        entry["alarm"] = "pending"
            summary["events"].append(dict(id=event["id"], kind=event["kind"], decision=entry["decision"], relation=entry.get("relation")))
        if event["kind"] == "send" and entry.get("relation") == "correction" and entry.get("start_state") != "witnessed":
            state, reference = witnessed_start(context["worker_transcript"], event["body"], event["at"])
            if state != entry.get("start_state"):
                entry["start_state"] = state
                if state == "witnessed":
                    entry["start"], entry["start_at"] = reference, reference.rsplit("@", 1)[-1]
                elif state == "unavailable":
                    entry.update(start_gap=reference, alarm="pending")
        if entry != known.get(event["id"]):
            worker = record_event(ws, worker["key"], event["id"], entry) or worker
    rounds = round_sequence(chain(ws, worker, mode))
    numbered = {r["id"]: r["round"] for r in rounds}
    alarms = []
    events_now = worker.get("recovery_events") or {}
    label = str(worker.get("name") or worker["key"])
    for eid, entry in sorted(events_now.items(), key=lambda kv: (kv[1].get("at", ""), kv[0])):
        if entry.get("alarm") == "pending":
            if entry.get("start_state") == "unavailable":
                reason = "corrective start cannot be witnessed: " + entry.get("start_gap", "")
            elif entry.get("relation") == "uncertain" and entry.get("decision") == "routine":
                reason = "round relation uncertain: " + entry.get("reason", "")
            else:
                reason = "triage %s: %s" % (entry.get("decision"), entry.get("reason", ""))
            alarms.append((eid, R.event_signal([eid], reason, label, numbered.get(eid) or (rounds[-1]["round"] if rounds else None))))
    for entry in rounds:
        if entry["round"] >= R.ALARM_ROUND and entry["worker"] == worker["key"]:
            source = events_now.get(entry["id"]) or {}
            if source.get("round_alarm") != entry["round"]:
                alarms.append((entry["id"], R.round_signal(entry["round"], entry["id"], label)))
    if not alarms:
        return worker
    raised = raise_alarms(ws, worker, alarms, declaration, engine, summary, schedule)
    for eid, signal in alarms:
        if eid not in raised:
            continue
        entry = dict((worker.get("recovery_events") or {}).get(eid) or dict(kind="dispatch", at=""))
        if signal["kind"] == "round":
            entry["round_alarm"] = signal["round"]
        else:
            entry["alarm"] = "raised"
        worker = record_event(ws, worker["key"], eid, entry) or worker
    return worker


def record_refusals(actions, registered, summary):
    """Agent calls refused before registration: once, as adding no round."""
    path = R.root() / "events.jsonl"
    seen = {row.get("tool_use_id") for row in R.rows(path) if row.get("event") == REFUSED_EVENT}
    for action in actions:
        result = action.get("result")
        if (action["tool"] == "Agent" and result and result["is_error"] and action["id"] not in registered
                and action["id"] not in seen):
            R.F.append_jsonl(str(path), dict(at=time.time(), event=REFUSED_EVENT, tool_use_id=action["id"],
                name=str(action["input"].get("name") or ""), round_effect="none",
                reason="refused before registration; no work started", result=result["text"][:300]))
        if action["tool"] == "Agent" and result and result["is_error"] and action["id"] not in registered:
            summary["refused"].append(action["id"])


def drain(engine, entity, ws, triage=None, schedule=None):
    """One pass over every governed worker. Idempotent: recorded events are
    never triaged twice, and alarms that could not be raised are retried."""
    import recovery_runner as RR
    import recovery_triage
    import recovery_controls as C
    triage = triage or recovery_triage.triage
    schedule = schedule or C.schedule
    declaration = R.mode(entity)
    summary = dict(mode=declaration["mode"], events=[], refused=[], gaps=[], alarms=[], triage_calls=0, triage_cost_usd=0.0)
    if declaration["mode"] == "off":
        return summary
    repos = {os.path.realpath(p) for p in declaration["repositories"]}
    workers = [r for r in ws.all_agents(include_done=True) if governed(r, declaration["mode"], repos)]
    review_rows = R.rows(R.F.review_ledger_default())
    by_lead = {}
    for worker in workers:
        by_lead.setdefault(worker["dispatch_transcript"], []).append(worker)
    for transcript, group in by_lead.items():
        try:
            actions = RR.transcript_actions(transcript)
        except (OSError, ValueError) as error:
            summary["gaps"].append("lead transcript unavailable, its workers' events are not observed: %s" % error)
            continue
        registered = {w.get("tool_use_id") for w in ws.all_agents(include_done=True) if w.get("tool_use_id")}
        record_refusals(actions, registered, summary)
        session = group[0]["session_id"]
        ledger, ledger_path, ledger_gap = send_ledger(session)
        if ledger_gap:
            summary["gaps"].append(ledger_gap)
        used = set()
        messages = []
        try:
            messages = human_requests(transcript)
        except (OSError, ValueError, RecursionError) as error:
            summary["gaps"].append("the user's own words are unavailable from %s: %s" % (transcript, error))
        for worker in sorted(group, key=lambda w: str(w.get("spawned_at") or "")):
            dispatch = next((a for a in actions if a["id"] == worker["tool_use_id"]), None)
            if dispatch is None:
                summary["gaps"].append("dispatch %s of %s is not in its lead transcript" % (worker["tool_use_id"], worker["key"]))
                continue
            events, gaps = lead_events(actions, worker, ledger, used)
            summary["gaps"].extend(gaps)
            projected = dict(worker, recovery_assignment=quiet_projection(worker, declaration["mode"]))
            try:
                work, aliases, _acceptance = C.work(ws, projected)
            except Exception as error:  # noqa: BLE001 -- registry SpecError is not a ValueError
                summary["gaps"].append("job identity unavailable for %s: %s" % (worker["key"], error))
                continue
            # A handover or review result follows its recorded attempt and is
            # processed once per job, under its newest worker.
            members = chain(ws, worker, declaration["mode"])
            if members and members[-1]["key"] == worker["key"]:
                taken = {eid for m in members[:-1] for eid in (m.get("recovery_events") or {})}
                repo_ids = {R.F.review_repo_identity(w["repo"]) for w in worker.get("workspaces") or [] if w.get("repo")}
                events += [e for e in result_events(worker, work, aliases, repo_ids, review_rows) if e["id"] not in taken]
            if not events:
                continue
            context = event_context(ws, transcript, messages, worker, dispatch, events, actions, entity)
            try:
                process(ws, worker, events, context, declaration, engine, summary, triage, schedule)
            except Exception as error:  # noqa: BLE001 -- one worker's fault never stops the drain
                summary["gaps"].append("events of %s not processed: %s" % (worker["key"], error))
    return summary


def event_context(ws, transcript, messages, worker, dispatch, events, actions, entity):
    """What triage reads for this worker's events, each bounded by reference."""
    gaps = []
    try:
        sources = standing_sources(worker, dispatch["input"], entity)
    except (OSError, ValueError, KeyError) as error:
        sources = []
        gaps.append("standing instructions unavailable: %s" % error)
    import recovery_runner as RR
    brief = str(dispatch["input"].get("prompt") or "")
    history = [a for a in actions if a["tool"] == "SendMessage" and a.get("result") and not a["result"]["is_error"]
               and a["input"].get("to") in (worker.get("name"), worker.get("agent_id"))]

    def requests(event):
        if not messages:
            return [], ["the user's original requests could not be read from %s" % transcript]
        return requests_until(messages, event["at"], transcript)

    def preceding(event):
        items = [dict(at=a["timestamp"] or "", kind="lead-send", text=RR.action_text(a)[:MAX_ITEM])
                 for a in history if (a["timestamp"] or "") < event["at"]]
        items += [dict(at=e["at"], kind="review-result", text=(e["head"] + ": " + e["body"])[:MAX_ITEM])
                  for e in events if e["kind"] == "result" and e["at"] < event["at"]]
        return sorted(items, key=lambda i: i["at"])

    def cited(event):
        lines, missing = cited_instructions([event["body"], brief], sources)
        return lines + [dict(source="evidence gap", text=g) for g in missing]

    worker_transcript = ""
    try:
        worker_transcript = ws.platform_agent_transcript(worker)
    except (OSError, ValueError):
        gaps.append("the worker transcript could not be located")
    return dict(brief=brief, requests=requests, gaps=gaps, preceding=preceding, cited=cited,
                worker_transcript=worker_transcript)


# ---------------------------------------------------------------------------
# watcher entry: a drain runs only when an input it reads has changed
# ---------------------------------------------------------------------------

def fingerprint(ws, entity):
    declaration = R.mode(entity)
    if declaration["mode"] == "off":
        return None
    repos = {os.path.realpath(p) for p in declaration["repositories"]}
    def stat(path):
        try:
            s = os.stat(path)
            return [s.st_size, s.st_mtime_ns]
        except (OSError, TypeError):
            return None
    parts = [declaration["mode"], stat(R.F.review_ledger_default())]
    for worker in ws.all_agents(include_done=True):
        if governed(worker, declaration["mode"], repos):
            parts.append([worker["key"], stat(worker.get("dispatch_transcript")),
                          stat(ws.platform_agent_transcript(worker)), sorted(worker.get("recovery_events") or {})])
            parts.append(stat(send_ledger(worker["session_id"])[1]))
    return R.digest(parts)


def launch(engine, entity, ws, env=None):
    """Start one detached drain when anything it reads changed. Never waits."""
    try:
        current = fingerprint(ws, entity)
    except (OSError, ValueError) as error:
        return dict(gap="event drain fingerprint unavailable: %s" % error)
    if current is None:
        return None
    cursor = R.root() / "events.cursor.json"
    try:
        if json.loads(cursor.read_text()).get("fingerprint") == current:
            return None
    except (OSError, ValueError):
        pass
    R.root().mkdir(parents=True, exist_ok=True)
    log = R.root() / "events.drain.log"
    with log.open("ab") as stream:
        child = subprocess.Popen([sys.executable, "-B", str(Path(__file__).resolve()), "--engine", engine, "--entity", entity],
                                 stdin=subprocess.DEVNULL, stdout=stream, stderr=stream, env=dict(os.environ, **(env or {})),
                                 start_new_session=True)
    return dict(pid=child.pid)


def main(argv=None):
    ap = argparse.ArgumentParser(description="Drain recorded work events into triage and advisor alarms.")
    ap.add_argument("--engine", required=True)
    ap.add_argument("--entity", required=True)
    args = ap.parse_args(argv)
    import second_review as SR
    ws = SR.load_workspaces(args.engine)
    R.root().mkdir(parents=True, exist_ok=True)
    fd = os.open(R.root() / "events.drain.lock", os.O_CREAT | os.O_RDWR, 0o600)
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return 0  # one drain at a time; the next tick picks up what changed
        before = fingerprint(ws, args.entity)
        summary = drain(args.engine, args.entity, ws)
        if not any("kept pending" in g for g in summary["gaps"]):
            R.F.write_json_atomic(str(R.root() / "events.cursor.json"), dict(fingerprint=before, at=time.time()))
        print(json.dumps(dict(summary, at=time.time()), sort_keys=True), flush=True)
        return 0
    finally:
        os.close(fd)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print("recovery events: %s" % error, file=sys.stderr)
        raise SystemExit(2)
