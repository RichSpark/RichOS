#!/usr/bin/env python3
"""Resource verification refuses tampering and pins the PDF Unicode correction."""
# run-tests: no-host-screen: temporary manifest fixtures and source transformations; no app or browser
# run-tests: inputs richos/app/scripts/cleaner-resources.test.py richos/app/scripts/build-cleaner-resources.py
# run-tests: covers richos/app/scripts/build-cleaner-resources.py
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
sys.dont_write_bytecode=True
source=Path(__file__).with_name("build-cleaner-resources.py")
spec=importlib.util.spec_from_file_location("cleaner_resources",source)
resources=importlib.util.module_from_spec(spec)
spec.loader.exec_module(resources)

class Resources(unittest.TestCase):
    def test_missing_or_tampered_runtime_is_refused(self):
        with tempfile.TemporaryDirectory(prefix="richcleaner-resource-test-",dir=os.environ["TMPDIR"]) as temp:
            root=Path(temp)
            (root/"manifest.json").write_text(json.dumps({"files":{}}))
            with self.assertRaisesRegex(RuntimeError,"omits"):
                resources.verify(root)
            files={}
            for name in ["native","perl/bin/perl","exiftool/exiftool"]:
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(b"fixture")
                files[name]=hashlib.sha256(path.read_bytes()).hexdigest()
            (root/"manifest.json").write_text(json.dumps({"files":files}))
            (root/"native").write_bytes(b"changed")
            with self.assertRaisesRegex(RuntimeError,"changed or is missing"):
                resources.verify(root)
            (root/"native").unlink()
            with self.assertRaisesRegex(RuntimeError,"changed or is missing"):
                resources.verify(root)
    def test_pdf_uses_existing_utf16_codec_and_refuses_source_drift(self):
        with tempfile.TemporaryDirectory(prefix="richcleaner-unicode-test-",dir=os.environ["TMPDIR"]) as temp:
            root=Path(temp);writer=root/"lib/Image/ExifTool/WritePDF.pl";reader=root/"lib/Image/ExifTool/PDF.pm";writer.parent.mkdir(parents=True)
            writer.write_text("before Encode($val,'UCS2','MM') after")
            reader.write_text("before ? 'UCS2' : 'PDFDoc' after")
            resources.pdf_unicode(root)
            self.assertEqual(writer.read_text(),"before Encode($val,'UTF16','MM') after")
            self.assertEqual(reader.read_text(),"before ? 'UTF16' : 'PDFDoc' after")
            with self.assertRaisesRegex(RuntimeError,"no longer matches"):
                resources.pdf_unicode(root)

if __name__=="__main__":
    unittest.main()
