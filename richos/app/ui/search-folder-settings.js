// RichFinder's folder map. Paths are display data; all mutations use host-issued IDs.
(function () {
  "use strict";
  if (!window.RichSettings || !window.RichBridge) return;
  const invoke = (command, args) => window.RichBridge.invoke(command, args || {});
  const modes = [["off", "Off"], ["chosen", "Chosen folders"], ["all", "All eligible"]];
  const number = new Intl.NumberFormat();
  let preferences, status, savedRows = [], roots = [], epoch = 0, busy = false, timer;
  const branches = new Map();
  let sheet, tree, coverage, feedback, reindex;

  function element(tag, cls, text) {
    const node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text) node.textContent = text;
    return node;
  }

  function button(text, cls, action) {
    const node = element("button", cls, text);
    node.type = "button";
    node.addEventListener("click", action);
    return node;
  }

  function paintRow() {
    const row = document.getElementById("set-search-folders-state");
    if (!row || !preferences) return;
    const count = preferences.chosen.length;
    row.textContent = preferences.mode === "all" ? "All eligible folders"
      : preferences.mode === "off" || !count ? "Document search off"
      : number.format(count) + (count === 1 ? " search folder" : " search folders");
  }

  function close(back) {
    ++epoch;
    clearTimeout(timer);
    branches.clear();
    sheet.hidden = true;
    if (back) window.RichSettings.open();
    else document.getElementById("set-btn")?.focus();
  }

  function build() {
    if (sheet) return;
    sheet = element("div", "rf-folders-scrim");
    sheet.hidden = true;
    const page = element("section", "rf-folders-page");
    page.setAttribute("role", "dialog");
    page.setAttribute("aria-modal", "true");
    page.setAttribute("aria-labelledby", "rf-folders-title");
    const header = element("header", "rf-folders-header");
    const back = button("‹ Settings", "rf-folders-back", () => close(true));
    const title = element("h2", "", "Search folders");
    title.id = "rf-folders-title";
    const switcher = element("fieldset", "rf-folder-modes");
    switcher.appendChild(element("legend", "sr-only", "What RichOS can search"));
    for (const [value, label] of modes) {
      const option = element("label");
      const radio = element("input");
      radio.type = "radio";
      radio.name = "rf-folder-mode";
      radio.id = "rf-folder-mode-" + value;
      radio.value = value;
      radio.addEventListener("change", () => save("set_search_folder_mode", { mode: value }));
      option.append(radio, element("span", "", label));
      switcher.appendChild(option);
    }
    header.append(back, title, switcher);
    const columns = element("div", "rf-folders-columns");
    tree = element("section", "rf-folder-tree");
    tree.setAttribute("aria-label", "Folders on this Mac");
    coverage = element("aside", "rf-folder-coverage");
    columns.append(tree, coverage);
    const footer = element("footer", "rf-folders-footer");
    feedback = element("p", "", "Changes apply to the next search. No restart needed.");
    feedback.setAttribute("role", "status");
    reindex = button("Reindex", "", refreshIndex);
    footer.append(feedback, reindex);
    page.append(header, columns, footer);
    sheet.appendChild(page);
    document.body.appendChild(sheet);
    sheet.addEventListener("mousedown", (event) => { if (event.target === sheet) close(false); });
    document.addEventListener("keydown", (event) => {
      if (sheet.hidden || event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      close(false);
    }, true);
    sheet.addEventListener("keydown", (event) => {
      if (event.key !== "Tab") return;
      const available = [...page.querySelectorAll("button, input")].filter(node => !node.disabled && node.offsetParent !== null);
      if (event.shiftKey && document.activeElement === available[0]) { event.preventDefault(); available.at(-1)?.focus(); }
      else if (!event.shiftKey && document.activeElement === available.at(-1)) { event.preventDefault(); available[0]?.focus(); }
    });
  }

  function setFeedback(message) { feedback.textContent = message; }

  function renderRow(row, level) {
    const line = element("div", "rf-folder-row");
    line.style.setProperty("--rf-folder-level", level);
    const expanded = branches.get(row.id);
    if (row.state === "eligible") {
      const toggle = button(expanded ? "▾" : "›", "rf-folder-twisty", () => expand(row.id));
      toggle.id = "rf-folder-expand-" + row.id;
      toggle.setAttribute("aria-expanded", String(!!expanded));
      toggle.setAttribute("aria-label", (expanded ? "Collapse " : "Expand ") + row.name);
      line.appendChild(toggle);
    } else line.appendChild(element("span"));
    const choice = element("input");
    choice.type = "checkbox";
    choice.id = "rf-folder-choice-" + row.id;
    choice.setAttribute("aria-label", "Search " + row.name);
    const exact = preferences?.chosen.includes(row.path);
    const inherited = !exact && preferences?.chosen.some(parent => row.path.startsWith(parent + "/"));
    choice.checked = row.state === "eligible" && (preferences?.mode === "all" || preferences?.mode === "chosen" && (exact || inherited));
    choice.disabled = busy || row.state !== "eligible" || preferences?.mode === "all" || (preferences?.mode === "chosen" && inherited);
    choice.addEventListener("change", () => save("set_search_folder_selected", { folderId: row.id, selected: choice.checked }));
    const label = element("label", "rf-folder-name", row.name);
    label.htmlFor = choice.id;
    label.title = row.path;
    const note = row.state === "locked" ? "Never searched · " + row.reason
      : row.state === "unavailable" ? "Unavailable"
      : preferences?.mode === "chosen" && inherited ? "Included by parent folder" : "";
    line.append(choice, label, element("span", "rf-folder-row-note", note));
    tree.appendChild(line);
    if (!expanded) return;
    expanded.rows.forEach(child => renderRow(child, level + 1));
    if (expanded.next_offset != null) tree.appendChild(button("More folders", "", () => expand(row.id, expanded.next_offset)));
  }

  function paint() {
    if (sheet.hidden) return;
    const prior = document.activeElement?.id;
    for (const radio of sheet.querySelectorAll('[name="rf-folder-mode"]')) {
      radio.checked = radio.value === preferences?.mode;
      radio.disabled = busy || !preferences;
    }
    tree.replaceChildren();
    const caption = preferences?.mode === "all" ? "Everything eligible is included."
      : preferences?.mode === "off" ? "Tick a folder to start." : "Tick what to search.";
    tree.appendChild(element("p", "rf-folder-tree-caption", caption + " Locked rows are never searched."));
    roots.forEach(row => renderRow(row, 0));
    paintCoverage();
    if (prior && document.getElementById(prior) !== document.activeElement) document.getElementById(prior)?.focus();
    paintRow();
  }

  function paintCoverage() {
    coverage.replaceChildren();
    coverage.appendChild(element("h3", "", "Document search"));
    const folder = status?.folder;
    const meaning = status?.meaning;
    const off = preferences?.mode === "off";
    reindex.disabled = busy || !preferences || off;
    if (off || folder?.state === "none") {
      coverage.append(element("strong", "rf-folder-total", "Off"),
        element("p", "", "Nothing is searched until you choose. You can still find companies and chats."));
    } else if (folder?.state === "ready") {
      const completed = meaning?.state === "indexing" ? meaning.files_done : meaning?.state === "ready" ? meaning.files : 0;
      const building = meaning?.state === "indexing" ? meaning.files_total - completed : 0;
      coverage.appendChild(element("strong", "rf-folder-total", number.format(folder.files) + " documents"));
      coverage.appendChild(element("p", "", number.format(completed) + " searchable with RichFinder"));
      if (building > 0) coverage.appendChild(element("p", "", number.format(building) + " indexing · word search works now"));
      coverage.appendChild(element("p", "", number.format(folder.skipped + folder.links_skipped) + " skipped"));
      const known = completed + building + folder.skipped + folder.links_skipped;
      if (known) {
        const bar = element("div", "rf-folder-progress");
        bar.setAttribute("aria-hidden", "true");
        for (const [count, label] of [[completed, "done"], [building, "working"], [folder.skipped + folder.links_skipped, "skipped"]]) {
          if (!count) continue;
          const segment = element("span", "rf-folder-progress-" + label);
          segment.style.width = count / known * 100 + "%";
          bar.appendChild(segment);
        }
        coverage.appendChild(bar);
      }
    } else {
      coverage.appendChild(element("p", "", folder?.state === "error" ? folder.message : "Reading folders…"));
    }
    const unavailable = status?.unavailable_folders || 0;
    if (!off && unavailable) coverage.appendChild(element("p", "rf-folder-unavailable", number.format(unavailable) + " unavailable folders"));
    for (const row of savedRows.filter(item => preferences?.mode === "chosen" && item.state !== "eligible")) {
      const line = element("div", "rf-folder-unavailable");
      line.append(element("span", "", row.name + " · " + (row.reason || "Unavailable")),
        button("Remove", "", () => save("set_search_folder_selected", { folderId: row.id, selected: false })));
      coverage.appendChild(line);
    }
    coverage.append(element("h3", "", "Never searched"),
      element("p", "", "System folders, app data, dependencies and generated output stay excluded. Links are skipped."));
  }

  function scheduleStatus(token) {
    clearTimeout(timer);
    if (token !== epoch || sheet.hidden || preferences?.mode === "off" || status?.folder?.state === "none") return;
    const pending = status?.folder?.state === "loading" || ["starting", "indexing"].includes(status?.meaning?.state);
    if (!pending) return;
    timer = setTimeout(async () => {
      try {
        const next = await invoke("search_documents_status");
        if (token !== epoch || sheet.hidden || busy) return;
        status = next;
        paintCoverage();
        scheduleStatus(token);
      } catch (_error) { /* The visible controls can still load current status on demand. */ }
    }, 1200);
  }

  async function expand(id, offset) {
    const token = epoch;
    if (offset == null && branches.has(id)) { branches.delete(id); paint(); return; }
    try {
      const result = await invoke("search_folder_tree", { parent: id, offset: offset || 0 });
      if (token !== epoch || sheet.hidden) return;
      const earlier = offset ? branches.get(id)?.rows || [] : [];
      branches.set(id, { rows: earlier.concat(result.rows), next_offset: result.next_offset });
      paint();
    } catch (error) { if (token === epoch) setFeedback(String(error)); }
  }

  async function save(command, args) {
    if (busy) return;
    const token = epoch;
    const focused = document.activeElement?.id;
    busy = true;
    paint();
    try {
      const next = await invoke(command, args);
      const [nextStatus, selected] = await Promise.all([invoke("search_documents_status"), invoke("search_selected_folders")]);
      if (token !== epoch || sheet.hidden) return;
      preferences = next;
      status = nextStatus;
      savedRows = selected;
      setFeedback("Saved. Applies to the next search, no restart.");
      window.dispatchEvent(new Event("richfinder:folders-changed"));
    } catch (error) {
      if (token === epoch) setFeedback(String(error));
    } finally {
      busy = false;
      if (token === epoch) {
        paint();
        const replacement = focused && document.getElementById(focused);
        const target = replacement && !replacement.disabled ? replacement : sheet.querySelector(".rf-folders-back");
        target?.focus();
        scheduleStatus(token);
      }
    }
  }

  async function refreshIndex() {
    const token = epoch;
    try {
      const next = await invoke("reindex_search_folders");
      if (token !== epoch || sheet.hidden) return;
      status = next;
      setFeedback("Reindex started.");
      window.dispatchEvent(new Event("richfinder:folders-changed"));
      paint();
      scheduleStatus(token);
    } catch (error) { if (token === epoch) setFeedback(String(error)); }
  }

  async function open() {
    window.RichSettings.close(false);
    build();
    const token = ++epoch;
    sheet.hidden = false;
    sheet.querySelector(".rf-folders-back").focus();
    try {
      const [choice, current, page, selected] = await Promise.all([
        invoke("search_folder_preferences"), invoke("search_documents_status"),
        invoke("search_folder_tree"), invoke("search_selected_folders")
      ]);
      if (token !== epoch || sheet.hidden) return;
      preferences = choice;
      status = current;
      roots = page.rows;
      savedRows = selected;
      paint();
      scheduleStatus(token);
    } catch (error) { if (token === epoch) setFeedback(String(error)); }
  }

  function refreshRow() {
    if (sheet && !sheet.hidden) { paintRow(); return; }
    invoke("search_folder_preferences").then(next => { preferences = next; paintRow(); }, () => {});
  }
  window.RichFinderFolders = { open };
  window.RichSettings.registerSearchFolders({ open, paint: paintRow, onOpen: refreshRow });
  refreshRow();
})();
