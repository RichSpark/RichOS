"use strict";
const path=require("path");
const {loadPlaywright,createRun,assert,assertEqual,UI_DIR,leaveHome}=require("./lib/harness");
// Drive the real main.js event handlers, including concurrent prose and activity.
// The bridge only supplies fixture replies; it does not implement layout behavior.
async function stableQuestionDuringArrivals(browser, mode) {
  const page = await browser.newPage({viewport:{width:1280,height:900}});
  const errors=[];page.on('pageerror', error=>errors.push(String(error)));
  try {
  await page.addInitScript(()=>{
   Object.defineProperty(window,'RichBridge',{configurable:true,set(bridge){
    window.fixtureHandlers={};const listen=bridge.listen.bind(bridge);
    bridge.listen=(name,fn)=>{window.fixtureHandlers[name]=fn;return listen(name,fn)};
    Object.defineProperty(window,'RichBridge',{value:bridge,writable:true,configurable:true});
   }});
  });
  await page.goto('file://'+path.join(UI_DIR,'index.html'));await leaveHome(page);
  await page.evaluate(()=>openThread('general'));
  await page.evaluate(()=>{
   window.fixtureAnswers=[];const invoke=RichBridge.invoke.bind(RichBridge);
   RichBridge.invoke=async(c,a)=>{if(c==='question_shown')return;if(c==='answer_question'){fixtureAnswers.push(a);return {outcome:'accepted',question:{...fixtureQuestion,state:'answered',revision:1,answer:{...a.answer,method:a.method,surface:'mac'}}}};return invoke(c,a)};
   const at=Date.now();window.fixtureFence={threadId:activeThreadId,entityId:timelineModel.entityId,bindingRevision:timelineModel.bindingRevision,turnId:'fixture-stream',visibility:'ceo'};
   fixtureHandlers['rich://turn-status']({payload:{...fixtureFence,status:'working',at,startedAt:at}});
   fixtureHandlers['rich://message-started']({payload:{...fixtureFence,messageId:'fixture-prose',seq:0,slot:'stream',createdAt:at,at}});
   window.fixtureQuestion={id:'fixture-question',text:'When should the release ship?',options:[{id:'today',label:'Ship today',description:'Earlier fixes'},{id:'tomorrow',label:'Ship tomorrow',description:'More testing'}],multiple:false,free_answer:true,state:'open',revision:0,delivered:false};
   fixtureHandlers['rich://question-upserted']({payload:{...fixtureFence,turnId:'fixture-obligation',id:'fixture-question',kind:'question',question:fixtureQuestion,sequence:0,slot:'stream',createdAt:at+1,at:at+1}});
  });
  await page.locator('[data-question-id="fixture-question"] .question-option').first().waitFor();
  await page.locator('[data-question-id="fixture-question"] .question-option').first().scrollIntoViewIfNeeded();
  const before=await page.locator('[data-question-id="fixture-question"] .question-option').first().boundingBox();
  if(mode==='keyboard'){await page.locator('[data-question-id="fixture-question"] .question-option').first().focus();await page.mouse.move(50,50)}else{await page.mouse.move(before.x+before.width/2,before.y+before.height/2)};
  await page.evaluate(()=>{
   fixtureHandlers['rich://question-upserted']({payload:{...fixtureFence,threadId:'foreign-thread',id:'foreign-question',turnId:'foreign-obligation',kind:'question',question:{...fixtureQuestion,id:'foreign-question'},sequence:0,slot:'stream',createdAt:Date.now(),at:Date.now()}});
   fixtureHandlers['rich://message-delta']({payload:{...fixtureFence,messageId:'fixture-prose',seq:0,textDelta:'A growing response while the question is waiting. '.repeat(150),at:Date.now()}});
   fixtureHandlers['rich://activity-upserted']({payload:{...fixtureFence,id:'fixture-activity',kind:'activity',title:'Inspecting progress',status:'running',sequence:1,createdAt:Date.now(),at:Date.now()}});
   if(!timelineModel.items.has('fixture-activity'))throw new Error('Fixture activity was not accepted');
  });
  await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
  const prose=await page.evaluate(()=>timelineModel.items.get('fixture-prose').text);if(prose.length<1000)throw new Error('Fixture did not actually receive the response');
  const after=await page.locator('[data-question-id="fixture-question"] .question-option').first().boundingBox();
  if(mode==='composer') {
   await page.locator('#input').fill('A quick new request');
  } else if(mode==='withdrawn') {
   await page.evaluate(()=>fixtureHandlers['rich://question-upserted']({payload:{...fixtureFence,turnId:'fixture-obligation',id:'fixture-question',kind:'question',question:{...fixtureQuestion,state:'withdrawn',revision:1,withdrawal_reason:'The decision is already settled'},sequence:0,slot:'stream',createdAt:Date.now(),at:Date.now()}}));
  } else if(mode==='threadswitch') {
   await page.evaluate(()=>openThread('acme'));
  } else if(mode==='keyboard')await page.keyboard.press('Enter');else await page.mouse.click(before.x+before.width/2,before.y+before.height/2);
  await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
  const interim=await page.evaluate(()=>({text:document.querySelector('[data-message-id="fixture-prose"]')?.textContent||'',questions:document.querySelectorAll('.question-option').length,state:document.querySelector('[data-question-id="fixture-question"]')?.dataset.state,thread:activeThreadId}));
  await page.mouse.move(50,50);await page.locator('#input').focus();
  await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
  const painted=await page.evaluate(()=>document.querySelector('[data-message-id="fixture-prose"]')?.textContent||'');
  const result={mode,before,after,movedY:after.y-before.y,answers:await page.evaluate(()=>fixtureAnswers),interim,receivedTextLength:prose.length,paintedAfterInteraction:painted.length,errors};
  if(errors.length || result.movedY!==0)throw new Error('Question moved before an actual change or renderer failed');
  if(['pointer','keyboard'].includes(mode) && (result.answers.length!==1 || painted.length<1000))throw new Error('Answer missed or paint did not resume');
  if(mode==='composer' && (interim.text.length<1000 || result.answers.length))throw new Error('Composer interaction still held the reply');
  if(mode==='withdrawn' && (interim.state!=='withdrawn' || interim.questions || interim.text.length<1000 || result.answers.length))throw new Error('Withdrawal did not update during interaction');
  if(mode==='threadswitch' && (interim.thread!=='acme' || interim.questions || result.answers.length))throw new Error('Question interaction held another thread on screen');

    return result;
  } finally { await page.close(); }
}

async function main(){
 const run=createRun("non-blocking question cards"); const browser=await loadPlaywright().webkit.launch();
 const page=await browser.newPage({viewport:{width:1280,height:900}}); const errors=[];page.on("pageerror",e=>errors.push(String(e)));
 await page.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(page);
 await page.evaluate(()=>{
  // This component suite owns its mounting surface. main.js retains the original
  // messages element and periodically repaints it, even without a user turn.
  // Give fixtures an identical sibling so that refresh cannot erase a test card.
  const live=document.getElementById("messages");
  const fixture=live.cloneNode(false);live.id="live-messages";live.hidden=true;
  live.after(fixture);
  window.answerCalls=[];window.shownCalls=[];
  const invoke=window.RichBridge.invoke;
  window.RichBridge.invoke=async(command,args)=>{
   if(command==="question_shown"){shownCalls.push(args);return;}
   if(command==="answer_question"){
    answerCalls.push(args);const q=structuredClone(window.fixtureQuestion);
    q.state="answered";q.revision++;q.answer={...args.answer,method:args.method,surface:"mac"};return {outcome:"accepted",question:q};
   }
   return invoke(command,args);
  };
  window.mountQuestion=(multiple=false)=>{
   document.querySelectorAll(".question-row").forEach(n=>n.remove());
   const q={id:crypto.randomUUID(),thread_id:"general",set_id:"set",text:"When should the release ship?",options:[{id:"today",label:"Ship today",description:"Earlier fixes"},{id:"tomorrow",label:"Ship tomorrow",description:"More testing"}],multiple,free_answer:true,recommended:"tomorrow",state:"open",revision:0,delivered:false};
   window.fixtureQuestion=q;document.getElementById("messages").appendChild(window.RichQuestions.render({threadId:"general",question:q}));
  };
 });
 await run.check("question module loaded before the native bridge still acknowledges and answers",async()=>{
  // index.html loads questions.js before main.js installs RichBridge in Tauri.
  // mock.js installs it earlier in ordinary browser fixtures, hiding this ordering.
  const late=await browser.newPage();const lateErrors=[];late.on("pageerror",e=>lateErrors.push(String(e)));
  try {
   await late.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(late);
   await late.evaluate(()=>{window.savedBridge=window.RichBridge;delete window.RichBridge;});
   await late.addScriptTag({path:path.join(UI_DIR,"questions.js")});
   await late.evaluate(()=>{
    window.lateCalls=[];
    const q={id:"late-bridge",text:"Ship when?",options:[{id:"today",label:"Today",description:"Earlier fixes"},{id:"tomorrow",label:"Tomorrow",description:"More testing"}],multiple:false,free_answer:true,state:"open",revision:0,delivered:false};
    window.RichBridge={...savedBridge,invoke:async(c,a)=>{lateCalls.push(c);if(c==="answer_question")return {outcome:"accepted",question:{...q,state:"answered",revision:1,remaining:0,answer:{...a.answer,method:a.method,surface:"mac"}}};}};
    document.getElementById("messages").appendChild(RichQuestions.render({threadId:"general",question:q}));
   });
   await late.waitForFunction(()=>lateCalls.includes("question_shown"));
   await late.locator('[data-question-id="late-bridge"] .question-option').first().click();
   await late.waitForFunction(()=>lateCalls.includes("answer_question"));
   assertEqual(await late.locator('[data-question-id="late-bridge"]').getAttribute("data-state"),"answered","native-order answer stayed pending");
   assertEqual(lateErrors,[],"native-order renderer errors");
  } finally {await late.close();}
  return "bridge resolves when used, after the native shell installs it";
 });
 await run.check("single tap saves once and leaves composer draft and focus alone",async()=>{
  await page.fill("#input","Keep this draft");await page.evaluate(()=>mountQuestion());
  assertEqual(await page.inputValue("#input"),"Keep this draft","draft changed");
  assertEqual(await page.evaluate(()=>document.activeElement.id),"input","card stole focus");
  assertEqual(await page.locator('.question-option[aria-checked="true"]').count(),0,"option preselected");
  await page.locator(".question-option").first().click();
  await page.waitForFunction(()=>answerCalls.length===1);
  assertEqual(await page.inputValue("#input"),"Keep this draft","answer overwrote composer");
  assert((await page.textContent(".question-status")).includes("Ship today"),"answer missing");
  assertEqual(await page.evaluate(()=>shownCalls.length),1,"display was not acknowledged once");
  return "saved one answer and preserved draft";
 });
 await run.check("multiple choices require Send and keyboard digits stay in the card",async()=>{
  await page.evaluate(()=>mountQuestion(true));await page.focus("#input");await page.keyboard.type("2");
  assertEqual(await page.evaluate(()=>answerCalls.length),1,"composer digit answered question");
  await page.locator(".question-option").first().focus();await page.keyboard.press("1");await page.keyboard.press("2");
  assertEqual(await page.locator('.question-option[aria-checked="true"]').count(),2,"digits did not select choices");
  assertEqual(await page.evaluate(()=>answerCalls.length),1,"selection sent before Send");
  await page.locator(".ask-foot .ask-send").click();
    await page.waitForFunction(()=>answerCalls.length===2);
  assertEqual(await page.evaluate(()=>answerCalls[1].answer.option_ids.length),2,"multiple answer lost choice");return "scoped keyboard and explicit submission";
 });
 await run.check("free answer and Escape preserve the main conversation",async()=>{
  await page.evaluate(()=>mountQuestion());await page.getByRole("button",{name:"Other answer",exact:true}).click();
  await page.getByRole("textbox",{name:"Other answer"}).fill("Next Tuesday");
  await page.locator(".question-controls form button").click();await page.waitForFunction(()=>answerCalls.length===3);
  assertEqual(await page.evaluate(()=>answerCalls[2].answer.text),"Next Tuesday","free answer lost");
  await page.getByRole("button",{name:"Change answer",exact:true}).click();await page.keyboard.press("Escape");
  assertEqual(await page.evaluate(()=>document.activeElement.id),"input","Escape trapped focus");
  await page.locator(".question-option").last().click();await page.waitForFunction(()=>answerCalls.length===4);
  assertEqual(await page.evaluate(()=>answerCalls[3].answer.expected_revision),1,"edit lost revision");return "typed answer, explicit revision and unblocked composer";
 });

 await run.check("uncertain save retries the same answer without replacing it",async()=>{
  await page.evaluate(()=>{mountQuestion();window.originalAnswerInvoke=RichBridge.invoke;let uncertain=true;RichBridge.invoke=async(c,a)=>{if(c==="answer_question" && uncertain){uncertain=false;window.uncertainAnswer=a;throw new Error("Receipt lost");}return originalAnswerInvoke(c,a);};});
  await page.locator(".question-option").first().click();
  await page.getByRole("button",{name:"Retry saved answer",exact:true}).click();
  await page.waitForFunction(()=>answerCalls.length===5);
  assertEqual(await page.evaluate(()=>answerCalls[4]),await page.evaluate(()=>uncertainAnswer),"retry changed the saved input");
  await page.evaluate(()=>{RichBridge.invoke=originalAnswerInvoke;});
  return "same submission identity and exact answer after an uncertain receipt";
 });
 await run.check("question controls stay stable during arrivals and release for actual changes",async()=>{
  for(const mode of ["pointer","keyboard","composer","withdrawn","threadswitch"]) await stableQuestionDuringArrivals(browser,mode);
  return "first click and keyboard answer survive prose/activity; new input, withdrawal and thread switch stay live";
 });
 await run.check("round 13 text, boundaries and font scaling work in both themes",async()=>{
  const C=require("./lib/contrast");await page.addScriptTag({content:C.pageScript()});
  const measurements=[];
  for(const theme of ["dark","light"]){
   await page.evaluate(theme=>{document.documentElement.dataset.theme=theme;mountQuestion(true);},theme);
   await page.locator(".question-card.ask").scrollIntoViewIfNeeded();
   await page.locator(".question-option").first().focus();
   await page.waitForTimeout(650);
   const values=await page.evaluate(()=>{
    const M=window.__contrastMath,card=document.querySelector(".question-card.ask");
    const background=el=>{const chain=[];for(let e=el;e;e=e.parentElement)chain.unshift(e);let bg={r:255,g:255,b:255,a:1};for(const e of chain){const c=M.parseCssColor(getComputedStyle(e).backgroundColor);if(c)bg=M.compositeOver(c,bg);}return bg;};
    const text=[...card.querySelectorAll("*")].filter(el=>el.getClientRects().length&&[...el.childNodes].some(n=>n.nodeType===3&&n.textContent.trim())&&!el.closest(":disabled")&&!el.classList.contains("ask-box")).map(el=>{const cs=getComputedStyle(el),bg=background(el);return {text:el.textContent,ratio:M.contrastRatio(M.compositeOver(M.parseCssColor(cs.color),bg),bg),size:parseFloat(cs.fontSize)};});
    const controls=[...card.querySelectorAll(".ask-opt,.ask-box")].map(el=>{const cs=getComputedStyle(el),bg=background(el);return {name:el.className,ratio:M.contrastRatio(M.compositeOver(M.parseCssColor(cs.borderTopColor),bg),bg)};});
    return {text,controls,title:getComputedStyle(card.querySelector(".ask-q")).fontSize};
   });
   for(const n of values.text){assert(n.ratio>=4.5,theme+" text contrast: "+JSON.stringify(n));assert(n.size>=16,theme+" text below 16px: "+JSON.stringify(n));}
   for(const n of values.controls)assert(n.ratio>=3,theme+" boundary contrast: "+JSON.stringify(n));
   assertEqual(values.title,"22px","round 13 open title size");
   await page.evaluate(()=>{document.documentElement.style.fontSize="20px";});
   assertEqual(await page.locator(".ask-q").evaluate(el=>getComputedStyle(el).fontSize),"27.5px","question ignored font scaling");
   await page.evaluate(()=>{document.documentElement.style.fontSize="";});
   if(process.env.RICHOS_QUESTION_SHOTS){require("fs").mkdirSync(process.env.RICHOS_QUESTION_SHOTS,{recursive:true});await page.screenshot({path:path.join(process.env.RICHOS_QUESTION_SHOTS,"questions-"+theme+".png")});}
   measurements.push(theme+" text min "+Math.min(...values.text.map(n=>n.ratio)).toFixed(2)+":1; boundary min "+Math.min(...values.controls.map(n=>n.ratio)).toFixed(2)+":1");
  }
  return measurements.join("; ");
 });
 await run.check("a job that stopped before Rich got the answer says so, and only then",async()=>{
  // Rich's ruling on esc-20260927T220629Z-cbc90040. Runs after the round 13 check, which
  // installed window.__contrastMath. The line keeps the "On its way" styling (.ask-state).
  const STOPPED="This job stopped before Rich got your answer.",ON_ITS_WAY="On its way to Rich";
  const measurements=[];
  for(const theme of ["dark","light"]){
   for(const ended of [false,true]){
    const line=await page.evaluate(([theme,ended])=>{
     document.documentElement.dataset.theme=theme;
     document.querySelectorAll(".question-row").forEach(n=>n.remove());
     const q={id:crypto.randomUUID(),thread_id:"general",set_id:"set",text:"When should the release ship?",options:[{id:"today",label:"Ship today",description:"Earlier fixes"},{id:"tomorrow",label:"Ship tomorrow",description:"More testing"}],multiple:false,free_answer:true,recommended:"tomorrow",state:"answered",revision:1,delivered:false,handoff_started:true,remaining:0,ended_before_taken:ended,answer:{option_ids:["today"],text:"",method:"click",surface:"mac"}};
     document.getElementById("messages").appendChild(window.RichQuestions.render({threadId:"general",question:q}));
     const span=document.querySelector(".question-card.ask .ask-state span");
     const M=window.__contrastMath;let bg={r:255,g:255,b:255,a:1};const chain=[];for(let e=span;e;e=e.parentElement)chain.unshift(e);
     for(const e of chain){const c=M.parseCssColor(getComputedStyle(e).backgroundColor);if(c)bg=M.compositeOver(c,bg);}
     const cs=getComputedStyle(span);
     return {text:span.textContent,ratio:M.contrastRatio(M.compositeOver(M.parseCssColor(cs.color),bg),bg),size:parseFloat(cs.fontSize),change:!!document.querySelector(".question-card.ask .ask-link")};
    },[theme,ended]);
    assertEqual(line.text,ended?STOPPED:ON_ITS_WAY,theme+" state line with ended_before_taken="+ended);
    assertEqual(line.change,false,theme+" a handed-off answer offered Change answer");
    if(ended){
     assert(line.ratio>=4.5,theme+" stopped line contrast: "+JSON.stringify(line));assert(line.size>=16,theme+" stopped line below 16px: "+JSON.stringify(line));
     measurements.push(theme+" "+line.ratio.toFixed(2)+":1 at "+line.size+"px");
    }
   }
  }
  return "stopped line only when flagged; "+measurements.join("; ");
 });
 await run.check("a withdrawn card never shows the asker's note, card ids included (nightly 66)",async()=>{
  // Nightly 66 p13: "Rich no longer needs this: Duplicate: ... (card ee45a329-...)".
  const NOTE="Duplicate: Rich already asked the user this exact formal-vs-casual question (card ee45a329-0f1e-4c55-9f0e-6b1f8f1d2a77)";
  const measurements=[];
  for(const theme of ["dark","light"]){
   const line=await page.evaluate(([theme,note])=>{
    document.documentElement.dataset.theme=theme;
    document.querySelectorAll(".question-row").forEach(n=>n.remove());
    const q={id:crypto.randomUUID(),thread_id:"general",set_id:"set",text:"Formal or casual?",options:[{id:"formal",label:"Formal",description:"Polished"},{id:"casual",label:"Casual",description:"Relaxed"}],multiple:false,free_answer:true,state:"withdrawn",revision:1,delivered:false,withdrawal_reason:note};
    document.getElementById("messages").appendChild(window.RichQuestions.render({threadId:"general",question:q}));
    const el=document.querySelector(".question-card.ask .ask-withdrawn"),M=window.__contrastMath;
    let bg={r:255,g:255,b:255,a:1};const chain=[];for(let e=el;e;e=e.parentElement)chain.unshift(e);
    for(const e of chain){const c=M.parseCssColor(getComputedStyle(e).backgroundColor);if(c)bg=M.compositeOver(c,bg);}
    const cs=getComputedStyle(el);
    return {card:document.querySelector(".question-card.ask").textContent,text:el.textContent,ratio:M.contrastRatio(M.compositeOver(M.parseCssColor(cs.color),bg),bg),size:parseFloat(cs.fontSize)};
   },[theme,NOTE]);
   assertEqual(line.text,"Rich no longer needs this.",theme+" withdrawn line");
   assert(!line.card.includes("ee45a329") && !line.card.includes("Duplicate"),theme+" the asker's note is on the card: "+line.card);
   assert(line.ratio>=4.5,theme+" withdrawn line contrast: "+JSON.stringify(line));assert(line.size>=16,theme+" withdrawn line below 16px: "+JSON.stringify(line));
   measurements.push(theme+" "+line.ratio.toFixed(2)+":1 at "+line.size+"px");
  }
  return "the line reads only that Rich no longer needs it; "+measurements.join("; ");
 });
 assertEqual(errors,[],"renderer errors");await browser.close();process.exitCode = run.report()?1:0;
}
main().catch(e=>{console.error(e);process.exit(1);});
