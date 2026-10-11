"""No hook skips itself because a transcript is large (CEO 2026-10-10).

The four readers that used to refuse a long session's transcript each get a
synthetic transcript over their old cap, whose LAST turn carries the case the
hook acts on, and must give the same answer they give on a small transcript:

  turn-manifest.py          read_turn        (old cap 48 MB, said UNAVAILABLE)
  guard-idle-land.py        read_turn        (old cap 48 MB, returned None)
  guard-unresolved-claims   read_transcript  (old cap 48 MB, returned nothing)
  lib/failure-type.py       read_turn        (old cap 256 MB, "over this gate's read cap")

guard-unresolved-claims keeps its session-wide grounding: a name seen only in an
early turn of the large transcript is still grounded.
"""
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
LIB = os.path.join(os.path.dirname(HERE), "lib")
MB = 1024 * 1024
TURN = "11111111-turn-last"
OLD = "00000000-turn-old"
EARLY_NAME = "zach-opus-earlyname1"
DEEP_NAME = "mark-sonnet-deepname1"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


TM = load("turn_manifest", os.path.join(HERE, "turn-manifest.py"))
IDLE = load("guard_idle_land", os.path.join(HERE, "guard-idle-land.py"))
CLAIMS = load("guard_unresolved_claims", os.path.join(HERE, "guard-unresolved-claims.py"))
FT = load("failure_type", os.path.join(LIB, "failure-type.py"))


def rec(**kw):
    return json.dumps(kw) + "\n"


def last_turn(fh):
    fh.write(rec(type="user", promptId=TURN, timestamp="2026-10-10T10:00:00Z",
                 cwd="/tmp/x", message={"role": "user", "content": "do the thing"}))
    fh.write(rec(type="assistant", message={"role": "assistant", "content": [
        {"type": "tool_use", "id": "toolu_last1", "name": "Bash",
         "input": {"command": "git merge feature-branch"}}]}))
    fh.write(rec(type="user", message={"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "toolu_last1", "content": "Merge made"}]}))


def build(path, target_bytes, early_name_in_first_turn=True):
    """A transcript of at least target_bytes: an early turn naming EARLY_NAME,
    a mass of older turns, then the last turn."""
    filler = rec(type="assistant", promptId=OLD, message={
        "role": "assistant", "content": [{"type": "text", "text": "x" * (MB - 200)}]})
    with open(path, "w") as fh:
        fh.write(rec(type="user", promptId=OLD, message={"role": "user", "content": "start"}))
        if early_name_in_first_turn:
            fh.write(rec(type="assistant", message={"role": "assistant", "content": [
                {"type": "tool_use", "id": "toolu_early", "name": "Agent",
                 "input": {"name": EARLY_NAME}}]}))
        written = fh.tell()
        while written < target_bytes:
            fh.write(filler)
            written += len(filler)
        last_turn(fh)


class Big(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dir = tempfile.mkdtemp(prefix="large-transcript.", dir=os.environ.get("TMPDIR"))
        cls.small = os.path.join(cls.dir, "small.jsonl")
        build(cls.small, 1 * MB)
        cls.big = os.path.join(cls.dir, "big48.jsonl")
        build(cls.big, 49 * MB)
        assert os.path.getsize(cls.big) > 48 * MB

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.dir, ignore_errors=True)

    # --- turn-manifest ----------------------------------------------------
    def test_turn_manifest_reads_a_transcript_over_48_mb(self):
        small = TM.read_turn(self.small, TURN)
        big = TM.read_turn(self.big, TURN)
        self.assertIsNone(big[3], "UNAVAILABLE on a large transcript: %s" % (big[3],))
        self.assertEqual(big[0], [("toolu_last1", "Bash")])
        self.assertEqual(TM.render(big[0], big[1]), TM.render(small[0], small[1]))

    # --- guard-idle-land --------------------------------------------------
    def test_idle_land_reads_a_transcript_over_48_mb(self):
        small = IDLE.read_turn(self.small, TURN)
        big = IDLE.read_turn(self.big, TURN)
        self.assertIsNotNone(big, "the gate stood down on a large transcript")
        self.assertEqual(big["bash"], ["git merge feature-branch"])
        self.assertEqual(big["tools"], ["Bash"])
        self.assertEqual(big["tools"], small["tools"])
        self.assertEqual(big["bash"], small["bash"])
        self.assertEqual(big["started_at"], small["started_at"])

    # --- guard-unresolved-claims ------------------------------------------
    def test_claims_grounding_survives_a_transcript_over_48_mb(self):
        needles = {EARLY_NAME, DEEP_NAME, "Agent-"}
        seen, tools = CLAIMS.read_transcript(self.big, TURN, needles)
        self.assertIn(EARLY_NAME, seen, "a name seen in an early turn must stay grounded")
        self.assertNotIn(DEEP_NAME, seen)
        self.assertEqual(tools, ["Bash"])
        small_seen, small_tools = CLAIMS.read_transcript(self.small, TURN, needles)
        self.assertEqual(tools, small_tools)
        self.assertEqual([n in seen for n in sorted(needles)],
                         [n in small_seen for n in sorted(needles)])

    def test_claims_unregistered_needle_is_loud(self):
        seen, _ = CLAIMS.read_transcript(self.small, TURN, {EARLY_NAME})
        with self.assertRaises(KeyError):
            "never-registered" in seen

    # --- the start of the turn is found from the end ------------------------
    def test_turn_start_is_found_from_the_end_even_when_the_turn_is_long(self):
        path = os.path.join(self.dir, "longturn.jsonl")
        with open(path, "w") as fh:
            fh.write(rec(type="user", promptId=OLD, message={"role": "user", "content": "a"}))
            fh.write(rec(type="user", promptId=TURN, timestamp="2026-10-10T10:00:00Z",
                         message={"role": "user", "content": "go"}))
            for i in range(12):  # a 12 MB turn, wider than the first 4 MB window
                fh.write(rec(type="assistant", message={"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t%d" % i, "name": "Read", "input": {"p": "y" * MB}}]}))
        calls, _results, _n, err = TM.read_turn(path, TURN)
        self.assertIsNone(err)
        self.assertEqual(len(calls), 12)


class BigFailureType(unittest.TestCase):
    def test_failure_type_reads_a_transcript_over_256_mb(self):
        d = tempfile.mkdtemp(prefix="large-transcript-ft.", dir=os.environ.get("TMPDIR"))
        self.addCleanup(shutil.rmtree, d, True)
        path = os.path.join(d, "big256.jsonl")
        build(path, 257 * MB, early_name_in_first_turn=False)
        self.assertGreater(os.path.getsize(path), 256 * MB)
        records, err = FT.read_turn(path, TURN)
        self.assertIsNone(err, "refused a large transcript: %s" % (err,))
        self.assertEqual([r.get("type") for r in records], ["user", "assistant", "user"])


if __name__ == "__main__":
    unittest.main()
