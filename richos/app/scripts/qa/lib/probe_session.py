"""Deadline and process ownership for the context and effort stream probes."""
from contextlib import contextmanager, suppress
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import time
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "engine/scripts/lib"))
import proc_tree


class ProbeError(RuntimeError):
    pass


def remaining(deadline):
    return max(0, deadline - time.monotonic())


class ProbeSession:
    def __init__(self, child, deadline):
        self.child = child
        self.deadline = deadline
        self.pending = bytearray()
        self.eof = False

    def send(self, frame, deadline):
        fd = self.child.stdin.fileno()
        output = self.child.stdout.fileno()
        payload = memoryview((json.dumps(frame) + "\n").encode())
        while payload:
            left = remaining(deadline)
            if not left:
                raise ProbeError("provider input did not accept a frame before the deadline")
            readable, writable, _ = select.select([] if self.eof else [output], [fd], [], left)
            if readable:
                self.read_chunk(output)
            if not writable:
                continue
            try:
                payload = payload[os.write(fd, payload):]
            except BlockingIOError:
                continue

    def read_chunk(self, fd):
        chunk = os.read(fd, 65536)
        self.pending.extend(chunk)
        self.eof = not chunk

    def frames(self, deadline):
        fd = self.child.stdout.fileno()
        while remaining(deadline):
            end = self.pending.find(b"\n")
            if end >= 0 or self.eof:
                line = self.pending[:end + 1] if end >= 0 else self.pending[:]
                del self.pending[:len(line)]
                if not line:
                    return
                try:
                    frame = json.loads(line)
                except (ValueError, UnicodeError):
                    continue
                if isinstance(frame, dict):
                    yield frame
            else:
                if not select.select([fd], [], [], remaining(deadline))[0]:
                    return
                self.read_chunk(fd)

    def start_turn(self, text):
        self.send({"type": "user", "uuid": str(uuid.uuid4()),
                   "message": {"role": "user", "content": text}}, self.deadline)
        return self.frames(self.deadline)

    def control(self, request):
        deadline = min(self.deadline, time.monotonic() + 60)
        request_id = str(uuid.uuid4())
        if remaining(deadline):
            self.send({"type": "control_request", "request_id": request_id, "request": request}, deadline)
            for frame in self.frames(deadline):
                if frame.get("type") == "control_response":
                    response = frame.get("response") or {}
                    if response.get("request_id") == request_id:
                        return response
        return None


@contextmanager
def provider(args, cwd, deadline):
    scope = uuid.uuid4().hex
    env = dict(os.environ)
    env[proc_tree.SCOPE_ENV] = env.get(proc_tree.SCOPE_ENV, "") + ":" + scope
    # Use the probe's root so the tracker can recover the tagged tree even if
    # the provider leader exits before its first process snapshot.
    tracker = proc_tree.TrackedTree(os.getpid(), scope)
    child = subprocess.Popen(args, cwd=cwd, env=env, start_new_session=True,
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    session = ProbeSession(child, deadline)
    handlers = {}

    def interrupted(signum, _frame):
        raise SystemExit(128 + signum)

    try:
        for sig in (signal.SIGTERM, signal.SIGHUP):
            handlers[sig] = signal.signal(sig, interrupted)
        tracker.refresh(tags=True)
        os.set_blocking(child.stdin.fileno(), False)
        yield session
    finally:
        try:
            with suppress(OSError):
                child.stdin.close()
            # Give a healthy provider a chance to flush its transcript on EOF.
            # Even when the leader exits, sweep its owned descendants.
            with suppress(subprocess.TimeoutExpired):
                child.wait(timeout=min(0.25, remaining(session.deadline)))
            left = proc_tree.finish_scope(child, tracker, grace=min(0.25, remaining(session.deadline)))
            if left or child.poll() is None:
                raise ProbeError(f"provider cleanup left owned processes: {left or [child.pid]}")
        finally:
            child.stdout.close()
            for sig, handler in handlers.items():
                signal.signal(sig, handler)
