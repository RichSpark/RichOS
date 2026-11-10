'use strict';
// Real browser storage and crypto, with controlled failures and public fictional inputs.
const assert = require('node:assert/strict');
const {enrollFixture,ready,accepted}=require('./verification.cjs');
const storageKey = 'richpass:master', databaseName = 'richpass-session-keys';
async function settled(page) {
 await page.waitForFunction(() => !document.getElementById('session-warning').textContent.startsWith('Saving'));
}
// Independent envelope decoder: does not call the production decrypt implementation.
async function restoredMaster(page) {
 return page.evaluate(async ({storageKey,databaseName}) => {
  const saved=sessionStorage.getItem(storageKey);if(!saved)return null;
  const record=JSON.parse(saved);
  const database=await new Promise((resolve,reject)=>{const r=indexedDB.open(databaseName,1);r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
  try {
   const key=await new Promise((resolve,reject)=>{const r=database.transaction('keys').objectStore('keys').get('master-key');r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
   const bytes=text=>Uint8Array.from(atob(text),c=>c.charCodeAt(0));
   const decoded=await crypto.subtle.decrypt({name:'AES-GCM',iv:bytes(record.iv),additionalData:new TextEncoder().encode('RichPass encrypted tab master v1'),tagLength:128},key,bytes(record.ciphertext));
   return JSON.parse(new TextDecoder().decode(decoded));
  } finally {database.close();}
 },{storageKey,databaseName});
}
async function testSessions({browser,check,fileURL}) {
 async function fresh(fn,init,arg) {
  const context=await browser.newContext();
  try {
   await context.route(/^https?:/,route=>route.abort());
   if(init)await context.addInitScript(init,arg);
   const page=await context.newPage();await page.goto(fileURL);
   await enrollFixture(page);
   await fn(page,context);
  } finally {await context.close();}
 }
 async function enter(page,master='fictional violet river lantern',wait=true) {
  await page.locator('#master').fill(master);await page.locator('#advance').click();
  await page.locator('#generate-view').waitFor({state:'visible'});if(wait)await settled(page);
 }
 // A synthetic cancelable event checks cleanup without waiting to prove an
 // absence. Real browser dialogs below separately prove the navigation behavior.
 async function guardActive(page) {
  return page.evaluate(()=>!window.dispatchEvent(new Event('beforeunload',{cancelable:true})));
 }
 async function holdEncryption(page) {
  await page.evaluate(()=>{
   const encrypt=crypto.subtle.encrypt.bind(crypto.subtle);
   crypto.subtle.encrypt=async(...args)=>{
    window.encryptWaiting=true;
    await new Promise((resolve,reject)=>{window.releaseEncrypt=resolve;window.rejectEncrypt=()=>reject(new Error('synthetic encryption denial'));});
    return encrypt(...args);
   };
  });
 }
 await check('pending save confirms reload; Stay preserves the entry and completed save removes the guard',async()=>{
  await fresh(async page=>{
   assert.equal(await guardActive(page),false);
   await holdEncryption(page);await enter(page,undefined,false);
   await page.waitForFunction(()=>window.encryptWaiting);
   assert.equal(await guardActive(page),true);
   await page.locator('#domain').fill('example.com');assert.equal(await page.locator('#generate').isDisabled(),true);
   await page.evaluate(()=>window.entryMarker='same live document');
   const prompt=page.waitForEvent('dialog');
   // location.reload requests the real browser confirmation without waiting for
   // a load event that correctly never happens when the user chooses Stay.
   const reload=page.evaluate(()=>{location.reload();});
   const dialog=await prompt;assert.equal(dialog.type(),'beforeunload');await dialog.dismiss();await reload;
   assert.equal(await page.evaluate(()=>window.entryMarker),'same live document');
   assert.equal(await page.locator('#generate-view').isVisible(),true);
   await page.evaluate(()=>window.releaseEncrypt());await settled(page);
   assert.equal(await guardActive(page),false);
   assert.equal(await restoredMaster(page),'fictional violet river lantern');
   let unexpected=0;page.on('dialog',async dialog=>{unexpected++;await dialog.dismiss();});
   await page.reload();await page.locator('#generate-view').waitFor({state:'visible'});
   assert.equal(unexpected,0);assert.equal(await guardActive(page),false);
  });
 });
 await check('pending save confirms tab close; choosing Leave permits closing',async()=>{
  await fresh(async page=>{
   await holdEncryption(page);await enter(page,undefined,false);await page.waitForFunction(()=>window.encryptWaiting);
   const prompt=page.waitForEvent('dialog');
   await page.close({runBeforeUnload:true});
   const dialog=await prompt;assert.equal(dialog.type(),'beforeunload');await dialog.accept();
   if(!page.isClosed())await page.waitForEvent('close');
   assert.equal(page.isClosed(),true);
  });
 });
 for(const ending of ['failure','timeout','forget']) {
  await check(`pending-save confirmation is removed after ${ending}`,async()=>{
   await fresh(async page=>{
    if(ending==='timeout')await page.clock.install();
    await holdEncryption(page);await enter(page,undefined,false);await page.waitForFunction(()=>window.encryptWaiting);
    assert.equal(await guardActive(page),true);
    if(ending==='failure')await page.evaluate(()=>window.rejectEncrypt());
    else if(ending==='timeout')await page.clock.fastForward(5001);
    else await page.locator('#forget').click();
    await settled(page);assert.equal(await guardActive(page),false);
    assert.equal(await page.evaluate(key=>sessionStorage.getItem(key),storageKey),null);
    let unexpected=0;page.on('dialog',async dialog=>{unexpected++;await dialog.dismiss();});
    await page.reload();await page.locator('#home-view').waitFor({state:'visible'});
    assert.equal(unexpected,0);
   });
  });
 }
 await check('non-extractable stored key, independent decoding and fresh IV preserve exact master text',async()=>{
  await fresh(async page=>{
   const master='fictional 🗝️ e\u0301 \ud800 text';
   await enrollFixture(page,master);
   // The automation text-input protocol replaces lone surrogates. Construct that
   // code unit in the page to exercise the application's exact-string contract.
   await page.evaluate(()=>{const field=document.getElementById('master');field.value='fictional 🗝️ e\u0301 '+String.fromCharCode(0xd800)+' text';field.dispatchEvent(new Event('input'));document.getElementById('advance').click();});
   await accepted(page);assert.equal(await restoredMaster(page),master);
   const first=await page.evaluate(key=>JSON.parse(sessionStorage.getItem(key)),storageKey);
   const keyInfo=await page.evaluate(async databaseName=>{
    const db=await new Promise(resolve=>{const r=indexedDB.open(databaseName,1);r.onsuccess=()=>resolve(r.result);});
    try {
     const key=await new Promise(resolve=>{const r=db.transaction('keys').objectStore('keys').get('master-key');r.onsuccess=()=>resolve(r.result);});
     let exported=false;try {await crypto.subtle.exportKey('raw',key);exported=true;}catch{}
     return {extractable:key.extractable,algorithm:key.algorithm,length:key.usages.length,exported};
    } finally {db.close();}
   },databaseName);
   assert.deepEqual(keyInfo,{extractable:false,algorithm:{name:'AES-GCM',length:256},length:2,exported:false});
   await page.reload();await page.locator('#generate-view').waitFor({state:'visible'});assert.equal(await restoredMaster(page),master);
   await page.locator('#forget').click();await enter(page,master);
   const second=await page.evaluate(key=>JSON.parse(sessionStorage.getItem(key)),storageKey);
   assert.notEqual(first.iv,second.iv);assert.notEqual(first.ciphertext,second.ciphertext);
  });
 });
 for(const failure of ['ciphertext','missing-key','corrupt-key','unknown-format']) {
  await check(`recovery rejects ${failure} without replacing the key or the record`,async()=>{
   await fresh(async page=>{
    await enter(page);
    await page.evaluate(async ({failure,storageKey,databaseName})=>{
     if(failure==='ciphertext'||failure==='unknown-format') {
      const r=JSON.parse(sessionStorage.getItem(storageKey));
      if(failure==='ciphertext')r.ciphertext=(r.ciphertext[0]==='A'?'B':'A')+r.ciphertext.slice(1);else r.version=2;
      sessionStorage.setItem(storageKey,JSON.stringify(r));return;
     }
     const db=await new Promise(resolve=>{const r=indexedDB.open(databaseName,1);r.onsuccess=()=>resolve(r.result);});
     try {await new Promise((resolve,reject)=>{const tx=db.transaction('keys','readwrite');const store=tx.objectStore('keys');if(failure==='missing-key')store.delete('master-key');else store.put('corrupt','master-key');tx.oncomplete=resolve;tx.onabort=()=>reject(tx.error);});}finally{db.close();}
    },{failure,storageKey,databaseName});
    const saved=await page.evaluate(key=>sessionStorage.getItem(key),storageKey);
    await page.reload();await ready(page);
    assert.match(await page.locator('#start-status').textContent(),/could not be restored|could not be accessed or decrypted/);
    assert.equal(await page.locator('#home-view').isVisible(),true);
    assert.equal(await page.evaluate(key=>sessionStorage.getItem(key),storageKey),saved);
    if(failure==='missing-key'||failure==='corrupt-key') {
     assert.equal(await page.evaluate(async databaseName=>{
      const db=await new Promise(resolve=>{const r=indexedDB.open(databaseName,1);r.onsuccess=()=>resolve(r.result);});
      try{return await new Promise(resolve=>{const r=db.transaction('keys').objectStore('keys').get('master-key');r.onsuccess=()=>resolve(r.result??null);});}finally{db.close();}
     },databaseName),failure==='missing-key'?null:'corrupt');
    }
   });
  });
 }
 for(const failure of ['key-storage','encryption']) {
  await check(`${failure} failure warns and keeps the live page usable without plaintext fallback`,async()=>{
   await fresh(async page=>{
    await page.evaluate(failure=>{
     // WebKit can replace an IndexedDB instance wrapper, losing an own-method
     // override. Patch the realm prototype so the intended failure persists.
     if(failure==='key-storage')Object.getPrototypeOf(indexedDB).open=()=>{throw new Error('synthetic denied key storage');};
     else crypto.subtle.encrypt=async()=>{throw new Error('synthetic crypto failure');};
    },failure);
    await enter(page);assert.match(await page.locator('#session-warning').textContent(),/page only/);
    assert.equal(await page.evaluate(key=>sessionStorage.getItem(key),storageKey),null);
    await page.locator('#domain').fill('example.com');await page.locator('#generate').click();
    await page.waitForFunction(()=>!!document.getElementById('password').value);
   });
  });
 }
 await check('hanging key storage has a bounded warning and never claims a session was saved',async()=>{
  await fresh(async page=>{
   await page.clock.install();
   await page.evaluate(()=>Object.getPrototypeOf(indexedDB).open=()=>{window.keyOpenWaiting=true;return {};});
   await enter(page,undefined,false);await page.waitForFunction(()=>window.keyOpenWaiting);await page.clock.fastForward(5001);await settled(page);
   assert.match(await page.locator('#session-warning').textContent(),/page only/);
   assert.equal(await page.evaluate(key=>sessionStorage.getItem(key),storageKey),null);
   await page.locator('#domain').fill('example.com');assert.equal(await page.locator('#generate').isEnabled(),true);
  });
 });
 await check('Forget cancels a save already encrypting and cannot be undone by its late result',async()=>{
  await fresh(async page=>{
   await page.evaluate(()=>{
    const encrypt=crypto.subtle.encrypt.bind(crypto.subtle);
    crypto.subtle.encrypt=async(...args)=>{window.encryptWaiting=true;await new Promise(resolve=>window.releaseEncrypt=resolve);try{return await encrypt(...args);}finally{window.encryptFinished=true;}};
   });
   await enter(page,undefined,false);await page.waitForFunction(()=>window.encryptWaiting);
   await page.locator('#forget').click();await page.evaluate(()=>window.releaseEncrypt());await page.waitForFunction(()=>window.encryptFinished);
   assert.equal(await page.evaluate(key=>sessionStorage.getItem(key),storageKey),null);
   assert.equal(await page.locator('#home-view').isVisible(),true);
   await page.reload();assert.equal(await page.locator('#advance').isDisabled(),true);
  });
 });
 await check('timed-out recovery cannot overwrite a subsequently entered master',async()=>{
  await fresh(async page=>{
   await enter(page);
   await page.addInitScript(()=>{
    const decrypt=crypto.subtle.decrypt.bind(crypto.subtle);
    crypto.subtle.decrypt=async(...args)=>{if(new TextDecoder().decode(args[0].additionalData)!=='RichPass encrypted tab master v1')return decrypt(...args);window.decryptWaiting=true;await new Promise(resolve=>window.releaseDecrypt=resolve);try{return await decrypt(...args);}finally{window.decryptFinished=true;}};
   });
   await page.clock.install();await page.reload();await page.waitForFunction(()=>window.decryptWaiting);
   await page.clock.fastForward(5001);
   await page.waitForFunction(()=>document.getElementById('start-status').textContent.includes('could not be restored'));
   const replacement='fictional violet river lantern';await enter(page,replacement);
   await page.evaluate(()=>window.releaseDecrypt());await page.waitForFunction(()=>window.decryptFinished);
   // Use the real native decrypt again for the independent assertion.
   await page.evaluate(()=>{delete crypto.subtle.decrypt;});
   assert.equal(await restoredMaster(page),replacement);
   await page.locator('#domain').fill('example.com');await page.locator('#generate').click();
   await page.waitForFunction(()=>!!document.getElementById('password').value);
   const actual=await page.locator('#password').inputValue();
   const expected=await page.evaluate(master=>RichPass.derivePassword({master,domain:'example.com',label:'',version:''}),replacement);
   assert.equal(actual,expected);
  });
 });
 await check('independent verified tabs share a key; Forget preserves the other tab',async()=>{
  await fresh(async(first,context)=>{
   const second=await context.newPage();await second.goto(fileURL);await ready(second);
   await enter(first);await enter(second);
   assert.equal(await restoredMaster(first),'fictional violet river lantern');
   assert.equal(await restoredMaster(second),'fictional violet river lantern');
   await first.locator('#forget').click();await second.reload();await accepted(second);
   assert.equal(await restoredMaster(second),'fictional violet river lantern');
   assert.equal(await first.evaluate(key=>sessionStorage.getItem(key),storageKey),null);
  });
 });
 await check('a known saved record still warns after failed clearing during replacement entry',async()=>{
  await fresh(async page=>{
   await enter(page);
   const saved=await page.evaluate(key=>sessionStorage.getItem(key),storageKey);
   await page.evaluate(()=>Storage.prototype.removeItem=function(){throw new Error('synthetic failed clearing');});
   await page.locator('#forget').click();
   assert.match(await page.locator('#start-status').textContent(),/could not confirm clearing/);
   await enter(page,'fictional violet river lantern');
   assert.match(await page.locator('#session-warning').textContent(),/earlier session may remain/);
   assert.equal(await page.evaluate(key=>sessionStorage.getItem(key),storageKey),saved);
   assert.equal(await restoredMaster(page),'fictional violet river lantern');
   await page.locator('#domain').fill('example.com');await page.locator('#generate').click();
   await page.waitForFunction(()=>!!document.getElementById('password').value);
  });
 });
 await check('browser recommendation is advisory and never selects weaker storage',async()=>{
  const macSafari='Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Version/18.6 Safari/605.1.15';
  const cases=[
   {name:'Mac Safari',ua:macSafari,touchPoints:0,hidden:true},
   {name:'desktop-mode iPad Safari',ua:macSafari,touchPoints:5,hidden:false},
   {name:'Mac-style single-touch device',ua:macSafari,touchPoints:1,hidden:false},
   {name:'Mac-style unknown touch support',ua:macSafari,touchPoints:undefined,hidden:false},
   {name:'mobile-mode iPad Safari',ua:'Mozilla/5.0 (iPad; CPU OS 18_6 like Mac OS X) AppleWebKit/605.1.15 Version/18.6 Mobile/15E148 Safari/604.1',touchPoints:5,hidden:false},
   {name:'Mac Chrome',ua:'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/156.0.0.0 Safari/537.36',touchPoints:0,hidden:false}
  ];
  for(const test of cases) {
   await fresh(async page=>{
    assert.equal(await page.locator('#browser-guidance').evaluate(n=>n.hidden),test.hidden,test.name);
    await enter(page);assert.equal(await restoredMaster(page),'fictional violet river lantern',test.name);
   },({ua,touchPoints})=>{
    Object.defineProperty(navigator,'userAgent',{get:()=>ua});
    Object.defineProperty(navigator,'maxTouchPoints',{get:()=>touchPoints});
   },test);
  }
 });
}
module.exports={testSessions,settled,restoredMaster};
