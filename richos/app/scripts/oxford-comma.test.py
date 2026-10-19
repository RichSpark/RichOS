#!/usr/bin/env python3
"""oxford-comma.test.py — the fixer git runs at every commit (CEO, 2026-10-19: "in the UI as well
as any of our docs etc, the Oxford comma must be always off"), driven through `--json`, the exact
interface autocheck.py calls, over planted text. The UI cases read the real UI tree to classify a
path; nothing is written. `autocheck.test.py` (class OxfordComma) proves git runs it at the
commit, on what is being committed.
"""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import unittest

FIXER = Path(__file__).resolve().parent / "oxford-comma.js"
NODE = shutil.which("node")


def fix(path, content):
    """The fixed content, or None when the fixer leaves the file alone."""
    done = subprocess.run([NODE, str(FIXER), "--json"], input=json.dumps({"files": [{"path": path, "content": content}]}),
                          capture_output=True, text=True, timeout=60)
    if done.returncode:
        raise AssertionError(done.stderr)
    files = json.loads(done.stdout)["files"]
    return files[0]["content"] if files else None


class Rule(unittest.TestCase):
    def test_the_comma_before_and_or_nor_goes_and_nothing_else_does(self):
        self.assertEqual(fix("docs/a.md", "Red, green, and blue."), "Red, green and blue.")
        self.assertEqual(fix("docs/a.md", "Tea, or coffee, nor milk."), "Tea or coffee nor milk.")
        self.assertEqual(fix("docs/a.md", "Saved here, and I'll keep trying."), "Saved here and I'll keep trying.")
        self.assertEqual(fix("docs/a.md", "A line,\nand the next."), "A line\nand the next.")
        for kept in ("Android, order, Oregon.", "One, AND two.", "Yes, andante.", "Red,and blue."):
            self.assertIsNone(fix("docs/a.md", kept), kept)


class Docs(unittest.TestCase):
    def test_code_front_matter_quotations_and_comments_keep_every_comma(self):
        doc = "\n".join([
            "---", "description: red, white, and blue", "---",
            "Apples, pears, and plums.",
            "Inline `a, and b` stays. \"Quoted, and kept\" stays. “Curly, or kept” stays.",
            "> A quote, and more.", "<!-- a comment, and more -->",
            "```", "code, and more", "```", "",
            "    indented code, and more", "",
            "- list item, and more", "",
        ])
        want = doc.replace("pears, and", "pears and").replace("item, and", "item and")
        self.assertEqual(fix("docs/guide.md", doc), want)
        self.assertIsNone(fix("docs/guide.md", want), "a clean doc is left alone")

    def test_a_fixture_captured_or_third_party_doc_and_a_non_doc_are_left_alone(self):
        for path in ("richos/app/crates/richos-core/tests/fixtures/ledgers/README.md",
                     "richos/engine/scripts/hooks/fixtures/third_party/copywriting/README.md",
                     "docs/research/raw/call.md", "docs/notes.txt", "richos/app/scripts/thing.py"):
            self.assertIsNone(fix(path, "He said, and left."), path)
        self.assertEqual(fix("richos/engine/agents/rich.md", "Plan, and do."), "Plan and do.")


class UiText(unittest.TestCase):
    """Only the UI text of a UI file, the way the m-dash check in ui/tests/dialect.js reads it."""

    def test_a_shipped_ui_file_loses_the_comma_in_its_strings_and_keeps_its_comments(self):
        src = ('const a = "Saved here, and I will keep trying."; // a comment, and fine\n'
               "const b = `<p>Tea, or coffee</p><!-- a note, or fine -->`;\n")
        self.assertEqual(fix("richos/app/ui/main.js", src),
                         'const a = "Saved here and I will keep trying."; // a comment, and fine\n'
                         "const b = `<p>Tea or coffee</p><!-- a note, or fine -->`;\n")

    def test_a_file_under_ui_that_does_not_ship_is_not_ui_text(self):
        self.assertIsNone(fix("richos/app/ui/tests/dialect.js", 'const a = "Saved, and kept.";\n'))

    def test_a_rust_sentence_the_ui_relays_loses_it_and_a_log_line_keeps_it(self):
        rs = ('fn f() -> Result<(), String> {\n    Err("I could not open it, and nothing changed.".into())\n}\n'
              'fn g() {\n    eprintln!("Log line, and it is for a terminal.");\n}\n')
        self.assertEqual(fix("richos/app/src-tauri/src/main.rs", rs), rs.replace("it, and nothing", "it and nothing"))


if __name__ == "__main__":
    result = unittest.main(exit=False, verbosity=2).result
    sys.exit(0 if result.wasSuccessful() else 1)
