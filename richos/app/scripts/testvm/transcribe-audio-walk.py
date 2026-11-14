#!/usr/bin/env python3
"""transcribe-audio-walk.py — after the rename of the audio skill (CEO 2026-11-14), Rich transcribes
a voice memo with `transcribe-audio`, still watches a video with `watch-video`, and his sessions
list `rich-skills:transcribe-audio` and nothing under the old name.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      transcribe-audio-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

THE QUESTION. The CEO, 2026-11-14: "To make the audio skill just as intuitive, I'd want to rename it
to "transcribe-audio". Because then, I wouldn't need to remember anything." The rename changes the
skill's name and path, never what it does (richos-hq docs/plans/2026-11-14-rename-audio-skill-to-
transcribe-audio.md, step 5). Its trap 1: if the app's folder and the terminal's are renamed apart,
the terminal's copy reaches every app session; so the init frames must name the old one under no
plugin.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65). identity, setup, first-run
and connect are video-watch-walk.py's own steps (the speech model must be installed to transcribe).
  audio   a spoken memo made with `say -o` straight to ~/Downloads/voice-memo.m4a (nothing is played,
          CEO §53), then MEMO_TASK typed. PASS needs a completed Skill call naming transcribe-audio,
          no Skill call naming the old name, and an answer with the memo's words.
  video   VIDEO_TASK typed with a 19-second video that has uploaded captions. PASS needs a completed
          Skill call naming watch-video and an answer naming --video-keyword.
  init    every journaled system/init frame read; PASS needs one listing rich-skills:transcribe-audio
          and rich-skills:watch-video, and the old name in no machinery journal line at all.
Approve is pressed when the panel offers one, at most --approvals times per step. Every app
instance is quit by run-walk.py's stop.sh (CEO §54). Exit 0 when every step passes.
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

_spec = importlib.util.spec_from_file_location('video_watch_walk', HERE / 'video-watch-walk.py')
video_watch_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(video_watch_walk)
StepFailed = video_watch_walk.StepFailed

STEPS = ['identity', 'setup', 'first-run', 'connect', 'audio', 'video', 'init']
NEW = 'transcribe-audio'
# The old name, built from parts so `git grep` for it across the repository stays empty.
OLD = 'audio-' + 'transcription'
WANT = ('rich-skills:' + NEW, 'rich-skills:watch-video')
MEMO = 'voice-memo.m4a'
SPOKEN = 'The blue pineapple meeting moves to Thursday at four.'
MEMO_KEYWORDS = ['pineapple', 'Thursday']
MEMO_TASK = 'Please transcribe the voice memo at ~/Downloads/' + MEMO + ' and tell me what it says.'
VIDEO_URL = 'https://www.youtube.com/watch?v=jNQXAC9IVRw'  # 19 s, uploaded English captions
VIDEO_TASK = 'Please watch this video and tell me what is in it: {url}'

SKILL_CALLS = r"""import glob,json,sys
out=[]
for p in glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl"):
    for l in open(p, errors="replace"):
        try: c=json.loads(l).get("callback",{})
        except Exception: continue
        if c.get("tool_name")!="Skill": continue
        if c.get("hook_event_name") not in ("PreToolUse","PostToolUse","PostToolUseFailure"): continue
        out.append({"event":c.get("hook_event_name"),"agent_id":c.get("agent_id"),
                    "input":json.dumps(c.get("tool_input") or {})[:500]})
print(json.dumps(out))
"""


def completed(calls, name):
    """The Skill calls naming `name` that completed (PostToolUse, never a PostToolUseFailure)."""
    return [c for c in calls if c['event'] == 'PostToolUse' and name in c['input']]


def named(calls, name):
    """Every Skill call naming `name`, in any state."""
    return [c for c in calls if name in c['input']]


def init_listing(raw_lines, wanted=WANT):
    """For each journaled `system/init` frame, the wanted skill names it lists. A payload over the
    journal's 32 KB cap is kept as a string prefix (machinery.rs `cap_payload`), so it is searched
    as text."""
    out = []
    for line in raw_lines:
        try:
            row = json.loads(line)
        except ValueError:
            continue
        payload = row.get('payload')
        if isinstance(payload, dict) and payload.get('type') == 'system' and payload.get('subtype') == 'init':
            skills = set(payload.get('skills') or [])
            out.append(sorted(w for w in wanted if w in skills))
        elif isinstance(payload, str) and '"subtype":"init"' in payload:
            out.append(sorted(w for w in wanted if f'"{w}"' in payload))
    return out


class TranscribeAudioWalk(video_watch_walk.VideoWatchWalk):
    def skill_calls(self):
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(SKILL_CALLS) + ' ' + shlex.quote(self.data), 60))

    def asked(self, task, skill, keywords, seconds, name):
        def done(said, runs):
            return self.answered(said, keywords) and completed(self.skill_calls(), skill)
        got = self.converse(task, done, seconds)
        calls = self.skill_calls()
        got['skill_calls'] = calls
        (self.out / f'{name}-observed.json').write_text(json.dumps(got, indent=2) + '\n')
        self.shot(f'{name}-answer.png')
        failures = []
        if not completed(calls, skill):
            failures.append(f'no Skill call naming {skill} completed: {json.dumps(calls)[:1500]}')
        if named(calls, OLD):
            failures.append(f'a Skill call named {OLD}: {named(calls, OLD)}')
        if not self.answered(got['said'], keywords):
            failures.append(f'the answer does not name {keywords}')
        if failures:
            raise StepFailed('; '.join(failures))
        return {'skill_calls': [c['input'] for c in completed(calls, skill)],
                'answer': (got['said']['words'] or ' '.join(got['said']['notices']))[-2000:],
                'approvals_pressed': got['approvals_pressed']}

    def audio(self):
        memo = self.downloads + '/' + MEMO
        guest(self.vm, 'mkdir -p ' + shlex.quote(self.downloads) + ' && say -o ' + shlex.quote(memo)
              + ' --file-format=m4af --data-format=aac ' + shlex.quote(SPOKEN), 60)
        listed = guest(self.vm, 'ls -la ' + shlex.quote(memo) + ' 2>/dev/null || true')
        if MEMO not in listed:
            raise StepFailed('the spoken memo was not made in the guest: ' + listed)
        return self.asked(MEMO_TASK, NEW, MEMO_KEYWORDS, self.a.within, 'audio')

    def video(self):
        return self.asked(VIDEO_TASK.format(url=self.a.video_url), 'watch-video', self.a.video_keyword,
                          self.a.within, 'video')

    def init(self):
        journals = shlex.quote(self.data) + '/machinery/*/*.raw.jsonl'
        raw = guest(self.vm, 'grep -hE ' + shlex.quote('subtype.{1,3}:.{1,3}init') + ' ' + journals
                    + ' 2>/dev/null || true', timeout=60)
        frames = init_listing(raw.splitlines())
        old = guest(self.vm, 'grep -ho ' + shlex.quote('[a-z-]*:' + OLD) + ' ' + journals
                    + ' 2>/dev/null | sort | uniq -c || true', timeout=60).strip()
        (self.out / 'init-skills.json').write_text(json.dumps({'frames': frames, 'old_name': old}, indent=2) + '\n')
        if not frames:
            raise StepFailed('no system/init frame was journaled')
        failures = []
        if not any(set(WANT) <= set(f) for f in frames):
            failures.append(f'no system/init frame lists both {list(WANT)}: {frames}')
        if old:
            failures.append(f'the machinery journals name {OLD}: {old}')
        if failures:
            raise StepFailed('; '.join(failures))
        return {'init_frames': len(frames), 'listing': frames, 'old_name_lines': 0}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--video-url', default=VIDEO_URL)
    p.add_argument('--video-keyword', action='append', default=None,
                   help='a word the video answer must contain (repeatable; default: elephant)')
    p.add_argument('--setup-within', type=float, default=2700, help='seconds for the video tools (about 1 GB)')
    p.add_argument('--within', type=float, default=1200, help='seconds for each answer')
    p.add_argument('--approvals', type=int, default=6, help='most Approve presses per step')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    a.video_keyword = a.video_keyword or ['elephant']
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = TranscribeAudioWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, KeyError, IndexError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    try:
        walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired) as exc:
        print(f'logs not kept: {exc}', flush=True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
