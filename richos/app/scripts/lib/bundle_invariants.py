#!/usr/bin/env python3
"""Checks owned by the finished app bundle, shared by build and verify-only."""
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import sys
sys.dont_write_bytecode = True

from no_host_paths import DEFAULT_ALLOWED, scan_bytes


def names_predicate():
    path = Path(__file__).resolve().parents[3] / 'engine/scripts/lib/named-persons.py'
    spec = importlib.util.spec_from_file_location('named_persons', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def reviewed_upstream_paths():
    # Trust the reviewed source recipe, never an exception invented by a bundle's
    # own manifest. Re-signing a changed file cannot excuse a private path.
    scripts = Path(__file__).resolve().parent.parent
    spec = importlib.util.spec_from_file_location('cleaner_resource_recipe', scripts / 'build-cleaner-resources.py')
    recipe = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(recipe)
    rows = {}
    for prefix, entries in [
        ('runtime/', json.loads((scripts / 'runtime-sources.json').read_text()).get('upstream_path_exemptions', [])),
        ('RichCleaner/', recipe.UPSTREAM_EXAMPLES),
    ]:
        for entry in entries:
            rows['Contents/Resources/' + prefix + entry['member']] = entry
    return rows


def home_path_findings(member, data, app, reviewed):
    findings = scan_bytes(data, DEFAULT_ALLOWED, os.path.expanduser('~'))
    entry = reviewed.get(str(member.relative_to(app)))
    if entry and hashlib.sha256(data).hexdigest() == entry['sha256']:
        for prefix in entry['prefixes']:
            findings.pop(prefix, None)
    return findings


def verify(app):
    app = app.resolve(strict=True)
    failures = set()
    members = sorted(app.rglob('*'))
    files = []
    for member in members:
        if not member.resolve(strict=True).is_relative_to(app):
            failures.add('bundle-contained-paths: a member resolves outside the bundle')
        elif member.is_file():
            files.append(member)
    print(f'bundle inventory: {len(files)} files, {len(members)} members (diagnostic only)')
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    name = info.get('CFBundleExecutable', '')
    if not name or Path(name).name != name or name in ('.', '..'):
        failures.add('bundle-executable-path: executable must be a single filename')
        executable = None
    else:
        executable = app / 'Contents/MacOS' / name
    stamp = info.get('RichOSSourceCommit', '')
    if not isinstance(stamp, str) or not re.fullmatch(r'[0-9a-f]{40}(?:-dirty)?', stamp):
        failures.add('bundle-source-identity: missing or invalid RichOSSourceCommit')
    elif executable not in files or stamp.encode() not in executable.read_bytes():
        failures.add('bundle-source-identity: plist source stamp is absent from the executable')

    reviewed = reviewed_upstream_paths()
    predicate = names_predicate()
    try:
        denylist = predicate.load_list()
    except (predicate.ListAbsent, predicate.ListBroken):
        raise ValueError('bundle-private-names: a valid external named-persons list is required') from None
    # Candidate filtering avoids constructing spans for every machine-code
    # string. The existing predicate still decides matches. Hash entries scan all.
    needles = {token for sequence, _ in denylist.sequences for token in sequence}
    needles.update(single[0] for single in denylist.singles)
    for member in files:
        data = member.read_bytes()
        # The path checker is the same predicate used before signing. Nothing
        # about an existing signature excuses a path leak in verify-only mode.
        if home_path_findings(member, data, app, reviewed):
            failures.add('bundle-home-paths: a member contains a home-directory path')
        relative = str(member.relative_to(app))
        if denylist.match(relative, 'bundle member'):
            failures.add('bundle-private-names: a member path contains a listed name')
        # Printable strings include UTF-8 names in binaries. Scan each run so
        # nontext bytes never join two unrelated strings into a synthetic name.
        for run in re.finditer(rb'(?:[\t\r\n\x20-\x7e]|[\xc2-\xdf][\x80-\xbf]|[\xe0-\xef][\x80-\xbf]{2}|[\xf0-\xf4][\x80-\xbf]{3})+', data):
            text = run.group().decode('utf-8', 'replace')
            folded = text.lower() if text.isascii() else predicate.fold(text)
            if not denylist.digests and not any(needle in folded for needle in needles):
                continue
            if denylist.match(text, 'bundle content'):
                failures.add('bundle-private-names: member content contains a listed name')
                break
    for failure in sorted(failures):
        print(failure, file=sys.stderr)
    return not failures


if __name__ == '__main__':
    try:
        sys.exit(0 if verify(Path(sys.argv[1])) else 1)
    except (OSError, ValueError, KeyError) as error:
        # Never print matched names, contents or private roster paths.
        print(f'bundle-invariants: unable to verify ({type(error).__name__})', file=sys.stderr)
        if isinstance(error, ValueError) and str(error).startswith('bundle-private-names:'):
            print(str(error), file=sys.stderr)
        sys.exit(1)
