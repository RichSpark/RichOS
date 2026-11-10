'use strict';
// Canonical RichPass 1.1.0 reference for unchanged account controls/Help/Card.
// The approved round-63.1 fixture covers the changed Generate heading/footer.
// Only fictional inputs are used. RichPass vectors define derivation.
const assert = require('node:assert/strict');
const {enrollFixture,ready,accepted}=require('./verification.cjs');
const {readFileSync} = require('node:fs');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const tool = path.resolve(__dirname, '..');
const reference = pathToFileURL(path.join(__dirname, 'RichPass.style-reference.html')).href;
const canonical = pathToFileURL(path.join(tool, 'RichPass.html')).href;
const properties = ['display','position','boxSizing','flexDirection','flexWrap','alignItems',
  'justifyContent','gap','columnCount','columnGap','textAlign','fontFamily','fontSize',
  'fontWeight','lineHeight','color','backgroundColor','boxShadow','outline','border',
  'borderRadius','padding','margin','animationName','transitionDuration','printColorAdjust'];
function viewportWidths() {
  const html = readFileSync(path.join(__dirname, 'RichPass.style-reference.html'), 'utf8');
  const css = html.match(/<style id="richpass-css">([\s\S]*?)<\/style>/)[1];
  const widths = new Set([320,390,1920]);
  for (const [,condition] of css.matchAll(/@media\s*([^{}]+)\{/g)) {
    for (const [,boundary] of condition.matchAll(/(?:min|max)-width\s*:\s*([\d.]+)px/g)) {
      const at = Math.round(Number(boundary));
      for (const width of [at-1,at,at+1]) widths.add(width);
    }
  }
  return [...widths].sort((a,b)=>a-b);
}
async function measure(page, selectors) {
  return page.evaluate(async ({selectors,properties})=>{
    await document.fonts.ready;
    // Finish finite view-entry/focus transitions, not the looping loading glow.
    await Promise.allSettled(document.getAnimations().filter(a=>
      Number.isFinite(a.effect.getComputedTiming().endTime)).map(a=>a.finished));
    return selectors.map(selector=>{
      const node=document.querySelector(selector);
      if(!node)throw new Error('Missing design element '+selector);
      const style=getComputedStyle(node),rect=node.getBoundingClientRect();
      return {style:Object.fromEntries(properties.map(p=>[p,style[p]])),x:rect.x,width:rect.width,height:rect.height};
    });
  },{selectors,properties});
}
async function compareLayout(browser) {
  const contexts=await Promise.all([browser.newContext(),browser.newContext()]);
  const failures=new Map(),errors=[],network=[];
  const widths=viewportWidths();
  try {
    for(const context of contexts)await context.route(/^https?:/,route=>{network.push(route.request().url());return route.abort();});
    const pages=await Promise.all(contexts.map(c=>c.newPage()));
    for(const page of pages)page.on('pageerror',e=>errors.push(e.message));
    async function compare(view,selectors) {
      for(const width of widths) {
        for(const page of pages) {
          await page.setViewportSize({width,height:844});
          // A resize moves the button away from a previously hovered coordinate.
          if(view.endsWith('hover'))await page.locator('#copy').hover();
          else await page.mouse.move(0,0);
        }
        const [expected,actual]=await Promise.all(pages.map(p=>measure(p,selectors)));
        for(let i=0;i<selectors.length;i++) {
          for(const property of properties)if(actual[i].style[property]!==expected[i].style[property]) {
            failures.set(view+' '+selectors[i]+' '+property,`${width}px: ${actual[i].style[property]} vs ${expected[i].style[property]}`);
          }
          // Help copy, the approved footer and the longer action label change view/form height.
          // Keep every unchanged control's geometry; states.cjs independently
          // checks the new complete Generate column against round-63.1.
          for(const dimension of (['#help','#generate-view .col','#account-form'].includes(selectors[i])?['x','width']:['x','width','height']))if(Math.abs(actual[i][dimension]-expected[i][dimension])>=0.25) {
            failures.set(view+' '+selectors[i]+' '+dimension,`${width}px: ${actual[i][dimension]} vs ${expected[i][dimension]}`);
          }
        }
        // The print sheet deliberately uses physical page widths. Screen overflow
        // is an independent usability check; print geometry is compared above.
        if(!view.endsWith(' print'))for(const page of pages)assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),view+' horizontal overflow at '+width);
      }
    }
    // Use the approved output-rule copy in the frozen design fixture so its
    // extra line is included in the comparison without changing the CSS baseline.
    const outputRules = '20 characters from <code>A–Z</code>, <code>a–z</code>, <code>0–9</code> and <code>@</code>. The final steps ensure a digit and uppercase letter and replace hyphens and underscores with lowercase letters. At most two consecutive digits.';
    const ruleParagraph = '#algorithm-view h3:text-is("Output rules (deterministic)") + p';
    async function loadDesignPair() {
      await Promise.all([pages[0].goto(reference),pages[1].goto(canonical)]);
      assert.equal(await pages[1].locator(ruleParagraph).innerHTML(), outputRules);
      await pages[0].locator(ruleParagraph).evaluate((node, copy) => { node.innerHTML = copy; }, outputRules);
    }
    await loadDesignPair();
    for(const page of pages)await enrollFixture(page);
    // Enrollment reloads both documents. Restore the fixture copy afterwards.
    await pages[0].locator(ruleParagraph).evaluate((node, copy) => { node.innerHTML = copy; }, outputRules);
    for(const scheme of ['dark','light']) {
      for(const page of pages) {
        await page.emulateMedia({colorScheme:scheme,reducedMotion:'reduce',media:'screen'});
        await page.waitForFunction(s=>document.documentElement.dataset.theme===s,scheme);
        await page.waitForFunction(()=>!document.getElementById('start-status').textContent.startsWith('Checking tab storage'));
        if(await page.locator('#forget').isVisible())await page.locator('#forget').click();
        await page.locator('#master').focus();
      }
      for(const page of pages) {
        await page.locator('#master').fill('fictional violet river lantern');
        await page.locator('#show-master').click();assert.equal(await page.locator('#master').getAttribute('type'),'text');
        await page.locator('#show-master').click();
        await page.locator(page===pages[0]?'#get-started':'#advance').click();
        await page.waitForFunction(()=>!document.getElementById('session-warning').textContent.includes('Saving'));
        await page.locator('#domain').fill('https://login.example.com/path');
        await page.locator('#label').fill(' Work ');await page.locator('#domain').focus();
      }
      const account=['#generate-view .col','#account-form','.fields','#domain','#label','#password-version','#output-group','#output-key','#output-label','#password','#copy','#domain-preview','#label-warning','#version-preview'];
      await compare(scheme+' Generate focus',account);
      for(const page of pages) {
        await page.getByRole('button',{name:'How is the domain used?'}).focus();
        await page.waitForFunction(()=>getComputedStyle(document.getElementById('tip-domain')).opacity==='1');
      }
      await compare(scheme+' tooltip',['#tip-domain']);
      for(const page of pages) {
        await page.evaluate(()=>{hashwasm.argon2id=()=>new Promise(resolve=>{window.finishLayoutKdf=resolve;});});
        await page.locator('#generate').click();
        await page.waitForFunction(()=>typeof window.finishLayoutKdf==='function');
      }
      await compare(scheme+' loading',account);
      for(const page of pages) {
        await page.evaluate(()=>window.finishLayoutKdf(new Uint8Array(32)));
        await page.waitForFunction(()=>!!document.getElementById('password').value);
        await page.locator('#copy').hover();
      }
      await compare(scheme+' output and hover',account);
      for(const [route,view,selectors] of [
        ['help','#help',['#help','.help-title','#help .panel','.help-actions']],
        ['card','#algorithm-view',['#algorithm-view','.col-card','.card-head','.print-area','.algo-card','.pb-cols']]
      ]) {
        for(const page of pages){await page.evaluate(route=>location.hash='/'+route,route);await page.locator(view).waitFor({state:'visible'});}
        await compare(scheme+' '+route,selectors);
      }
      for(const page of pages)await page.emulateMedia({media:'print'});
      await compare(scheme+' print',['html','.no-print','.print-area','.algo-card','.pb-cols']);
      // Use navigation to reset the test-only KDF override before changing theme.
      await loadDesignPair();
    }
    // Check the approved entrance animation with normal motion too.
    for(const page of pages){await ready(page);await page.emulateMedia({media:'screen',reducedMotion:'no-preference'});if(await page.locator('#forget').isVisible())await page.locator('#forget').click();}
    assert.deepEqual(network,[],'Styling must stay self-contained');
    assert.deepEqual(errors,[],'Styling must not break the page scripts');
    assert.equal(failures.size,0,'Approved RichPass design differences:\n'+[...failures].map(([k,v])=>k+' '+v).join('\n'));
    return widths;
  } finally {await Promise.all(contexts.map(c=>c.close()));}
}
module.exports={compareLayout,viewportWidths,measure,properties};
