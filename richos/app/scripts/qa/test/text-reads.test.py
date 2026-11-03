#!/usr/bin/env python3
"""Unreadable text and failed process/scanner reads cannot receive a clean verdict."""
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(os.environ.get('G8_ROOT', Path(__file__).resolve().parents[5]))
spec = importlib.util.spec_from_file_location('stop_probe', ROOT / 'richos/app/scripts/qa/stop-probe.py')
stop = importlib.util.module_from_spec(spec); spec.loader.exec_module(stop)

class ReadFailures(unittest.TestCase):
    def test_failed_ps_is_not_an_empty_tree(self):
        def failed_ps(*args, **kwargs):
            if kwargs.get('check'):
                raise subprocess.CalledProcessError(1, args[0])
            return subprocess.CompletedProcess(args[0], 1, '', 'refused')
        with patch.object(stop.subprocess, 'run', side_effect=failed_ps):
            with self.assertRaises((OSError, RuntimeError, subprocess.SubprocessError)):
                stop.sleeps(123)

    def gate(self, mode):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); qa=root/'richos/app/scripts/qa'; qa.mkdir(parents=True)
            shutil.copy2(ROOT/'richos/app/scripts/qa/ocr-gate.sh',qa/'ocr-gate.sh')
            helper=root/'richos/engine/scripts/lib/named-persons.py'; helper.parent.mkdir(parents=True)
            helper.write_text("import sys\nsys.stdin.read()\n" + ("print('CLEAN')\n" if mode in ('read', 'clean', 'hit') else "print('CLEAN' if sys.argv[-1]=='probe' else 'BROKEN')\n" if mode=='broken' else "sys.exit(0 if sys.argv[-1]=='probe' else 7)\n"))
            text=root/'evidence.txt'; text.write_text('qa-fixture@example.invalid' if mode=='hit' else 'safe text')
            bin_dir=root/'bin'; bin_dir.mkdir()
            cat=bin_dir/'cat'; cat.write_text('#!/bin/sh\nexit 1\n' if mode=='read' else '#!/bin/sh\nexec /bin/cat "$@"\n'); cat.chmod(0o755)
            # load-bound: hang guard only; the exit status establishes the read verdict.
            return subprocess.run(['bash',str(qa/'ocr-gate.sh'),'--text',str(text)],env={**os.environ,'PATH':str(bin_dir)+':'+os.environ['PATH']},capture_output=True,text=True,timeout=30)

    def test_gate_requires_read_and_scanner_success(self):
        for mode in ('read','broken','crash','clean','hit'):
            with self.subTest(mode=mode):
                result=self.gate(mode)
                self.assertEqual(result.returncode, 0 if mode=='clean' else 1 if mode=='hit' else 2, result.stdout+result.stderr)

if __name__=='__main__': unittest.main()
