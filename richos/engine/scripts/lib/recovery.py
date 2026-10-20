#!/usr/bin/env python3
"""Host-owned recovery dispositions. Advisors supply proposals, never authority.

This is an extension of review state, not workspace lifecycle ownership. Each
repository/job has one locked record and an append-only event history. Admission
only reads/reconciles records; model execution belongs to the asynchronous runner.
"""
import contextlib
import fcntl
import hashlib
import json
import os
import math
import re
import sys
import time
from pathlib import Path

import operator_fences as F
import machine_settings as M

THRESHOLD = 3
ALARM_ROUND = 4  # the start of the fourth actual round of one unresolved outcome
MAX_RECOVERIES = 3
EXECUTION_SECONDS = 3 * 3600
LOCK_SECONDS = 4
MAX_LEDGER_BYTES = 16 * 1024 * 1024
# questions-only (PRD 2026-10-20 section 8): triage, advisors and owner question
# delivery run; worker controls, holds, budgets and finding resolution stay as
# in observe. Every mode branch asks one of these two questions explicitly.
MODES = ("off", "observe", "questions-only", "enforce")


def quiet(mode):
    """Observe-side branch: record gaps and proposals, never refuse or control."""
    return mode in ("observe", "questions-only")


def asks(mode):
    """Enforce-side branch for owner questions and acknowledged delivery only."""
    return mode in ("questions-only", "enforce")


class Unavailable(ValueError):
    pass


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def root(ledger=None):
    return Path(ledger or F.review_ledger_default()).parent / "recovery"


def storage_root(kind):
    """Host-owned durable storage. Explicit roots are portable and absolute."""
    if kind not in ("evidence", "wip"):
        raise Unavailable("unknown recovery storage kind")
    try:
        return M.state("recovery-" + kind, "RICHOS_RECOVERY_" + kind.upper() + "_ROOT", default=root() / kind)
    except (ValueError, OSError) as error:
        raise Unavailable(str(error)) from error


def key(repo, work):
    return digest([F.review_repo_identity(repo), work])


def rows(path):
    try:
        if Path(path).stat().st_size > MAX_LEDGER_BYTES:
            raise Unavailable("review ledger exceeds bounded recovery input; compact by retained identity")
        data = Path(path).read_bytes()
    except FileNotFoundError:
        return []
    result = []
    for line in data.splitlines(keepends=True):
        if not line.endswith(b"\n"):
            break  # A writer may be appending the final row.
        row = json.loads(line)
        if not isinstance(row, dict):
            raise Unavailable("non-object review record in %s" % path)
        result.append(row)
    return result


@contextlib.contextmanager
def locked(identity, ledger=None):
    root(ledger).mkdir(parents=True, exist_ok=True)
    fd = os.open(root(ledger) / (identity + ".lock"), os.O_CREAT | os.O_RDWR, 0o600)
    until = time.monotonic() + LOCK_SECONDS
    try:
        while True:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic() >= until:
                    raise Unavailable("recovery state is busy; dependent admission refused")
                time.sleep(0.02)
        yield
    finally:
        os.close(fd)


def read(identity, ledger=None):
    if not re.fullmatch(r"[0-9a-f]{64}", identity):
        raise Unavailable("invalid recovery identity")
    path = root(ledger) / (identity + ".json")
    try:
        value = json.loads(path.read_text())
    except FileNotFoundError:
        return None
    if not isinstance(value, dict) or value.get("schema") != 1 or value.get("id") != identity:
        raise Unavailable("invalid recovery record %s" % path)
    for field in ("deadline", "wes_deadline", "created_at"):
        if not isinstance(value.get(field), (int, float)) or not math.isfinite(value[field]):
            raise Unavailable("invalid recovery budget %s" % path)
    if (value.get("mode") not in MODES[1:] or value.get("status") not in
            ("investigating", "reviewing_premise", "recovering", "accepted", "resolved_findings", "awaiting_alex", "unavailable")
            or any(not isinstance(value.get(k), list) for k in ("attempts", "controls", "consults", "budgets"))):
        raise Unavailable("invalid recovery state %s" % path)
    return value


def save(record, event, ledger=None):
    F.write_json_atomic(str(root(ledger) / (record["id"] + ".json")), record)
    F.append_jsonl(str(root(ledger) / "events.jsonl"), dict(at=time.time(), job=record["id"],
                   event=event, revision=digest(record), status=record["status"],
                   axel_required=record.get("axel_required"), wes_candidates=record.get("wes_candidates", []),
                   trigger_failures=len(record["failed"]), recovery_failures=record.get("recovery_failures", 0)))


def conflict_task(repo, work, tip, ledger=None):
    """One durable independent reconciliation for contradictory source verdicts.

    A recheck that upholds the blocker finishes this task without clearing it.
    This is review correctness and does not depend on recovery mode.
    """
    source=rows(ledger or F.review_ledger_default())
    ident=F.review_repo_identity(repo)
    history=[r for r in source if r.get('work')==work and r.get('verdict') and F.review_row_belongs(r,ident)]
    scoped=[r for r in history if r.get('tip')==tip and r.get('trigger') in F.REVIEW_LAND_KINDS]
    if not scoped:return None
    end=max(i for i,r in enumerate(history) if r.get('tip')==tip and r.get('trigger') in F.REVIEW_LAND_KINDS)
    history=history[:end+1]
    effective,_=F.review_of(history,tip,repo)
    identity=digest(dict(repo=ident,work=work,tip=tip,purpose='outcome-reconciliation'))
    path=root(ledger)/'reconciliations'/(identity+'.json')
    with locked(identity,ledger):
        task=json.loads(path.read_text()) if path.exists() else None
        if task:
            reviewed=[r for r in scoped if r.get('recheck_of')==task['prior']]
            if reviewed:
                task.update(status='resolved' if effective.get('verdict')=='passed' else 'reviewed-unresolved',review=reviewed[-1]['id'])
                F.write_json_atomic(str(path),task)
                return None
            return task if task['status']=='requested' else None
        if effective.get('verdict')!='changes-requested' or not any(r.get('verdict')=='passed' for r in scoped):return None
        blocked=effective.get('effective_findings') or {}
        producers={fid.split('#',1)[0] for fid in blocked}
        prior=next((r for r in reversed(scoped) if r.get('id') in producers or r.get('verdict')=='changes-requested'),scoped[-1])
        task=dict(schema=1,id=identity,repo=repo,repo_id=ident,work=work,tip=tip,status='requested',prior=prior['id'],
                  evidence=[r['id'] for r in history],findings=sorted(blocked),created_at=time.time(),
                  instruction='Independently reconcile these conflicting reviews at the exact unchanged source. Read original authority and every referenced verdict. Establish each blocking finding explicitly as upheld, fixed or withdrawn; neither majority vote nor a later PASS resolves it.')
        path.parent.mkdir(parents=True,exist_ok=True)
        F.write_json_atomic(str(path),task)
        return task


def failed_attempts(ledger, repo, aliases):
    ident = F.review_repo_identity(repo)
    handovers = [r for r in ledger if r.get("work") in aliases and r.get("verdict")
                 and r.get("trigger") in F.REVIEW_LAND_KINDS and F.review_row_belongs(r, ident)]
    tips = list(dict.fromkeys(r.get("tip") for r in handovers if r.get("tip")))
    failed = []
    for tip in tips:
        effective, _ = F.review_of(handovers, tip, repo)
        if effective and effective.get("verdict") == "changes-requested":
            failed.append(dict(tip=tip, reviews=sorted({r["id"] for r in handovers if r.get("tip") == tip})))
    return failed


def round_signal(number, start, label, reason=""):
    """The witnessed start of actual round 4 or later summons both advisors.

    Round 1 is the initial actual attempt at one unresolved outcome and each
    corrective pass is the next round (PRD 2026-10-20 section 5). A visible
    name suffix is not a round. This is an alarm, not proof of a defect: it
    bypasses only the routine/inspect decision, never the need for a witnessed
    start. `start` is the event identity that witnessed the round beginning.
    """
    if not isinstance(number, int) or number < ALARM_ROUND:
        return None
    return dict(kind="round", round=number, events=[start], label=label,
                reason=reason or "actual round %d of one unresolved outcome started" % number)


def event_signal(events, reason, label, round_number=None):
    """Triage judged these work events to warrant both advisors (inspect,
    uncertain or unavailable). The reason cites the evidence, never a verdict."""
    return dict(kind="event", round=round_number, events=list(events), label=label, reason=reason)


def signal_label(signal):
    """A short human label for an alarm, for owner notices."""
    signal = signal or {}
    label = str(signal.get("label") or "work")
    return label + (" (round %d)" % signal["round"] if signal.get("kind") == "round" else "")


def merge_signal(previous, new, asked):
    """One alarm per assignment frontier. Until both advisors have asked their
    questions, a new concern joins the pending alarm and keeps its identity, so
    an in-flight consultation is not invalidated. After questions exist, a new
    material event or a later round reopens scrutiny under a new identity.
    Nothing that raised a concern is discarded: its event stays listed.
    Returns (signal, reopened)."""
    events = list(dict.fromkeys((previous or {}).get("events", []) + new.get("events", [])))
    if previous and set(new.get("events", [])) <= set(previous.get("events", [])) and (
            new.get("round") or 0) <= (previous.get("round") or 0):
        return previous, False  # duplicate delivery of a known concern
    if previous and not asked:
        merged = dict(previous, events=events, round=max(previous.get("round") or 0, new.get("round") or 0) or None,
                      reason=previous["reason"] if new["reason"] in previous["reason"] else previous["reason"] + "; " + new["reason"])
        if new.get("kind") == "round":
            merged.update(kind="round", label=new["label"])
        return merged, False
    signal = dict(new, events=events if previous else new.get("events", []))
    signal["id"] = digest([signal["kind"], signal.get("round"), signal["events"]])
    return signal, True


def repeat_pending(record, role):
    return bool(record.get("repeat_signal") and
                (record.get("repeat_questions", {}).get(role) or {}).get("frontier") != record["evidence_cursor"])


def questioning(record):
    return bool(record.get("repeat_signal") and not record.get("repeat_recovery") and
                not record["attempts"] and record["status"] != "awaiting_alex" and
                not any(c["action"] != "continue" for c in record["controls"]))


def repeat_question_key(record, role):
    question = record.get("repeat_questions", {}).get(role) or {}
    return "recovery:%s:questions:%s" % (record["id"], digest(
        [record["evidence_cursor"], role, question.get("consultation")]))


def repeat_ready(record):
    """Questions reach the lead before a later judgment can run."""
    return all(not repeat_pending(record, role) and
               (not asks(record["mode"]) or repeat_question_key(record, role) in record.get("deliveries", {}))
               for role in ("axel", "wes"))


def settle_questions(record):
    # Delivery is not an answer. A satisfactory reply lets Rich correct course;
    # it does not waive ordinary review, acceptance or any existing stop.
    if (record.get("repeat_outcome") == "self_correction" and repeat_ready(record) and
            record["status"] not in ("accepted", "awaiting_alex", "unavailable") and
            not record["attempts"] and not record.get("repeat_recovery") and
            not any(c["action"] != "continue" for c in record["controls"])):
        record["status"] = "resolved_findings"


def repeat_questioned(identity, role, answer, consultation, sources=()):
    """Store probing questions without requiring or granting a verdict."""
    with locked(identity):
        record = read(identity)
        if (role not in ("axel", "wes") or set(answer) != {"job", "acceptance", "frontier", "questions"} or
                answer["job"] != identity or answer["acceptance"] != record["acceptance"] or
                answer["frontier"] != record["evidence_cursor"] or
                not isinstance(answer["questions"], list) or not 1 <= len(answer["questions"]) <= 2 or
                any(not isinstance(q, dict) or set(q) != {"question", "evidence"} or
                    any(not isinstance(q[k], str) or not q[k].strip() for k in ("question", "evidence"))
                    for q in answer["questions"])):
            raise Unavailable("questions lack exact current job applicability or evidence")
        if record["status"] in ("accepted", "awaiting_alex", "unavailable"):
            raise Unavailable("late questions cannot reopen a resolved or escalated job")
        record.setdefault("repeat_questions", {})[role] = dict(frontier=answer["frontier"],
            questions=answer["questions"], consultation=consultation, sources=list(sources))
        settle_questions(record)
        save(record, "repetition-questions-ready")
        return record


def repeat_response(identity, role, answer, consultation, replies):
    """Only real, distinct replies to delivered questions can justify escalation."""
    with locked(identity):
        record = read(identity)
        question = record.get("repeat_questions", {}).get(role) or {}
        prior = record.setdefault("repeat_responses", {}).setdefault(role, [])
        used = {uid for row in prior for uid in row["answer_ids"]}
        available = {row["uuid"] for row in replies}
        ids = answer.get("answer_ids") or []
        outcome = answer.get("outcome")
        if (answer.get("job") != identity or answer.get("acceptance") != record["acceptance"] or
                answer.get("frontier") != record["evidence_cursor"] or
                answer.get("question") != question.get("consultation") or
                repeat_question_key(record, role) not in record.get("deliveries", {}) or
                not ids or len(set(ids)) != len(ids) or not set(ids) <= available or set(ids) & used or
                outcome not in ("self_correction", "probe_again", "unrelated", "escalate") or
                not isinstance(answer.get("reason"), str) or not answer["reason"].strip()):
            raise Unavailable("response lacks new witnessed answers to delivered questions")
        bad = [row for row in prior if row["frontier"] == record["evidence_cursor"] and row["outcome"] == "probe_again"]
        if outcome == "escalate" and not any(row["question"] != question["consultation"] for row in bad):
            raise Unavailable("stronger intervention requires repeated unsatisfying answers, after probing again")
        followup = answer.get("questions")
        if outcome == "probe_again" and (not isinstance(followup, list) or not 1 <= len(followup) <= 2 or
                any(not isinstance(q, dict) or set(q) != {"question", "evidence"} or
                    any(not isinstance(q[k], str) or not q[k].strip() for k in ("question", "evidence")) for q in followup)):
            raise Unavailable("an unsatisfying first answer requires a probing follow-up question")
        prior.append(dict(frontier=record["evidence_cursor"], question=question["consultation"],
            answer_ids=ids, outcome=outcome, reason=answer["reason"], consultation=consultation))
        if outcome == "probe_again":
            record["repeat_questions"][role] = dict(question, questions=followup, consultation=consultation)
        elif outcome == "self_correction":
            record["repeat_outcome"] = "self_correction"
        elif outcome == "escalate":
            record["repeat_recovery"] = True
        record["update_needed"] = consultation
        settle_questions(record)
        save(record, "repetition-answer-assessed")
        return record


def reconcile(repo, work, aliases, owner, acceptance, now=None, mode="observe", dry=False, premises=False, signal=None):
    """Level-triggered, including when no watcher has polled. The original
    deadline survives source movement and worker replacement. `signal` is an
    actual-round or triage-event alarm from the event adapter; without one an
    existing alarm is kept unchanged.
    """
    now = time.time() if now is None else now
    repetition = signal
    ledger = rows(F.review_ledger_default())
    failed = failed_attempts(ledger, repo, aliases)
    relevant = [r for r in ledger if r.get("work") in aliases and F.review_row_belongs(r, F.review_repo_identity(repo))]
    opened = F.open_review_findings(relevant)
    candidates = [fid for fid, finding in opened.items() if finding.get("blocks")] if premises else []
    if premises == "carried":
        carried = set()
        for row in relevant:
            detail = F.review_record(row)
            if not detail:
                continue
            for earlier in (detail.get("answer") or {}).get("earlier_findings", []):
                if earlier.get("status") == "still-open" and earlier.get("blocks"):
                    carried.add(earlier.get("id"))
        candidates = [fid for fid in candidates if fid in carried]
    identity = key(repo, work)
    with (contextlib.nullcontext() if dry else locked(identity)):
        record = read(identity)
        if record is None and len(failed) < THRESHOLD and not candidates and not repetition:
            return None
        if record is None:
            record = dict(schema=1, id=identity, repo=os.path.realpath(repo),
                repo_id=F.review_repo_identity(repo), work=work, aliases=sorted(aliases),
                acceptance=acceptance, owner=owner, created_at=now, deadline=now + EXECUTION_SECONDS,
                status="investigating" if len(failed)>=THRESHOLD else "reviewing_premise",
                mode=mode, failed=failed, plan=None, attempts=[], controls=[], axel_required=len(failed)>=THRESHOLD,
                wes_deadline=now+EXECUTION_SECONDS,
                consults=[], evidence_cursor=0, cancellation=[], escalation=None, plans=[],
                budgets=[dict(start=now, deadline=now + EXECUTION_SECONDS, reason="initial recovery")])
        if record["acceptance"] != acceptance:
            raise Unavailable("acceptance changed without an explicit recovery reconciliation")
        before = digest(record)
        previous_count = record.get("recovery_failures", 0)
        record.update(failed=failed, aliases=sorted(aliases), owner=owner, mode=mode, wes_candidates=candidates)
        previous = record.get("repeat_signal")
        if repetition:
            asked = any((record.get("repeat_questions", {}).get(role) or {}).get("frontier") == record["evidence_cursor"]
                        for role in ("axel", "wes"))
            merged, reopened = merge_signal(previous, repetition, asked)
            record["repeat_signal"] = merged
            if reopened:
                record["repeat_questions"] = {}
                record.pop("repeat_outcome", None)
                if record["status"] in ("accepted", "resolved_findings", "reviewing_premise"):
                    record.update(status="investigating", plan=None, deadline=now + EXECUTION_SECONDS,
                                  wes_deadline=now + EXECUTION_SECONDS)
                    record["budgets"].append(dict(start=now, deadline=record["deadline"],
                                                  reason="new material work event or later actual round"))
        if len(failed)>=THRESHOLD and (not record.get("axel_required", True) or
                (record["status"]=="resolved_findings" and record.get("repeat_outcome") != "self_correction")):
            record["deadline"] = now + EXECUTION_SECONDS
            record["budgets"].append(dict(start=now, deadline=record["deadline"], reason="recovery threshold crossed after premise resolution"))
            record["status"] = "investigating"
            record["plan"] = None
        record["axel_required"] = record.get("axel_required", True) or len(failed)>=THRESHOLD or bool(record.get("repeat_signal"))
        if record["axel_required"] and record["status"] == "reviewing_premise":
            record["status"] = "investigating"
        frontier = digest([failed, record["repeat_signal"]["id"]]) if record.get("repeat_signal") else digest(failed)
        if record.get("repeat_signal") and frontier != record["evidence_cursor"] and record.get("repeat_outcome"):
            record.pop("repeat_outcome", None)
            if record["status"] == "resolved_findings":
                record["status"] = "investigating"
        record["evidence_cursor"] = frontier
        failures = {a["tip"] for a in failed}
        for attempt in record["attempts"]:
            attempt["failed"] = bool(attempt.get("tip") in failures)
        count = len({a["tip"] for a in record["attempts"] if a.get("failed")})
        record["recovery_failures"] = count
        if record["status"] == "awaiting_alex" or count >= MAX_RECOVERIES:
            record["status"] = "awaiting_alex"
            record["escalation"] = record["escalation"] or dict(at=now,
                reason="Three recovery attempts failed. Alex requires escalation before any further correction.",
                failures=record["attempts"], acceptance=record["acceptance"], outcome=record["work"],
                constraints="No fourth correction or budget reset without a new explicit Alex decision.",
                options=["Reauthorize with a changed outcome or new constraints", "Defer the job", "Discard the job"],
                approaches=[p.get("approach") for p in record.get("plans",[])], delivered=False)
        elif not record.get("repeat_signal") and len(failed) < THRESHOLD and not record["attempts"] and (record["axel_required"] or not candidates):
            record["status"] = "resolved_findings"
        elif count > previous_count:
            # A newly established recovery failure permits the next bounded
            # diagnosis. Worker/source updates alone never renew its budget.
            if record.get("plan"):
                record["plans"].append(record["plan"])
            record.update(status="investigating", plan=None, deadline=now + EXECUTION_SECONDS)
            record["budgets"].append(dict(start=now, deadline=record["deadline"],
                                         reason="established recovery failure", failures=count))
        elif now >= record["deadline"] and record["status"] == "investigating":
            record["status"] = "unavailable"
        if now>=record["wes_deadline"] and candidates:
            record["wes_status"] = "unavailable"
        if not dry and (before != digest(record) or not (root() / (identity + ".json")).exists()):
            save(record, "reconciled")
        return record


def admission(record, worker, brief, disposition="", dry=False, replaces="", safe=None):
    """Reserve a recovery attempt atomically. Unchanged dispatch retries cannot
    reserve another slot, and pending review does not allow a fourth attempt.
    """
    if not record or record["mode"] != "enforce":
        return None
    with (contextlib.nullcontext() if dry else locked(record["id"])):
        record = read(record["id"])
        if questioning(record):
            return None  # Questions or silence do not authorize taking over work.
        if record["status"] in ("awaiting_alex", "unavailable"):
            raise Unavailable("job %s is %s" % (record["id"], record["status"]))
        if record["status"] in ("accepted", "resolved_findings"):
            return None
        rejected = [c for c in record["controls"] if c["action"] in ("stop", "redirect")]
        plan = record.get("plan")
        if not plan:
            if rejected:
                raise Unavailable("the rejected direction is stopped; an accepted replacement is required")
            return None  # useful implementation overlaps the finite investigation
        if disposition != plan["id"] or not plan.get("accepted"):
            raise Unavailable("replacement requires accepted recovery disposition %s" % plan["id"])
        if plan["acceptance"] != record["acceptance"] or plan["frontier"] != record["evidence_cursor"]:
            raise Unavailable("recovery evidence or acceptance changed; update the existing investigation")
        if digest(brief) != plan["brief_identity"]:
            raise Unavailable("replacement brief differs from the accepted direction")
        # This callback is the host's workspace observer, never an advisor's
        # assertion. It must verify every affected writer before replacement.
        affected = {w for c in record["controls"] for w in c.get("workers", {})}
        if affected and (safe is None or not all(safe(w) for w in affected)):
            raise Unavailable("affected writers are not demonstrably quiescent; replacement held")
        prior = [a for a in record["attempts"] if a["worker"] == worker]
        if prior:
            return prior[0]
        pending = [a for a in record["attempts"] if not a.get("tip")]
        if pending:
            attempt = pending[-1]
            if replaces != attempt["worker"] or attempt["plan"] != plan["id"]:
                raise Unavailable("a recovery attempt already owns implementation; name its safely stopped replacement")
            if safe is None or not safe(replaces):
                raise Unavailable("infrastructure replacement has no host proof of quiescence")
            if not dry:
                attempt.update(worker=worker)
                save(record, "recovery-worker-replaced")
            return attempt
        if len(record["attempts"]) >= MAX_RECOVERIES:
            raise Unavailable("three recovery attempts are reserved; no automatic fourth attempt")
        if len(record["attempts"]) == 2 and plan["approach"] in {a["approach"] for a in record["attempts"]}:
            raise Unavailable("the third recovery must use a different approved approach")
        attempt = dict(number=len(record["attempts"]) + 1, worker=worker, plan=plan["id"],
                       approach=plan["approach"], tip=None, failed=False, admitted_at=time.time())
        if not dry:
            record["attempts"].append(attempt)
            save(record, "recovery-admitted")
        return attempt


def handover(identity, worker, tip):
    with locked(identity):
        record = read(identity)
        if not record:
            return
        record.setdefault("handovers", {})[worker] = tip
        for attempt in record["attempts"]:
            if attempt["worker"] == worker:
                if attempt.get("tip") and attempt["tip"] != tip:
                    raise Unavailable("one recovery handover cannot change source identity")
                attempt["tip"] = tip
                save(record, "recovery-handover")
                return
        save(record, "implementation-handover")


def plan_problems(value, record):
    if not isinstance(value, dict):
        return ["proposal must be an object"]
    required = {"job", "acceptance", "frontier", "diagnosis", "matrix", "proof", "approach", "brief", "control", "disproof", "retained_evidence"}
    if set(value) != required:
        return ["proposal fields differ from the host schema"]
    if (value["job"], value["acceptance"], value["frontier"]) != (
            record["id"], record["acceptance"], record["evidence_cursor"]):
        return ["proposal does not match the current job/evidence"]
    if not all(isinstance(value[k], str) and value[k].strip() for k in ("diagnosis", "approach", "brief", "disproof")):
        return ["diagnosis, approach and replacement brief must be substantive"]
    if not isinstance(value["retained_evidence"], list) or not all(isinstance(v,str) and v.strip() for v in value["retained_evidence"]):
        return ["retained valid evidence must be explicitly listed"]
    if (not isinstance(value["matrix"], list) or not isinstance(value["proof"], list)
            or not value["matrix"] or not value["proof"]):
        return ["acceptance matrix and proof plan required"]
    conditions = set()
    for item in value["matrix"]:
        if not isinstance(item, dict) or set(item) != {"condition", "evidence"} or not all(
                isinstance(v, str) and v.strip() for v in item.values()):
            return ["matrix requires observable conditions and supporting evidence"]
        conditions.add(item["condition"])
    coverage = set()
    for item in value["proof"]:
        if not isinstance(item, dict) or set(item) != {"condition", "check"} or not all(
                isinstance(v, str) and v.strip() for v in item.values()):
            return ["proof requires a qualified check for each condition"]
        coverage.add(item["condition"])
    if coverage != conditions:
        return ["proof does not cover exactly the acceptance matrix"]
    if value["control"] not in ("continue", "stop", "redirect"):
        return ["invalid control action"]
    return []


def propose(identity, proposal, consultation):
    with locked(identity):
        record = read(identity)
        problems = plan_problems(proposal, record)
        if problems:
            raise Unavailable("; ".join(problems))
        if time.time() >= record["deadline"] or record["status"] in ("awaiting_alex", "unavailable"):
            raise Unavailable("late proposal cannot reopen expired or escalated recovery")
        record["plan"] = dict(proposal, id=digest([consultation, proposal]), accepted=False,
                             brief_identity=digest(proposal["brief"]), consultation=consultation)
        # Applying control needs its own exact worker/source binding. A plan
        # alone never grants permission to stop an unspecified successor.
        save(record, "recovery-proposed")
        return record["plan"]


def accept(identity, plan_id, assessment):
    with locked(identity):
        record = read(identity)
        plan = record.get("plan")
        if not plan or plan["id"] != plan_id or plan["frontier"] != record["evidence_cursor"]:
            raise Unavailable("independent assessment is stale")
        if time.time() >= record["deadline"] or record["status"] in ("awaiting_alex", "unavailable"):
            raise Unavailable("late assessment cannot reset expired or escalated recovery")
        if (not isinstance(assessment, dict) or assessment.get("plan") != plan_id
                or assessment.get("accepted") is not True or not assessment.get("evidence")
                or assessment.get("reviewer") == plan["consultation"]):
            raise Unavailable("matching independent assessment with evidence required")
        if len(record["attempts"]) >= 2:
            if (assessment.get("materially_different") is not True or not assessment.get("difference_evidence")
                    or plan["approach"] in {a["approach"] for a in record["attempts"]}):
                raise Unavailable("third recovery requires independent evidence of a materially different approach")
        plan.update(accepted=True, assessment=assessment)
        record["status"] = "recovering"
        save(record, "recovery-plan-accepted")


def landing_gap(repo, work, ledger=None):
    record = read(key(repo, work), ledger)
    if record and record["mode"]=="enforce":
        for control in record["controls"]:
            if control["action"]=="continue":
                continue
            for worker in control["workers"]:
                observed = control.get("observations", {}).get(worker) or {}
                if not observed.get("quiescent") or observed.get("cancellation_status")!="acknowledged":
                    return "job %s has unresolved recovery control of %s" % (record["id"], worker)
        if not questioning(record) and record.get("axel_required", True) and record["status"] not in ("resolved_findings", "accepted"):
            return "job %s has unresolved recovery (%s)" % (record["id"], record["status"])
    return ""


def control(identity, recommendation, consultation):
    """Interim controls are independent of plan acceptance. The host still
    checks each exact worker and source before disabling its tools.
    """
    with locked(identity):
        record = read(identity)
        required = {"job", "acceptance", "frontier", "action", "workers", "reason", "evidence"}
        if (not isinstance(recommendation, dict) or set(recommendation) != required
                or recommendation["job"] != identity
                or recommendation["acceptance"] != record["acceptance"]
                or recommendation["frontier"] != record["evidence_cursor"]
                or recommendation["action"] not in ("continue", "stop", "redirect")
                or not isinstance(recommendation["workers"], dict)
                or not all(isinstance(v, str) and re.fullmatch(r"[0-9a-f]{40}:[0-9a-f]{64}", v) for v in recommendation["workers"].values())
                or not all(isinstance(recommendation[k], str) and recommendation[k].strip()
                           for k in ("reason", "evidence"))):
            raise Unavailable("control lacks exact current job/source applicability and evidence")
        if record["status"] in ("accepted", "resolved_findings", "awaiting_alex", "unavailable"):
            raise Unavailable("late control cannot reopen a resolved, expired or escalated job")
        if questioning(record):
            raise Unavailable("probing questions and repeated unsatisfying answers must precede advisor control")
        result = dict(recommendation, id=digest([consultation, recommendation]), consultation=consultation,
                      at=time.time(), observations={}, observe_only=record["mode"] != "enforce")
        if not any(c["id"] == result["id"] for c in record["controls"]):
            record["controls"].append(result)
            save(record, "control-requested")
        return result


def complete(identity, row):
    """The ordinary independent reviewer must establish every matrix condition
    at the exact recovery handover. Neither an advisor nor a later PASS alone
    can close the job.
    """
    with locked(identity):
        record = read(identity)
        plan = record.get("plan")
        if not plan or not plan.get("accepted") or record["status"] in ("awaiting_alex", "unavailable"):
            return False
        attempt = next((a for a in record["attempts"] if a.get("tip") == row.get("tip")
                        and a["plan"] == plan["id"]), None)
        coverage = row.get("recovery_coverage") or {}
        effective, _ = F.review_of(rows(F.review_ledger_default()), row.get("tip"), record["repo"])
        source_bound = attempt or row.get("tip") in record.get("handovers", {}).values()
        if (not source_bound or row.get("recovery_plan") != plan["id"]
                or not effective or effective.get("verdict") != "passed"
                or set(coverage) != {c["condition"] for c in plan["matrix"]}
                or not all(isinstance(v, str) and v.strip() for v in coverage.values())):
            return False
        if record["mode"] == "enforce":
            for control in record["controls"]:
                if control["action"] == "continue":
                    continue
                for worker in control["workers"]:
                    observed = control.get("observations", {}).get(worker) or {}
                    if (not observed.get("quiescent") or observed.get("cancellation_status") != "acknowledged"):
                        return False
        record.update(status="accepted", accepted_review=row["id"], accepted_at=time.time())
        save(record, "recovery-accepted")
        return True


def mode(entity):
    """An absent declaration leaves installation inert. Observation has no
    control effects. questions-only also delivers advisor questions to the
    owner, with no control effects and no prior qualification. Enforcement
    requires a named qualification report.
    """
    path = Path(entity or ".") / ".claude" / "recovery.json"
    try:
        if path.stat().st_size > 65536:
            raise Unavailable("recovery declaration exceeds 64 KiB")
        declaration = json.loads(path.read_text())
    except FileNotFoundError:
        return dict(mode="off", repositories=[])
    if (not isinstance(declaration, dict) or declaration.get("schema") != 1
            or declaration.get("mode") not in MODES
            or not isinstance(declaration.get("repositories"), list)
            or not all(isinstance(p, str) and os.path.isabs(p) for p in declaration["repositories"])
            or declaration.get("wes_policy", "carried") not in ("carried", "all")):
        raise Unavailable("invalid recovery declaration %s" % path)
    if declaration["mode"] == "enforce":
        reference = declaration.get("qualification")
        if not reference or not os.path.isabs(reference):
            raise Unavailable("enforcement requires an absolute qualification record")
        qualification = json.loads(Path(reference).read_text())
        if (qualification.get("schema") != 1 or qualification.get("status") != "qualified"
                or not qualification.get("evidence")
                or not set(declaration["repositories"]).issubset(set(qualification.get("repositories", [])))):
            raise Unavailable("enforcement scope has not been qualified")
        required = {"historical_replay", "axel_semantics", "wes_routing", "host_races", "native_capabilities", "observation_pilot", "independent_review"}
        results = qualification.get("results") or {}
        if set(results)!=required or any(v.get("status")!="passed" for v in results.values() if isinstance(v, dict)) or any(
                not isinstance(v, dict) or not v.get("evidence") for v in results.values()):
            raise Unavailable("required rollout evidence is incomplete")
        pins = qualification.get("source_pins") or {}
        engine = Path(__file__).resolve().parents[2]
        required_pins = {"recovery-profiles/axel.md", "recovery-profiles/wes.md", "scripts/lib/recovery.py", "scripts/lib/recovery_runner.py",
                         "scripts/lib/recovery_controls.py", "scripts/lib/pierce.py", "mega-lander/workspaces.py", "scripts/app-engine-hook.py"}
        if not required_pins.issubset(pins) or any(F.file_digest(str(engine/path))!=value for path,value in pins.items()):
            raise Unavailable("qualified runtime/profile source identity changed")
    return declaration


def source_findings(source, repo, work, tip, ledger=None):
    """Resolve carried findings within one attested repository/job/source."""
    aliases = {work}
    record = read(key(repo, work), ledger)
    if record:
        aliases.update(record.get("aliases", []))
    scoped = [r for r in source if r.get("work") in aliases and F.review_row_belongs(r, F.review_repo_identity(repo))]
    if not any(r.get("tip")==tip for r in scoped):
        return {}
    # Include predecessors so the shared reader can reconcile still-open,
    # withdrawn and resolved dispositions. Never pull another job's findings.
    end = max(i for i,r in enumerate(scoped) if r.get("tip")==tip)
    return F.open_review_findings(scoped[:end+1])


def route_request(repo, work, tip, findings, origin, ledger=None):
    """A routing request cannot resolve a finding or authorize implementation."""
    request = dict(repo_id=F.review_repo_identity(repo), repo=os.path.realpath(repo), work=work,
        tip=tip, findings=sorted(set(findings)), origin=origin, at=time.time())
    request["id"] = digest({k:v for k,v in request.items() if k!="at"})
    path = root(ledger)/"routing-requests.jsonl"
    path.parent.mkdir(parents=True, exist_ok=True)
    with locked(key(repo, work), ledger):
        if not any(r.get("id")==request["id"] for r in rows(path)):
            F.append_jsonl(str(path), request)
    return request


def pending_routes(repo, aliases):
    ident = F.review_repo_identity(repo)
    return [r for r in rows(root()/"routing-requests.jsonl") if r.get("repo_id")==ident and r.get("work") in aliases]


def owner_delivered(reference, delivery_key, owner):
    path = Path(reference)
    ledger = path.parent.parent/"reviews.jsonl"
    identity = path.stem
    with locked(identity, ledger):
        record = read(identity, ledger)
        if not record or record["owner"]!=owner:
            return
        record.setdefault("deliveries", {}).setdefault(delivery_key, dict(at=time.time(), owner=owner))
        if record.get("repeat_signal") and delivery_key in {
                repeat_question_key(record, role) for role in ("axel", "wes") if not repeat_pending(record, role)}:
            settle_questions(record)
            # Existing follow-up scheduling sees delivery, without a new scheduler.
            record["update_needed"] = delivery_key
        if record["status"]=="awaiting_alex" and record.get("escalation"):
            record["escalation"].update(delivered=True, delivered_to_owner=owner)
        save(record, "recovery-owner-delivery-accepted", ledger)
