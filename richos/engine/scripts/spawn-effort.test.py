"""The spawn payload carries the `effort` --effort names, or the effort word of the name, and only a level Claude Code 2.1.292 accepts."""
import importlib.util
from pathlib import Path
import sys
import unittest

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS / "lib"))
spec = importlib.util.spec_from_file_location("spawn_lib", SCRIPTS / "lib" / "spawn.py")
spawn = importlib.util.module_from_spec(spec)
spec.loader.exec_module(spawn)

BASE = ["zach-opus-t1", "--repo", "/tmp/r", "--type", "zach", "--brief", "b"]


class Effort(unittest.TestCase):
    def payload(self, extra):
        args = spawn.parse_args(BASE + extra)
        return spawn.build_payload(args, "Build it", [])

    def test_effort_is_in_the_payload_with_the_option(self):
        self.assertEqual(self.payload(["--effort", "high"])["effort"], "high")

    def test_no_effort_key_without_the_option_for_a_name_with_no_effort_word(self):
        self.assertNotIn("effort", self.payload([]))

    def test_the_names_effort_word_is_the_level_without_the_option(self):
        args = spawn.parse_args(["zach-opus-xhigh-t1"] + BASE[1:])
        self.assertEqual(spawn.build_payload(args, "Build it", [])["effort"], "xhigh")

    def test_the_option_is_kept_beside_another_word_so_the_guard_can_refuse_it(self):
        args = spawn.parse_args(["zach-opus-xhigh-t1"] + BASE[1:] + ["--effort", "low"])
        self.assertEqual(spawn.build_payload(args, "Build it", [])["effort"], "low")

    def test_a_name_given_without_its_role_takes_the_type_as_its_first_name(self):
        for short, full in (("opus-high-t1", "zach-opus-high-t1"), ("opus-t1", "zach-opus-t1")):
            self.assertEqual(spawn.parse_args([short] + BASE[1:])["name"], full)

    def test_only_the_five_levels_are_accepted(self):
        for level in ("low", "medium", "high", "xhigh", "max"):
            self.assertEqual(self.payload(["--effort", level])["effort"], level)
        with self.assertRaises(spawn.Refusal):
            spawn.parse_args(BASE + ["--effort", "ultra"])


if __name__ == "__main__":
    unittest.main()
