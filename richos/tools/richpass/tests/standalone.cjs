'use strict';
// Exercise the actual canonical file with public fictional inputs, never a user's profile.
const fs = require('fs'), path = require('path'), assert = require('node:assert/strict');
const {pathToFileURL} = require('url');
const {compareLayout} = require('./layout.cjs');
const {compareSetupLayout,testInitialProgress}=require('./setup-layout.cjs');
const {testStates}=require('./states.cjs');
const {testMaster}=require('./master.cjs');
const {enrollFixture,ready,accepted,testVerification,defaultMaster}=require('./verification.cjs');
const {testSessions, settled, restoredMaster} = require('./sessions.cjs');
const tool = path.resolve(__dirname, '..'), repo = path.resolve(__dirname, '../../../..');
const harness = path.join(repo, 'richos/app/ui/tests/lib/harness.js');
const pw = fs.existsSync(harness) ? require(harness).loadPlaywright() : require(process.env.RICHOS_PLAYWRIGHT || 'playwright');
const engine = process.env.RICHPASS_BROWSER || 'chromium';
const sessionMasterKey = 'richpass:master';
const fixtures = JSON.parse(fs.readFileSync(path.join(__dirname, engine === 'webkit' ? 'vectors-webkit.json' : 'vectors.json'), 'utf8'));
const reportPath = process.env.RICHPASS_REPORT;
const report = {artifactSha256:require("crypto").createHash("sha256").update(fs.readFileSync(path.join(tool,"RichPass.html"))).digest("hex"),engine, profile:fixtures.profile, checks: [], network: [], errors: [], limitations: engine === 'webkit' ? ['WebKit offline emulation rejects file URLs even for a trivial page; HTTP(S) requests are blocked instead. This is not a native Safari/OS-offline test.'] : []};
async function check(name, fn) {const start = Date.now();try {await fn();report.checks.push({name, passed: true, ms: Date.now()-start});console.log('PASS '+name);} catch(e) {report.checks.push({name, passed:false, error:e.message});throw e;}}
(async () => {
 const browser = await pw[engine].launch({headless:true});report.browser = browser.version();
 try {
  await check('initial setup starts at20% without a backwards transition and later steps animate',async()=>{await testInitialProgress(browser);});
  await testMaster({browser,check,fileURL:pathToFileURL(path.join(tool,'RichPass.html')).href});
  await testStates({browser,check,fileURL:pathToFileURL(path.join(tool,'RichPass.html')).href});
  await testVerification({browser,check,fileURL:pathToFileURL(path.join(tool,'RichPass.html')).href});
  await check('round-62.1 setup wording and visual behavior match the approved mockup',async()=>{await compareSetupLayout(browser);});
  await check('approved RichPass styling in both themes at every viewport boundary', async () => {
   report.layoutWidths = await compareLayout(browser);
   console.log('  Compared widths: ' + report.layoutWidths.join(', ') + ' px');
  });
  const context = await browser.newContext({viewport:{width:1100,height:850}});
  if(engine !== 'webkit') await context.setOffline(true);
  await context.route(/^https?:/, route => {report.network.push(route.request().url());return route.abort();});
  await context.addInitScript(() => {
   window.storageWrites = [];
   const setItem = Storage.prototype.setItem;
   Storage.prototype.setItem = function(key, value) {
    window.storageWrites.push({scope:this === window.sessionStorage ? 'session' : 'local',key,value});
    return setItem.call(this,key,value);
   };
  });
  const page = await context.newPage();page.on('pageerror', e => report.errors.push(e.message));
  let enrolledMaster=null;
  async function load(master='fictional violet river lantern') {
   await page.goto('about:blank');
   await page.goto(pathToFileURL(path.join(tool,'RichPass.html')).href);
   // Await actual recovery before deciding whether this retained tab needs Forget.
   await page.waitForFunction(()=>!document.getElementById('start-status').textContent.startsWith('Checking tab storage'));
   if(enrolledMaster!==master.trim()||await page.locator('#setup-progress').isVisible()) {await enrollFixture(page,master);enrolledMaster=master.trim();}
   if(await page.locator('#forget').isVisible())await page.locator('#forget').click();
   await page.locator('#master').fill(master);
   await page.locator('#advance').click();
   await accepted(page);
  }
  await check('fixed-profile RichPass vectors', async () => {
   for (const v of fixtures.vectors) {
    await load(v.master);await page.locator('#domain').fill(v.domain);await page.locator('#label').fill(v.label);await page.locator('#password-version').fill(v.version);
    const current = await page.evaluate(({domain,label,version}) => ({domain:RichPass.normalizeDomain(domain),label:RichPass.normalizeLabel(label),version:RichPass.normalizeVersion(version)}),v);
    assert.deepEqual(current,{domain:v.effective.domain,label:v.effective.label,version:v.effective.version},v.name+' normalization');
    if(v.password===null) {if(v.domain)await page.locator('#generate').click();assert.equal(await page.locator('#password').inputValue(),'');assert.equal(await page.locator('#copy').isDisabled(),true);}
    else {await page.locator('#generate').click();await page.waitForFunction(expected=>document.getElementById('password').value===expected,v.password);}
   }
  });
  await check('every input edit clears an actual generated password and disables Copy',async()=>{
   for(const [id,value] of [['domain','example.net'],['label','changed'],['password-version','v2']]) {
    await load();await page.locator('#domain').fill('example.com');await page.locator('#generate').click();await page.waitForFunction(()=>!!document.getElementById('password').value);
    await page.locator('#'+id).fill(value);assert.equal(await page.locator('#password').inputValue(),'');assert.equal(await page.locator('#copy').isDisabled(),true);
   }
  });
  await check('stale KDF result cannot publish after input edit or Forget', async()=>{
   for(const action of ['domain','label','password-version','forget']) {
    await load();await page.locator('#domain').fill('example.com');
    await page.evaluate(()=>{window.originalKdf=hashwasm.argon2id;hashwasm.argon2id=()=>new Promise(resolve=>{window.finishKdf=resolve;});});
    await page.locator('#generate').click();await page.waitForFunction(()=>typeof window.finishKdf==='function');
    if(action!=='forget')await page.locator('#'+action).fill('changed');else await page.locator('#forget').click();
    await page.evaluate(()=>window.finishKdf(new Uint8Array(32)));
    await page.waitForFunction(()=>!document.getElementById('generation-status').textContent.includes('Generating'));
    assert.equal(await page.locator('#password').inputValue(),'');assert.equal(await page.locator('#copy').isDisabled(),true);
    if(action==='forget') {
     assert.equal(await page.locator('#master').inputValue(),'');
     assert.equal(await page.evaluate(key=>sessionStorage.getItem(key),sessionMasterKey),null);
    }
   }
  });
  await check('WASM failure leaves no output and allows explicit retry',async()=>{
   await load();await page.locator('#domain').fill('example.com');await page.evaluate(()=>{window.originalKdf=hashwasm.argon2id;hashwasm.argon2id=async()=>{throw new Error('synthetic unavailable memory');};});
   await page.locator('#generate').click();await page.waitForFunction(()=>document.getElementById('generation-status').textContent.includes('could not generate'));
   assert.equal(await page.locator('#password').inputValue(),'');assert.equal(await page.locator('#copy').isDisabled(),true);assert.equal(await page.locator('#generate').isEnabled(),true);
   await page.evaluate(()=>hashwasm.argon2id=window.originalKdf);await page.locator('#generate').click();await page.waitForFunction(()=>!!document.getElementById('password').value);
  });
  await check('clipboard denial retains manual selection without reading clipboard',async()=>{
   await load();await page.locator('#domain').fill('example.com');await page.locator('#generate').click();await page.waitForFunction(()=>!!document.getElementById('password').value);
   await page.evaluate(()=>Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{throw new Error('synthetic denied');}}}));
   await page.locator('#copy').click();await page.waitForFunction(()=>document.getElementById('copy-status').textContent.includes('Select the password'));
   assert.equal(await page.locator('#password').evaluate(n=>n.selectionEnd-n.selectionStart),20);
  });
  await check('20-second reminder is honest and does not overwrite clipboard',async()=>{
   await page.evaluate(()=>{window.copyWrites=0;Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{window.copyWrites++;}}});});
   const started=Date.now();await page.locator('#copy').click();await page.waitForFunction(()=>document.getElementById('copy-status').textContent.includes('Copy something else'),{},{timeout:24000});
   assert.ok(Date.now()-started>=20000);assert.equal(await page.evaluate(()=>window.copyWrites),1);
  });
  await check('only ciphertext is stored for the tab; no automatic network or secret URL',async()=>{
   const stored = await page.evaluate(()=>({local:Object.keys(localStorage),session:{...sessionStorage},writes:window.storageWrites}));
   assert.deepEqual(stored.local,['richpass:verifier']);
   assert.ok(!await page.evaluate(()=>localStorage.getItem('richpass:verifier')).then(value=>value.includes('fictional')));
   assert.deepEqual(Object.keys(stored.session),[sessionMasterKey]);
   assert.equal(await restoredMaster(page),'fictional violet river lantern');
   assert.equal(stored.writes.length,1);
   assert.equal(stored.writes[0].scope,'session');
   assert.equal(stored.writes[0].key,sessionMasterKey);
   assert.equal(stored.writes[0].value,stored.session[sessionMasterKey]);
   assert.ok(!stored.writes[0].value.includes('fictional'));
   assert.deepEqual(Object.keys(JSON.parse(stored.writes[0].value)).sort(),['ciphertext','iv','version']);
   assert.deepEqual(report.network,[]);assert.deepEqual(report.errors,[]);assert.equal(new URL(page.url()).search,'');
  });
  await check('reload and history return retain the tab master without saving account details or output',async()=>{
   const vector=fixtures.vectors.find(v=>v.name==='default');
   await load('  '+vector.master+'  ');
   await page.locator('#domain').fill(vector.domain);await page.locator('#generate').click();
   await page.waitForFunction(expected=>document.getElementById('password').value===expected,vector.password);
   await page.reload();
   await page.locator('#generate-view').waitFor({state:'visible'});
   assert.equal(await page.locator('#master').inputValue(),'');
   assert.equal(await page.locator('#password').inputValue(),'');
   assert.equal(await page.locator('#copy').isDisabled(),true);
   assert.equal(await page.locator('#domain').inputValue(),'');
   assert.equal(await page.locator('#label').inputValue(),'');
   assert.equal(await restoredMaster(page),vector.master.trim());
   await page.goto('about:blank');await page.goBack();
   await page.locator('#generate-view').waitFor({state:'visible'});
   await page.locator('#domain').fill(vector.domain);await page.locator('#generate').click();
   await page.waitForFunction(expected=>document.getElementById('password').value===expected,vector.password);
   await page.goto(pathToFileURL(path.join(tool,'RichPass.html')).href+'#/');
   await page.locator('#generate-view').waitFor({state:'visible'});
  });
  await check('a separately opened tab starts without the master',async()=>{
   const other=await context.newPage();
   try {
    await other.goto(pathToFileURL(path.join(tool,'RichPass.html')).href);
    await other.locator('#master').waitFor({state:'visible'});
    assert.equal(await other.evaluate(key=>sessionStorage.getItem(key),sessionMasterKey),null);
    assert.equal(await other.locator('#advance').isDisabled(),true);
   } finally {await other.close();}
   assert.equal(await page.locator('#generate-view').isVisible(),true);
  });
  for(const flow of ['entry','forget','reenter']) {
   await check(`one Back leaves after ${flow}; Forward restores the correct tab state`,async()=>{
    const navigation=await context.newPage();
    const initialMaster='fictional violet river lantern';
    const replacementMaster=initialMaster; // Every re-entry must use the enrolled master.
    try {
     await navigation.goto('data:text/html,<title>Earlier page</title><p>Before RichPass</p>');
     await navigation.goto(pathToFileURL(path.join(tool,'RichPass.html')).href);
     await ready(navigation);
     await navigation.locator('#master').fill(initialMaster);
     await navigation.locator('#advance').click();
     await navigation.locator('#generate-view').waitFor({state:'visible'});
     await settled(navigation);
     if(flow!=='entry') {
      await navigation.locator('#forget').click();
      await navigation.locator('#master').waitFor({state:'visible'});
      assert.equal(await navigation.evaluate(key=>sessionStorage.getItem(key),sessionMasterKey),null);
     }
     if(flow==='reenter') {
      await navigation.locator('#master').fill(replacementMaster);
      await navigation.locator('#advance').click();
      await navigation.locator('#generate-view').waitFor({state:'visible'});
     await settled(navigation);
     }
     await navigation.goBack();
     await navigation.getByText('Before RichPass',{exact:true}).waitFor();
     await navigation.goForward();
     const expectedMaster=flow==='forget'?null:flow==='reenter'?replacementMaster:initialMaster;
     await navigation.locator(flow==='forget'?'#master':'#generate-view').waitFor({state:'visible'});
     assert.equal(await restoredMaster(navigation),expectedMaster);
     if(flow==='forget')assert.equal(await navigation.locator('#advance').isDisabled(),true);
    } finally {await navigation.close();}
   });
  }
  await check('Forget clears session storage and reload requires the master again',async()=>{
   await page.locator('#forget').click();
   assert.equal(await page.evaluate(key=>sessionStorage.getItem(key),sessionMasterKey),null);
   await page.reload();
   await page.locator('#master').waitFor({state:'visible'});
   assert.equal(await page.locator('#advance').isDisabled(),true);
   assert.equal(await page.locator('#password').inputValue(),'');
  });
  await check('unavailable session storage keeps generation usable and reports the reload limitation',async()=>{
   for(const failure of ['unavailable','read','write','remove']) {
    const limited=await browser.newContext();
    try {
     await limited.route(/^https?:/,route=>route.abort());
     await limited.addInitScript(failure=>{
      if(failure==='unavailable') {
       Object.defineProperty(window,'sessionStorage',{get(){throw new Error('synthetic storage denial');}});
      } else if(failure==='read') {
       const read=Storage.prototype.getItem;
       Storage.prototype.getItem=function(...args){if(this===window.sessionStorage)throw new Error('synthetic storage read denial');return read.apply(this,args);};
      } else if(failure==='remove') {
       const remove=Storage.prototype.removeItem;
       Storage.prototype.removeItem=function(...args){if(this===window.sessionStorage)throw new Error('synthetic storage removal denial');return remove.apply(this,args);};
      } else {
       const setItem=Storage.prototype.setItem;
       Storage.prototype.setItem=function(key,value) {
        if(this===window.sessionStorage)throw new Error('synthetic storage write denial');
        return setItem.call(this,key,value);
       };
      }
     },failure);
     const tab=await limited.newPage();
     await tab.goto(pathToFileURL(path.join(tool,'RichPass.html')).href);
     await enrollFixture(tab);
     if(failure==='unavailable'||failure==='read') {
      assert.match(await tab.locator('#start-status').textContent(),/could not access tab storage/);
      assert.doesNotMatch(await tab.locator('#start-status').textContent(),/saved tab session.*restored/);
     }
     await tab.locator('#master').fill('fictional violet river lantern');await tab.locator('#advance').click();
     await accepted(tab);
     assert.match(await tab.locator('#session-warning').textContent(),/on this page/);
     if(failure!=='write')assert.equal(await tab.locator('#session-warning').textContent(),'Your master remains usable on this page. RichPass could not access tab storage or confirm that it was cleared.');
     assert.doesNotMatch(await tab.locator('#session-warning').textContent(),/earlier session|clear or replace its saved/);
     await tab.locator('#domain').fill('example.com');await tab.locator('#generate').click();
     await tab.waitForFunction(()=>!!document.getElementById('password').value);
     if(failure==='unavailable'||failure==='remove') {
      await tab.locator('#forget').click();
      assert.match(await tab.locator('#start-status').textContent(),/could not confirm clearing tab storage/);
      assert.doesNotMatch(await tab.locator('#start-status').textContent(),/saved tab session/);
     }
     await tab.reload();
     await tab.locator('#master').waitFor({state:'visible'});
     assert.equal(await tab.locator('#advance').isDisabled(),true);
    } finally {await limited.close();}
   }
  });
  await check('storage access lost after restore reports uncertainty without losing live generation',async()=>{
   await load();await page.reload();await page.locator('#generate-view').waitFor({state:'visible'});
   const record=await page.evaluate(key=>sessionStorage.getItem(key),sessionMasterKey);
   await page.evaluate(()=>{
    window.readBeforeDenial=Storage.prototype.getItem;
    Storage.prototype.getItem=function(...args){if(this===sessionStorage)throw new Error('synthetic mid-session access denial');return window.readBeforeDenial.apply(this,args);};
    const remove=Storage.prototype.removeItem;Storage.prototype.removeItem=function(...args){if(this===sessionStorage)throw new Error('synthetic mid-session access denial');return remove.apply(this,args);};
   });
   await page.locator('#forget').click();
   assert.match(await page.locator('#start-status').textContent(),/could not confirm clearing tab storage/);
   await page.locator('#master').fill(defaultMaster);await page.locator('#advance').click();
   await accepted(page);
   assert.equal(await page.locator('#session-warning').textContent(),'Your master remains usable on this page. RichPass could not access tab storage or confirm that it was cleared.');
   assert.equal(await page.evaluate(key=>window.readBeforeDenial.call(sessionStorage,key),sessionMasterKey),record);
   await page.locator('#domain').fill('example.com');await page.locator('#generate').click();
   await page.waitForFunction(()=>!!document.getElementById('password').value);
   // Reload clears these test-only overrides; the next check starts afresh.
   await page.reload();await page.locator('#generate-view').waitFor({state:'visible'});
  });
  await check('Forget reports a failed session removal without claiming it cleared storage',async()=>{
   await load();
   await page.evaluate(()=>{
    Storage.prototype.removeItem=function(){throw new Error('synthetic removal denial');};
   });
   await page.locator('#forget').click();
   await page.locator('#master').waitFor({state:'visible'});
   assert.match(await page.locator('#start-status').textContent(),/could not confirm clearing/);
   assert.equal(await restoredMaster(page),'fictional violet river lantern');
   await page.reload();await page.locator('#generate-view').waitFor({state:'visible'});
   await page.locator('#forget').click();
   assert.equal(await page.evaluate(key=>sessionStorage.getItem(key),sessionMasterKey),null);
  });
  await testSessions({browser, check, fileURL:pathToFileURL(path.join(tool,'RichPass.html')).href});
  await check('responsive layout and keyboard labels',async()=>{
   await page.setViewportSize({width:390,height:844});assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
   await load();await page.locator('#forget').click();
   assert.equal(await page.locator('input[name]').count(),0);assert.equal(await page.locator('#show-master').getAttribute('aria-pressed'),'false');
   await page.getByRole('link',{name:'Why & How'}).click();
   await page.locator('#help').waitFor({state:'visible'});
   await page.getByRole('link',{name:'Go to master entry'}).click();
   await page.locator('#master').waitFor({state:'visible'});
   if(process.env.RICHPASS_SHOT)await page.screenshot({path:process.env.RICHPASS_SHOT,fullPage:true});
  });
  await context.close();
 } finally {await browser.close();if(reportPath)fs.writeFileSync(reportPath,JSON.stringify(report,null,2)+'\n');}
})().catch(e=>{console.error(e);process.exitCode=1;});
