// THE UNIVERSAL SETTINGS BUTTON — controller.
//
// CEO ruling §15, and the invariant is the whole point:
//
//   1. "That little settings button is ALWAYS EVERYWHERE ON EVERY PAGE." It is global
//      chrome, not a feature of a surface. A screen without it is a defect. So this file
//      mounts it against `document.body` on load and nothing has to remember to include it.
//   2. "Even if the user can't change the light/dark mode, the very least that settings
//      button always provides (from every page and every screen) is a quick access to the
//      'Bust a bug' button." The theme row is the part that can be absent. The button and
//      its bug path never are.
//
// Structure and behaviour are deeply's, ported (see the CSS header for the source of record
// and the exact list of what was adapted). What is RichOS's own is the menu's CONTENTS and
// their order, which §15 fixes: the theme switch, then Text size "directly under the theme
// switch", then "directly under that, a Techy Mode toggle". Then the floor.
//
// STANDALONE ON PURPOSE. This file depends on `theme-boot.js` and on nothing else — not on
// main.js, not on the Tauri bridge, not on the shell existing. That is what lets the same
// component sit on the opening screen before the app has booted, and it is what will let it
// be lifted into the website the ruling also names. Durable persistence and the Techy
// toggle are OPTIONAL capabilities that a host registers; absent a host, the control still
// works and still remembers, through the mirror.
"use strict";

window.RichSettings = (function () {
  var T = window.RichTheme;

  // ---- optional host capabilities ------------------------------------------------------
  // Registered by main.js when the shell is up. Each is a plain object or null; every call
  // site below tolerates null, because the opening screen has no shell behind it yet.
  var durable = null; // { saveTheme(pref), saveScale(pct) }
  var techy = null; // { read(), write(on) }
  var oxford = null; // { read(), write(on) }: the Oxford comma in what Claude writes (CEO, 2026-10-19)
  var bug = null; // { open() } — what "Bust a bug" actually opens, when that exists
  var updates = null; // { render(container), onOpen() } — the update surface fills its own row
  var company = null; // { read(), write(id) } — which company this copy of Rich works for
  var repositories = null; // { open() }
  var account = null; // { open() }, the first account's sign-in, reached from its card (round 18)
  var accounts = null; // { open(), paint() }, "Claude accounts" for everyone (round 18, accounts.js)
  var dictation = null; // { open(), paint() }, "Dictation" for everyone once it is there (round 19, dictation.js)
  var quota = null; // { open() }, desktop technical settings only
  var memory = null; // { open() } — where his memory is kept (audit-7 row 13)
  var phonePairing = null; // { open() } — "Use Rich from your phone" (plan §4.1)
  var home = null; // { open() } — the home screen's company buttons: their labels and which show
  var codexReviews = null; // { render(container), onOpen() }, "Let Codex review your team's work" (round 20.2, codex-reviews.js)
  var spend = null; // { render(container), onOpen() }, "Token spend" (round 22.1, spend.js)
  var terminalSetup = null; // { open(), paint(), onOpen() }, "Terminal setup" (round 41 piece 1, terminal-setup.js)
  var gitName = null; // { open(), paint(), onOpen() }, "Your name in Git" (round 41 piece 2, git-name.js)
  // What the rail gear's Preferences held until round 27, owned by main.js: { parts: { model,
  // interrupt, "techy-hint", retention, name, splash } (nodes this file places), onOpen(),
  // onClose() } — onClose is where Rich's model, held while the panel is open, is saved.
  var preferences = null;

  var wrap = null;
  var menuEl = null;
  var btnEl = null;
  var toastTimer = null;

  // ---- icons, copied from the reference implementation ---------------------------------
  var SVG_NS = "http://www.w3.org/2000/svg";
  function icon(paths, size) {
    var svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "2");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    svg.setAttribute("aria-hidden", "true");
    for (var i = 0; i < paths.length; i++) {
      var spec = paths[i];
      var node = document.createElementNS(SVG_NS, spec[0]);
      for (var k in spec[1]) node.setAttribute(k, spec[1][k]);
      svg.appendChild(node);
    }
    if (size) {
      svg.setAttribute("width", size);
      svg.setAttribute("height", size);
    }
    return svg;
  }
  // The settings glyph is a SLIDER PAIR, not a cog — the reference is explicit about it.
  var ICON_SETTINGS = [
    ["path", { d: "M20 7h-9" }],
    ["path", { d: "M14 17H5" }],
    ["circle", { cx: "17", cy: "17", r: "3" }],
    ["circle", { cx: "7", cy: "7", r: "3" }],
  ];
  var ICON_SUN = [
    ["circle", { cx: "12", cy: "12", r: "4" }],
    ["path", { d: "M12 2v2" }],
    ["path", { d: "M12 20v2" }],
    ["path", { d: "m4.93 4.93 1.41 1.41" }],
    ["path", { d: "m17.66 17.66 1.41 1.41" }],
    ["path", { d: "M2 12h2" }],
    ["path", { d: "M20 12h2" }],
    ["path", { d: "m6.34 17.66-1.41 1.41" }],
    ["path", { d: "m19.07 4.93-1.41 1.41" }],
  ];
  var ICON_SYSTEM = [
    ["rect", { width: "20", height: "14", x: "2", y: "3", rx: "2" }],
    ["line", { x1: "8", x2: "16", y1: "21", y2: "21" }],
    ["line", { x1: "12", x2: "12", y1: "17", y2: "21" }],
  ];
  var ICON_MOON = [["path", { d: "M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" }]];
  /// **A CHEVRON, AND IT IS THE ANSWER TO URBAN'S G10** — *"Four action rows in the Settings
  /// panel are visually identical to its two headings ... `.bugbtn` and `.setmenu-title` are
  /// both `1rem/600/--ink`; the action rows need the icon `.bugbtn` was built for, or a
  /// chevron — pick one and say why."*
  ///
  /// **The two headings are "Settings" and "Updates"** (his frame 10, read off the frame rather
  /// than off the class list: `.setmenu-title` and `.update-title`, the latter taking its weight
  /// and its ink from `.set-name`). Under them sat Connected repositories, Account connection,
  /// Memory folder and Use Rich from your phone, at the same size, the same 600 and the same
  /// `--ink`, with nothing to say which of the six could be pressed.
  ///
  /// **A CHEVRON RATHER THAN FOUR ICONS, and the reason is that there is one fact to state.**
  /// All four rows do the same thing — each opens a modal sheet — and a chevron says exactly
  /// that, once, in a shape every operating system already uses for it. Four invented glyphs
  /// would make four new visual claims (which drawing means "account connection"?), would each
  /// have to be learned, and would still leave every row's LABEL starting at the same x as the
  /// two headings. `Bust a bug!` keeps its own icon and no chevron, which is now a distinction
  /// that carries information: it is the one row here that is not a disclosure.
  ///
  /// Trailing rather than leading, so the labels stay aligned with the other rows in the panel
  /// and the change adds nothing to the left edge.
  var ICON_CHEVRON = [["path", { d: "m9 18 6-6-6-6" }]];
  var ICON_BUG = [
    ["path", { d: "m8 2 1.88 1.88" }],
    ["path", { d: "M14.12 3.88 16 2" }],
    ["path", { d: "M9 7.13v-1a3.003 3.003 0 1 1 6 0v1" }],
    ["path", { d: "M12 20c-3.3 0-6-2.7-6-6v-3a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v3c0 3.3-2.7 6-6 6" }],
    ["path", { d: "M12 20v-9" }],
    ["path", { d: "M6.53 9C4.6 8.8 3 7.1 3 5" }],
    ["path", { d: "M6 13H2" }],
    ["path", { d: "M3 21c0-2.1 1.7-3.8 3.8-4" }],
    ["path", { d: "M20.97 5c0 2.1-1.6 3.8-3.5 4" }],
    ["path", { d: "M22 13h-4" }],
    ["path", { d: "M17.2 17c2.1.2 3.8 1.9 3.8 4" }],
  ];
  /// Request a feature's lightbulb (round 24), the same lucide glyph the mockup draws.
  var ICON_BULB = [
    ["path", { d: "M15 14c.2-1 .7-1.7 1.5-2.5 1-.9 1.5-2.2 1.5-3.5A6 6 0 0 0 6 8c0 1 .2 2.2 1.5 3.5.7.7 1.3 1.5 1.5 2.5" }],
    ["path", { d: "M9 18h6" }],
    ["path", { d: "M10 22h4" }],
  ];

  function elem(tag, cls, attrs) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    for (var k in attrs || {}) n.setAttribute(k, attrs[k]);
    return n;
  }

  // ---- the menu ------------------------------------------------------------------------

  function buildThemeRow() {
    var row = elem("div", "set-row");
    var label = elem("span", "set-name", { id: "set-theme-label" });
    label.textContent = "Theme";
    var seg = elem("div", "theme-seg", { role: "group", "aria-labelledby": "set-theme-label" });
    var opts = [
      ["light", "Light theme", "Light", ICON_SUN],
      ["system", "Follow the system", "System", ICON_SYSTEM],
      ["dark", "Dark theme", "Dark", ICON_MOON],
    ];
    for (var i = 0; i < opts.length; i++) {
      var b = elem("button", "theme-opt", {
        type: "button",
        "data-th": opts[i][0],
        "aria-label": opts[i][1],
        title: opts[i][2],
        "aria-pressed": "false",
      });
      b.appendChild(icon(opts[i][3]));
      seg.appendChild(b);
    }
    row.appendChild(label);
    row.appendChild(seg);
    return row;
  }

  function buildFontRow() {
    var row = elem("div", "set-row");
    var label = elem("span", "set-name", { id: "set-font-label" });
    label.textContent = "Text size";
    var seg = elem("div", "font-seg", { role: "group", "aria-labelledby": "set-font-label" });

    var down = elem("button", "font-opt", { type: "button", id: "font-down", "aria-label": "Smaller text", title: "Smaller (⌘−)" });
    down.textContent = "−";
    var val = elem("span", "font-val", { id: "font-val", "aria-live": "polite" });
    var up = elem("button", "font-opt", { type: "button", id: "font-up", "aria-label": "Larger text", title: "Larger (⌘+)" });
    up.textContent = "+";

    seg.appendChild(down);
    seg.appendChild(val);
    seg.appendChild(up);
    row.appendChild(label);
    row.appendChild(seg);
    return row;
  }

  /// THE OXFORD COMMA (CEO, 2026-10-19; round 33, `design/mockups/rounds/round-33/`). His words:
  /// "make the Oxford comma linter switchable in settings. Default setting: Oxford comma off.
  /// Explainer line underneath" and, for the heading, "In the off setting change this heading:
  /// "Oxford comma" to "Oxford comma: OFF" and in the on setting "Oxford comma: ON"". Drawn the
  /// way the Technical view row is (the same `.set-row`, `.set-name` and `.set-switch`), with his
  /// explainer in the panel's hint style under it. It decides only how what CLAUDE writes is
  /// drawn; the app's own text never carries the comma, whatever this says.
  function oxfordLabel(on) {
    return on ? "Oxford comma: ON" : "Oxford comma: OFF";
  }
  function buildOxfordRow() {
    var row = elem("div", "set-row", { id: "set-oxford-row" });
    var label = elem("span", "set-name", { id: "set-oxford-label" });
    label.textContent = oxfordLabel(!!oxford.read());
    var input = elem("input", "set-switch", {
      type: "checkbox",
      id: "set-oxford",
      "aria-labelledby": "set-oxford-label",
      "aria-describedby": "oxford-hint",
    });
    row.appendChild(label);
    row.appendChild(input);
    return row;
  }
  function buildOxfordHint() {
    var hint = elem("p", "popover-hint", { id: "oxford-hint" });
    hint.textContent =
      'Oxford comma AKA serial comma is what some style guides place before "and" and "or". ' +
      "RichOS recommends to keep that nuisance off.";
    return hint;
  }

  function buildTechyRow() {
    var row = elem("div", "set-row", { id: "set-techy-row" });
    var label = elem("span", "set-name", { id: "set-techy-label" });
    // **"Technical view", because the gear panel, the chip and the modal all already say
    // that** — Ray's candidate-.12 defect A. This row said "Techy Mode" while the gear panel
    // said "Technical view" with a "Show it" checkbox, and clicking THIS toggle opened a modal
    // headed "Turn off the technical view" (frame 13): one setting, two names, and the person
    // who turns one off is told they turned off the other. It is also the informal of the two,
    // in front of an audience of non-technical CEOs.
    //
    // The name is not a free choice: §9.1 and the modal's own heading fixed it. `techy` stays
    // in code identifiers, where nobody reads it.
    label.textContent = "Technical view";
    var input = elem("input", "set-switch", {
      type: "checkbox",
      id: "set-techy",
      "aria-labelledby": "set-techy-label",
    });
    // The hint under it (round 27, from Preferences) says which tier holds the view on.
    if (part("techy-hint")) input.setAttribute("aria-describedby", "techy-hint");
    row.appendChild(label);
    row.appendChild(input);
    return row;
  }

  /// UPDATES — an empty slot, and the host fills it (RICH-TODOs row 12).
  ///
  /// EVERY OTHER CAPABILITY HERE IS DATA (`read`/`write`) AND THIS ONE IS A SLOT, which is a
  /// deliberate exception rather than a shortcut. Techy Mode and the opening screen are one
  /// boolean each, so this file can own their whole control. The update surface is nine
  /// states, three buttons, a progress bar and a disclosure — owning it here would put a
  /// `RichBridge` dependency inside the one component whose stated contract is that it has
  /// none (see this file's header: it must keep working on the opening screen, before the
  /// shell exists, and be liftable into the website). So this file owns the ROW and knows
  /// nothing else; `updates.js` owns everything inside it and is the only thing that talks
  /// to the shell.
  ///
  /// It sits below the opening-screen row and above the floor: §15 fixes the order of the
  /// three rows it names and says nothing about this one, and "Bust a bug" stays last
  /// because the ruling's floor is that it is always there.
  /// WHICH COMPANY THIS COPY OF RICH WORKS FOR.
  ///
  /// IT IS HERE AND NOT IN THE RAIL'S PREFERENCES POPOVER, and the reason is measured. It
  /// was built there first — beneath the opening-screen switch, on the same "a thing he
  /// decides once goes last" argument — and it made that popover 821px tall in a 950px
  /// window. The popover is anchored `bottom: 46px; left: 10px` INSIDE the rail, so those
  /// extra ~195px land on top of the thread list: `techy.js` check 9 stopped being able to
  /// click a conversation while the popover was open, because a `.popover-option` was now
  /// over it, and the contrast walk of that surface measured 8 fewer nodes for the same
  /// reason. A settings row that covers the navigation is not a placement anyone wins.
  ///
  /// This menu is `position: fixed; top: 18px; right: 18px`, so it grows downward over the
  /// empty stage and covers nothing. It is also the surface §15 made universal — "always
  /// everywhere on every page" — and at `z-index: 300` it is above the correction and
  /// feedback overlays, which is exactly where this control has to be reachable from: the
  /// desks refuse to read anything while no company is chosen.
  ///
  /// A SELECT AND NOT A RADIO GROUP. This menu's shape is one row, one label, one control
  /// on the right, and the registry it lists is a table that grows. Four stacked radios
  /// would be the popover problem again, one row down.
  function buildCompanyRow() {
    var row = elem("div", "set-row", { id: "set-company-row" });
    var label = elem("span", "set-name", { id: "set-company-label" });
    label.textContent = "Company";
    row.appendChild(label);
    var state = (company && company.read && company.read()) || null;
    if (state && state.pinnedByEnvironment) {
      // NOT A DISABLED CONTROL. `RICHOS_ENTITY` decided this from outside the window and
      // nothing in here can move it, so the row states the answer and the reason instead of
      // showing a dead menu — the §21 rule for a state he cannot fix.
      var said = elem("span", "set-said", { id: "set-company-pinned" });
      said.textContent = state.chosen ? nameOf(state, state.chosen) : "Not recognized";
      // THE EXPLANATION IS A `title`, AND THAT IS DELIBERATE RATHER THAN LAZY: this menu's
      // rows are one label and one control wide, a second full-width line would be a new
      // row shape for one state, and a `title` is read by the affordance scrape and by a
      // screen reader alike. It is the state's own account of who owns it.
      said.title =
        "That was set when RichOS was started up, from outside this window, so it can't be " +
        "changed from in here. Whoever set RichOS up is the one who changes it.";
      row.appendChild(said);
      return row;
    }
    var sel = elem("select", "set-select chrome-select", {
      id: "set-company",
      "aria-labelledby": "set-company-label",
    });
    if (!state || !state.chosen) {
      // The unanswered state is a real option and is SELECTED, so the control never shows a
      // company he has not picked. Choosing it back is not offered: un-answering is not a
      // thing the store supports and a menu entry that silently did nothing would be worse.
      var none = elem("option", null, { value: "", disabled: "disabled", selected: "selected" });
      none.textContent = "Not set yet";
      sel.appendChild(none);
    }
    var options = (state && state.options) || [];
    for (var i = 0; i < options.length; i++) {
      var o = elem("option", null, { value: options[i].id });
      o.textContent = options[i].display_name;
      if (state && options[i].id === state.chosen) o.setAttribute("selected", "selected");
      sel.appendChild(o);
    }
    // Custom chevron (audit D7, 2026-09-17) — see `.chrome-select` in style.css for why this
    // is a real text glyph and not a background-image or an inline SVG. `selWrap`, not
    // `wrap`: this function's own local wrapper, distinct from the module-level settings
    // button wrapper of the same short name declared above.
    var selWrap = elem("span", "chrome-select-wrap");
    selWrap.appendChild(sel);
    var chevron = elem("span", "chrome-select-chevron", { "aria-hidden": "true", "data-contrast-role": "indicator" });
    chevron.textContent = "▾";
    selWrap.appendChild(chevron);
    row.appendChild(selWrap);
    return row;
  }

  function nameOf(state, id) {
    var options = (state && state.options) || [];
    for (var i = 0; i < options.length; i++) if (options[i].id === id) return options[i].display_name;
    return id;
  }

  /// THE HOME SCREEN'S COMPANY BUTTONS — one row, one label, one control, exactly like every
  /// other row here.
  ///
  /// The control is a BUTTON that opens a panel rather than the setting itself, and that is
  /// this menu's own rule rather than a shortcut: what it leads to is a table — one line per
  /// company, with a name field and a switch on each — and `buildCompanyRow` above already
  /// records why a table cannot live in here ("Four stacked radios would be the popover
  /// problem again, one row down"). Six companies with two controls each would be that
  /// problem twelve rows down.
  ///
  /// It sits directly under Company, because it is about the same six things.
  function buildHomeRow() {
    var row = elem("div", "set-row", { id: "set-home-row" });
    var label = elem("span", "set-name", { id: "set-home-label" });
    label.textContent = "Home screen";
    row.appendChild(label);
    var b = elem("button", "set-open", {
      type: "button",
      id: "set-home-open",
      "aria-labelledby": "set-home-label",
      "aria-haspopup": "dialog",
    });
    b.textContent = "Company buttons…";
    row.appendChild(b);
    return row;
  }

  function buildUpdatesRow() {
    var row = elem("div", "set-updates", { id: "set-updates" });
    return row;
  }

  function buildBugButton() {
    var b = elem("button", "bugbtn", { type: "button", id: "bug-btn" });
    b.appendChild(icon(ICON_BUG));
    b.appendChild(document.createTextNode("Bust a bug!"));
    return b;
  }

  /// **REQUEST A FEATURE, DIRECTLY UNDER BUST A BUG** (CEO 2026-10-11, round 24: "Under the
  /// "Bust a bug" button, I want to add a "Request a feature" button"). The same full-row action,
  /// with a lightbulb.
  function buildIdeaButton() {
    var b = elem("button", "bugbtn", { type: "button", id: "idea-btn" });
    b.appendChild(icon(ICON_BULB));
    b.appendChild(document.createTextNode("Request a feature"));
    return b;
  }

  /// **THE FLOOR**: Bust a bug, and Request a feature directly under it, in one box that stays at
  /// the panel's bottom edge while the rows above it scroll (style.css "THE FLOOR STAYS IN VIEW").
  function buildFloor() {
    var floor = elem("div", "set-floor", { id: "set-floor" });
    floor.appendChild(buildBugButton());
    floor.appendChild(buildIdeaButton());
    return floor;
  }

  /** **A ROW THAT OPENS A SHEET** — one builder for all four, which is Urban's G10 fix.
   *
   *  Four rows were written out four times, inline, and drifted: they had carried the class
   *  `set-bug`, which no rule in the whole stylesheet matches, for long enough to ship as
   *  unstyled native buttons (N2 in the 2026-09-17 dev-walk audit). One builder is why the
   *  chevron cannot be added to three of them.
   *
   *  The chevron is `aria-hidden` like every other icon here — it is drawn evidence that this
   *  row opens something, and the row's own label is what a screen reader needs.
   */
  /** A row that opens a sheet, and — when `stateId` is given — SAYS WHAT STATE THAT SHEET IS IN.
   *
   *  Ray's candidate-.16 defect D2: with port 8443 open and code 56WS37AH live, this panel read
   *  `Use Rich from your phone >` and nothing else — *"byte-for-byte the same row as with no code
   *  at all, and the same again while a phone is paired. A user who started a pairing, closed the
   *  sheet and walked away has no way to learn from Settings that their Mac is currently answering
   *  on its Tailscale name."*
   *
   *  A SECOND LINE, NOT AN INLINE SUFFIX. The panel is 267px wide (Ray's own measurement of it);
   *  "Use Rich from your phone" already fills that, so a state put beside the label would be
   *  truncated or would wrap the label. The two lines sit in one column and the chevron keeps its
   *  place at the row's right edge.
   *
   *  AND IT IS 16px, NOT 14px. CEO §15 puts 14px at "skippable"; whether a pairing code is live on
   *  this Mac right now is the opposite of skippable, so the line keeps the row's size and differs
   *  from the label in weight and ink alone. Both computed in `.set-row-state` (style.css).
   */
  function buildDisclosureRow(id, label, onOpen, stateId) {
    // A plain button, no `menuitem` role: since round 27 the panel is a dialog that holds a text
    // field, radios and switches, not a menu, and a menuitem outside a menu is announced wrongly.
    var b = elem("button", "bugbtn bugbtn--disclosure", { type: "button", id: id });
    if (stateId) {
      var col = elem("span", "set-row-text");
      var name = elem("span", "set-name");
      name.textContent = label;
      col.appendChild(name);
      col.appendChild(elem("span", "set-row-state", { id: stateId }));
      b.appendChild(col);
    } else {
      b.appendChild(document.createTextNode(label));
    }
    b.appendChild(icon(ICON_CHEVRON));
    b.addEventListener("click", function () {
      close();
      onOpen();
    });
    return b;
  }

  /** Build (or rebuild) the menu for the CURRENT capabilities.
   *
   *  The theme row is OMITTED, not disabled, when the theme is forced (§15's always-dark
   *  opening screen). A disabled control is a promise that it will work later; here it
   *  never will, and the ruling's floor is about what the button still DOES, not about
   *  showing a dead switch.
   *
   *  The Techy row appears only when a host has registered the capability, so a page with no
   *  shell behind it carries no dead toggle. It DOES appear on the opening screen, and that
   *  is correct rather than an oversight: what it moves is the GLOBAL default (§3.1 — "all"
   *  has to be one switch), which is a real preference whether or not a conversation happens
   *  to be open, and the shell is already live underneath the curtain.
   */
  function buildMenu() {
    // `data-dismiss` IS THE DECLARATION THE GLOBAL ESCAPE HANDLER READS (CEO, 2026-09-17,
    // item 1: "close any popup of any kind by simply tapping the escape key"). This menu has
    // had its own Escape since it was written — the listener at the foot of `mount()` — and
    // that one only fires because it is bound to `document`. The marker is what puts the menu
    // into `main.js`'s DOM-derived enumeration as well, so the two agree on what is open and
    // on which surface is topmost instead of racing. Both are idempotent (`close()` returns
    // early when the menu is already hidden), so the overlap costs nothing.
    //
    // ROUND 27 (richos-hq design/mockups/rounds/round-27, NOTES.md "Where every setting now
    // lives"): ONE Settings panel, reached one way. Everything the top-right menu and the rail
    // gear's Preferences held is here, in three columns and a band across the foot, and
    // Technical view has one switch. A DIALOG, not a menu: it holds a text field, radios and
    // switches, and its visible heading names it.
    var menu = elem("div", "setmenu", {
      id: "set-menu",
      role: "dialog",
      "aria-labelledby": "set-menu-title",
      "data-dismiss": "escape",
    });
    menu.hidden = true;
    // THE PANEL SAYS WHAT IT IS — Ray's candidate .11 defect 3.4: a person scanning for the
    // word finds it on the panel. Round 27 makes it the panel's heading (Newsreader, 26px), and
    // the dialog's accessible name through `aria-labelledby`.
    var head = elem("header", "sm-head");
    var title = elem("h2", "setmenu-title", { id: "set-menu-title" });
    title.textContent = "Settings";
    head.appendChild(title);
    menu.appendChild(head);

    var cols = elem("div", "sm-cols");
    var foot = elem("div", "sm-foot");

    // ---- COLUMN 1: how it looks -----------------------------------------------------------
    var look = [];
    if (!T.forcedDark()) look.push(buildThemeRow());
    look.push(buildFontRow()); // ...then Text size directly under it (§15)
    if (oxford) {
      // Round 33: directly under Text size, after a rule, with his explainer under it.
      look.push(divider());
      look.push(buildOxfordRow());
      look.push(buildOxfordHint());
    }
    if (techy) {
      // Technical view: ONE switch (it was in both panels), the Preferences hint under it, and
      // Claude Code quota under that, shown only while the view is on (`paint`).
      look.push(divider());
      look.push(buildTechyRow());
      var techyHint = part("techy-hint");
      if (techyHint) look.push(techyHint);
      if (quota) look.push(buildDisclosureRow("set-quota-open", "Claude Code quota", function () { quota.open(); }, "set-quota-state"));
    }
    var retention = part("retention");
    if (retention) {
      look.push(divider());
      look.push(retention);
    }
    addColumn(cols, "set-group-look", "How it looks", look);

    // ---- COLUMN 2: working with Rich ------------------------------------------------------
    var rich = [];
    var model = part("model");
    if (model) rich.push(model);
    var interrupt = part("interrupt");
    if (interrupt) {
      if (rich.length) rich.push(divider());
      rich.push(interrupt);
    }
    var doors = [];
    // DICTATION, FOR EVERYONE (round 19, state 5; dictation plan slice 2), with its one plain
    // line ("Off", "On. Tap F1 to talk", "Needs a permission"). `dictation.js` registers it only
    // once the app says dictation is there (DICTATION_READY).
    if (dictation) {
      doors.push(buildDisclosureRow("set-dictation-open", "RichScribe", function () {
        dictation.open();
      }, "set-dictation-state"));
    }
    if (phonePairing) {
      doors.push(buildDisclosureRow("set-phone-open", "Use Rich from your phone", function () {
        phonePairing.open();
      }, "set-phone-state"));
    }
    // TERMINAL SETUP (round 41 piece 1): last in the column, with the time it was last read.
    if (terminalSetup) {
      doors.push(buildDisclosureRow("set-terminal-open", "Terminal setup", function () {
        terminalSetup.open();
      }, "set-terminal-state"));
    }
    if (doors.length && rich.length) rich.push(divider());
    rich = rich.concat(doors);
    addColumn(cols, "set-group-rich", "Working with Rich", rich);

    // ---- COLUMN 3: your company and team --------------------------------------------------
    var team = [];
    if (company) team.push(buildCompanyRow()); // which company this copy is for
    if (home) team.push(buildHomeRow()); // ...and directly under it, the home screen's buttons
    // TOKEN SPEND (round 22.1; the CEO, §118: "The user should never need to "ask Rich" for
    // getting those token stats."). A slot its owner fills (`spend.js`): the row, this month's
    // figure, and the screen it opens are that file's.
    if (spend) {
      var spSlot = elem("div", "set-spend-slot", { id: "set-spend" });
      spend.render(spSlot);
      team.push(spSlot);
    }
    // "LET CODEX REVIEW YOUR TEAM'S WORK" (round 20.2; the CEO, ruling §114). A slot its owner
    // fills (`codex-reviews.js`): the row's states, its tooltip and its nudge are that file's.
    if (codexReviews) {
      var cxSlot = elem("div", "set-codex-slot", { id: "set-codex" });
      codexReviews.render(cxSlot);
      team.push(cxSlot);
    }
    var places = [];
    // CLAUDE ACCOUNTS, FOR EVERYONE (round 18; the CEO 2026-10-06, feedback item 9), with one
    // plain line under it and the week's mini bar, which `accounts.js` paints.
    if (accounts) {
      places.push(buildDisclosureRow("set-accounts-open", "Claude accounts", function () {
        accounts.open();
      }, "set-accounts-state"));
    }
    if (repositories) {
      places.push(buildDisclosureRow("set-repositories-open", "Connected folders (repositories)", function () {
        repositories.open();
      }));
    }
    // YOUR NAME IN GIT (round 41 piece 2): under the folders, with the name and email it puts on
    // the team's saved changes.
    if (gitName) {
      places.push(buildDisclosureRow("set-git-open", "Your name in Git", function () {
        gitName.open();
      }, "set-git-state"));
    }
    // The old account row stays only where "Claude accounts" is not registered (a page with no
    // accounts sheet behind it), so the first account's sign-in always has a door.
    if (account && !accounts) {
      places.push(buildDisclosureRow("set-account-open", "Account connection", function () {
        account.open();
      }));
    }
    // WHERE HIS MEMORY IS KEPT — audit-7 row 13: "Not now" is remembered, so the offer needs a
    // door that is not a question.
    if (memory) {
      places.push(buildDisclosureRow("set-memory-open", "Memory folder", function () {
        memory.open();
      }));
    }
    if (places.length && team.length && !codexReviews) team.push(divider());
    team = team.concat(places);
    if (updates) {
      // ...then what version this is, and what is waiting, last, after a divider.
      if (team.length) team.push(divider());
      team.push(buildUpdatesRow());
    }
    addColumn(cols, "set-group-team", "Your company and team", team);

    cols.setAttribute("data-n", String(cols.children.length));
    if (cols.children.length) menu.appendChild(cols);

    // ---- THE FOOT BAND: what is set once (name, splash screen), and the floor -------------
    var name = part("name");
    if (name) foot.appendChild(footCell(name));
    var splash = part("splash");
    if (splash) foot.appendChild(footCell(splash));
    // THE FLOOR, always last and always present (§15): Bust a bug, and Request a feature
    // directly under it, in the foot band's last cell. The band stays at the panel's bottom
    // edge while the columns above it scroll (style.css "THE FLOOR STAYS IN VIEW").
    foot.appendChild(footCell(buildFloor()));
    foot.setAttribute("data-n", String(foot.children.length));
    menu.appendChild(foot);
    return menu;
  }

  /// A thin rule between groups inside a column, the panel's `--line-soft`.
  function divider() {
    return elem("div", "sm-divider", { role: "presentation" });
  }

  /// One column: its 14px caps label (the rail's company-label tier), then its rows. A column
  /// with nothing registered in it is left out rather than drawn empty.
  function addColumn(cols, id, label, nodes) {
    if (!nodes.length) return;
    var col = elem("section", "sm-col", { "aria-labelledby": id });
    var lab = elem("p", "sm-group", { id: id });
    lab.textContent = label;
    col.appendChild(lab);
    for (var i = 0; i < nodes.length; i++) col.appendChild(nodes[i]);
    cols.appendChild(col);
  }

  function footCell(node) {
    var cell = elem("div", "sm-foot-cell");
    cell.appendChild(node);
    return cell;
  }

  /// A part of the panel that `main.js` owns (Rich's model, interruptions, stored output, the
  /// technical-view hint, the name field, the splash switch), or null when no host registered
  /// it. The SAME node every rebuild: it is moved, never copied, so its listeners and its
  /// state come with it.
  function part(key) {
    var parts = preferences && preferences.parts;
    return (parts && parts[key]) || null;
  }

  function wireHome(menu) {
    var b = menu.querySelector("#set-home-open");
    if (!b || !home || !home.open) return;
    b.addEventListener("click", function () {
      // The menu closes on its way out: the panel it opens is modal, and leaving a popover
      // hanging behind a dialog is two surfaces claiming the same corner of the screen.
      close();
      home.open();
    });
  }

  function wireCompany(menu) {
    var sel = menu.querySelector("#set-company");
    if (!sel || !company || !company.write) return;
    sel.addEventListener("change", function () {
      if (!sel.value) return;
      company.write(sel.value);
    });
  }

  function rebuild() {
    if (!wrap) return;
    var wasOpen = menuEl && !menuEl.hidden;
    // A rebuild while the panel is open (the company answer moving, a capability arriving)
    // must not cost him the field he is in: the parts main.js owns are moved into the new
    // panel, not copied, so the same node can take focus back.
    var focused = wasOpen && menuEl.contains(document.activeElement) ? document.activeElement : null;
    if (menuEl) menuEl.remove();
    menuEl = buildMenu();
    wrap.appendChild(menuEl);
    wireMenu();
    paint();
    if (wasOpen) {
      // Reopened as the SAME open: no fresh read of the held model choice, which would put
      // back what he picked a moment ago.
      open({ rebuilt: true });
      if (focused && menuEl.contains(focused)) focused.focus();
    }
  }

  // ---- state painting ------------------------------------------------------------------

  function paint() {
    if (!menuEl) return;
    var opts = menuEl.querySelectorAll(".theme-opt");
    for (var i = 0; i < opts.length; i++) {
      opts[i].setAttribute("aria-pressed", String(opts[i].dataset.th === T.theme()));
    }
    var val = menuEl.querySelector("#font-val");
    if (val) val.textContent = T.scale() + "%";
    var steps = T.STEPS;
    var down = menuEl.querySelector("#font-down");
    var up = menuEl.querySelector("#font-up");
    if (down) down.disabled = T.scale() <= steps[0];
    if (up) up.disabled = T.scale() >= steps[steps.length - 1];
    var sw = menuEl.querySelector("#set-techy");
    if (sw && techy) sw.checked = !!techy.read();
    var ox = menuEl.querySelector("#set-oxford");
    if (ox && oxford) {
      ox.checked = !!oxford.read();
      var oxLabel = menuEl.querySelector("#set-oxford-label");
      if (oxLabel) oxLabel.textContent = oxfordLabel(ox.checked);
    }
    var quotaRow = menuEl.querySelector("#set-quota-open");
    if (quotaRow) { quotaRow.hidden = !(techy && techy.read()); if (quota.paint) quota.paint(); }
    if (accounts && accounts.paint && menuEl.querySelector("#set-accounts-open")) accounts.paint();
    if (dictation && dictation.paint && menuEl.querySelector("#set-dictation-open")) dictation.paint();
    // The updates row is painted by its owner, because this file does not know what is in
    // it. Called on every paint so a rebuild (a forced-dark flip) never leaves an empty row.
    // The company row is REBUILT rather than repainted when its shape can change (chosen
    // vs pinned vs not-set-yet are three different rows), and repainted when only the
    // selection moved. `paint` is the cheap half; `rebuild` is called by `registerCompany`
    // and by the host when the answer changes.
    var comp = menuEl.querySelector("#set-company");
    if (comp && company && company.read) {
      var cs = company.read();
      if (cs && cs.chosen) comp.value = cs.chosen;
    }
    if (terminalSetup && terminalSetup.paint && menuEl.querySelector("#set-terminal-open")) terminalSetup.paint();
    if (gitName && gitName.paint && menuEl.querySelector("#set-git-open")) gitName.paint();
    var up2 = menuEl.querySelector("#set-updates");
    if (up2 && updates && updates.render) updates.render(up2);
    if (wrap) wrap.setAttribute("data-force-dark", String(T.forcedDark()));
    paintBug();
  }

  /// **BUST A BUG IS DISABLED WHILE A REPORT IS OPEN** (CEO, 2026-10-10: "WHY THE FUCK IS IT EVEN
  /// POSSIBLE TO CLICK THE FUCKING BUTTON AGAIN IF THE ACTION RELATED TO THAT IS ALREADY GOING
  /// ON???"): from the press until the report is sent, canceled or its panel closed, it is shown
  /// disabled, never hidden; `bug-report.js` answers `busy()` and calls `repaintBugReport` when that
  /// changes.
  function paintBug() {
    // Request a feature is the same exchange, one at a time across both (round 24): while a bug
    // report or an idea is open, both buttons are disabled.
    ["#bug-btn", "#idea-btn"].forEach(function (sel) {
      var b = menuEl && menuEl.querySelector(sel);
      if (b) b.disabled = !!(bug && bug.busy && bug.busy());
    });
  }

  // ---- open / close --------------------------------------------------------------------

  function open(opts) {
    opts = opts || {};
    if (!menuEl || !menuEl.hidden) return;
    paintBug();
    // What Preferences did as it opened (round 27 moved it here): show the saved model choice,
    // and re-read what a hand-edited config.json can change. Not on a rebuild's reopen.
    if (!opts.rebuilt && preferences && preferences.onOpen) preferences.onOpen();
    menuEl.hidden = false;
    // **IT OPENS AT ITS TOP, EVERY TIME** — Urban's G12, and he found it by being bitten by it:
    // the panel reopened at the scroll position it was closed at, his click landed a row off,
    // and he turned the splash screen on by accident. The panel is `max-height` +
    // `overflow-y: auto` (see `.setmenu`), so on a 1024x700 window — the app's own minimum —
    // it genuinely scrolls, and a popover that remembers a scroll offset it never showed the
    // user is a popover whose rows are not where they were the last time they looked.
    //
    // AFTER `hidden = false` AND NOT BEFORE: `.setmenu[hidden]` is `display: none !important`,
    // and an element with no box has no scroll to set — the assignment is silently dropped.
    menuEl.scrollTop = 0;
    wrap.classList.add("open");
    btnEl.setAttribute("aria-expanded", "true");
    // A claim about an update must not be stale on the screen that makes it: the automatic
    // launch check may have completed while this menu was closed. One read of an in-memory
    // struct, on an explicit click.
    if (updates && updates.onOpen) updates.onOpen();
    // AND THE SAME FOR THE PHONE, for the same reason and on the same explicit click (Ray's D2).
    // A code expires five minutes after it is issued and a phone can pair while this menu is
    // shut, so the row's state is read when the menu is opened rather than cached from whenever
    // the sheet was last looked at. It is one `phone_status` call on this Mac — no network — and
    // it is never on a timer: an open menu does not poll.
    if (phonePairing && phonePairing.onOpen) phonePairing.onOpen();
    // AND THE TWO ROUND 41 ROWS: the last read of the Terminal setup and the name in Git, which
    // the user can change in their own files while this menu is shut. Each is one local read.
    if (terminalSetup && terminalSetup.onOpen) terminalSetup.onOpen();
    if (gitName && gitName.onOpen) gitName.onOpen();
    // AND CODEX, for the same reason: it can be installed, signed in or out while this menu is
    // shut, so whether the switch can be turned on is read when the menu opens (one
    // `codex_reviews_status`: Codex's own `login status`, a few milliseconds, no network).
    if (codexReviews && codexReviews.onOpen) codexReviews.onOpen();
    // AND THE SPEND ROW'S FIGURE, taken when the menu opens, so the menu holds no ticking number
    // either (round 22.1): one `spend_view` for this month.
    if (spend && spend.onOpen) spend.onOpen();
    if (opts.focus) focusInside(opts.focus, opts.call);
  }

  /// Opened FOR one control (the nameplate's "Set your name", the header's Technical view
  /// chip): focus it, and with `call` light it for 1.6s (`@keyframes called`), so the eye finds
  /// the field in a panel this size.
  function focusInside(selector, call) {
    var target = menuEl.querySelector(selector);
    if (!target) return;
    target.focus();
    if (!call) return;
    target.classList.remove("is-called");
    void target.offsetWidth;
    target.classList.add("is-called");
  }

  function close(refocus) {
    if (!menuEl || menuEl.hidden) return;
    menuEl.hidden = true;
    wrap.classList.remove("open");
    btnEl.setAttribute("aria-expanded", "false");
    // Every close saves Rich's model, as every close of Preferences did (round 23): closing the
    // panel IS the confirmation, whichever way it closes.
    if (preferences && preferences.onClose) preferences.onClose();
    if (refocus) btnEl.focus();
  }

  // ---- actions -------------------------------------------------------------------------

  // NONE OF THESE THREE CALL `paint()`, AND THAT IS DELIBERATE.
  //
  // Every one of them moves state through `RichTheme`, which paints and then notifies its
  // subscribers — and this file is a subscriber (see `mount`). A local `paint()` here as
  // well is dead code that LOOKS load-bearing: deleting it changes nothing, so a future
  // reader cannot tell from the source whether the row updates because of this line or in
  // spite of it. Worse, it hides the real dependency. If the subscription breaks, the menu
  // stops tracking the keyboard, and check 10 is the thing that says so — which it can only
  // do if there is exactly one path.
  function applyTheme(pref) {
    if (T.setTheme(pref)) saveTheme(pref);
    close(true);
  }
  function saveTheme(pref) {
    if (durable && durable.saveTheme) {
      try {
        durable.saveTheme(pref);
      } catch (e) {
        /* the mirror already holds it; a failed durable write is not a failed preference */
      }
    }
  }
  function saveScale(pct) {
    if (durable && durable.saveScale) {
      try {
        durable.saveScale(pct);
      } catch (e) {
        /* as above */
      }
    }
  }

  function stepFont(delta) {
    var next = T.stepScale(delta);
    saveScale(next);
    return next;
  }
  function resetFont() {
    T.setScale(T.DEFAULT_SCALE);
    saveScale(T.DEFAULT_SCALE);
    return T.DEFAULT_SCALE;
  }

  function bustABug() {
    // A report already open: the button is disabled (`paintBug`), and a press that reaches here
    // anyway does nothing either.
    if (bug && bug.busy && bug.busy()) return;
    close(true);
    if (bug && bug.open) {
      bug.open();
      return;
    }
    // In the app, `bug-report.js` registers what this opens (round 21: Rich asks what went
    // wrong, writes it up, and files it once the user says Send). This notice is only for a
    // page with nothing registered — this component lifted onto the website, say — where it
    // must still not do nothing: a control that appears to do nothing is indistinguishable
    // from a broken one.
    toast(
      "Got it. The bug report starts from this exact screen, as it stands. " +
        "Nothing leaves this machine until you say so."
    );
  }

  /// Request a feature: Rich asks what the user would like RichOS to do (`bug-report.js`, round
  /// 24). A page with nothing registered still answers the press.
  function requestAFeature() {
    if (bug && bug.busy && bug.busy()) return;
    close(true);
    if (bug && bug.open) {
      bug.open("idea");
      return;
    }
    toast("Got it. Your idea starts from this exact screen. Nothing leaves this machine until you say so.");
  }

  function toast(text) {
    var t = document.getElementById("bug-toast");
    if (!t) {
      t = elem("div", "", { id: "bug-toast", role: "status", "aria-live": "polite" });
      document.body.appendChild(t);
    }
    t.textContent = text;
    t.hidden = false;
    t.classList.remove("is-leaving");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () {
      t.classList.add("is-leaving");
      setTimeout(function () {
        t.hidden = true;
        t.classList.remove("is-leaving");
      }, 450);
    }, 3400);
  }

  // ---- wiring --------------------------------------------------------------------------

  function wireMenu() {
    var opts = menuEl.querySelectorAll(".theme-opt");
    for (var i = 0; i < opts.length; i++) {
      (function (o) {
        o.addEventListener("click", function () {
          applyTheme(o.dataset.th);
        });
      })(opts[i]);
    }
    var down = menuEl.querySelector("#font-down");
    var up = menuEl.querySelector("#font-up");
    if (down) down.addEventListener("click", function () { stepFont(-1); });
    if (up) up.addEventListener("click", function () { stepFont(1); });

    var sw = menuEl.querySelector("#set-techy");
    if (sw) {
      sw.addEventListener("change", function () {
        if (techy && techy.write) techy.write(sw.checked);
      });
    }
    var ox = menuEl.querySelector("#set-oxford");
    if (ox) {
      ox.addEventListener("change", function () {
        if (oxford && oxford.write) oxford.write(ox.checked);
        paint();
      });
    }
    var bugEl = menuEl.querySelector("#bug-btn");
    if (bugEl) bugEl.addEventListener("click", bustABug);
    var ideaEl = menuEl.querySelector("#idea-btn");
    if (ideaEl) ideaEl.addEventListener("click", requestAFeature);
    wireCompany(menuEl);
    wireHome(menuEl);
  }

  function mount() {
    if (wrap) return;
    wrap = elem("div", "settings", { id: "settings" });
    btnEl = elem("button", "setbtn", {
      type: "button",
      id: "set-btn",
      "aria-label": "Settings",
      "aria-haspopup": "true",
      "aria-expanded": "false",
    });
    btnEl.appendChild(icon(ICON_SETTINGS));
    wrap.appendChild(btnEl);
    menuEl = buildMenu();
    wrap.appendChild(menuEl);
    document.body.appendChild(wrap);

    btnEl.addEventListener("click", function () {
      if (menuEl.hidden) open();
      else close(false);
    });
    wireMenu();

    document.addEventListener("pointerdown", function (e) {
      if (!e.target.closest || !e.target.closest(".settings")) close(false);
    });

    document.addEventListener("keydown", function (e) {
      // THE ACCELERATOR IS THE APP'S, NOT THE WEBVIEW'S (§15). `preventDefault` is what
      // takes it: without it WebKit runs its own page zoom, which scales the whole document
      // including fixed chrome and layout, does not persist, and is invisible to the Text
      // size row — two controls, two states, one of them a lie. With it, ⌘+/−/0 and the row
      // move the same single persisted number.
      if ((e.metaKey || e.ctrlKey) && !e.altKey) {
        if (e.key === "+" || e.key === "=") {
          e.preventDefault();
          stepFont(1);
          return;
        }
        if (e.key === "-" || e.key === "_") {
          e.preventDefault();
          stepFont(-1);
          return;
        }
        if (e.key === "0") {
          e.preventDefault();
          resetFont();
          return;
        }
      }
      if (e.key === "Escape") close(true);
    });

    // A forced-dark flip CHANGES THE MENU'S SHAPE (the theme row is omitted, not disabled),
    // so it needs a rebuild and not just a repaint. splash.js raises the clamp against
    // `RichTheme` directly — it is loaded in `<head>` and this file is not — so the
    // reaction has to live here rather than only in `RichSettings.forceDark`.
    var lastForced = T.forcedDark();
    T.onChange(function (s) {
      if (s.forcedDark !== lastForced) {
        lastForced = s.forcedDark;
        rebuild();
        return;
      }
      paint();
    });
    paint();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", mount);
  } else {
    mount();
  }

  return {
    mount: mount,
    close: close,

    /** OPEN THE MENU FROM OUTSIDE IT. Added 2026-09-04 for the update cue (`updates.js`),
     *  which sits in this component's own wrapper and whose whole job is to lead to the
     *  Updates row inside this menu.
     *
     *  It exists rather than the caller synthesizing a click on `#set-btn`, because that
     *  button TOGGLES: a synthetic click on an already-open menu closes it, so "open" and
     *  "click the opener" are not the same operation and only one of them is what the caller
     *  means. `open()` is idempotent and also runs the capability's `onOpen()`, which is what
     *  keeps a claim about an update from being stale on the screen that makes it. */
    openMenu: function (opts) {
      // `opts.focus`: a selector inside the panel to focus once it is open (round 27: the
      // nameplate opens it on the name field, the Technical view chip on its switch). An
      // already-open panel still takes the focus.
      if (menuEl && !menuEl.hidden) {
        if (opts && opts.focus) focusInside(opts.focus, opts.call);
        return;
      }
      open(opts);
    },
    /** ROUND 27: what the rail gear's Preferences held now lives in this panel. `host.parts` are
     *  nodes `main.js` owns and wires (`model`, `interrupt`, `techy-hint`, `retention`, `name`,
     *  `splash`); this file places them. `host.onOpen()` runs as the panel opens, `host.onClose()`
     *  on every close, which is where the held model choice is saved. */
    registerPreferences: function (host) { preferences = host || null; rebuild(); },
    paint: paint,
    stepFont: stepFont,
    resetFont: resetFont,
    toast: toast,

    /** The shell registers durable persistence once the bridge is up. Until then the
     *  mirror carries the preference on its own, which is why the control works on the
     *  opening screen and on a page that has no backend at all. */
    registerDurable: function (host) {
      durable = host || null;
    },

    /** §15: "directly under that, a Techy Mode toggle" — since round 27 the ONLY switch for
     *  the technical view. The host's read is the resolved state of the conversation on screen
     *  and its write asks where (`requestTechyToggle`); `RichSettings.paint()` after any change
     *  keeps the switch true to it. */
    registerTechy: function (host) {
      techy = host || null;
      rebuild();
    },

    /** THE OXFORD COMMA (round 33): `host.read()` is whether he keeps it, `host.write(on)` saves
     *  his choice. Registering is what makes the row exist, so a page with no shell behind it
     *  (the opening screen) carries no switch it cannot save. */
    registerOxford: function (host) {
      oxford = host || null;
      rebuild();
    },

    /** WHICH COMPANY THIS COPY OF RICH WORKS FOR. `host.read()` returns the shape
     *  `entity_choice` puts on the wire (`chosen`, `options`, `pinnedByEnvironment`) and
     *  `host.write(id)` is the same call the launch picker makes, so the two doors move one
     *  state. Registering is also what makes the row exist, so a page with no shell behind
     *  it — the opening screen — carries no company control it cannot deliver. */
    registerCompany: function (host) {
      company = host || null;
      rebuild();
    },

    /** THE HOME SCREEN'S COMPANY BUTTONS (CEO, 2026-09-01): what each one says, and which
     *  ones are on the screen at all. `host.open()` opens the panel that owns both; this
     *  file owns only the row that leads to it. Registering is what makes the row exist, so
     *  a page with no home screen behind it carries no door to a panel that is not there. */
    registerRepositories: function (host) { repositories = host || null; rebuild(); },
    registerAccount: function (host) { account = host || null; rebuild(); },
    /** "Claude accounts" for everyone (round 18). `host.open()` opens the accounts sheet,
     *  `host.paint()` writes the row's plain line and mini bar. */
    registerAccounts: function (host) { accounts = host || null; rebuild(); },
    /** "Dictation" (round 19, state 5): `host.open()` opens the Dictation sheet, `host.paint()`
     *  writes the row's plain line. `dictation.js` registers it only once the app says
     *  dictation is there, so a build without it carries no row. */
    registerDictation: function (host) { dictation = host || null; rebuild(); },
    /** "Let Codex review your team's work" (round 20.2): `host.render(container)` fills the row's
     *  slot below Technical view, `host.onOpen()` reads Codex's state when the menu opens.
     *  `codex-reviews.js` registers it once the app answers `codex_reviews_status`, so a page with
     *  no shell behind it carries no switch it cannot save. */
    registerCodexReviews: function (host) { codexReviews = host || null; rebuild(); },
    /** "Token spend" (round 22.1): `host.render(container)` fills the row's slot under Technical
     *  view, `host.onOpen()` takes this month's figure when the menu opens. `spend.js` registers it
     *  when the app is there, so a page with no shell behind it carries no row to a screen it
     *  cannot fill. */
    registerSpend: function (host) { spend = host || null; rebuild(); },
    /** The first account's sign-in (the setup sheet's account step), opened from its card in
     *  the accounts sheet now that the old row is gone. */
    openAccount: function () { close(); if (account && account.open) account.open(); },
    registerQuota: function (host) { quota = host || null; rebuild(); },
    /** WHERE HIS MEMORY IS KEPT. `host.open()` opens the same sheet the first-launch question
     *  uses, in whatever state `memory_status` reports at the press — which is why the row
     *  exists at all: "Not now" is remembered since audit-7 row 13, so an offer he put off has
     *  to be reachable without being asked again. Registering is what makes the row exist, so a
     *  page with no shell behind it carries no door to a sheet that is not there. */
    registerMemory: function (host) { memory = host || null; rebuild(); },

    /** The door to the pairing screen. Registered by `phone.js`, so a page with no shell behind
     *  it carries no row to a sheet that is not there — the same contract every other capability
     *  in this file has. */
    registerPhone: function (host) { phonePairing = host || null; rebuild(); },
    /** "Terminal setup" (round 41 piece 1) and "Your name in Git" (piece 2): `host.open()` opens
     *  the sheet, `host.paint()` writes the row's plain line, `host.onOpen()` reads it again as the
     *  menu opens. `terminal-setup.js` and `git-name.js` register them when the shell is there. */
    registerTerminalSetup: function (host) { terminalSetup = host || null; rebuild(); },
    registerGitName: function (host) { gitName = host || null; rebuild(); },

    registerHome: function (host) {
      home = host || null;
      rebuild();
    },

    /** Repaint the company row from outside, after the host's own answer moved. Its SHAPE
     *  can change (not-set-yet loses its placeholder entry once he answers), so this is a
     *  rebuild rather than a paint. */
    refreshCompany: function () {
      rebuild();
    },

    /** What "Bust a bug" opens: `bug-report.js` registers round 21's exchange with Rich. */
    registerBugReport: function (host) {
      bug = host || null;
      paintBug();
    },

    /** A report opened or ended: Bust a bug is disabled or pressable again (`paintBug`). */
    repaintBugReport: function () {
      paintBug();
    },

    /** THE UPDATE SURFACE (RICH-TODOs row 12), as a SLOT rather than as data — see
     *  `buildUpdatesRow` for why this one capability is shaped differently from the rest.
     *  `host.render(container)` fills the row; `host.onOpen()` is called when the menu is
     *  opened. Registering is also what makes the row exist, so the opening screen — which
     *  has no shell behind it yet — carries no dead update panel. */
    registerUpdates: function (host) {
      updates = host || null;
      rebuild();
    },

    /** Whether Technical view is on, as the menu's own switch reads it. Rows that keep a
     *  technical line behind it (`updates.js`'s update server) ask here. */
    technicalView: function () {
      return !!(techy && techy.read && techy.read());
    },

    /** Repaint the menu from outside. `updates.js` calls this when a `rich://update` event
     *  arrives while the menu is open, so a download's progress moves on screen rather than
     *  only in memory. */
    repaintUpdates: function () {
      paint();
    },

    /** THE PHONE ROW'S SECOND LINE (Ray's candidate-.16 defect D2). `phone.js` owns the words —
     *  it is the file that knows what a live code and a paired device are called — and this only
     *  puts them on the row. An empty string clears the line, which is the state where there is
     *  nothing to say: no code, nothing paired, and the row reads exactly as it always did.
     *
     *  A no-op when the menu has not been built or the row is not in it, because the capability
     *  is registered conditionally and the opening screen carries no phone row at all. */
    setPhoneState: function (line) {
      if (!menuEl) return;
      var node = menuEl.querySelector("#set-phone-state");
      if (!node) return;
      node.textContent = line || "";
      node.hidden = !line;
    },

    /** §15's one permanent exception, raised while the opening screen's curtain is up and
     *  dropped when it yields. The CEO's own preference is never written, so light mode is
     *  exactly where he left it the moment the shell appears. */
    forceDark: function (on) {
      T.forceDark(on);
      rebuild();
    },
  };
})();
