"use strict";
// The app registry owns connections. Rendering never infers them from folder names.
(function () {
  const bridge = window.RichBridge;
  const sheet = document.createElement("div");
  sheet.id = "repositories-sheet"; sheet.className = "overlay"; sheet.hidden = true;
  sheet.setAttribute("role", "dialog"); sheet.setAttribute("aria-labelledby", "repositories-title");
  // ESCAPE CLOSES IT FROM ANYWHERE (CEO, 2026-09-17, item 1). The keydown listener below is
  // bound to the SHEET and so only fired while focus was inside it. `data-dismiss` names the
  // sheet's own Close button for main.js's document-level handler, which means Escape inherits
  // `close()`'s refusal to close mid-connect rather than working around it.
  sheet.setAttribute("data-dismiss", "control:#repository-close");
  sheet.innerHTML = `<div class="overlay-panel overlay-panel--compact">
    <h2 id="repositories-title" class="overlay-title">Connected project folders (repositories)</h2>
    <p class="overlay-note">Connect a project folder as this company’s home for Rich. Connecting it does not move or change its existing files.</p>
    <p class="overlay-note repository-guidance">One folder per company is usually enough. But in rare cases additional folders might be needed. Ask Rich if you’re unsure.</p>
    <label class="entity-add-label" for="repository-company">Company</label>
    <select id="repository-company" class="entity-add-input"></select>
    <ul id="repository-list"></ul>
    <label class="entity-add-label" for="repository-folder">Project folder location</label>
    <input id="repository-folder" class="entity-add-input" type="text" placeholder="Click and select folder" autocomplete="off" spellcheck="false">
    <p id="repository-message" class="overlay-note" role="status"></p>
    <div class="desk-card-actions"><button id="repository-connect" class="desk-btn desk-btn--confirm" type="button">Connect folder</button>
    <button id="repository-close" class="desk-btn" type="button">Close</button></div></div>`;
  document.body.appendChild(sheet);
  const field = id => sheet.querySelector("#" + id);
  let companies = [], busy = false, returnFocus, lastAdded = null;
  const LINK = '<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" aria-hidden="true"><path d="M8.5 11.5a3.5 3.5 0 0 0 5 0l2.5-2.5a3.5 3.5 0 0 0-5-5l-1 1"/><path d="M11.5 8.5a3.5 3.5 0 0 0-5 0L4 11a3.5 3.5 0 0 0 5 5l1-1"/></svg>';
  /// The other companies that have `path` connected: one folder may be connected in several
  /// (plan item 11, round 36). Names, in the order the sheet lists companies.
  function otherCompanies(path, id) {
    return companies.filter(c => c.id !== id && (c.repositories || []).includes(path)).map(c => c.name);
  }
  const joined = names => names.length > 1 ? names.slice(0, -1).join(", ") + " and " + names.at(-1) : names[0];
  function renderList() {
    const company = companies.find(c => c.id === field("repository-company").value);
    const list = field("repository-list"); list.replaceChildren();
    for (const path of company?.repositories || []) {
      // Break opportunities at `/`, `.`, `-`, `_` (audit D7, 2026-09-17) rather than
      // leaving `overflow-wrap: anywhere` alone to pick a point mid-word.
      const row = document.createElement("li"); row.appendChild(window.RichWrapPath(path)); row.style.overflowWrap = "anywhere"; list.appendChild(row);
      const also = otherCompanies(path, company.id);
      if (also.length) {
        const note = document.createElement("span"); note.className = "repository-also";
        note.innerHTML = LINK; const text = document.createElement("span"); text.append("Also in "); const bold = document.createElement("b"); bold.textContent = joined(also); text.appendChild(bold);
        note.appendChild(text); row.appendChild(note);
      }
      if (path === lastAdded) row.classList.add("repository-new");
    }
    if (!list.children.length) { const row = document.createElement("li"); row.textContent = "No project folders connected yet."; list.appendChild(row); }
    field("repository-connect").disabled = busy || !company;
    paintPick();
  }
  /// What the folder in the field means for the company chosen: a folder another company already
  /// has says so BEFORE anything is connected, and the button says what it will do.
  function paintPick() {
    sheet.querySelector(".repository-shared")?.remove();
    const button = field("repository-connect"); button.textContent = "Connect folder";
    const company = companies.find(c => c.id === field("repository-company").value);
    const path = field("repository-folder").value.trim();
    if (!company || !path || busy) return;
    if ((company.repositories || []).includes(path)) { field("repository-message").textContent = "Already connected to " + company.name + "."; return; }
    const also = otherCompanies(path, company.id);
    if (!also.length) return;
    field("repository-message").textContent = "";
    const box = document.createElement("div"); box.className = "repository-shared"; box.setAttribute("role", "status");
    const title = document.createElement("p"); title.className = "repository-shared-title"; title.innerHTML = LINK; title.append("Already connected to " + joined(also));
    const how = document.createElement("p"); how.textContent = "Connect it here too and Rich can work in it from " + company.name + " chats as well as " + joined(also) + " chats.";
    const same = document.createElement("p"); same.textContent = "Nothing in the folder moves and each company keeps its own chats and notes.";
    box.append(title, how, same); field("repository-message").before(box);
    button.textContent = "Connect to " + company.name + " too";
  }
  async function refresh(preferred) {
    const result = await bridge.invoke("repository_connections");
    companies = result.companies;
    const select = field("repository-company"); select.replaceChildren();
    const blank = document.createElement("option"); blank.value = ""; blank.textContent = "Choose a company"; select.appendChild(blank);
    for (const company of companies) { const option = document.createElement("option"); option.value = company.id; option.textContent = company.name; select.appendChild(option); }
    if (preferred && companies.some(c => c.id === preferred)) select.value = preferred;
    renderList();
  }
  async function open() {
    returnFocus = document.activeElement; sheet.hidden = false;
    field("repository-message").textContent = "Loading connections…";
    // PRE-SELECTED WHEREVER POSSIBLE (CEO feedback 2026-10-06, item 2b): the company he is
    // in, else the only company there is. He can still choose another one.
    const onScreen = window.RichCompanyOnScreen ? window.RichCompanyOnScreen() : null;
    try {
      await refresh(onScreen);
      if (!field("repository-company").value && companies.length === 1) { field("repository-company").value = companies[0].id; renderList(); }
      field("repository-message").textContent = companies.length ? "" : "Add a company before connecting a folder.";
    }
    catch (error) { companies = []; renderList(); field("repository-message").textContent = String(error); }
    // With the company already chosen, the next thing to do is the folder.
    field(field("repository-company").value ? "repository-folder" : "repository-company").focus();
  }
  function close() { if (busy) return; sheet.hidden = true; returnFocus?.focus(); }
  // A click in the folder field opens the system folder chooser (CEO feedback 2026-10-06,
  // item 2c): main.js's shared picker, the same one the first-run company sheet uses. Once
  // he has chosen, Connect folder is the next thing to press.
  window.RichFolderPicker.attach(field("repository-folder"), "Choose a project folder", () => field("repository-connect").focus());
  field("repository-company").addEventListener("change", () => { lastAdded = null; field("repository-message").textContent = ""; renderList(); });
  field("repository-folder").addEventListener("input", paintPick);
  field("repository-close").addEventListener("click", close);
  // Escape is main.js's alone, through `data-dismiss` above: it closes whatever is on top. A
  // handler of its own here closed this sheet even with the report panel open over it (review
  // rv-20261019T002816Z-ba3129ad-c627).
  sheet.addEventListener("keydown", event => {
    if (event.key === "Tab") {
      const targets = [...sheet.querySelectorAll("button, input, select")].filter(node => !node.disabled);
      if (event.shiftKey && document.activeElement === targets[0]) { event.preventDefault(); targets.at(-1).focus(); }
      else if (!event.shiftKey && document.activeElement === targets.at(-1)) { event.preventDefault(); targets[0].focus(); }
    }
  });
  field("repository-connect").addEventListener("click", async () => {
    const entityId = field("repository-company").value;
    if (busy || !entityId) return;
    busy = true; renderList(); field("repository-close").disabled = true;
    field("repository-message").textContent = "Checking this folder…";
    try {
      // Every connected folder gets Git (CEO, 2026-10-06); the message says when it was set up.
      const result = await bridge.invoke("connect_repository", {entityId, folder:field("repository-folder").value.trim()});
      const folder = field("repository-folder").value.trim();
      const also = otherCompanies(folder, entityId);
      lastAdded = folder; field("repository-folder").value = "";
      await refresh(entityId);
      const name = companies.find(c => c.id === entityId)?.name;
      field("repository-message").textContent = also.length ? "Folder connected. It is now in " + name + " and " + joined(also) + "." : result.repository.initialized ? "Folder connected. Git was set up to track its files." : "Folder connected.";
    } catch (error) { field("repository-message").textContent = String(error); }
    finally { busy = false; renderList(); field("repository-close").disabled = false; }
  });
  window.RichSettings.registerRepositories({open});
})();
