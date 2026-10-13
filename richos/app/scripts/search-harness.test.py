"""The archive-search harness, driven as a program (search-harness.test.sh builds it and says why).

Every run gets its own temporary index folder; the folder searched is the crate's synthetic
stand-in, and the embedder is the stand-in one, so nothing here needs a model or the archive.
"""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

APP = Path(__file__).resolve().parents[1]
CRATE = APP / "crates" / "richos-search"
STANDIN = CRATE / "tests" / "fixtures" / "standin"
QUESTIONS = CRATE / "tests" / "fixtures" / "standin-questions.json"
HARNESS = os.environ["RICHOS_SEARCH_HARNESS"]


def harness(*args):
    return subprocess.run([HARNESS, *map(str, args)], capture_output=True, text=True, timeout=300)


class Harness(unittest.TestCase):
    def setUp(self):
        self.scratch = Path(tempfile.mkdtemp(prefix="search-harness-")).resolve()

    def tearDown(self):
        subprocess.run(["rm", "-rf", str(self.scratch)], check=True)

    def test_inventory_counts_the_folder_and_names_a_long_file(self):
        r = harness("inventory", "--root", STANDIN)
        self.assertEqual(r.returncode, 0, r.stderr)
        inv = json.loads(r.stdout)
        self.assertEqual(inv["files"], 10)
        self.assertGreater(inv["passages"], 10)
        self.assertEqual(inv["skipped"], [])
        self.assertEqual(inv["representative_files"]["long"]["path"], "2024/onboarding-email-brainstorm.md")

    def test_query_finds_by_meaning_only_with_super_finder_on(self):
        index = self.scratch / "index"
        query = ["query", "--root", STANDIN, "--index", index, "--embedder", "stand-in", "the app felt slow on first open"]
        on = harness(*query)
        self.assertEqual(on.returncode, 0, on.stderr)
        on = json.loads(on.stdout)
        self.assertTrue(on["effective"]["super_finder"])
        self.assertIn("2025/iphone-build-three-seconds.md", [h["rel_path"] for h in on["hits"]])
        off = harness(*query[:-1], "--off", query[-1])
        self.assertEqual(off.returncode, 0, off.stderr)
        off = json.loads(off.stdout)
        self.assertFalse(off["effective"]["super_finder"])
        self.assertNotIn("2025/iphone-build-three-seconds.md", [h["rel_path"] for h in off["hits"]])

    def test_compare_scores_the_committed_questions_in_both_modes(self):
        out = self.scratch / "compare.json"
        r = harness("compare", "--root", STANDIN, "--index", self.scratch / "index", "--embedder", "stand-in",
                    "--questions", QUESTIONS, "--out", out)
        self.assertEqual(r.returncode, 0, r.stderr)
        report = json.loads(out.read_text())
        questions = json.loads(QUESTIONS.read_text())["questions"]
        answerable = [q for q in questions if q.get("expected")]
        self.assertEqual(report["answerable_questions"], len(answerable))
        self.assertEqual(len(report["questions"]), len(questions))
        found = report["found_in_first_five"]
        # Super Finder adds meaning matches to the same text list; it may not lose a question the
        # text list alone answers.
        self.assertGreaterEqual(found["super_finder_on"], found["super_finder_off"])
        self.assertEqual(report["regressions"], [])
        # s1's file shares none of the query's words: only meaning puts it in the first five. A
        # question counts as found only when the passage shown holds the expected phrase, which
        # depends on the embedder, so the file is what is asserted here.
        s1 = next(q for q in report["questions"] if q["id"] == "s1")
        top = {mode: [r["file"] for r in s1["modes"][mode]["top5"]] for mode in ("on", "off")}
        self.assertIn("2025/iphone-build-three-seconds.md", top["on"])
        self.assertNotIn("2025/iphone-build-three-seconds.md", top["off"])
        for mode in ("on", "off"):
            self.assertGreater(report["service_latency_ms"][mode]["n"], 0)

    def test_compare_refuses_an_output_inside_the_published_repository(self):
        inside = APP / "target-search-harness-test" / "compare.json"
        r = harness("compare", "--root", STANDIN, "--index", self.scratch / "index", "--embedder", "stand-in",
                    "--questions", QUESTIONS, "--out", inside)
        self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
        self.assertIn("refusing --out inside", r.stderr)
        self.assertFalse(inside.exists())
        self.assertFalse(inside.parent.exists())


if __name__ == "__main__":
    sys.exit(0 if unittest.main(exit=False, verbosity=2).result.wasSuccessful() else 1)
