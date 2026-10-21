"use strict";
const path=require("path");
const {loadPlaywright,createRun,assert,assertEqual,UI_DIR,leaveHome}=require("./lib/harness");
async function main(){
 const run=createRun("repository connection in the shipping renderer");
 const browser=await loadPlaywright().webkit.launch();
 await run.check("two repositories connect to an explicitly selected company",async()=>{
  const page=await browser.newPage({viewport:{width:1280,height:900}});const errors=[];page.on("pageerror",e=>errors.push(String(e)));
  await page.addInitScript(()=>window.__RICHOS_MOCK_PRESET__={gitFolders:["/fictional/first project"]});
  await page.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(page);
  await page.click("#set-btn");await page.click("#set-repositories-open");
  await page.waitForSelector("#repository-company option:nth-child(2)",{state:"attached"});
  // 2b: the company he is in (the mock opens on Harbor Analytics' "Running") is already chosen.
  assertEqual(await page.inputValue("#repository-company"),"harbor","the company on screen is pre-selected");
  const company=await page.locator("#repository-company option").nth(1).getAttribute("value");
  await page.selectOption("#repository-company",company);
  await page.fill("#repository-folder","/fictional/first project");await page.click("#repository-connect");
  await page.waitForFunction(()=>document.getElementById("repository-message").textContent==="Folder connected.");
  await page.fill("#repository-folder","/fictional/second project");await page.click("#repository-connect");
  // CEO, 2026-10-06: every connected folder gets Git, so there is no opt-in to tick, and
  // the message says when Git was set up.
  await page.waitForFunction(()=>document.getElementById("repository-message").textContent==="Folder connected. Git was set up to track its files.");
  assertEqual(await page.locator("#repository-initialize").count(),0,"the Initialize Git checkbox is gone");
  assertEqual(await page.locator("#repository-list li").count(),2,"connections retained");
  await page.click("#repository-close");assert(await page.isHidden("#repositories-sheet"),"close failed");
  assertEqual(errors.length,0,"renderer errors");await page.close();return "explicit company, two folders, and Git set up for the one without it";
 });
 await run.check("a refused connection stays unconnected and says why",async()=>{
  const page=await browser.newPage({viewport:{width:1280,height:900}});
  await page.addInitScript(()=>window.__RICHOS_MOCK_PRESET__={repositoryRefusal:true});
  await page.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(page);
  await page.click("#set-btn");await page.click("#set-repositories-open");
  await page.waitForSelector("#repository-company option:nth-child(2)",{state:"attached"});
  await page.selectOption("#repository-company",await page.locator("#repository-company option").nth(1).getAttribute("value"));
  await page.fill("#repository-folder","/fictional/nonempty");await page.click("#repository-connect");
  await page.waitForFunction(()=>document.getElementById("repository-message").textContent.includes("overlaps"));
  assert((await page.textContent("#repository-list")).includes("No project folders connected yet."),"refusal rendered as connected");
  assertEqual(await page.inputValue("#repository-folder"),"/fictional/nonempty","retry lost entered folder");
  assert(!await page.isDisabled("#repository-connect"),"retry disabled");await page.close();return "backend refusal preserved with retry available";
 });
 await run.check("2d  the sheet says his copy, word for word",async()=>{
  // CEO feedback 2026-10-06_01, item 2d: the table is the specification.
  const page=await browser.newPage({viewport:{width:1280,height:900}});
  await page.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(page);
  await page.click("#set-btn");await page.click("#set-repositories-open");
  await page.waitForSelector("#repository-company option:nth-child(2)",{state:"attached"});
  const seen=await page.evaluate(()=>{const q=s=>document.querySelector(s);const notes=[...document.querySelectorAll("#repositories-sheet .overlay-panel > p.overlay-note:not([role])")].map(n=>n.textContent);
   return {headline:q("#repositories-title").textContent,description:notes[0],guidance:notes[1],guidanceSize:notes[1]?getComputedStyle(document.querySelectorAll("#repositories-sheet .overlay-panel > p.overlay-note:not([role])")[1]).fontSize:null,descriptionSize:getComputedStyle(document.querySelectorAll("#repositories-sheet .overlay-panel > p.overlay-note:not([role])")[0]).fontSize,
    company:q('label[for="repository-company"]').textContent,folder:q('label[for="repository-folder"]').textContent,placeholder:q("#repository-folder").placeholder,
    connect:q("#repository-connect").textContent,close:q("#repository-close").textContent,empty:q("#repository-list").textContent};});
  assertEqual(seen.headline,"Connected project folders (repositories)","headline");
  assertEqual(seen.description,"Connect a project folder as this company\u2019s home for Rich. Connecting it does not move or change its existing files.","description");
  assertEqual(seen.guidance,"One folder per company is usually enough. But in rare cases additional folders might be needed. Ask Rich if you\u2019re unsure.","guidance");
  assert(parseFloat(seen.guidanceSize)<parseFloat(seen.descriptionSize)&&parseFloat(seen.guidanceSize)>=14,`guidance is smaller text and never below 14px: ${seen.guidanceSize} under ${seen.descriptionSize}`);
  assertEqual(seen.company,"Company","company label");assertEqual(seen.empty,"No project folders connected yet.","empty state");
  assertEqual(seen.folder,"Project folder location","folder label");assertEqual(seen.placeholder,"Click and select folder","placeholder");
  assertEqual(seen.connect,"Connect folder","primary button");assertEqual(seen.close,"Close","secondary button");
  await page.close();return "headline, description, guidance, labels, empty state, placeholder and both buttons";
 });
 await run.check("2a  the settings row says Connected folders (repositories)",async()=>{
  // CEO feedback 2026-10-06_01, item 2a.
  const page=await browser.newPage({viewport:{width:1280,height:900}});
  await page.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(page);
  await page.click("#set-btn");await page.waitForSelector("#set-repositories-open");
  assertEqual((await page.textContent("#set-repositories-open")).trim(),"Connected folders (repositories)","the settings menu row");
  await page.close();return "the row reads Connected folders (repositories)";
 });
 await run.check("2b  the company he is in is already selected, and the folder field has focus",async()=>{
  // CEO feedback 2026-10-06_01, item 2b and repositories1.mov: he was in a company's thread
  // and had to choose that company again. Here he is in Lumen Labs' thread, which is NOT the
  // company setting (Harbor Analytics), so the thread on screen is what decides.
  const page=await browser.newPage({viewport:{width:1280,height:900}});
  await page.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(page);
  await page.click('[data-thread-id="partner"]');
  await page.waitForFunction(()=>document.getElementById("scope-entity").textContent==="Lumen Labs");
  await page.click("#set-btn");await page.click("#set-repositories-open");
  await page.waitForSelector("#repository-company option:nth-child(2)",{state:"attached"});
  await page.waitForFunction(()=>document.activeElement&&document.activeElement.id==="repository-folder");
  assertEqual(await page.inputValue("#repository-company"),"lumen","the company on screen is pre-selected");
  assert(!await page.isDisabled("#repository-connect"),"Connect folder is ready without choosing a company");
  await page.selectOption("#repository-company","northwind");
  assertEqual(await page.inputValue("#repository-company"),"northwind","choosing another company still works");
  await page.close();return "Lumen Labs pre-selected from the thread on screen; folder field focused; still changeable";
 });
 await run.check("2c  a click in the folder field opens the folder chooser, and the chosen folder fills it",async()=>{
  // CEO feedback 2026-10-06_01, item 2c and repositories2.mov: "a seamless experience would
  // be if I clicked here and the Finder would have opened".
  const page=await browser.newPage({viewport:{width:1280,height:900}});
  await page.addInitScript(()=>window.__RICHOS_MOCK_PRESET__={pickedFolder:"/fictional/picked project"});
  await page.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(page);
  await page.click("#set-btn");await page.click("#set-repositories-open");
  await page.waitForSelector("#repository-company option:nth-child(2)",{state:"attached"});
  await page.click("#repository-folder");
  await page.waitForFunction(()=>document.getElementById("repository-folder").value==="/fictional/picked project");
  assertEqual(await page.evaluate(()=>window.__RICHOS_FOLDER_ASKS__),["Choose a project folder"],"one chooser, titled for this field");
  await page.waitForFunction(()=>document.activeElement&&document.activeElement.id==="repository-connect");
  await page.click("#repository-connect");
  await page.waitForFunction(()=>document.getElementById("repository-message").textContent.startsWith("Folder connected."));
  assert((await page.textContent("#repository-list")).includes("picked"),"the chosen folder is connected");
  await page.close();return "click, chosen folder in the field, Connect folder focused and connecting it";
 });
 await run.check("2c  closing the chooser keeps the field, and a typed path still works",async()=>{
  const page=await browser.newPage({viewport:{width:1280,height:900}});
  await page.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(page);
  await page.click("#set-btn");await page.click("#set-repositories-open");
  await page.waitForSelector("#repository-company option:nth-child(2)",{state:"attached"});
  await page.fill("#repository-folder","/fictional/typed project");
  await page.click("#repository-folder");
  await page.waitForFunction(()=>(window.__RICHOS_FOLDER_ASKS__||[]).length===1);
  assertEqual(await page.inputValue("#repository-folder"),"/fictional/typed project","closing the chooser left the typed path");
  await page.close();return "typed path kept when the chooser is closed without a folder";
 });
 await run.check("2c  the first-run company sheet's folder field uses the same chooser",async()=>{
  // CEO feedback 2026-10-06_03: "Clicking into 'Its folder on this Mac' also needs to open a
  // finder or folder picker", and its placeholder becomes "Click and select folder".
  const page=await browser.newPage({viewport:{width:1280,height:900}});
  await page.addInitScript(()=>window.__RICHOS_MOCK_PRESET__={chosenEntity:null,memory:"none",pickedFolder:"/fictional/company folder"});
  await page.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(page);
  await page.waitForSelector("#memory-setup:not([hidden])");await page.click("#memory-setup-later");
  await page.waitForSelector("#entity-picker:not([hidden])");
  await page.evaluate(()=>[...document.querySelectorAll("#entity-picker button")].find(x=>/add/i.test(x.textContent)).click());
  await page.waitForSelector("#entity-add-folder",{state:"visible"});
  assertEqual(await page.getAttribute("#entity-add-folder","placeholder"),"Click and select folder","placeholder");
  await page.click("#entity-add-folder");
  await page.waitForFunction(()=>document.getElementById("entity-add-folder").value==="/fictional/company folder");
  assertEqual(await page.evaluate(()=>window.__RICHOS_FOLDER_ASKS__),["Choose the company's folder"],"one chooser, titled for this field");
  await page.fill("#entity-add-folder","/fictional/typed");
  assertEqual(await page.inputValue("#entity-add-folder"),"/fictional/typed","typing a path still works");
  await page.close();return "same chooser, its own title, placeholder Click and select folder, typing still works";
 });
 await run.check("Escape with focus back in the folder field closes the report on top first, then the sheet (review rv-20261019T002816Z-ba3129ad-c627)",async()=>{
  // The report panel opened over this sheet from Settings, then focus Tabbed back into the
  // folder field: Escape closed the sheet underneath and left the report open.
  const seen=[];
  for(const opener of ["#bug-btn","#idea-btn"]){
   const page=await browser.newPage({viewport:{width:1280,height:900}});
   await page.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(page);
   await page.click("#set-btn");await page.click("#set-repositories-open");
   await page.waitForFunction(()=>document.activeElement&&document.activeElement.id==="repository-folder");
   await page.click("#set-btn");await page.click(opener);
   await page.waitForSelector("#bugdock:not([hidden])");
   await page.focus("#repository-folder");
   await page.keyboard.press("Escape");
   assert(await page.isHidden("#bugdock"),`${opener}: Escape left the report open`);
   assert(await page.isVisible("#repositories-sheet"),`${opener}: Escape closed the sheet under the report`);
   await page.focus("#repository-folder");
   await page.keyboard.press("Escape");
   assert(await page.isHidden("#repositories-sheet"),`${opener}: the second Escape left the sheet open`);
   seen.push(opener.slice(1,-4));await page.close();
  }
  return seen.join(" and ")+": the report closed first, the sheet on the next Escape";
 });
 await run.check("11  one folder in two companies: the notice, the button, the Also-in line on both rows",async()=>{
  // Plan item 11 and round 36 screen 2 (CEO 2026-10-21, ruling 138): a folder another company has
  // is no longer refused. The sheet says so before anything is connected, the button says what it
  // will do, and the folder's row names the other company in both lists.
  const page=await browser.newPage({viewport:{width:1280,height:900}});const errors=[];page.on("pageerror",e=>errors.push(String(e)));
  await page.goto("file://"+path.join(UI_DIR,"index.html"));await leaveHome(page);
  await page.click("#set-btn");await page.click("#set-repositories-open");
  await page.waitForSelector("#repository-company option:nth-child(3)",{state:"attached"});
  const names=await page.$$eval("#repository-company option",o=>o.map(x=>[x.value,x.textContent]));
  const [[a,aName],[b,bName]]=names.slice(1,3);
  await page.selectOption("#repository-company",a);
  await page.fill("#repository-folder","/fictional/shared project");
  assertEqual(await page.locator(".repository-shared").count(),0,"a folder no other company has shows no notice");
  assertEqual(await page.textContent("#repository-connect"),"Connect folder","the button is unchanged for such a folder");
  await page.click("#repository-connect");
  await page.waitForFunction(()=>document.getElementById("repository-message").textContent.startsWith("Folder connected."));
  assertEqual(await page.locator(".repository-also").count(),0,"nothing is shared yet");
  await page.selectOption("#repository-company",b);
  await page.fill("#repository-folder","/fictional/shared project");
  await page.waitForSelector(".repository-shared");
  const notice=await page.textContent(".repository-shared");
  assert(notice.includes("Already connected to "+aName),"the notice names the company that has it: "+notice);
  assert(notice.includes("Nothing in the folder moves and each company keeps its own chats and notes."),"the notice says what does not change");
  assert(notice.includes("Rich can work in it from "+bName+" chats as well as "+aName+" chats"),"the notice says what connecting means");
  assertEqual(await page.textContent("#repository-connect"),"Connect to "+bName+" too","the button says what it will do");
  await page.click("#repository-connect");
  await page.waitForFunction(()=>document.getElementById("repository-message").textContent.startsWith("Folder connected."));
  assertEqual(await page.textContent("#repository-message"),"Folder connected. It is now in "+bName+" and "+aName+".","the message");
  assertEqual(await page.locator(".repository-shared").count(),0,"the notice is gone once connected");
  assertEqual((await page.textContent("#repository-list .repository-also")).trim(),"Also in "+aName,"this company's row names the other");
  assertEqual(await page.locator("#repository-list li.repository-new").count(),1,"the new row lights once");
  await page.selectOption("#repository-company",a);
  assertEqual((await page.textContent("#repository-list .repository-also")).trim(),"Also in "+bName,"the other company's row names this one");
  assertEqual(await page.textContent("#repository-connect"),"Connect folder","the button is back");
  assertEqual(errors.length,0,"renderer errors");await page.close();return "notice, Connect to "+bName+" too, and Also in on both rows";
 });
 await browser.close();process.exitCode = run.report()?1:0;
}
main().catch(e=>{console.error(e);process.exit(1);});
