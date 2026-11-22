#!/usr/bin/env python3
"""Observe an in-place bundle upgrade from nightly 115 in the signed test VM.

  run-walk.py --bundle NIGHTLY_115_ZIP --engine NIGHTLY_115_ENGINE --home EMPTY_HOME \
      --report REPORT -- ./codex-legacy-upgrade-walk.py \
      --next-bundle CANDIDATE_ZIP --next-engine CANDIDATE_ENGINE --expect-sha COMMIT --out DIR

Use an empty synthetic home. This walk replaces the guest bundle and engine
at their original paths while retaining the same home, as a manual upgrade.
It records Claude transcripts and session bindings before opening the candidate
and checks them while the app and its watcher run for ten minutes.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import time

from relaunch import guest, quit_app, relaunch

HERE = Path(__file__).resolve().parent
NIGHTLY = "1.3.7-nightly.20261122.115"
IDLE_SECONDS = 600

# Run inside the isolated guest so the legacy endpoint and cursors carry its actual paths and
# inode identities. Match the partially consumed spool and saved Markdown offsets on nightly 115.
SEED = r"""
import hashlib
import json
from pathlib import Path
import sys
import time

home = Path(sys.argv[1])
company, thread = "acme", "thr_" + "1" * 32
coord = home / ".richos-coordination"
legacy = coord / "rich-codex"
legacy.mkdir(parents=True, exist_ok=True)
raw = legacy / "to-rich.md"
raw.write_text(("## Old request\n" + "Archived Codex notes. " * 300 + "\n") * 250)
(legacy / "to-codex.md").write_text("Old Rich handoff.\n")
data = home / "Library/Application Support/com.richos.app"
data.mkdir(parents=True, exist_ok=True)
root = home / "acme"
root.mkdir(exist_ok=True)
watch = data / "engine-state/codex-watch"
spool = watch / "pending.jsonl"
ident = "legacy-" + hashlib.sha256(str(legacy).encode()).hexdigest()[:24]
owner = dict(version=1, entity_id=company, thread_id=thread)

def write(path, obj):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj) + "\n")

write(data / "entities.json", {"version": 2, "entities": [
    {"id": company, "display_name": "Acme", "status": "active", "roots": [str(root)]}]})
write(data / "config.json", dict(entity=company, schema_version=1))
write(data / "conversation-ledger.jsonl", dict(at=int(time.time() * 1000), binding_revision=1,
      entity_id=company, event="ThreadCreated", person_id="ceo-default", thread_id=thread,
      title="Codex", written_by=1))
write(coord / "automatic.json", owner)
write(coord / "connections" / company / thread / ident / "connection.json",
      dict(owner, connection_id=ident, legacy_folder=str(legacy), legacy_boundary=raw.stat().st_size,
           imports_legacy_spool=True, legacy_live=True, preserves_legacy_outbox=True))

record = json.dumps("Historical spool entry. " * 250) + "\n"
spool.parent.mkdir(parents=True, exist_ok=True)
spool.write_text(record * 250)
st = spool.stat()
write(watch / "accepted" / (ident + ".json"),
      dict(through=len(record), source_identity=f"{st.st_dev}:{st.st_ino}"))
old = {str(raw): dict(offset=0, inode=raw.stat().st_ino, seen=raw.stat().st_size)}
write(watch / "watch/last-told.json", old)
write(watch / "watch/app-host/offsets.json", old)
print(json.dumps(dict(raw_bytes=raw.stat().st_size, spool_bytes=st.st_size, legacy_cursor=len(record))))
"""

SNAPSHOT = r"""
import json
import os
from pathlib import Path
import sys

home = Path(sys.argv[1])
state = home / "Library/Application Support/com.richos.app/engine-state"
watch = state / "codex-watch"
offsets = watch / "watch/app-host/live-offsets.json"
raw = home / ".richos-coordination/rich-codex/to-rich.md"

def stat(path):
    st = path.stat()
    return [st.st_size, st.st_mtime_ns]

def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False

record = watch / "host-child.json"
watcher = json.loads(record.read_text())["pid"] if record.exists() else None
files = [*home.glob(".claude/projects/**/*.jsonl"), *home.glob(".claude/sessions/*.json"),
         *state.glob("evidence/*/binding.json"), *state.glob("messages/*/*/stu.jsonl")]
print(json.dumps(dict(records={str(p.relative_to(home)): stat(p) for p in files},
     app_alive=alive(int(sys.argv[2])) if sys.argv[2] else None,
     watcher=watcher, watcher_alive=alive(watcher) if watcher is not None else False,
     poll=offsets.stat().st_mtime_ns if offsets.exists() else None,
     raw_offset=json.loads(offsets.read_text()).get(str(raw), {}).get("offset") if offsets.exists() else None,
     live_bytes=(watch / "live.jsonl").stat().st_size if (watch / "live.jsonl").exists() else 0)))
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("vm")
    parser.add_argument("--next-bundle", required=True, type=Path)
    parser.add_argument("--next-engine", required=True, type=Path)
    parser.add_argument("--expect-sha", required=True)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    result = {}
    vm_state = Path(os.environ.get("TESTVM_ROOT", str(Path.home() / ".richos-testvm"))) / "run" / args.vm
    payload = (vm_state / "payload").read_text().strip()
    home = payload + "/home"

    def run(*argv, timeout=60):
        return guest(args.vm, shlex.join(argv), timeout)

    def python(script, *argv):
        return json.loads(run("python3", "-c", script, home, *argv))

    def version(app):
        return run("/usr/libexec/PlistBuddy", "-c", "Print :CFBundleShortVersionString",
                   app + "/Contents/Info.plist")

    def windows(pid):
        return int(run("osascript", "-e", 'tell application "System Events" to count windows of '
                       f"(first process whose unix id is {pid})"))

    try:
        bundles = run("find", payload, "-maxdepth", "1", "-name", "*.app", "-type", "d").splitlines()
        if len(bundles) != 1 or version(bundles[0]) != NIGHTLY:
            raise ValueError("The guest must start with the official nightly 115 bundle")
        installed = bundles[0]
        result["previous_version"] = NIGHTLY
        quit_app(args.vm)
        result["fixture"] = python(SEED)
        staged = payload + "/candidate"
        run("mkdir", "-p", staged + "/app", staged + "/engine")
        for source, name in [(args.next_bundle, "app.zip"), (args.next_engine, "engine.tar.gz")]:
            subprocess.run([str(HERE / "guest.sh"), args.vm, "--push", str(source), staged + "/" + name],
                           check=True, timeout=300)
        run("ditto", "-x", "-k", staged + "/app.zip", staged + "/app", timeout=180)
        run("tar", "-C", staged + "/engine", "--strip-components", "1", "-xzf", staged + "/engine.tar.gz",
            timeout=180)
        candidate, = run("find", staged + "/app", "-maxdepth", "1", "-name", "*.app", "-type", "d").splitlines()
        run("codesign", "--verify", "--strict", candidate, timeout=120)
        run("mv", installed, staged + "/previous.app")
        run("mv", candidate, installed)
        run("mv", payload + "/engine", staged + "/previous-engine")
        run("mv", staged + "/engine", payload + "/engine")
        result["candidate_version"] = version(installed)
        if result["candidate_version"] == NIGHTLY:
            raise ValueError("The candidate bundle must carry a different version from nightly 115")
        baseline = python(SNAPSHOT, "")
        result["baseline"] = baseline
        started = relaunch(args.vm, installed)
        result["candidate"] = started
        pid = str(started["pid"])
        window_deadline = time.monotonic() + 60
        while not (initial_windows := windows(pid)):
            if time.monotonic() >= window_deadline:
                raise ValueError("The updated app did not open a window")
            time.sleep(1)
        result["initial_windows"] = initial_windows
        began = time.monotonic()
        polls = set()
        while True:
            sample = python(SNAPSHOT, pid)
            elapsed = time.monotonic() - began
            if sample["watcher_alive"] and sample["poll"] is not None:
                polls.add(sample["poll"])
            result.update(seconds=round(elapsed, 1), last_sample=sample, distinct_watcher_polls=len(polls))
            if sample["records"] != baseline["records"] or sample["live_bytes"]:
                raise ValueError("Opening the candidate staged Stu input or started Claude activity")
            if not sample["app_alive"]:
                raise ValueError("The updated app exited during the observation")
            if elapsed >= IDLE_SECONDS:
                break
            time.sleep(min(5, IDLE_SECONDS - elapsed))
        if not sample["watcher_alive"] or len(polls) < 3 or sample["raw_offset"] != result["fixture"]["raw_bytes"]:
            raise ValueError("The app watcher did not keep polling from the end of the legacy file")
        result["final_windows"] = windows(pid)
        if not result["final_windows"]:
            raise ValueError("The updated app window closed during the observation")
        log = run("cat", started["log"])
        (args.out / "candidate.log").write_text(log + "\n")
        built = re.search(r"this app: built from (\S+)", log)
        if not built or not (built[1].startswith(args.expect_sha) or args.expect_sha.startswith(built[1])):
            raise ValueError("The app log does not name the candidate source commit")
        result["verdict"] = "PASS"
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        result.update(verdict="FAIL", reason=str(error))
    finally:
        (args.out / "codex-legacy-upgrade.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if result["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
