#!/usr/bin/env python3
"""app_management_watch.py: A READ-ONLY WATCHER FOR macOS APP MANAGEMENT NOTICES THAT NAME RICHOS.

On 2026-10-28 macOS showed "'RichOS.app' was prevented from modifying apps on your Mac". The
investigation found no RichOS process touching an app bundle: the kernel had denied two of the
engine's own lease temp-file writes in a burst of App Management checks that also hit unrelated
apps, and six other checks that day were never attributed. The unified log keeps those lines for a
short while only, so by the time anyone looks the evidence is gone. This watcher makes the NEXT
notice come with its own evidence.

WHAT IT RECORDS, one JSON object per line, in <engine-state>/app-management/ledger.jsonl:
  kind "kernel-deny"      a kernel `deny(N) file-write...` line ("System Policy:", the App
                          Management and privacy enforcement, or "Sandbox:"), with the process,
                          its pid and every absolute path the line names
  kind "tcc-context"      tccd's AUTHREQ_CTX for the App Management service
                          (kTCCServiceSystemPolicyAppBundles)
  kind "tcc-attribution"  tccd's AUTHREQ_ATTRIBUTION for that same request (msgID): the
                          responsible, accessing and requesting processes, with binary paths
  kind "tcc-result"       tccd's AUTHREQ_RESULT for that request
Every record carries the log's own timestamp, the process and pid, the message verbatim, and the
absolute paths found in it ("paths").

NOT RECORDED: on a Mac where agents run, nearly every kernel file-write denial is a sandboxed
shell writing /dev/dtracehelper or /dev/tty, several a second while they work. A write to a device
is never an app bundle, so the predicate leaves every " /dev/" line in logd. tccd's requests for
other services reach the watcher only to be paired by request id and dropped; App Management is a
small share of them.

READ-ONLY. It runs `/usr/bin/log stream` with a predicate and writes only its own ledger. It
changes no permission, asks for none and works around nothing: RichOS needs no App Management
permission, and the block is macOS doing its job.

NEARLY NO CPU WHILE IDLE. It blocks reading the log stream's output, and logd evaluates the
predicate, so the watcher wakes only for a line that matches. There is no polling loop. Its end
with the app is a block too: a kqueue NOTE_EXIT on the app's pid (RICHOS_WATCH_HOST) wakes it
once, when the app is gone. If the stream itself ends (logd restarted), it starts it again after a
pause that grows to a minute while it keeps failing.

THE LEDGER IS BOUNDED: past MAX_LEDGER_BYTES it is moved to ledger.1.jsonl (one generation kept).

Usage:
  app_management_watch.py --state <engine-state dir>          the app's child, from launch to quit
  app_management_watch.py --state <dir> --input <file|fifo>   read ndjson lines from a file instead
                                                              (APP_MANAGEMENT_WATCH_INPUT too; tests)
  app_management_watch.py --predicate                         print the log predicate
  app_management_watch.py --check-predicate                   ask `log` whether it accepts it (2 s)
Suite: scripts/lib/app_management_watch.test.py.
"""
import collections
import json
import os
import re
import select
import signal
import subprocess
import sys
import threading
import time

LOG = "/usr/bin/log"
HOST_ENV = "RICHOS_WATCH_HOST"
INPUT_ENV = "APP_MANAGEMENT_WATCH_INPUT"
LEDGER_DIR = "app-management"
LEDGER = "ledger.jsonl"
MAX_LEDGER_BYTES = 5 * 1024 * 1024
APP_BUNDLES = "SystemPolicyAppBundles"

# Evaluated by logd: the watcher is handed only these lines.
PREDICATE = ('(process == "kernel" AND eventMessage CONTAINS "deny(" AND eventMessage CONTAINS "file-write"'
             ' AND NOT eventMessage CONTAINS " /dev/")'
             ' OR (process == "tccd" AND (eventMessage CONTAINS "AUTHREQ_CTX"'
             ' OR eventMessage CONTAINS "AUTHREQ_ATTRIBUTION" OR eventMessage CONTAINS "AUTHREQ_RESULT"))')

KERNEL_DENY = re.compile(r"deny\(\d+\)\s+file-write")
MSG_ID = re.compile(r"msgID=([0-9]+(?:\.[0-9]+)?)")
# A kernel denial names one path, the rest of its line (it may hold spaces: "Application Support").
KERNEL_PATH = re.compile(r"deny\(\d+\)\s+\S+\s+(/.*?)\s*$")
# tccd names paths as key=value inside {...}, ended by a comma or a brace.
TCC_PATH = re.compile(r"(?:binary_path|responsible_path|path)=(/[^,}]*)")
REMEMBER = 512   # request ids held for pairing; the oldest is forgotten first


def paths_in(text):
    """Every absolute path the line names, whole (spaces included)."""
    text = text or ""
    found = [m.group(1) for m in KERNEL_PATH.finditer(text)] + [m.group(1).strip() for m in TCC_PATH.finditer(text)]
    out = []
    for p in found:
        if len(p) > 1 and p not in out:
            out.append(p)
    return out


class Recorder:
    """Pairs tccd's lines by request id and appends the matching records to the ledger."""

    def __init__(self, ledger):
        self.ledger = ledger
        self.app_bundle_ids = collections.OrderedDict()   # msgID -> True: App Management requests
        self.pending = collections.OrderedDict()          # msgID -> records seen before its context
        os.makedirs(os.path.dirname(ledger), exist_ok=True)

    @staticmethod
    def _remember(table, key, value):
        table[key] = value
        table.move_to_end(key)
        while len(table) > REMEMBER:
            table.popitem(last=False)

    def _write(self, record):
        try:
            if os.path.getsize(self.ledger) > MAX_LEDGER_BYTES:
                os.replace(self.ledger, self.ledger[:-len(".jsonl")] + ".1.jsonl")
        except OSError:
            pass
        with open(self.ledger, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n")

    def event(self, event):
        """One decoded event from `log stream --style ndjson`. Returns the records written."""
        message = str(event.get("eventMessage") or "")
        image = str(event.get("processImagePath") or "")
        process = os.path.basename(image) or str(event.get("process") or "")
        base = {"at": event.get("timestamp"), "process": process, "pid": event.get("processID"),
                "message": message, "paths": paths_in(message)}
        written = []
        if process == "kernel":
            if KERNEL_DENY.search(message):
                record = dict(base, kind="kernel-deny")
                self._write(record)
                written.append(record)
            return written
        if process != "tccd":
            return written
        if "AUTHREQ_CTX" in message:
            kind = "tcc-context"
        elif "AUTHREQ_ATTRIBUTION" in message:
            kind = "tcc-attribution"
        elif "AUTHREQ_RESULT" in message:
            kind = "tcc-result"
        else:
            return written
        m = MSG_ID.search(message)
        msg_id = m.group(1) if m else None
        record = dict(base, kind=kind, msgID=msg_id)
        if APP_BUNDLES in message or (msg_id and msg_id in self.app_bundle_ids):
            if msg_id:
                self._remember(self.app_bundle_ids, msg_id, True)
            self._write(record)
            written.append(record)
            for held in self.pending.pop(msg_id, []) if msg_id else []:
                self._write(held)
                written.append(held)
        elif msg_id:
            held = self.pending.get(msg_id, [])
            held.append(record)
            self._remember(self.pending, msg_id, held[-4:])
        return written

    def line(self, raw):
        raw = raw.strip()
        if not raw.startswith("{"):
            return []
        try:
            event = json.loads(raw)
        except ValueError:
            return []
        return self.event(event) if isinstance(event, dict) else []


def end_with_host(on_exit):
    """Call on_exit once the host (the app) has exited: one blocking kqueue wait, no poll."""
    host = os.environ.pop(HOST_ENV, "").strip()
    if not host.isdigit():
        return False
    pid = int(host)

    def wait():
        try:
            kq = select.kqueue()
            kq.control([select.kevent(pid, filter=select.KQ_FILTER_PROC,
                                      flags=select.KQ_EV_ADD | select.KQ_EV_ONESHOT,
                                      fflags=select.KQ_NOTE_EXIT)], 0, 0)
            kq.control(None, 1, None)   # blocks until the host exits
        except (AttributeError, OSError):
            pass                         # no kqueue here, or the host is already gone
        on_exit()
    threading.Thread(target=wait, name="host-exit", daemon=True).start()
    return True


def stream_command():
    return [LOG, "stream", "--style", "ndjson", "--level", "info", "--predicate", PREDICATE]


def run(state, source=None):
    recorder = Recorder(os.path.join(state, LEDGER_DIR, LEDGER))
    child = {"proc": None}

    def stop(*_):
        proc = child["proc"]
        if proc is not None and proc.poll() is None:
            try:
                proc.terminate()
            except OSError:
                pass
        os._exit(0)

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    end_with_host(stop)

    if source:
        with open(source, encoding="utf-8", errors="replace") as fh:
            for raw in fh:   # on a fifo this blocks until a writer writes
                recorder.line(raw)
        return 0

    pause = 5
    while True:
        try:
            proc = subprocess.Popen(stream_command(), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                    stdin=subprocess.DEVNULL, text=True, errors="replace")
        except OSError as err:
            sys.stderr.write("app-management watch: %s cannot run (%s)\n" % (LOG, err))
            return 1
        child["proc"] = proc
        began = time.monotonic()
        for raw in proc.stdout:   # blocks until logd hands over a matching line
            recorder.line(raw)
        proc.wait()
        # The stream ended (logd restarted, or the log tool failed): start it again after a pause
        # that doubles while it keeps failing at once and resets after a long healthy run.
        pause = 5 if time.monotonic() - began > 600 else min(pause * 2, 60)
        sys.stderr.write("app-management watch: the log stream ended (exit %s); again in %d s\n"
                         % (proc.returncode, pause))
        time.sleep(pause)


def check_predicate():
    """0 when `log` accepts the predicate (it streams for two seconds and exits 0)."""
    try:
        out = subprocess.run([LOG, "stream", "--style", "ndjson", "--predicate", PREDICATE, "--timeout", "2s"],
                             stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True, timeout=60)
    except (OSError, subprocess.SubprocessError) as err:
        sys.stderr.write("log stream could not run: %s\n" % err)
        return 2
    if out.returncode != 0:
        sys.stderr.write(out.stderr)
    return out.returncode


def main(argv):
    if argv == ["--predicate"]:
        print(PREDICATE)
        return 0
    if argv == ["--check-predicate"]:
        return check_predicate()
    state, source = None, None
    i = 0
    while i < len(argv):
        if argv[i] == "--state" and i + 1 < len(argv):
            state, i = argv[i + 1], i + 2
        elif argv[i] == "--input" and i + 1 < len(argv):
            source, i = argv[i + 1], i + 2
        else:
            break
    if not state or i != len(argv):
        sys.stderr.write(__doc__)
        return 2
    return run(state, source or os.environ.get(INPUT_ENV) or None)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
