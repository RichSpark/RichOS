#!/usr/bin/env bash
# Exercise the actual smoke executable without credentials or model requests.
# The fixture speaks only the control protocol. The gate cases retain a real
# child process across a hold, then verify its exact handoff or stop refusal.
# run-tests: inputs richos/app/scripts/claude-quota.test.sh richos/app/Cargo.toml richos/app/Cargo.lock richos/app/crates/richos-core
# run-tests: covers richos/app/crates/richos-core/examples/claude_quota.rs
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$DIR/.."
python3 - <<'PY'
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time

# No clock of this suite's own around real work (audit R13, 2026-09-29): the build waits on
# Cargo's lock behind whatever else builds, the smoke executable and the gate are real process
# starts, and 240 s, 30 s and 5 s were verdicts on how busy the Mac was. Every wait below ends
# on a fact or on the process ending; a hang is caught by the enclosing runner's per-suite
# deadline, which names this suite.
build = subprocess.run(
    ["cargo", "build", "--quiet", "-p", "richos-core", "--example", "claude_quota", "--message-format=json"],
    check=True, capture_output=True, text=True,
)
executables = [row["executable"] for line in build.stdout.splitlines()
               if (row := json.loads(line)).get("reason") == "compiler-artifact"
               and row.get("target", {}).get("name") == "claude_quota" and row.get("executable")]
assert len(executables) == 1, "Cargo did not report the smoke executable"
executable = executables[0]

with tempfile.TemporaryDirectory(prefix="claude-quota-proof-") as temporary:
    root = Path(temporary)
    runs = root / "runs"
    runs.mkdir()
    log = root / "requests"
    # The fixture is a shell launcher for its Python body: a "#!" line cannot name an
    # interpreter whose path has a space in it (the engine's own Python lives under
    # "Application Support"), and the kernel then answers ENOENT for the fixture itself.
    fixture = root / "claude-fixture"
    # A shell wrapper, never `#!<python>`: the app's own Python lives under "Application Support",
    # and a shebang path with a space cannot be executed (ENOENT, "usage reader could not start").
    fixture_code = root / "claude-fixture.py"
    fixture.write_text("#!/bin/sh\nexec " + shlex.quote(sys.executable) + " "
                       + shlex.quote(str(fixture_code)) + ' "$@"\n')
    fixture_code.write_text('''import json, os, sys
from pathlib import Path
if os.environ.get("QUOTA_PROOF_EOF") == "1":
    sys.exit(0)
assert "--no-session-persistence" in sys.argv
assert sys.argv[sys.argv.index("--tools") + 1] == ""
assert "--strict-mcp-config" in sys.argv
for line in sys.stdin:
    request = json.loads(line)
    assert request["type"] == "control_request", "a model request was sent"
    kind = request["request"]["subtype"]
    assert kind in ("initialize", "get_usage")
    with open(os.environ["QUOTA_PROOF_LOG"], "a") as log:
        log.write(kind + "\\n")
    response = {} if kind == "initialize" else {
        "rate_limits_available": True,
        "rate_limits": {"five_hour": {"utilization": 37, "resets_at": "2099-01-01T00:00:00Z"}},
        "account_email": "private-fixture@example.invalid"
    }
    print(json.dumps({"type": "control_response", "response": {
        "subtype": "success", "request_id": request["request_id"], "response": response
    }}), flush=True)
''')
    fixture.chmod(0o700)
    env = {**os.environ, "TMPDIR": str(runs), "QUOTA_PROOF_LOG": str(log)}

    def invoke(*args, extra=None):
        return subprocess.run([executable, *map(str, args)], env={**env, **(extra or {})},
                              capture_output=True, text=True)

    for args in [(), ("--live",), ("--invalid",)]:
        result = invoke(*args)
        assert result.returncode != 0 and not result.stdout, "invalid CLI input claimed success"
    print("PASS invalid and missing arguments are rejected")

    result = invoke("--live", fixture)
    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "Quota state: Fresh; recognized windows: 1; no model turn requested."
    assert log.read_text().splitlines() == ["initialize", "get_usage"]
    assert "private-fixture" not in result.stdout + result.stderr
    assert not list(runs.iterdir()), "successful smoke left temporary data"
    print("PASS live mode uses control-only reads, prints a bounded summary and removes scratch")

    result = invoke("--live", fixture, extra={"QUOTA_PROOF_EOF": "1"})
    assert result.returncode != 0 and "No subscription windows returned" in result.stderr
    assert not list(runs.iterdir()), "failed smoke left temporary data"
    print("PASS provider EOF fails honestly and removes scratch")

    state = root / "engine-state"
    state.mkdir()
    scope = root / "scope.json"
    observed = root / "canonical-callback.json"
    canonical = root / "canonical.py"
    canonical.write_text("import sys\nfrom pathlib import Path\nPath(sys.argv[1]).write_bytes(sys.stdin.buffer.read())\n")
    payload = json.dumps({"agent_id": "same-agent", "tool_name": "Read", "tool_input": {"path": "a file"}})

    def publish(path, value):
        staging = path.with_suffix(".pending")
        staging.write_text(json.dumps(value))
        staging.replace(path)

    def start_waiter():
        publish(scope, {"version": 1, "actions_allowed": True,
                        "binding": {"entity_id": "company", "thread_id": "thread", "session_id": "session"}})
        now = time.time_ns() // 1_000_000
        # Only a real above-threshold window holds work. Missing telemetry does not.
        publish(state / "claude-quota.json", {
            "state": "fresh", "windows": [{"id": "five_hour", "label": "Five-hour",
                "usedPercent": 95, "resetsAt": now + 3_600_000, "durationMs": 18_000_000}],
            "checkedAt": now, "retryAt": None, "nextCheckAt": None,
            "refreshIntervalMs": 300000, "message": None,
            "policy": {"enabled": True, "pausePercent": 93},
            "admission": {"state": "held", "resetsAt": now + 3_600_000},
        })
        child = subprocess.Popen(
            [executable, "--claude-quota-gate", sys.executable, str(canonical), str(observed)],
            env={**env, "RICHOS_APP_STATE": str(state), "RICHOS_APP_SCOPE": str(scope)},
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        child.stdin.write(payload)
        child.stdin.close()
        child.stdin = None
        try:
            while True:
                if any(not p.name.endswith(".released.json") for p in (state / "quota-waits").glob("*.json")):
                    assert child.poll() is None and not observed.exists()
                    return child
                if child.poll() is not None:
                    raise AssertionError("hook ended before it entered the observed wait: "
                                         + child.communicate()[1])
                time.sleep(0.01)
        except BaseException:
            child.kill()
            child.communicate()
            raise

    child = start_waiter()
    try:
        publish(state / "claude-quota.json", {
            "state": "unavailable", "windows": [], "checkedAt": None, "retryAt": None,
            "nextCheckAt": None, "refreshIntervalMs": 300000, "message": None,
            "policy": {"enabled": False, "pausePercent": 93}, "admission": {"state": "disabled"},
        })
        stdout, stderr = child.communicate()
        assert child.returncode == 0, stderr
        assert observed.read_text() == payload, "callback changed during the retained wait"
    finally:
        if child.poll() is None:
            child.kill()
            child.communicate()
    print("PASS gate retains the process and forwards the exact callback after release")

    observed.unlink()
    (state / "claude-quota.json").unlink()
    child = start_waiter()
    try:
        publish(scope, {"version": 1, "actions_allowed": False, "background_work_allowed": False})
        stdout, stderr = child.communicate()
        assert child.returncode == 2 and "stopped" in stderr
        assert not observed.exists(), "stopped work reached the canonical hook"
    finally:
        if child.poll() is None:
            child.kill()
            child.communicate()
    print("PASS revoked work exits without forwarding or claiming completion")
PY
