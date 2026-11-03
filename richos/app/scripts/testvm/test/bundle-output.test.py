#!/usr/bin/env python3
"""Relative and absolute bundle outputs survive staging cleanup. No build or signing."""
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile
import unittest

HERE = Path(os.environ.get('G8_ROOT', Path(__file__).resolve().parents[5])) / 'richos/app/scripts/testvm'

class BundleOutput(unittest.TestCase):
    def test_both_helpers_preserve_output(self):
        for helper in ('branch-bundle.sh', 'branch-bundle-richdoctor.sh'):
            for relative in (True, False):
                with self.subTest(helper=helper, relative=relative), tempfile.TemporaryDirectory() as temp:
                    root = Path(temp)
                    scripts = root / 'app/scripts'
                    (scripts / 'testvm').mkdir(parents=True)
                    (scripts / 'lib').mkdir()
                    (root / 'app/src-tauri').mkdir()
                    shutil.copy2(HERE / helper, scripts / 'testvm' / helper)
                    bin_dir = root / 'bin'; bin_dir.mkdir()
                    exe = root / 'richos-tauri'; exe.write_text('fixture'); exe.chmod(0o755)
                    for name, body in {
                        'cargo': f'echo \'{{"executable":"{exe}"}}\'',
                        'git': 'echo fixture-sha',
                        'codesign': 'exit 0',
                    }.items():
                        file = bin_dir / name; file.write_text('#!/bin/sh\n' + body + '\n'); file.chmod(0o755)
                    (scripts / 'lib/gui-launch.sh').write_text('gui_identity() { echo fixture 1.0; }\ngui_bundle() { mkdir -p "$1"; cp "$2" "$1/exe"; }\n')
                    out = Path('deliverables/result.zip') if relative else root / 'absolute/result.zip'
                    args = ['--out', str(out)]
                    if helper == 'branch-bundle.sh':
                        args += ['--engine-version', '1', '--engine-url', 'fixture', '--engine-sha256', 'fixture']
                    else:
                        for name in ('RichOS', 'RichDoctor'):
                            app = root / f'{name}.app/Contents'; app.mkdir(parents=True)
                            (app / 'Info.plist').write_bytes(plistlib.dumps({'CFBundleShortVersionString':'1.0', 'CFBundleVersion':'1'}))
                            subprocess.run(['ditto', '-c', '-k', '--keepParent', str(app.parent), str(root / f'{name}.zip')], check=True)
                        args += ['--in', str(root / 'RichOS.zip'), '--richdoctor-zip', str(root / 'RichDoctor.zip')]
                    done = subprocess.run(['bash', str(scripts / 'testvm' / helper), *args], cwd=root,
                                          env={**os.environ, 'PATH':str(bin_dir) + ':' + os.environ['PATH']}, capture_output=True, text=True, timeout=30)
                    self.assertEqual(done.returncode, 0, done.stderr)
                    self.assertTrue((root / out).is_file(), done.stdout)

if __name__ == '__main__': unittest.main()
