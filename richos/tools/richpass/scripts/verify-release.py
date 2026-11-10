#!/usr/bin/env python3
"""Verify the exact distributable and vendored bytes without installing dependencies."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--production', action='store_true')
parser.add_argument('--signature', type=Path, default=root / 'RichPass.html.minisig')
parser.add_argument('--public-key', type=Path, default=root / 'richos-updater.pub')
args = parser.parse_args()

def require(ok, message):
    if not ok:
        raise SystemExit(message)

def sha(data):
    return hashlib.sha256(data).hexdigest()

html = (root / 'RichPass.html').read_bytes()
manifest = json.loads((root / 'releases.json').read_text(encoding='utf-8'))
release = next(r for r in manifest['releases'] if r['release'] == manifest['bundled'])
require(release['approved'] and sha(html) == release['sha256'], 'Canonical artifact hash is not approved')
require(len(html) <= 4 * 1024 * 1024, 'Artifact is too large')
vendors = json.loads((root / 'vendor/manifest.json').read_text(encoding='utf-8'))
for name, expected in vendors['files'].items():
    require(sha((root / 'vendor' / name).read_bytes()) == expected, 'Vendor mismatch: ' + name)
text = html.decode()
for element, file in [('vendor-argon2', 'hash-wasm/argon2.umd.min.js'), ('vendor-domain', 'tldts/index.umd.min.js')]:
    match = re.search(r'<(?:script|style) id="' + element + r'"[^>]*>(.*?)</(?:script|style)>', text, re.S)
    require(match, 'Missing inline vendor section: ' + element)
    expected = (root / 'vendor' / file).read_text(encoding='utf-8')
    if element == 'vendor-domain':
        expected = re.sub(r'\n?//# sourceMappingURL=.*', '', expected)
    require(match[1].strip() == expected.strip(), 'Embedded vendor differs: ' + element)
# The frozen CEO-approved design is independent of the current implementation.
# Keep fonts, CSS and decorative scripts byte-identical; browser checks compare
# presentation and interactions against this same input with fictional data only.
style_reference = (root / 'tests/RichPass.style-reference.html').read_bytes()
require(sha(style_reference) == 'ae0ded1256449b06691fb5d90593f6037fbd7f4da771f46a1f4c1804d27b5f09', 'Approved styling reference changed')
setup_reference = (root / 'tests/RichPass.setup-reference.html').read_bytes()
require(sha(setup_reference) == '63f06796919bcf54007f978a1b7a85e24907560cfe824ad98c8167822c25236b', 'Approved round-62.1 reference changed')
states_reference = (root / 'tests/RichPass.states-reference.html').read_bytes()
require(sha(states_reference) == '4c0de31d128d5fc172334ad501a0533736c616086b2fc37482be2e88f5c3f050', 'Approved round-63.1 reference changed')
master_reference = (root / 'tests/RichPass.master-reference.html').read_bytes()
require(sha(master_reference) == 'e7f2f6f8659e91f736f78694d717c4c4172e10d3ee972e2ada1d2d898131ec45', 'Approved round-64.1 reference changed')
for element in ['richos-fonts', 'richpass-css', 'richos-speckle', 'richos-theme']:
    pattern = r'<(?:script|style) id="' + element + r'"[^>]*>(.*?)</(?:script|style)>'
    actual = re.search(pattern, text, re.S)
    reference = re.search(pattern, master_reference.decode(), re.S)
    if element == 'richpass-css' and reference:
        # Exclude only the mock controls; production keeps all approved product CSS.
        expected = re.sub(r'    /\* ---- Mockup chrome \(not RichPass\): the dock[\s\S]*?(?=    @media \(prefers-reduced-motion)', '', reference[1])
        expected = re.sub(r'    /\* Mockup chrome \(not RichPass\): room under [\s\S]*?    #master-view\.page \{ padding-bottom: 5rem; \}\n', '', expected)
        expected = expected.removesuffix('  ')  # Remove a whitespace-only mock line.
        expected += '''
    /* 1.1.3: the CEO's longer label wraps within the existing responsive panel. */
    #generate { max-width: 100%; white-space: normal; text-align: center; }
'''
    else:
        expected = reference[1] if reference else None
    require(actual and reference and actual[1] == expected, 'Approved styling differs: ' + element)
# Cover all executable first-party scripts, including decoration. Vendors may
# contain strings naming Node APIs. Only the session module may use IndexedDB;
# the browser suite checks encrypted records, key persistence and session lifetime.
first_party = '\n'.join(body for attrs, body in re.findall(r'<script\b([^>]*)>(.*?)</script>', text, re.S)
                        if not re.search(r'id="(?:vendor-argon2|vendor-domain|public-test-vectors)"', attrs))
require(not re.search(r'\b(?:fetch|XMLHttpRequest|WebSocket|sendBeacon)\b', first_party), 'Unexpected first-party network/persistent-storage surface')
session = re.search(r'<script id="richpass-session"[^>]*>(.*?)</script>', text, re.S)
require(session, 'Missing encrypted-session module')
verifier = re.search(r'<script id="richpass-verifier"[^>]*>(.*?)</script>', text, re.S)
require(verifier, 'Missing master-verification module')
require(not re.search(r'\blocalStorage\b', first_party.replace(verifier[1], '')), 'Persistent ciphertext storage must stay in the verifier module')
other_scripts = first_party.replace(session[1], '')
require(not re.search(r'\bindexedDB\b', other_scripts), 'Key storage must stay in the session module')
require(not re.search(r'\b(?:exportKey|importKey)\b', first_party), 'Unexpected raw key surface')
require('<meta name="application-version" content="' + manifest['bundled'] + '">' in text, 'Artifact version differs from manifest')
if args.production:
    expected_url = f"https://github.com/RichSpark/RichOS/releases/download/richpass-v{release['release']}/RichPass.html"
    require(release.get('download_url') == expected_url, 'Production requires the exact versioned RichOS release asset URL')
    checksum = root / 'RichPass.html.sha256'
    require(checksum.is_file() and checksum.read_text(encoding='ascii').split() == [release['sha256'], 'RichPass.html'],
            'Published checksum differs from the approved artifact')
    require(args.signature.is_file() and args.public_key.is_file(), 'Production requires a detached signature and the RichOS updater public key')
    conf = json.loads((root.parent.parent / 'app/src-tauri/tauri.conf.json').read_bytes())
    trusted_key = base64.b64decode(conf['plugins']['updater']['pubkey'], validate=True).strip()
    require(args.public_key.read_bytes().strip() == trusted_key, 'Public key differs from the RichOS distribution trust anchor')
    require(not (root / 'tests/release-blockers.json').exists(), 'Resolve the recorded compatibility/platform release blockers before production')
    require(shutil.which('minisign') is not None, 'Production verification requires Minisign on PATH')
    subprocess.run(['minisign', '-Vm', str(root / 'RichPass.html'), '-x', str(args.signature),
                    '-p', str(args.public_key)], check=True)
print('PASS canonical HTML, approved hash and embedded vendor bytes')
