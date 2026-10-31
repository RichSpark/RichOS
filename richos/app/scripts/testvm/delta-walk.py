#!/usr/bin/env python3
"""Run the desktop/phone delta scenario against an owned, booted VM.

Use reserve.py around run.sh, this command and stop.sh. Capture and reports go
under --out; no model turn is retried automatically. Run --help for inputs.

The phone is the actual RichConnect client on the host, over the native apps' API
(walkphone.py): it pairs, and says the six words do not match for rejected-phone. The
steps that photographed the phone web page in the guest's Safari (typed-to-phone,
reply-to-phone, phone-to-Mac) are retired with that page (2026-10-30): a native phone
has no screen in the guest to read, and the native apps' own speed is measured on the
phones. The mid-turn switch for the work chip is now a Mac turn.
"""
import argparse
import json
import hashlib
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import threading
import time
import uuid

HERE=Path(__file__).resolve().parent
QA=HERE.parent/'qa'
sys.path.insert(0,str(QA/'lib'))
import qaimg
import qaocr
from scenario import Failure, TurnBudget, run
from reserve import cpu_admission
from relaunch import guest,relaunch
from rollback import exercise
import walkphone
import importlib.util
spec=importlib.util.spec_from_file_location('walk_timeline',QA/'timeline.py')
timeline=importlib.util.module_from_spec(spec);spec.loader.exec_module(timeline)


# The wait band's headline while Rich works: "Rich is thinking" until the first sign of
# work, then "Rich is working" or "Writing the reply" (ui/main.js). Every band matcher
# takes all three, so the opening frames count as band-visible.
WORKING_HEADLINE=r'(?m)^\s*(?:Rich is thinking|Rich is working|Writing the reply)[.\s]*$'


def marker_pattern(token):
    # OCR can collapse or insert spaces. Do not accept lookalike characters.
    return r'(?<![a-z0-9])'+r'\s*'.join(map(re.escape,token))+r'(?![a-z0-9])'


def visibility(directory, present):
    directory=Path(directory);record=json.loads((directory/'bounds.json').read_text())
    bounds=timeline.visibility_bounds(record,present-1,present,1e12)
    rows=[f for f in record['frames'] if f['number'] in (present-1,present)]
    for f in rows:
        if hashlib.sha256((directory/('%04d.png'%f['number'])).read_bytes()).hexdigest()!=f['sha256']:
            raise Failure('harness failure','frame bytes changed since capture')
    return bounds['milliseconds']


def command(argv,timeout=30):
    r=subprocess.run(list(map(str,argv)),capture_output=True,text=True,timeout=timeout)
    if r.returncode:raise Failure('harness failure','command failed (%d): %s\n%s' % (r.returncode,' '.join(map(str,argv)),r.stderr))
    return r.stdout


class Walk:
    def __init__(self,a):
        self.a=a;self.vm=a.vm;self.out=a.out;self.out.mkdir(parents=True,exist_ok=False)
        self.samples=[];self.switched=None;self.switch=None
        self.phone_state=self.out/'phone.json' # the walk phone's own test key and session
        self.state=Path(os.environ.get('TESTVM_ROOT',str(Path.home()/'.richos-testvm')))/'run'/a.vm
        self.payload=(self.state/'payload').read_text().strip();self.home=self.payload+'/home'
        self.remote=self.payload+'/walk-'+uuid.uuid4().hex[:8]
        self.started=time.monotonic()
        guest(self.vm,'mkdir -p '+shlex.quote(self.remote))
        command([HERE/'guest.sh',self.vm,'--push',QA,self.remote+'/qa'],120)
        os.environ['RICHOS_QA_OCR_CACHE']=str(self.out/'ocr-cache')
        qaocr.positive_control(QA/'fixtures/ocr-control.png',re.compile('qa-fixture@example.invalid'))
    def ax(self,mode,*args,app=None):
        argv=[HERE/'ax.sh',self.vm,mode,*args,'--json']
        if app:argv+=['--app',app]
        result=subprocess.run(list(map(str,argv)),capture_output=True,text=True,timeout=25)
        rows=[json.loads(s) for s in result.stdout.splitlines() if s.startswith('{')]
        failure=next((r for r in rows if r.get('error')),None)
        if failure:
            raise Failure('harness failure',json.dumps(failure),status=failure['error'])
        if result.returncode:
            raise Failure('harness failure',result.stdout+'\n'+result.stderr,status='transport_failure')
        return rows
    def press(self,title,app=None,role=None):
        selector=['--title',title]
        if role:selector+=['--role',role]
        return self.ax('click',*selector,app=app)
    def select_tailnet_route(self):
        # Explicit managed-route fixture only. A selected-value receipt is local
        # evidence; live acceptance also checks the resulting route/Connect state.
        return self.ax('click','--id','phone-use-tailnet','--in','dialog','--expect','value=1')
    def press_when_present(self,title,app=None,budget=20):
        # Press the moment the button exists, looking first, instead of sleeping a fixed time
        # that costs full price when it is already drawn and is still too short when it is slow.
        end=time.monotonic()+budget
        while True:
            try:return self.press(title,app=app)
            except Failure as exc:
                if exc.status != 'notfound' or time.monotonic()>=end:raise
            time.sleep(.5)

    def optional_press(self,title):
        try:self.press(title)
        except Failure as exc:
            if exc.status != 'notfound':raise
    def tree(self,app=None):return self.ax('tree',app=app)
    def strings(self,rows):return '\n'.join(str(row.get(k,'')) for row in rows for k in ('title','desc','value'))
    def settings(self):self.press('Settings',role='AXPopUpButton')
    def phone_prerequisite(self,step,budget):
        pid=(self.state/'app.pid').read_text().strip()
        if not pid.isdigit():raise Failure('harness failure','recorded app PID is invalid')
        code="import pathlib,plistlib,subprocess,sys; exe=subprocess.check_output(['ps','-p',sys.argv[1],'-o','comm='],text=True).strip(); print(plistlib.loads(pathlib.Path(exe.split('/Contents/MacOS/')[0]+'/Contents/Info.plist').read_bytes())['CFBundleShortVersionString'])"
        actual=guest(self.vm,shlex.join(['python3','-c',code,pid]))
        if actual!=self.a.current:raise Failure('prerequisite unavailable','running build '+actual+' differs from pinned '+self.a.current)
        try:command([HERE/'keychain.sh','check',self.vm,self.home],90)
        except Failure as exc:raise Failure('prerequisite unavailable',str(exc)) from exc
        self.optional_press('Not now')
        self.optional_press('Use this folder')
        return {'keychain':'GUI settings and access checked','build':self.a.current}
    def pairing_url(self):
        end=time.monotonic()+25
        while time.monotonic()<end:
            text=self.strings(self.ax('tree','--in','dialog'))
            m=re.search(r'https://[^\s\"<>]+(?:pair=)[A-Z0-9]+',text)
            if m:return m.group(0)
            code=re.search(r'(?m)^[A-Z2-9]{8}$',text)
            if code:
                name=command([HERE/'tailnet.sh','name',self.vm]).strip()
                if '=' in name:name=name.split('=',1)[1].split()[0]
                return 'https://'+name+':8443/#pair='+code.group(0)
            if 'keychain did not answer' in text:raise Failure('prerequisite unavailable','app could not access phone keys')
            time.sleep(.5)
        raise Failure('product failure','no pairing URL/code rendered within 25 seconds')
    def pair(self,step,budget):
        self.settings();self.press('Use Rich from your phone',role='AXMenuItem')
        if self.a.tailnet_route_choice:self.select_tailnet_route()
        self.press('Set my phone up')
        self.url=self.pairing_url()
        paired=self.phone_pair(self.phone_state,self.url)
        # Sage F1: the Mac, not the phone, activates a paired device — its own "They match".
        self.press_when_present('They match',budget=40)
        self.press('Close')
        # A fixed layout gives the frame stream a known rectangle.
        script='tell application "System Events"\n tell process "richos-tauri"\n set position of window 1 to {0,25}\n set size of window 1 to {1024,700}\n end tell\nend tell'
        guest(self.vm,shlex.join(['osascript','-e',script]))
        self.press(self.a.thread_a)
        return {'paired':True,'words':paired.get('words'),'layout':{'mac':[0,25,1024,700]}}
    def phone_pair(self,state,url,reject=False):
        try:return walkphone.pair(state,url,reject=reject)
        except walkphone.PhoneFailed as exc:raise Failure('product failure','the phone could not pair: '+str(exc)) from exc
    def capture(self,label,budget,switch=False):
        try:
            admission=cpu_admission(wait_seconds=self.a.admission_wait_seconds)
        except BlockingIOError as exc:
            raise Failure('prerequisite unavailable',str(exc)+' No send issued.') from exc
        # Refuse stale layout before spending a turn. Coordinates are screen
        # coordinates; each capture/crop below relies on this exact placement.
        want=(0,25,1024,700)
        node=next((x for x in self.ax('find','--role','AXWindow','--first',app='richos-tauri') if x.get('role')=='AXWindow'),{})
        actual=tuple(node.get(k) for k in ('x','y','w','h'))
        if actual!=want:raise Failure('harness failure','window geometry changed; recalibrate before send: '+str(('richos-tauri',actual,want)))
        number=budget.take() # Counts the attempt even if the send later fails.
        token='probe'+uuid.uuid4().hex[:10]
        input_token=token[::-1]
        prompt='Reverse this character sequence and reply with only the reversed sequence: '+input_token
        if switch:prompt+=' Before replying, use your terminal tool to run sleep 12.'
        self.ax('type',prompt,'--role','AXTextArea','--first','--replace','--in','composer')
        action=['--key','36']
        remote=self.remote+'/'+label
        args=['python3',self.remote+'/qa/timeline.py','capture',remote,'25','--region','0,25,1024,700',
              '--baseline','2',*action]
        thread=None;switched={}
        if switch:
            def move():
                time.sleep(8)
                try:
                    before=self.guest_clock();self.press(self.a.thread_b)
                    switched.update(before=before,after=self.guest_clock(),target=self.a.thread_b)
                except Exception as exc:switched['failure']=str(exc)
            thread=threading.Thread(target=move);thread.start()
        try:
            output=guest(self.vm,shlex.join(args),60)
        finally:
            if thread:thread.join(timeout=30)
        # Transfer the immutable sequence once, then perform all analysis locally.
        command([HERE/'guest.sh',self.vm,'--pull',remote,self.out/label],120)
        row={'number':number,'token':token,'input_token':input_token,'prompt':prompt,'directory':str(self.out/label),**admission,
             'capture':output,'switch':switched}
        (self.out/(label+'.json')).write_text(json.dumps(row,indent=2))
        return row
    def guest_clock(self):
        return float(guest(self.vm,"python3 -c 'import time; print(time.time()*1000)'"))
    def samples_action(self,step,budget):
        for n in range(step['count']):self.samples.append(self.capture('mac-%d'%n,budget))
        return {'captures':[x['directory'] for x in self.samples]}
    def hit(self,row,pattern):
        directory=Path(row['directory'])
        argv=[sys.executable,QA/'ocr-find.py',pattern,directory,'--first','--quiet','--timeline','--json','--flat']
        r=subprocess.run(list(map(str,argv)),capture_output=True,text=True,timeout=180)
        if r.returncode==1:raise Failure('harness failure','OCR did not establish the event; inspect retained pixels before judging the product: '+pattern)
        if r.returncode:raise Failure('harness failure',r.stderr)
        hit=json.loads(r.stdout.splitlines()[0])
        try:hit['milliseconds']=visibility(directory,int(Path(hit['frame']).stem))
        except (ValueError,KeyError,OSError) as exc:raise Failure('harness failure','event has no verified consecutive absence/presence frames: '+str(exc)) from exc
        return hit
    def bound(self,values,limit,signed=False):
        evidence={'milliseconds':values,'limit_ms':limit}
        if any((x<0 and not signed) or x>limit for x in values):raise Failure('product failure',json.dumps(evidence))
        return evidence
    def interval_bound(self,values,limit):
        evidence={'intervals_ms':values,'limit_ms':limit}
        if any(lo>limit for lo,hi in values):raise Failure('product failure',json.dumps(evidence))
        if any(hi>limit for lo,hi in values):raise Failure('harness failure','capture interval straddles target; result unproven: '+json.dumps(evidence))
        return evidence
    def first_words(self,step,budget):
        return self.interval_bound([self.hit(x,marker_pattern(x['token']))['milliseconds'] for x in self.samples],step['limit_ms'])
    def wait_band(self,step,budget):
        x=self.samples[0];directory=Path(x['directory'])
        # Full coverage of every captured frame for the negative claim.
        first=self.hit(x,WORKING_HEADLINE)
        result=subprocess.run([str(QA/'ocr-find.sh'),'Nothing.has.come.back',x['directory'],'--quiet'],capture_output=True,text=True,timeout=180)
        if result.returncode==0:raise Failure('product failure','old wait-band detail line appeared')
        if result.returncode!=1:raise Failure('harness failure',result.stderr)
        # Inspect every band-visible desktop frame. OCR is cached from the full scan.
        frames=sorted(directory.glob('*.png'))
        visible=[f for f in frames if re.search(WORKING_HEADLINE,qaocr.text(f),re.I)]
        if len(visible)<2:raise Failure('prerequisite unavailable','fewer than two band-visible frames')
        boxes=[]
        for frame in visible:
            result=command([sys.executable,QA/'frame.py','box',frame,*map(str,self.a.band_box),self.a.band_color,'--tol','0'])
            match=re.search(r'x (\d+)\.\.(\d+) .* y (\d+)\.\.(\d+)',result)
            if not match:raise Failure('harness failure','boundary color was not measured; recalibrate the supplied region/color against the current active frame')
            boxes.append(tuple(map(int,match.groups())))
        if len(set(boxes))!=1:raise Failure('product failure','wait-band boundary changed: '+json.dumps(boxes))
        return {'detail_hits':0,'frames':len(frames),'band_visible_frames':len(visible),'band_box':boxes[0]}
    def work_chip(self,step,budget):
        # One Mac turn that asks for a 12-second terminal wait, so the switch at eight seconds
        # has active work to observe (it was the phone turn until the phone page was retired).
        self.press(self.a.thread_a)
        self.switched=self.capture('switch-0',budget,switch=True)
        self.switch=self.switched['switch']
        return self.judge_switch()
    def judge_switch(self):
        if not self.switched or not self.switch or 'failure' in self.switch:raise Failure('prerequisite unavailable','no successful mid-turn switch capture')
        meta,frames=timeline._read_meta(self.switched['directory'])
        before=self.switch['before'];after=self.switch['after']
        # AX lookup and dispatch can take several seconds. Date the visible
        # destination header, not the start of the command that seeks it. The
        # sidebar already contains both names, so a whole-frame token is invalid.
        observed=None
        for index,(n,when) in enumerate(frames):
            if when<before:continue
            path=Path(self.switched['directory'])/('%04d.png'%n)
            img=qaimg.load(str(path));x,y,w,h=self.a.header_box
            crop=self.out/('switch-header-%04d.png'%n)
            qaimg.save(img.crop(x,y,x+w,y+h),str(crop))
            if re.search(re.escape(self.a.thread_b),qaocr.text(crop),re.I):
                observed=(index,when);break
        if observed is None:raise Failure('harness failure','destination header never appeared in the switch capture')
        index,switched_at=observed
        if index==0 or switched_at-frames[index-1][1]>1500:
            raise Failure('harness failure','no frame immediately before the visible switch')
        last=Path(self.switched['directory'])/('%04d.png'%frames[index-1][0])
        if not re.search(WORKING_HEADLINE,qaocr.text(last),re.I):
            raise Failure('prerequisite unavailable','work was not visibly active immediately before the switch')
        chosen=[n for n,t in frames if switched_at<=t<=switched_at+3000]
        if not chosen:raise Failure('harness failure','capture does not cover post-switch interval')
        # The destination must be selected in AX, not just present in the sidebar.
        selected=self.ax('find','--title',self.a.thread_b,'--contains','--role','AXButton','--in','sidebar')
        if not any(row.get('current') in (True,'true','page') or row.get('selected') is True for row in selected):
            raise Failure('harness failure','AX did not confirm the destination thread is selected')
        for n in chosen:
            path=Path(self.switched['directory'])/('%04d.png'%n)
            img=qaimg.load(str(path));x,y,w,h=self.a.chip_box
            crop=self.out/('chip-%04d.png'%n);qaimg.save(img.crop(x,y,x+w,y+h),str(crop))
            if re.search(r"\d+\s+(working|waiting|ready|done|saved work records?|team jobs?|I can.t see)",qaocr.text(crop),re.I):
                raise Failure('product failure','destination chip shows work counts from another thread')
        return {'post_switch_frames':len(chosen),'stale_counts':0,'switch':self.switch,'visible_switch_ms':switched_at}
    def rejected_phone(self,step,budget):
        self.settings();self.press('Use Rich from your phone',role='AXMenuItem');self.press('Forget this phone')
        self.press('Set my phone up');url=self.pairing_url()
        # A second phone, which says the six words do not match.
        self.phone_pair(self.out/'phone-rejected.json',url,reject=True)
        rejected=self.home+'/Library/Application Support/com.richos.app/phone/rejected.json'
        before=guest(self.vm,'shasum -a 256 '+shlex.quote(rejected)).split()[0]
        started=relaunch(self.vm)
        # No signal to poll: the check is that the file did NOT change, so this is a settle margin
        # (2 s) for the app to have rewritten it if it were going to. Kept on purpose, recorded here.
        time.sleep(2)
        after=guest(self.vm,'shasum -a 256 '+shlex.quote(rejected)).split()[0]
        if before!=after:raise Failure('product failure','rejected-phone record changed across relaunch')
        self.settings();self.press('Use Rich from your phone',role='AXMenuItem')
        text=self.strings(self.ax('tree','--in','dialog'))
        if 'Your phone said the six words did not match' not in text:
            raise Failure('product failure','persisted rejection message absent after relaunch')
        listeners=guest(self.vm,'lsof -nP -a -p %d -iTCP:8443 -sTCP:LISTEN -t 2>/dev/null || true'%started['pid'])
        if listeners:raise Failure('product failure','rejected phone listener reopened after relaunch')
        return {'record_survived':True,'rejection_message':True,'listener_closed':True,'boot_log':started['log']}
    def rollback(self,step,budget):
        if not self.a.previous_app:raise Failure('prerequisite unavailable','a pinned previous bundle in the guest is required')
        return exercise(self.vm,self.a.previous,self.a.current,self.a.endpoint,self.a.previous_app)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('vm');p.add_argument('--out',type=Path,required=True)
    p.add_argument('--current',required=True);p.add_argument('--previous',required=True);p.add_argument('--endpoint',required=True)
    p.add_argument('--keychain-endurance',action='store_true',help='pair then idle/relaunch for 30 minutes; no test sends (app initialization may use the model)')
    p.add_argument('--previous-app');p.add_argument('--thread-a',default='Scenario A');p.add_argument('--thread-b',default='Scenario B')
    p.add_argument('--tailnet-route-choice',action='store_true',help='fixture has a visible managed route choice; select Tailscale by DOM ID and verify its value')
    p.add_argument('--admission-wait-seconds',type=float,default=0,
                   help='bounded wait for CPU below 80% busy before each attempt; excludes at most 5s final probe; recorded separately from capture')
    p.add_argument('--band-box',nargs=4,type=int,required=True,help='x0 y0 x1 y1 enclosing a stable band boundary')
    p.add_argument('--band-color',required=True,help='measured boundary color')
    p.add_argument('--chip-box',nargs=4,type=int,required=True,help='x y width height of work summary chip')
    p.add_argument('--header-box',nargs=4,type=int,default=[300,28,650,55],help='desktop crop containing only the selected conversation header')
    a=p.parse_args()
    if not 0<=a.admission_wait_seconds<=300:p.error('--admission-wait-seconds must be between 0 and 300')
    manifest=json.loads((HERE/'scenarios/delta.json').read_text())
    walk=Walk(a)
    if a.keychain_endurance:
        spec=importlib.util.spec_from_file_location('keychain_verify',HERE/'keychain-verify.py')
        verifier=importlib.util.module_from_spec(spec);spec.loader.exec_module(verifier)
        manifest['name']='keychain-endurance'
        manifest['steps']=manifest['steps'][:2]+[{'id':'endurance','action':'endurance','requires':['pair']}]
        walk.endurance=lambda step,budget:verifier.verify(a.vm)
    actions={x['action']:getattr(walk,'samples_action' if x['action']=='samples' else x['action']) for x in manifest['steps']}
    try:
        result=run(manifest,actions,a.out/'report.json',{'current':a.current,'previous':a.previous,'vm':a.vm},TurnBudget(6))
        return 0 if all(x['outcome']=='PASS' for x in result['steps']) else 1
    finally:
        # Evidence is local before guest scratch is removed. stop.sh owns VM cleanup.
        guest(a.vm,'rm -rf '+shlex.quote(walk.remote))


if __name__=='__main__':
    try:sys.exit(main())
    except (Failure,RuntimeError,OSError,ValueError) as exc:print(str(exc),file=sys.stderr);sys.exit(2)
