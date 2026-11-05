"""Immutable evidence for completed nightly gates, including interrupted builds.

Only build candidates reconcile gates. Release/stable and the release-path smoke
check retain their fresh-run requirements. A reuse is never a new passing run.
"""
import json
from pathlib import Path
import time
import subprocess

import proof_evidence as evidence


class GateEvidence:
    def __init__(self, root, run_id, identity, announce):
        self.root = Path(root)
        self.run_id = run_id
        self.identity = identity
        self.announce = announce
        self.selected = {}

    def latest(self, name, identity):
        """A newer red or damaged record must not expose an older green result."""
        try:
            paths = sorted(self.root.glob("*/" + name.replace("/", "-") + ".json"),
                           key=lambda p: p.stat().st_mtime_ns, reverse=True)
        except OSError:
            return None  # A rotated record is missing evidence, never a pass.
        for path in paths:
            try:
                record = json.loads(path.read_text())
                if evidence.file_digest(path) != path.with_suffix(".sha256").read_text().strip():
                    return None
                if record["gate"] != name or record["schema"] != 1:
                    return None
                if record["identity"] == identity:
                    return path, record
            except (OSError, ValueError, KeyError, TypeError):
                return None
        return None

    def reusable(self, name, identity):
        latest = self.latest(name, identity)
        if not latest or latest[1].get("status") != "passed":
            return None
        return self.intact(*latest)

    def proven_pass(self, name, path):
        """The record at `path` when it is an intact actual pass of `name`, else None.

        What a later reader (`stable --from-nightly`) calls to check a `REUSED` line a build
        wrote: the same integrity checks as `reusable`, with the identity left to the caller
        because the line names the record, not the inputs.
        """
        path = Path(path)
        try:
            if evidence.file_digest(path) != path.with_suffix(".sha256").read_text().strip():
                return None
            record = json.loads(path.read_text())
            if record["gate"] != name or record["schema"] != 1 or record.get("status") != "passed":
                return None
        except (OSError, ValueError, KeyError, TypeError):
            return None
        found = self.intact(path, record)
        return found[1] if found else None

    def intact(self, path, record):
        try:
            if evidence.file_digest(path.with_suffix(".log")) != record["log_sha256"]:
                return None
            if evidence.digest(record.get("artifacts")) != record["artifacts_sha256"]:
                return None
        except (OSError, ValueError, KeyError, TypeError):
            return None
        return path, record

    def unresolved(self, name, identity):
        latest = self.latest(name, identity)
        return latest is not None and latest[1].get("status") != "passed"

    def reuse(self, name, identity):
        found = self.reusable(name, identity)
        if not found:
            return None
        path, record = found
        self.selected[name] = {"status": "reused", "record": str(path),
                               "record_sha256": evidence.file_digest(path),
                               "run_id": record["run_id"], "identity": identity}
        self.announce(f"REUSED {name}: actual pass in {record['run_id']} ({path}); "
                      "source, dependencies and execution inputs match")
        return record.get("artifacts")

    def record(self, name, identity, status, log, artifacts=None):
        folder = self.root / self.run_id
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / (name.replace("/", "-") + ".json")
        # A run id is unique. Never overwrite evidence from a completed invocation.
        if path.exists():
            raise ValueError(f"nightly gate evidence already exists: {path}")
        path.with_suffix(".log").write_text(log)
        record = {"schema": 1, "gate": name, "identity": identity, "status": status,
                  "run_id": self.run_id, "created_ns": time.time_ns(),
                  "log_sha256": evidence.file_digest(path.with_suffix(".log")),
                  "artifacts": artifacts, "artifacts_sha256": evidence.digest(artifacts)}
        evidence.atomic(path, record)
        path.with_suffix(".sha256").write_text(evidence.file_digest(path) + "\n")
        self.selected[name] = {"status": status, "record": str(path),
                               "record_sha256": evidence.file_digest(path),
                               "run_id": self.run_id, "identity": identity}

    def wrap(self, name, body, identity, read_log, artifacts, stopping=lambda: False):
        def run():
            try:
                body()
            except BaseException as error:
                status = {"GateStopped": "stopped", "GateTimedOut": "timed-out",
                          "GateNotAdmitted": "refused", "CommandCleanupError": "cleanup-failed"}.get(
                              type(error).__name__, "stopped" if stopping() else "failed")
                self.record(name, identity, status, read_log())
                raise
            # Re-read, with a new input snapshot. A pass on changing inputs is
            # still recorded, but cannot certify a later invocation.
            try:
                status = "passed" if self.identity() == identity else "inputs-changed"
            except (OSError, ValueError, subprocess.SubprocessError):
                status = "unqualified"
            result = artifacts()
            if result == {"skipped": True}:
                status = "skipped"
            if result == {"missing": True}:
                status = "unqualified"
            self.record(name, identity, status, read_log(), result)
            if status in ("inputs-changed", "unqualified"):
                raise RuntimeError(f"{name} cannot certify this candidate: {status}; "
                                   "its actual output is preserved, but its inputs or evidence must be resolved")
        return run
