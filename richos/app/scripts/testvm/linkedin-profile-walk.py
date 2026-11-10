#!/usr/bin/env python3
"""linkedin-profile-walk.py — Rich's LinkedIn tool, as claude reaches it, in Safari and Chrome in a guest.

The job, as the CEO confirmed it on 2026-11-10: during onboarding, Rich uses the user's default
browser exactly as it is. The user is already signed in everywhere, so there's no sign-in, setup or
extra step for them. Rich quickly opens the user's LinkedIn profile and the company pages connected
to it, and grabs the info he needs, browsing LinkedIn only as much as that takes. And: "Not just
reading. Linkedin likes to hide a shitload of stuff behind those annoying "see more" links etc."

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes it:

  linkedin-standin.py --profile SAVED.html --slug SLUG --about KEY=VANITY=SAVED_ABOUT.html --out SITE
  run-walk.py --no-app --home EMPTY_DIR --engine "" --report REPORT -- \\
      linkedin-profile-walk.py --out DIR --site SITE --exe RICHOS_TAURI [--expect-sha SHA]

The guest has no LinkedIn sign-in and nobody is asked for one. SITE is the stand-in (saved pages
served from 127.0.0.1, only to a browser holding the cookie /sign-in sets, with collapsed sections
whose text Accessibility cannot read until their button is pressed). It is built in a scratch
directory from saved pages and never committed: they are real people's information.

The tool is the app's own executable, `--linkedin-mcp`, driven over stdio exactly as claude drives
it (initialize, tools/list, tools/call read_profile). Nothing runs on the host's screen (CEO ruling
§65); the browsers are the guest's.

  site         browser-page-guest.py serve-site on SITE in the guest; a plain fetch of the profile is
               sent to /authwall (the signed-out shape)
  chrome       Google Chrome installed in the guest (browser-page-walk.py's step)
  tool         the executable pushed; it answers initialize and lists exactly read_profile
  signed-out   Safari the default browser, no cookie: the tool refuses with the /authwall address
               and opens nothing more
  safari       Safari signed in (its own /sign-in): the profile with its hidden text, its own
               /details/experience/ page with its page-only and hidden text, never the
               recommendations page, and every Experience company's About page with its hidden text
  chrome-read  the same with Chrome the default browser and signed in
  cleanup      the server stopped; the guest and every browser in it end with run-walk.py's stop.sh

Exit 0 when every step passes. DIR/report.json is the report; DIR/<browser>-reply.json the tool's
whole answer (it holds the stand-in's saved text: keep it out of any public repository).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

PORT = 18766
TOOL = 'read_profile'
STEPS = ['site', 'chrome', 'tool', 'signed-out', 'safari', 'chrome-read', 'cleanup']
SAFARI, CHROME = 'com.apple.Safari', 'com.google.Chrome'


class StepFailed(Exception):
    pass


def command(argv, timeout=120):
    r = subprocess.run(list(map(str, argv)), capture_output=True, text=True, timeout=timeout)
    if r.returncode:
        raise StepFailed('command failed (%d): %s\n%s%s' % (r.returncode, ' '.join(map(str, argv)), r.stdout, r.stderr))
    return r.stdout


def browser_page_walk():
    spec = importlib.util.spec_from_file_location('browser_page_walk', HERE / 'browser-page-walk.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Walk:
    def __init__(self, a):
        import os
        self.a, self.vm = a, a.vm
        a.out.mkdir(parents=True, exist_ok=True)
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / a.vm
        self.payload = (state / 'payload').read_text().strip()
        self.helper = self.payload + '/browser-page-guest.py'
        self.site_dir = self.payload + '/standin'
        self.exe = self.payload + '/richos-tauri'
        self.base = f'http://127.0.0.1:{PORT}'
        self.manifest = json.loads((a.site / 'site.json').read_text())
        self.profile = f"{self.base}/in/{self.manifest['slug']}/"

    def g(self, cmd, timeout=60):
        return guest(self.vm, cmd, timeout)

    def mcp(self, *calls, timeout=600):
        """Frames to the tool's stdin, its replies by id (the server ends at end of input)."""
        frames = [{'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {}},
                  {'jsonrpc': '2.0', 'method': 'notifications/initialized'}]
        frames += [dict(c, jsonrpc='2.0', id=10 + n) for n, c in enumerate(calls)]
        text = ''.join(json.dumps(f) + '\n' for f in frames)
        began = time.monotonic()
        out = self.g(f'printf %s {shlex.quote(text)} | {shlex.quote(self.exe)} --linkedin-mcp', timeout)
        replies = {r.get('id'): r for r in (json.loads(l) for l in out.splitlines() if l.strip().startswith('{'))}
        return replies, round(time.monotonic() - began, 1)

    def read(self):
        replies, seconds = self.mcp({'method': 'tools/call', 'params': {'name': TOOL, 'arguments': {'url': self.profile}}})
        result = replies.get(10, {}).get('result')
        if not result:
            raise StepFailed(f'no tools/call reply: {replies}')
        return result, seconds

    def default(self, bundle):
        row = json.loads(self.g(f'python3 {shlex.quote(self.helper)} default-browser --bundle {bundle} || true', 90).splitlines()[-1])
        if row.get('after') != bundle.lower():
            raise StepFailed(f'{bundle} could not be made the default browser in the guest: {row}')
        return row

    # --- steps --------------------------------------------------------------------------------
    def site(self):
        archive = self.a.out / 'standin.tar'
        command(['tar', '-C', self.a.site, '-cf', archive, '.'])
        command([HERE / 'guest.sh', self.vm, '--push', HERE / 'browser-page-guest.py', self.helper])
        command([HERE / 'guest.sh', self.vm, '--push', archive, self.payload + '/standin.tar'], 600)
        archive.unlink()
        self.g(f'mkdir -p {self.site_dir} && tar -C {self.site_dir} -xf {self.payload}/standin.tar && rm {self.payload}/standin.tar')
        self.g(f'nohup python3 {shlex.quote(self.helper)} serve-site --port {PORT} --dir {self.site_dir} '
               f'>{self.site_dir}.log 2>&1 &')
        for _ in range(30):
            where = self.g(f'curl -s -o /dev/null -w %{{redirect_url}} {self.profile} || true')
            if where:
                break
            time.sleep(0.5)
        if '/authwall' not in where:
            raise StepFailed(f'a plain fetch of the profile was not sent to the sign-in wall: {where!r}')
        return {'profile': self.profile, 'plain_fetch_redirect': where, 'experience_keys': self.manifest['experience_keys']}

    def chrome(self):
        bp = browser_page_walk()
        shim = argparse.Namespace(vm=self.vm, out=self.a.out)
        return bp.Walk(shim).chrome()

    def tool(self):
        command([HERE / 'guest.sh', self.vm, '--push', self.a.exe, self.exe], 900)
        self.g(f'chmod +x {self.exe}')
        replies, seconds = self.mcp({'method': 'tools/list'})
        tools = [t['name'] for t in replies.get(10, {}).get('result', {}).get('tools', [])]
        if tools != [TOOL]:
            raise StepFailed(f'the tool server listed {tools}: {replies}')
        return {'tools': tools, 'server': replies[1]['result']['serverInfo'], 'seconds': seconds}

    def signed_out(self):
        default = self.default(SAFARI)
        result, seconds = self.read()
        text = result['content'][0]['text']
        if not result.get('isError') or '/authwall' not in text:
            raise StepFailed(f'signed out, the tool did not refuse with the sign-in wall: {result}')
        if 'Join LinkedIn' in text:
            raise StepFailed('the sign-in wall\'s own text reached the model')
        return {'default': default, 'answer': text, 'seconds': seconds}

    def judged(self, bundle):
        self.g(f'open -g -b {bundle} {self.base}/sign-in')
        time.sleep(4)
        result, seconds = self.read()
        if result.get('isError'):
            raise StepFailed(f'{bundle}: the tool refused: {result}')
        answer = json.loads(result['content'][0]['text'])
        name = bundle.split('.')[-1].lower()
        (self.a.out / f'{name}-reply.json').write_text(json.dumps(answer, indent=2) + '\n')
        expect = self.manifest['expect']
        profile = answer['profile']
        details = {d['url'].rstrip('/').split('/')[-1]: d for d in answer['profile_details']}
        companies = answer['companies']
        problems = []
        if expect['profile_hidden'] not in profile['text']:
            problems.append('the profile\'s hidden text did not come back')
        exp = details.get('experience', {})
        if expect['details_only'] not in exp.get('text', ''):
            problems.append('the /details/experience/ page was not read')
        if expect['details_hidden'] not in exp.get('text', ''):
            problems.append('the /details/experience/ page\'s hidden text did not come back')
        if 'recommendations' in details:
            problems.append('the recommendations page was opened')
        keys = self.manifest['experience_keys']
        if len(companies) != len(keys):
            problems.append(f'{len(companies)} company pages read for {len(keys)} Experience companies')
        rows = []
        for key, row in zip(keys, companies):
            vanity = self.manifest['about'].get(key, {}).get('vanity', key)
            hidden = expect['companies'][vanity]
            text = row.get('text', '')
            if hidden not in text:
                problems.append(f'company {key}: its hidden text did not come back ({row.get("error", "")})')
            rows.append({'url': row['url'], 'shown_url': row.get('shown_url'), 'expanders_pressed': row.get('expanders_pressed'),
                         'chars': len(text), 'hidden_text_back': hidden in text, 'head': text[:240]})
        summary = {
            'browser': answer['browser'], 'seconds': seconds,
            'profile': {'shown_url': profile['shown_url'], 'expanders_pressed': profile['expanders_pressed'],
                        'chars': len(profile['text']), 'hidden_text_back': expect['profile_hidden'] in profile['text']},
            'profile_details': [{'url': d['url'], 'expanders_pressed': d.get('expanders_pressed'),
                                 'chars': len(d.get('text', '')), 'error': d.get('error')} for d in answer['profile_details']],
            'companies': rows, 'note': answer.get('note'),
        }
        if problems:
            raise StepFailed(f'{bundle}: ' + '; '.join(problems) + ' — ' + json.dumps(summary)[:3000])
        return summary

    def safari(self):
        return self.judged(SAFARI)

    def chrome_read(self):
        default = self.default(CHROME)
        row = self.judged(CHROME)
        row['default'] = default
        return row

    def cleanup(self):
        pid = self.g(f'cat {self.site_dir}/server.pid 2>/dev/null || true')
        if pid.isdigit():
            self.g(f'kill {pid} 2>/dev/null || true')
        return {'server_pid_stopped': pid}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--site', type=Path, required=True, help='the stand-in site linkedin-standin.py built')
    p.add_argument('--exe', type=Path, required=True, help='the richos-tauri executable under test')
    p.add_argument('--expect-sha', default='')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = Walk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
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
        if not ok and step != 'cleanup':
            if 'cleanup' in steps and steps.index('cleanup') > steps.index(step):
                walk.cleanup()
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
