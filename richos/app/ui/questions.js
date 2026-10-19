"use strict";
// Inline question controls. The composer is never used as an answer field.
(function () {
  // main.js installs the native bridge after this module loads. Resolve it at
  // use time; capturing it here works only with the earlier mock bridge.
  const shown = new Set();
  const drafts = new Map();
  // What Claude wrote on the card (the question and its options) is drawn through the Oxford comma
  // setting (CEO, 2026-10-19), the same filter as Rich's replies (`oxford-comma.js`).
  const said=(text)=>window.RichOxford?window.RichOxford.display(text):text;
  function node(tag, text, cls) { const el=document.createElement(tag); if(text) el.textContent=text; if(cls) el.className=cls; return el; }
  function render(item, opts) {
    const q=item.question;
    const card=node("section",null,"question-card ask");card.dataset.questionId=q.id;card.dataset.state=q.state;
    card.setAttribute("role","group");card.setAttribute("aria-labelledby",`question-${q.id}`);

    if(q.set_count>1)card.append(node("div",`Question ${q.set_index} of ${q.set_count}`,"ask-set"));
    const title=node("p",said(q.text),"question-title ask-q");title.id=`question-${q.id}`;card.append(title);
    const draft=drafts.get(q.id) || {editing:false,selected:new Set(),text:"",other:false,revision:q.revision};
    if(draft.revision!==q.revision){draft.editing=false;draft.selected.clear();draft.revision=q.revision;}
    drafts.set(q.id,draft);
    let busy=false; const selected=draft.selected;
    const pendingKey=`richos-question-answer:${q.id}`;
    function pendingAnswer(){try{return JSON.parse(localStorage.getItem(pendingKey)||"null");}catch{return null;}}
    const controls=node("div",null,"question-controls");
    const status=node("div",null,"question-status");status.setAttribute("role","status");status.setAttribute("aria-live","polite");
    card.append(controls,status);
    const labels=(answer)=>[...q.options.filter(o=>answer.option_ids.includes(o.id)).map(o=>o.label),answer.text].filter(Boolean).join("; ");
    function draw() {
      controls.replaceChildren();status.replaceChildren();
      card.classList.toggle("is-answered",q.state==="answered");card.classList.toggle("is-withdrawn",q.state==="withdrawn");card.classList.toggle("is-delivered",!!q.delivered);
      const saved=pendingAnswer();
      if(saved) {
        status.textContent=saved.deleted?"This conversation was deleted. Your answer was not sent. You can copy it below.":"Your answer is saved here and has not been confirmed.";
        controls.append(node("pre",saved.display));
        if(!saved.deleted){const retry=node("button","Retry saved answer");retry.onclick=()=>sendSaved(saved);controls.append(retry);}
        return;
      }
      // The withdrawal reason is the asker's note for the record (nightly 66: "Duplicate: ...
      // (card ee45a329-...)"), not words for him, so the card says only what changed.
      if(q.state==="withdrawn") {status.append(node("p","Rich no longer needs this.","ask-withdrawn"));return;}
      if(q.state==="answered" && !draft.editing) {
        const method={click:"click",keyboard:"keyboard",typed:"typing",spoken:"voice",phone_tap:"tap",phone_typed:"typing",phone_voice:"voice note"}[q.answer.method]||"your words";
        const answerLine=node("p",null,"ask-answer");answerLine.append(node("span","✓","ask-check"),node("span",`You answered: ${labels(q.answer)}`));status.append(answerLine);
        status.append(node("p",`By ${method}, on ${q.answer.surface==="phone"?"your phone":"this Mac"}`,"ask-how"));
        const stateLine=node("div",null,"ask-state");stateLine.append(node("span",q.delivered?"Rich has your answer":q.ended_before_taken?"This job stopped before Rich got your answer.":q.remaining>0?"Waiting for the remaining answers":q.waiting_for_turn?"It reaches Rich when his current reply ends":"On its way to Rich"));
        if(!q.delivered && !q.handoff_started) {const change=node("button","Change answer","ask-link");change.type="button";change.onclick=()=>{draft.editing=true;selected.clear();for(const id of q.answer.option_ids)selected.add(id);draft.text=q.answer.text;draw();controls.querySelector("button")?.focus();};stateLine.append(change);}
        status.append(stateLine);
        return;
      }
      status.textContent=draft.editing?"Change your answer before Rich receives it.":"";
      const buttons=[];
      const options=node("div",null,"ask-options");controls.append(options);
      for(const [index,option] of q.options.entries()) {
        const button=node("button",null,"question-option ask-opt");button.type="button";
        button.append(node("span",String(index+1),"ask-key"));
        if(q.multiple){button.classList.add("is-multi");const box=node("span","✓","ask-box");box.setAttribute("aria-hidden","true");button.append(box);}
        const body=node("span",null,"ask-body");body.append(node("span",said(option.label),"question-label ask-label"),node("span",said(option.description),"question-description ask-desc"));button.append(body);
        if(q.recommended===option.id) button.append(node("span",q.asker==="Your team"?"Your team recommends":"Rich recommends","question-recommended ask-rec"));
        if(q.multiple) {button.setAttribute("role","checkbox");button.setAttribute("aria-checked",String(selected.has(option.id)));}
        button.addEventListener("click",event=> {
          if(busy)return;
          if(q.multiple) {selected.has(option.id)?selected.delete(option.id):selected.add(option.id);button.setAttribute("aria-checked",String(selected.has(option.id)));send.disabled=!selected.size;}
          else submit([option.id],"",event.detail===0?"keyboard":"click");
        });
        button.addEventListener("keydown",event=> {
          if(event.key==="ArrowDown"||event.key==="ArrowRight"||event.key==="ArrowUp"||event.key==="ArrowLeft") {
            event.preventDefault();buttons[(index+(["ArrowUp","ArrowLeft"].includes(event.key)?buttons.length-1:1))%buttons.length].focus();
          }
        });
        buttons.push(button);options.append(button);
      }
      const foot=node("div",null,"ask-foot");controls.append(foot);
      const send=node("button","Send answer","ask-send");send.type="button";send.disabled=!selected.size;
      send.onclick=event=>submit([...selected],"",event.detail===0?"keyboard":"click");if(q.multiple)foot.append(send);
      if(q.free_answer) {
        const other=node("button","Other answer","ask-link");other.type="button";other.setAttribute("aria-expanded","false");
        const form=node("form",null,"ask-other");form.hidden=!draft.other;other.setAttribute("aria-expanded",String(draft.other));
        const input=node("textarea");input.setAttribute("aria-label","Other answer");input.placeholder="Your answer";input.maxLength=16384;input.value=draft.text;input.oninput=()=>{draft.text=input.value;};
        const custom=node("button","Send answer","ask-send");custom.type="submit";
        form.append(input,custom);foot.prepend(other);controls.append(form);
        other.onclick=()=>{form.hidden=!form.hidden;draft.other=!form.hidden;other.setAttribute("aria-expanded",String(!form.hidden));if(!form.hidden)input.focus();};
        form.onsubmit=event=>{event.preventDefault();if(input.value.trim())submit(q.multiple?[...selected]:[],input.value.trim(),"typed");};
      }
      controls.append(node("p",`↑ ↓ move · 1-${q.options.length} choose · ${q.multiple?"Space toggles · Send answer confirms":"Enter answers"} · Esc back to the composer`,"ask-hint"));
    }
    async function submit(optionIds,text,method) {
      if(busy || pendingAnswer())return;
      const answer={question_id:q.id,client_id:crypto.randomUUID(),option_ids:optionIds,text};
      if(draft.editing)answer.expected_revision=q.revision;
      const saved={answer,method,threadId:item.threadId,display:labels(answer)};
      try {localStorage.setItem(pendingKey,JSON.stringify(saved));}
      catch {status.textContent="Your answer could not be saved on this Mac. Try again after freeing storage.";return;}
      await sendSaved(saved);
    }
    async function sendSaved(saved) {
      if(busy)return;busy=true;status.textContent="Saving your answer…";
      try {
        const result=await window.RichBridge.invoke("answer_question",{threadId:saved.threadId,answer:saved.answer,method:saved.method});
        if(result.outcome==="conversation_deleted") {
          saved.deleted=true;localStorage.setItem(pendingKey,JSON.stringify(saved));draw();
          document.dispatchEvent(new Event("richos-questions-changed"));return;
        }
        localStorage.removeItem(pendingKey);
        if(result.question)Object.assign(q,result.question);
        draft.editing=false;draft.selected.clear();draft.text="";draft.other=false;draw();
        card.dataset.state=q.state;
        document.dispatchEvent(new Event("richos-questions-changed"));
      }catch(error){draw();status.textContent="Your saved answer could not be confirmed. Retry it when the connection is available.";}
      finally{busy=false;}
    }
    card.addEventListener("keydown",event=>{
      if(event.key==="Escape"){event.preventDefault();event.stopPropagation();document.getElementById("input")?.focus();}
      if(/^[1-4]$/.test(event.key) && !["TEXTAREA","INPUT"].includes(event.target.tagName)) {
        const choice=controls.querySelectorAll(".question-option")[Number(event.key)-1];
        if(choice){event.preventDefault();choice.click();}
      }
    });
    draw();
    // A render receipt follows DOM attachment, not construction or persistence.
    const acknowledge=()=>{
      if(card.isConnected && document.visibilityState === "visible" && card.getClientRects().length && !shown.has(q.id)) {
        shown.add(q.id);
        window.RichBridge.invoke("question_shown",{threadId:item.threadId,questionId:q.id}).catch(()=>shown.delete(q.id));
      }
    };
    requestAnimationFrame(acknowledge);
    const visibilityChanged=()=>{if(!card.isConnected)document.removeEventListener("visibilitychange",visibilityChanged);else acknowledge();};
    document.addEventListener("visibilitychange",visibilityChanged);
    const row=node("article",null,"tl-rich question-row");
    row.dataset.questionOwner=q.id;
    if(!opts || opts.showIdentity()) {
      const meta=node("div",null,"tl-rich-meta");
      if(opts?.showAvatar()) {const avatar=node("img",null,"tl-avatar");avatar.src="assets/rich-hand.png";avatar.alt="";meta.append(avatar);}
      meta.append(node("span",q.asker && q.asker!=="Rich" ? q.asker+" · through Rich" : "Rich","tl-who"));row.append(meta);
    }
    row.append(card);
    return row;
  }
  function installControl() {
    const zone=document.getElementById("composer-zone");if(!zone)return;
    const control=node("button",null,"questions-open");control.type="button";control.hidden=true;control.title="Jump to the oldest open question (⌘J)";zone.prepend(control);
    const recovery=node("div",null,"question-recovery");zone.prepend(recovery);
    function showRecovery(){
      recovery.replaceChildren();
      for(let i=0;i<localStorage.length;i++){
        const key=localStorage.key(i);if(!key?.startsWith("richos-question-answer:"))continue;
        let saved;try{saved=JSON.parse(localStorage.getItem(key));}catch{continue;}
        if(!saved?.deleted)continue;
        recovery.append(node("p","This conversation was deleted. This answer was not sent:"),node("pre",saved.display));
        const copy=node("button","Copy saved answer");copy.onclick=()=>navigator.clipboard.writeText(saved.display);recovery.append(copy);
      }
    }
    const refresh=()=>{showRecovery();const cards=[...document.querySelectorAll('.question-card[data-state="open"]')];control.textContent=`${cards.length} question${cards.length===1?"":"s"} open`;control.hidden=!cards.length||cards.every(c=>{const r=c.getBoundingClientRect();return r.top>=0&&r.bottom<=zone.getBoundingClientRect().top;});};
    const jump=()=>{const card=document.querySelector('.question-card[data-state="open"]');card?.scrollIntoView({block:"center"});card?.querySelector("button")?.focus();if(card){card.classList.remove("flash");void card.offsetWidth;card.classList.add("flash");}};
    control.onclick=jump;
    const rail=document.getElementById("rail-actions");
    if(rail){const jumpButton=node("button","Oldest open question","rail-action");jumpButton.id="nav-oldest-question";jumpButton.type="button";jumpButton.append(node("kbd","⌘J","rail-kbd"));jumpButton.onclick=jump;rail.append(jumpButton);}
    document.addEventListener("keydown",e=>{if((e.metaKey||e.ctrlKey)&&!e.shiftKey&&e.code==="KeyJ"){e.preventDefault();jump();}});
    document.addEventListener("richos-questions-changed",refresh);
    document.addEventListener("scroll",refresh,true);
    const observer=new MutationObserver(records=>{if(records.some(r=>r.target!==control && !control.contains(r.target) && r.target!==recovery && !recovery.contains(r.target)))refresh();});
    observer.observe(document.getElementById("messages")||document.getElementById("transcript")||zone.parentElement,{childList:true,subtree:true});
    refresh();
  }
  window.RichQuestions={render};
  if(document.readyState==="loading")document.addEventListener("DOMContentLoaded",installControl);else installControl();
})();
