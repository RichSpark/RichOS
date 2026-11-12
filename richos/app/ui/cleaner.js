/* RichCleaner's native window and nonactivating panel. Every state comes from the owner. */
(function () {
  "use strict";
  const native = window.__TAURI__;
  if (!native || !native.core) return;
  const call = (name, args) => native.core.invoke(name, args || {});
  const panel = new URLSearchParams(location.search).has("panel");
  const root = document.getElementById("cleaner-root");
  const esc = value => String(value == null ? "" : value).replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"})[c]);
  const name = entry => String(entry.path || "").split("/").pop();
  let status = {}, operation = {}, entries = [], selected = null, next = null, query = "", generation = 0, loading = false, reload = false, detailGeneration = 0;
  const opening = new Set(), downloading = new Set();
  const button = (action, text, id, cls) => `<button type="button" class="${cls || "rf-quiet"}" data-action="${action}"${id ? ` data-id="${esc(id)}"` : ""}>${esc(text)}</button>`;
  const openButton = id => button("open", downloading.has(id) ? "Downloading from iCloud…" : "Open", id, "rf-open");
  function icon(entry) { const ext = name(entry).split(".").pop().toUpperCase().slice(0,5); return `<svg class="fico" viewBox="0 0 40 48" width="44" height="52" aria-hidden="true"><path d="M6 1h19l10 10v36H6z" fill="var(--surface)" stroke="var(--line-control)"/><path d="M25 1v10h10" fill="var(--gold)"/><text x="20" y="38" text-anchor="middle" font-family="var(--font)" font-size="9" fill="var(--ink)">${esc(ext)}</text></svg>`; }
  function tell(error) {let target=document.getElementById("cleaner-error");if(!target){target=document.createElement("p");target.id="cleaner-error";target.className="rf-error";target.setAttribute("role","alert");root.prepend(target);}target.textContent=String(error);}
  function noteMode(e) {return e.projection.mode==="embedded" ? "Note inside the file: current" : e.projection.mode==="older" ? "Note saved in RichOS. The note inside the file is older" : `Note saved in RichOS only. ${e.projection.reason || ""}`;}
  function actions(e) {return openButton(e.id)+button("reveal","Show in Folder",e.id)+(e.undo ? button("undo","Undo filing",e.id) : "")+button("edit-focus","Edit note",e.id);}
  function batchSummary(result, folder) {
    const files = count => `${count} ${count === 1 ? "file" : "files"}`;
    return [
      result.filed && `${files(result.filed)} filed to ${folder}`,
      result.inbox && `${files(result.inbox)} in Inbox`,
      result.draft && `${files(result.draft)} left in place`,
      result.skipped && `${files(result.skipped)} handled separately`,
      result.failed && `${files(result.failed)} ${result.failed === 1 ? "needs" : "need"} attention`,
    ].filter(Boolean).join(". ");
  }
  function selectedNames(names) {
    const visible = names.slice(0,2).map(name=>{
      const characters = Array.from(name);
      return characters.length <= 32 ? name : characters.slice(0,20).join("")+"…"+characters.slice(-11).join("");
    }).join(", ");
    return names.length > 2 ? `${visible} and ${names.length-2} more` : visible;
  }
  function paintPanel(view) {
    operation=view;
    if(view.quiet)return;
    if (view.stage==="idle") { root.innerHTML="";return; }
    const e=view.entry, f=e || view.file, k=`F${view.key || status.key || 1}`;
    const batch = view.batch;
    const selection = batch?.names;
    const texts={busy:"Still sorting the previous file",checking:"Checking the file…",listening:"Listening",writing:"Writing it down…",saved:"Note saved",choosing:"Choosing a folder…",filing:`Filing to ${view.folder || "your folder"}…`,filed:`Filed to ${e ? e.folder : "your folder"}`,"inbox-pending":"Safely stored in Inbox",inbox:"Stored in Inbox",draft:"Note saved, not filed",canceled:"Canceled",error:"Couldn't complete it"};
    if(batch){
      texts.busy="Still sorting the previous selection";
      texts.checking=`Checking ${batch.total} files…`;
      texts["inbox-pending"]=`${view.batchStored} of ${batch.total} files in Inbox`;
      if(view.batchResult)texts["batch-result"]=batchSummary(view.batchResult,view.folder);
    }
    let controls="";
    if(view.stage==="listening") controls=button("finish","Done",view.id,"dov-btn")+button("cancel","Cancel",view.id);
    else if(["checking","writing","choosing"].includes(view.stage)) controls=button("cancel","Cancel",view.id);
    else if(e && ["filed","inbox","inbox-pending","draft","error","busy"].includes(view.stage)) controls=actions(e)+button("library","Recent files");
    if(view.stage==="error" && !e) controls=button("settings","RichCleaner settings")+button("library","Recent files");
    if(batch && ["inbox-pending","busy","batch-result"].includes(view.stage))controls=button("library","Recent files");
    let sub=view.message || "";
    if(view.stage==="listening") sub=`Tap ${k} to finish`;
    if(view.stage==="choosing") sub=batch ? "Cancel keeps the note and leaves the files where they are." : "Cancel keeps the note and leaves the file where it is.";
    if(view.batchResult?.localNote) {
      const count = view.batchResult.localNote;
      sub = `The note is saved in RichOS only for ${count} ${count === 1 ? "file" : "files"}. See Recent files for details.`;
    }
    root.innerHTML=`<section class="rf-panel ${view.stage==="listening" ? "is-listen" : ""}" aria-label="RichCleaner capture">
      ${f || selection ? `<div class="rf-file"><span class="rf-thumb">${icon(f || {path:"FILES"})}</span><div class="rf-ident"><div class="rf-name">${esc(batch ? `${batch.total} Desktop files` : f.name || name(f))}</div><div class="rf-where" title="${esc(selection ? selection.join(", ") : f.path)}">${esc(selection ? selectedNames(selection) : f.path)}</div></div><span class="rf-mark"><img src="cleaner-icon.png" alt=""></span></div>` : ""}
      <div class="rf-stage"><span class="dh-orb" aria-hidden="true">${view.stage==="listening" ? "●" : ["filed","saved","inbox-pending","inbox"].includes(view.stage) ? "✓" : ""}</span>
      <strong class="dh-label" role="status">${esc(texts[view.stage] || view.stage)}</strong>${view.stage==="listening" ? '<span class="dh-meter" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span>' : ""}
      ${sub ? `<p class="rf-sub">${esc(sub)}</p>` : ""}<div class="rf-acts is-row">${controls}</div></div>
      ${view.noteSaved && (e || view.batchNote) ? `<div class="rf-note"><span class="rf-note-k">Your note</span>${esc(e?.note || view.batchNote)}</div>` : ""}
      ${["filed","inbox","inbox-pending"].includes(view.stage) && e ? `<p class="rf-sub">${esc(noteMode(e))}</p>` : ""}
      ${["checking","writing","choosing","filing"].includes(view.stage) ? '<div class="rf-bar" aria-hidden="true"></div>' : ""}
      </section>`;
  }
  function paintList(total) {
    const list=document.getElementById("cleaner-list");
    list.innerHTML=`<div class="rf-list-head"><b>${query ? "Search results" : "Recent files"}</b><span>${total} ${total===1 ? "file" : "files"}</span></div>`;
    // Off, the empty window says so and offers the switch, rather than steps that cannot work (round 58).
    if(!entries.length && !query && status.enabled===false) {list.innerHTML+=`<div class="rf-empty"><img src="cleaner-icon.png" alt=""><h3>RichCleaner is off</h3><p>Turn it on to file the Desktop files you describe. Your files stay where they are until you do.</p>${button("settings","Turn on RichCleaner",null,"rf-open")}</div>`;selected=null;paintDetails();return;}
    if(!entries.length) {list.innerHTML+=`<div class="rf-empty"><img src="cleaner-icon.png" alt=""><h3>${query ? "No matching notes" : "A clear Desktop starts here"}</h3><p>${query ? "Try a different description, or ask Rich to find it." : `Select the Desktop files, tap F${status.key || 1}, describe your selection and tap the same key again.`}</p></div>`;selected=null;paintDetails();return;}
    for(const e of entries) { const fileName=name(e); list.insertAdjacentHTML("beforeend",`<button type="button" role="option" aria-selected="${selected===e.id}" class="rf-row" data-select="${esc(e.id)}"><span class="rf-ico">${icon(e)}</span><span class="rf-row-text"><span class="rf-nm" title="${esc(fileName)}"><span class="h">${esc(fileName.slice(0,-15))}</span><span class="t">${esc(fileName.slice(-15))}</span></span><span class="rf-row-note">${esc(e.note)}</span><span class="rf-row-meta">${esc(e.folder || "Not filed")} · ${esc(new Date(e.updated).toLocaleString())}${e.pendingSort ? '<span class="rf-chip">In Inbox · sorting</span>' : e.phase==="draft" ? '<span class="rf-chip">Not filed</span>' : e.inbox ? '<span class="rf-chip">Inbox</span>' : ""}</span></span></button>`); }
    if(!selected || !entries.some(e=>e.id===selected)) selected=entries[0].id;
    for(const row of list.querySelectorAll("[data-select]")) row.setAttribute("aria-selected",String(row.dataset.select===selected));paintDetails();
  }
  function paintDetails(verified) {
    const detail=document.getElementById("cleaner-details");const e=entries.find(e=>e.id===selected);
    detail.hidden=!e;if(!e)return;
    detail.innerHTML=`<div class="rf-det-top"><span class="rf-det-thumb">${icon(e)}</span><div><h2 class="rf-det-name">${esc(name(e))}</h2><div class="rf-det-kind">${esc(e.identity.size.toLocaleString())} bytes</div></div></div>
      <div class="rf-det-acts">${verified?.available ? openButton(e.id)+button("reveal","Show in Folder",e.id) : button("locate","Locate original",e.id)}</div>
      <section class="rf-sec"><h3 class="rf-sec-t">Where it is</h3><p class="rf-path">${esc(e.path)}</p><p>${esc(noteMode(e))}</p>${e.problem ? `<p class="rf-error">${esc(e.problem)}</p>` : ""}</section>
      <section class="rf-sec"><div class="rf-sec-h"><label class="rf-sec-t" for="cleaner-note">Your note</label><span class="rf-sec-s" id="cleaner-save-state" role="status">Saved</span></div><textarea id="cleaner-note" class="rf-edit" data-id="${esc(e.id)}">${esc(e.note)}</textarea><p>Type or click in the box and tap F${status.key || 1} to say it.</p></section>
      <section class="rf-sec"><h3 class="rf-sec-t">Filing</h3><p>Changing the note does not move the file. File again asks RichCleaner to choose a folder using this note.</p><div class="rf-det-acts">${button("file-again","File again",e.id)}${e.undo ? button("undo","Undo filing",e.id) : ""}</div></section>
      ${button("remove","Remove from RichCleaner",e.id,"rf-remove")}<p class="rf-remove-why">The file stays exactly where it is.</p>`;
    if(!verified){const version=++detailGeneration;call("cleaner_details",{id:e.id}).then(answer=>{if(version!==detailGeneration || selected!==e.id)return;const index=entries.findIndex(item=>item.id===e.id);if(index>=0)entries[index]=answer.entry;const field=document.getElementById("cleaner-note");if(field?.matches(":focus") || field?.value!==e.note)return;paintDetails(answer);}).catch(tell);}

  }
  async function load(more) {
    if(loading){if(!more)reload=true;return;}loading=true;const id=generation;
    try{const [answer,view]=await Promise.all([call("cleaner_list",{query,offset:more ? next || 0 : 0}),more ? null : call("cleaner_status").catch(()=>null)]);if(view)status=view;if(id!==generation)return;if(saveBusy || dirty()){deferredRefresh=true;return;}entries=more ? entries.concat(answer.entries) : answer.entries;next=answer.next;paintList(answer.total);}catch(e){tell(e);}finally{loading=false;if(reload){reload=false;load(false);}}
  }
  function paintLibrary() {
    root.innerHTML=`<section class="rf-win"><header class="rf-win-bar"><div class="rf-win-title"><img src="assets/app-icons/richcleaner-128.png" alt="">RichCleaner</div><label class="rf-search"><input id="cleaner-search" type="search" placeholder="Search your notes" aria-label="Search your saved files"></label></header><div class="rf-win-body"><div class="rf-list" id="cleaner-list" role="listbox" aria-label="Saved files"></div><aside class="rf-det" id="cleaner-details" aria-label="Selected file"></aside></div></section>`;
    let timer;document.getElementById("cleaner-search").addEventListener("input",event=>{query=event.target.value;generation++;clearTimeout(timer);timer=setTimeout(()=>load(false),100);});
    document.getElementById("cleaner-list").addEventListener("scroll",event=>{const list=event.target;if(next!==null && list.scrollTop+list.clientHeight>=list.scrollHeight-100)load(true);});load(false);
  }
  root.addEventListener("click",async event=>{
    const row=event.target.closest("[data-select]");if(row){try{await flushEdits();}catch(e){tell(e);return;}selected=row.dataset.select;for(const node of root.querySelectorAll("[data-select]"))node.setAttribute("aria-selected",String(node===row));paintDetails();return;}
    const b=event.target.closest("[data-action]");if(!b)return;const action=b.dataset.action,id=b.dataset.id;
    if(action==="open"){
      if(opening.has(id))return;
      opening.add(id);
    }
    try{
      if(["finish","cancel"].includes(action)){await call("cleaner_capture_control",{id,action});return;}
      if(action==="settings"){await call("cleaner_open_main",{surface:"cleaner"});return;}
      if(action==="library" || action==="edit-focus"){await call("cleaner_open_window");if(!panel){document.getElementById("cleaner-note")?.focus();}return;}
      if(!panel)await flushEdits();
      b.disabled=true;const result=await call("cleaner_action",{id,action,note:null});
      if(panel && result.id) paintPanel({id:operation.id,stage:action==="undo" ? "draft" : operation.stage,entry:result,noteSaved:true,message:action==="undo" ? "Filing undone. Your note is kept." : ""});
      if(!panel) await load(false);
    }catch(e){tell(e);}finally{
      b.disabled=false;
      if(action==="open"){
        opening.delete(id);
        downloading.delete(id);
        for(const control of root.querySelectorAll('[data-action="open"]')){
          if(control.dataset.id===id)control.textContent="Open";
        }
      }
    }
  });
  root.addEventListener("keydown",event=>{if(event.key==="Escape" && panel && ["checking","listening","writing","choosing"].includes(operation.stage)){event.preventDefault();call("cleaner_capture_control",{id:operation.id,action:"cancel"}).catch(tell);}const row=event.target.closest("[data-select]");if(row && ["ArrowDown","ArrowUp"].includes(event.key)){event.preventDefault();const rows=[...root.querySelectorAll("[data-select]")];const nextRow=rows[rows.indexOf(row)+(event.key==="ArrowDown" ? 1 : -1)];if(nextRow){nextRow.click();nextRow.focus();}}});
  // Save after editing stops or focus leaves. The owner commits locally before updating metadata.
  let saveTimer,saveChain=Promise.resolve(),saveBusy=0,deferredRefresh=false;
  function dirty(){const field=document.getElementById("cleaner-note");return field && entries.find(e=>e.id===field.dataset.id)?.note!==field.value;}
  function saveNote(field){
    const value=field.value,fileId=field.dataset.id;saveBusy++;
    const task=saveChain.then(async()=>{
      if(entries.find(e=>e.id===fileId)?.note===value)return;
      const hint=()=>selected===fileId ? document.getElementById("cleaner-save-state") : null;
      if(hint())hint().textContent="Saving…";
      try{const result=await call("cleaner_action",{id:fileId,action:"edit",note:value});const index=entries.findIndex(e=>e.id===fileId);if(index>=0)entries[index]=result;
        const current=document.getElementById("cleaner-note");if(hint())hint().textContent=current?.value===value ? "Saved" : "Unsaved changes";
      }catch(e){if(hint())hint().textContent="Couldn't save. Your changes are still here";throw e;}
    }).finally(()=>{saveBusy--;if(!saveBusy && deferredRefresh && !dirty() && !document.getElementById("cleaner-note")?.matches(":focus")){deferredRefresh=false;load(false);}});
    saveChain=task.catch(()=>{});return task;
  }
  function flushEdits(){clearTimeout(saveTimer);const field=document.getElementById("cleaner-note");return field ? saveNote(field) : saveChain;}
  root.addEventListener("input",event=>{if(event.target.id!=="cleaner-note")return;clearTimeout(saveTimer);const field=event.target;const hint=document.getElementById("cleaner-save-state");if(hint)hint.textContent="Unsaved changes";saveTimer=setTimeout(()=>saveNote(field).catch(tell),600);});
  root.addEventListener("focusout",event=>{if(event.target.id!=="cleaner-note")return;clearTimeout(saveTimer);if(dirty())saveNote(event.target).catch(tell);});
  if(!panel && native.window?.getCurrentWindow){const win=native.window.getCurrentWindow();let closing=false;win.onCloseRequested(async event=>{if(closing || !dirty() && !saveBusy)return;event.preventDefault();try{await flushEdits();closing=true;await win.close();}catch(e){tell(e);}}).catch(tell);}
  // Back in the window (after Turn on RichCleaner in the main window, say), read the switch and the list
  // again, so an off state that is no longer true never stays on screen (round 58); never while the note is being edited.
  if(!panel){const back=()=>{if(document.hidden || !document.getElementById("cleaner-list") || saveBusy || dirty() || document.activeElement?.id==="cleaner-note")return;load(false);};window.addEventListener("focus",back);document.addEventListener("visibilitychange",back);}
  document.body.className=panel ? "cleaner-panel" : "cleaner-library";
  document.documentElement.classList.toggle("cleaner-panel",panel);
  native.event.listen("rich://cleaner",event=>{const view=event.payload;if(panel){if(view.stage!=="library-changed")paintPanel(view);}else if(!saveBusy && !dirty() && !document.getElementById("cleaner-note")?.matches(":focus"))load(false);}).catch(tell);
  native.event.listen("rich://cleaner-download",event=>{
    const {id}=event.payload;
    if(!opening.has(id))return;
    downloading.add(id);
    for(const control of root.querySelectorAll('[data-action="open"]')){
      if(control.dataset.id===id)control.textContent="Downloading from iCloud…";
    }
  }).catch(tell);
  native.event.listen("cleaner-level",event=>{const level=Math.max(0,Math.min(1,event.payload/100));const pill=root.querySelector(".rf-panel");if(pill)pill.style.setProperty("--lvl",level);root.querySelectorAll(".dh-meter i").forEach((bar,i)=>bar.style.transform=`scaleY(${Math.max(.08,level*[.5,.8,1,.8,.5][i])})`);}).catch(tell);
  if(panel){let last="";new ResizeObserver(()=>{const pill=root.querySelector(".rf-panel");if(!pill)return;const w=630,h=Math.ceil(pill.getBoundingClientRect().height+48),key=`${w}:${h}`;if(key!==last){last=key;call("cleaner_panel_size",{w,h}).catch(tell);}}).observe(root);}
  call("cleaner_status").then(view=>{status=view;if(!panel && window.RichSettings){window.RichSettings.registerCleaner({open:()=>call("cleaner_open_main",{surface:"cleaner"}),paint:()=>{}});window.RichSettings.registerBugReport({open:kind=>call("cleaner_open_main",{surface:kind==="idea" ? "idea" : "bug"}).catch(tell)});window.RichSettings.registerDurable({saveTheme:theme=>call("set_theme",{theme}),saveScale:scale=>call("set_font_scale",{scale})});}if(window.RichTheme)window.RichTheme.sync({theme:view.theme,font_scale:view.fontScale});if(panel)paintPanel(view.operation);else paintLibrary();}).catch(tell);
})();
