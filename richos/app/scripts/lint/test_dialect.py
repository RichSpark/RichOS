from pathlib import Path
import tempfile
import unittest
from common import Refusal
import dialect
from dialect import scan

ROOT = Path(__file__).resolve().parents[4]


def british(american):
    """The table's British form of an American word. Read from the table, never typed here,
    so this file carries no word the engine's Write hook would change (its own catch C8)."""
    module, table = dialect._fixer(ROOT / "richos/engine")
    return next(k for k, (v, _) in sorted(table.items()) if v == american and len(k) == len(american) + 1)


class DialectAdapter(unittest.TestCase):
    def test_red_green_and_quote(self):
        word = british("color")
        path = 'richos/app/lint-probe.md'
        self.assertEqual(scan(ROOT, path, f'The {word} is blue.'), [f'{word} -> color'])
        self.assertEqual(scan(ROOT, path, 'The color is blue.'), [])
        self.assertEqual(scan(ROOT, path, f'> The {word} is blue.'), [])
        self.assertEqual(scan(ROOT, 'richos/app/fixtures/probe.md', f'The {word} is blue.'), [])

    def test_counts_what_the_hook_fixes_and_what_it_refuses(self):
        word = british("color")
        text = f'# the {word} of it\n{word}=1\n'
        self.assertEqual(scan(ROOT, 'richos/app/scripts/lint-probe.sh', text),
                         [f'{word} -> color', f'{word} -> color'])

    def test_undeclared_locale_is_not_clean(self):
        with tempfile.TemporaryDirectory(prefix='lint-dialect-') as tmp:
            entity = Path(tmp)
            (entity / 'orchestration.config').write_text('DIALECT_TARGET=""\n')
            with self.assertRaises(Refusal):
                scan(ROOT, 'richos/app/lint-probe.md', 'The color is blue.', entity=entity)

    def test_reads_the_shared_fixer_not_the_hook(self):
        source = Path(dialect.__file__).read_text()
        self.assertIn('scripts/lib/dialect/dialect-fix.py', source)
        self.assertNotIn('bash", "-x', source)
        self.assertNotIn('dialect-guard.sh"]', source)
