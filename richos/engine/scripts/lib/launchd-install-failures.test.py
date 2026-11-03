#!/usr/bin/env python3
"""Only fixture plists and a shell-function launchctl are used."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


class InstallFailures(unittest.TestCase):
    def test_failed_move_is_never_reported_applied(self):
        self.move_cases(False)

    def test_failed_restore_is_reported_and_pending_is_kept(self):
        self.move_cases(True)

    def move_cases(self, restore_fails):
        for loaded in (False, True):
            for move_fails in (False, True):
                if restore_fails and not (loaded and move_fails):
                    continue
                with self.subTest(loaded=loaded, move_fails=move_fails, restore_fails=restore_fails), \
                        tempfile.TemporaryDirectory(prefix='schedule-move-') as temporary:
                    root = Path(temporary)
                    old, new = root / 'schedule.plist', root / 'new.plist'
                    old.write_text('old schedule')
                    new.write_text('new schedule')
                    state = root / 'config/state'
                    state.mkdir(parents=True)
                    pending = state / 'launchd-pending-fixture.pid'
                    pending.write_text('pending')
                    if loaded:
                        (root / 'loaded').touch()
                    script = root / 'fixture.sh'
                    script.write_text('''source "$1"
launchctl() {
    printf '%s\\n' "$*" >> "$FIXTURE/calls"
    case "$1" in
        print) [ -f "$FIXTURE/loaded" ] ;;
        bootout) rm -f "$FIXTURE/loaded" ;;
        bootstrap) [ "$FAIL_RESTORE" = 0 ] || return 1; touch "$FIXTURE/loaded" ;;
        *) return 90 ;;
    esac
}
mv() {
    [ "$FAIL_MOVE" = 0 ] || return 1
    command mv "$@"
}
launchd_install fixture "$FIXTURE/schedule.plist" "$FIXTURE/new.plist"
status=$?
printf '%s:%s:%s\\n' "$status" "$LAUNCHD_INSTALL_RESULT" "$LAUNCHD_INSTALL_DETAIL"
''')
                    env = dict(os.environ, FIXTURE=str(root), FAIL_MOVE=str(int(move_fails)),
                               CLAUDE_CONFIG_DIR=str(root / 'config'), FAIL_RESTORE=str(int(restore_fails)))
                    result = subprocess.run(['bash', str(script), str(HERE / 'launchd-install.sh')],
                                            env=env, capture_output=True, text=True, check=True)
                    calls = (root / 'calls').read_text()
                    if move_fails:
                        self.assertTrue(result.stdout.startswith('2:failed:'), result.stdout)
                        self.assertEqual(old.read_text(), 'old schedule')
                        if loaded:
                            self.assertIn('bootstrap ', calls)
                            self.assertEqual((root / 'loaded').exists(), not restore_fails)
                            detail = ('previous schedule could not be restored' if restore_fails
                                      else 'previous schedule restored')
                            self.assertIn(detail, result.stdout)
                        else:
                            self.assertNotIn('bootstrap ', calls)
                        self.assertTrue(pending.exists())
                    else:
                        self.assertTrue(result.stdout.startswith('0:' + ('replaced' if loaded else 'loaded')),
                                        result.stdout)
                        self.assertEqual(old.read_text(), 'new schedule')
                        self.assertIn('bootstrap ', calls)
                        self.assertFalse(pending.exists())


if __name__ == '__main__':
    unittest.main()
