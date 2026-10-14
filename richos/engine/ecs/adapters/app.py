# SPDX-License-Identifier: AGPL-3.0-only
"""Explicit app scope over the vendor-neutral ECS store. No cwd inference."""
from dataclasses import asdict
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sqlite3

from ecs_core import (CEO_SEAT_PREFIX, EventStore, PERSON_ID, ScopeError, ValidationError,
                      ECSError, canonical_json, ceo_seat, is_ceo_row)
from ecs_checkpoint import checkpoint, checkpoint_receipt
from ecs_inspect import inspect_records

PROTOCOL_VERSION = 1
BINDING_FIELDS = ("entity_id", "thread_id", "session_id", "turn_id", "audience", "revision")
# THE WORK SEAT. ecs_active_context is a CURSOR keyed by person_id, not a store: it
# answers "which entity, thread, session and turn is this principal in right now",
# while the records themselves are keyed by entity and thread. The conversation
# rewrites that one row at the start of every turn, so a background assignment
# holding a binding frozen minutes ago is stale the moment the CEO speaks. It gets
# its own row instead -- one per assignment, never one per lease, because the table
# is person_id PRIMARY KEY and a second assignment on one seat upserts the first
# one's cursor out from under it. The seat is explicit or absent; absent is the
# CEO's own cursor and every call shape that predates seats is untouched.
#
# HIS OWN SEATS, ONE PER CONVERSATION THREAD, for the same reason and in the same
# table: he can run several conversation threads at once, each holding its own
# front desk, and N front desks on the one ceo-default row is the collision above
# with the CEO on both sides of it -- thread B's bind upserts thread A's cursor,
# and A's next checkpoint raises "stale app binding", which
# with_fresh_active_fence never retries (ecs_core.py:171-220: a ScopeError "is not
# a race"), so the turn dead-letters. A thread's seat is DERIVED from the thread
# (ceo_seat -> "ceo-thread:<thread_id>"), so the two halves below never ask
# whether a person equals the PERSON_ID literal: they ask is_ceo_row, which
# re-derives the seat from the row's own thread and requires the ceo audience. A
# work seat named after an assignment answers no, whatever audience it asks for.
CONVERSATION_ONLY = ("checkpoint", "receipt", "brief")
# Enumerating and releasing seats is the HOST's reconciliation, never a background
# lease's: a lease that could release seats could release another lease's.
HOST_ONLY = ("seats", "release-seat")


# THE OPERATOR'S COMPLETION (richos-hq operator back-end spec r1 (c), r3 (c)). His own
# team, run behind the app, reports through `richos_operator.report`; the host then
# closes the assignment's obligation through `operator-complete`, and nothing else
# can. The evidence is re-verified HERE, never taken from the report:
#   git:<absolute repo>:<branch>:<full sha>   the commit is in refs/heads/<branch>
#   answer:<sha256>                           the digest of `answer_text`, sent with it
# The status is `completed`, or the store's withdrawn status for a failed assignment,
# which only an answer can close: a land never closes a failure.
OPERATOR_ACTOR = "richos-operator-v1"
OPERATOR_WITHDRAWN = "cancelled"  # dialect-exempt: the store's own terminal status value (ecs_core.py)
OPERATOR_OPEN = ("candidate", "accepted", "active", "blocked", "pending")
_OPERATOR_GIT = re.compile(r"^git:(/[^:\n]+):([A-Za-z0-9._/][A-Za-z0-9._/-]{0,199}):([0-9a-f]{40}|[0-9a-f]{64})$")
_OPERATOR_ANSWER = re.compile(r"^answer:([0-9a-f]{64})$")
_OPERATOR_ANSWER_LIMIT = 262144


def _operator_git_verified(repo, branch, commit):
    if ".." in branch.split("/") or branch.endswith((".lock", "/")) or "//" in branch:
        raise ValidationError(f"the land in {repo} could not be confirmed: {branch!r} is not a branch name")
    env = {**os.environ, "GIT_OPTIONAL_LOCKS": "0", "LC_ALL": "C"}
    for name in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR"):
        env.pop(name, None)
    try:
        result = subprocess.run(["git", "-C", repo, "-c", "core.hooksPath=/dev/null", "merge-base",
                                 "--is-ancestor", commit, "refs/heads/" + branch],
                                capture_output=True, text=True, timeout=30, env=env)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ValidationError(f"the land in {repo} could not be confirmed: git could not run ({error})")
    if result.returncode != 0:
        raise ValidationError(f"the land in {repo} could not be confirmed: {commit[:12]} is not in {branch}")


def operator_evidence(request):
    """The sorted evidence list, every item verified, or a ValidationError."""
    items = request.get("evidence")
    if (not isinstance(items, list) or not 1 <= len(items) <= 20
            or any(not isinstance(e, str) or len(e) > 1024 for e in items) or len(set(items)) != len(items)):
        raise ValidationError("operator completion needs one to twenty distinct evidence strings")
    answers = 0
    for item in items:
        land = _OPERATOR_GIT.match(item)
        if land:
            repo, branch, commit = land.groups()
            if not Path(repo).is_dir():
                raise ValidationError(f"the land in {repo} could not be confirmed: no such repository")
            _operator_git_verified(repo, branch, commit)
            continue
        answer = _OPERATOR_ANSWER.match(item)
        if answer:
            answers += 1
            text = request.get("answer_text")
            if (not isinstance(text, str) or len(text.encode("utf-8")) > _OPERATOR_ANSWER_LIMIT
                    or hashlib.sha256(text.encode("utf-8")).hexdigest() != answer.group(1)):
                raise ValidationError("answer evidence must be the SHA-256 of the answer_text sent with it")
            continue
        raise ValidationError("operator evidence is git:<absolute repo>:<branch>:<full sha> or "
                              "answer:<sha256>, and nothing else")
    if answers > 1:
        raise ValidationError("operator completion carries at most one answer")
    return sorted(items)


def operator_complete(store, request, context):
    if not is_ceo_row(context):
        raise ScopeError("operator completion belongs to the conversation's own seat")
    obligation = required(request, "obligation_id")
    status = request.get("status", "completed")
    if status not in ("completed", OPERATOR_WITHDRAWN):
        raise ValidationError("operator completion closes an obligation as completed or withdrawn, nothing else")
    evidence = operator_evidence(request)
    if status == OPERATOR_WITHDRAWN and any(item.startswith("git:") for item in evidence):
        raise ValidationError("a land cannot close a failed assignment; send the failure's answer instead")
    identity = hashlib.sha256(canonical_json([obligation, status, evidence]).encode("utf-8")).hexdigest()
    payload = {"item_id": obligation, "status": status, "evidence_ref": "operator:" + ";".join(evidence)}
    key = "app-operator-complete:" + identity
    existing = store.existing_event(key)
    if existing:
        if (existing["entity_id"] != context["entity_id"] or existing["thread_id"] != context["thread_id"]
                or json.loads(existing["payload_json"]) != payload):
            raise ScopeError("operator completion receipt does not match its original scope or evidence")
        duplicate = True
    else:
        item = inspect_records(store, person_id=context["person_id"], item_id=obligation)["item"]
        if item["status"] not in OPERATOR_OPEN:
            raise ValidationError(f"only an open assignment can be closed; {obligation} is {item['status']}")
        store.append("continuity.item_closed", entity_id=context["entity_id"], thread_id=context["thread_id"],
                     session_id=context["session_id"], active_context_revision=context["revision"],
                     person_id=context["person_id"], actor_kind="authority_adapter", actor_id=OPERATOR_ACTOR,
                     source_ref=required(request, "source_ref"), idempotency_key=key,
                     expected_revision=item["revision"], payload=payload)
        duplicate = False
    return {"obligation_closed": True, "obligation_id": obligation, "status": status,
            "evidence_ref": payload["evidence_ref"], "duplicate": duplicate}


# THE APP'S OWN ANSWER CLOSE (richos-hq docs/verification/2026-09-27-background-command-
# finish, "Not done"). An assignment the back end handled ITSELF -- a command he asked
# for, or the reason it could not begin -- has no worker, so `complete` (which needs
# one, mega-lander/app.py) can never close it, and the back end used to tell him so.
# The host closes it here, on the digest of the answer he was given, from the
# conversation's own seat. Two refusals keep a code change on `complete`: the host
# calls this only when the assignment's receipts hold no helper, and this verb
# refuses any assignment whose work seat has a work unit recorded on it.
APP_ANSWER_ACTOR = "richos-app-v1"
WORK_SEAT_PREFIX = "work-seat:"  # the app's contract spelling (richos-core assignment.rs)


def answer_complete(store, request, context):
    if not is_ceo_row(context):
        raise ScopeError("closing an assignment on its answer belongs to the conversation's own seat")
    obligation = required(request, "obligation_id")
    text = request.get("answer_text")
    if (not isinstance(text, str) or not text.strip()
            or len(text.encode("utf-8")) > _OPERATOR_ANSWER_LIMIT):
        raise ValidationError("an answer close needs the nonempty, bounded answer it closes on")
    conn = store.connect()
    try:
        helpers = conn.execute("SELECT COUNT(*) FROM ecs_work_units WHERE entity_id=? AND person_id=?",
                               (context["entity_id"], WORK_SEAT_PREFIX + obligation)).fetchone()[0]
    finally:
        conn.close()
    if helpers:
        raise ValidationError("a helper worked on this assignment, so it closes through complete "
                              "with its workers, never on an answer")
    digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
    payload = {"item_id": obligation, "status": "completed", "evidence_ref": "answer:" + digest}
    key = "app-answer-complete:" + hashlib.sha256(canonical_json([obligation, digest]).encode("utf-8")).hexdigest()
    existing = store.existing_event(key)
    if existing:
        if (existing["entity_id"] != context["entity_id"] or existing["thread_id"] != context["thread_id"]
                or json.loads(existing["payload_json"]) != payload):
            raise ScopeError("answer completion receipt does not match its original scope or answer")
        duplicate = True
    else:
        item = inspect_records(store, person_id=context["person_id"], item_id=obligation)["item"]
        if item["status"] not in OPERATOR_OPEN:
            raise ValidationError(f"only an open assignment can be closed; {obligation} is {item['status']}")
        store.append("continuity.item_closed", entity_id=context["entity_id"], thread_id=context["thread_id"],
                     session_id=context["session_id"], active_context_revision=context["revision"],
                     person_id=context["person_id"], actor_kind="authority_adapter", actor_id=APP_ANSWER_ACTOR,
                     source_ref=required(request, "source_ref"), idempotency_key=key,
                     expected_revision=item["revision"], payload=payload)
        duplicate = False
    return {"obligation_closed": True, "obligation_id": obligation, "status": "completed",
            "evidence_ref": payload["evidence_ref"], "duplicate": duplicate}


def required(document, key):
    value = document.get(key)
    if not isinstance(value, str) or not value.strip() or len(value) > 1024:
        raise ValidationError(f"{key} must be a nonempty bounded string")
    return value


def binding_of(row):
    return {key: row[key] for key in BINDING_FIELDS}


def seat_of(request):
    """The caller's explicit seat, or None for the CEO's own cursor."""
    return None if request.get("seat") is None else required(request, "seat")


def fence(store, binding, seat=PERSON_ID):
    if not isinstance(binding, dict) or set(binding) != set(BINDING_FIELDS):
        raise ScopeError("an explicit app entity/thread/session/turn/audience/revision binding is required")
    current = store.current_context(seat)
    if current is None or binding_of(current) != binding:
        raise ScopeError("stale app binding; reconcile in its original scope before retrying")
    return current


def fenced(store, binding, seat):
    # An absent seat calls fence with the arguments it has always been called with,
    # so the conversation's path through this adapter is byte-identical to the one
    # that shipped before seats existed.
    return fence(store, binding) if seat is None else fence(store, binding, seat)


def bind(store, request, seat=None):
    scope = request.get("scope", {})
    if set(scope) != set(BINDING_FIELDS) - {"revision"}:
        raise ScopeError("bind requires explicit entity, thread, session, turn and audience")
    for key in scope:
        required(scope, key)
    source = required(request, "source_ref")
    request_id = required(request, "request_id")
    expected = request.get("expected_revision")
    if expected is not None and (type(expected) is not int or expected < 1):
        raise ValidationError("expected_revision must be a positive integer or null for first use")
    # Refused HERE as well as in the reducer, and before a single event is
    # written: the bind below is split into four appends, and a seat rejected on
    # the fourth would leave the first three behind for no reason. The reducer
    # keeps the same rule for every other writer (ecs_core.py _on_thread_activated).
    if seat is not None and seat.startswith(CEO_SEAT_PREFIX) and (
            seat != ceo_seat(scope["thread_id"]) or scope["audience"] != "ceo"):
        raise ScopeError(
            "a CEO thread seat must be derived from the thread it binds and carry the ceo audience")
    entity, thread, session = (scope[k] for k in ("entity_id", "thread_id", "session_id"))
    # Stable app identities are metadata. They are not claims about repository
    # ownership. The app repository registry owns those mappings separately.
    owner = f"app-entity:{entity}"
    common = dict(entity_id=entity, thread_id=thread, session_id=session,
                  actor_kind="app", actor_id="richos-app-v1", source_ref=source)
    # THE BIND IS SPLIT, and it is a decision rather than a typo. entity.registered,
    # thread.created and session.observed are statements about things that EXIST --
    # this entity, this thread, this session -- and they are the same facts whoever
    # is looking, so they stay on the registry person. Only thread.activated says who
    # is where right now, which is what a seat is, so only it carries the seat.
    # Carrying the seat on all four raises "entity ... is already registered
    # differently" (ecs_core.py:726-737) before the second seat exists at all.
    for suffix, event, payload in (
        ("entity", "entity.registered", {"display_name": entity, "canonical_root": owner,
                                         "git_common_dir": owner, "status": "active"}),
        ("thread", "thread.created", {"title": thread}),
        ("session", "session.observed", {"vendor": "richos-app", "status": "active"}),
        ("active", "thread.activated", {"audience": scope["audience"], "turn_id": scope["turn_id"]}),
    ):
        store.append(event, **common, payload=payload,
                     person_id=PERSON_ID if suffix != "active" or seat is None else seat,
                     expected_revision=expected if suffix == "active" else None,
                     idempotency_key=f"app-bind:{request_id}:{suffix}")
    current = store.current_context() if seat is None else store.current_context(seat)
    if any(current[k] != value for k, value in scope.items()):
        raise ScopeError("binding was superseded; an old bind receipt cannot reactivate it")
    return {"binding": binding_of(current)}


def execute(state_root, request):
    if not isinstance(request, dict) or type(request.get("protocol")) is not int or request.get("protocol") != PROTOCOL_VERSION:
        raise ValidationError("unsupported ECS app protocol")
    command = required(request, "command")
    root = Path(state_root)
    if not root.is_absolute():
        raise ValidationError("ECS app state root must be explicit and absolute")
    if command == "hello":
        migrations = Path(__file__).resolve().parents[1] / "migrations"
        identity = hashlib.sha256()
        for file in sorted(migrations.glob("*.sql")):
            identity.update(file.name.encode())
            identity.update(file.read_bytes())
        # A positive capability answer, so the app never has to infer per-thread
        # support from a bind that would succeed on an older engine and then be
        # refused at the first checkpoint ("belong to the conversation's own seat").
        return {"protocol": PROTOCOL_VERSION, "event_schema": 1,
                "migration_digest": identity.hexdigest(), "state_root": str(root),
                "ceo_thread_seats": True, "ceo_seat_prefix": CEO_SEAT_PREFIX,
                "commands": ["current", "bind", "prepare-turn", "checkpoint", "receipt", "brief", "inspect", "observe", "verified-work", "observation-receipt", "import-preview", "import-apply", "sync-loro-receipts", "complete-obligation", "operator-complete", "seats", "release-seat", "answer-complete"]}
    if command == "prepare-turn":
        # The front desk's existing four requests, in one Python invocation. Keep
        # their handlers, revision fences, order and receipt-error behavior.
        scope = request.get("scope")
        if not isinstance(scope, dict) or scope.get("audience") != "ceo":
            raise ScopeError("turn preparation belongs to the conversation's own seat")
        budget = request.get("budget_chars", 6000)
        if type(budget) is not int or not 900 <= budget <= 24000:
            raise ValidationError("brief budget must be between 900 and 24000 characters")
        current = execute(root, {**request, "command": "current"})["binding"]
        if current is not None and all(current.get(k) == v for k, v in scope.items()) and set(scope) == set(BINDING_FIELDS) - {"revision"}:
            binding = current
        else:
            binding = execute(root, {**request, "command": "bind",
                "expected_revision": current["revision"] if current else None})["binding"]
        fields = {**request, "binding": binding}
        receipts, receipt_error = None, None
        try:
            receipts = execute(root, {**fields, "command": "sync-loro-receipts"})
        except (ECSError, ValueError, TypeError, OSError, sqlite3.Error) as error:
            # As on the old app path, failed receipt synchronization is disclosed
            # in the brief; a bind or brief failure still refuses the turn.
            receipt_error = str(error)
        brief = execute(root, {**fields, "command": "brief"})["text"]
        return {"binding": binding, "text": brief, "receipts": receipts, "receipt_error": receipt_error}
    if command == "import-preview":
        from import_records import preview
        return preview(root, request.get("envelope"), request.get("target"))
    store = EventStore(root)
    seat = seat_of(request)
    if command == "current":
        current = store.current_context() if seat is None else store.current_context(seat)
        return {"binding": binding_of(current) if current is not None else None}
    if command == "bind":
        return bind(store, request, seat)
    binding = request.get("binding")
    context = fenced(store, binding, seat)
    # Checkpoints, their receipts and the executive brief are the CONVERSATION'S, and
    # they read or write the CEO's cursor by construction (ecs_checkpoint.py:15, 79,
    # 152 and the TurnScope person at :119-120; compile_checkpoint's caller below
    # passes no person). A background lease has no business writing his continuity,
    # and on a work seat these raise a revision conflict rather than refusing
    # cleanly. The refusal is here, in the engine, as well as in the app's per-lease
    # MCP config: an allow-list on tool NAMES cannot see whose seat is calling.
    if command in CONVERSATION_ONLY and not is_ceo_row(context):
        raise ScopeError("checkpoint, receipt and brief belong to the conversation's own seat")
    if command in HOST_ONLY and not is_ceo_row(context):
        raise ScopeError("seat reconciliation belongs to the conversation's own seat")
    if command == "sync-loro-receipts":
        from loro_receipts import synchronize
        result = synchronize(store, binding, context["person_id"])
    elif command == "import-apply":
        from import_records import apply
        # The ENVELOPE's target person stays ceo-default: it is part of the receipt
        # digest, so an import stays idempotent no matter which of his threads
        # applied it. The fence and the append follow the calling seat, because
        # continuity.item_opened is conversational and is fenced against the row
        # belonging to the event's own person.
        return apply(root, request.get("envelope"), binding, seat=seat,
                     person_id=context["person_id"])
    elif command == "checkpoint":
        document = request.get("checkpoint")
        if not isinstance(document, dict):
            raise ValidationError("checkpoint must be an object")
        # A model-authored checkpoint cannot certify external work. Cancellation
        # or rejection is still an explicit operational transition. Verified
        # completion must arrive through an app authority adapter.
        for statement in document.get("statements", []):
            if isinstance(statement, dict):
                fields = statement.get("fields", {})
                if isinstance(fields,dict) and fields.get("class") == "wiki_loro":
                    raise ValidationError("durable knowledge corrections must use the app's Loro proposal/confirmation desk; ECS consumes its writer receipt")
                if isinstance(fields, dict) and (fields.get("status") == "completed" or
                        (statement.get("verb") == "close" and "status" not in fields)):
                    raise ValidationError("completion needs an app verification receipt, not a checkpoint claim")
        result = checkpoint(store, document, session=context["session_id"], turn=context["turn_id"],
                            revision=context["revision"], request_id=required(request, "request_id"),
                            person_id=context["person_id"])
    elif command == "receipt":
        result = checkpoint_receipt(store, session=required(request, "session_id"),
                                    turn=required(request, "turn_id"), request_id=required(request, "request_id"),
                                    person_id=context["person_id"])
    elif command == "brief":
        budget = request.get("budget_chars", 6000)
        if type(budget) is not int or not 900 <= budget <= 24000:
            raise ValidationError("brief budget must be between 900 and 24000 characters")
        result = {"text": store.compile_checkpoint(context["entity_id"], context["thread_id"],
                   expected_active_revision=context["revision"], session_id=context["session_id"],
                   budget_chars=budget, person_id=context["person_id"]),
                  "inspection": inspect_records(store, person_id=context["person_id"])}
    elif command == "inspect":
        query = request.get("query", {})
        if not isinstance(query, dict) or set(query) - {"section", "offset", "limit", "sequence", "item_id", "query", "include_closed"}:
            raise ValidationError("unsupported inspection fields")
        result = inspect_records(store, person_id=context["person_id"], **query)
    elif command == "seats":
        # Every cursor in the store, so reconciliation can see the seats a crash
        # left behind. health() reports them too, for the same reason: a diagnostic
        # that shows one cursor while two exist lies exactly when it is being used
        # to diagnose a second cursor.
        conn = store.connect()
        try:
            result = {"seats": [dict(row) for row in conn.execute(
                "SELECT person_id, entity_id, thread_id, session_id, turn_id, audience, revision "
                "FROM ecs_active_context ORDER BY person_id").fetchall()]}
        finally:
            conn.close()
    elif command == "release-seat":
        target = required(request, "person_id")
        if target == PERSON_ID:
            raise ScopeError("the conversation's own seat is never reconciled away")
        if target == context["person_id"]:
            raise ScopeError("a seat cannot reconcile itself away while it is the one calling")
        row = store.current_context(target)
        if row is None:
            result = {"released": False, "seat": target, "reason": "no such seat"}
        else:
            # A work seat is bound inside the conversation's own thread, so its
            # release stays inside that partition. One of HIS thread seats is in
            # another thread by construction -- that is what makes it reconcilable
            # from here at all -- so the boundary it keeps is the company's.
            his = is_ceo_row(row)
            if row["entity_id"] != context["entity_id"] or (
                    not his and row["thread_id"] != context["thread_id"]):
                raise ScopeError("a seat cannot be released from another company or thread")
            revision = int(row["revision"])
            if his:
                # THE LIVENESS PROOF, and it is the engine's half of "a live
                # thread's seat is never released". Whether a conversation thread
                # still exists is the app's knowledge, not the store's; what the
                # store can prove is MOVEMENT. The caller names the revision it saw
                # when it enumerated seats, and a seat that has bound a turn since
                # then belongs to a thread that is demonstrably alive, so the
                # release is refused rather than racing the front desk using it. A
                # work seat's release is unchanged and needs no proof.
                observed = request.get("expected_revision")
                if type(observed) is not int or observed < 1:
                    raise ValidationError(
                        "releasing one of his thread seats requires the revision it was enumerated at")
                if observed != revision:
                    raise ScopeError(
                        "that thread seat has moved since it was enumerated; a live thread's seat is never released")
            store.append("thread.deactivated", entity_id=row["entity_id"], thread_id=row["thread_id"],
                session_id=row["session_id"], person_id=target, expected_revision=revision,
                actor_kind="app", actor_id="richos-app-v1", source_ref=required(request, "source_ref"),
                idempotency_key=f"app-release-seat:{required(request, 'request_id')}",
                payload={"reason": request.get("reason") or "assignment settled"})
            result = {"released": True, "seat": target, "turn_id": row["turn_id"]}
    elif command == "observation-receipt":
        event = store.existing_event(f"app-observe:{required(request, 'request_id')}")
        if event is None:
            result = {"observed": False}
        else:
            if event["entity_id"] != context["entity_id"] or event["thread_id"] != context["thread_id"]:
                raise ScopeError("observation receipt belongs to another scope")
            result = {"observed": True, "work": json.loads(event["payload_json"]),
                      "source_ref": event["source_ref"], "expected_revision": event["expected_revision"]}
    elif command == "complete-obligation":
        # Host-only: every final implementation must have actual review, Git and
        # cleanup evidence. A model checkpoint cannot certify this transition.
        import importlib.util
        spec=importlib.util.spec_from_file_location("richos_completed_work",Path(__file__).resolve().parents[2]/"mega-lander/app.py")
        work=importlib.util.module_from_spec(spec);spec.loader.exec_module(work)
        identity=required(request,"completion_id")
        receipt=work.verify_completion({"binding":binding},identity)
        payload={"item_id":receipt["obligation_id"],"status":"completed","evidence_ref":receipt["evidence_ref"]}
        key="app-complete:"+identity
        existing=store.existing_event(key)
        if existing:
            if (existing["entity_id"] != context["entity_id"] or existing["thread_id"] != context["thread_id"]
                    or json.loads(existing["payload_json"]) != payload or existing["source_ref"] != receipt["source_ref"]
                    or existing["expected_revision"] != receipt["expected_revision"]):
                raise ScopeError("completion receipt does not match its original scope or evidence")
        else:
            # continuity.item_closed is a CONVERSATIONAL_EVENT (ecs_core.py:38-43), so
            # _reduce fences it through _fence_active (:607-608), which selects the row
            # belonging to the EVENT'S OWN person (:562) and compares that row's revision
            # against active_context_revision. EventStore.append defaults person_id to
            # PERSON_ID (:391), so this append without a seat compared the WORK row's
            # revision against the CEO's row and failed the moment he spoke. The fenced
            # row's own person is the seat, so it can never drift from what was fenced.
            store.append("continuity.item_closed",entity_id=context["entity_id"],thread_id=context["thread_id"],
                session_id=context["session_id"],active_context_revision=context["revision"],
                person_id=context["person_id"],
                actor_kind="authority_adapter",actor_id="richos-provider-v1",source_ref=receipt["source_ref"],
                idempotency_key=key,expected_revision=receipt["expected_revision"],payload=payload)
        result={"obligation_closed":True,"obligation_id":receipt["obligation_id"],"evidence_ref":receipt["evidence_ref"]}
    elif command == "operator-complete":
        # Host-only, like complete-obligation: his team's report never closes an
        # obligation by itself. The MCP server does not expose it (mcp.py TOOLS).
        result = operator_complete(store, request, context)
    elif command == "answer-complete":
        # Host-only, like operator-complete: the MCP server does not expose it.
        result = answer_complete(store, request, context)
    elif command in ("observe", "verified-work"):
        # Host-issued observations only. The app translates provider facts;
        # generic checkpoints cannot impersonate a task authority.
        payload = request.get("work")
        if not isinstance(payload, dict) or payload.get("authority") != "richos-provider-v1":
            raise ValidationError("unknown app task authority")
        if command == "verified-work":
            # This command is not exposed by the continuity MCP server. The app
            # work adapter must have a durable review/integration receipt and
            # Git must still prove the commit is in the recorded target branch.
            import importlib.util
            spec=importlib.util.spec_from_file_location("richos_verified_work",Path(__file__).resolve().parents[2]/"mega-lander/app.py")
            work=importlib.util.module_from_spec(spec);spec.loader.exec_module(work)
            evidence=work.verification_evidence({"binding":binding},payload.get("external_id"))
            if payload.get("status")!="completed" or payload.get("evidence_ref")!=evidence:
                raise ValidationError("completion does not match the verified Git receipt")
        elif payload.get("status") == "completed":
            raise ValidationError("provider completion is not verified assignment completion")
        # observe appends work_unit.upserted, which IS conversational and fails exactly
        # as complete-obligation does. verified-work appends
        # work_unit.authority_completed, which is NOT (ecs_core.py:609-610) and is
        # fenced on the session instead -- it carries the seat anyway, because the row
        # it writes carries person_id (:1053) and background records are the work
        # seat's. Between them these two are how a background assignment says anything
        # at all.
        result = asdict(store.append("work_unit.authority_completed" if command=="verified-work" else "work_unit.upserted", entity_id=context["entity_id"],
            thread_id=context["thread_id"], session_id=context["session_id"], person_id=context["person_id"],
            active_context_revision=context["revision"], source_ref=required(request, "source_ref"),
            idempotency_key=f"app-observe:{required(request, 'request_id')}",
            expected_revision=request.get("expected_revision"),
            actor_kind="authority_adapter", actor_id="richos-provider-v1", payload=payload))
    else:
        raise ValidationError(f"unsupported ECS command: {command}")
    # A read racing a scope switch must not return the new scope's records to
    # the old caller. Mutations also carry the core's transactional fence.
    fenced(store, binding, seat)
    return result
