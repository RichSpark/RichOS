#!/usr/bin/env python3
"""dictation-sheet-walk.py's verdicts and the words it reads, offline. The live walk is its own proof."""
import importlib.util
from pathlib import Path
import plistlib
import unittest

HERE = Path(__file__).resolve().parent.parent
APP = HERE.parent.parent
spec = importlib.util.spec_from_file_location('dictation_sheet_walk', HERE / 'dictation-sheet-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)


class TheWordsItReads(unittest.TestCase):
    """A walk that looks for words the product no longer draws fails for the wrong reason; each
    is checked against the source it comes from."""

    def test_iris_sentence_is_the_microphone_usage_string(self):
        plist = plistlib.loads((APP / 'src-tauri/Info.plist').read_bytes())
        sentence = plist['NSMicrophoneUsageDescription']
        self.assertEqual(sentence, 'So you can talk instead of typing. I listen only when you ask me to.')
        for fragment in walk.IRIS_FRAGMENTS:
            self.assertIn(fragment, sentence)

    def test_the_sheet_lines_are_the_ones_dictation_js_draws(self):
        source = (APP / 'ui/dictation.js').read_text()
        self.assertIn('On. Tap ${kcap(k)} ' + walk.ON_RUN, source)
        self.assertIn(walk.DENIED_LINE, source)
        # The second copy says round 19's On line (the key's name is a template in the source).
        self.assertIn('[`On. Tap ${keyName()} to talk`, false]', source)
        self.assertEqual(walk.OTHER_SHEET, 'On. Tap F1 ' + walk.ON_RUN)
        self.assertNotIn('On even when RichOS is closed.', source, 'line 1 is drawn nowhere (the CEO, 2026-10-08)')


class TheSteps(unittest.TestCase):
    def test_each_step_is_defined_once(self):
        """A helper given a step's method name replaces the step or is replaced by it (walk
        490c9ae819b3: a helper named open_settings was shadowed by the open-settings step, so
        opening the sheet pressed Open System Settings)."""
        source = (HERE / 'dictation-sheet-walk.py').read_text()
        for step in walk.STEPS:
            name = step.replace('-', '_')
            self.assertTrue(hasattr(walk.SheetWalk, name), name)  # here or inherited (identity)
            self.assertLessEqual(source.count(f'    def {name}(self'), 1, name)

    def test_buttons_go_through_click_not_the_key_press(self):
        """DictationWalk.press(key) posts the dictation key and shadows adopt-walk's
        press(title); a button title passed to it fails with "invalid literal for int()"
        (walk-8e2679058844: self.press('Not now'))."""
        import ast
        tree = ast.parse((HERE / 'dictation-sheet-walk.py').read_text())
        titled = [n.lineno for n in ast.walk(tree)
                  if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and n.func.attr == 'press'
                  and isinstance(n.func.value, ast.Name) and n.func.value.id == 'self'
                  and n.args and isinstance(n.args[0], ast.Constant) and isinstance(n.args[0].value, str)
                  and not n.args[0].value.isdigit() and n.args[0].value not in ('brightness',)]
        self.assertEqual(titled, [])


class TheVerdicts(unittest.TestCase):
    def test_the_untitled_switch_is_the_one_on_the_name_row(self):
        # walk-943b91d5720b: five AXCheckBox nodes with no title; the rows as the frame showed
        # them (osascript 238, RichOS 280, sshd-keygen-wrapper 322, ...).
        boxes = [{'role': 'AXCheckBox', 'x': 1140, 'y': y - 10, 'w': 26, 'h': 20, 'value': i % 2} for i, y in enumerate((238, 280, 322, 364, 406))]
        name = {'role': 'AXStaticText', 'value': 'RichOS', 'x': 758, 'y': 271, 'w': 44, 'h': 18}
        self.assertTrue(walk.names_richos(name), 'a static text says its string in AXValue (walk-350622c946bf)')
        self.assertTrue(walk.names_richos({'role': 'AXStaticText', 'title': 'RichOS'}))
        self.assertFalse(walk.names_richos({'role': 'AXStaticText', 'value': 'osascript'}))
        self.assertEqual(walk.switch_on_the_row([name], boxes), [boxes[1]])
        self.assertEqual(walk.center_of(boxes[1]), (1153.0, 280.0))
        far = dict(name, y=500)
        self.assertEqual(walk.switch_on_the_row([far], boxes), [], 'no switch within half a row')
        self.assertEqual(walk.switch_on_the_row([], boxes), [])
        # walk-ba7a4420692c: nothing in the pane names a row, and exactly one switch is off.
        self.assertEqual(walk.the_only_switch_off(boxes), [], 'values 0,1,0,1,0 have three off: no')
        seen = [dict(b, value=v) for b, v in zip(boxes, ('1', '0', '1', '1', '1'))]
        self.assertEqual(walk.the_only_switch_off(seen), [seen[1]])
        self.assertEqual(walk.the_only_switch_off([seen[1]]), [], 'one box alone is no list')

    def test_within_two_seconds_allows_one_round_trip(self):
        self.assertTrue(walk.grant_seconds_ok(0.4))
        self.assertTrue(walk.grant_seconds_ok(2.9))
        self.assertFalse(walk.grant_seconds_ok(3.2))
        self.assertFalse(walk.grant_seconds_ok(None))

    def test_the_steps_start_from_no_grants_and_end_with_the_second_copy(self):
        self.assertEqual(walk.STEPS[:3], ['identity', 'stage', 'relaunch'])
        self.assertLess(walk.STEPS.index('mic-prompt'), walk.STEPS.index('ax-prompt'))
        self.assertLess(walk.STEPS.index('ax-prompt'), walk.STEPS.index('granted'))
        self.assertEqual(walk.STEPS[-1], 'second-copy')

    def test_the_grant_rows_name_only_this_app(self):
        sql = walk.GRANT.format(svc='kTCCServiceAccessibility')
        self.assertIn("'kTCCServiceAccessibility', 'com.richos.app'", sql)


if __name__ == '__main__':
    unittest.main()
