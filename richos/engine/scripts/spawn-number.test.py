"""spawn.sh numbers a teammate's first name by the active teammates of that first name (CEO,
2026-10-21, richos-hq docs/plans/follow-ups.md): the lowest free number among the active ones,
none for the first. Driven through the real command (`spawn.sh <name> --next-name`) against a
registry of this test's own (RICHOS_WORKSPACES_DIR in a temporary folder)."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parent
SPAWN = SCRIPTS / "spawn.sh"
NAME_RULE = SCRIPTS / "lib" / "teammate-name.sh"


class Numbering(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="spawn-number.")
        self.registry = Path(self.tmp.name) / "workspaces"
        (self.registry / "agents").mkdir(parents=True)
        self.addCleanup(self.tmp.cleanup)

    def teammate(self, name, finished=False):
        """One registry record. No session, so finished_state reads only its own facts: running
        until an end is recorded."""
        key = "fixture-session--" + name
        record = {"key": key, "name": name, "session_id": "", "agent_id": "", "workspaces": [],
                  "end": {"signal": "SubagentStop", "at": 1} if finished else None,
                  "handed_in": None, "pause": None, "disposition": None, "orphan": False}
        (self.registry / "agents" / (key + ".json")).write_text(json.dumps(record))

    def next_name(self, name, *extra):
        env = {**os.environ, "RICHOS_WORKSPACES_DIR": str(self.registry), "PYTHONDONTWRITEBYTECODE": "1"}
        out = subprocess.run(["bash", str(SPAWN), name, "--next-name", *extra], env=env,
                             capture_output=True, text=True, timeout=60)
        self.assertEqual(out.returncode, 0, out.stderr)
        return out.stdout.strip()

    def test_with_echo_and_echo2_active_the_next_is_echo3(self):
        self.teammate("echo-opus-rename4")
        self.teammate("echo2-fable-launch1")
        self.assertEqual(self.next_name("echo-opus-engupd1"), "echo3-opus-engupd1")

    def test_with_only_echo2_active_the_next_is_echo(self):
        self.teammate("echo2-fable-launch1")
        self.assertEqual(self.next_name("echo-opus-engupd1"), "echo-opus-engupd1")

    def test_the_first_has_no_number(self):
        self.teammate("zach-opus-infra1")
        self.assertEqual(self.next_name("echo-opus-x1"), "echo-opus-x1")

    def test_a_finished_teammate_frees_its_number(self):
        self.teammate("echo-opus-old1", finished=True)
        self.teammate("echo2-opus-old2")
        self.assertEqual(self.next_name("echo-opus-x1"), "echo-opus-x1")

    def test_a_number_given_by_hand_is_replaced(self):
        self.teammate("echo-opus-rename4")
        self.assertEqual(self.next_name("echo4-opus-names1"), "echo2-opus-names1")

    def test_only_the_rest_of_the_name_takes_the_type_as_first_name(self):
        self.teammate("echo-opus-rename4")
        self.assertEqual(self.next_name("opus-names1", "--type", "echo"), "echo2-opus-names1")

    def test_a_taken_name_counts_as_active(self):
        """The app adds its own receipts still being prepared, which the registry may not hold yet."""
        self.teammate("mark-opus-aaaaaaaaaaaa")
        self.assertEqual(self.next_name("mark-opus-bbbbbbbbbbbb", "--taken", "mark2-opus-cccccccccccc"),
                         "mark3-opus-bbbbbbbbbbbb")

    def test_a_named_registry_counts_its_running_teammates_too(self):
        """The app keeps one registry per conversation and names the company's others."""
        other = Path(self.tmp.name) / "other-conversation"
        (other / "agents").mkdir(parents=True)
        key = "fixture-session--mark-opus-aaaaaaaaaaaa"
        (other / "agents" / (key + ".json")).write_text(json.dumps(
            {"key": key, "name": "mark-opus-aaaaaaaaaaaa", "session_id": "", "agent_id": "", "workspaces": [],
             "end": None, "handed_in": None, "pause": None, "disposition": None, "orphan": False}))
        self.assertEqual(self.next_name("mark-opus-bbbbbbbbbbbb"), "mark-opus-bbbbbbbbbbbb")
        self.assertEqual(self.next_name("mark-opus-bbbbbbbbbbbb", "--registry", str(other)), "mark2-opus-bbbbbbbbbbbb")

    def test_the_naming_guard_accepts_a_numbered_first_name_lowest_or_not(self):
        """guard-worktree-isolation.sh clause 2c applies this shared shape rule; it checks the
        shape only, so echo2 and a number that is not the lowest free one (echo5) both pass."""
        for name in ("echo2-opus-names1", "echo5-opus-names1"):
            out = subprocess.run(["bash", "-c", '. "$1"; teammate_name_check "$2" "fable opus sonnet haiku"',
                                  "rule", str(NAME_RULE), name], capture_output=True, text=True)
            self.assertEqual(out.returncode, 0, out.stdout)


if __name__ == "__main__":
    unittest.main()
