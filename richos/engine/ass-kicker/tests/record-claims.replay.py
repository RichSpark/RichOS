#!/usr/bin/env python3
"""record-claims.replay.py — replay ARM 3 of ass-kicker.py over real
lead transcripts, exactly as the Stop payload would present each turn end.

    python3 ass-kicker/tests/record-claims.replay.py <transcripts-dir> \
        [--out <private.jsonl>] [--since YYYY-MM-DD]

A STOP POINT is every place a Stop event fired: the last assistant text before
a `Stop hook feedback` record (a sibling hook refused and the lead went on),
before a `stop_hook_summary` system record, or at the end of the prompt span.
Each one is replayed with the transcript truncated at that point (read_turn
reads to end of file, and at Stop time the file ends there), the span's
prompt_id, the record's cwd, and stop_hook_active set when an earlier Stop in
the same span was refused. The 2026-10-02 claim was at such an INNER stop
point, refused by a different hook; a replay that read only each span's last
text would never have seen it.

stdout carries COUNTS ONLY. The sentences, which quote the lead's replies, go
to --out, which belongs in the private record and never in this repository.

Target classification reads today's filesystem (a worktree removed since is
classified as a plain file and needs no SHA). That is a limit of a replay, not
of the hook, which runs while the files are where the turn left them.
"""
import argparse
import importlib.util
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("gsa", os.path.join(HERE, "..", "ass-kicker.py"))
gsa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gsa)


def text_blocks(rec):
    c = (rec.get("message") or {}).get("content")
    if not isinstance(c, list):
        return []
    return [b.get("text", "") for b in c if isinstance(b, dict) and b.get("type") == "text" and b.get("text", "").strip()]


def is_stop_feedback(rec):
    if rec.get("type") == "system" and rec.get("subtype") == "stop_hook_summary":
        return True
    if rec.get("type") == "user":
        c = (rec.get("message") or {}).get("content")
        if isinstance(c, str) and c.startswith("Stop hook feedback:"):
            return True
    return False


def refused_feedback(rec):
    if rec.get("type") != "user":
        return False
    c = (rec.get("message") or {}).get("content")
    return isinstance(c, str) and c.startswith("Stop hook feedback:")


def stop_points(path):
    """Yield (prompt_id, cwd, lines_so_far, last_text, refired, ts)."""
    lines = []
    pid = None
    cwd = None
    last_text = None
    refired = False
    span_start = 0
    ts = None
    with open(path, encoding="utf-8", errors="replace") as fh:
        for raw in fh:
            try:
                rec = json.loads(raw)
            except Exception:
                continue
            p = rec.get("promptId")
            if p and p != pid:
                if pid and last_text:
                    yield pid, cwd, lines[span_start:], last_text, refired, ts
                pid, last_text, refired = p, None, False
                span_start = len(lines)
            lines.append(raw)
            if rec.get("cwd"):
                cwd = rec["cwd"]
            if rec.get("isSidechain"):
                continue
            if rec.get("type") == "assistant":
                tb = text_blocks(rec)
                if tb:
                    last_text = tb[-1]
                    ts = rec.get("timestamp")
            elif is_stop_feedback(rec) and pid and last_text:
                yield pid, cwd, lines[span_start:], last_text, refired, ts
                if refused_feedback(rec):
                    refired = True
                last_text = None
    if pid and last_text:
        yield pid, cwd, lines[span_start:], last_text, refired, ts


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dir")
    ap.add_argument("--out")
    ap.add_argument("--since")
    a = ap.parse_args()
    files = sorted((os.path.join(a.dir, f) for f in os.listdir(a.dir) if f.endswith(".jsonl")),
                   key=os.path.getmtime)
    n_files = n_points = n_claims = n_refused = n_backed = n_unread = n_alerted = 0
    out = open(a.out, "w", encoding="utf-8") if a.out else None
    tmpdir = tempfile.mkdtemp(prefix="record-claims-replay.")
    try:
        for f in files:
            n_files += 1
            sys.stderr.write("replay: %s\n" % os.path.basename(f)[:8])
            sys.stderr.flush()
            for pid, cwd, span, text, refired, ts in stop_points(f):
                if a.since and (ts or "") < a.since:
                    continue
                n_points += 1
                if not gsa.record_claims(text):
                    continue
                tpath = os.path.join(tmpdir, "turn.jsonl")
                with open(tpath, "w", encoding="utf-8") as t:
                    t.writelines(span)
                payload = {"session_id": os.path.basename(f)[:-6], "prompt_id": pid,
                           "transcript_path": tpath, "cwd": cwd or "/",
                           "stop_hook_active": refired, "last_assistant_message": text}
                v = gsa.record_claim_verdict(payload, text)
                n_claims += 1
                if gsa.unbacked_claims(v):
                    n_alerted += 1
                if v["verdict"] == "refused":
                    n_refused += 1
                elif v["verdict"] == "backed":
                    n_backed += 1
                else:
                    n_unread += 1
                if out:
                    out.write(json.dumps({"session": payload["session_id"][:8], "ts": ts, "prompt_id": pid,
                                          "refire": refired, "verdict": v["verdict"],
                                          "alerted": bool(gsa.unbacked_claims(v)),
                                          "claims": v["claims"], "failing": v["failing"],
                                          "written": v["written"]}) + "\n")
    finally:
        for name in os.listdir(tmpdir):
            os.unlink(os.path.join(tmpdir, name))
        os.rmdir(tmpdir)
        if out:
            out.close()
    print(json.dumps({"files": n_files, "stop_points": n_points, "claim_turns": n_claims,
                      "alerted": n_alerted, "judged_unbacked_or_unplaced": n_refused,
                      "refused": n_refused, "backed": n_backed, "unread": n_unread}))


if __name__ == "__main__":
    sys.exit(main())
