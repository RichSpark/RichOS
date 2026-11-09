'use strict';
// CEO round-64.1 content and latest 1.1.3 overrides, using real encrypted records.
const assert=require('node:assert/strict'),path=require('node:path');
const {pathToFileURL}=require('node:url');
const {ready,enrollFixture,fillSetup,accepted,defaultMaster}=require('./verification.cjs');
const {measure,viewportWidths}=require('./layout.cjs');
const mockURL=pathToFileURL(path.join(__dirname,'RichPass.master-reference.html')).href;
const warning1='⚠️WARNING!⚠️ Changing your master passphrase will COMPLETELY change every generated password! Are you sure you want to proceed?';
const warning2='⛔STOP!⛔ Are you ABSOLUTELY SURE you understand the ⚠️consequences⚠️ of changing your master passphrase?';
const warningMore='Do you understand that after changing your master passphrase you will NOT be able to get from RichPass ANY passwords you previously generated with your current master passphrase?';
async function records(p){return p.evaluate(()=>({verifier:RichPassVerifier.read(),session:RichPassSession.read()}));}
async function login(p){await enrollFixture(p);await p.locator('#master').fill(defaultMaster);await p.locator('#advance').click();await accepted(p);}
async function openMaster(p){const scope=await p.locator('#generate-view').isVisible()?'#generate-view':'#home-view';await p.locator(scope+' a[href="#/master"]').click();await p.locator('#master-view').waitFor({state:'visible'});}
async function startChange(p){
 await p.locator('#change-master').click();await p.locator('#warn-1[open]').waitFor();
 assert.equal(await p.evaluate(()=>document.activeElement.id),'warn-1-cancel');
 await p.locator('#warn-1-yes').click();await p.locator('#warn-2[open]').waitFor();
 assert.equal(await p.evaluate(()=>document.activeElement.id),'warn-2-cancel');
 await p.locator('#warn-2-yes').click();await p.locator('#keep-row').waitFor({state:'visible'});
 assert.equal(await p.locator('#setup-progress').getAttribute('aria-valuenow'),'20');
}
async function password(p){await p.locator('#domain').fill('example.com');await p.locator('#generate').click();await p.waitForFunction(()=>!!document.getElementById('password').value);return p.locator('#password').inputValue();}
async function testMaster({browser,check,fileURL}){
 async function fresh(fn,options={}){const c=await browser.newContext({reducedMotion:'reduce',...options}),errors=[],network=[];try{await c.route(/^https?:/,r=>{network.push(r.request().url());return r.abort();});const p=await c.newPage();p.on('pageerror',e=>errors.push(e.message));await p.goto(fileURL);await ready(p);await fn(p,c);assert.deepEqual(errors,[]);assert.deepEqual(network,[]);}finally{await c.close();}}
 await check('Master ideas and both back buttons work before and after setup',async()=>{
  await fresh(async p=>{
   assert.equal(await p.locator('#home-master').isVisible(),true);await openMaster(p);
   assert.equal(await p.locator('#master-title').textContent(),'Master Passphrase Ideas');
   assert.deepEqual(await p.locator('.method .steps').evaluateAll(nodes=>nodes.map(n=>n.children.length)),[2,2,2]);
   assert.equal(await p.locator('#change-master').isVisible(),false,'there is no fingerprint to change before enrollment');
   assert.equal(await p.locator('#master-back-top').getAttribute('href'),await p.locator('#master-return').getAttribute('href'));
   await p.locator('#master-back-top').click();await p.locator('#setup-progress').waitFor({state:'visible'});
   await fillSetup(p);await p.locator('#advance').click();await p.locator('#setup-done').waitFor({state:'visible'});
   await p.waitForFunction(()=>!document.getElementById('start-status').textContent.includes('Saving'));await openMaster(p);
   assert.equal(await p.locator('#change-master').textContent(),'Change master fingerprint');
   assert.equal(await p.locator('#master-return').textContent(),'Go back');await p.locator('#master-return').click();
   await p.locator('#setup-done').waitFor({state:'visible'});await accepted(p);
   assert.equal(await p.locator('#generate').textContent(),'Derive password from master + above input');
   await openMaster(p);assert.equal(await p.locator('#master-return').textContent(),'Go to Generate');await p.locator('#master-back-top').click();
   await p.locator('#forget').click();await openMaster(p);assert.equal(await p.locator('#master-return').textContent(),'Go to master entry');
  });
 });
 await check('both warning exits are write-free, Cancel-focused and keyboard safe',async()=>{
  await fresh(async p=>{
   await login(p);const old=await records(p);await openMaster(p);
   for(const stage of [1,2])for(const exit of ['cancel','escape','backdrop','enter','navigation']){
    await p.locator('#change-master').click();if(stage===2)await p.locator('#warn-1-yes').click();
    const d=p.locator('#warn-'+stage);await d.waitFor({state:'visible'});
    assert.equal(await p.locator('#warn-1-text').textContent(),warning1);
    assert.equal(await p.locator('#warn-2-text').textContent(),warning2);
    assert.equal(await p.locator('#warn-2-more').textContent(),warningMore);
    assert.equal(await p.evaluate(()=>document.activeElement.id),'warn-'+stage+'-cancel');
    if(exit==='cancel')await p.locator('#warn-'+stage+'-cancel').click();
    else if(exit==='escape')await p.keyboard.press('Escape');
    else if(exit==='backdrop')await p.mouse.click(2,2);
    else if(exit==='enter')await p.keyboard.press('Enter');
    else await p.goBack();
    await p.waitForFunction(()=>!document.querySelector('dialog[open]'));assert.deepEqual(await records(p),old);
    assert.equal(await p.locator('#keep-row').isVisible(),false);
    if(exit==='navigation')await openMaster(p);else assert.equal(await p.evaluate(()=>document.activeElement.id),'change-master');
   }
  });
 });
 await check('replacement retains the old records until Finish and Keep cancels without changing passwords',async()=>{
  await fresh(async p=>{
   await login(p);const oldPassword=await password(p),old=await records(p);await openMaster(p);await startChange(p);
   await fillSetup(p,'fictional cobalt harbor chorus');assert.deepEqual(await records(p),old);
   await p.evaluate(()=>document.getElementById('account-form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true})));
   assert.equal(await p.locator('#password').inputValue(),'');assert.equal(await p.locator('#generate').isDisabled(),true);
   assert.equal(await p.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),false);
   await p.locator('#keep-master').click();await p.locator('#master-view').waitFor();assert.deepEqual(await records(p),old);
   await p.locator('#master-return').click();assert.equal(await password(p),oldPassword);
  });
 });
 await check('replacement commits only five exact matches and re-enrolling the same master preserves outputs',async()=>{
  await fresh(async p=>{
   await login(p);const oldPassword=await password(p),old=await records(p);const nextMaster='fictional cobalt harbor chorus';
   await openMaster(p);await startChange(p);await fillSetup(p,nextMaster);assert.deepEqual(await records(p),old);
   await p.locator('#advance').click();await p.locator('#setup-done').waitFor();assert.notEqual((await records(p)).verifier,old.verifier);
   assert.equal(await p.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),false);await accepted(p);
   const nextPassword=await password(p);assert.notEqual(nextPassword,oldPassword);const next=await records(p);
   await openMaster(p);await startChange(p);await fillSetup(p,nextMaster);await p.locator('#advance').click();await accepted(p);
   assert.notEqual((await records(p)).verifier,next.verifier);assert.equal(await password(p),nextPassword);
   await p.reload();await accepted(p);assert.equal(await password(p),nextPassword);
   await p.locator('#forget').click();await p.locator('#master').fill(defaultMaster);await p.locator('#advance').click();await p.locator('#fail.open').waitFor();
   assert.equal(await p.locator('#generate-view').isVisible(),false);
   await p.locator('#master').fill(nextMaster);await p.locator('#advance').click();await accepted(p);assert.equal(await password(p),nextPassword);
  });
 });
 await check('replacement cancellation rejects late KDF/encryption results and retains original storage',async()=>{
  for(const phase of ['kdf','encrypt'])await fresh(async p=>{
   await login(p);const old=await records(p);await openMaster(p);await startChange(p);
   await p.evaluate(phase=>{
    if(phase==='kdf'){
     const original=hashwasm.argon2id;window.restorePending=()=>{hashwasm.argon2id=original;};
     hashwasm.argon2id=(...args)=>new Promise(resolve=>{window.releasePending=async()=>{const value=await original(...args);window.pendingFinished=true;resolve(value);};});
    }else{
     const original=crypto.subtle.encrypt.bind(crypto.subtle);window.restorePending=()=>{crypto.subtle.encrypt=original;};
     crypto.subtle.encrypt=(...args)=>new Promise(resolve=>{window.releasePending=async()=>{const value=await original(...args);window.pendingFinished=true;resolve(value);};});
    }
   },phase);
   await fillSetup(p,'fictional deferred new entry');await p.locator('#advance').click();await p.waitForFunction(()=>typeof window.releasePending==='function');
   await p.locator('#keep-master').click();await p.locator('#master-view').waitFor();await p.evaluate(()=>window.releasePending());await p.waitForFunction(()=>window.pendingFinished);
   await p.evaluate(()=>window.restorePending());assert.deepEqual(await records(p),old,phase+' must not publish after cancel');
   await p.locator('#master-return').click();await password(p);
  });
 });
 await check('failed replacement save and reload preserve the previously saved fingerprint',async()=>{
  await fresh(async p=>{
   await login(p);const old=await records(p);await openMaster(p);await startChange(p);
   await p.evaluate(()=>{const set=Storage.prototype.setItem;Storage.prototype.setItem=function(key,value){if(key==='richpass:verifier')throw new Error('fixture denied');return set.call(this,key,value);};});
   await fillSetup(p,'fictional failed new entry');await p.locator('#advance').click();await p.getByText(/Master verification could not be saved/).waitFor();
   assert.deepEqual(await records(p),old);assert.equal(await p.locator('#setup-done').isVisible(),false);await p.locator('#keep-master').click();
   assert.deepEqual(await records(p),old);await p.reload();await ready(p);await p.locator('#master-return').click();await accepted(p);assert.ok(await password(p));
  });
 });
 await check('browser Back cancels a draft on the first press and reload abandons it',async()=>{
  await fresh(async p=>{
   await login(p);const old=await records(p);await openMaster(p);await startChange(p);await p.locator('#master').fill('fictional draft');
   await p.goBack();await p.locator('#generate-view').waitFor();assert.equal(await p.locator('#keep-row').isVisible(),false);assert.deepEqual(await records(p),old);
   await openMaster(p);await startChange(p);await p.reload();await accepted(p);assert.equal(await p.locator('#keep-row').isVisible(),false);assert.deepEqual(await records(p),old);
   await p.locator('#forget').click();await openMaster(p);await startChange(p);await p.goBack();
   await p.locator('#home-view').waitFor();assert.equal(await p.locator('#keep-row').isVisible(),false);assert.equal(await p.locator('#setup-progress').isVisible(),false);
  });
 });
 await check('changed verification in another tab cancels a draft before Finish can overwrite it',async()=>{
  await fresh(async(p,c)=>{
   await login(p);await openMaster(p);await startChange(p);await p.locator('#master').fill('fictional unfinished draft');
   const other=await c.newPage();await other.goto(fileURL);await ready(other);
   await other.evaluate(async()=>{const record=await RichPassVerifier.create('fictional other confirmed master');const saved=await RichPassSession.encryptVerifier(record);RichPassVerifier.write(RichPassVerifier.read(),saved);});
   const changed=(await records(other)).verifier;
   // File-page storage events can be suppressed (observed in WebKit). The real
   // Finish action must independently re-read authority before replacing it.
   if(await p.locator('#keep-row').isVisible())await fillSetup(p,'fictional other draft master');
   else await p.locator('#master').fill('fictional other draft master');
   await p.locator('#advance').click();
   await p.waitForFunction(()=>document.getElementById('keep-row').hidden&&!document.getElementById('master').disabled);
   assert.equal((await records(p)).verifier,changed,'the newer record must not be overwritten');
   assert.equal(await p.locator('#keep-row').isVisible(),false);assert.equal(await p.locator('#password').inputValue(),'');assert.equal(await p.locator('#generate-view').isVisible(),false);
   await p.locator('#master').fill('fictional other confirmed master');await p.locator('#advance').click();await accepted(p);
  });
 });
 await check('home attention waits exactly three controlled seconds only with no saved fingerprint',async()=>{
  for(const reducedMotion of ['no-preference','reduce']){
   const c=await browser.newContext({reducedMotion});try{const p=await c.newPage();await p.clock.install({time:new Date(100000)});await p.clock.pauseAt(new Date(101000));
    await p.goto(fileURL);await ready(p);assert.equal(await p.locator('#home-master').isVisible(),true);
    assert.equal(await p.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),false);
    await p.clock.runFor(2999);assert.equal(await p.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),false);
    await p.clock.runFor(1);assert.equal(await p.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),true);
    if(reducedMotion==='reduce')assert.equal(await p.locator('#home-master .mk-glow').evaluate(e=>getComputedStyle(e).animationName),'none');
    await p.clock.resume();await p.emulateMedia({reducedMotion:'reduce'});await fillSetup(p);await p.locator('#advance').click();await p.locator('#setup-done').waitFor();
    assert.equal(await p.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),false);
   }finally{await c.close();}
  }
 });
 await check('saved, unreadable and unknown fingerprints never enable attention on load or replacement',async()=>{
  for(const kind of ['saved','unreadable','denied'])await fresh(async(p,c)=>{
   if(kind==='saved')await enrollFixture(p);
   else if(kind==='unreadable')await p.evaluate(()=>localStorage.setItem('richpass:verifier','{"invalid":true}'));
   else await c.addInitScript(()=>{const get=Storage.prototype.getItem;Storage.prototype.getItem=function(key){if(key==='richpass:verifier')throw new Error('fixture denied');return get.call(this,key);};});
   const next=await c.newPage();await next.clock.install();await next.goto(fileURL);await ready(next);await next.clock.pauseAt(new Date(await next.evaluate(()=>Date.now()+1000)));await next.clock.runFor(10000);
   assert.equal(await next.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),false,kind);
   if(kind==='saved'){
    await next.clock.resume();await openMaster(next);await startChange(next);await next.clock.pauseAt(new Date(await next.evaluate(()=>Date.now()+1000)));await next.clock.runFor(10000);
    assert.equal(await next.locator('#home-master').evaluate(e=>e.classList.contains('beacon')),false,'replacement has a saved fingerprint');
   }
  });
 });
 await check('Master page and warnings match round64.1 with the requested labels at every boundary',async()=>{
  for(const scheme of ['dark','light']){
   const contexts=await Promise.all([browser.newContext({colorScheme:scheme,reducedMotion:'reduce'}),browser.newContext({colorScheme:scheme,reducedMotion:'reduce'})]);
   try{
    const pages=await Promise.all(contexts.map(c=>c.newPage())),[reference,actual]=pages;
    await reference.goto(mockURL);await reference.locator('#setup-done').waitFor();
    await reference.evaluate(()=>{
     document.getElementById('richpass-css').textContent=document.getElementById('richpass-css').textContent.replace(/    \/\* Mockup chrome \(not RichPass\): room under [\s\S]*?    #master-view\.page \{ padding-bottom: 5rem; \}\n/,'');
     document.getElementById('change-master').textContent='Change master fingerprint';document.getElementById('mock-dock').hidden=true;
    });
    await actual.goto(fileURL);await fillSetup(actual);await actual.locator('#advance').click();await actual.locator('#setup-done').waitFor();await actual.waitForFunction(()=>!document.getElementById('start-status').textContent.includes('Saving'));
    for(const p of pages)await openMaster(p);
    async function compare(name,selectors){for(const width of viewportWidths()){
     for(const p of pages){await p.setViewportSize({width,height:900});await p.mouse.move(0,0);
      // The real five-entry flow and a storage-free mock have different input
      // histories. Compare the same unfocused presentation; focus/trapping and
      // Cancel defaults are asserted independently above.
      await p.evaluate(()=>document.activeElement?.blur());
     }
     const [expected,got]=await Promise.all(pages.map(p=>measure(p,selectors)));
     for(let i=0;i<selectors.length;i++){assert.deepEqual(got[i].style,expected[i].style,`${scheme} ${name} ${width} ${selectors[i]} style`);for(const dim of ['x','width','height'])assert.ok(Math.abs(got[i][dim]-expected[i][dim])<0.25,`${scheme} ${name} ${width} ${selectors[i]} ${dim}: ${got[i][dim]} vs ${expected[i][dim]}`);}
     assert.ok(await actual.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'Master horizontal overflow '+width);
     assert.ok(await actual.locator('.master-copy .panel').evaluateAll(nodes=>nodes.every(n=>n.scrollHeight<=n.clientHeight+1)),'Master panel clips text '+width);
    }}
    await compare('Master',['#master-view .col','.master-top','#master-title','#master-back-top','.master-copy','.master-intro','#method-photos','#method-music','#method-places','.master-actions','#master-return','#change-master']);
    for(const p of pages)await p.locator('#change-master').click();await compare('warning1',['#warn-1','.warn-caution .warn-body','#warn-1-text','#warn-1-cancel','#warn-1-yes']);
    for(const p of pages)await p.locator('#warn-1-yes').click();await compare('warning2',['#warn-2','.warn-stop .warn-body','#warn-2-text','#warn-2-more','#warn-2-cancel','#warn-2-yes']);
   }finally{await Promise.all(contexts.map(c=>c.close()));}
  }
 });
}
module.exports={testMaster};
