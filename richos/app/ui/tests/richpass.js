"use strict";
// Real Settings controller and RichPass module. Only the native picker/owner boundary is mocked.
const path=require("path");
const {loadPlaywright,createRun,assert,assertEqual,UI_DIR}=require("./lib/harness");
async function main(){
 const run=createRun("RichPass file protection"),browser=await loadPlaywright().webkit.launch({headless:true});
 async function open(preset={}){
  const page=await browser.newPage({viewport:{width:1100,height:850}});
  await page.goto("file://"+path.join(UI_DIR,"cleaner.html"));
  await page.evaluate(preset=>{
   window.__calls=[];window.__listeners={};window.__view={phase:"idle",message:"",path:null,release:null,lastChecked:null,notice:null,newerAvailable:false,downloadUrl:null,supported:true,...preset};
   window.RichBridge={invoke:async(name,args)=>{window.__calls.push([name,args]);if(name==="richpass_choose")throw Error("Not recognized as a RichPass release. Save a verified copy instead.");if(name==="richpass_stop")window.__view={...window.__view,phase:"idle",path:null,notice:null};if(name==="richpass_check")window.__view={...window.__view,phase:"verified",message:""};if(name==="richpass_acknowledge")window.__view.notice=null;return window.__view;},listen:async(name,handler)=>{window.__listeners[name]=handler;return ()=>{};}};
  },preset);
  await page.addScriptTag({path:path.join(UI_DIR,"richpass.js")});
  await page.click("#set-btn");await page.click("#set-richpass-open");await page.getByRole("dialog",{name:"RichPass",exact:true}).waitFor();
  return page;
 }
 await run.check("Settings saves the bundled copy without enrolling protection", async () => {
   const p = await open();
   await p.click('[data-richpass="save"]');
   const calls = await p.evaluate(() => window.__calls.filter(([name]) => name === "richpass_save" || name === "open_external"));
   assertEqual(calls, [["richpass_save", { protect: false }]], "one native save with no browser or enrollment");
   await p.close();
 });
 await run.check("primary Save-and-protect invokes native consent without a page-supplied path",async()=>{const p=await open();await p.click('[data-richpass="save-protect"]');const calls=await p.evaluate(()=>window.__calls);assert(calls.some(([name,args])=>name==="richpass_save" && args.protect===true && !args.path),"native picker owns the path");assertEqual(await p.locator('#richpass-settings input').count(),0,"no master or password input in RichOS");await p.close();});
 await run.check("unknown selected file gives an honest error and retains the prior state",async()=>{const p=await open();await p.click('[data-richpass="choose"]');await p.locator('#richpass-error:not([hidden])').waitFor();assert((await p.locator('#richpass-error').innerText()).includes("Not recognized"),"recognition failure shown");assert((await p.locator('#richpass-settings').innerText()).includes("No protected file"),"no false protected status");await p.close();});
 await run.check("paused protection offers explicit Check now and Stop without deleting a file",async()=>{const p=await open({phase:"paused",path:"/fictional/RichPass.html",release:"richpass-1",message:"Automatic repairs paused"});await p.click('[data-richpass="check"]');await p.getByRole('heading',{name:'Verified',exact:true}).waitFor();await p.click('[data-richpass="stop"]');await p.getByRole('heading',{name:'No protected file',exact:true}).waitFor();const calls=await p.evaluate(()=>window.__calls);assert(calls.some(([n])=>n==="richpass_stop"),"owner stops protection");assert(!calls.some(([n])=>n.includes("delete")),"no file deletion action");await p.close();});
 await run.check("restoration notice persists until acknowledgement and explains open-page limit",async()=>{const p=await open({phase:"verified",path:"/fictional/RichPass.html",notice:{kind:"contents",at:100}});assert((await p.locator('#richpass-settings').innerText()).includes('Close any open RichPass tab'),"loaded-page limitation");await p.click('[data-richpass="acknowledge"]');assertEqual(await p.locator('[data-richpass="acknowledge"]').count(),0,"acknowledged explicitly");await p.close();});
 await run.check("Settings reports a current permission failure alongside an earlier repair", async () => {
   const p = await open({
     phase: "verified",
     path: "/fictional/RichPass.html",
     notice: { kind: "contents", at: 100 },
   });
   try {
     await p.keyboard.press("Escape");
     await p.click("#set-btn");
     const row = p.locator("#set-richpass-state");
     assertEqual(await row.innerText(), "File restored; review notice", "verified repair summary");
     await p.evaluate(() => {
       window.__view = { ...window.__view, phase: "blocked", message: "Allow folder access, then Check now." };
       window.__listeners["rich://richpass"]({ payload: window.__view });
     });
     assertEqual(await row.innerText(), "Protection needs attention", "the current permission failure controls the row");
   } finally {
     await p.close();
   }
 });
 await run.check("unsupported platform exposes standalone Save without a dead Protect action",async()=>{const p=await open({supported:false});assertEqual(await p.locator('[data-richpass="save-protect"]').count(),0,"no dead protection");await p.click('[data-richpass="save"]');assert((await p.evaluate(()=>window.__calls)).some(([n,a])=>n==="richpass_save" && a.protect===false),"standalone save");assertEqual(await p.locator('[data-richpass="download"]').count(),0,"no separate network action");await p.close();});
 await run.check("newer approved release uses Save-and-protect and Escape restores settings focus",async()=>{const p=await open({phase:"verified",path:"/fictional/RichPass.html",newerAvailable:true});assert((await p.locator('[data-richpass="save-protect"]').innerText()).includes('newer'),"deliberate upgrade action");await p.keyboard.press("Escape");assertEqual(await p.locator('#richpass-settings').isVisible(),false,"closed");assertEqual(await p.evaluate(()=>document.activeElement.id),'set-btn',"focus restored");await p.close();});
 await run.check("folder permission recovery opens the native pane without claiming a grant",async()=>{const p=await open({folderPermissions:true,phase:"blocked",path:"/fictional/RichPass.html",message:"Allow folder access, then Check now."});await p.click('[data-richpass="permissions"]');assert((await p.evaluate(()=>window.__calls)).some(([n])=>n==="richpass_permissions"),"native settings route");assert((await p.locator('#richpass-settings').innerText()).includes("Protection needs attention"),"opening settings does not grant access or mark protection verified");await p.close();});
 await browser.close();process.exitCode=run.report()?1:0;
}
main().catch(e=>{console.error(e);process.exitCode=1;});
