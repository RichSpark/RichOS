#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Extraction layer: turn text into typed continuity statements.

Two inputs, two very different trust levels, one deterministic parser:

1. **Rich's reply** carries fenced ``ecs`` blocks. Each line is a typed
   statement (``commitment: title="..." owner=rich due=2026-09-05``). This is
   the checkpoint protocol from architecture section 6.2: Rich states what the
   turn meant in a machine-readable form, and the Stop adapter writes it
   through the same fenced event store as the CLI. It is not a summary being
   reduced into state; every line maps to one typed event or is rejected with
   a reason that is itself recorded.

2. **The CEO's prompt** is scanned for the narrow shapes of an explicit
   correction ("NEVER ...", "don't ... again", "I told you", "from now on").
   A hit becomes an ``observed`` correction: durable evidence with the exact
   sentence, awaiting Rich's classification. Nothing is applied from a regex.

Both parsers are pure functions over text so the core stays vendor-neutral
and the tests use fixed fixtures. ``apply_statements`` performs the writes and
returns per-statement outcomes; it never swallows a fence (scope, revision,
secret) - those become recorded rejections, and the turn's extraction row
counts them.
"""

from __future__ import annotations

import json
import re
import shlex
import sqlite3
from dataclasses import dataclass, field
from typing import Any, Callable, Optional

from ecs_core import (
    CONTINUITY_ITEM_TYPES,
    CORRECTION_ROUTABLE_CLASSES,
    TERMINAL_WORK_STATUSES,
    ECSError,
    EventStore,
    RevisionConflict,
    ScopeError,
    ValidationError,
    stable_id,
    work_unit_identity,
)


ECS_BLOCK = re.compile(r"^```ecs[ \t]*\r?\n(.*?)^```[ \t]*$", re.S | re.M)
FENCE = re.compile(r"^```", re.M)
STATEMENT = re.compile(r"^([a-z_]+)\s*:\s*(.*)$")
MAX_STATEMENTS_PER_TURN = 40
MAX_DETECTIONS_PER_PROMPT = 5
MAX_SENTENCE_CHARS = 400

OPEN_VERBS = CONTINUITY_ITEM_TYPES
ITEM_FIELDS = {
    "id": "item_id", "title": "title", "details": "details", "owner": "owner",
    "beneficiary": "beneficiary", "next": "next_action", "next_action": "next_action",
    "due": "due_at", "due_at": "due_at", "rank": "rank", "status": "status",
    "visibility": "visibility", "evidence": "evidence_ref", "evidence_ref": "evidence_ref",
    "condition_json": "condition_json", "reply_owed": "reply_owed",
}
DELEGATED_FIELDS = {"task", "title", "owner", "authority"}
CORRECTION_FIELDS = {
    "id": "correction_id", "mode": "mode", "class": "classification",
    "classification": "classification", "dest": "destination", "destination": "destination",
    "scope": "entity_scope", "entity_scope": "entity_scope", "temporal": "temporal_scope",
    "temporal_scope": "temporal_scope", "before": "before", "after": "after",
    "content": "content", "reject": "rejection_reason",
}




CORRECTION_TRIGGERS = (
    re.compile(r"\bNEVER\b"),
    re.compile(r"\bALWAYS\b"),
    re.compile(r"\bDON'?T\b[^.!?\n]{0,80}\bAGAIN\b"),
    re.compile(r"(?i)\bnever again\b"),
    re.compile(r"(?i)\bdo(?:n'?t| not)\b[^.!?\n]{0,80}\bagain\b"),
    re.compile(r"(?i)\bi(?:'ve| have)? (?:already )?told you\b"),
    re.compile(r"(?i)\bi already (?:said|told|decided|ruled)\b"),
    re.compile(r"(?i)\bhow many times\b"),
    re.compile(r"(?i)\bfrom now on\b"),
    re.compile(r"(?i)\bstop (?:doing|asking|telling|saying|reporting|calling)\b"),
    re.compile(r"(?i)\bstanding (?:rule|order)\b"),
)
SENTENCE_SPLIT = re.compile(r"(?<=[.!?])\s+|\n+")


@dataclass
class Statement:
    verb: str
    fields: dict[str, str]
    line_no: int
    raw: str


@dataclass
class Outcome:
    statement: Statement
    status: str
    detail: str = ""
    record_id: Optional[str] = None
    routed: Optional[dict[str, Any]] = None


@dataclass
class ExtractionResult:
    block_present: bool
    statements: list[Statement] = field(default_factory=list)
    parse_errors: list[str] = field(default_factory=list)
    outcomes: list[Outcome] = field(default_factory=list)

    @property
    def applied(self) -> int:
        return sum(1 for o in self.outcomes if o.status in ("applied", "duplicate"))

    @property
    def rejected(self) -> int:
        return len(self.parse_errors) + sum(1 for o in self.outcomes if o.status == "rejected")

    @property
    def rejections(self) -> list[str]:
        return list(self.parse_errors) + [
            f"line {o.statement.line_no} {o.statement.verb}: {o.detail}"
            for o in self.outcomes if o.status == "rejected"
        ]




def parse_ecs_blocks(text: str) -> tuple[bool, list[Statement], list[str]]:
    """Return (block_present, statements, parse_errors) for fenced ``ecs`` blocks."""
    if not isinstance(text, str) or not text:
        return False, [], []
    blocks = ECS_BLOCK.findall(text)
    if not blocks:
        return False, [], []
    statements: list[Statement] = []
    errors: list[str] = []
    line_no = 0
    for block in blocks:
        for raw in block.splitlines():
            line_no += 1
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            if len(statements) >= MAX_STATEMENTS_PER_TURN:
                errors.append(f"line {line_no}: statement limit {MAX_STATEMENTS_PER_TURN} exceeded")
                break
            match = STATEMENT.match(line)
            if not match:
                errors.append(f"line {line_no}: not a `verb: key=value` statement")
                continue
            verb, rest = match.group(1), match.group(2)
            try:
                tokens = shlex.split(rest, posix=True)
            except ValueError as exc:
                errors.append(f"line {line_no}: unbalanced quotes ({exc})")
                continue
            fields: dict[str, str] = {}
            bad = False
            for token in tokens:
                if "=" not in token:
                    errors.append(f"line {line_no}: token {token!r} is not key=value")
                    bad = True
                    break
                key, value = token.split("=", 1)
                key = key.strip().lower()
                if not key or key in fields:
                    errors.append(f"line {line_no}: empty or repeated key {key!r}")
                    bad = True
                    break
                fields[key] = value
            if bad:
                continue
            statements.append(Statement(verb=verb, fields=fields, line_no=line_no, raw=line))
    return True, statements, errors




def _strip_fenced(text: str) -> str:
    """Drop fenced code so pasted files cannot masquerade as instructions."""
    out: list[str] = []
    inside = False
    for line in text.splitlines():
        if FENCE.match(line):
            inside = not inside
            continue
        if not inside:
            out.append(line)
    return "\n".join(out)


def detect_ceo_corrections(prompt: str) -> list[str]:
    """Return sentences of ``prompt`` that carry an explicit-correction trigger."""
    if not isinstance(prompt, str) or not prompt.strip():
        return []
    body = _strip_fenced(prompt)
    hits: list[str] = []
    seen: set[str] = set()
    for sentence in SENTENCE_SPLIT.split(body):
        candidate = " ".join(sentence.split())
        if not candidate or candidate.startswith(">"):
            continue
        if not any(pattern.search(candidate) for pattern in CORRECTION_TRIGGERS):
            continue
        clipped = candidate[:MAX_SENTENCE_CHARS]
        key = clipped.lower()
        if key in seen:
            continue
        seen.add(key)
        hits.append(clipped)
        if len(hits) >= MAX_DETECTIONS_PER_PROMPT:
            break
    return hits




@dataclass
class TurnScope:
    entity_id: str
    thread_id: str
    session_id: Optional[str]
    turn_id: str
    source_ref: str
    person_id: str


def _int_or_none(value: Optional[str], name: str) -> Optional[int]:
    if value is None:
        return None
    try:
        return int(value)
    except ValueError as exc:
        raise ValidationError(f"{name} must be an integer") from exc


def _item_payload(fields: dict[str, str], *, opening: bool) -> dict[str, Any]:
    payload: dict[str, Any] = {}
    for key, value in fields.items():
        column = ITEM_FIELDS.get(key)
        if column is None:
            raise ValidationError(f"unknown item field {key!r}")
        payload[column] = value
    if "rank" in payload:
        payload["rank"] = _int_or_none(payload["rank"], "rank")
    if opening:
        payload.setdefault("details", "")
        payload.setdefault("status", "active")
        payload.setdefault("visibility", "worker")
    return payload


def _correction_payload(fields: dict[str, str]) -> dict[str, Any]:
    payload: dict[str, Any] = {}
    for key, value in fields.items():
        column = CORRECTION_FIELDS.get(key)
        if column is None:
            raise ValidationError(f"unknown correction field {key!r}")
        payload[column] = value
    return payload


def _correction_turns(store: EventStore, correction_id: str) -> Optional[set[str]]:
    """The turns a correction was observed in, or None when it does not exist."""
    conn = store.connect()
    try:
        row = conn.execute(
            "SELECT source_turn, last_observed_turn FROM ecs_corrections WHERE correction_id=?",
            (correction_id,),
        ).fetchone()
    finally:
        conn.close()
    if row is None:
        return None
    return {value for value in (row["source_turn"], row["last_observed_turn"]) if value}


def _revision_of(store: EventStore, table: str, key: str, value: str) -> int:
    conn = store.connect()
    try:
        row = conn.execute(f"SELECT revision FROM {table} WHERE {key}=?", (value,)).fetchone()
    finally:
        conn.close()
    if row is None:
        if table == "ecs_continuity_items" and value.startswith("wrk_"):
            raise ValidationError("update/close require continuity item IDs, not wrk_ task IDs; "
                                  "inspect --section work and reconcile task-authority evidence. "
                                  "A new open_loop does not change a delegated task's status.")
        label = {"ecs_continuity_items": "continuity item", "ecs_corrections": "correction"}.get(table, table)
        raise ValidationError(f"unknown {label}: {value}")
    return int(row["revision"])


def apply_statements(
    store: EventStore,
    statements: list[Statement],
    scope: TurnScope,
    *,
    active_revision: int,
    actor: tuple[str, str] = ("rich", "rich"),
) -> list[Outcome]:
    """Write each statement through the fenced store; never raise past a statement."""
    outcomes: list[Outcome] = []
    for statement in statements:
        try:
            outcomes.append(_apply_one(store, statement, scope, active_revision, actor))
        except (ValidationError, ScopeError, RevisionConflict) as exc:
            outcomes.append(Outcome(statement, "rejected", f"{type(exc).__name__}: {exc}"))
        except (ECSError, sqlite3.Error) as exc:




            outcomes.append(Outcome(statement, "rejected", f"{type(exc).__name__}: {exc}"))
    return outcomes


def _apply_one(
    store: EventStore,
    statement: Statement,
    scope: TurnScope,
    active_revision: int,
    actor: tuple[str, str],
) -> Outcome:
    verb, fields = statement.verb, dict(statement.fields)
    line_key = stable_id("stm", scope.turn_id, str(statement.line_no), statement.raw)
    idempotency_key = f"turn:{line_key}"
    prior = store.existing_event(idempotency_key)
    if prior is not None:





        return _prior_outcome(store, statement, scope, active_revision, prior)
    common = dict(
        entity_id=scope.entity_id,
        thread_id=scope.thread_id,
        session_id=scope.session_id,
        person_id=scope.person_id,
        source_ref=scope.source_ref,
        active_context_revision=active_revision,
        actor_kind=actor[0],
        actor_id=actor[1],
    )
    if verb in OPEN_VERBS:
        payload = _item_payload(fields, opening=True)
        payload["item_type"] = verb
        if "title" not in payload:
            raise ValidationError("title is required")
        payload.setdefault("item_id", stable_id("eci", scope.entity_id, verb, payload["title"]))
        result = store.append(
            "continuity.item_opened", idempotency_key=f"turn:{line_key}", payload=payload, **common
        )
        return Outcome(statement, "duplicate" if result.duplicate else "applied", record_id=payload["item_id"])
    if verb == "supersede":
        if set(fields) != {"id", "replacement", "evidence"}:
            raise ValidationError("supersede needs id, replacement and evidence")
        revision = _revision_of(store, "ecs_continuity_items", "item_id", fields["id"])
        result = store.append(
            "continuity.item_superseded", idempotency_key=idempotency_key,
            payload={"item_id": fields["id"], "replacement_id": fields["replacement"],
                     "evidence_ref": fields["evidence"]}, expected_revision=revision, **common,
        )
        return Outcome(statement, "duplicate" if result.duplicate else "applied", record_id=fields["id"])
    if verb in ("update", "close"):
        payload = _item_payload(fields, opening=False)
        item_id = payload.get("item_id")
        if not item_id:
            raise ValidationError("id is required")
        revision = _revision_of(store, "ecs_continuity_items", "item_id", item_id)
        if verb == "close":
            payload.setdefault("status", "completed")
        result = store.append(
            "continuity.item_updated" if verb == "update" else "continuity.item_closed",
            idempotency_key=f"turn:{line_key}", payload=payload,
            expected_revision=revision, **common,
        )
        return Outcome(statement, "duplicate" if result.duplicate else "applied", record_id=item_id)
    if verb == "delegated":
        unknown_fields = sorted(set(fields) - DELEGATED_FIELDS)
        if unknown_fields:



            raise ValidationError(
                f"delegated does not accept {', '.join(unknown_fields)}; only "
                + ", ".join(sorted(DELEGATED_FIELDS)) + " (status comes from the task authority)"
            )
        task = fields.get("task")
        title = fields.get("title")
        if not task or not title:
            raise ValidationError("delegated needs task=<id> and title=...")
        authority = fields.get("authority", "claude-code-task-store")
        work_id = work_unit_identity(authority, task, scope.entity_id)
        conn = store.connect()
        try:
            existing = conn.execute(
                "SELECT revision, status FROM ecs_work_units WHERE work_unit_id=?", (work_id,)
            ).fetchone()
        finally:
            conn.close()
        if existing and existing["status"] in TERMINAL_WORK_STATUSES:


            return Outcome(
                statement, "duplicate",
                f"task authority already reports {existing['status']}; reference kept, not rewritten",
                record_id=work_id,
            )
        payload = {
            "work_unit_id": work_id, "authority": authority, "external_id": task,
            "title": title, "status": "unknown",
        }
        if fields.get("owner"):
            payload["owner"] = fields["owner"]
        result = store.append(
            "work_unit.upserted", idempotency_key=f"turn:{line_key}", payload=payload,
            expected_revision=int(existing["revision"]) if existing else None, **common,
        )
        return Outcome(statement, "duplicate" if result.duplicate else "applied", record_id=work_id)
    if verb == "withdraw":
        if set(fields) != {"id", "evidence"} or not fields["evidence"].strip():
            raise ValidationError("withdraw needs a correction id and evidence")
        revision = _revision_of(store, "ecs_corrections", "correction_id", fields["id"])
        result = store.append("correction.withdrawn", idempotency_key=idempotency_key,
                              payload={"correction_id": fields["id"], "evidence_ref": fields["evidence"]},
                              expected_revision=revision, **common)
        return Outcome(statement, "duplicate" if result.duplicate else "applied", record_id=fields["id"])
    if verb == "correction":
        payload = _correction_payload(fields)
        mode = payload.get("mode")
        if mode not in ("explicit", "inferred"):
            raise ValidationError("correction needs mode=explicit or mode=inferred")
        if not payload.get("content"):
            raise ValidationError("correction needs content=...")
        payload.setdefault("classification", "principle" if mode == "inferred" else "unclassified")
        if mode == "explicit" and payload["classification"] == "unclassified":
            payload["apply_state"] = "observed"
        payload.setdefault("correction_id", stable_id("cor", scope.entity_id, mode, payload["content"]))
        payload["source_turn"] = scope.turn_id
        payload.pop("rejection_reason", None)
        result = store.append(
            "correction.recorded", idempotency_key=f"turn:{line_key}", payload=payload, **common
        )
        outcome = Outcome(statement, "duplicate" if result.duplicate else "applied", record_id=payload["correction_id"])
        outcome.routed = _route_if_ready(store, payload["correction_id"], scope, active_revision)
        return outcome
    if verb == "classify":
        payload = _correction_payload(fields)
        correction_id = payload.pop("correction_id", None)
        if not correction_id:
            raise ValidationError("classify needs id=<correction id>")
        payload.pop("mode", None)
        payload.pop("content", None)
        payload = {"correction_id": correction_id, **payload}
        if payload.get("classification") and payload["classification"] not in CORRECTION_ROUTABLE_CLASSES:
            raise ValidationError(
                "class must be one of " + ", ".join(sorted(CORRECTION_ROUTABLE_CLASSES))
            )
        revision = _revision_of(store, "ecs_corrections", "correction_id", correction_id)
        result = store.append(
            "correction.classified", idempotency_key=f"turn:{line_key}", payload=payload,
            expected_revision=revision, **common,
        )
        outcome = Outcome(statement, "duplicate" if result.duplicate else "applied", record_id=correction_id)
        outcome.routed = _route_if_ready(store, correction_id, scope, active_revision)
        return outcome
    raise ValidationError(
        f"unknown verb {verb!r}; use one of "
        + ", ".join(sorted(OPEN_VERBS | {"update", "close", "delegated", "correction", "classify"}))
    )


def _prior_outcome(
    store: EventStore, statement: Statement, scope: TurnScope, active_revision: int, prior: Any
) -> Outcome:
    """The outcome of a line an earlier delivery of this turn already applied."""
    payload = json.loads(prior["payload_json"]) if isinstance(prior["payload_json"], str) else {}
    record_id = (
        payload.get("item_id") or payload.get("work_unit_id") or payload.get("correction_id")
    )
    outcome = Outcome(
        statement, "duplicate", "already applied by an earlier delivery of this turn", record_id=record_id,
    )
    if statement.verb in ("correction", "classify") and record_id:

        outcome.routed = _route_if_ready(store, record_id, scope, active_revision)
    return outcome


def _route_if_ready(
    store: EventStore, correction_id: str, scope: TurnScope, active_revision: int
) -> Optional[dict[str, Any]]:
    """Route immediately when the correction is accepted and unambiguous; else report why not."""
    conn = store.connect()
    try:
        row = conn.execute(
            "SELECT mode, apply_state, ambiguity_state FROM ecs_corrections WHERE correction_id=?",
            (correction_id,),
        ).fetchone()
    finally:
        conn.close()
    if row is None:
        return None
    if row["mode"] != "explicit" or row["apply_state"] != "accepted" or row["ambiguity_state"] != "unambiguous":
        return {
            "correction_id": correction_id,
            "apply_state": row["apply_state"],
            "ambiguity_state": row["ambiguity_state"],
            "routed": False,
        }
    result = store.route_correction(
        correction_id,
        entity_id=scope.entity_id,
        thread_id=scope.thread_id,
        active_context_revision=active_revision,
        source_ref=scope.source_ref,
        session_id=scope.session_id,
        person_id=scope.person_id,
    )
    result["routed"] = True
    return result


def record_extraction(
    store: EventStore,
    scope: TurnScope,
    *,
    source_kind: str,
    result: ExtractionResult,
    idempotency_suffix: str,
) -> None:
    """Append the turn's extraction row: what was seen, applied and rejected."""
    store.append(
        "turn.extracted",
        entity_id=scope.entity_id,
        thread_id=scope.thread_id,
        session_id=scope.session_id,
        person_id=scope.person_id,
        source_ref=scope.source_ref,
        idempotency_key=f"extract:{scope.turn_id}:{source_kind}:{idempotency_suffix}",
        payload={
            "extraction_id": stable_id("ext", scope.turn_id, source_kind, idempotency_suffix),
            "turn_id": scope.turn_id,
            "source_kind": source_kind,
            "block_present": result.block_present,
            "statements": len(result.statements),
            "applied": result.applied,
            "rejected": result.rejected,
            "rejections": [r[:500] for r in result.rejections][:50],
        },
        actor_kind="extractor",
        actor_id="ecs-extraction-v1",
    )


def extract_reply(
    store: EventStore,
    reply_text: str,
    scope: TurnScope,
    *,
    active_revision: int,
    idempotency_suffix: str,
) -> ExtractionResult:
    """Parse Rich's reply, apply its ecs statements, record the extraction row."""
    block_present, statements, errors = parse_ecs_blocks(reply_text)
    result = ExtractionResult(block_present=block_present, statements=statements, parse_errors=errors)
    if statements:
        result.outcomes = apply_statements(store, statements, scope, active_revision=active_revision)
    record_extraction(
        store, scope, source_kind="rich_reply", result=result, idempotency_suffix=idempotency_suffix
    )
    return result


def extract_prompt(
    store: EventStore,
    prompt_text: str,
    scope: TurnScope,
    *,
    active_revision: int,
    redact: Callable[[str], str],
) -> ExtractionResult:
    """Record observed CEO corrections from the prompt; nothing is applied."""
    sentences = detect_ceo_corrections(prompt_text)
    statements = [
        Statement(verb="correction", fields={"mode": "explicit", "content": redact(s)}, line_no=i + 1, raw=s)
        for i, s in enumerate(sentences)
    ]
    result = ExtractionResult(block_present=bool(statements), statements=statements)
    outcomes: list[Outcome] = []
    for statement in statements:
        content = statement.fields["content"]
        correction_id = stable_id("cor", scope.entity_id, "observed", content)
        existing = _correction_turns(store, correction_id)
        if existing is not None and scope.turn_id in existing:


            outcomes.append(Outcome(
                statement, "duplicate", "already observed in this turn", record_id=correction_id,
            ))
            continue
        if existing is not None:




            try:
                appended = store.append(
                    "correction.reobserved",
                    entity_id=scope.entity_id,
                    thread_id=scope.thread_id,
                    session_id=scope.session_id,
                    person_id=scope.person_id,
                    source_ref=scope.source_ref,
                    idempotency_key=f"turn:{scope.turn_id}:ceo-reobserved:{correction_id}",
                    active_context_revision=active_revision,
                    payload={"correction_id": correction_id, "source_turn": scope.turn_id},
                    actor_kind="ceo",
                    actor_id="ceo",
                )
                outcomes.append(Outcome(
                    statement, "duplicate",
                    "repeat: already observed in an earlier turn; reobservation journaled",
                    record_id=correction_id,
                ))
            except (ValidationError, ScopeError, RevisionConflict) as exc:
                outcomes.append(Outcome(statement, "rejected", f"{type(exc).__name__}: {exc}"))
            continue
        try:
            appended = store.append(
                "correction.recorded",
                entity_id=scope.entity_id,
                thread_id=scope.thread_id,
                session_id=scope.session_id,
                person_id=scope.person_id,
                source_ref=scope.source_ref,
                idempotency_key=f"turn:{scope.turn_id}:ceo-correction:{correction_id}",
                active_context_revision=active_revision,
                payload={
                    "correction_id": correction_id,
                    "mode": "explicit",
                    "classification": "unclassified",
                    "apply_state": "observed",
                    "content": content,
                    "source_turn": scope.turn_id,
                },
                actor_kind="ceo",
                actor_id="ceo",
            )
            outcomes.append(Outcome(statement, "duplicate" if appended.duplicate else "applied", record_id=correction_id))
        except RevisionConflict as exc:


            outcomes.append(Outcome(statement, "duplicate", f"already observed: {exc}", record_id=correction_id))
        except (ValidationError, ScopeError) as exc:
            outcomes.append(Outcome(statement, "rejected", f"{type(exc).__name__}: {exc}"))
    result.outcomes = outcomes
    record_extraction(
        store, scope, source_kind="ceo_prompt", result=result, idempotency_suffix="prompt"
    )
    return result
