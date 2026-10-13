#!/usr/bin/env python3
"""search-walk.py's guest-side halves and its argument check, offline. The live walk is its own proof.

`--make-notes` and `--tool-probe` are what the walk pushes into the guest and runs there; here they
run on this Mac against a temporary folder and a stub of the app's `--search-mcp` server.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent.parent
WALK = HERE / 'search-walk.py'
STANDIN = HERE.parents[1] / 'crates' / 'richos-search' / 'tests' / 'fixtures' / 'standin'

# A stand-in for `richos-tauri --search-mcp <endpoint>`: answers initialize, then one tools/call,
# refusing a `folder` argument the way search_tools.rs does and otherwise echoing the modes asked.
STUB = r'''#!/usr/bin/env python3
import json, sys
for line in sys.stdin:
    req = json.loads(line)
    if req["method"] == "initialize":
        print(json.dumps({"jsonrpc": "2.0", "id": req["id"], "result": {"protocolVersion": "2025-06-18"}}))
        continue
    args = req["params"]["arguments"]
    if "folder" in args:
        result = {"isError": True, "content": [{"type": "text", "text": "'folder' is not one of them. Nothing was searched."}]}
    else:
        mode = {"super_finder": args.get("super_finder", True), "strict": args.get("strict", False)}
        answer = {"requested": mode, "effective": mode, "meaning_search": "ready", "coverage": {"files_in_folder": 10},
                  "total_matches": 1, "results": [{"path": "2025/refund-window.md", "why": "words", "excerpt": "x" * 12}]}
        result = {"isError": False, "content": [{"type": "text", "text": json.dumps(answer)}]}
    print(json.dumps({"jsonrpc": "2.0", "id": req["id"], "result": result}))
'''


class GuestHalves(unittest.TestCase):
    def setUp(self):
        self.scratch = Path(tempfile.mkdtemp(prefix='search-walk-')).resolve()

    def tearDown(self):
        subprocess.run(['rm', '-rf', str(self.scratch)], check=True)

    def test_make_notes_writes_the_count_asked_and_no_stand_in_subject(self):
        r = subprocess.run([sys.executable, WALK, '--make-notes', self.scratch, '25'], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(json.loads(r.stdout)['written'], 25)
        notes = sorted((self.scratch / 'bulk').glob('*.md'))
        self.assertEqual(len(notes), 25)
        text = ' '.join(n.read_text().lower() for n in notes)
        subjects = {w.lower() for f in STANDIN.rglob('*.md') for w in f.read_text().split('\n', 1)[0].split()[1:]}
        queries = {'app', 'felt', 'slow', 'first', 'open', 'cancel', '30', 'days'}
        overlap = sorted((subjects | queries) & set(text.replace('.', ' ').split()) - {'a', 'the', 'to', 'of', 'and', 'in'})
        self.assertEqual(overlap, [], 'the bulk notes would compete with the answers')
        again = Path(tempfile.mkdtemp(prefix='search-walk-again-')).resolve()
        try:
            subprocess.run([sys.executable, WALK, '--make-notes', again, '25'], capture_output=True, check=True)
            self.assertEqual([n.read_text() for n in notes], [n.read_text() for n in sorted((again / 'bulk').glob('*.md'))])
        finally:
            subprocess.run(['rm', '-rf', str(again)], check=True)

    def test_tool_probe_records_each_mode_and_the_refusal(self):
        stub = self.scratch / 'stub-app'
        stub.write_text(STUB)
        stub.chmod(0o755)
        r = subprocess.run([sys.executable, WALK, '--tool-probe', stub, self.scratch / 'endpoint.json'],
                           capture_output=True, text=True, timeout=120)
        self.assertEqual(r.returncode, 0, r.stderr)
        rows = [json.loads(l) for l in r.stdout.splitlines()]
        self.assertEqual(len(rows), 5)
        self.assertEqual([row['effective']['super_finder'] for row in rows[:4]], [True, False, True, False])
        self.assertEqual([row['effective']['strict'] for row in rows[:4]], [False, False, True, True])
        self.assertIn('refused', rows[4])
        self.assertTrue(all(isinstance(row['wall_ms_including_process_start'], int) for row in rows))

    def test_an_unknown_step_is_refused_before_anything_runs(self):
        r = subprocess.run([sys.executable, WALK, 'richos-test-1', '--out', self.scratch / 'o', '--expect-sha', 'abc',
                            '--steps', 'identity,nonsense'], capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 2)
        self.assertIn('unknown step(s): nonsense', r.stderr)
        self.assertFalse((self.scratch / 'o').exists())


if __name__ == '__main__':
    sys.exit(0 if unittest.main(exit=False, verbosity=2).result.wasSuccessful() else 1)
