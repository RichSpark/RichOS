'use strict';
// Exact approved round-63.1 states, exercised with real encrypted verification.
const assert=require('node:assert/strict'),path=require('node:path');
const {pathToFileURL}=require('node:url');
const {fillSetup,ready,accepted,defaultMaster}=require('./verification.cjs');
const {measure,viewportWidths}=require('./layout.cjs');
const mockURL=pathToFileURL(path.join(__dirname,'RichPass.states-reference.html')).href;
async function testStates({browser,check,fileURL}) {
 await check('approved completion, wrong-master and Generate states match round-63.1',async()=>{
  for(const scheme of ['dark','light']) {
   const contexts=await Promise.all([browser.newContext({colorScheme:scheme,reducedMotion:'reduce'}),browser.newContext({colorScheme:scheme,reducedMotion:'reduce'})]);
   const errors=[],network=[];
   try {
    for(const c of contexts)await c.route(/^https?:/,r=>{network.push(r.request().url());return r.abort();});
    const pages=await Promise.all(contexts.map(c=>c.newPage())),[reference,actual]=pages;
    for(let i=0;i<pages.length;i++)pages[i].on('pageerror',e=>errors.push({page:i,message:e.message}));
    await Promise.all([reference.goto(mockURL),actual.goto(fileURL)]);await ready(actual);
    assert.equal(await actual.locator('#master-slot').isVisible(),true);
    await reference.locator('#setup-done').waitFor({state:'visible'});
    await reference.locator('[data-go="setup"]').click();
    for(const p of pages){await fillSetup(p);await p.locator('#advance').click();await p.locator('#setup-done').waitFor({state:'visible'});}
    await actual.waitForFunction(()=>!document.getElementById('start-status').textContent.includes('Saving'));
    // Completion is durable but acknowledgement is mandatory for the UI route.
    assert.ok(await actual.evaluate(()=>localStorage.getItem('richpass:verifier')));
    assert.equal(await actual.locator('#generate-view').isVisible(),false);
    assert.equal(await actual.locator('#setup-progress').getAttribute('aria-valuenow'),'100');
    assert.equal(await actual.locator('.done-note br').count(),1);
    assert.equal(await actual.locator('.done-note').textContent(),"The fingerprint (hash) of your master passphrase is saved.This safeguards you against typos whenever you need to re-enter your master passphrase.");
    assert.equal(await actual.locator('.done-warn').textContent(),'Make sure you can always remember or recover your master passphrase. RichPass cannot recover it for you.');
    assert.equal(await actual.locator('#lets-go').textContent(),"OK. Let's go!");
    assert.equal(await actual.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),false);
    async function compare(name,selectors) {
     for(const width of viewportWidths()) {
      for(const p of pages){await p.setViewportSize({width,height:900});await p.mouse.move(0,0);}
      if(name==='completion')for(const p of pages)await p.waitForFunction(()=>{
       const bar=document.getElementById('setup-progress'),title=document.getElementById('setup-title-text'),form=document.getElementById('setup-form');
       const target=Math.round(Math.min(form.clientWidth,title.getBoundingClientRect().width+2.5*parseFloat(getComputedStyle(document.documentElement).fontSize)));
       return Math.abs(bar.getBoundingClientRect().width-target)<0.25;
      });
      // Latest user overrides: completion has no beacon and the longer Generate
      // label wraps. These change no account rules or independent golden values.
      await reference.evaluate(()=>{
       document.getElementById('home-master').classList.remove('beacon');
       const action=document.getElementById('generate');
       action.textContent='Derive password from master + above input';
       action.style.maxWidth='100%';action.style.whiteSpace='normal';action.style.textAlign='center';
      });
      // The frozen mock has the same WebKit grid-cache defect on live resize.
      // Snapshot identical fresh reference markup at this width, then restore
      // its event-wired original. The product DOM stays untouched so clipping
      // remains a real failure. No reference text, style or expected size changes.
      await reference.evaluate(()=>{
       const original=document.querySelector('#fail.open');
       if(original){window.referenceFailureOriginal=original;original.replaceWith(original.cloneNode(true));}
      });
      let expected,got;
      try{[expected,got]=await Promise.all(pages.map(p=>measure(p,selectors)));}
      finally{await reference.evaluate(()=>{
       const original=window.referenceFailureOriginal;
       if(original){document.getElementById('fail').replaceWith(original);delete window.referenceFailureOriginal;}
      });}
      for(let i=0;i<selectors.length;i++) {
       const expectedStyle={...expected[i].style};
       // One internal layout-mode difference fixes WebKit's stale grid tracks.
       // Keep every visible geometry/font/color/spacing assertion against the mock.
       if(selectors[i]==='#fail'&&got[i].style.display==='block') {
        assert.equal(expectedStyle.display,'grid');expectedStyle.display='block';
       }
       assert.deepEqual(got[i].style,expectedStyle,`${scheme} ${name} ${width} ${selectors[i]} style`);
       for(const dim of ['x','width','height'])assert.ok(Math.abs(got[i][dim]-expected[i][dim])<0.25,
        `${scheme} ${name} ${width} ${selectors[i]} ${dim}: ${got[i][dim]} vs ${expected[i][dim]}`);
      }
      assert.ok(await actual.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),name+' overflow '+width);
     }
    }
    await compare('completion',['.home-card','#setup-title','#setup-progress','.progress-fill','#master-label','.done','.done-note','.done-warn','#lets-go','#home-view .links','#home-master']);
    for(const p of pages){await p.locator('#home-master').click();await p.locator('#master-view').waitFor({state:'visible'});assert.match(await p.locator('#master-view').textContent(),p===actual?/Master Passphrase Ideas/:/content for this page will be added later/);await p.locator('#master-return').click();await p.locator('#setup-done').waitFor({state:'visible'});await p.locator('#lets-go').click();await p.locator('#generate-view').waitFor({state:'visible'});}
    await accepted(actual);
    assert.equal(await actual.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),false);
    assert.equal((await actual.locator('#generate-title').textContent()).trim(),'RichPass: Generate');
    assert.equal(await actual.locator('#generate-title picture').count(),1);
    assert.equal(await actual.locator('#forget').evaluate(e=>!!(e.compareDocumentPosition(document.querySelector('#generate-view .links'))&Node.DOCUMENT_POSITION_PRECEDING)),true);
    assert.deepEqual(await actual.locator('#generate-view .links a').allTextContents(),['Why & How','Master','Algo Card']);
    await compare('Generate',['#generate-view .col','.view-head','#generate-title','#generate-title img','#account-form','#generate-view .links','.forget-row','#forget']);
    const defaultVector = require('./vectors.json').vectors.find(vector => vector.name === 'default');
    await actual.locator('#domain').fill(defaultVector.domain);
    await actual.locator('#generate').click();
    await actual.waitForFunction(password => document.getElementById('password').value === password, defaultVector.password);
    for(const p of pages)await p.locator('#forget').click();
    const wrong='fictional violet river lantner';
    for(let attempt=1;attempt<=3;attempt++) {
     for(const p of pages){await p.locator('#master').fill(wrong);await p.locator('#advance').click();await p.locator('#fail.open').waitFor();await p.waitForFunction(()=>document.getElementById('advance-text').textContent==='Try again'&&!document.getElementById('advance').disabled);}
     assert.equal(await actual.locator('#fail-title-text').textContent(),"This doesn't match the saved passphrase fingerprint.");
     assert.equal(await actual.locator('#master').getAttribute('aria-invalid'),'true');
     assert.equal(await actual.locator('#master').inputValue(),wrong);
     assert.equal(await actual.locator('#caps-row').isVisible(),false);
     assert.ok(!(await actual.locator('#fail').textContent()).includes('See Master'));
     if(attempt===1) {
      await compare('wrong master',['.home-card','#master-label','#master-group','#master','#fail','#fail-title-text','#fail-why','.fail-checks','#reveal','#advance','#home-master']);
      for(const p of pages)await p.locator('#reveal').click();
      await compare('revealed entry',['#readout','#readout-text','#readout-count','#master','#reveal']);
      assert.equal(await actual.locator('#readout .sp').count(),3);
      for(const p of pages){await p.locator('#reveal').click();await p.locator('#master').fill('changed');}
      assert.equal(await actual.locator('#readout-text').textContent(),'');
      assert.equal(await actual.locator('#master').getAttribute('aria-invalid'),'false');
     }
    }
    assert.equal(await actual.locator('#fail-why').textContent(),"Still no match after 3 tries. If you can't recall it exactly, your recovery clues can help.");
    for(const p of pages)await p.evaluate(()=>document.getElementById('master').dispatchEvent(new KeyboardEvent('keydown',{key:'CapsLock',modifierCapsLock:true,bubbles:true})));
    await compare('Caps Lock on',['#caps-row','#caps-text','#fail']);
    assert.equal(await actual.locator('#caps-row').isVisible(),true);
    for(const p of pages)await p.evaluate(()=>document.getElementById('master').dispatchEvent(new KeyboardEvent('keyup',{key:'CapsLock',modifierCapsLock:false,bubbles:true})));
    assert.equal(await actual.locator('#caps-row').isVisible(),false);
    assert.equal(await actual.locator('#generate-view').isVisible(),false);
    assert.equal(await actual.evaluate(()=>sessionStorage.getItem('richpass:master')),null);
    for(const p of pages){await p.locator('#master').fill(defaultMaster);await p.locator('#advance').click();await p.locator('#generate-view').waitFor({state:'visible'});}
    await accepted(actual);assert.equal(await actual.locator('#readout-text').textContent(),'');
    assert.deepEqual(network,[]);assert.deepEqual(errors,[]);
   } finally{await Promise.all(contexts.map(c=>c.close()));}
  }
 });
 await check('Checking feedback reflects real verification and revealed entry remains literal text',async()=>{
  const context=await browser.newContext();
  try {
   const p=await context.newPage();await p.goto(fileURL);await fillSetup(p);await p.locator('#advance').click();
   await p.locator('#setup-done').waitFor({state:'visible'});
   assert.equal(await p.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),false);
   await accepted(p);
   assert.equal(await p.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),false);
   assert.equal(await p.locator('.mk-spark').count(),0);
   await p.locator('#forget').click();
   await p.evaluate(()=>{const original=hashwasm.argon2id;window.stateOriginalKdf=original;hashwasm.argon2id=(...args)=>new Promise(resolve=>{window.releaseStateCheck=()=>resolve(original(...args));});});
   await p.locator('#master').fill('fictional wrong master');await p.locator('#advance').click();
   await p.waitForFunction(()=>document.getElementById('advance-text').textContent==='Checking...');
   assert.equal(await p.locator('#advance').isDisabled(),true);assert.equal(await p.locator('#generate-view').isVisible(),false);
   await p.evaluate(()=>window.releaseStateCheck());await p.locator('#fail.open').waitFor();
   // Verify full content, including after narrow rewraps, in both motion modes.
   // WebKit previously kept a stale fractional track and clipped the lower rows.
   for(const reducedMotion of ['no-preference','reduce']) {
    await p.emulateMedia({reducedMotion});
    for(const width of [1200,320,768,375]) {
     await p.setViewportSize({width,height:900});
     await p.waitForFunction(()=>{
      const wrap=document.getElementById('fail'),inner=wrap.querySelector('.fail');
      return Math.abs(wrap.getBoundingClientRect().height-inner.getBoundingClientRect().height
       -parseFloat(getComputedStyle(inner).marginTop))<0.5;
     });
    }
   }
   await p.locator('#reveal').click();assert.notEqual(await p.locator('#readout-text').textContent(),'');
   // XSS-looking input remains text, including when spaces are marked.
   await p.locator('#master').fill('<img src=x onerror=alert(1)>  ');
   assert.equal(await p.locator('#readout img').count(),0);
   assert.match(await p.locator('#readout-count').textContent(),/spaces at the ends are ignored/);
   await p.locator('#show-master').click();assert.equal(await p.locator('#readout-text').textContent(),'');
   await p.evaluate(()=>{hashwasm.argon2id=window.stateOriginalKdf;});
   await p.locator('#master').fill(defaultMaster);await p.locator('#advance').click();await accepted(p);
   assert.equal(await p.locator('#advance').evaluate(e=>e.classList.contains('busy')),false,'successful check must release the cursor');
   await p.locator('#forget').click();await p.locator('#master').fill(defaultMaster);
   assert.notEqual(await p.locator('#advance').evaluate(e=>getComputedStyle(e).cursor),'progress','idle Get started after Forget');
  }finally{await context.close();}
 });
}
module.exports={testStates};
