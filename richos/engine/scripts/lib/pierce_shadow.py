#!/usr/bin/env python3
"""Pierce on Haiku, run in parallel with Pierce on Opus: data collection only.

The CEO's ask (2026-10-10): a parallel track that collects a few hundred Pierce
feedbacks from both models, in two kinds (1: his context or input is involved,
2: it is not), so the data can later show if and when Pierce could run on Haiku.
Nothing here is used in production: the Haiku answer is shown to nobody, and the
Opus answer, the spawn and the hook's output are never touched. The plan:
richos-hq docs/plans/2026-10-10-pierce-haiku-parallel-track.md (Sage), built
without its section 8 switch-over limits (the CEO set none; the data decides).

    pierce_shadow.py run <job>   started detached by pierce.py beside every initial
                                 Opus review; never by hand
    pierce_shadow.py status [--project <repository>]
                                progress in that repository's selected period

On and off: PIERCE_SHADOW_MODEL in the governed repository's orchestration.config
(empty is off). The track also stops itself once both kinds have their sample and
raises one work-complete escalation saying so.

PIERCE_SHADOW_PERIOD selects an independent collection under periods/<id>/.
Its counters, pending judgments, skips and completion marker never read another
period or the legacy experiment. Detached jobs retain their original directory.
The period manifest pins reviewer/model inputs; select a new id after a change.
Absent or blank preserves the legacy layout. Invalid ids never fall back to it.

Everything is written under <state>/pierce-shadow/ (beside Pierce's own state,
outside every repository): jobs/, pairs/<pair>.{opus,haiku,kind,adjudication}.json,
revisions/, skipped.jsonl, dismissals.jsonl, shadow.log, complete.json.
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
import secrets
import shutil
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ENGINE = HERE.parents[1]
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import second_review as SR  # noqa: E402  (its quota hold and its isolated Codex call)

_spec = importlib.util.spec_from_file_location("pierce", HERE / "pierce.py")
pierce = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pierce)

RULE_VERSION = 1
# The sample: 100 briefs per kind with at least one real problem (plan section 5,
# roughly 450 paired runs per kind, his "few hundred"). A sample size, not a limit.
TARGET_POSITIVE_PER_KIND = 100
OPUS_WAIT_SECONDS = pierce.LIMIT + 120
ADJUDICATION_LIMIT_SECONDS = 30 * 60
ADJUDICATION_ATTEMPTS = 5
PENDING_PER_RUN = 3
LABELS = ("matched", "real-material", "real-minor", "false", "unverifiable")
LOADED_SHA256 = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
COLLECTION_ROOT = None

# ---------------------------------------------------------------------------
# Kind 1 or kind 2: rule v1 (plan section 2), the exact rule Sage's measure.py
# used for the traffic split. Every row keeps the three signals, so the split can
# be re-cut later without running anything again.
# ---------------------------------------------------------------------------
RUN = 8
ATTRIBUTION = re.compile(r"(His words|His order|His answer|The CEO's words|\(CEO,? \d{4}-\d{2}-\d{2}\)|CEO,? \d{4}-\d{2}-\d{2})")
QUOTED = re.compile(r"[\"“*][^\"”*\n]{25,}[\"”*]")
CEO_FILE = re.compile(r"(docs/ceo-input/|richos-hq/ceo/)")


def words(text):
    return re.findall(r"[a-z0-9']+", text.lower())


def runs(text):
    w = words(text)
    return {" ".join(w[i:i + RUN]) for i in range(len(w) - RUN + 1)}


def kind_of(request):
    """(kind, signals): kind 1 when the brief carries the CEO's words or input."""
    brief = request.get("outgoing_assignment", "")
    shared = runs(brief) & runs(request.get("user_requests_in_order", ""))
    shared -= runs(request.get("standing_instructions", ""))  # boilerplate he once pasted does not count
    session_words = bool(shared)
    record_words = False
    for paragraph in re.split(r"\n\s*\n", brief):
        found = ATTRIBUTION.search(paragraph)
        if found and QUOTED.search(paragraph[found.start():]):
            record_words = True
            break
    input_file = bool(CEO_FILE.search(brief))
    return (1 if session_words or record_words or input_file else 2,
            {"session_words": session_words, "record_words": record_words, "input_file": input_file})


# ---------------------------------------------------------------------------
# small helpers
# ---------------------------------------------------------------------------

def root():
    if COLLECTION_ROOT is not None:
        return COLLECTION_ROOT
    return pierce.shadow_dir(os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd())


def pairs():
    return root() / "pairs"


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    pierce.atomic(path, value)


def read(path):
    try:
        return json.loads(Path(path).read_text())
    except (OSError, ValueError):
        return None


def log(text):
    pierce.shadow_log(text, root())


def skip(pair, why, **extra):
    root().mkdir(parents=True, exist_ok=True, mode=0o700)
    with open(root() / "skipped.jsonl", "a") as out:
        out.write(json.dumps({"pair": pair, "at": time.time(), "why": why, **extra}, ensure_ascii=False) + "\n")


@contextlib.contextmanager
def held(path):
    """A nonblocking per-pair lock: yields False when another run holds it."""
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    with open(path, "a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            yield False
            return
        yield True


def heads(read_roots):
    """The commit of every read root that is a Git checkout, so a late judge reads what both models read."""
    out = {}
    for directory in read_roots or []:
        try:
            p = subprocess.run(["git", "-C", directory, "rev-parse", "--show-toplevel", "HEAD"],
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, timeout=10)
        except (OSError, subprocess.TimeoutExpired):
            continue
        lines = p.stdout.split()
        if p.returncode == 0 and len(lines) == 2:
            out[lines[0]] = lines[1]
    return out


def instruction_sources(request):
    return re.findall(r"(?m)^(?:Agent definition|Standing instructions) \((.+)\):$",
                      request.get("standing_instructions", ""))


# ---------------------------------------------------------------------------
# admission: his 93% quota rule only, never waited on. A busy CPU never skips a
# twin (Rich's instruction for this track, 2026-10-11).
# ---------------------------------------------------------------------------

def refused():
    """The reason no Haiku run may start now, or None."""
    held_now, reading = SR.quota_hold(str(ENGINE))
    if held_now:
        return "quota pause", reading
    return None


# ---------------------------------------------------------------------------
# the blind judge (plan section 4): Codex, a third model family, read-only
# ---------------------------------------------------------------------------
ADJUDICATION_SCHEMA = {
    "type": "object", "additionalProperties": False,
    "required": ["findings", "matches", "brief_depends_on_ceo_words", "ceo_words_evidence"],
    "properties": {
        "findings": {"type": "array", "items": {
            "type": "object", "additionalProperties": False,
            "required": ["id", "report", "text", "label", "evidence"],
            "properties": {
                "id": {"type": "string"},
                "report": {"type": "string", "enum": ["A", "B"]},
                "text": {"type": "string"},
                "label": {"type": "string", "enum": list(LABELS)},
                "evidence": {"type": "string"}}}},
        "matches": {"type": "array", "items": {
            "type": "object", "additionalProperties": False, "required": ["a", "b"],
            "properties": {"a": {"type": "string"}, "b": {"type": "string"}}}},
        "brief_depends_on_ceo_words": {"type": "boolean"},
        "ceo_words_evidence": {"type": "string"}}}


def needs_judging(opus, haiku):
    """Only a pair with two valid verdicts and at least one FINDINGS can separate the models."""
    verdicts = (opus or {}).get("verdict"), (haiku or {}).get("verdict")
    return all(v in ("PASS", "FINDINGS") for v in verdicts) and "FINDINGS" in verdicts


def judge_prompt(job, reports, commits):
    request = job["request"]
    lines = [
        "You are a neutral judge. Two independent inspectors, whose identities you are not told, each "
        "inspected the same outgoing assignment (\"the brief\") that a planner was about to give a worker. "
        "Their answers are Report A and Report B below. Judge their findings; change nothing.",
        "",
        "The standard both inspectors were given for a blocking finding, verbatim:",
        pierce.REVIEW_RULES.strip(),
        "",
        "Do this:",
        "1. Split each report into its separate findings (a PASS report usually has none). "
        "Number them A1, A2, ... and B1, B2, ...",
        "2. Match findings in A and B that describe the same problem; list each pair in matches and "
        "label both of its findings \"matched\".",
        "3. Label every unmatched finding: \"real-material\" (true, and it meets the blocking standard "
        "above: it would materially misdirect the work, violate the human's authorization or make the "
        "required proof invalid), \"real-minor\" (true, but only note-grade), \"false\" (the claim is "
        "wrong), or \"unverifiable\" (it cannot be checked with what you can read). Give evidence for "
        "each label: the file and what it says.",
        "4. Answer whether this brief depends on the CEO's own words or input (his messages below, a "
        "quotation attributed to him, or a file of his input), with one line of evidence.",
        "",
        "Check facts as they were when the brief was inspected: read files with "
        "`git -C <repository> show <commit>:<path>`, not the working tree, which may have changed since.",
        "Repositories and their commits at inspection time:",
    ]
    lines += ["- %s at %s" % (repo, commit) for repo, commit in sorted(commits.items())] or ["- (none recorded)"]
    lines += ["", "Inherited instruction files the worker receives:"]
    lines += ["- " + path for path in instruction_sources(request)] or ["- (none)"]
    lines.append("")
    text = "\n".join(lines) + "\n"
    text += SR.fence("THE BRIEF", request.get("outgoing_assignment", ""))
    text += SR.fence("THE HUMAN'S MESSAGES, IN ORDER", request.get("user_requests_in_order", ""))
    for label in ("A", "B"):
        text += SR.fence("REPORT %s (verdict %s)" % (label, reports[label]["verdict"]), reports[label]["report"])
    return text


def answer_problems(answer):
    if not isinstance(answer, dict) or not isinstance(answer.get("findings"), list) \
            or not isinstance(answer.get("matches"), list) \
            or not isinstance(answer.get("brief_depends_on_ceo_words"), bool):
        return "the answer does not have the required shape"
    ids = {}
    for finding in answer["findings"]:
        if not isinstance(finding, dict) or finding.get("report") not in ("A", "B") \
                or finding.get("label") not in LABELS or not isinstance(finding.get("id"), str):
            return "a finding does not have the required shape"
        if finding["id"] in ids:
            return "finding id %s is used twice" % finding["id"]
        ids[finding["id"]] = finding
    matched = set()
    for match in answer["matches"]:
        a, b = ids.get((match or {}).get("a")), ids.get((match or {}).get("b"))
        if not a or not b or a["report"] != "A" or b["report"] != "B":
            return "a match does not join a finding of A to a finding of B"
        matched.update((a["id"], b["id"]))
    for finding in ids.values():
        if (finding["label"] == "matched") != (finding["id"] in matched):
            return "finding %s is labeled %s against the matches" % (finding["id"], finding["label"])
    return None


def summarize(answer, order):
    """De-blinded counts per model: what the analysis reads (labels stay in the raw answer)."""
    out = {model: {"findings": 0, "matched": 0, "real-material": 0, "real-minor": 0, "false": 0,
                   "unverifiable": 0} for model in order.values()}
    for finding in answer["findings"]:
        counts = out[order[finding["report"]]]
        counts["findings"] += 1
        counts[finding["label"]] += 1
    positive = any(f["label"] in ("real-material", "matched") for f in answer["findings"])
    return out, positive


def adjudicate(pair):
    """Judge one pair under its lock. Codex missing, signed out or failing leaves it pending."""
    path = pairs() / (pair + ".adjudication.json")
    with held(pairs() / (pair + ".adjudication.lock")) as mine:
        if not mine:
            return
        state = read(path) or {"pair": pair, "status": "pending", "attempts": 0}
        if state.get("status") != "pending":
            return
        job = read(root() / "jobs" / (pair + ".json"))
        opus, haiku = read(pairs() / (pair + ".opus.json")), read(pairs() / (pair + ".haiku.json"))
        kind = read(pairs() / (pair + ".kind.json")) or {}
        if not job or not needs_judging(opus, haiku):
            state.update(status="not-needed")
            write(path, state)
            return
        codex = SR.find_codex()
        signed_in, said = SR.codex_signed_in(codex) if codex else (False, "Codex was not found")
        if not signed_in:
            state["last_reason"] = "Codex cannot run: " + said
            write(path, state)
            return
        order = dict(zip(("A", "B"), secrets.SystemRandom().sample(["opus", "haiku"], 2)))
        reports = {label: {"opus": opus, "haiku": haiku}[model] for label, model in order.items()}
        work = root() / "work" / pair
        shutil.rmtree(work, ignore_errors=True)
        work.mkdir(parents=True, mode=0o700)
        try:
            schema, answer_path = work / "schema.json", work / "answer.json"
            schema.write_text(json.dumps(ADJUDICATION_SCHEMA))
            cwd = job["request"].get("project_dir")
            cwd = cwd if cwd and os.path.isdir(cwd) else str(work)
            argv = [codex, "exec"] + SR.CODEX_ISOLATION + [
                "--sandbox", "read-only", "-C", cwd, "--skip-git-repo-check",
                "-m", SR.CODEX_MODEL, "-c", 'model_reasoning_effort="%s"' % SR.CODEX_EFFORT,
                "-c", 'approval_policy="never"', "--output-schema", str(schema), "-o", str(answer_path),
                "--json", "--color", "never", "-"]
            prompt = judge_prompt(job, reports, kind.get("commits", {}))
            code, seconds = SR.run_bounded(argv, cwd, prompt, str(work / "stdout.jsonl"),
                                           str(work / "stderr.log"), ADJUDICATION_LIMIT_SECONDS, env=SR.codex_env())
            facts = SR.codex_run_facts(str(work / "stdout.jsonl"), str(work / "stderr.log"))
            answer = read(answer_path)
            problem = ("Codex did not finish within %d s" % ADJUDICATION_LIMIT_SECONDS if code is None
                       else "Codex exited %d" % code if code else answer_problems(answer))
            if problem:
                state["attempts"] = state.get("attempts", 0) + 1
                state["last_reason"] = problem
                if state["attempts"] >= ADJUDICATION_ATTEMPTS:
                    state["status"] = "abandoned"
                write(path, state)
                log("pair %s: adjudication attempt %d failed: %s" % (pair, state["attempts"], problem))
                return
            counts, positive = summarize(answer, order)
            write(path, {"pair": pair, "status": "done", "at": time.time(), "order": order,
                         "judge": {"model": facts.get("model") or SR.CODEX_MODEL, "effort": facts.get("effort")
                                   or SR.CODEX_EFFORT, "tokens": facts.get("tokens"), "seconds": round(seconds, 1)},
                         "answer": answer, "counts": counts, "positive": positive,
                         "kind": kind.get("kind"), "judge_says_ceo_words": answer["brief_depends_on_ceo_words"],
                         "attempts": state.get("attempts", 0) + 1})
        finally:
            shutil.rmtree(work, ignore_errors=True)
            with contextlib.suppress(OSError):
                work.parent.rmdir()  # only when no other judge is working


def pending_pairs():
    out = []
    for path in sorted(pairs().glob("*.adjudication.json")) if pairs().is_dir() else []:
        if (read(path) or {}).get("status") == "pending":
            out.append(path.name[:-len(".adjudication.json")])
    return out


# ---------------------------------------------------------------------------
# progress and the self-off at the sample
# ---------------------------------------------------------------------------

def progress():
    """Per kind: pairs, valid pairs, judged, pending, positive briefs."""
    out = {k: {"pairs": 0, "both valid": 0, "judged": 0, "pending": 0, "positive": 0,
               "haiku unavailable": 0} for k in (1, 2)}
    if not pairs().is_dir():
        return out
    for path in sorted(pairs().glob("*.kind.json")):
        pair = path.name[:-len(".kind.json")]
        kind = (read(path) or {}).get("kind")
        if kind not in out:
            continue
        haiku, opus = read(pairs() / (pair + ".haiku.json")), read(pairs() / (pair + ".opus.json"))
        if not haiku:
            continue
        row = out[kind]
        row["pairs"] += 1
        row["haiku unavailable"] += haiku.get("verdict") == "UNAVAILABLE"
        if opus and all(v.get("verdict") in ("PASS", "FINDINGS") for v in (opus, haiku)):
            row["both valid"] += 1
        judged = read(pairs() / (pair + ".adjudication.json")) or {}
        row["judged"] += judged.get("status") == "done"
        row["pending"] += judged.get("status") == "pending"
        row["positive"] += bool(judged.get("positive"))
    return out


def kind_complete(counts, kind):
    return counts[kind]["positive"] >= TARGET_POSITIVE_PER_KIND


def finish_if_complete():
    counts = progress()
    if not all(kind_complete(counts, k) for k in (1, 2)):
        return
    try:
        fd = os.open(root() / "complete.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError:
        return
    with os.fdopen(fd, "w") as out:
        json.dump({"at": time.time(), "progress": counts}, out)
    title = "Pierce on Haiku parallel track: the sample is collected and the track has stopped itself"
    question = ("Both kinds have %d briefs with a real problem (%s). Who analyzes the data in %s?"
                % (TARGET_POSITIVE_PER_KIND, json.dumps(counts), root()))
    p = subprocess.run([sys.executable, "-B", str(HERE / "escalations.py"), "raise", "--title", title,
                        "--state", "work-complete", "--for", "lead", "--question", question,
                        "--teammate", "pierce-shadow"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                       text=True, timeout=60)
    log("sample complete; escalation exit %d: %s" % (p.returncode, p.stdout.strip()[:500]))


# ---------------------------------------------------------------------------
# one pair
# ---------------------------------------------------------------------------

def run(job_path):
    # A detached worker belongs to the job's original directory. Configuration
    # may change while it runs; never reselect or migrate its counters/evidence.
    global COLLECTION_ROOT
    COLLECTION_ROOT = Path(job_path).resolve().parent.parent
    job = read(job_path)
    if not job:
        log("job %s unreadable" % job_path)
        return 1
    pair, request = job["pair"], job["request"]
    try:
        if job.get("collection") or root().parent.name == "periods":
            collection = job.get("collection") or {}
            expected = {"period": root().name if root().parent.name == "periods" else "", "model": job["model"], "effort": job["effort"],
                        "source_sha256": pierce.SOURCE_SHA256,
                        "profile_sha256": hashlib.sha256(job["profile"].encode()).hexdigest(),
                        "rules_sha256": hashlib.sha256(pierce.REVIEW_RULES.encode()).hexdigest(),
                        "access_sha256": pierce.access_fingerprint(request["project_dir"]),
                        "shadow_sha256": LOADED_SHA256}
            if (collection != expected or request.get("access_sha256") != expected["access_sha256"]
                    or read(root() / "period.json") != collection):
                skip(pair, "period or inspector identity changed before twin started")
                return 0
        kind, signals = kind_of(request)
        write(pairs() / (pair + ".kind.json"), {
            "pair": pair, "kind": kind, "signals": signals, "rule_version": RULE_VERSION,
            "commits": heads(request.get("read_roots")),
            "sizes": {"brief_chars": len(request.get("outgoing_assignment", "")),
                      "human_chars": len(request.get("user_requests_in_order", "")),
                      "instruction_chars": len(request.get("standing_instructions", "")),
                      "read_roots": len(request.get("read_roots") or [])},
            "worker": job.get("worker")})
        if kind_complete(progress(), kind):
            skip(pair, "kind %d sample complete" % kind)
            return 0
        why = refused()
        if why:
            skip(pair, why[0], detail=why[1])
            return 0
        record = {"pair": pair, "model": job["model"], "effort": job["effort"], "started": time.time()}
        try:
            verdict, metrics = pierce.run_review(request, job["profile"], job["model"], job["effort"], "stream-json")
            record.update(verdict=verdict["verdict"], report=verdict["report"], notes=verdict["notes"])
        except (OSError, ValueError) as error:
            # A refused or failed Haiku run is UNAVAILABLE, never a verdict.
            metrics = getattr(error, "metrics", {})
            record.update(verdict="UNAVAILABLE", report=str(error))
        prompts = metrics.get("request_prompt_tokens") or []
        record.update(metrics=metrics, resolved_models=sorted((metrics.get("modelUsage") or {}).keys()),
                      requests_over_100k=sum(1 for n in prompts if n > 100000))
        write(pairs() / (pair + ".haiku.json"), record)
        deadline = job["at"] + OPUS_WAIT_SECONDS
        opus_path = pairs() / (pair + ".opus.json")
        while not opus_path.exists() and time.time() < deadline:
            time.sleep(1)
        if needs_judging(read(opus_path), record):
            write(pairs() / (pair + ".adjudication.json"), {"pair": pair, "status": "pending", "attempts": 0})
            adjudicate(pair)
        elif not opus_path.exists():
            log("pair %s: no Opus record within %d s" % (pair, OPUS_WAIT_SECONDS))
        for other in [p for p in pending_pairs() if p != pair][:PENDING_PER_RUN]:
            adjudicate(other)
        finish_if_complete()
        return 0
    except Exception as error:  # recorded, never raised into anything
        skip(pair, "error", detail="%s: %s" % (type(error).__name__, error))
        log("pair %s: %s: %s" % (pair, type(error).__name__, error))
        return 1
    finally:
        log("pair %s finished" % pair)


def status():
    counts = progress()
    print("Pierce on Haiku parallel track: %s" % root())
    print("  model: as set by PIERCE_SHADOW_MODEL; %s" % ("STOPPED: the sample is complete"
          if (root() / "complete.json").exists() else "collecting"))
    for kind in (1, 2):
        row = counts[kind]
        print("  kind %d: %d pairs, %d with both verdicts valid, %d judged, %d pending a judge, "
              "%d of %d positive briefs, Haiku unavailable %d"
              % (kind, row["pairs"], row["both valid"], row["judged"], row["pending"], row["positive"],
                 TARGET_POSITIVE_PER_KIND, row["haiku unavailable"]))
    skipped = {}
    try:
        for line in open(root() / "skipped.jsonl"):
            try:
                why = json.loads(line).get("why")
            except ValueError:
                continue
            skipped[why] = skipped.get(why, 0) + 1
    except OSError:
        pass
    print("  no twin: %s" % (", ".join("%s %d" % item for item in sorted(skipped.items())) or "none"))
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    one = sub.add_parser("run")
    one.add_argument("job")
    report = sub.add_parser("status")
    report.add_argument("--project", help="governed repository whose configured period to inspect")
    args = parser.parse_args(argv)
    if args.command == "status" and args.project:
        global COLLECTION_ROOT
        COLLECTION_ROOT = pierce.shadow_dir(args.project)
    return run(args.job) if args.command == "run" else status()


if __name__ == "__main__":
    raise SystemExit(main())
