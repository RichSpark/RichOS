'use strict';
const assert=require('node:assert/strict');
const defaultMaster='fictional violet river lantern';
const verifierKey='richpass:verifier';
async function ready(page) {
 await page.waitForFunction(()=>!document.getElementById('start-status').textContent.startsWith('Checking'));
}
// Seed a fictional enrolled fixture for existing session/derivation owners. The
// dedicated checks below exercise the actual five-entry UI and persistence path.
async function enrollFixture(page,master=defaultMaster) {
 await ready(page);
 await page.evaluate(async master=>{
  const record=await RichPassVerifier.create(master);
  localStorage.setItem(RichPassVerifier.storageKey,await RichPassSession.encryptVerifier(record));
  try{RichPassSession.remove();}catch{/* A session-only denial is intentional in some fixtures. */}
 },master);
 await page.reload();await ready(page);
}
async function fillSetup(page,master=defaultMaster) {
 await ready(page);
 for(const [index,percent] of [40,60,80,95].entries()) {
  await page.locator('#master').fill(master);await page.locator('#advance').click();
  await page.waitForFunction(percent=>document.getElementById('setup-progress').getAttribute('aria-valuenow')===String(percent),percent);
  assert.equal(await page.locator('#master').inputValue(),'');
 }
 await page.locator('#master').fill(master);
}
async function accepted(page) {
 // Helpers acknowledge only after the real completion screen appears. Dedicated
 // state tests assert the completion barrier and exact words before clicking it.
 await page.waitForFunction(()=>!document.getElementById('generate-view').hidden
   ||document.getElementById('setup-done')?.hidden===false);
 if(await page.locator('#setup-done').isVisible())await page.locator('#lets-go').click();
 await page.locator('#generate-view').waitFor({state:'visible'});
 await page.waitForFunction(()=>!document.getElementById('session-warning').textContent.startsWith('Saving'));
}
async function testVerification({browser,check,fileURL}) {
 async function fresh(fn) {
  const context=await browser.newContext();
  try {await context.route(/^https?:/,r=>r.abort());const page=await context.newPage();await page.goto(fileURL);await ready(page);await fn(page,context);}
  finally{await context.close();}
 }
 await check('missing encryption support cannot offer a working setup or generation',async()=>{
  const context=await browser.newContext();
  try{await context.addInitScript(()=>Object.defineProperty(window,'crypto',{value:{},configurable:true}));
   const page=await context.newPage();await page.goto(fileURL);
   await page.getByText('This browser cannot run RichPass. Use a supported browser with JavaScript and WebAssembly enabled.').waitFor();
   assert.equal(await page.locator('#advance').isDisabled(),true);assert.equal(await page.locator('#generate-view').isVisible(),false);
  }finally{await context.close();}
 });
 await check('master verifier matches an independent Argon2id vector',async()=>{
  await fresh(async page=>{
   // argon2-cffi 25.1.0 / bindings 26.1.0, independent of hash-wasm.
   const record={version:1,salt:'AAECAwQFBgcICQoLDA0ODw==',hash:'YuCVsqZgk1c66Fp22dSIONiurdHrYOyYCMMT01O+Qu0='};
   assert.equal(await page.evaluate(async({master,record})=>RichPassVerifier.matches(master,record),{master:defaultMaster,record}),true);
   assert.equal(await page.evaluate(async({master,record})=>RichPassVerifier.matches(master,record),{master:defaultMaster+' typo',record}),false);
  });
 });
 await check('five sequential exact matches are required before any verifier is saved',async()=>{
  await fresh(async page=>{
   assert.equal(await page.locator('#setup-progress').getAttribute('aria-valuenow'),'20');
   await page.locator('#master').fill(defaultMaster);assert.equal(await page.locator('#advance').isEnabled(),true);
   // The helper completes only entries 1-4 and leaves entry 5 ready to submit.
   await page.locator('#master').fill('');await fillSetup(page);
   assert.equal(await page.evaluate(()=>localStorage.getItem('richpass:verifier')),null);
   assert.equal(await page.locator('#setup-progress').getAttribute('aria-valuenow'),'95');
   await page.locator('#master').fill(defaultMaster+' typo');
   assert.equal(await page.locator('#advance').isDisabled(),true);
   assert.equal(await page.locator('#master').getAttribute('aria-invalid'),'true');
   assert.match(await page.locator('#verdict-body').textContent(),/This doesn't match your first entry/);
   await page.locator('#master').fill(defaultMaster);
   assert.equal(await page.locator('#verdict-body').textContent(),'');
   assert.equal(await page.locator('#advance').isEnabled(),true);
   await page.locator('#show-master').click();assert.equal(await page.locator('#master').getAttribute('type'),'text');
   await page.evaluate(()=>{window.completedAfterSave=[];new MutationObserver(()=>{if(document.getElementById('setup-progress').getAttribute('aria-valuenow')==='100')window.completedAfterSave.push(!!localStorage.getItem('richpass:verifier'));}).observe(document.getElementById('setup-progress'),{attributes:true,attributeFilter:['aria-valuenow']});});
   await page.locator('#advance').click();await accepted(page);
   assert.equal(await page.locator('#master').inputValue(),'');
   assert.deepEqual(await page.evaluate(()=>window.completedAfterSave),[true]);
   const saved=await page.evaluate(()=>localStorage.getItem('richpass:verifier'));
   assert.ok(saved);assert.ok(!saved.includes('fictional'));assert.deepEqual(Object.keys(JSON.parse(saved)).sort(),['ciphertext','iv','version']);
   const info=await page.evaluate(async saved=>{
    const record=await RichPassSession.decryptVerifier(saved);
    return {keys:Object.keys(record).sort(),saltBytes:atob(record.salt).length,hashBytes:atob(record.hash).length,
      correct:await RichPassVerifier.matches('fictional violet river lantern',record),wrong:await RichPassVerifier.matches('fictional violet river lantner',record)};
   },saved);
   assert.deepEqual(info,{keys:['hash','salt','version'],saltBytes:16,hashBytes:32,correct:true,wrong:false});
   await assert.rejects(()=>page.evaluate(saved=>RichPassSession.decrypt(saved),saved));
   const tab=await page.evaluate(()=>sessionStorage.getItem('richpass:master'));
   await assert.rejects(()=>page.evaluate(tab=>RichPassSession.decryptVerifier(tab),tab));
   await page.locator('#forget').click();assert.equal(await page.evaluate(()=>localStorage.getItem('richpass:verifier')),saved);
   await page.locator('#master').fill(defaultMaster+' typo');await page.locator('#advance').click();
   await page.getByText("This doesn't match the saved passphrase fingerprint.").waitFor();
   assert.equal(await page.locator('#generate-view').isVisible(),false);assert.equal(await page.locator('#password').inputValue(),'');
   assert.equal(await page.evaluate(()=>sessionStorage.getItem('richpass:master')),null);
   const start=Date.now();await page.locator('#master').fill(defaultMaster);await page.locator('#advance').click();await accepted(page);
   console.log('  Verified entry including tab save: '+(Date.now()-start)+' ms');
   await page.reload();await accepted(page);assert.equal(await page.locator('#setup-progress').isVisible(),false);
  });
 });
 await check('confirmation mismatch is immediate, recoverable and Start over clears the first entry',async()=>{
  await fresh(async page=>{
   await page.locator('#master').fill('fictional first entry');await page.locator('#advance').press('Enter');
   for(const [step,percent] of [[1,40],[2,60],[3,80],[4,95]]) {
    assert.equal(await page.locator('#setup-progress').getAttribute('aria-valuenow'),String(percent));
    await page.locator('#master').fill('fictional ');assert.equal(await page.locator('#master').getAttribute('aria-invalid'),'false');
    assert.equal(await page.locator('#advance').isDisabled(),true);
    await page.locator('#master').pressSequentially('X');assert.equal(await page.locator('#master').getAttribute('aria-invalid'),'true');
    await page.locator('#master').press('Backspace');assert.equal(await page.locator('#master').getAttribute('aria-invalid'),'false');
    await page.locator('#master').fill('fictional first entry too long');assert.equal(await page.locator('#advance').isDisabled(),true);
    await page.locator('#master').fill('fictional first entry');assert.equal(await page.locator('#verdict-body').textContent(),'');
    if(step<4)await page.locator('#advance').press('Enter');
   }
   await page.locator('#master').fill('wrong');await page.locator('#start-over').click();
   assert.equal(await page.locator('#setup-progress').getAttribute('aria-valuenow'),'20');assert.equal(await page.locator('#master').inputValue(),'');
   assert.equal(await page.locator('#master').getAttribute('type'),'password');assert.equal(await page.locator('#advance').isDisabled(),true);
   assert.equal(await page.evaluate(()=>localStorage.getItem('richpass:verifier')),null);
   await fillSetup(page,'fictional corrected first entry');await page.locator('#advance').click();await accepted(page);
   const match=await page.evaluate(async()=>RichPassVerifier.matches('fictional corrected first entry',await RichPassSession.decryptVerifier(localStorage.getItem('richpass:verifier'))));assert.equal(match,true);
  });
 });
 await check('incomplete prefix gets feedback on Enter or a real disabled-button press',async()=>{
  await fresh(async page=>{
   await page.locator('#master').fill(defaultMaster+' ');await page.locator('#advance').click();
   await page.locator('#master').fill(defaultMaster);
   assert.equal(await page.locator('#advance').isDisabled(),true);
   assert.equal(await page.locator('#master').getAttribute('aria-invalid'),'false');
   await page.locator('#master').press('Enter');
   assert.equal(await page.locator('#master').getAttribute('aria-invalid'),'true');
   await page.locator('#start-over').click();assert.equal(await page.locator('#setup-progress').getAttribute('aria-valuenow'),'20');
   await page.locator('#master').fill(defaultMaster);await page.locator('#advance').click();
   for(let step=1;step<5;step++) {
    await page.locator('#master').fill(defaultMaster.slice(0,-1));
    assert.equal(await page.locator('#advance').isDisabled(),true);
    const bounds=await page.locator('#advance').boundingBox();
    await page.mouse.click(bounds.x+bounds.width/2,bounds.y+bounds.height/2);
    assert.equal(await page.locator('#master').getAttribute('aria-invalid'),'true','step '+(step+1)+' real pointer press');
    assert.match(await page.locator('#verdict-body').textContent(),/This doesn't match your first entry/);
    await page.locator('#master').fill(defaultMaster);assert.equal(await page.locator('#master').getAttribute('aria-invalid'),'false');
    if(step<4)await page.locator('#advance').click();
   }
   await page.locator('#advance').click();await accepted(page);
  });
 });
 await check('incomplete setup prefix gets feedback from a real touch press',async()=>{
  const context=await browser.newContext({viewport:{width:375,height:900},hasTouch:true,isMobile:true});
  try {
   const page=await context.newPage();await page.goto(fileURL);await ready(page);
   await page.locator('#master').fill(defaultMaster+' ');await page.locator('#advance').tap();
   await page.locator('#master').fill(defaultMaster);
   assert.equal(await page.locator('#advance').isDisabled(),true);
   const bounds=await page.locator('#advance').boundingBox();
   await page.touchscreen.tap(bounds.x+bounds.width/2,bounds.y+bounds.height/2);
   assert.equal(await page.locator('#master').getAttribute('aria-invalid'),'true');
   await page.locator('#start-over').tap();
   assert.equal(await page.locator('#setup-progress').getAttribute('aria-valuenow'),'20');
  }finally{await context.close();}
 });
 await check('one Back leaves after first setup; Forward restores the verified tab',async()=>{
  const context=await browser.newContext();
  try {
   const page=await context.newPage();
   await page.goto('data:text/html,<title>Earlier page</title><p>Before setup</p>');await page.goto(fileURL);
   await fillSetup(page);await page.locator('#advance').click();await accepted(page);
   await page.goBack();await page.getByText('Before setup',{exact:true}).waitFor();
   await page.goForward();await accepted(page);
   assert.equal(await page.locator('#master').inputValue(),'');assert.equal(await page.locator('#password').inputValue(),'');
  }finally{await context.close();}
 });
 await check('sequential setup fits every viewport and each step at 200 percent zoom',async()=>{
  await fresh(async page=>{
   const widths=require('./layout.cjs').viewportWidths();
   async function fieldsFit(label) {
    const fit=await page.evaluate(()=>({overflow:document.documentElement.scrollWidth>innerWidth+1,
     controls:['master','show-master','advance'].map(id=>{const r=document.getElementById(id).getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth+1;})}));
    assert.equal(fit.overflow,false,label);assert.deepEqual(fit.controls,[true,true,true],label+' field/button bounds');
   }
   for(const scheme of ['dark','light']) {
    await page.emulateMedia({colorScheme:scheme,reducedMotion:'reduce'});
    for(const width of widths) {
     await page.setViewportSize({width,height:844});await page.evaluate(()=>document.fonts.ready);
     assert.equal(await page.locator('#setup-progress').isVisible(),true);await fieldsFit(scheme+' '+width);
    }
   }
   await page.setViewportSize({width:1200,height:900});await page.evaluate(()=>document.body.style.zoom='200%');
   for(let index=0;index<5;index++) {
    await fieldsFit('200% zoom step '+(index+1));
    if(index<4){await page.locator('#master').fill(defaultMaster);await page.locator('#advance').click();}
   }
  });
 });
 await check('persistent verifier survives tab closure and rejects wrong master in a new tab',async()=>{
  await fresh(async(page,context)=>{
   await fillSetup(page);await page.locator('#advance').click();await accepted(page);
   await page.close();const next=await context.newPage();await next.goto(fileURL);await ready(next);
   assert.equal(await next.locator('#setup-progress').isVisible(),false);
   await next.locator('#master').fill(defaultMaster+' mistake');await next.locator('#advance').click();
   await next.getByText("This doesn't match the saved passphrase fingerprint.").waitFor();
   await next.locator('#master').fill(defaultMaster);await next.locator('#advance').click();await accepted(next);
  });
 });
 await check('encrypted verification and browser key survive a browser-process restart',async()=>{
  const fs=require('node:fs'),path=require('node:path'),os=require('node:os');
  const temp=fs.realpathSync(process.env.TMPDIR||os.tmpdir());
  const profile=fs.mkdtempSync(path.join(temp,'richpass-restart-'));
  let context;
  try {
   const type=browser.browserType();context=await type.launchPersistentContext(profile,{headless:true});
   await context.route(/^https?:/,route=>route.abort());let page=await context.newPage();await page.goto(fileURL);
   await fillSetup(page);await page.locator('#advance').click();await accepted(page);
   const saved=await page.evaluate(()=>localStorage.getItem('richpass:verifier'));
   await page.locator('#forget').click();await context.close();context=null;
   context=await type.launchPersistentContext(profile,{headless:true});await context.route(/^https?:/,route=>route.abort());
   page=await context.newPage();await page.goto(fileURL);await ready(page);
   assert.equal(await page.locator('#setup-progress').isVisible(),false);assert.equal(await page.evaluate(()=>localStorage.getItem('richpass:verifier')),saved);
   await page.locator('#master').fill(defaultMaster+' typo');await page.locator('#advance').click();
   await page.getByText("This doesn't match the saved passphrase fingerprint.").waitFor();
   await page.locator('#master').fill(defaultMaster);await page.locator('#advance').click();await accepted(page);
  }finally{if(context)await context.close();fs.rmSync(profile,{recursive:true,force:true});}
 });
 await check('fresh setup uses new salt without changing account passwords',async()=>{
  await fresh(async page=>{
   await fillSetup(page);await page.locator('#advance').click();await accepted(page);
   const first=await page.evaluate(async()=>RichPassSession.decryptVerifier(localStorage.getItem('richpass:verifier')));
   const before=await page.evaluate(master=>RichPass.derivePassword({master,domain:'example.com',label:'',version:'v1'}),defaultMaster);
   await page.evaluate(()=>localStorage.removeItem('richpass:verifier'));await page.reload();await ready(page);
   assert.equal(await page.locator('#setup-progress').isVisible(),true);
   await fillSetup(page);await page.locator('#advance').click();await accepted(page);
   const second=await page.evaluate(async()=>RichPassSession.decryptVerifier(localStorage.getItem('richpass:verifier')));
   assert.notEqual(first.salt,second.salt);assert.notEqual(first.hash,second.hash);
   assert.equal(await page.evaluate(master=>RichPass.derivePassword({master,domain:'example.com',label:'',version:'v1'}),defaultMaster),before);
  });
 });
 for(const failure of ['corrupt-record','missing-key','corrupt-key','denied-read','denied-write']) {
  await check(`${failure} returns to five-entry setup without bypassing verification`,async()=>{
   await fresh(async page=>{
    await fillSetup(page);await page.locator('#advance').click();await accepted(page);
    if(failure==='corrupt-record')await page.evaluate(()=>localStorage.setItem('richpass:verifier','not an encrypted record'));
    if(failure==='missing-key'||failure==='corrupt-key')await page.evaluate(async failure=>{
     const db=await new Promise(resolve=>{const r=indexedDB.open('richpass-session-keys',1);r.onsuccess=()=>resolve(r.result);});
     await new Promise((resolve,reject)=>{const tx=db.transaction('keys','readwrite'),store=tx.objectStore('keys');if(failure==='missing-key')store.delete('master-key');else store.put('corrupt','master-key');tx.oncomplete=resolve;tx.onabort=reject;});db.close();
    },failure);
    if(failure==='denied-read'||failure==='denied-write')await page.addInitScript(failure=>{
     const method=failure==='denied-read'?'getItem':'setItem',original=Storage.prototype[method];
     Storage.prototype[method]=function(...args){if(this===localStorage)throw new Error('synthetic verifier storage denial');return original.apply(this,args);};
    },failure);
    await page.reload();await ready(page);
    if(failure==='denied-write') {
     // An existing readable verifier remains valid; force a fresh setup explicitly.
     await page.evaluate(()=>localStorage.removeItem('richpass:verifier'));await page.reload();await ready(page);
    }
    assert.equal(await page.locator('#setup-progress').isVisible(),true);
    assert.equal(await page.locator('#generate-view').isVisible(),false);
    await fillSetup(page);await page.locator('#advance').click();
    if(failure.startsWith('denied')) {
     await page.waitForFunction(()=>document.getElementById('start-status').textContent.includes('could not be saved'));
     assert.equal(await page.locator('#generate-view').isVisible(),false);
    }else{await accepted(page);}
   });
  });
 }
 await check('recovered tab master must match the enrolled verifier',async()=>{
  await fresh(async page=>{
   await fillSetup(page);await page.locator('#advance').click();await accepted(page);
   await page.evaluate(async()=>sessionStorage.setItem('richpass:master',await RichPassSession.encrypt('fictional different unconfirmed master')));
   await page.reload();await ready(page);
   assert.equal(await page.locator('#generate-view').isVisible(),false);assert.equal(await page.locator('#setup-progress').isVisible(),false);
   assert.match(await page.locator('#start-status').textContent(),/could not be restored or verified/);
  });
 });
 await check('simultaneous first verifier encryption shares one atomically initialized key',async()=>{
  await fresh(async(first,context)=>{
   const second=await context.newPage();await second.goto(fileURL);await ready(second);
   const records=await Promise.all([first,second].map((page,index)=>page.evaluate(index=>RichPassSession.encryptVerifier({fictionalProbe:index}),index)));
   for(const page of [first,second])for(let index=0;index<2;index++)assert.deepEqual(await page.evaluate(record=>RichPassSession.decryptVerifier(record),records[index]),{fictionalProbe:index});
   assert.equal(await first.evaluate(()=>localStorage.getItem('richpass:verifier')),null);
   await second.close();
  });
 });
 await check('a changed verifier cancels an unfinished master check',async()=>{
  await fresh(async(page,context)=>{
   await fillSetup(page);await page.locator('#advance').click();await accepted(page);await page.locator('#forget').click();
   const other=await context.newPage();await other.goto(fileURL);await ready(other);
   await page.evaluate(()=>{const hash=hashwasm.argon2id;hashwasm.argon2id=async options=>{window.checkWaiting=true;await new Promise(resolve=>window.releaseCheck=resolve);try{return await hash(options);}finally{window.checkFinished=true;}};});
   await page.locator('#master').fill(defaultMaster);await page.locator('#advance').click();await page.waitForFunction(()=>window.checkWaiting);
   assert.equal(await page.evaluate(()=>!window.dispatchEvent(new Event('beforeunload',{cancelable:true}))),false,'ordinary checking must not prompt to leave');
   await other.evaluate(()=>localStorage.removeItem('richpass:verifier'));await page.locator('#setup-progress').waitFor({state:'visible'});
   await page.evaluate(()=>window.releaseCheck());await page.waitForFunction(()=>window.checkFinished);
   assert.equal(await page.locator('#generate-view').isVisible(),false);assert.equal(await page.evaluate(()=>sessionStorage.getItem('richpass:master')),null);
   assert.equal(await page.evaluate(()=>!window.dispatchEvent(new Event('beforeunload',{cancelable:true}))),false);
   await other.close();
  });
 });
 await check('verification changes cancel generated output across tabs',async()=>{
  await fresh(async(page,context)=>{
   await fillSetup(page);await page.locator('#advance').click();await accepted(page);
   await page.locator('#domain').fill('example.com');
   await page.evaluate(()=>{hashwasm.argon2id=()=>new Promise(resolve=>window.releaseGeneration=resolve);});
   await page.locator('#generate').click();await page.waitForFunction(()=>!!window.releaseGeneration);
   const other=await context.newPage();await other.goto(fileURL);await ready(other);
   await other.evaluate(()=>localStorage.removeItem('richpass:verifier'));
   await page.locator('#setup-progress').waitFor({state:'visible'});
   await page.evaluate(()=>window.releaseGeneration(new Uint8Array(32)));
   assert.equal(await page.locator('#password').inputValue(),'');assert.equal(await page.locator('#copy').isDisabled(),true);
   await other.close();
  });
 });
}
module.exports={enrollFixture,ready,fillSetup,accepted,testVerification,defaultMaster,verifierKey};
