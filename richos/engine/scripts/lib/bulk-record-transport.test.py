"""Large real records must retain CEO items and existing work claims."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ENGINE = Path(os.environ.get('RICHOS_BULK_RECORD_TEST_ROOT', Path(__file__).resolve().parents[2]))


class BulkRecords(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='bulk-records-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        self.repo = self.root / 'repo'
        (self.repo / 'wiki').mkdir(parents=True)
        (self.root / 'home').mkdir()
        self.env = {'PATH': os.environ['PATH'], 'HOME': str(self.root / 'home'),
                    'LANG': 'C', 'LC_ALL': 'C', 'TMPDIR': str(self.root)}
        self.git('init', '-q', '-b', 'main')
        self.git('config', 'user.name', 'Fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.git('config', 'core.hooksPath', str(self.root / 'no-hooks'))
        (self.repo / '.ceo-todos').write_text('TODO_RECORD="wiki/open-items.md"\nTODO_VIEW="CEO-TODOs.md"\nROOT_README="README.md"\nCEO_SECTIONS="1"\nPREPARER_SECTION="3"\nARTIFACT_ROOTS="self=."\n')
        (self.repo / '.row-currency').write_text('ROW_SECTIONS="3"\nROW_STATUS_TOKENS="OPEN BUILT BOUNDED BLOCKED-ON-RICH CLOSED"\nROW_TERMINAL_TOKENS="CLOSED"\n')
        (self.repo / 'orchestration.config').write_text('CEO_TODOS_REPOS="."\n')
        (self.repo / 'README.md').write_text('# Fixture\n')
        (self.repo / 'CEO-TODOs.md').write_text('# View\n')

    def git(self, *args):
        return subprocess.run(['git', '-C', str(self.repo), *args], env=self.env,
                              text=True, capture_output=True, check=True).stdout.strip()

    def run_lib(self, name, command):
        result = subprocess.run([shutil.which('bash'), '-c',
                                 'set -eo pipefail; source "$1"; repo="$2"; ' + command,
                                 '_', str(ENGINE / 'scripts/lib' / name), str(self.repo)],
                                text=True, capture_output=True, env=self.env, cwd=self.repo, timeout=120)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_large_ceo_item_inventory_is_encoded_completely(self):
        title = 'detailed decision context ' * 300
        body = '# Items\n\n## 1. Waiting on the CEO\n\n'
        for index in range(1, 25):
            body += f'### 1.{index} READY-FOR-CEO — {title}\n\n- **Open:** `self/README.md`\n- **Time:** 5 minutes\n- **Done:** decision recorded\n- **Unblocks:** delivery\n\n'
        body += '## 3. Buildable now\n\nNothing.\n'
        (self.repo / 'wiki/open-items.md').write_text(body)
        self.run_lib('ceo-asks.sh', 'CA_REPOS="$repo"; ca_items_json "$repo/items.json"')
        raw = (self.repo / 'items.json').read_text()
        self.assertGreater(len(raw), 131072)
        items = json.loads(raw)
        self.assertEqual(len(items), 24)
        self.assertEqual(items[-1]['id'], '1.24')

    def test_large_claim_inventory_is_not_replaced_by_an_empty_set(self):
        (self.repo / 'wiki/open-items.md').write_text('# Items\n\n## 3. Buildable now\n\n| # | Item | State |\n|---|---|---|\n| 3.1 | Claimed work | **State:** `OPEN` |\n')
        (self.repo / 'RICH-TODOs.md').write_text('# Queue\n\n## Next\n\n| # | Item | Blocked by |\n|---|---|---|\n| ~~11~~ | ~~Landed fixture~~ | done |\n')
        self.git('add', '-A'); self.git('commit', '-qm', 'fixture')
        worker = self.root / 'worker'
        self.git('worktree', 'add', '-qb', 'worker', str(worker))
        (worker / '.claude').mkdir()
        ids = ['claim-' + str(i) + '-' + 'a' * 120 for i in range(1200)] + ['3.1']
        (worker / '.claude/row-claims.txt').write_text('\n'.join(ids) + '\n')
        self.run_lib('unstarted-rows.sh', 'ur_resolve "$repo"; ur_collect_claims; printf "%s" "$UR_CLAIMS_JSON" > "$repo/claims.json"; ur_sweep; printf "%s\\n%s\\n" "$UR_VERDICT" "$UR_N_CLAIMED" > "$repo/verdict.txt"; printf "%s" "$UR_BROKEN_REASON" > "$repo/broken.txt"')
        claims = json.loads((self.repo / 'claims.json').read_text())
        by_path = {w['path']: w['ids'] for w in claims['worktrees']}
        self.assertIn(str(worker), by_path, 'large claims disappeared')
        recorded = by_path[str(worker)]
        self.assertEqual(recorded, ids)
        self.assertGreater((self.repo / 'claims.json').stat().st_size, 131072)
        verdict, claimed = (self.repo / 'verdict.txt').read_text().splitlines()
        self.assertEqual(verdict, 'SWEPT', (self.repo / 'broken.txt').read_text())
        self.assertGreaterEqual(int(claimed), 1)

    def test_failed_claim_encoding_reports_unknown_coverage(self):
        (self.repo / 'wiki/open-items.md').write_text('# Items\n\n## 3. Buildable now\n\n| # | Item | State |\n|---|---|---|\n| 3.1 | Unstarted work | **State:** `OPEN` |\n')
        (self.repo / 'RICH-TODOs.md').write_text('# Queue\n\n## Next\n\n| # | Item | Blocked by |\n|---|---|---|\n| ~~11~~ | ~~Landed fixture~~ | done |\n')
        self.git('add', '-A'); self.git('commit', '-qm', 'fixture')
        fakebin = self.root / 'bin'
        fakebin.mkdir()
        wrapper = fakebin / 'python3'
        marker = str(self.root / 'fault-consumed')
        wrapper.write_text('#!' + sys.executable + '\nimport os,sys\n'
                           'if len(sys.argv)>2 and sys.argv[1]=="-c" and "seen_paths = set()" in sys.argv[2] and not os.path.exists(' + repr(marker) + '):\n'
                           ' open(' + repr(marker) + ', "w").close()\n'
                           ' print(\'{"worktrees":[]}\')\n'
                           ' raise SystemExit(91)\n'
                           'os.execv(' + repr(sys.executable) + ', [' + repr(sys.executable) + '] + sys.argv[1:])\n')
        wrapper.chmod(0o755)
        self.env['PATH'] = str(fakebin) + os.pathsep + self.env['PATH']
        result = self.run_lib('unstarted-rows.sh', 'ur_resolve "$repo"; ur_collect_claims; ur_sweep; printf "%s\\t%s\\n" "$UR_VERDICT" "$UR_BROKEN_REASON"; ur_collect_claims; ur_sweep; printf "RETRY:%s\\n" "$UR_VERDICT"')
        self.assertTrue(result.stdout.startswith('BROKEN\t'), result.stdout)
        self.assertIn('coverage is unknown', result.stdout)
        self.assertTrue(result.stdout.endswith('RETRY:UNSTARTED\n'), result.stdout)


if __name__ == '__main__':
    unittest.main()
