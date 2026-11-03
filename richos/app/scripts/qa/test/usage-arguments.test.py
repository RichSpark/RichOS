#!/usr/bin/env python3
"""A missing option value is refused before any provider can be started."""
import os
from pathlib import Path
import subprocess
import unittest

ROOT=Path(os.environ.get('G8_ROOT',Path(__file__).resolve().parents[5]))
class Arguments(unittest.TestCase):
    def test_each_missing_value_exits_two(self):
        for flag in ('--wait','--claude','--home','--config-dir'):
            with self.subTest(flag=flag):
                # load-bound: hang guard only; a parser must terminate without a provider.
                result=subprocess.run(['sh',str(ROOT/'richos/app/scripts/qa/usage-shape.sh'),flag],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=5)
                self.assertEqual(result.returncode,2)

if __name__=='__main__': unittest.main()
