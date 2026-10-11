#!/usr/bin/env python3
"""video-skill-walk.py — the app's Rich loads the video skill: his session's opening frame lists
`rich-skills:watch-video` (media-tools plan, slice 4; CEO 2026-10-07).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      video-skill-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument.

THE QUESTION. The CEO, 2026-10-07: the tools are installed "for when the user tells Rich to watch
or download a Youtube video". A skill Rich never loads leaves him improvising (Frank's review of
the plan, M1), and `--plugin-dir` says nothing when a skill is not loaded (skills.rs, cell K4). So
this walk reads the claim off the wire: the `system/init` frame of a real session, as the app
journaled it.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: memory setup declined, company "Acme" registered, questions declined
  turn       one short message typed into the Mac's composer and sent; waits for the turn to end
  init       reads the raw machinery journal (<data>/machinery/<thread>/<day>.raw.jsonl) for the
             `system/init` frames, and passes when one lists `rich-skills:watch-video` in `skills`

The full watch of a video runs in the nightly's own walk once ffmpeg and the setup step land.
Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt_walk)
StepFailed = adopt_walk.StepFailed

STEPS = ['identity', 'first-run', 'turn', 'init']
WANT = 'rich-skills:watch-video'
MESSAGE = 'Please answer with one word: ready.'


def init_verdicts(raw_lines):
    """Each journaled `system/init` frame's skills, as (skills or None, truncated). A payload over
    the journal's 32 KB cap is kept as a string prefix (machinery.rs `cap_payload`); then only a
    substring answer is possible, and a miss is not evidence of absence."""
    out = []
    for line in raw_lines:
        try:
            row = json.loads(line)
        except ValueError:
            continue
        payload = row.get('payload')
        if isinstance(payload, dict) and payload.get('type') == 'system' and payload.get('subtype') == 'init':
            out.append((list(payload.get('skills') or []), False))
        elif isinstance(payload, str) and '"subtype":"init"' in payload:
            out.append(([WANT] if f'"{WANT}"' in payload else None, True))
    return out


class VideoSkillWalk(adopt_walk.Walk):
    def ledger(self):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true')
        return [json.loads(r) for r in rows.splitlines() if r.startswith('{')]

    def turn(self):
        self.type_into(MESSAGE, '--role', 'AXTextArea', '--title', 'Message to Rich')
        self.press('Send')
        end = time.monotonic() + 180
        while time.monotonic() < end:
            rows = self.ledger()
            asked = [r for r in rows if r.get('event') == 'PromptReceived' and MESSAGE in (r.get('text') or '')]
            if asked:
                turn = asked[-1]['turn_id']
                ended = [r for r in rows if r.get('turn_id') == turn and r.get('event') in ('TurnCompleted', 'TurnInterrupted')]
                if ended:
                    return {'turn': turn, 'ended': ended[-1]['event']}
            time.sleep(1)
        raise StepFailed('the turn did not end within 180 s')

    def init(self):
        # Only the lines that can be an init frame cross the wire; a raw shard holds every frame.
        raw = guest(self.vm, 'grep -hE ' + shlex.quote('subtype.{1,3}:.{1,3}init') + ' '
                    + shlex.quote(self.data) + '/machinery/*/*.raw.jsonl 2>/dev/null || true', timeout=60)
        verdicts = init_verdicts(raw.splitlines())
        (self.out / 'init-skills.json').write_text(json.dumps(
            [{'skills': s, 'truncated': t} for s, t in verdicts], indent=2) + '\n')
        if not verdicts:
            raise StepFailed('no system/init frame was journaled')
        listed = [s for s, _ in verdicts if s and WANT in s]
        if not listed:
            raise StepFailed(f'{len(verdicts)} system/init frame(s), none lists {WANT}: {verdicts}')
        return {'init_frames': len(verdicts), 'skills': listed[0]}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = VideoSkillWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
