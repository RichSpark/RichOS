/* RichPass is a separate local HTML app. This surface owns only its optional file protection. */
(function () {
  "use strict";
  const bridge=window.RichBridge;if(!bridge)return;
  const call=(name,args)=>bridge.invoke(name,args || {});
  const esc=value=>String(value==null ? "" : value).replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"})[c]);
  const phases={idle:"No protected file",checking:"Checking",verified:"Verified",repairing:"Restoring verified copy",waiting:"Waiting for folder",paused:"Automatic repairs paused",blocked:"Protection needs attention",cache_unavailable:"Verified repair copy unavailable"};
  let view=null,sheet=null,busy=false;
  const button=(action,label)=>`<button type="button" class="dict-btn" data-richpass="${action}"${busy ? " disabled" : ""}>${label}</button>`;
  function paintRow() {
    const row = document.getElementById("set-richpass-state");
    if (!row) return;
    const restored = view?.phase === "verified" && view.notice;
    row.textContent = view ? (restored ? "File restored; review notice" : phases[view.phase] || "Unavailable") : "";
  }
  function paint(){paintRow();if(!sheet || sheet.hidden || !view)return;
    sheet.querySelector(".dict-col").innerHTML=`<section class="dict-sec"><h3 class="dict-sec-t">${esc(phases[view.phase] || "Unavailable")}</h3>
      <p role="status">${esc(view.message)}</p>${view.path ? `<p class="rf-path">${esc(view.path)}</p><p>Release: ${esc(view.release)}${view.lastChecked ? ` · Last checked ${esc(new Date(view.lastChecked*1000).toLocaleString())}` : ""}</p>` : ""}
      ${view.notice ? `<div role="status"><p>RichOS restored the ${view.notice.kind==="contents" ? "contents of" : "reserved location for"} RichPass.html. Close any open RichPass tab and reopen the verified file before entering secrets. Other moved or renamed copies are unprotected.</p>${button("acknowledge","Dismiss this notice")}</div>` : ""}
      ${view.newerAvailable ? "<p>A newer verified release is available. Save and protect a verified copy to switch deliberately.</p>" : ""}
      ${view.supported ? button("save-protect",view.newerAvailable ? "Save and protect the newer verified copy" : "Save and protect a verified copy")+button("choose","Protect an existing RichPass.html") : "<p>Automatic protection is unavailable here. You can still save a standalone copy.</p>"}
      ${view.path ? button("check","Check now")+button("show","Show in Folder")+button("stop","Stop protecting") : ""}
      </section><section class="dict-sec"><h3 class="dict-sec-t">Use RichPass independently</h3><p>Save the self-contained HTML file and open it in your browser. It works without RichOS or an internet connection.</p>${button("save","Save a verified copy without protection")}</section>
      <p id="richpass-error" role="alert" hidden></p>`;
  }
  async function read(){try{view=await call("richpass_status");paint();}catch(e){error(e);}}
  function error(e){if(!sheet || sheet.hidden)return;const n=sheet.querySelector("#richpass-error");if(n){n.hidden=false;n.textContent=String(e);}}
  function close(){sheet.hidden=true;document.getElementById("set-btn")?.focus();}
  function build(){if(sheet)return;sheet=document.createElement("div");sheet.id="richpass-settings";sheet.className="overlay";sheet.hidden=true;sheet.setAttribute("role","dialog");sheet.setAttribute("aria-labelledby","richpass-title");sheet.dataset.dismiss="control:#richpass-settings .dict-close";
    sheet.innerHTML=`<section class="overlay-panel dict-panel"><header class="dict-heading"><h2 id="richpass-title">RichPass</h2><button class="dict-close" type="button" data-richpass="close" aria-label="Close RichPass">×</button></header><div class="dict-body"><div class="dict-col"></div><aside class="dict-how"><h3>Your passwords stay in RichPass</h3><p>Enter your master passphrase only in the standalone HTML file. RichOS never asks for it.</p><h3>One protected location</h3><p>After you confirm the exact location, RichOS restores changed contents and the name RichPass.html after renaming or deletion. Stop protecting before editing, moving or deleting it permanently.</p><p>Protection works only while RichOS runs. Other copies are unprotected. Repair cannot retract code already loaded in an open browser tab.</p><p>Source code is readable inside the HTML. Verified bytes identify an approved release; they do not prove it is free of bugs.</p>${view?.folderPermissions ? button("permissions","Folder permissions") : ""}</aside></div></section>`;document.body.append(sheet);
    sheet.addEventListener("click",async event=>{if(event.target===sheet){close();return;}const b=event.target.closest("[data-richpass]");if(!b)return;const action=b.dataset.richpass;if(action==="close"){close();return;}if(busy)return;
      busy=true;paint();try{if(action==="show"){await call("richpass_show");}else if(action==="permissions"){await call("richpass_permissions");}else{view=await call(action==="save" || action==="save-protect" ? "richpass_save" : "richpass_"+action,{protect:action==="save-protect"});}}
      catch(e){busy=false;paint();error(e);return;}busy=false;paint();sheet.querySelector(`[data-richpass="${action}"]`)?.focus();
    });
    sheet.addEventListener("keydown",e=>{if(e.key==="Escape"){e.preventDefault();e.stopPropagation();close();}if(e.key==="Tab"){const items=[...sheet.querySelectorAll("button")].filter(n=>!n.disabled && n.getClientRects().length);if(e.shiftKey && document.activeElement===items[0]){e.preventDefault();items.at(-1)?.focus();}else if(!e.shiftKey && document.activeElement===items.at(-1)){e.preventDefault();items[0]?.focus();}}});
  }
  function open(){window.RichSettings?.close();window.RichHome?.hide("richpass");if(window.RichDismiss?.question())return;build();sheet.hidden=false;read();sheet.querySelector(".dict-close").focus();}
  window.RichSettings?.registerRichPass({open,paint:paintRow});window.RichPassFiles={open};
  bridge.listen?.("rich://richpass",({payload})=>{view=payload;paint();});
  bridge.listen?.("rich://richpass-notice",()=>window.RichSettings?.toast("RichOS restored RichPass.html. Close open RichPass tabs and reopen the verified file. See Settings > RichPass."));
  read();
})();
