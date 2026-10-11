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
import time
from pathlib import Path

import operator_fences as F

THRESHOLD = 3
MAX_RECOVERIES = 3
EXECUTION_SECONDS = 3 * 3600
LOCK_SECONDS = 4
MAX_LEDGER_BYTES = 16 * 1024 * 1024


class Unavailable(ValueError):
    pass


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def root():
    return Path(F.review_ledger_default()).parent / "recovery"


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
def locked(identity):
    root().mkdir(parents=True, exist_ok=True)
    fd = os.open(root() / (identity + ".lock"), os.O_CREAT | os.O_RDWR, 0o600)
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


def read(identity):
    path = root() / (identity + ".json")
    try:
        value = json.loads(path.read_text())
    except FileNotFoundError:
        return None
    if not isinstance(value, dict) or value.get("schema") != 1 or value.get("id") != identity:
        raise Unavailable("invalid recovery record %s" % path)
    return value


def save(record, event):
    F.write_json_atomic(str(root() / (record["id"] + ".json")), record)
    F.append_jsonl(str(root() / "events.jsonl"), dict(at=time.time(), job=record["id"],
                   event=event, revision=digest(record), status=record["status"]))


def failed_attempts(ledger, repo, aliases):
    ident = F.review_repo_identity(repo)
    handovers = [r for r in ledger if r.get("work") in aliases and r.get("verdict")
                 and r.get("trigger") in F.REVIEW_LAND_KINDS and F.review_row_belongs(r, ident)]
    tips = list(dict.fromkeys(r.get("tip") for r in handovers if r.get("tip")))
    failed = []
    for tip in tips:
        effective, _ = F.review_of(handovers, tip, repo)
        if effective and effective.get("verdict") == "changes-requested":
            failed.append(dict(tip=tip, reviews=[r["id"] for r in handovers if r.get("tip") == tip]))
    return failed


def reconcile(repo, work, aliases, owner, acceptance, now=None, mode="observe", dry=False):
    """Level-triggered, including when no watcher has polled. The original
    deadline survives source movement and worker replacement.
    """
    now = time.time() if now is None else now
    failed = failed_attempts(rows(F.review_ledger_default()), repo, aliases)
    identity = key(repo, work)
    with (contextlib.nullcontext() if dry else locked(identity)):
        record = read(identity)
        if record is None and len(failed) < THRESHOLD:
            return None
        if record is None:
            record = dict(schema=1, id=identity, repo=os.path.realpath(repo),
                repo_id=F.review_repo_identity(repo), work=work, aliases=sorted(aliases),
                acceptance=acceptance, owner=owner, created_at=now, deadline=now + EXECUTION_SECONDS,
                status="investigating", mode=mode, failed=failed, plan=None, attempts=[], controls=[],
                consults=[], evidence_cursor=0, cancellation=[], escalation=None, plans=[],
                budgets=[dict(start=now, deadline=now + EXECUTION_SECONDS, reason="initial recovery")])
        if record["acceptance"] != acceptance:
            raise Unavailable("acceptance changed without an explicit recovery reconciliation")
        before = digest(record)
        previous_count = record.get("recovery_failures", 0)
        record.update(failed=failed, aliases=sorted(aliases), owner=owner, mode=mode)
        record["evidence_cursor"] = digest(failed)
        failures = {a["tip"] for a in failed}
        for attempt in record["attempts"]:
            attempt["failed"] = bool(attempt.get("tip") in failures)
        count = len({a["tip"] for a in record["attempts"] if a.get("failed")})
        record["recovery_failures"] = count
        if record["status"] == "awaiting_alex" or count >= MAX_RECOVERIES:
            record["status"] = "awaiting_alex"
            record["escalation"] = record["escalation"] or dict(at=now,
                reason="Three recovery attempts failed. Alex requires escalation before any further correction.",
                failures=record["attempts"], delivered=False)
        elif len(failed) < THRESHOLD and not record["attempts"]:
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
        if record["status"] in ("awaiting_alex", "unavailable"):
            raise Unavailable("job %s is %s" % (record["id"], record["status"]))
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
        for attempt in record["attempts"]:
            if attempt["worker"] == worker:
                if attempt.get("tip") and attempt["tip"] != tip:
                    raise Unavailable("one recovery handover cannot change source identity")
                attempt["tip"] = tip
                save(record, "recovery-handover")
                return


def plan_problems(value, record):
    if not isinstance(value, dict):
        return ["proposal must be an object"]
    required = {"job", "acceptance", "frontier", "diagnosis", "matrix", "proof", "approach", "brief", "control"}
    if set(value) != required:
        return ["proposal fields differ from the host schema"]
    if (value["job"], value["acceptance"], value["frontier"]) != (
            record["id"], record["acceptance"], record["evidence_cursor"]):
        return ["proposal does not match the current job/evidence"]
    if not all(isinstance(value[k], str) and value[k].strip() for k in ("diagnosis", "approach", "brief")):
        return ["diagnosis, approach and replacement brief must be substantive"]
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
        if (not isinstance(assessment, dict) or assessment.get("plan") != plan_id
                or assessment.get("accepted") is not True or not assessment.get("evidence")
                or assessment.get("reviewer") == plan["consultation"]):
            raise Unavailable("matching independent assessment with evidence required")
        plan.update(accepted=True, assessment=assessment)
        record["status"] = "recovering"
        save(record, "recovery-plan-accepted")


def landing_gap(repo, work):
    with locked(key(repo, work)):
        record = read(key(repo, work))
    if record and record["mode"] == "enforce" and record["status"] not in ("resolved_findings", "accepted"):
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
                or not all(isinstance(v, str) and v for v in recommendation["workers"].values())
                or not all(isinstance(recommendation[k], str) and recommendation[k].strip()
                           for k in ("reason", "evidence"))):
            raise Unavailable("control lacks exact current job/source applicability and evidence")
        if record["status"] in ("accepted", "resolved_findings", "awaiting_alex", "unavailable"):
            raise Unavailable("late control cannot reopen a resolved, expired or escalated job")
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
        if not plan or not plan.get("accepted") or record["status"] == "awaiting_alex":
            return False
        attempt = next((a for a in record["attempts"] if a.get("tip") == row.get("tip")
                        and a["plan"] == plan["id"]), None)
        coverage = row.get("recovery_coverage") or {}
        effective, _ = F.review_of(rows(F.review_ledger_default()), row.get("tip"), record["repo"])
        if (not attempt or row.get("recovery_plan") != plan["id"]
                or not effective or effective.get("verdict") != "passed"
                or set(coverage) != {c["condition"] for c in plan["matrix"]}
                or not all(isinstance(v, str) and v.strip() for v in coverage.values())):
            return False
        record.update(status="accepted", accepted_review=row["id"], accepted_at=time.time())
        save(record, "recovery-accepted")
        return True


def mode(entity):
    """An absent declaration leaves installation inert. Observation has no
    control effects. Enforcement requires a named qualification report.
    """
    path = Path(entity or ".") / ".claude" / "recovery.json"
    try:
        if path.stat().st_size > 65536:
            raise Unavailable("recovery declaration exceeds 64 KiB")
        declaration = json.loads(path.read_text())
    except FileNotFoundError:
        return dict(mode="off", repositories=[])
    if (not isinstance(declaration, dict) or declaration.get("schema") != 1
            or declaration.get("mode") not in ("off", "observe", "enforce")
            or not isinstance(declaration.get("repositories"), list)
            or not all(isinstance(p, str) and os.path.isabs(p) for p in declaration["repositories"])):
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
    return declaration
