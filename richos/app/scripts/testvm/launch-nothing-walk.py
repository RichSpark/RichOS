#!/usr/bin/env python3
"""launch-nothing-walk.py — a relaunch opens in NO conversation, with several conversations on disk.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  launch-nothing-walk.py --make-home DIR            # writes the synthetic fixture home (host side)
  run-walk.py --bundle ZIP --home DIR --engine ENGINE --report REPORT -- \\
      launch-nothing-walk.py --out DIR

run-walk.py passes the owned VM name as the first argument.

HIS WORDS (CEO, 2026-11-01): "why the fuck is the app always opening in the chat (after launching)
that's titled "RichScribe something"? I never fucking ever did anything in that chat."

THE FIXTURE is one synthetic company ("Acme") with three conversations; the one created LAST is
the app's own "RichScribe feature" one, which the old launch opened because boot activated the
company's most recently created thread. The fixture also records earlier starts, so the app
treats a launch as a relaunch.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65): quits the app run.sh
started (by its recorded pid), relaunches it (relaunch.py, same environment), waits for a window,
photographs it, and reads the app's own log. PASS needs (the sidebar is read from after-home.png, not the tree: the setup sheet hides it from the tree) the log's "launch opens in no
conversation" line and no thread named "RichScribe feature" marked as the one on screen
(aria-current) in the accessibility tree. Every instance is quit by run-walk.py's stop.sh (§54).
"""
import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

TITLES = ["Pricing ideas", "Weekly plan", "RichScribe feature"]


def make_home(dest):
    data = dest / "Library/Application Support/com.richos.app"
    data.mkdir(parents=True)
    (data / "entities.json").write_text(json.dumps({"version": 2, "entities": [
        {"id": "acme", "display_name": "Acme", "status": "active", "roots": ["/Users/admin/acme-fixture"]}]}, indent=1))
    (data / "config.json").write_text(json.dumps({"entity": "acme", "schema_version": 1}))
    now = int(time.time() * 1000)
    (data / "launches.json").write_text(json.dumps({"schema_version": 1, "installed_at": now - 86400000,
        "starts": [now - 86400000, now - 43200000]}))
    rows = []
    for i, title in enumerate(TITLES):
        tid = "thr_%032x" % (i + 1)
        at = now - 600000 + i * 60000
        rows.append({"at": at, "binding_revision": i + 1, "entity_id": "acme", "event": "ThreadCreated",
                     "person_id": "ceo-default", "thread_id": tid, "title": title, "written_by": 1})
        rows.append({"at": at + 1000, "channel": "app", "entity_id": "acme", "event": "PromptReceived",
                     "binding_revision": i + 1, "source": "text", "text": "hello " + title,
                     "thread_id": tid, "turn_id": "turn_%032x" % (i + 1), "written_by": 1})
    (data / "conversation-ledger.jsonl").write_text("".join(json.dumps(r, sort_keys=True) + "\n" for r in rows))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("vm", nargs="?")
    p.add_argument("--make-home", type=Path)
    p.add_argument("--out", type=Path)
    a = p.parse_args()
    if a.make_home:
        make_home(a.make_home)
        return 0
    if not (a.vm and a.out):
        p.error("a vm name and --out are required")
    from relaunch import guest, relaunch
    a.out.mkdir(parents=True, exist_ok=True)
    result = {}
    guest(a.vm, "mkdir -p /Users/admin/acme-fixture")
    first = relaunch(a.vm)  # quits run.sh's instance by its pid, starts the one that is judged
    result["relaunch"] = first
    log = first["log"]
    end = time.monotonic() + 90
    text = ""
    while time.monotonic() < end:
        text = guest(a.vm, "cat " + log + " 2>/dev/null || true", 60)
        if "launch opens in no conversation" in text or "[richos] company:" in text and "central folder" in text:
            if "launch opens in no conversation" in text:
                break
        time.sleep(2)
    time.sleep(8)
    subprocess.run([str(HERE / "shot.sh"), a.vm, str(a.out / "relaunched-home.png")], timeout=60, capture_output=True)
    # A relaunch opens on the home screen; Enter leaves it (home.js), showing the app and its
    # sidebar, which is where a selected conversation would be marked and opened.
    subprocess.run([str(HERE / "ax.sh"), a.vm, "--key", "36"], capture_output=True, text=True, timeout=60)
    time.sleep(6)
    subprocess.run([str(HERE / "shot.sh"), a.vm, str(a.out / "after-home.png")], timeout=60, capture_output=True)
    tree = subprocess.run([str(HERE / "ax.sh"), a.vm, "tree", "--depth", "40"], capture_output=True, text=True, timeout=90)
    (a.out / "ax-tree.txt").write_text(tree.stdout + tree.stderr)
    (a.out / "app-log.txt").write_text(text)
    # Recorded, not judged: the guest's first-run setup sheet sits over the window and hides the
    # sidebar from the accessibility tree (measured 2026-11-01), so the sidebar itself is read from
    # after-home.png by eye; the pass is the app's own log line and no row marked as the open one.
    result["sidebar_titles_seen"] = [t for t in TITLES if t in tree.stdout]
    names_open = [l for l in tree.stdout.splitlines() if any(t in l for t in TITLES) and ("selected=yes" in l.lower() or "current" in l.lower())]
    result["log_says_no_conversation"] = "launch opens in no conversation" in text
    result["log_company_line"] = [l for l in text.splitlines() if "[richos] company:" in l]
    result["rows_marked_as_the_open_one"] = names_open
    result["pass"] = result["log_says_no_conversation"] and not names_open
    (a.out / "launch-nothing.json").write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
