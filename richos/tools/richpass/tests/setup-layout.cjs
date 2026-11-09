'use strict';
// Compare the wired setup against the exact CEO-approved round-62.1 mockup.
const assert=require('node:assert/strict'),path=require('node:path');
const {pathToFileURL}=require('node:url');
const {measure}=require('./layout.cjs');
const labels=['Master passphrase','Confirm master passphrase','Confirm master passphrase again','Master passphrase once more','Confirm master passphrase one last time!'];
const percentages=[20,40,60,80,95];
// Inspect startup with ordinary motion: settled reduced-motion snapshots cannot
// detect a backwards transition that has already finished. Controlled JS time
// holds the page's setup state while the browser reports its real CSS keyframes.
async function testInitialProgress(browser) {
 const context=await browser.newContext({reducedMotion:'no-preference'});
 try {
  await context.route(/^https?:/,r=>r.abort());
  const page=await context.newPage();
  await page.clock.install({time:new Date(100000)});await page.clock.pauseAt(new Date(101000));
  await page.goto(pathToFileURL(path.join(__dirname,'../RichPass.html')).href);
  await page.waitForFunction(()=>!document.getElementById('start-status').textContent.startsWith('Checking'));
  for(const delta of [0,16,300,1000]) {
   if(delta)await page.clock.runFor(delta);
   const state=await page.evaluate(()=>{
    const bar=document.getElementById('setup-progress'),fill=bar.querySelector('.progress-fill');
    return {width:fill.style.width,value:bar.getAttribute('aria-valuenow'),text:bar.getAttribute('aria-valuetext'),
     fraction:fill.getBoundingClientRect().width/bar.getBoundingClientRect().width,
     transitions:fill.getAnimations().filter(a=>a.effect.target===fill&&a.transitionProperty==='width').map(a=>a.effect.getKeyframes())};
   });
   assert.equal(state.width,'20%');assert.equal(state.value,'20');assert.equal(state.text,'20 percent, step 1 of 5');
   assert.ok(Math.abs(state.fraction-0.2)<0.002,'first-load fill starts and stays at20%');
   assert.deepEqual(state.transitions,[],'no startup width transition');
  }
  await page.clock.resume();await page.locator('#master').fill('fictional violet river lantern');await page.locator('#advance').click();
  assert.equal(await page.locator('.progress-fill').evaluate(fill=>fill.style.width),'40%');
  const transitions=await page.locator('.progress-fill').evaluate(fill=>fill.getAnimations()
   .filter(a=>a.effect.target===fill&&a.transitionProperty==='width').map(a=>{a.pause();const frames=a.effect.getKeyframes();a.finish();return frames;}));
  assert.equal(transitions.length,1,'normal forward step animation is retained');
  assert.equal(transitions[0][0].width,'20%');assert.equal(transitions[0].at(-1).width,'40%');
 }finally{await context.close();}
}
async function compareSetupLayout(browser) {
 const contexts=await Promise.all([browser.newContext(),browser.newContext()]);
 const errors=[],network=[];
 try {
  for(const context of contexts)await context.route(/^https?:/,r=>{network.push(r.request().url());return r.abort();});
  const pages=await Promise.all(contexts.map(c=>c.newPage()));
  for(const page of pages)page.on('pageerror',error=>errors.push(error.message));
  const urls=[pathToFileURL(path.join(__dirname,'RichPass.setup-reference.html')).href,pathToFileURL(path.join(__dirname,'../RichPass.html')).href];
  const selectors=['.home-card','.brand','.brand img','.lead','#setup-title','#setup-progress','.progress-fill','#master-label','#master-group','#master','#show-master','#verdict','#master-note','#advance','#setup-explainer'];
  for(const scheme of ['dark','light']) {
   for(let i=0;i<2;i++){await pages[i].emulateMedia({colorScheme:scheme,reducedMotion:'reduce'});await pages[i].goto(urls[i]);}
   await pages[1].waitForFunction(()=>!document.getElementById('start-status').textContent.startsWith('Checking'));
   for(let step=0;step<5;step++) {
    for(const page of pages) {
     await page.waitForFunction(label=>document.getElementById('master-label-text').textContent===label,labels[step]);
     assert.equal(await page.locator('#setup-progress').getAttribute('aria-valuenow'),String(percentages[step]));
     assert.equal(await page.locator('#setup-title-text').textContent(),'Quick One-Time Setup');
     assert.equal(await page.locator('#advance-text').textContent(),step===0?'Get started':step===4?'Finish setup':'Next');
    }
    for(const width of [320,375,768,1200]) {
     for(const page of pages){
      await page.setViewportSize({width,height:900});
      // The mock observes layout; production sizes on viewport/font/view changes.
      // Wait for its actual geometry, not an old animation snapshot or a sleep.
      await page.waitForFunction(()=>{
       const title=document.getElementById('setup-title-text'),form=document.getElementById('setup-form'),bar=document.getElementById('setup-progress');
       const rem=parseFloat(getComputedStyle(document.documentElement).fontSize)||16;
       const target=Math.round(Math.min(form.clientWidth,title.getBoundingClientRect().width+2.5*rem));
       return parseFloat(bar.style.width)===target&&Math.abs(bar.getBoundingClientRect().width-target)<0.25;
      });
     }
     const [expected,actual]=await Promise.all(pages.map(page=>measure(page,selectors)));
     for(let i=0;i<selectors.length;i++) {
      assert.deepEqual(actual[i].style,expected[i].style,`${scheme} step ${step+1} ${width}px ${selectors[i]} styles`);
      for(const dim of ['x','width','height'])assert.ok(Math.abs(actual[i][dim]-expected[i][dim])<0.25,
       `${scheme} step ${step+1} ${width}px ${selectors[i]} ${dim}: ${actual[i][dim]} vs ${expected[i][dim]}`);
     }
    }
    if(step>0) {
     for(const page of pages)await page.locator('#master').fill('wrong');
     const [expected,actual]=await Promise.all(pages.map(page=>measure(page,['#master-group','#master','#verdict','#verdict-body','#start-over'])));
     assert.deepEqual(actual.map(x=>x.style),expected.map(x=>x.style),scheme+' mismatch styling step '+(step+1));
     for(const page of pages) {
      assert.equal(await page.locator('#advance').isDisabled(),true);
      assert.match(await page.locator('#verdict-body').textContent(),/This doesn't match your first entry/);
     }
    }
    for(const page of pages) {
     await page.locator('#master').fill('fictional violet river lantern');
     assert.equal(await page.locator('#advance').isEnabled(),true);
     assert.equal(await page.locator('#verdict').evaluate(n=>n.classList.contains('open')),false);
     if(step<4)await page.locator('#advance').click();
    }
    assert.equal(await pages[1].evaluate(()=>localStorage.getItem('richpass:verifier')),null,'first four confirmations cannot persist a verifier');
   }
  }
  assert.deepEqual(errors,[]);assert.deepEqual(network,[]);
 }finally{await Promise.all(contexts.map(c=>c.close()));}
}
module.exports={compareSetupLayout,testInitialProgress};
