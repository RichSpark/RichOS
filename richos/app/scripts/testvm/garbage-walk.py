#!/usr/bin/env python3
"""garbage-walk.py — what the regular app leaves behind after ordinary use, a quit and a forced kill.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      garbage-walk.py --out DIR --expect-sha SHA [--gone PATTERN ...] [--wav SPOKEN.wav --model GGML.bin]

THE QUESTION (CEO §54 addendum 2, 2026-09-18): "The RichOS app must always clean up its garbage
automatically, no matter what." Which files and folders does the app create and not remove, and
does a later start, a settled job or a quit remove them; a crash or a force-quit at the next start.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65). stu-reminder-walk.py's
identity, first-run, job (a job for Stu that needs a teammate, SETTLED) and idle steps, unchanged,
then the app's own Quit, a restart, a SIGKILL of the app by its recorded pid, and a restart again:

  mark          a marker dated two seconds before the running app's own start: everything the
                listings report was written after the app started
  list-*        files-since.py over the app's HOME, /private/tmp, the guest user's own
                /private/var/folders directory, the guest's real ~/Library and /Users/Shared:
                every file, link and newly born directory written since the mark that still exists,
                with its size, as listing-<name>.json, grouped by place in the step's evidence; and
                the processes still running from this payload or named claude / provider-supervisor
  quit          the app's Quit (the menu bar item, main.rs MENU_QUIT); "Quit and stop the work" if
                asked; the recorded pid gone within --quit-within seconds
  restart       relaunch.py; the composer back; then --settle seconds for start-up work
  kill          SIGKILL to the recorded pid, verified to be this payload's app first
  --gone        a regular expression for a kind of path the app must clean up. list-quit FAILS
                naming each matching path the running app had that is still there after its Quit;
                list-final each matching path the killed run left that is still there after the
                next start. What the run now running made is its own. Without --gone the walk only
                reports (the discovery run).

A positive control (a file named garbage-walk-control, written in the app's HOME after the mark) must
be found by every listing, or the listing FAILS: a search that cannot see its own control proves
nothing about what it did not report.

THE LEFTOVERS THAT NEED A KILL AT THE WRONG MOMENT (CEO 2026-10-25: "proves the cleanup runs
automatically ... in the app just as well without any ifs and buts"), with --wav and --model:

  plant-live    a recording of the RUNNING app, voice-scratch/utt-<its pid>.wav (32044 bytes, one
                second as stt.rs writes it). A real one lives only while the decoder runs, seconds,
                and no sweep is timed inside them, so this one stands in for it: same name, same
                owner, same size. list-running FAILS unless a sweep ran while that app was running
                (a "cleanup at settled" line after the plant) and the recording and that app's
                profiles (its owner record in them) are still there: a live recording and a profile
                in use are kept. Its
                own Quit takes it (list-quit judges it with the other kinds)
  stage-voice   voice-walk.py's: the microphone grant, the spoken sample and the speech model in
  restart       relaunched with the sample as the microphone (capture.rs RICHOS_VOICE_INPUT_WAV),
                until the app says voice is ready
  kill          A FORCE-QUIT DURING A SPOKEN TURN: the talk control pressed, and a watcher in the
                guest sends SIGKILL to the recorded app pid the moment the decoder is running on
                voice-scratch/utt-<pid>.wav; the recording is still there after
  plant         the killed run's leftovers put in the two states a sweep must also handle: one of
                its profile pairs SPLIT (the base removed, as a sweep out of time leaves it; the
                -standing copy must carry its own owner record), and a pair of the killed app's
                whose base holds a folder nobody can read, so it cannot be deleted
  reported      the start sweep after the kill says FAILED in the app's log (failures >= 1), the
                engine's deletion log has the abandoned recording, the split copy and the unreadable
                pair's copy DELETED and its base FAILED, and that base is still there (made after
                listing-kill, it is not in list-final's judgment; this step judges it)

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
from collections import defaultdict
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
_spec = importlib.util.spec_from_file_location('stu_reminder_walk', HERE / 'stu-reminder-walk.py')
srw = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(srw)
_spec = importlib.util.spec_from_file_location('voice_walk', HERE / 'voice-walk.py')
vw = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(vw)
from relaunch import guest  # noqa: E402

StepFailed = srw.StepFailed
CONTROL = 'garbage-walk-control'
STEPS = ['identity', 'mark', 'first-run', 'plant-live', 'job', 'idle', 'list-running', 'quit', 'list-quit',
         'stage-voice', 'restart', 'list-restart', 'kill', 'list-kill', 'plant', 'restart-after-kill',
         'list-final', 'reported']
# Without --wav: the walk as it was, a SIGKILL at an idle moment and nothing planted.
VOICE_STEPS = ('plant-live', 'stage-voice', 'plant', 'reported')
CLEANUP = '[richos] cleanup at '
# One second of 16 kHz mono 16-bit silence, as stt.rs transcribe_inner writes a second of speech.
WAV = ('import os,sys,wave\n'
       'os.makedirs(os.path.dirname(sys.argv[1]),exist_ok=True)\n'
       'w=wave.open(sys.argv[1],"wb");w.setnchannels(1);w.setsampwidth(2);w.setframerate(16000)\n'
       'w.writeframes(b"\\0\\0"*16000);w.close()\n'
       'print(os.stat(sys.argv[1]).st_size)\n')
# The force-quit during a spoken turn, run in the guest: SIGKILL to the recorded app pid the moment
# a decoder runs on that app's recording. Polls every 10 ms, and reads the process table only once
# the recording exists. Writes what it saw to argv[3].
WATCH = ('import json,os,subprocess,sys,time\n'
         'pid=int(sys.argv[1]);wav=sys.argv[2];out=sys.argv[3];end=time.time()+float(sys.argv[4])\n'
         'while time.time()<end:\n'
         '    if os.path.exists(wav):\n'
         '        ps=subprocess.run(["ps","-axo","pid=,command="],capture_output=True,text=True).stdout\n'
         '        dec=[l.strip() for l in ps.splitlines() if "whisper-cli" in l and wav in l]\n'
         '        if dec:\n'
         '            st=os.stat(wav)\n'
         '            os.kill(pid,9)\n'
         '            json.dump({"killed_at":time.time(),"wav_bytes":st.st_size,"wav_mtime":st.st_mtime,\n'
         '                       "decoder":dec[0][:300]},open(out,"w"))\n'
         '            sys.exit(0)\n'
         '    time.sleep(0.01)\n'
         'json.dump({"timeout":True},open(out,"w"))\n')
# The owner record of every folder under engine-profiles (app_cleanup.rs OWNER_FILE), or null.
OWNERS = ('import json,os,sys\n'
          'root=sys.argv[1];out={}\n'
          'for n in sorted(os.listdir(root)) if os.path.isdir(root) else []:\n'
          '    try: out[n]=json.load(open(os.path.join(root,n,".richos-owner.json")))\n'
          '    except (OSError,ValueError): out[n]=None\n'
          'print(json.dumps(out))\n')
# Which earlier listing a listing judges --gone against: after the Quit, what the running app had;
# after the restart that follows the kill, what the killed run left.
BEFORE = {'quit': 'running', 'final': 'kill'}
# The processes still running that this walk reports: this payload's, and any claude or supervisor.
# The probe leaves out itself and the shell that ran it, whose command lines carry the same words.
PROCS = ('import json,os,subprocess,sys\n'
         'me={os.getpid(),os.getppid()}\n'
         'out=subprocess.run(["ps","-axo","pid=,ppid=,command="],capture_output=True,text=True).stdout\n'
         'rows=[]\n'
         'for l in out.splitlines():\n'
         '    p=l.split(None,2)\n'
         '    if len(p)<3 or int(p[0]) in me: continue\n'
         '    c=p[2]\n'
         '    if sys.argv[1] in c or "provider-supervisor" in c or "/claude" in c.split(" ")[0]:\n'
         '        rows.append({"pid":int(p[0]),"ppid":int(p[1]),"command":c[:300]})\n'
         'print(json.dumps(rows))\n')


def place(path, roots):
    """The leftover's place: its root and up to three names below it."""
    for root in sorted(roots, key=len, reverse=True):
        if path == root or path.startswith(root.rstrip('/') + '/'):
            rel = path[len(root):].strip('/').split('/')
            return root.rstrip('/') + '/' + '/'.join(rel[:3])
    return path


class GarbageWalk(srw.StuReminderWalk):
    def tool(self):
        return self.payload + '/garbage-walk/files-since.py'

    def recorded_pid(self):
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / self.vm
        pid = (state / 'app.pid').read_text().strip()
        if not pid.isdigit():
            raise StepFailed('the recorded app pid is not a number: %r' % pid)
        return int(pid)

    def mark(self):
        pid = self.recorded_pid()
        made = self.payload + '/garbage-walk'
        guest(self.vm, 'mkdir -p ' + shlex.quote(made), 60)
        srw.command_walk.command([HERE / 'guest.sh', self.vm, '--push', HERE / 'files-since.py', made + '/'], 300)
        # The app's own start, read from the process: the mark is two seconds before it.
        script = ('import os,subprocess,sys,time\n'
                  'pid=sys.argv[1]\n'
                  'lst=subprocess.run(["ps","-o","lstart=","-p",pid],capture_output=True,text=True,'
                  'env=dict(os.environ,LC_ALL="C")).stdout.strip()\n'
                  'start=time.mktime(time.strptime(lst,"%a %b %d %H:%M:%S %Y"))\n'
                  'm=sys.argv[2]\n'
                  'open(m,"a").close(); os.utime(m,(start-2,start-2))\n'
                  'print(start)\n')
        started = float(guest(self.vm, 'python3 -c %s %s %s' % (
            shlex.quote(script), pid, shlex.quote(made + '/mark')), 60))
        uid = guest(self.vm, 'id -u', 30).strip()
        user_tmp = guest(self.vm, 'getconf DARWIN_USER_TEMP_DIR', 30).strip().rstrip('/')
        # /private/var/folders/xx/yyyy/T -> the user's own folder, holding T, C and 0.
        user_folders = os.path.dirname(os.path.realpath(user_tmp)) if user_tmp else ''
        if not user_folders.startswith('/var/folders/') and not user_folders.startswith('/private/var/folders/'):
            raise StepFailed('the guest user\'s temporary folder is not under /var/folders: %r' % user_tmp)
        if not user_folders.startswith('/private'):
            user_folders = '/private' + user_folders
        real_home = guest(self.vm, 'echo "$HOME"', 30).strip()
        self.facts['garbage'] = {'mark': made + '/mark', 'app_started': started, 'app_pid': pid, 'uid': uid,
                                 'roots': [self.home, '/private/tmp', user_folders, real_home + '/Library',
                                           '/Users/Shared'],
                                 'harness': [self.payload + '/garbage-walk', self.home + '/Library/Keychains']}
        self.save()
        return self.facts['garbage']

    def listing(self, name):
        g = self.facts.get('garbage')
        if not g:
            raise StepFailed('mark must have run (no marker on record)')
        control = self.home + '/' + CONTROL + '-' + name
        guest(self.vm, 'echo control > ' + shlex.quote(control), 30)
        argv = ['/usr/bin/python3', self.tool(), 'scan', '--since', g['mark'], '--control', control,
                '--named', CONTROL]
        for r in g['roots']:
            argv += ['--root', r]
        for h in g['harness']:
            argv += ['--harness', h]
        # The system folders always hold a few directories this user cannot read; the app's
        # HOME is not one of them, so an unreadable directory there still breaks the search.
        for r in g['roots'][1:]:
            argv += ['--tolerate-unreadable', r]
        raw = guest(self.vm, shlex.join(argv) + '; echo "exit=$?" >&2', 600)
        guest(self.vm, 'rm -f ' + shlex.quote(control), 30)
        out = json.loads(raw)
        (self.out / ('listing-%s.json' % name)).write_text(json.dumps(out, indent=1) + '\n')
        if out.get('controls_missing') or out.get('unreadable_roots') or out.get('unreadable_dirs'):
            raise StepFailed('the search is not working: control missing %s, unreadable roots %s, unreadable '
                             'directories %s' % (out.get('controls_missing'), out.get('unreadable_roots'),
                                                 out.get('unreadable_dirs', [])[:5]))
        rows = [r for r in out['files'] if r['class'] not in ('control', 'harness')]
        places = defaultdict(lambda: {'files': 0, 'dirs': 0, 'bytes': 0})
        for r in rows:
            p = places[place(r['path'], g['roots'])]
            p['files' if r['kind'] != 'dir' else 'dirs'] += 1
            p['bytes'] += r.get('bytes', 0)
        procs = json.loads(guest(self.vm, 'python3 -c %s %s' % (shlex.quote(PROCS), shlex.quote(self.payload)), 60) or '[]')
        summary = {'entries': len(rows), 'bytes': sum(r.get('bytes', 0) for r in rows),
                   'places': dict(sorted(places.items())), 'processes': procs}
        (self.out / ('summary-%s.json' % name)).write_text(json.dumps(summary, indent=1) + '\n')
        before = BEFORE.get(name)
        if self.a.gone and before:
            # A LEFTOVER is a path of a kind the app must clean up that was there in the listing
            # before (the running app's, before its Quit; the killed run's, before the restart) and
            # is still there now. What the run now running made is its own, not a leftover.
            try:
                earlier = {r['path'] for r in json.loads((self.out / ('listing-%s.json' % before)).read_text())['files']}
            except (OSError, ValueError, KeyError):
                raise StepFailed('listing-%s.json is needed to judge what was left since it' % before)
            kinds = [re.compile(p) for p in self.a.gone]
            # The pair plant makes after listing-kill is not in it, so `reported` judges it.
            judged = sorted(p for p in earlier if any(k.search(p) for k in kinds))
            left = sorted(r['path'] for r in rows if r['path'] in earlier and any(k.search(r['path']) for k in kinds))
            summary['judged_since'] = before
            summary['were_there'] = judged
            summary['still_there'] = left
            (self.out / ('summary-%s.json' % name)).write_text(json.dumps(summary, indent=1) + '\n')
            if left:
                raise StepFailed('%d of %d leftover(s) from the %s listing are still there, first: %s'
                                 % (len(left), len(judged), before, left[:10]))
        return summary

    def list_running(self):
        summary = self.listing('running')
        if self.a.wav:
            summary['kept_while_running'] = self.kept_while_running()
            (self.out / 'summary-running.json').write_text(json.dumps(summary, indent=1) + '\n')
        return summary

    def log_of(self, run):
        """The app log of `run` ('first' is run.sh's own launch log)."""
        return (self.facts.get('logs') or {}).get(run) or (self.payload + '/app.log')

    def cleanup_lines(self, run):
        text = guest(self.vm, 'cat ' + shlex.quote(self.log_of(run)) + ' 2>/dev/null || true', 60)
        return [line for line in text.splitlines() if line.startswith(CLEANUP)]

    def owners(self):
        return json.loads(guest(self.vm, 'python3 -c %s %s' % (
            shlex.quote(OWNERS), shlex.quote(self.data + '/engine-profiles')), 60) or '{}')

    def plant_live(self):
        """A recording of the running app, as stt.rs names it (see the module docstring)."""
        pid = self.recorded_pid()
        path = self.data + '/voice-scratch/utt-%d.wav' % pid
        size = int(guest(self.vm, 'python3 -c %s %s' % (shlex.quote(WAV), shlex.quote(path)), 60))
        before = len([l for l in self.cleanup_lines('first') if l.startswith(CLEANUP + 'settled:')])
        self.facts['live_recording'] = {'path': path, 'pid': pid, 'bytes': size, 'settled_lines_before': before}
        self.save()
        return self.facts['live_recording']

    def kept_while_running(self):
        """After the job: a sweep ran while the app was running, and kept its recording and its
        profiles."""
        live = self.facts.get('live_recording')
        if not live:
            raise StepFailed('plant-live must have run (no live recording on record)')
        settled = [l for l in self.cleanup_lines('first') if l.startswith(CLEANUP + 'settled:')]
        swept = settled[live['settled_lines_before']:]
        if not swept:
            raise StepFailed('no sweep ran while the app was running after the plant, so nothing was shown '
                             'kept (settled lines: %s)' % settled)
        there = guest(self.vm, 'test -f %s && stat -f %%z %s || true' % ((shlex.quote(live['path']),) * 2), 30)
        if there != str(live['bytes']):
            raise StepFailed('the running app\'s recording %s did not survive its own sweeps (%r)' % (live['path'], there))
        pid = self.recorded_pid()
        if pid != live['pid']:
            raise StepFailed('the app now running (pid %d) is not the one that was planted for (%d)' % (pid, live['pid']))
        mine = sorted(n for n, o in self.owners().items() if isinstance(o, dict) and o.get('pid') == pid)
        if not mine:
            raise StepFailed('the running app (pid %d) has no profile with its owner record to show kept' % pid)
        return {'sweeps_after_plant': swept, 'recording': live['path'], 'recording_bytes': int(there),
                'profiles_of_the_running_app': mine}

    def list_quit(self):
        return self.listing('quit')

    def list_restart(self):
        return self.listing('restart')

    def list_kill(self):
        return self.listing('kill')

    def list_final(self):
        return self.listing('final')

    def gone(self, pid, seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if guest(self.vm, 'kill -0 %d 2>/dev/null && echo alive || true' % pid, 30) != 'alive':
                return True
            time.sleep(0.5)
        return False

    def quit(self):
        pid = self.recorded_pid()
        began = time.monotonic()
        # The app's own Quit item (main.rs, MENU_QUIT) is in the menu bar, which is in no window.
        self.ax('click', '--title', 'Quit RichOS', '--role', 'AXMenuItem', '--in', 'menubar', '--first')
        asked = False
        end = time.monotonic() + 15
        while time.monotonic() < end:
            if guest(self.vm, 'kill -0 %d 2>/dev/null && echo alive || true' % pid, 30) != 'alive':
                break
            try:
                if self.present('Quit and stop the work'):
                    self.press('Quit and stop the work')
                    asked = True
                    break
            except StepFailed as exc:
                # With nothing registered the app quits at once, so the app can be gone between
                # the check above and this read (garbage walk 1, walk-823596393f20).
                if 'noprocess' not in str(exc):
                    raise
                break
            time.sleep(1)
        if not self.gone(pid, self.a.quit_within):
            raise StepFailed('the app (pid %d) was still running %.0f s after its Quit' % (pid, self.a.quit_within))
        return {'pid': pid, 'asked_to_stop_work': asked, 'gone_after_s': round(time.monotonic() - began, 1)}

    def restart(self):
        """The app opened again, and seen on screen. Not CommandWalk.relaunch: it presses the
        conversation by the title first-run recorded ("Onboarding"), which the app renames after
        the job (walk-0bf5672df92a: back on screen, its conversation retitled, the press found
        nothing). A walk about leftovers needs the app running, not a conversation open."""
        return self.relaunched('restart', voice=bool(self.a.wav))

    def relaunched(self, run, voice=False):
        from relaunch import relaunch
        environment = {'RICHOS_VOICE_INPUT_WAV': self.home + '/voice-sample.wav',
                       'RICHOS_VOICE_WHISPER_MODEL_ID': self.a.model_id} if voice else None
        launched = relaunch(self.vm, environment=environment)
        self.facts.setdefault('logs', {})[run] = launched['log']
        self.save()
        home = self.after_relaunch()
        if home.get('home_on_screen_after_s') is None:
            raise StepFailed('the app (pid %s) was not on screen within %.0f s of its relaunch; see '
                             'home-after-relaunch.png' % (launched.get('pid'), self.a.relaunch_within))
        ready = self.voice_ready(launched['log']) if voice else None
        time.sleep(self.a.settle)
        return dict(launched, **home, settled_s=self.a.settle, voice_ready=ready)

    def voice_ready(self, log):
        """The app's own readiness line (voice-walk.py READY), so the press listens at once."""
        end = time.monotonic() + 300
        while time.monotonic() < end:
            text = guest(self.vm, 'cat ' + shlex.quote(log) + ' 2>/dev/null || true', 60)
            lines = [l for l in text.splitlines() if vw.READY in l or vw.NOT_READY in l]
            if lines:
                if vw.NOT_READY in lines[-1]:
                    raise StepFailed(lines[-1])
                return lines[-1]
            time.sleep(2)
        raise StepFailed('the app printed no voice-readiness line within 300 s of its relaunch')

    def restart_after_kill(self):
        return self.relaunched('restart-after-kill')

    def stage_voice(self):
        """voice-walk.py's microphone grant, and the sample and the model copied in (never fetched)."""
        microphone = vw.VoiceWalk.grant_microphone(self)
        srw.command_walk.command([HERE / 'guest.sh', self.vm, '--push', self.a.wav, self.home + '/voice-sample.wav'], 120)
        models = self.home + '/.config/richos/models'
        guest(self.vm, 'mkdir -p ' + shlex.quote(models), 30)
        target = '%s/ggml-%s.bin' % (models, self.a.model_id)
        srw.command_walk.command([HERE / 'guest.sh', self.vm, '--push', self.a.model, target], 900)
        return {'microphone_grant': microphone, 'model': target,
                'model_sha256': guest(self.vm, 'shasum -a 256 ' + shlex.quote(target), 120).split()[0]}

    # voice-walk.py's, unchanged: the guest's own output muted, and its DOM-id reads and presses.
    mute_guest_output = vw.VoiceWalk.mute_guest_output
    by_id = vw.VoiceWalk.by_id
    press_by_id = vw.VoiceWalk.press_by_id

    def kill(self):
        pid = self.recorded_pid()
        rows = {r['pid'] for r in self.app_rows()}
        if pid not in rows:
            raise StepFailed('the recorded pid %d is not this payload\'s app (running: %s)' % (pid, sorted(rows)))
        if self.a.wav:
            return self.kill_while_decoding(pid)
        guest(self.vm, 'kill -KILL %d' % pid, 30)
        if not self.gone(pid, 10):
            raise StepFailed('the app (pid %d) survived SIGKILL for 10 s' % pid)
        self.facts['killed_pid'] = pid
        self.save()
        return {'pid': pid, 'signal': 'KILL'}

    def kill_while_decoding(self, pid):
        """A force-quit during a spoken turn: the talk control pressed once, and the guest's
        watcher (WATCH) sends SIGKILL to `pid`, this payload's app, the moment the decoder runs on
        its recording. CEO §53: the sample is a file standing in for the microphone and the guest's
        output is muted; nothing is played anywhere, and the turn never reaches Rich."""
        muted = self.mute_guest_output()
        wav = self.data + '/voice-scratch/utt-%d.wav' % pid
        seen = self.payload + '/garbage-walk/kill-while-decoding.json'
        guest(self.vm, 'rm -f %s; nohup /usr/bin/python3 -c %s %d %s %s %d >/dev/null 2>&1 &' % (
            shlex.quote(seen), shlex.quote(WATCH), pid, shlex.quote(wav), shlex.quote(seen), int(self.a.within)), 30)
        end = time.monotonic() + 60
        while not self.by_id('talk-toggle'):
            if time.monotonic() > end:
                self.shot('talk-missing.png')
                raise StepFailed('the talk control (#talk-toggle) is not on screen within 60 s')
            if self.by_id('home-enter'):
                self.press_by_id('home-enter')
            time.sleep(1)
        self.press_by_id('talk-toggle')  # once; the watcher says what it did
        end = time.monotonic() + self.a.within + 10
        watched = {}
        while time.monotonic() < end:
            text = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(seen), 30)
            if text:
                watched = json.loads(text)
                break
            time.sleep(1)
        if not watched or watched.get('timeout'):
            log = guest(self.vm, 'grep -F "[richos-voice]" %s | tail -20 || true' % shlex.quote(self.log_of('restart')), 60)
            raise StepFailed('no decoder ran on %s within %.0f s of the press, so nothing was killed; voice log:\n%s'
                             % (wav, self.a.within, log))
        if not self.gone(pid, 10):
            raise StepFailed('the app (pid %d) survived SIGKILL for 10 s' % pid)
        left = guest(self.vm, 'test -f %s && stat -f %%z %s || true' % ((shlex.quote(wav),) * 2), 30)
        if not left.isdigit():
            raise StepFailed('the killed app left no recording at %s: the kill missed the spoken turn' % wav)
        self.facts['killed_pid'] = pid
        self.facts['abandoned_recording'] = wav
        self.save()
        return {'pid': pid, 'signal': 'KILL', 'guest_output_muted': muted, 'watcher': watched,
                'recording': wav, 'recording_bytes_after': int(left)}

    def plant(self):
        """The killed run's profiles in the two states a sweep must also handle (module docstring)."""
        killed = self.facts.get('killed_pid')
        if not killed:
            raise StepFailed('kill must have run (no killed pid on record)')
        root = self.data + '/engine-profiles'
        owners = self.owners()
        pairs = [n for n in owners if not n.endswith('-standing') and n + '-standing' in owners
                 and isinstance(owners[n], dict) and owners[n].get('pid') == killed]
        if not pairs:
            raise StepFailed('the killed app (pid %d) left no profile pair to split: %s' % (killed, owners))
        base = pairs[0]
        standing_owner = owners[base + '-standing']
        if not isinstance(standing_owner, dict) or standing_owner.get('pid') != killed:
            raise StepFailed('the -standing copy of %s carries no owner record of its own: %r' % (base, standing_owner))
        guest(self.vm, 'rm -rf ' + shlex.quote(root + '/' + base), 60)
        lease = str(uuid.uuid4())
        stuck = root + '/' + lease
        record = json.dumps(owners[base])
        guest(self.vm, ' && '.join([
            'mkdir -p %s %s' % (shlex.quote(stuck + '/agents/locked'), shlex.quote(stuck + '-standing/agents')),
            'printf %%s %s > %s' % (shlex.quote(record), shlex.quote(stuck + '/.richos-owner.json')),
            'printf %%s %s > %s' % (shlex.quote(record), shlex.quote(stuck + '-standing/.richos-owner.json')),
            'echo an agent > %s' % shlex.quote(stuck + '/agents/locked/rich.md'),
            'chmod 000 %s' % shlex.quote(stuck + '/agents/locked')]), 30)
        self.facts['planted'] = {'split_standing': root + '/' + base + '-standing', 'split_base_removed': root + '/' + base,
                                 'standing_owner_record': standing_owner, 'undeletable_base': stuck,
                                 'undeletable_standing': stuck + '-standing', 'owner_record': owners[base]}
        self.save()
        return self.facts['planted']

    def reported(self):
        """The start sweep after the kill: FAILED in the app's log, and what the engine logged."""
        planted = self.facts.get('planted')
        if not planted:
            raise StepFailed('plant must have run (nothing planted on record)')
        lines = [l for l in self.cleanup_lines('restart-after-kill') if l.startswith(CLEANUP + 'start:')]
        if len(lines) != 1:
            raise StepFailed('the restart after the kill logged %d start sweep line(s): %s' % (len(lines), lines))
        line = lines[0]
        failures = re.search(r' failures (\d+) ', line)
        if not line.startswith(CLEANUP + 'start: FAILED, ') or not failures or int(failures.group(1)) < 1:
            raise StepFailed('a sweep that could not delete the unreadable base was logged as: ' + line)
        deletions = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(self.home + '/.claude/state/scratch-reaper.log'), 60)
        (self.out / 'reaper-log.txt').write_text(deletions + '\n')
        want = {'DELETED ' + planted['split_standing'] + ' ', 'DELETED ' + planted['undeletable_standing'] + ' ',
                'FAILED ' + planted['undeletable_base'] + ' '}
        if self.facts.get('abandoned_recording'):
            want.add('DELETED ' + self.facts['abandoned_recording'] + ' ')
        missing = sorted(w for w in want if w not in deletions)
        if missing:
            raise StepFailed('the engine\'s deletion log is missing %s' % missing)
        stuck = guest(self.vm, 'test -d %s && echo there || true' % shlex.quote(planted['undeletable_base']), 30)
        if stuck != 'there':
            raise StepFailed('the unreadable base is gone, so nothing failed: ' + planted['undeletable_base'])
        return {'start_line': line, 'failures': int(failures.group(1)),
                'deletion_log': sorted(l for l in deletions.splitlines() if any(w.strip() in l for w in want))}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--task', default=srw.JOB)
    p.add_argument('--within', type=float, default=300)
    p.add_argument('--job-within', type=float, default=1200)
    p.add_argument('--idle-within', type=float, default=900)
    p.add_argument('--approvals', type=int, default=3)
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--quit-within', type=float, default=30, help='seconds for the app to be gone after its Quit')
    p.add_argument('--settle', type=float, default=60, help='seconds after a restart before the listing')
    p.add_argument('--gone', action='append', default=[],
                   help='regular expression for a kind of guest path the app must clean up: after the Quit '
                        'and after the restart that follows the kill, none that was there before may remain')
    p.add_argument('--gone-file', type=Path,
                   help='a file of --gone expressions, one per line (blank lines and # lines skipped)')
    p.add_argument('--wav', type=Path,
                   help='a short spoken sample, 16 kHz mono WAV (say -o, then afconvert): with --model, the kill '
                        'is a force-quit during a spoken turn and the planted cases run')
    p.add_argument('--model', type=Path, help='the GGML speech weights to copy in, matching --model-id')
    p.add_argument('--model-id', default='large-v3-turbo-q5_0')
    p.add_argument('--steps', default=None)
    a = p.parse_args()
    if bool(a.wav) != bool(a.model):
        p.error('--wav and --model go together')
    if a.steps is None:
        a.steps = ','.join(s for s in STEPS if a.wav or s not in VOICE_STEPS)
    if a.gone_file:
        a.gone += [line.strip() for line in a.gone_file.read_text().splitlines()
                   if line.strip() and not line.lstrip().startswith('#')]
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = GarbageWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'gone': a.gone, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, KeyError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            try:
                walk.shot('failed-%s.png' % step)
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired):
                pass
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
