#!/usr/bin/env python3
"""Workspace adapter for recovery. Sealing, quiescence and harness ending
are distinct observations. This adapter never manufactures an end record.
"""
import hashlib
import json
import os
from pathlib import Path
import tarfile
import time
import stat

import recovery as R

CANCEL_SECONDS = 300


def work(ws, rec):
    observing = bool((rec.get("recovery_assignment") or {}).get("observation_only"))
    chain = ws.recovery_chain(rec, observation=observing)
    aliases = {"teammate:"+r["key"] for r in chain}
    identity = ws.recovery_work_key(rec, observation=observing)
    aliases.add(identity)
    original = chain[0].get("recovery_assignment") or {}
    acceptance = original.get("acceptance") or "legacy:"+chain[0]["key"]
    return identity, aliases, acceptance


def admitted(ws, payload, rec, assignment, repos, entity, dry=False):
    declaration = R.mode(entity)
    if R.quiet(declaration["mode"]):
        assignment = rec.get("recovery_observation_assignment") or assignment
    if declaration["mode"] == "off" or assignment["kind"] != "implementation":
        return []
    if R.quiet(declaration["mode"]):
        # Dispatch admission follows observe: a recovery gap never refuses.
        try:
            return _admitted(ws, payload, rec, assignment, repos, declaration, dry)
        except Exception as error:
            rec["recovery_observation_gap"] = str(error)
            return []
    return _admitted(ws, payload, rec, assignment, repos, declaration, dry)


def _admitted(ws, payload, rec, assignment, repos, declaration, dry):
    rec = dict(rec, recovery_assignment=assignment)
    identity, aliases, acceptance = work(ws, rec)
    ti = payload.get("tool_input") or {}
    prompt = str(ti.get("prompt") or "")
    disposition = ws.prompt_lines(prompt, "recovery-disposition")
    replaces = ws.prompt_lines(prompt, "recovery-replaces")
    if len(disposition) > 1 or len(replaces) > 1:
        raise R.Unavailable("ambiguous recovery replacement metadata")
    prefix, marker, body = prompt.partition("\nrecovery-implementation:\n") if disposition else ("", "", prompt)
    if disposition and (not marker or any(line.strip() and not line.startswith((
            "recovery-disposition:", "recovery-replaces:", "continues:", "workspace:", "cross-repo-worktree:", "lands-pending:", "correction-of:", "recovery-role:"))
            for line in prefix.splitlines())):
        raise R.Unavailable("replacement prefix must contain only host recovery/workspace metadata")
    result = []
    for repo in sorted(set(os.path.realpath(p) for p in repos if p)):
        if repo not in {os.path.realpath(p) for p in declaration["repositories"]}:
            continue
        record = R.reconcile(repo, identity, aliases, rec["session_id"], acceptance,
                             mode=declaration["mode"], dry=dry, premises=declaration.get("wes_policy", "carried"))
        if not record:
            continue
        R.admission(record, rec["key"], body, disposition[0] if disposition else "", dry=dry,
                    replaces=replaces[0] if replaces else "", safe=lambda key: safe(ws, key))
        result.append(record)
    return result


def begin_tool(ws, payload, rec):
    """Serialize tool permission with control installation. Conservative open
    admissions are cleared only by native completion or actual harness end.
    """
    if not rec.get("recovery_control"):
        declaration = R.mode(rec.get("entity"))
        if R.quiet(declaration["mode"]):
            return True, ""  # no holds outside enforce
        if not rec.get("recovery_jobs") and (declaration["mode"] != "enforce" or
                (rec.get("recovery_assignment") or {}).get("kind") != "implementation"):
            return True, ""
    with ws.Lock():
        fresh = ws.load_agent(rec["key"])
        if not fresh:
            return False, "registration vanished"
        control = fresh.get("recovery_control")
        if control:
            return False, "recovery %s disabled this worker's tools: %s" % (control["job"], control["reason"])
        for identity in fresh.get("recovery_jobs", []):
            record = R.read(identity)
            if record and record["mode"] == "enforce" and record["status"] in ("awaiting_alex", "unavailable"):
                return False, "recovery %s is %s; this job cannot keep correcting" % (identity, record["status"])
        tid = payload.get("tool_use_id")
        if tid:
            fresh.setdefault("recovery_tools", {})[tid] = dict(tool=payload.get("tool_name"), at=time.time())
            ws.save_agent(fresh)
    return True, ""


def finish_tool(ws, payload):
    key = ws.key_for_id(str(payload.get("agent_id") or ""))
    if not key:
        return
    rec = ws.load_agent(key)
    tid = payload.get("tool_use_id")
    if not rec or tid not in rec.get("recovery_tools", {}):
        return
    with ws.Lock():
        rec = ws.load_agent(key)
        if not rec:
            return
        tid = payload.get("tool_use_id")
        if tid and tid in rec.get("recovery_tools", {}):
            del rec["recovery_tools"][tid]
            ws.save_agent(rec)


def acknowledgement(ws, session, aid, signal):
    key = ws.key_for_id(aid)
    if not key:
        return
    with ws.Lock():
        rec = ws.load_agent(key)
        if not rec:
            return
        # Only native observed ending, not stop.sh intent or a side-effect seal.
        if (signal not in ("stopped", "SubagentStop") or not rec.get("end")
                or rec["end"].get("signal") != signal):
            return
        rec["recovery_native_end"] = dict(at=time.time(), signal=signal, session=session, agent_id=aid)
        if not rec.get("recovery_control"):
            ws.save_agent(rec)
            return
        rec["recovery_control"].update(harness_ended_at=time.time(), harness_signal=signal)
        rec["recovery_tools"] = {}
        ws.save_agent(rec)


def safe(ws, key):
    rec = ws.load_agent(key)
    control = (rec or {}).get("recovery_control") or {}
    result = control.get("owned_commands") or {}
    return bool(control.get("side_effects_disabled") and not rec.get("recovery_tools")
                and result.get("ok") and not result.get("skipped")
                and not result.get("survivors") and not result.get("excluded") and preserved(control.get("wip")))


def preserved(reference):
    if not reference:
        return False
    try:
        path = Path(reference)
        for n, space in enumerate(json.loads(path.read_text())):
            directory = path.parent / str(n)
            for name, field in (("tracked.patch", "tracked_sha256"), ("untracked.tar", "untracked_sha256")):
                if hashlib.sha256((directory/name).read_bytes()).hexdigest()!=space[field]:
                    return False
        return True
    except (OSError, ValueError, KeyError, TypeError):
        return False


def snapshot(ws, rec, job):
    root = R.storage_root("wip") / job / hashlib.sha256(rec["key"].encode()).hexdigest()
    root.mkdir(parents=True, exist_ok=True)
    manifest = []
    for n, space in enumerate(rec.get("workspaces") or []):
        path = space.get("path")
        if not path or not Path(path).is_dir():
            continue
        directory = root / str(n)
        directory.mkdir(exist_ok=True)
        code, patch, why = ws.git(path, "diff", "--binary", "HEAD")
        if code:
            raise R.Unavailable("could not preserve WIP: "+why)
        (directory / "tracked.patch").write_text(patch)
        code, untracked, why = ws.git(path, "ls-files", "-z", "--others", "--exclude-standard")
        if code:
            raise R.Unavailable("could not enumerate WIP: "+why)
        files = [p for p in untracked.split("\0") if p]
        total = 0
        with tarfile.open(directory / "untracked.tar", "w") as archive:
            for name in files:
                target = Path(path) / name
                if not target.is_symlink():
                    if not stat.S_ISREG(target.stat().st_mode):
                        raise R.Unavailable("non-regular untracked WIP requires explicit preservation: "+name)
                    total += target.stat().st_size
                if total > 128 * 1024 * 1024:
                    raise R.Unavailable("untracked WIP exceeds preservation bound; replacement held")
                archive.add(target, arcname=name, recursive=False)
        code, tip, why = ws.git(path, "rev-parse", "HEAD")
        if code:
            raise R.Unavailable("WIP source identity unavailable: "+why)
        manifest.append(dict(workspace=path, repo=space.get("repo"), branch=space.get("branch"), tip=tip.strip(), files=files,
                             tracked_sha256=hashlib.sha256(patch.encode()).hexdigest(),
                             untracked_sha256=hashlib.sha256((directory/"untracked.tar").read_bytes()).hexdigest(), bytes=total))
    (root / "manifest.json").write_text(json.dumps(manifest, sort_keys=True))
    return str(root / "manifest.json")


def source_identity(ws, path):
    code, tip, why = ws.git(path, "rev-parse", "HEAD")
    if code:
        raise R.Unavailable("worker source unavailable: "+why)
    code, delta, why = ws.git(path, "diff", "--binary", "HEAD")
    if code:
        raise R.Unavailable("worker delta unavailable: "+why)
    code, untracked, why = ws.git(path, "ls-files", "-z", "--others", "--exclude-standard")
    if code:
        raise R.Unavailable("worker untracked identity unavailable: "+why)
    files, total = [], 0
    for name in sorted(p for p in untracked.split("\0") if p):
        target = Path(path)/name
        if target.is_symlink():
            data = os.readlink(target).encode()
        else:
            if not stat.S_ISREG(target.stat().st_mode):
                raise R.Unavailable("worker contains non-regular untracked input: "+name)
            total += target.stat().st_size
            if total>128*1024*1024:
                raise R.Unavailable("worker source identity exceeds its bound")
            data = target.read_bytes()
        files.append([name, hashlib.sha256(data).hexdigest()])
    return tip.strip()+":"+R.digest([delta, files])


def stop(ws, key, record, control, tip):
    if record["mode"] != "enforce":
        return dict(status="would_stop", source=tip)
    rec = ws.load_agent(key)
    if not rec or ws.recovery_work_key(rec) != record["work"]:
        return dict(status="unavailable", reason="exact worker no longer belongs to this job")
    existing = rec.get("recovery_control")
    if existing and existing["job"] != record["id"]:
        return dict(status="unavailable", reason="conflicting recovery control")
    if not existing:
        with ws.Lock():
            rec = ws.load_agent(key)
            matching = [w for w in rec.get("workspaces") or [] if w.get("repo") and
                        R.F.review_repo_identity(w["repo"]) == record["repo_id"] and w.get("path")]
            sources = [source_identity(ws, w["path"]) for w in matching]
            if not sources or any(source != tip for source in sources):
                return dict(status="stale", reason="worker source changed; bounded applicability update required")
            rec["recovery_control"] = dict(job=record["id"], id=control["id"], source=tip, reason=control["reason"],
                requested_at=time.time(), side_effects_disabled=True, cancellation_deadline=time.time()+CANCEL_SECONDS,
                cancellation_requested=True)
            notice = R.root() / "cancellations" / (R.digest(rec["session_id"])+".json")
            keys = json.loads(notice.read_text()) if notice.exists() else []
            R.F.write_json_atomic(str(notice), sorted(set(keys+[key])))
            ws.save_agent(rec)
    # Releasing only this target's old pause permits its owned commands to end.
    ws.release_held_ids(rec["session_id"], rec.get("agent_id") or "", rec.get("name") or "", key)
    result = ws.end_owned_work(rec["session_id"], rec.get("agent_id") or "", rec.get("name") or "", key,
                               "recovery side effects disabled; harness ending remains unacknowledged")
    session, why = ws.session_state(rec["session_id"], rec.get("session_identity"))
    with ws.Lock():
        fresh = ws.load_agent(key)
        native = fresh.get("recovery_native_end") or {}
        if native and native.get("agent_id")==fresh.get("agent_id") and native.get("session")==fresh["session_id"]:
            fresh["recovery_control"].update(harness_ended_at=native["at"], harness_signal=native["signal"])
            fresh["recovery_tools"] = {}
        if session=="ended":
            fresh["recovery_control"].update(harness_ended_at=time.time(), harness_signal="session-ended", harness_evidence=why)
            fresh["recovery_tools"] = {}
        fresh["recovery_control"]["owned_commands"] = result or dict(ok=False, why="command observer unavailable")
        if (result and result.get("ok") and not result.get("skipped") and not result.get("survivors")
                and not result.get("excluded") and not fresh.get("recovery_tools") and not fresh["recovery_control"].get("wip")):
            fresh["recovery_control"]["wip"] = snapshot(ws, fresh, record["id"])
        state = fresh["recovery_control"]
        state["quiescent"] = bool(preserved(state.get("wip")) and not fresh.get("recovery_tools")
            and result and result.get("ok") and not any(result.get(k) for k in ("skipped", "survivors", "excluded")))
        if state.get("harness_ended_at"):
            state["cancellation_status"] = "acknowledged"
        elif time.time() >= state["cancellation_deadline"]:
            state["cancellation_status"] = "unavailable"
        else:
            state["cancellation_status"] = "requested"
        ws.save_agent(fresh)
    return dict(state)


def cancellation_authority(ws, task_id, session):
    # Typed recovery authority, independent from the CEO-only stop ack.
    matches = [r for r in ws.all_agents() if task_id in (r.get("agent_id"), r.get("key"), r.get("name"))
               and r.get("session_id") == session and r.get("recovery_control")]
    if len(matches) != 1:
        return False
    control = matches[0]["recovery_control"]
    record = R.read(control["job"])
    return bool(record and record["mode"] == "enforce" and control.get("side_effects_disabled")
                and any(c["id"] == control["id"] and matches[0]["key"] in c["workers"] for c in record["controls"]))


def schedule(record, engine, env=None):
    try:
        return _schedule(record, engine, env)
    except Exception as error:
        if not R.quiet(record["mode"]):
            raise
        return dict(observation_gap=str(error))


def _schedule(record, engine, env=None):
    import recovery_runner
    if record["mode"]=="enforce" and (record["controls"] or record["status"] == "awaiting_alex" or
            (record["status"] == "unavailable" and (not record.get("repeat_signal") or record.get("repeat_recovery")))):
        recovery_runner.launch(record, "control", engine, env=env)
    if record.get("axel_required", True):
        recovery_runner.launch(record, "axel", engine, env=env)
    if record.get("wes_candidates") or record.get("repeat_signal"):
        recovery_runner.launch(record, "wes", engine, env=env)


def cancellation_notice(ws, session):
    notice = R.root() / "cancellations" / (R.digest(session)+".json")
    try:
        keys = json.loads(notice.read_text())
    except FileNotFoundError:
        return ""
    except (OSError, ValueError):
        keys = [r["key"] for r in ws.all_agents() if r.get("session_id")==session and r.get("recovery_control")]
    pending = [r for key in keys for r in [ws.load_agent(key)] if r and r.get("session_id")==session and r.get("agent_id")
               and r.get("recovery_control") and not r["recovery_control"].get("harness_ended_at")]
    return "\n".join("RECOVERY CONTROL %s: tools disabled for %s. Execute TaskStop with task_id %s; "
        "record native completion. Command quiescence does not prove its model loop ended." % (
        r["recovery_control"]["job"], r["name"], r["agent_id"]) for r in pending)
