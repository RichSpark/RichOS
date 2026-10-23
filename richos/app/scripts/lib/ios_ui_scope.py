"""Case selection for the phone's existing XCTest runner.

Shared app inputs select the whole UI suite. Feature inputs select reviewed case
families. Unknown/new app files keep the conservative shared-input behavior.
Harness changes are proved by merge-check-scope.test.sh, not by phone features.
"""
from pathlib import Path
import re
import hashlib
import shlex
import sys

NATIVE = 'richos/mobile/native-ios/'
# Prefixes name XCTest families, not a second inventory of individual tests.
FEATURES = {
    'Pairing': ('ScreenshotTests/testPairing', 'ScreenshotTests/testRecovery',
                'ScreenshotTests/testUpdates', 'ScreenshotTests/testLaunch', 'PairWaitInteractionTests/',
                'InteractionTests/testTheyMatch', 'InteractionTests/testAWait', 'InteractionTests/testLeaving',
                'AccessibilityLayoutTests/testEveryPairing', 'AccessibilityLayoutTests/testThePairing',
                'AccessibilityLayoutTests/testATakeover', 'AccessibilityLayoutTests/testSystemAccessibilityAudit'),
    'Composer': ('ScreenshotTests/testComposer', 'ScreenshotTests/testConversation', 'ScreenshotTests/testConnection',
                 'InteractionTests/testCompose', 'InteractionTests/testReading', 'InteractionTests/testScrolling', 'QuestionTests/',
                 'AccessibilityLayoutTests/'),
    'Voice': ('ScreenshotTests/testVoice', 'InteractionTests/testHold', 'InteractionTests/testATap',
              'InteractionTests/testSlide', 'InteractionTests/testBackground', 'InteractionTests/testAPress',
              'InteractionTests/testTheMicrophone', 'AccessibilityLayoutTests/testLockedRecording',
              'AccessibilityLayoutTests/testTheMicrophone', 'AccessibilityLayoutTests/testTheKeptVoice',
              'AccessibilityLayoutTests/testSystemAccessibilityAudit'),
    'Settings': ('ScreenshotTests/testSettings', 'ScreenshotTests/testNotifications', 'InteractionTests/testSettings',
                 'InteractionTests/testNotNow', 'AccessibilityLayoutTests/testOverlays',
                 'AccessibilityLayoutTests/testSettings', 'AccessibilityLayoutTests/testADialog',
                 'AccessibilityLayoutTests/testTheMicrophone',
                 'AccessibilityLayoutTests/testSystemAccessibilityAudit'),
    'Conversation': ('ScreenshotTests/', 'QuestionTests/', 'InteractionTests/testCompose',
                     'InteractionTests/testReading', 'InteractionTests/testScrolling', 'AccessibilityLayoutTests/'),
    'Attachments': ('ScreenshotTests/testConversation', 'ScreenshotTests/testComposer',
                    'InteractionTests/testCompose', 'AccessibilityLayoutTests/testSystemAccessibilityAudit'),
}


# Adding a source file or test case requires a new scope review. Until then,
# the selector falls back to full coverage rather than guessing its dependencies.
REVIEWED_CASES = '45c7fb8a9cb07eb9e820aaf008823efb6acd178dd0441115f6d4064a08a0c0d2'
REVIEWED_FEATURE_FILES = {
    'App/Features/Attachments/AttachmentModel.swift',
    'App/Features/Attachments/AttachmentViews.swift',
    'App/Features/Attachments/PhotoScene.swift',
    'App/Features/Composer/ComposerView.swift',
    'App/Features/Conversation/ConnectionWords.swift',
    'App/Features/Conversation/ConversationChrome.swift',
    'App/Features/Conversation/EdgeCuedScroll.swift',
    'App/Features/Conversation/PulseSchedule.swift',
    'App/Features/Conversation/QuestionCardView.swift',
    'App/Features/Conversation/Rows.swift',
    'App/Features/Conversation/TranscriptView.swift',
    'App/Features/Conversation/TranscriptViewportGeometry.swift',
    'App/Features/Conversation/VoiceBubble.swift',
    'App/Features/Pairing/PairingLinkSheet.swift',
    'App/Features/Pairing/Scanner.swift',
    'App/Features/Pairing/Takeovers.swift',
    'App/Features/Settings/Overlays.swift',
    'App/Features/Voice/TooShortLine.swift',
    'App/Features/Voice/VoiceChrome.swift',
}
UNIT_FAMILIES = {'Conversation': 'TranscriptViewportGeometryTests',
                 'Composer': 'TooShortLineTests', 'Voice': 'TooShortLineTests'}

def inventory(root):
    cases = {}
    for path in sorted((Path(root) / NATIVE / 'UITests').glob('*.swift')):
        text = path.read_text()
        names = re.findall(r'\b(?:final\s+)?class\s+(\w+)\s*:\s*XCTestCase', text)
        methods = re.findall(r'\bfunc\s+(test\w+)\s*\(', text)
        if methods and len(names) != 1:
            raise ValueError(f'cannot attribute UI cases in {path}')
        if names:
            cases[path.name] = [names[0] + '/' + method for method in methods]
    return cases


def select(root, paths, reviewed=True):
    """The merge gate's selection (proof-for.sh). `reviewed=False` is only for an engineer's
    workspace run (workspace_select): there the cases a branch adds are selected by the lines
    it touched, so an unreviewed new case cannot be left out and the family map still applies."""
    cases = inventory(root)
    selected = set()
    for path in paths:
        if not path.startswith(NATIVE):
            continue
        relative = path[len(NATIVE):]
        if relative.startswith('UITests/') and Path(relative).name in cases:
            selected.update(cases[Path(relative).name])
        elif relative.startswith('UnitTests/'):
            source = Path(root) / NATIVE / relative
            name = source.stem
            if not source.is_file() or not re.search(r'\b(?:struct|class)\s+' + re.escape(name) + r'\b', source.read_text()):
                return None
            selected.update(['RichOSNativeTests/' + name,
                             'RichOSNativeTests/BuildStampTests/testTheBundleCarriesItsBuildStamp'])
        elif relative in REVIEWED_FEATURE_FILES:
            names = sorted(case for group in cases.values() for case in group)
            if reviewed and hashlib.sha256('\n'.join(names).encode()).hexdigest() != REVIEWED_CASES:
                return None
            family = relative.split('/')[2]
            prefixes = FEATURES[family]
            if family in UNIT_FAMILIES:
                selected.add('RichOSNativeTests/' + UNIT_FAMILIES[family])
                selected.add('RichOSNativeTests/BuildStampTests/testTheBundleCarriesItsBuildStamp')
            selected.update(case for group in cases.values() for case in group
                            if case.startswith(prefixes))
            # ComposerView embeds VoiceChrome: a composer change can affect its gestures too.
            if family == 'Composer':
                selected.update(case for group in cases.values() for case in group
                                if case.startswith(FEATURES['Voice']))
        else:
            # Core, root wiring, shared design, project/package/build resources and
            # new input shapes can affect every feature. Never silently omit them.
            return None
    if paths and not selected:
        return None
    return sorted(selected)


# ------------------------------------------------------------------------------------------------
# An engineer's workspace run (CEO, 2026-10-01): with no selection, native-ios-ui.test.sh in a
# teammate workspace runs only the cases the branch adds or changes, plus the cases the changed
# files are claimed by, on the iPhone SE alone. `workspace_scope.py` (engine) decides WHEN; this
# decides WHAT, from the same map the merge gate uses (select above), refined to the case.
# ------------------------------------------------------------------------------------------------
SMALLEST = ('se', 'iPhone SE (3rd generation)')
FUNC = re.compile(r'^\s*(?:@\w+(?:\([^)]*\))?\s+)*(?:override\s+)?func\s+(test\w+)\s*\(')


def _code(line, in_block):
    """The line with string literals and a trailing // comment removed, and whether a \"\"\" block
    is still open after it. Braces inside strings must not move the depth."""
    out, i = [], 0
    while i < len(line):
        if in_block:
            j = line.find('"""', i)
            if j < 0:
                return ''.join(out), True
            i, in_block = j + 3, False
        elif line.startswith('"""', i):
            i, in_block = i + 3, True
        elif line[i] == '"':
            j = i + 1
            while j < len(line) and line[j] != '"':
                j += 2 if line[j] == '\\' else 1
            i = j + 1
        elif line.startswith('//', i):
            break
        else:
            out.append(line[i])
            i += 1
    return ''.join(out), in_block


def case_spans(text):
    """{test method: (first line, last line)} for one XCTestCase file, 1-based. A case's span runs
    from the comment lines directly above its `func` to the brace that closes it. None when the
    braces do not balance: then nothing in the file is attributed to a single case."""
    spans, depth, current, pending, in_block = {}, 0, None, None, False
    for n, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if current is None and depth == 1 and not in_block:
            m = FUNC.match(line)
            if m:
                current = [m.group(1), pending or n, False]
            elif stripped.startswith('//') or stripped.startswith('@'):
                pending = pending or n
            else:
                pending = None
        code, in_block = _code(line, in_block)
        depth += code.count('{') - code.count('}')
        if depth < 0:
            return None
        if current is not None:
            current[2] = current[2] or depth > 1 or ('{' in code and '}' in code)
            if current[2] and depth <= 1:
                spans[current[0]] = (current[1], n)
                current, pending = None, None
    return spans if depth == 0 and current is None else None


def workspace_select(root, paths, touched):
    """(cases, every) for the suite-claimed `paths` a workspace branch changed. `touched(path)`
    gives the changed line numbers. `cases` is the sorted selection, or None when some path is
    claimed by every case (`every` names those paths: shared code, harness, unreviewed files)."""
    cases = inventory(root)
    selected, every = set(), []
    for path in paths:
        if not path.startswith(NATIVE):
            every.append(path)  # the suite's own harness: every case runs through it
            continue
        relative = path[len(NATIVE):]
        name = Path(relative).name
        source = Path(root) / path
        if relative.startswith('UITests/') and name in cases and source.is_file():
            klass = cases[name][0].split('/')[0] if cases[name] else ''
            spans = case_spans(source.read_text())
            hit, outside = set(), spans is None
            for n in touched(path):
                owners = [c for c, (a, b) in (spans or {}).items() if a <= n <= b]
                hit.update(klass + '/' + c for c in owners)
                outside = outside or not owners
            # A line outside every case (setUp, a helper, the class itself) can change any of them.
            selected.update(cases[name] if outside or not hit else hit)
            continue
        got = select(root, [path], reviewed=False)
        if got is None:
            every.append(path)
        else:
            selected.update(got)
    return (None if every else sorted(selected)), every


def workspace_plan(root, suite, full=False, have_only=False, have_device=False, environ=None):
    """What a no-selection run of native-ios-ui.test.sh does here: a dict with
    scope (full | narrow | refuse), line (what to print), only (selectors) and device."""
    sys.path.insert(0, str(Path(root) / 'richos/engine/scripts/lib'))
    import workspace_scope
    decided = workspace_scope.decide(root, full=full, environ=environ)
    plan = {'scope': 'full', 'line': '', 'only': [], 'device': ''}
    if decided['scope'] != 'narrow' or (have_only and have_device):
        return plan
    where = 'workspace run on %s, compared with main at %s' % (decided['branch'] or 'this checkout',
                                                               decided['base'][:12])
    full_cmd = 'bash scripts/native-ios-ui.test.sh --full'
    plan['scope'] = 'narrow'
    if not have_device:
        plan['device'] = SMALLEST[0]
    if have_only:
        plan['line'] = ('native-ios-ui: %s: your --only cases on the %s only. Both devices: '
                        '--device pm as well, or everything: %s' % (where, SMALLEST[1], full_cmd))
        return plan
    declared = workspace_scope.inputs(suite)
    paths = [p for p in workspace_scope.changed(decided['root'], decided['base'])
             if workspace_scope.claimed(p, declared)]
    if not paths:
        plan['scope'] = 'refuse'
        plan['line'] = ('native-ios-ui: REFUSED: %s: the branch changes nothing this suite reads, so no '
                        'test case maps to it and nothing would run. Everything: %s ; chosen cases: '
                        '--only <Class/test>' % (where, full_cmd))
        return plan
    chosen, every = workspace_select(root, paths, lambda p: workspace_scope.lines(decided['root'],
                                                                                  decided['base'], p))
    device = SMALLEST[1] if not have_device else 'the device you chose'
    if chosen is None:
        shown = ', '.join(every[:3]) + (' and %d more' % (len(every) - 3) if len(every) > 3 else '')
        plan['line'] = ('native-ios-ui: %s: every case on the %s only, because %s can affect every case. '
                        'Both devices and everything: %s' % (where, device, shown, full_cmd))
        return plan
    plan['only'] = chosen
    plan['line'] = ('native-ios-ui: %s: %d case(s) on the %s only: %s. More: --only <Class/test>, '
                    '--device pm|pro; everything: %s' % (where, len(chosen), device, ' '.join(chosen), full_cmd))
    return plan


def main():
    if sys.argv[1:2] == ['workspace']:
        # ios_ui_scope.py workspace <root> <suite> [--full] [--have-only] [--have-device]
        root, suite, flags = sys.argv[2], sys.argv[3], set(sys.argv[4:])
        try:
            plan = workspace_plan(root, suite, full='--full' in flags, have_only='--have-only' in flags,
                                  have_device='--have-device' in flags)
        except ValueError as exc:
            print('ios_ui_scope: %s' % exc, file=sys.stderr)
            sys.exit(64)
        print('SCOPE=%s' % shlex.quote(plan['scope']))
        print('SCOPE_LINE=%s' % shlex.quote(plan['line']))
        print('SCOPE_DEVICE=%s' % shlex.quote(plan['device']))
        print('SCOPE_ONLY=(%s)' % ' '.join(shlex.quote(c) for c in plan['only']))
        return
    root, changed = sys.argv[1:]
    selected = select(root, Path(changed).read_text().splitlines())
    if selected is not None:
        print(' '.join('--only ' + shlex.quote(case) for case in selected))


if __name__ == '__main__':
    main()
