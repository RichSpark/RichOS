#!/usr/bin/env python3
"""token-track report: fixture transcript + fixture readings give a known tokens-per-point."""
import json
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
T0 = 1791360000  # 2026-10-07T06:40:00Z


def iso(epoch):
    import datetime
    return datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")


class Install(unittest.TestCase):
    def test_token_tracker_installs_in_fresh_home(self):
        with tempfile.TemporaryDirectory(prefix='token-install-') as temporary:
            root = Path(temporary)
            bin_dir = root / 'bin'
            bin_dir.mkdir()
            launchctl = bin_dir / 'launchctl'
            launchctl.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$HOME/calls"\n')
            launchctl.chmod(0o755)
            home = root / 'home'
            home.mkdir()
            env = dict(os.environ, HOME=str(home), PATH=str(bin_dir) + os.pathsep + os.environ['PATH'])
            result = subprocess.run(['bash', str(Path(HERE) / 'token-track-launchd.sh'), 'install'],
                                    env=env, capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            plist = home / 'Library/LaunchAgents/com.richos.token-track.plist'
            data = plistlib.loads(plist.read_bytes())
            self.assertEqual(data['Label'], 'com.richos.token-track')
            self.assertEqual(len(data['StartCalendarInterval']), 60)
            self.assertIn('load ' + str(plist), (home / 'calls').read_text().splitlines())


class Report(unittest.TestCase):
    def test_tokens_per_weekly_point(self):
        with tempfile.TemporaryDirectory() as d:
            folder = os.path.join(d, "acct")
            os.makedirs(os.path.join(folder, "projects", "p"))
            rows = []
            # msg a streamed twice (the larger copy counts); 3 weekly points in total
            for mid, at, model, u in [
                ("a", T0 + 610, "m1", dict(input_tokens=1, output_tokens=1, cache_creation_input_tokens=0, cache_read_input_tokens=0)),
                ("a", T0 + 611, "m1", dict(input_tokens=10, output_tokens=100, cache_creation_input_tokens=1000, cache_read_input_tokens=10000)),
                ("b", T0 + 700, "m2", dict(input_tokens=20, output_tokens=200, cache_creation_input_tokens=2000, cache_read_input_tokens=20000)),
                ("c", T0 + 5000, "m1", dict(input_tokens=999, output_tokens=999, cache_creation_input_tokens=999, cache_read_input_tokens=999)),
            ]:
                rows.append({"timestamp": iso(at), "message": {"id": mid, "model": model, "usage": u}})
            with open(os.path.join(folder, "projects", "p", "s.jsonl"), "w") as fh:
                fh.write("\n".join(json.dumps(r) for r in rows) + "\n")
            state = os.path.join(d, "state")
            os.makedirs(state)
            reset = T0 + 86400
            with open(os.path.join(state, "readings.jsonl"), "w") as fh:
                for at, used in [(T0, 50), (T0 + 600, 53), (T0 + 1200, 54)]:  # 53 lasts from +600 to +1200: a and b inside, c after
                    fh.write(json.dumps({"at": at, "account": "x", "seven_day": {"used": used, "resets_at": reset}}) + "\n")
            env = dict(os.environ, TOKEN_TRACK_STATE=state)
            out = subprocess.run([sys.executable, os.path.join(HERE, "token-track.py"), "report", "--account", "x=" + folder],
                                 env=env, capture_output=True, text=True)
            self.assertEqual(out.returncode, 0, out.stderr)
            # tokens: input 30, output 300, write 3000, read 30000 over 1 point
            self.assertIn("1 point = 33,330 tokens (input 30, output 300, cache write 3000, cache read 30000)", out.stdout)
            self.assertIn("1 points over 1 rises", out.stdout)

    def test_weekly_73_74_75_gives_edges_of_74(self):
        sys.path.insert(0, HERE)
        import importlib.util
        spec = importlib.util.spec_from_file_location("tt", os.path.join(HERE, "token-track.py"))
        tt = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(tt)
        reset = T0 + 86400
        rows = [{"at": T0 + 60 * i, "account": "x", "seven_day": {"used": u, "resets_at": reset}}
                for i, u in enumerate([73, 73, 74, 74, 74, 75, 75])]
        self.assertEqual(tt.episodes(rows, "seven_day"), [(74, T0 + 120, T0 + 300, 75)])
        tokens = [(T0 + 100, "m", dict(input=1, output=1, cache_write=1, cache_read=1)),   # before 74 began
                  (T0 + 200, "m", dict(input=5, output=6, cache_write=7, cache_read=8)),   # inside 74
                  (T0 + 300, "m", dict(input=1, output=1, cache_write=1, cache_read=1)),   # at the edge, counts
                  (T0 + 400, "m", dict(input=9, output=9, cache_write=9, cache_read=9))]   # after
        r = tt.per_point(rows, tokens, "seven_day")
        self.assertEqual(r["points"], 1)
        self.assertEqual(r["tokens"], dict(input=6, output=7, cache_write=8, cache_read=9))


if __name__ == "__main__":
    unittest.main()
