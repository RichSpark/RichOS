"use strict";
// YOUR NAME IN GIT (richos-hq design round 41, piece 2), built as drawn.
//
// Commits carry the user's own name and email. When the team is about to save its first change
// in a connected folder and nothing names the user, the app asks ONCE, as a card in the
// conversation (the app's question card, the gold bar), and only that save waits for the answer
// (`richos-core`'s `git_name.rs`). The card: the name filled in, an email, Save or Use "RichOS"
// instead. Either answer is final: it is never asked again, and it is changed later in Settings →
// Your company and team → Your name in Git, under Connected folders. When the user's own Git setup
// names them, nothing is asked and Settings shows that name, read only.
//
// The shell's commands: `git_name_status`, `git_name_save`, `git_name_use_richos`,
// `git_name_open_file`; the card arrives as a `git_name` timeline item and on `rich://git-name`.
(function () {
  // main.js installs the native bridge after this module loads: resolve it at use time.
  const bridge = () => window.RichBridge;
  const SVG = (paths) =>
    '<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + paths + "</svg>";
  const ICON = {
    check: SVG('<path d="M20 6 9 17l-5-5"/>'),
    ok: SVG('<circle cx="12" cy="12" r="10"/><path d="m8.5 12.5 2.5 2.5 5-5.5"/>'),
    alert: SVG('<circle cx="12" cy="12" r="10"/><path d="M12 8v4"/><path d="M12 16h.01"/>'),
  };
  const QUESTION = "What name should go on your team’s saved work?";
  const HINT = "If you use GitHub, use your GitHub email so these changes show as yours.";
  const PLACEHOLDER = "you@yourcompany.com";
  const WRONG = {
    email: "That email isn’t complete. It needs an @ and a domain, like you@yourcompany.com.",
    no_email: "Add an email. Git needs one with the name, like you@yourcompany.com.",
  };
  const COULD_NOT_SAVE = "RichOS couldn’t save this, so nothing changed and the save is still waiting. Try again.";

  function el(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  /// The same rule the shell applies (`git_name::check`), so the reason is said at once.
  function problem(name, email) {
    if (!name.trim() || /[\n\r<>]/.test(name)) return "no_name";
    const e = email.trim();
    if (!e) return "no_email";
    const at = e.indexOf("@");
    const domain = at > 0 ? e.slice(at + 1) : "";
    const complete = at > 0 && domain.indexOf("@") < 0 && domain.split(".").length > 1 && domain.split(".").every((p) => p.length);
    if (!complete || /[\s<>]/.test(e)) return "email";
    return null;
  }

  /// A field's message: the GitHub hint, or the reason it cannot be saved, beside a mark.
  function setMessage(input, msg, why) {
    if (why) {
      input.setAttribute("aria-invalid", "true");
      msg.dataset.tone = "error";
      msg.innerHTML = ICON.alert;
      msg.append(el("span", null, WRONG[why]));
    } else {
      input.removeAttribute("aria-invalid");
      delete msg.dataset.tone;
      msg.textContent = HINT;
    }
  }

  /// The two fields, as the card and the sheet both draw them.
  function fields(prefix) {
    const box = el("div", "tg-fields");
    const nameLabel = el("label", null, "Name");
    nameLabel.htmlFor = prefix + "-name";
    const name = el("input", "tg-input");
    name.id = prefix + "-name";
    name.autocomplete = "name";
    name.spellcheck = false;
    const emailLabel = el("label", null, "Email");
    emailLabel.htmlFor = prefix + "-email";
    const email = el("input", "tg-input");
    email.id = prefix + "-email";
    email.type = "email";
    email.autocomplete = "email";
    email.spellcheck = false;
    email.placeholder = PLACEHOLDER;
    const msg = el("p", "tg-field-msg");
    msg.id = prefix + "-email-msg";
    email.setAttribute("aria-describedby", msg.id);
    box.append(nameLabel, name, emailLabel, email, msg);
    setMessage(email, msg, null);
    email.addEventListener("input", () => {
      if (email.getAttribute("aria-invalid")) setMessage(email, msg, null);
    });
    name.addEventListener("input", () => name.removeAttribute("aria-invalid"));
    return { box, name, email, msg };
  }

  /// Check the fields; say the reason and focus the field when they cannot be saved.
  function valid(f) {
    const why = problem(f.name.value, f.email.value);
    if (why === "no_name") {
      f.name.setAttribute("aria-invalid", "true");
      f.name.focus();
      return false;
    }
    f.name.removeAttribute("aria-invalid");
    if (why) {
      setMessage(f.email, f.msg, why);
      f.email.focus();
      return false;
    }
    setMessage(f.email, f.msg, null);
    return true;
  }

  // ---- THE CARD, in the conversation ---------------------------------------------------------
  // One card, asked once: what the user typed survives a redraw of the conversation.
  const draft = { name: null, email: "", why: null, failed: false, busy: false };

  function openSettingsOnRow() {
    if (window.RichSettings && window.RichSettings.openMenu) window.RichSettings.openMenu({ focus: "#set-git-open", call: true });
  }

  function render(item) {
    const g = item.gitName || {};
    const card = el("section", "question-card ask tg-ask");
    card.id = "git-name-card";
    card.setAttribute("role", "group");
    card.setAttribute("aria-labelledby", "git-name-q");
    const answered = g.state === "saved" || g.state === "richos";
    card.dataset.state = answered ? "answered" : "open";
    card.classList.toggle("is-answered", answered);
    card.append(el("div", "ask-set", "Asked once"));
    const q = el("p", "question-title ask-q", QUESTION);
    q.id = "git-name-q";
    card.append(q);

    if (answered) {
      const done = el("div", "tg-done");
      const check = el("span", "tg-done-check");
      check.setAttribute("aria-hidden", "true");
      check.dataset.contrastRole = "indicator";
      check.innerHTML = ICON.check;
      const body = el("div");
      body.append(el("div", "tg-done-main", g.state === "saved" ? "Your team’s changes now carry " + g.name + ", " + g.email : "Your team’s changes carry the name RichOS"));
      const sub = el("div", "tg-done-sub", "RichOS won’t ask again. ");
      const link = el("button", "tg-link", g.state === "saved" ? "Change it in Settings" : "Put your name on them in Settings");
      link.type = "button";
      link.id = "git-name-change";
      link.addEventListener("click", openSettingsOnRow);
      sub.append(link);
      body.append(sub);
      done.append(check, body);
      card.append(done);
    } else {
      const lede = el("p", "tg-ask-lede");
      if (g.folder) {
        lede.append("Your team is about to save its first change in ", el("b", null, g.folder), ". Git, which keeps the history of that folder, puts a name and an email on every change.");
      } else {
        lede.append("Your team is about to save its first change in one of your folders. Git, which keeps the history of that folder, puts a name and an email on every change.");
      }
      lede.append(" This Mac doesn’t have them set yet.");
      card.append(lede);
      const f = fields("git-name");
      f.name.value = draft.name != null ? draft.name : g.name || "";
      f.email.value = draft.email;
      if (draft.why) setMessage(f.email, f.msg, draft.why);
      card.append(f.box);
      if (draft.failed) {
        const p = el("div", "tg-problem");
        p.setAttribute("role", "alert");
        const mark = el("span");
        mark.dataset.contrastRole = "indicator";
        mark.innerHTML = ICON.alert;
        p.append(mark, el("p", null, COULD_NOT_SAVE));
        card.append(p);
      }
      const wait = el("div", "tg-ask-wait");
      const dot = el("span", "tg-dot");
      dot.setAttribute("aria-hidden", "true");
      dot.dataset.contrastRole = "indicator";
      wait.append(dot, "Your team keeps working. Only this save waits for your answer.");
      card.append(wait);
      const foot = el("div", "ask-foot");
      const skip = el("button", "tg-link", "Use “RichOS” instead");
      skip.type = "button";
      skip.id = "git-name-skip";
      const save = el("button", "ask-send", draft.busy ? "Saving…" : draft.failed ? "Try again" : "Save");
      save.type = "button";
      save.id = "git-name-save";
      save.disabled = draft.busy;
      foot.append(skip, save);
      card.append(foot);

      f.name.addEventListener("input", () => { draft.name = f.name.value; });
      f.email.addEventListener("input", () => { draft.email = f.email.value; draft.why = null; });
      const answer = async (command, args) => {
        if (draft.busy) return;
        draft.busy = true;
        save.disabled = true;
        save.textContent = "Saving…";
        try {
          await bridge().invoke(command, args);
          draft.busy = false;
          draft.failed = false;
          item.gitName = command === "git_name_save"
            ? Object.assign({}, g, { state: "saved", name: args.name.trim(), email: args.email.trim() })
            : Object.assign({}, g, { state: "richos" });
          redraw(card, item);
          paintRow();
        } catch (e) {
          draft.busy = false;
          if (WRONG[e] || e === "no_name") {
            draft.why = e === "no_name" ? null : e;
            redraw(card, item);
            return;
          }
          draft.failed = true;
          redraw(card, item);
        }
      };
      const send = () => {
        draft.name = f.name.value;
        draft.email = f.email.value;
        const why = problem(f.name.value, f.email.value);
        draft.why = why === "no_name" ? null : why;
        if (!valid(f)) return;
        answer("git_name_save", { name: f.name.value, email: f.email.value });
      };
      save.addEventListener("click", send);
      skip.addEventListener("click", () => answer("git_name_use_richos", {}));
      card.addEventListener("keydown", (e) => {
        if (e.key === "Enter" && e.target.matches(".tg-input")) { e.preventDefault(); send(); }
        if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); document.getElementById("input")?.focus(); }
      });
    }
    const row = el("article", "tl-rich question-row tg-ask-row");
    row.dataset.gitName = "card";
    row.append(card);
    return row;
  }

  /// Draw the card again in place, keeping the field the user is in.
  function redraw(card, item) {
    const row = card.closest(".tg-ask-row");
    if (!row) return;
    const focused = document.activeElement && card.contains(document.activeElement) ? document.activeElement.id : null;
    const fresh = render(item);
    row.replaceWith(fresh);
    const back = focused && document.getElementById(focused);
    if (back) back.focus();
    else if (item.gitName && item.gitName.state !== "open") fresh.querySelector("#git-name-change")?.focus();
    document.dispatchEvent(new Event("richos-questions-changed"));
  }

  // ---- THE SETTINGS ROW AND ITS SHEET ---------------------------------------------------------
  let status = null;
  let returnFocus = null;
  let opened = false;
  const sheet = el("div", "overlay tg-overlay");
  sheet.id = "git-name-sheet";
  sheet.hidden = true;
  sheet.setAttribute("role", "dialog");
  sheet.setAttribute("aria-labelledby", "gn-title");
  sheet.setAttribute("data-dismiss", "control:#gn-cancel,#gn-done");
  const panel = el("div", "tg-sheet tg-sheet--narrow");
  const title = el("h2", "tg-title", "Your name in Git");
  title.id = "gn-title";
  panel.append(title);
  // Changing it: the two fields.
  const edit = el("div");
  edit.id = "gn-edit";
  edit.append(el("p", "tg-lede", "Git puts this name and email on every change your team saves in your connected folders. Anyone who can see those folders, on GitHub for example, sees them."));
  const sf = fields("gn");
  edit.append(sf.box);
  const note = el("p", "tg-note tg-sheet-note");
  note.id = "gn-note";
  note.setAttribute("role", "status");
  note.setAttribute("aria-live", "polite");
  edit.append(note);
  const editFoot = el("div", "tg-foot tg-foot--end");
  const cancel = el("button", "tg-btn", "Cancel");
  cancel.type = "button";
  cancel.id = "gn-cancel";
  const saveSheet = el("button", "tg-btn tg-btn--gold", "Save");
  saveSheet.type = "button";
  saveSheet.id = "gn-save";
  editFoot.append(cancel, saveSheet);
  edit.append(editFoot);
  // From the user's own Git setup: read only.
  const own = el("div");
  own.id = "gn-own";
  own.hidden = true;
  const ownStatus = el("div", "tg-status");
  ownStatus.dataset.tone = "ok";
  const ownIcon = el("span", "tg-status-icon");
  ownIcon.dataset.contrastRole = "indicator";
  ownIcon.innerHTML = ICON.ok;
  const ownText = el("div");
  const ownMain = el("div", "tg-status-main");
  ownMain.id = "gn-own-main";
  const ownSub = el("div", "tg-status-sub");
  ownSub.id = "gn-own-sub";
  ownText.append(ownMain, ownSub);
  ownStatus.append(ownIcon, ownText, el("span"));
  const ownLede = el("p", "tg-lede tg-own-lede", "This is the name your Terminal’s Git uses, so your team uses it too, as it is. To change it, change it there: your team’s next saved change uses the new one.");
  const ownFoot = el("div", "tg-foot");
  const openFile = el("button", "tg-link", "Open the file");
  openFile.type = "button";
  openFile.id = "gn-open-file";
  const finder = el("button", "tg-link", "Show in Finder");
  finder.type = "button";
  finder.id = "gn-finder";
  const done = el("button", "tg-btn", "Done");
  done.type = "button";
  done.id = "gn-done";
  const spacer = el("span", "tg-note");
  spacer.style.flex = "1";
  ownFoot.append(spacer, openFile, finder, done);
  own.append(ownStatus, ownLede, ownFoot);
  panel.append(edit, own);
  sheet.append(panel);
  document.body.appendChild(sheet);

  function rowLine(s) {
    if (!s) return "";
    if (s.source === "git") return s.name + ", from your Git setup";
    if (s.choice === "own") return s.name + " · " + s.email;
    if (s.choice === "richos") return "RichOS, until you set yours";
    return "Not set yet";
  }
  function paintRow() {
    const line = document.getElementById("set-git-state");
    if (line) line.textContent = rowLine(status);
  }
  async function load() {
    try {
      status = await bridge().invoke("git_name_status");
    } catch (e) {
      /* the row keeps what it said */
    }
    paintRow();
    return status;
  }
  function paintSheet(saved) {
    const fromGit = status && status.source === "git";
    edit.hidden = !!fromGit;
    own.hidden = !fromGit;
    if (fromGit) {
      ownMain.textContent = status.name + " · " + status.email;
      ownSub.replaceChildren("From your own Git setup, ", Object.assign(el("span", "tg-path", status.file), {}));
      return;
    }
    if (!saved) {
      sf.name.value = (status && (status.choice === "own" ? status.name : status.suggested)) || "";
      sf.email.value = status && status.choice === "own" ? status.email : "";
      setMessage(sf.email, sf.msg, null);
      sf.name.removeAttribute("aria-invalid");
    }
    note.replaceChildren();
    if (saved) {
      const line = el("span", "tg-saved");
      line.innerHTML = ICON.check;
      line.append("Saved. Your team’s next saved change carries it.");
      note.append(line);
    } else {
      note.textContent = "Kept in RichOS’s own settings. Your Terminal’s Git setup is not changed.";
    }
    cancel.textContent = saved ? "Done" : "Cancel";
  }
  async function open() {
    returnFocus = document.activeElement;
    paintSheet(false);
    opened = true;
    sheet.hidden = false;
    await load();
    paintSheet(false);
    setTimeout(() => (status && status.source === "git" ? done : sf.email).focus(), 30);
  }
  function close() {
    if (!opened) return;
    opened = false;
    sheet.hidden = true;
    // Back to what opened it when that is still on screen; the Settings row is not (the panel
    // closes as the sheet opens), so then to the Settings button.
    const back = returnFocus && returnFocus !== document.body && returnFocus.isConnected && returnFocus.getClientRects().length ? returnFocus : document.getElementById("set-btn");
    if (back && back.focus) back.focus();
  }
  async function saveFromSheet() {
    if (!valid(sf)) return;
    try {
      status = await bridge().invoke("git_name_save", { name: sf.name.value, email: sf.email.value });
      paintRow();
      paintSheet(true);
    } catch (e) {
      if (WRONG[e]) { setMessage(sf.email, sf.msg, e); return; }
      if (e === "no_name") { sf.name.setAttribute("aria-invalid", "true"); return; }
      note.replaceChildren(el("span", null, COULD_NOT_SAVE));
    }
  }
  // However the sheet is hidden (Done, Escape, a click outside, or the window's own Escape
  // handler), the focus comes back out of it.
  new MutationObserver(() => { if (sheet.hidden && opened) close(); }).observe(sheet, { attributes: true, attributeFilter: ["hidden"] });
  cancel.addEventListener("click", close);
  done.addEventListener("click", close);
  saveSheet.addEventListener("click", saveFromSheet);
  panel.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && e.target.matches(".tg-input")) { e.preventDefault(); saveFromSheet(); }
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); }
  });
  sheet.addEventListener("pointerdown", (e) => { if (e.target === sheet) close(); });
  // A file that cannot be opened says so in the foot, beside the two links that try again.
  const openOwn = (reveal) => {
    spacer.textContent = "";
    bridge().invoke("git_name_open_file", { reveal }).catch((e) => { spacer.textContent = String(e); });
  };
  openFile.addEventListener("click", () => openOwn(false));
  finder.addEventListener("click", () => openOwn(true));

  if (window.RichSettings && window.RichSettings.registerGitName) {
    window.RichSettings.registerGitName({ open, paint: paintRow, onOpen: load });
  }
  window.RichGitName = { render, open, close };
})();
