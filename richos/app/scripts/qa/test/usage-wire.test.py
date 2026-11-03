#!/usr/bin/env python3
"""Preserve the compact quota request wire consumed by qa.test.sh's existing stand-in."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT=Path(os.environ.get('G8_ROOT',Path(__file__).resolve().parents[5]))

class ExistingQuotaStandIn(unittest.TestCase):
    def test_existing_us1_and_us2_cases(self):
        suite=(ROOT/'richos/app/scripts/qa.test.sh').read_text()
        fake=suite.split('cat > "$TMP/fake-usage-claude" <<\'FAKE\'\n',1)[1].split('\nFAKE\n',1)[0]
        for limits,code,verdict in ((None,1,'verdict: NO READING:'),({'five_hour':{'utilization':41}},0,'verdict: readable by RichOS quota reader')):
            with self.subTest(limits=limits),tempfile.TemporaryDirectory() as tmp:
                root=Path(tmp);claude=root/'claude';claude.write_text(fake+'\n');claude.chmod(0o755)
                payload=root/'usage.json';payload.write_text(json.dumps({'rate_limits_available':True,'rate_limits':limits,'session':{'secret':'never printed'}}))
                # load-bound: outer hang guard; protocol verdict and exit code decide success.
                result=subprocess.run(['sh',str(ROOT/'richos/app/scripts/qa/usage-shape.sh'),'--claude',str(claude),'--wait','2'],env={**os.environ,'FAKE_USAGE':str(payload)},capture_output=True,text=True,timeout=30)
                self.assertEqual(result.returncode,code,result.stdout+result.stderr)
                self.assertIn(verdict,result.stdout)
                self.assertNotIn('never printed',result.stdout+result.stderr)

if __name__=='__main__':unittest.main()
