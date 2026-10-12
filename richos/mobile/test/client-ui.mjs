// Real bundled DOM/controller with delayed native persistence. This catches lost
// keystrokes that headless core actions alone cannot reveal.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { rmSync, mkdirSync, mkdtempSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { cacheRoot, stageAssets } from '../cli/simulator.mjs';
const require = createRequire(import.meta.url);
const {createClient}=require('../core/client.js');
const {harness}=require('../dev/client-runtime.js');
const { loadPlaywright } = require('../../app/ui/tests/lib/harness.js');
export async function clientUI({engine='chromium'}={}) {
  const scratch = mkdtempSync(join(cacheRoot(), 'client-ui-')), assets = join(scratch, 'mobile-ui');
  const knownDefects = [];
  let browser;
  try {
    stageAssets(assets, false);
    browser = await loadPlaywright()[engine].launch({ headless: true });
    const context = await browser.newContext({ viewport: { width: 375, height: 812 }, isMobile: engine==='chromium', hasTouch: true });
    const h=harness(), seed=await createClient(h.ports);
    await seed.dispatch({type:'pair',link:'https://mac.example/#pair=code'});
    await seed.dispatch({type:'confirm-pair',matched:true}); seed.close();
    const release = require('../release-config.json');
    await context.addInitScript(({ disk: initial, release }) => {
      let data=initial, files=[{id:'retained',seconds:9,threadId:'general',origin:'https://mac.example'}], recording=false, serial=0;
      window.__voiceCalls=[];window.__streamCount=0;window.RichOSClientInspect=app=>{window.__client=app;};
      window.webkit = { messageHandlers: { richos: { async postMessage({ method, args }) {
        if (method === 'load') return data;
        if (method === 'save') { await new Promise(resolve => setTimeout(resolve, 25)); data = args.value; return true; }
        if (method === 'updateInfo') return { version: release.version, build: release.build, osVersion: '16.7.16', configured: false, appId: null, storefront: null };
        if (method === 'recordings') return files;
        if (method === 'recordStart') {recording=true;window.__voiceCalls.push(method);return true;}
        if (method === 'recordStop') {if(recording)files.push({id:'audio-'+(++serial),seconds:3});recording=false;window.__voiceCalls.push(method);return true;}
        if (method === 'recordCancel') {recording=false;window.__voiceCalls.push(method);return true;}
        if (['configure','sign','hash','recordHash'].includes(method)) return method==='hash'?'0'.repeat(64):'fixture';
        if (method === 'streamClose') return true;
        if (method === 'connectHealth')return {serviceState:'available'};
        if (method === 'streamOpen') {window.__streamCount++;window.__disconnect=()=>window.RichOSNativeEvent({id:args.id,kind:'stream-error'});window.__open=()=>window.RichOSNativeEvent({id:args.id,kind:'stream-open'});window.__message=frame=>window.RichOSNativeEvent({id:args.id,kind:'stream-data',data:btoa('event: message\ndata: '+JSON.stringify(frame)+'\n\n')});window.__hello=()=>window.RichOSNativeEvent({id:args.id,kind:'stream-data',data:btoa('event: hello\ndata: '+JSON.stringify({capabilities:['text','voice','audio']})+'\n\n')});return true;}
        if (method === 'request') return {status:200,body:JSON.stringify({messages:[]}),headers:{'X-RichOS-Challenge':'fresh'}};
        if (method === 'incomingLink' || method === 'pushIncoming') return null;
        throw Error(method);
      } } } };
    },{ disk: h.disk(), release: { version: release.version, build: release.build } });
    const page = await context.newPage(), errors = []; page.on('pageerror', error => errors.push(error.message));
    page.setDefaultTimeout(15000);
    await page.goto(pathToFileURL(join(assets, 'client.html')).href);
    const message = page.getByRole('textbox', { name: 'Message', exact: true });
    await page.getByRole('button', { name: 'Hold to record', exact: true }).waitFor();
    const recoveredSend=page.getByRole('button',{name:'Send unsent recording',exact:true});
    await recoveredSend.waitFor(); assert(await recoveredSend.isDisabled());
    await page.waitForFunction(()=>typeof window.__hello==='function');
    await page.evaluate(()=>window.__hello());
    await page.waitForFunction(()=>!document.querySelector('#recordings button[aria-label="Send unsent recording"]').disabled);
    assert.equal(await page.locator('#connection').textContent(),'','Healthy operation needs no connection announcement');
    const headerBefore=await page.locator('.topbar').boundingBox();
    const streamBefore=await page.evaluate(()=>window.__streamCount);
    await page.evaluate(()=>window.__disconnect());
    await page.waitForFunction(()=>document.getElementById('connection').textContent==='');
    assert.equal((await page.locator('.topbar').boundingBox()).height,headerBefore.height,'Brief reconnection must not move the conversation');
    assert.equal(await page.locator('#error').textContent(),'');
    assert(await message.isEnabled(),'The composer stays usable during recovery');
    await page.waitForFunction(n=>window.__streamCount>n,streamBefore);
    await page.evaluate(()=>window.__open());
    await page.waitForFunction(()=>document.getElementById('connection').textContent==='');
    const text = 'Native phone check 0123456789';
    await message.pressSequentially(text, { delay: 1 });
    await page.waitForFunction(value => document.querySelector('#message').value === value, text);
    // Wait on a semantic durability boundary, not a timing guess.
    await page.waitForFunction(() => !document.querySelector('#error').textContent);
    assert.equal(await message.inputValue(), text);
    // A new character after the delayed writes settle must append to the complete draft.
    await page.waitForTimeout(text.length * 35);
    assert.equal(await message.inputValue(), text);
    await message.pressSequentially('!'); assert.equal(await message.inputValue(), text + '!');
    for (const colorScheme of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme });
      for (const width of [320, 375, 430]) {
        await page.setViewportSize({ width, height: 812 });
        assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${colorScheme} ${width}: horizontal overflow`);
      }
    }
    if(process.env.RICHOS_UI_SHOTS){mkdirSync(process.env.RICHOS_UI_SHOTS,{recursive:true});await page.screenshot({path:join(process.env.RICHOS_UI_SHOTS,'native-composer.png')});}
    await message.fill(''); await message.dispatchEvent('input');
    const mic=page.getByRole('button',{name:'Hold to record',exact:true});await mic.waitFor();
    const hold=async()=>{const box=await mic.boundingBox();await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();await page.waitForFunction(()=>document.body.dataset.voice==='held');return box;};
    let box=await hold();await page.mouse.move(box.x+20,box.y-85,{steps:5});
    await page.waitForFunction(()=>document.body.dataset.voice==='locked');assert(await mic.isVisible(),'Keep the captured microphone attached until touch release');await page.mouse.up();assert(await mic.isHidden(),'Show only locked controls after release');
    if(process.env.RICHOS_UI_SHOTS)await page.screenshot({path:join(process.env.RICHOS_UI_SHOTS,'native-locked.png')});
    assert.equal(await page.evaluate(()=>window.__voiceCalls.filter(x=>x==='recordStop').length),0);
    await page.getByRole('button',{name:'Cancel recording',exact:true}).click();
    await page.waitForFunction(()=>document.body.dataset.voice==='idle');
    box=await hold();await page.mouse.move(box.x-100,box.y+20,{steps:5});await page.mouse.up();
    await page.waitForFunction(()=>document.body.dataset.voice==='idle');
    assert.equal(await page.evaluate(()=>window.__voiceCalls.filter(x=>x==='recordCancel').length),2);
    await hold();await page.mouse.up();await page.waitForFunction(()=>document.body.dataset.voice==='idle');
    assert.equal(await page.evaluate(()=>window.__voiceCalls.filter(x=>x==='recordStop').length),1);
    box=await hold();await page.mouse.move(box.x+20,box.y-85,{steps:5});
    await page.waitForFunction(()=>document.body.dataset.voice==='locked');assert(await mic.isVisible(),'Keep the captured microphone attached until touch release');await page.mouse.up();assert(await mic.isHidden(),'Show only locked controls after release');
    await page.getByRole('button',{name:'Send voice message',exact:true}).click();
    await page.waitForFunction(()=>document.body.dataset.voice==='idle');
    assert.equal(await page.evaluate(()=>window.__voiceCalls.filter(x=>x==='recordStop').length),2);
    // Real overflowing history, not an empty conversation that cannot scroll.
    for(let n=1;n<=18;n++)await page.evaluate(n=>window.__message({id:'history-'+n,cursor:n,role:'rich',text:('Earlier conversation '+n+'. ').repeat(16),complete:true}),n);
    await page.locator('[data-message-id="history-18"]').waitFor();
    const bottom=()=>page.waitForFunction(()=>{const a=document.getElementById('conversation');return a.scrollHeight-a.scrollTop-a.clientHeight<=2;});
    await bottom();
    // SETTLED, ON THE APP'S OWN RECEIPT: every dispatched action, gesture and durable write has
    // finished (`core/client.js` `settle`), then two rendering updates so the follow policy's
    // resize observer has seen the result. The product undoes a reader's scroll that lands on a
    // layout change still in flight (follow-yank, below), so the checks about his scroll are made
    // on a settled layout and that defect is measured on its own, every run.
    const settled=()=>page.evaluate(async()=>{await window.__client.settle();await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));});
    // KNOWN DEFECT follow-yank — reported, not fixed here: CEO §76 preserves the PWA and this
    // client as they are. `web-app/lib/follow.js` `pause()` ignores upward input once scrollTop is
    // 0, and both engines scroll before they dispatch the wheel, so a fling to the oldest message
    // never pauses following; the scroll handler then pins to the newest message whenever the
    // layout changed in the same frame. Modelled here as the composer growing while he drags up.
    await settled();
    const yank=await page.evaluate(async()=>{
      const area=document.getElementById('conversation'),composer=document.querySelector('.composer');
      const frames=()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
      composer.style.paddingBottom='60px';area.scrollTop=0;await frames();
      const seen={top:Math.round(area.scrollTop),latestHidden:document.getElementById('latest').hidden};
      composer.style.paddingBottom='';await frames();return seen;
    });
    assert(yank.top>200 && yank.latestHidden,`follow-yank is no longer reproduced (${JSON.stringify(yank)}): the product changed, so replace this expectation with the ordinary assertion`);
    knownDefects.push(`follow-yank: his scroll to the oldest message, in the same frame as a layout change, was returned to the newest (scrollTop 0 -> ${yank.top}, "Latest messages" hidden)`);
    await bottom();
    const readOlder=async()=>{await settled();await page.locator('#conversation').hover();await page.mouse.wheel(0,-50000);await page.waitForFunction(()=>document.getElementById('conversation').scrollTop<200);await page.getByRole('button',{name:'↓ Latest messages',exact:true}).waitFor();};
    await readOlder();
    await page.evaluate(()=>window.__message({id:'history-19',cursor:19,role:'rich',text:'An incoming reply must not pull the reader away from older messages.',complete:true}));
    await page.locator('[data-message-id="history-19"]').waitFor({state:'attached'});
    assert(await page.evaluate(()=>document.getElementById('conversation').scrollTop<200),'Incoming replies preserve older reading position');
    // Held release must jump to the outgoing bubble, including after the
    // recording controls and unsent panel resize the conversation.
    await hold();await page.mouse.up();await page.waitForFunction(()=>document.body.dataset.voice==='idle');await bottom();
    const stopsBeforeTouch=await page.evaluate(()=>window.__voiceCalls.filter(x=>x==='recordStop').length);
    // Chromium supplies trusted touch input. WebKit also runs the same DOM and
    // layout regressions, with Touch Events dispatched at the native mic target.
    const input=engine==='chromium'?await context.newCDPSession(page):{send:async(_,event)=>{
      const point=event.touchPoints[0];
      await mic.dispatchEvent(event.type.toLowerCase(),{touches:point?[{identifier:1,clientX:point.x,clientY:point.y}]:[],changedTouches:[{identifier:1,clientX:point?.x||0,clientY:point?.y||0}]});
    }};
    const touchStart=async()=>{const b=await mic.boundingBox();const point={x:b.x+b.width/2,y:b.y+b.height/2};await input.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[point]});await page.waitForFunction(()=>document.body.dataset.voice==='held');return point;};
    await touchStart();
    await mic.dispatchEvent('pointercancel',{pointerType:'touch'});
    assert.equal(await page.evaluate(()=>document.body.dataset.voice),'held','A cancelled pointer must not end an ongoing finger hold');
    await input.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
    await page.waitForFunction(()=>document.body.dataset.voice==='idle');
    assert.equal(await page.evaluate(()=>window.__voiceCalls.filter(x=>x==='recordStop').length),stopsBeforeTouch+1);
    let point=await touchStart();
    await input.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:point.x-120,y:point.y}]});
    await input.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
    await page.waitForFunction(()=>document.body.dataset.voice==='idle');
    assert.equal(await page.evaluate(()=>window.__voiceCalls.filter(x=>x==='recordCancel').length),3);
    await readOlder();
    point=await touchStart();
    await input.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:point.x,y:point.y-100}]});
    await page.waitForFunction(()=>document.body.dataset.voice==='locked');
    await input.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
    assert(await page.evaluate(()=>document.getElementById('conversation').scrollTop<200),'Locking and releasing leaves older history readable');
    for(const width of [320,375,430]) {
      await page.setViewportSize({width,height:812});
      const cancel=await page.getByRole('button',{name:'Cancel recording',exact:true}).boundingBox();
      const send=await page.getByRole('button',{name:'Send voice message',exact:true}).boundingBox();
      assert(Math.abs(cancel.x+cancel.width/2-width/2)<=1,`${width}: Cancel must be centred like Telegram`);
      assert(send.x-cancel.x-cancel.width>=24,`${width}: distinct Cancel and Send hit areas`);
      assert(cancel.height>=44 && send.height>=44);
    }
    if(process.env.RICHOS_UI_SHOTS)await page.screenshot({path:join(process.env.RICHOS_UI_SHOTS,'native-locked-centred.png')});
    await page.getByRole('button',{name:'Send voice message',exact:true}).tap();
    await page.waitForFunction(()=>document.body.dataset.voice==='idle');
    assert.equal(await page.evaluate(()=>window.__voiceCalls.filter(x=>x==='recordStop').length),stopsBeforeTouch+2);
    await bottom();
    await page.setViewportSize({width:375,height:600});await bottom();
    await page.setViewportSize({width:375,height:812});await bottom();
    // Follow an arriving reply and subsequent streamed growth, until an actual
    // user scrolls up. Even a small deliberate upward scroll pauses following.
    await page.evaluate(()=>window.__message({id:'history-20',cursor:20,role:'rich',text:'Reply begins',complete:false}));
    await page.locator('[data-message-id="history-20"]').waitFor();await bottom();
    await page.evaluate(()=>window.__message({id:'history-20',cursor:20,role:'rich',text:'A growing reply. '.repeat(120),complete:false}));
    await page.waitForFunction(()=>document.querySelector('[data-message-id="history-20"]').textContent.includes('A growing reply'));await bottom();
    await page.locator('#conversation').hover();await page.mouse.wheel(0,-24);
    await page.getByRole('button',{name:'↓ Latest messages',exact:true}).waitFor();
    const pausedTop=await page.locator('#conversation').evaluate(a=>a.scrollTop);
    await page.evaluate(()=>window.__message({id:'history-20',cursor:20,role:'rich',text:'Completed reply. '.repeat(200),complete:true}));
    await page.waitForFunction(()=>document.querySelector('[data-message-id="history-20"]').textContent.includes('Completed reply'));
    assert(Math.abs(await page.locator('#conversation').evaluate(a=>a.scrollTop)-pausedTop)<2,'Reply growth must not undo a small deliberate scroll up');
    await page.getByRole('button',{name:'↓ Latest messages',exact:true}).click();await bottom();
    await readOlder();await message.fill('Sending while reading older history');
    await page.getByRole('button',{name:'Send',exact:true}).click();await bottom();
    await page.getByRole('button',{name:'Settings',exact:true}).click();
    await page.getByRole('button',{name:'Close settings',exact:true}).click();
    assert.deepEqual(errors, []);
    return { engine, knownDefects, briefReconnectInvisible:true, rapidTypingPreserved: true, bothThemesFitPhoneWidths: true, sendsFollowLatest: true, streamedRepliesFollowLatest: true, deliberateSmallScrollPauses: true, latestButtonResumes: true, olderReadingPreserved: true, lockedCancelCentred: true };
  } finally { await browser?.close(); rmSync(scratch, { recursive: true, force: true }); }
}
