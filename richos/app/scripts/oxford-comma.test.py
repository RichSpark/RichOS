#!/usr/bin/env python3
"""oxford-comma.test.py — the Oxford comma rule (CEO, 2026-10-19): ", and" -> " and", ", or" ->
" or", ", nor" -> " nor", never in code. Driven through the fixer's `--json`, the interface
autocheck.py calls at every commit, plus the display filter the app draws Claude's words with.
`autocheck.test.py` (class OxfordComma) proves git runs it at the commit.
"""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import unittest

HERE = Path(__file__).resolve().parent
FIXER = HERE / "oxford-comma.js"
RULE = HERE.parent / "ui" / "oxford-comma.js"
NODE = shutil.which("node")


def fix(path, content):
    """The fixed content, or None when the fixer leaves the file alone."""
    done = subprocess.run([NODE, str(FIXER), "--json"], input=json.dumps({"files": [{"path": path, "content": content}]}),
                          capture_output=True, text=True, timeout=60)
    if done.returncode:
        raise AssertionError(done.stderr)
    files = json.loads(done.stdout)["files"]
    return files[0]["content"] if files else None


class Docs(unittest.TestCase):
    def test_the_rule(self):
        self.assertEqual(fix("docs/a.md", "Red, green, and blue. Tea, or coffee, nor milk."),
                         "Red, green and blue. Tea or coffee nor milk.")
        self.assertIsNone(fix("docs/a.md", "Android, order, Oregon."))

    def test_code_is_never_touched(self):
        for code in ("```\ncode, and more\n```\n",
                     "1. ```text\n   apples, and pears\n   ```\n",
                     "<pre><code>print('red, green, and blue')</code></pre>\n",
                     "Run `a, or b` now.\n"):
            self.assertIsNone(fix("docs/a.md", code), code)
        doc = "1. ```text\n   apples, and pears\n   ```\n\nNormal prose, and more.\n"
        self.assertEqual(fix("docs/a.md", doc), doc.replace("prose, and", "prose and"))

    def test_fixtures_and_non_docs_are_left_alone(self):
        for path in ("richos/app/crates/richos-core/tests/fixtures/ledgers/README.md",
                     "docs/research/raw/call.md", "docs/notes.txt", "richos/app/scripts/thing.py"):
            self.assertIsNone(fix(path, "He said, and left."), path)


class UiText(unittest.TestCase):
    def test_strings_lose_it_and_comments_keep_it(self):
        src = 'const a = "Saved here, and I will keep trying."; // a comment, and fine\n'
        self.assertEqual(fix("richos/app/ui/main.js", src), src.replace("here, and", "here and"))
        rs = 'fn f() -> Result<(), String> {\n    Err("I could not open it, and nothing changed.".into())\n}\n'
        self.assertEqual(fix("richos/app/src-tauri/src/main.rs", rs), rs.replace("it, and", "it and"))
        self.assertIsNone(fix("richos/app/ui/tests/dialect.js", 'const a = "Saved, and kept.";\n'))


class Display(unittest.TestCase):
    def test_a_reply_loses_it_and_its_code_keeps_it(self):
        reply = "Red, green, and blue.\n\n```js\nf(a, or b)\n```\n<pre><code>x, and y</code></pre>\n"
        done = subprocess.run([NODE, "-e", "process.stdout.write(require(process.argv[1]).display(process.argv[2]))",
                               str(RULE), reply], capture_output=True, text=True, timeout=60)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stdout, reply.replace("green, and", "green and"))


if __name__ == "__main__":
    result = unittest.main(exit=False, verbosity=2).result
    sys.exit(0 if result.wasSuccessful() else 1)
