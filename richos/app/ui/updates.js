// THE UPDATE SURFACE — what the CEO sees when RichOS updates itself.
//
// RICH-TODOs row 12: before this there was no updater of any kind, so "automatically
// download and install whatever the user needs" rested on zero infrastructure. The Rust
// half is `app/src-tauri/src/updates.rs`; this is the half a person looks at.
//
// A SILENT UPDATER IS NOT ACCEPTABLE, and that is a constraint rather than a preference. An
// updater nobody can see is one that can be broken for three months without anyone learning
// it. So:
//
//   * RichOS checks by itself, three seconds after launch. This row always says WHEN it last
//     looked and WHAT it found — "never checked" is a state, and it is a different state
//     from "checked, and you are current".
//   * Every failure names WHICH failure it is in a sentence a non-technical CEO can act on,
//     with the vendor's own error text one click behind it. An error that hides its cause
//     turns a five-minute diagnosis into a week.
//   * A REFUSED SIGNATURE IS NOT OFFERED A RETRY. Every other failure gets "Try again",
//     because every other failure is plausibly transient. A signature failure means the
//     bytes on the wire are not the bytes we signed; retrying produces the same refusal, and
//     a button that invites someone to keep pressing until a security check passes is the
//     wrong shape of button. It gets an explanation instead.
//
// WHERE IT ANNOUNCES ITSELF (CEO ruling §26, placement paragraph, 2026-09-04). The row below
// was, until that ruling, the whole surface — and it lives inside a menu, so a waiting update
// reached only a CEO who happened to open one. His words: *"the user can't be bothered to
// hunt for some update button somewhere."* So there is now a CUE — see the block above
// `cue()` for the whole reasoning — a single small element that APPEARS beside the universal
// settings button when, and only when, something is waiting, and is absent from the document
// entirely the rest of the time. It announces and leads; the row still decides.
//
// WHERE THE ROW LIVES, AND WHY NOT IN THE RAIL'S PREFERENCES POPOVER. It renders into a row of
// the UNIVERSAL settings menu — the button §15 says is "ALWAYS EVERYWHERE ON EVERY PAGE" —
// through `RichSettings.registerUpdates`, the same optional-capability seam Techy Mode and
// the opening-screen switch use. The rail popover was tried first and MEASURED: it went from
// 581px to 761px, and at 761px it covers eight more rail rows, which turned `techy.js`
// check 9 red because a click meant for a thread landed on the popover. That popover is
// full. The universal menu is 160px in a 900px window, and it is on every screen — which is
// also the better home for a thing whose entire point is that the CEO can see it.
//
// TALKS ONLY TO `window.RichBridge`, never to `window.__TAURI__` — the same rule main.js
// follows, and the reason `mock.js` can drive all nine states in a plain browser.
//
// WHILE RICHOS IS WORKING, THE ACTION IS ABSENT AND THE FACT IS NOT (CEO, 2026-09-05).
//
// He found this himself, watching another app: *"while a live thread is currently running in
// the Codex app, I saw that blue update button appear. And I suspect that the current thread
// would die if I were to press that update button. I'm certainly not gonna try to find out."*
// The button he was afraid of was ours too — `updates.rs`'s `update_relaunch` was
// `app.restart()` and nothing else.
//
// His fix, and it is better than disabling: *"we don't even show the update button/
// notification if the RichOS app currently has active things running."* So while
// `view.busy`, the Install control, the Restart control, the settings-button mark and the
// actionable cue are all REMOVED — not dimmed, not dulled. **This product already made this
// decision once**: when `voice_readiness` says a machine cannot turn speech into words,
// `main.js` removes the talk button rather than disabling it, because the affordance appears
// only where it functions.
//
// **AND HIDING THE ACTION IS NOT HIDING THE FACT.** 26 rules that *"an update nobody
// discovers is the same as no updater at all, and it is worse than none because the
// machinery reports itself healthy"*, and RichOS is built to run workers for long stretches
// — so an update could otherwise stay invisible for days. His answer: *"instead of a regular
// update button, they'd get some other visual cue but not an actionable thing."* That is
// `waitingCue()` below: a small non-interactive indicator that appears in the same chrome,
// carries its own explanation, and cannot be pressed.
//
//   * IT IS FOCUSABLE AND ITS EXPLANATION IS REACHABLE WITHOUT A POINTER. A hover tooltip
//     gives a keyboard user nothing and a tap nothing. The note opens on FOCUS as well as
//     hover, and the whole sentence is the element's accessible name, so it is announced
//     even where the note is never painted. A fix for invisibility that is invisible to part
//     of the audience is not a fix.
//   * IT SAYS THE REASSURING THING, NOT THE PROCEDURAL ONE. "We are not showing the update
//     button" explains OUR behavior; what he wants to know is that an update is ready, that
//     it is waiting for his work, and that nothing will be interrupted. One calm sentence in
//     Rich's voice, which is the direct answer to "I'm certainly not gonna try to find out".
//   * IT DOES NOT LOOK LIKE THE BUTTON. The actionable cue is a wide filled gold pill
//     carrying a version; this is a small outlined circle with no label. If the two resembled
//     each other he would eventually press one expecting the other, which is the anxiety
//     being removed.
//
// THE ROW'S SENTENCES STAY TRUE UNDER BOTH UPDATE MODES. Nothing here promises that RichOS
// will install by itself when the work ends — that is 26's mode 1, which that ruling puts in
// a design session of its own and which `updates.rs` does not build. "I'll wait until
// everything has finished" is true of the product that exists (the control returns and he
// presses it) and stays true of the one that is coming (it installs itself), so no sentence
// here becomes a lie in either direction.
//
// THE RELAUNCH (CEO feedback 2026-10-06, item 1) is the one place a sentence here promises an
// act RichOS does by itself, and it is only said where `updates.rs` does it: an update THIS
// app downloaded (`restartsWhenIdle`) is relaunched into once nothing is running, after the
// `restarting` notice. While work runs, the row says it will restart when the work is done.
//
// CONTRAST: every pair this file switches between is listed with its computed ratio, both
// themes, in `style.css`'s `.update-*` block. Nothing here is exempt from the floor and
// nothing here claims to be.
"use strict";

window.RichUpdates = (function () {
  var host = null; // the container `settings-button.js` hands us, or null before first paint
  var nodes = null; // refs into that container
  var view = null; // the last UpdateView we were given

  // AN ACT IS IN FLIGHT — a check, an install, a restart. NOT a read.
  //
  // This flag disarms the row's controls, and until 2026-09-06 the menu's own re-read of
  // `update_state` raised it too. That is the whole of finding 1 in
  // `esc-20260906T085912Z-4005a6b5`: `update_state` is a pure read (`updates.rs:767` —
  // `refresh_work_verdict` then `snapshot`), it changes nothing and it commits the CEO to
  // nothing, and disarming the one control he is walking toward for the length of a round
  // trip is a cost with no purchase. Pressing Install against a view one round trip old is
  // refused by `updates.rs` itself if the machine has since become busy — the affordance was
  // never the only guard, and it is not the right one to spend here.
  var busy = false;

  /// Every `call` takes one, synchronously. An answer whose ticket is no longer the current
  /// one is DROPPED rather than applied, so a read that was in flight when the CEO pressed
  /// Install cannot repaint the row from the view it left with. Same shape as `main.js`'s
  /// `navTicket`, and for the same reason: two answers in flight is not a race to be shortened.
  var callTicket = 0;

  /// The CEO pressed the cue and his hand has not landed on the control yet. Discharged by
  /// `handToTheControl`, which runs at the end of every paint — never by a timer.
  var focusWanted = false;

  // ---- formatting ------------------------------------------------------------------------

  /// Bytes as a person reads them. No unit is ever printed for a number that has not been
  /// divided into it.
  function bytes(n) {
    if (typeof n !== "number" || !isFinite(n) || n < 0) return "";
    if (n >= 1048576) return (n / 1048576).toFixed(1) + " MB";
    if (n >= 1024) return Math.round(n / 1024) + " KB";
    return n + " bytes";
  }

  /// "just now" / "8 minutes ago" / a date. The point of this phrase is that the CEO can see
  /// the check HAPPENED, so an unusable timestamp says nothing rather than rendering a lie.
  function when(ms) {
    if (typeof ms !== "number" || !isFinite(ms) || ms <= 0) return "";
    var delta = Date.now() - ms;
    if (delta < 60000) return "just now";
    if (delta < 3600000) {
      var mins = Math.round(delta / 60000);
      return mins === 1 ? "1 minute ago" : mins + " minutes ago";
    }
    if (delta < 86400000) {
      var hrs = Math.round(delta / 3600000);
      return hrs === 1 ? "1 hour ago" : hrs + " hours ago";
    }
    try {
      return "on " + new Date(ms).toLocaleDateString();
    } catch (e) {
      return "";
    }
  }

  /// How long an update has been sitting here unable to be offered, in words, and ONLY once
  /// that is worth saying.
  ///
  /// It exists because the gate can hide the control for a long time: RichOS runs workers
  /// for stretches, so "ready for 3 days" is honest and a silent indefinite wait is not.
  /// Under a day it returns nothing at all — an update that arrived this morning does not
  /// need a duration, and a line that always appears is a line nobody reads. It is never a
  /// nag: it is one clause, on a surface he opened on purpose, and nothing anywhere pushes
  /// it at him.
  function readyFor(ms) {
    if (typeof ms !== "number" || !isFinite(ms) || ms <= 0) return "";
    var days = Math.floor((Date.now() - ms) / 86400000);
    if (days < 1) return "";
    return days === 1 ? "It has been ready for a day." : "It has been ready for " + days + " days.";
  }

  /// THE RELAUNCH (CEO feedback 2026-10-06, item 1): *"the app should automatically re-launch
  /// immediately after downloading the new version. Avoiding/preventing a re-launch is only
  /// meant for when there are workers running."* `updates.rs` relaunches into an update this
  /// app downloaded the moment its work gate is clear; these are the sentences for it.
  var RESTART_WHEN_DONE =
    "RichOS will restart into this update as soon as that work is done. Nothing will be interrupted.";
  var RESTART_IN_A_MOMENT = "Nothing is running, so RichOS will restart into it in a moment.";
  var RESTARTING_SUB =
    "Nothing is running, so RichOS is restarting to finish the update. Your window and conversation will be right where you left them.";

  /// The clauses the work gate adds, joined to whatever the state already said.
  ///
  /// `busyReason` is composed in Rust (`work_gate::decide`) and rendered VERBATIM — the count
  /// of running workers, the running turn, or the honest "could not tell". `unchecked` is
  /// what the gate could not establish, and it is printed for the reason the ruling gives:
  /// never claim to have waited for something you did not check.
  function gateClauses(v) {
    var out = [];
    if (v.busy && v.busyReason) out.push(String(v.busyReason));
    if (v.busy) {
      // MODE-PROOF ON PURPOSE. It does not say the button comes back and it does not say
      // RichOS will install by itself; each of those is true of exactly one of the two update
      // modes, and this sentence has to survive mode 1 landing without becoming a lie.
      //
      // The one exception is an update THIS app downloaded (`restartsWhenIdle`, CEO feedback
      // 2026-10-06 item 1): `updates.rs` relaunches into it when the work ends, so that is
      // what the row says.
      out.push(
        v.state === "ready" && v.restartsWhenIdle
          ? RESTART_WHEN_DONE
          : v.state === "ready"
            ? "This update will activate next time RichOS opens. Your work will continue uninterrupted."
            : "I'll wait until everything has finished, so nothing will be interrupted."
      );
    }
    var since = readyFor(v.readySince);
    if (since && (v.state === "available" || v.state === "ready")) out.push(since);
    for (var i = 0; i < (v.unchecked || []).length; i++) out.push(String(v.unchecked[i]));
    return out;
  }

  /// The gate's clauses, then whatever the state already said. With the gate quiet this
  /// returns the state's own sentence unchanged, so nothing about an idle app moves.
  function join(clauses, tail) {
    var all = clauses.slice();
    if (tail) all.push(tail);
    return all.join(" ");
  }

  /// Everything the row says, in one place, so a state cannot exist with no sentence
  /// attached to it. Ten arms for nine states plus the unknown one, and the unknown one
  /// reports itself as unknown — falling back to "up to date" would be the single worst
  /// answer this file could give.
  ///
  /// The work gate does NOT get an eleventh arm. It is not a state — an update that is
  /// available is available whether or not Rich is busy — so it composes onto the arms that
  /// already exist through `gateClauses`. A parallel `waiting` state would be a second place
  /// for "there is an update" to be written down, and the two would drift.
  function sentences(v) {
    var current = "RichOS " + (v.currentVersion || "");
    // THE DIRECTION COMES OFF THE PAYLOAD, never off a comparison of two version strings.
    // A rollback and an update reach this row in the same three states and differ only in
    // what they promise, and `updates.rs` is the one place that knows which it is holding.
    var back = !!v.readyIsRollback;
    switch (v.state) {
      case "unconfigured":
        return {
          headline: current,
          sub:
            "There is no update server yet, so RichOS cannot check for new versions. " +
            "Where updates are published has not been decided.",
        };
      case "idle":
        return { headline: current, sub: "RichOS has not checked for updates yet." };
      case "checking":
        return { headline: current, sub: "Checking for updates…" };
      case "upToDate":
        return {
          headline: current + " is up to date.",
          sub: "Checked " + (when(v.checkedAt) || "just now") + ".",
        };
      case "available":
        return {
          headline: "RichOS " + v.availableVersion + " is available.",
          sub: join(
            gateClauses(v),
            v.notes ? String(v.notes) : "You are running " + (v.currentVersion || "") + "."
          ),
        };
      case "downloading":
        return {
          headline: "Downloading RichOS " + v.availableVersion + "…",
          // No denominator, no percentage. A bar with an invented total is a lie that looks
          // like a measurement, so a server that sent no Content-Length gets a byte count.
          sub:
            typeof v.percent === "number" && v.totalBytes
              ? bytes(v.downloadedBytes) + " of " + bytes(v.totalBytes)
              : bytes(v.downloadedBytes) + " so far",
        };
      case "installing":
        return {
          headline: back
            ? "Checking and preparing RichOS " + v.availableVersion + "…"
            : "Checking and preparing the update…",
          sub: back
            ? "RichOS is confirming this earlier version was signed by us before preparing it for the next launch."
            : "RichOS is confirming this update was signed by us before preparing it for the next launch.",
        };
      case "ready":
        return {
          headline: back
            ? "RichOS will go back to " + v.availableVersion + " when you next open it."
            : v.restartsWhenIdle
              ? "RichOS " + v.availableVersion + " is ready."
              : "RichOS " + v.availableVersion + " is ready for the next launch.",
          // The gate's own words come FIRST when it is busy: "nothing is lost" is true of the
          // install and says nothing about the turn he is watching, which is the thing he is
          // actually asking about.
          sub: join(
            gateClauses(v),
            back
              ? "It will go back automatically next time RichOS opens. Your work will continue uninterrupted."
              : v.restartsWhenIdle
                ? v.busy
                  ? ""
                  : RESTART_IN_A_MOMENT
                : "It will activate automatically next time RichOS opens. Your work will continue uninterrupted."
          ),
        };
      case "restarting":
        // The one short notice before the relaunch. Seconds long, and the window comes back
        // on the same conversation.
        return {
          headline: "Restarting into RichOS " + v.availableVersion + "…",
          sub: RESTARTING_SUB,
        };
      case "failed":
        return {
          headline: v.failure ? v.failure.headline : "The update did not complete.",
          sub: "You are still running RichOS " + (v.currentVersion || "") + ".",
        };
      default:
        return { headline: current, sub: "RichOS cannot tell what state the update is in." };
    }
  }

  function isSignature(v) {
    return !!(v && v.failure && v.failure.kind === "signature");
  }

  // ---- the row ------------------------------------------------------------------------------

  function el(tag, cls, attrs) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    for (var k in attrs || {}) n.setAttribute(k, attrs[k]);
    return n;
  }

  /// "SEE WHAT'S NEW" (CEO, 2026-10-19; round 34, `design/mockups/rounds/round-34/` in
  /// richos-hq): the RichOS changelog, opened in his default browser and never in this window.
  /// It sits after "Check for updates" because this row is the one place that names the running
  /// version (it is the action line's last item; see `build`). The click goes to `open_external` by KEY (`src-tauri/src/opener.rs` allowlists it,
  /// as `main.js` does for the pricing link); the `href` is the same page, so the link is
  /// announced and disclosed as what it is. `tabindex="0"` because WebKit on a Mac leaves a
  /// plain link out of the Tab order. Enter on a focused link fires this same `click`.
  var CHANGELOG_KEY = "github.com/RichSpark/RichOS/blob/main/docs/CHANGELOG.md";

  /// What a screen reader hears is the `aria-label`: the visible words first (WCAG 2.5.3 "Label
  /// in Name", as the update cue does), then where it goes. Not a clipped `.sr-only` span: that
  /// span's text is still laid out at full width from the arrow onward, invisible but ~200px
  /// long, and where the link ends near its column's edge it ran past the column
  /// (`tests/rich-model.js` check 6, "nothing in Settings leaves its place").
  function changelogLink() {
    var a = el("a", "update-notes", {
      id: "update-notes",
      href: "https://" + CHANGELOG_KEY,
      target: "_blank",
      rel: "noopener noreferrer",
      tabindex: "0",
      "aria-label": "See what’s new (opens in your web browser)",
    });
    var text = el("span", "update-notes-text");
    text.textContent = "See what’s new";
    var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    var attrs = {
      class: "update-notes-glyph",
      viewBox: "0 0 24 24",
      fill: "none",
      stroke: "currentColor",
      "stroke-width": "2.25",
      "stroke-linecap": "round",
      "stroke-linejoin": "round",
      "aria-hidden": "true",
      "data-contrast-role": "indicator",
    };
    for (var k in attrs) svg.setAttribute(k, attrs[k]);
    ["M7 17 17 7", "M8.5 7H17v8.5"].forEach(function (d) {
      var p = document.createElementNS("http://www.w3.org/2000/svg", "path");
      p.setAttribute("d", d);
      svg.appendChild(p);
    });
    a.appendChild(text);
    a.appendChild(svg);
    a.addEventListener("click", function (e) {
      e.preventDefault();
      var b = window.RichBridge;
      if (b) b.invoke("open_external", { target: CHANGELOG_KEY }).catch(function () {});
    });
    return a;
  }

  /// Build the row's DOM once per container. `settings-button.js` throws the menu away and
  /// rebuilds it on a forced-dark flip, so "once" is per container, not once ever.
  function build(container) {
    container.textContent = "";

    // `.set-name` is the menu's own row-label class, and carrying it is not cosmetic:
    // `appearance.js`'s `menuRows` reads `.set-name` to assert the menu's order, so a row
    // without it would report its whole body as its name. It also inherits the menu's
    // typography instead of restating it.
    var title = el("p", "set-name update-title");
    title.textContent = "Updates";

    var line = el("p", "update-line", { id: "update-line", role: "status", "aria-live": "polite" });
    var sub = el("p", "update-sub", { id: "update-sub" });

    var progress = el("div", "update-progress", {
      id: "update-progress",
      role: "progressbar",
      "aria-valuemin": "0",
      "aria-valuemax": "100",
      "aria-label": "Downloading the update",
    });
    progress.hidden = true;
    var fill = el("div", "update-progress-fill", { id: "update-progress-fill" });
    progress.appendChild(fill);

    var actions = el("div", "update-actions");
    var check = el("button", "update-btn", { id: "update-check", type: "button" });
    var install = el("button", "update-btn update-btn--go", { id: "update-install", type: "button" });
    install.hidden = true;
    install.textContent = "Download update";
    // THE THIRD VERB, and deliberately the PLAIN button rather than the gold one. Going
    // back is the thing he reaches for when the newest build is bad; it is not the row's
    // recommended act, and two primary-weight controls side by side make neither one
    // primary. It introduces no new token at all, so it inherits the measured
    // `.update-btn` pairs (label 14.78 dark / 15.58 light, border 3.79 / 4.06).
    var back = el("button", "update-btn", { id: "update-back", type: "button" });
    back.hidden = true;
    actions.appendChild(check);
    actions.appendChild(install);
    actions.appendChild(back);
    // LAST on the line: in round 34's state (up to date) that is right after Check for updates,
    // as drawn; where Download update or Go back show, the row's own action keeps the lead.
    actions.appendChild(changelogLink());

    var why = el("button", "update-why", {
      id: "update-why",
      type: "button",
      "aria-expanded": "false",
      "aria-controls": "update-detail",
    });
    why.hidden = true;
    why.textContent = "Show the technical reason";
    var detail = el("pre", "update-detail", { id: "update-detail" });
    detail.hidden = true;
    var endpoint = el("p", "update-endpoint", { id: "update-endpoint" });
    endpoint.hidden = true;

    container.appendChild(title);
    container.appendChild(line);
    container.appendChild(sub);
    container.appendChild(progress);
    container.appendChild(actions);
    container.appendChild(why);
    container.appendChild(detail);
    container.appendChild(endpoint);

    check.addEventListener("click", function () {
      call("update_check");
    });
    install.addEventListener("click", function () {
      call("update_install");
    });
    back.addEventListener("click", function () {
      call("update_rollback");
    });
    why.addEventListener("click", function () {
      var open = detail.hidden === false;
      detail.hidden = open;
      why.setAttribute("aria-expanded", String(!open));
      why.textContent = open ? "Show the technical reason" : "Hide the technical reason";
    });

    nodes = {
      container: container,
      line: line,
      sub: sub,
      progress: progress,
      fill: fill,
      check: check,
      install: install,
      back: back,
      why: why,
      detail: detail,
      endpoint: endpoint,
    };
  }

  function paint() {
    if (!nodes || !view) return;
    var v = view;
    var s = sentences(v);

    nodes.container.setAttribute("data-update-state", v.state);
    // ON THE CONTAINER, so a stylesheet or a check can tell a hidden-because-working control
    // from a hidden-because-nothing-to-do one without parsing prose — the same reason
    // `data-update-failure` is there.
    if (v.busy) nodes.container.setAttribute("data-update-busy", "true");
    else nodes.container.removeAttribute("data-update-busy");
    // The failure KIND is on the container too, so a stylesheet or a test can tell a refused
    // signature from a dropped connection without parsing prose.
    if (v.state === "failed" && v.failure) {
      nodes.container.setAttribute("data-update-failure", v.failure.kind);
    } else {
      nodes.container.removeAttribute("data-update-failure");
    }

    nodes.line.textContent = s.headline;
    nodes.sub.textContent = s.sub;
    nodes.sub.hidden = !s.sub;

    var downloading = v.state === "downloading" || v.state === "installing";
    nodes.progress.hidden = !downloading;
    if (downloading) {
      var pct = typeof v.percent === "number" ? Math.max(0, Math.min(100, v.percent)) : null;
      if (pct === null) {
        // Indeterminate: the fill sweeps rather than claiming a position, and the accessible
        // value is REMOVED rather than guessed.
        nodes.progress.setAttribute("data-indeterminate", "true");
        nodes.progress.removeAttribute("aria-valuenow");
        nodes.fill.style.width = "100%";
      } else {
        nodes.progress.removeAttribute("data-indeterminate");
        nodes.progress.setAttribute("aria-valuenow", String(pct));
        nodes.fill.style.width = pct + "%";
      }
    }

    var canAct = !busy && v.state !== "checking" && !downloading;
    // "Check" disappears where it would be the wrong thing to press: with an update waiting
    // or installed, the next act is not another check.
    nodes.check.hidden = v.state === "available" || v.state === "ready" || v.state === "restarting";
    nodes.check.disabled = !canAct || v.state === "unconfigured";
    nodes.check.textContent = v.state === "failed" && !isSignature(v) ? "Try again" : "Check for updates";

    // ABSENT WHILE RICHOS IS WORKING, NOT DISABLED (CEO, 2026-09-05). A dimmed control still
    // invites the press and then refuses it, which is the anxiety being removed rather than a
    // milder version of it. `updates.rs` refuses these commands too — hiding an affordance is
    // the affordance half of a fix and never the whole of it, exactly as `main.js` hides the
    // talk button while `start_voice_capture` still refuses.
    nodes.install.hidden = v.state !== "available" || !!v.busy;
    nodes.install.disabled = !canAct;

    // THE WAY BACK IS OFFERED ONLY WHERE IT IS TRUE AND PRESSABLE.
    //
    // It names its destination, because "Go back" without a version is a control nobody
    // presses — and the version is this Mac's own history, not the server's opinion.
    // Absent while RichOS is working, for the same reason the Install control is; absent
    // once something is already prepared for the next launch, because that is what the
    // next launch will do and `updates.rs` refuses to stage a second thing over it; and
    // absent, rather than disabled, when there is no earlier version recorded at all.
    var backTo = v.rollbackVersion;
    nodes.back.hidden =
      !backTo ||
      !!v.busy ||
      v.state === "ready" ||
      v.state === "restarting" ||
      v.state === "checking" ||
      v.state === "unconfigured" ||
      downloading;
    nodes.back.disabled = !canAct;
    if (backTo) nodes.back.textContent = "Go back to " + backTo;


    var detail = v.failure ? v.failure.detail : "";
    nodes.why.hidden = !detail;
    if (!detail) {
      nodes.detail.hidden = true;
      nodes.why.setAttribute("aria-expanded", "false");
      nodes.why.textContent = "Show the technical reason";
    } else {
      nodes.detail.textContent = detail;
    }

    // Shown whenever it is NOT the shipped default, so a build pointed at a staging manifest
    // or at a test server says so on screen instead of in a log nobody opens: in Technical view
    // only (hunt 57, finding 19, 2026-10-15), because nightly 57 put "…/nightly/latest.json"
    // under "Request a feature" for a business owner who has no use for a file name. Technical
    // view repaints this row (`main.js` `renderTechySettings` → `RichSettings.paint`).
    var technical = !!(window.RichSettings && window.RichSettings.technicalView && window.RichSettings.technicalView());
    if (v.endpoint && !v.endpointIsPlaceholder && technical) {
      nodes.endpoint.hidden = false;
      // The URL is the one part of this line that hard-wraps (audit D7, 2026-09-17):
      // `window.RichWrapPath` (main.js) inserts break opportunities after `/`, `.`, `-`
      // and `_` instead of leaving the browser to pick a break point mid-word.
      nodes.endpoint.replaceChildren(
        document.createTextNode("Update server: "),
        window.RichWrapPath(v.endpoint)
      );
    } else {
      nodes.endpoint.hidden = true;
    }

    marker(v);
    cue(v);
    waitingCue(v);
    // LAST, because it reads what everything above just decided: whether there is a control,
    // whether it is offered, and whether pressing it would do anything.
    handToTheControl();
  }

  /// The mark on the settings button. It exists for exactly two states, something to install
  /// and something to restart into, and for nothing else: a button that is permanently marked
  /// is a button nobody reads. It is never the ONLY signal; the row says the same thing in
  /// words, and since 2026-09-04 the CUE below says it in words WITHOUT the menu being opened.
  ///
  /// The mark is kept rather than retired now that the cue exists, and the redundancy is
  /// deliberate: the mark is painted ON the settings button, so it survives anything that
  /// suppresses a sibling element, and it is the signal that says "the thing you want is
  /// inside THIS menu" once the cue has been pressed and the menu is open over it.
  function marker(v) {
    var btn = document.getElementById("set-btn");
    if (!btn) return;
    // THE MARK GOES WHILE HE IS WORKING, and it is not an afterthought to the button going.
    // A badge that says LOOK AT ME when he cannot safely act is a smaller version of the same
    // defect: it interrupts and it offers nothing. What replaces it is `waitingCue`, which
    // says the same fact and cannot be pressed.
    var want = v.busy
      ? ""
      : v.state === "available"
        ? "available"
        : v.state === "ready" || v.state === "restarting"
          ? "ready"
          : "";
    var dot = btn.querySelector(".update-mark");
    if (!want) {
      if (dot) dot.remove();
      btn.removeAttribute("data-update-mark");
      return;
    }
    if (!dot) {
      dot = el("span", "update-mark", { "aria-hidden": "true", id: "update-mark" });
      btn.appendChild(dot);
    }
    btn.setAttribute("data-update-mark", want);
  }

  // ---- the cue -------------------------------------------------------------------------
  //
  // THE ANNOUNCEMENT, IN CHROME THE CEO IS ALREADY LOOKING AT.
  //
  // CEO ruling §26, the placement paragraph added 2026-09-04, in his words: *"the user can't
  // be bothered to hunt for some update button somewhere."* Everything above this line
  // renders into a row of the universal settings menu, which means a waiting update was
  // discoverable only by someone who opened that menu — and nothing gave them a reason to.
  // An update nobody discovers is the same as no updater at all, and worse than none,
  // because the machinery reports itself healthy.
  //
  // THE PROPERTY THAT MATTERS IS THAT IT IS TRANSIENT, NOT THAT IT IS VISIBLE. With nothing
  // to install there is NO CONTROL AT ALL here — not a dimmed one, not an "up to date" state,
  // not a placeholder. The element is removed from the document. The signal is the
  // APPEARANCE of something that was not there a moment ago; a permanent control that
  // occasionally changes color is one an eye learns to stop seeing. (The reference the CEO
  // supplied is Codex, captured 2026-09-04 and kept privately in richos-hq: with no update
  // pending that slot in its bottom bar holds an ordinary help control, and the blue download
  // circle exists only while an update is waiting.)
  //
  // WHERE IT SITS, AND WHY THERE. Inside `.settings` — the wrapper of the universal settings
  // button, which §15 puts "ALWAYS EVERYWHERE ON EVERY PAGE" and which `settings-button.js`
  // mounts against `document.body` at a fixed position above every overlay. That single
  // choice buys every surface at once: the shell, an open thread, the home screen, the
  // opening screen, and anything added later. The rail's footer was the closer analogue to
  // the reference's bottom bar and was rejected on two measurements rather than on taste:
  // the rail is `display: none` under `body.rail-closed`, which is reachable at any width
  // below 1180px, so the affordance would be behind a toggle — the one thing the ruling
  // forbids — and the rail's default width is 300px, which cannot hold a label that names a
  // version without truncating it.
  //
  // THIS FILE DOES NOT FORK THE FLOW. The cue carries no state of its own, issues no command,
  // and writes no sentence: its label is `sentences(v).headline`, the SAME string the row
  // renders, so the two can never disagree. Pressing it opens the menu the row lives in and
  // puts the hand on the row's own control. Everything that decides anything — the install,
  // the restart, the refused signature that is never offered a retry — stays in the row.
  //
  // WHICH STATES, AND WHY NOT THE OTHERS. Exactly `available` and `ready`: the two states
  // where something is waiting on the CEO, which is the same pair `marker()` has always used
  // and is therefore one rule rather than two that agree most of the time.
  //   * `downloading` / `installing` — the operation is under way and nothing is waiting on
  //     him. It can only have been started from the row, which is open in front of him and
  //     which owns the progress bar. A second progress readout in the chrome would be the
  //     fork this file exists not to make.
  //   * `failed` — deliberately NOT announced here, and this is the one gap worth naming
  //     rather than hiding. The failure headlines `updates.rs` writes are whole sentences
  //     ("This download was not signed by RichOS, so it was not installed."); a pill wide
  //     enough to carry one is a banner, and §26 asks for the opposite of a banner. A failure
  //     is still stated in full in the row, exactly as it was before this cue existed, so
  //     nothing regressed — but a failure that happens with the menu shut is still quiet, and
  //     what failure should look like in chrome is one of the things the CEO's reference does
  //     not answer.
  //
  // MODE 1 (install with no click at all) IS NOT PRESUPPOSED ANYWHERE HERE. §26 makes it the
  // default and the CEO ruled on 2026-09-04 that hands-free updating waits for a design
  // session of its own. The cue announces and leads; it never starts anything.

  var SVG_NS = "http://www.w3.org/2000/svg";

  /// A stroked glyph in the pill. `stroke: currentColor` so it is the label's own ink and
  /// cannot drift from it — one value to prove, not two.
  function glyph(paths, cls) {
    var svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "2");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("class", cls || "update-cue-glyph");
    for (var i = 0; i < paths.length; i++) {
      var p = document.createElementNS(SVG_NS, "path");
      p.setAttribute("d", paths[i]);
      svg.appendChild(p);
    }
    return svg;
  }

  // An arrow coming down into a tray and a readiness check. Two glyphs because the two states
  // ask for two different things — but the WORDS carry the difference as well, so a
  // monochrome display or a reader who does not parse iconography loses nothing.
  var GLYPH = {
    available: ["M12 3.5v10.5", "M7.5 10 12 14.5 16.5 10", "M4.5 19.5h15"],
    ready: ["M5 12l4 4L19 6"],
    // A circling arrow: RichOS is about to come back on the new version.
    restarting: ["M19.5 12a7.5 7.5 0 1 1-2.2-5.3", "M19.5 4.5v4.2h-4.2"],
  };

  function cue(v) {
    var host = document.getElementById("settings");
    if (!host) return;
    var node = document.getElementById("update-cue");
    // The pill LEADS TO A CONTROL — pressing it opens the menu and puts the hand on the
    // row's own button. With the control removed there is nothing to lead to, so the pill is
    // removed as well rather than leading somewhere empty. `waitingCue` takes its place in
    // the same chrome, saying the same fact with nothing to press.
    //
    // `restarting` is here too: it is the short notice before the relaunch (CEO feedback
    // 2026-10-06, item 1), and the pill is where he sees it with the menu shut. Its label is
    // the row's own headline, so the two say the same thing.
    var want =
      v && ((!v.busy && (v.state === "available" || v.state === "ready")) || v.state === "restarting")
        ? v.state
        : "";

    // NO UPDATE, NO ELEMENT. Removed from the document, not hidden and not disabled.
    if (!want) {
      if (node) node.remove();
      return;
    }

    if (!node) {
      node = el("button", "update-cue", {
        id: "update-cue",
        type: "button",
        "aria-haspopup": "true",
      });
      node.addEventListener("click", openTheRow);
      // FIRST child, so the pill grows leftward and the settings button never moves. The
      // right edge of this cluster is fixed chrome; a control that shifts under the pointer
      // when an update arrives is a control that gets mis-clicked.
      host.insertBefore(node, host.firstChild);
    }

    if (node.getAttribute("data-update-cue") !== want) {
      node.textContent = "";
      node.appendChild(glyph(GLYPH[want]));
      node.appendChild(el("span", "update-cue-label", { id: "update-cue-label" }));
      node.setAttribute("data-update-cue", want);
    }
    // The ROW's own sentence, not a second one. "RichOS 0.1.2 is available." — the version is
    // named, which is the one place this is deliberately better than the reference, whose
    // button says only "Update" and never says which.
    var said =
      want === "ready"
        ? v.readyIsRollback
          ? "RichOS will go back to " + v.availableVersion + "."
          : "RichOS " + v.availableVersion + " is ready."
        : sentences(v).headline;
    // The restart notice leads nowhere new: the row says the same sentence and offers no act.
    if (want === "restarting") {
      node.lastChild.textContent = said;
      node.setAttribute("aria-label", said + " " + RESTARTING_SUB);
      return;
    }
    node.lastChild.textContent = said;
    // A BUTTON'S NAME SHOULD SAY WHAT PRESSING IT DOES, and the visible label is a statement
    // rather than an act — so the accessible name is the statement PLUS the act. It starts
    // with the visible text verbatim, which is what WCAG 2.5.3 "Label in Name" asks for: a
    // voice user who says the words on the screen still reaches this control.
    node.setAttribute("aria-label", said + " Open the update settings.");
  }

  /// Press the cue: open the menu the row lives in, and put the hand on the row's control.
  ///
  /// THE FOCUS USED TO BE DEFERRED WITH `setTimeout(..., 0)`, on a stated reasoning that was
  /// the defect: *"One turn of the task queue later the answer has landed and the button is
  /// live."* It has not. `RichSettings.open()` calls `onOpen()`, which re-read `update_state`
  /// and marked the surface busy for the length of a bridge ROUND TRIP, and `focus()` on a
  /// disabled button does nothing at all, silently.
  ///
  /// MEASURED 2026-09-06 on `041eee8` in WebKit, one uniform latency per bridge call, pressing
  /// the cue and sampling at the instant the product's own timer fired and again a second later:
  ///
  ///     lag     focus at t+0    install.disabled at t+0    focus once everything settled
  ///     0ms     update-install  false                      update-install
  ///     10ms    (body)          false                      (body)
  ///     120ms   (body)          true                       (body)
  ///     400ms   (body)          true                       (body)
  ///
  /// A Tauri IPC call is never below 10 ms, so §26's "one keystroke from the act" held in the
  /// in-page fixture and nowhere else — and the hand never arrived LATE either, it never
  /// arrived at all.
  ///
  /// TWO CHANGES, NEITHER OF THEM A SHORTER WINDOW. The re-read no longer disarms the control
  /// (see `busy` above), so there is nothing to wait out; and the focus is bound to the control
  /// BECOMING USABLE rather than to a turn of the task queue, so it is correct whether that is
  /// this instant or a slow round trip away.
  function openTheRow() {
    var s = window.RichSettings;
    if (!s || !s.openMenu) return;
    focusWanted = true;
    s.openMenu();
    // The common path ends here: the row is painted from the view the cue itself was drawn
    // from, the control is live, and the hand lands with no round trip in between.
    handToTheControl();
  }

  /// Put the hand on the control the cue leads to — the moment that is a true thing to do.
  ///
  /// Called at the end of every `paint()`, so it fires on the paint that makes the control
  /// usable rather than on a timer that hopes one already has. It discharges exactly once, and
  /// it discharges either way: a press that turns out to have nothing to land on gives the
  /// focus up rather than holding it and yanking the CEO's hand somewhere minutes later.
  function handToTheControl() {
    if (!focusWanted) return;
    // The cue's promise is about the menu it opens. If that menu is not open any more, the
    // promise is void — he closed it, or something else did.
    var menu = document.getElementById("set-menu");
    if (!menu || menu.hidden) {
      focusWanted = false;
      return;
    }
    // The row is not built yet, so this is not the paint that answers. `paint()` returns early
    // in that state, which is why this cannot be reached from one.
    if (!nodes) return;
    var go = document.getElementById("update-install");
    if (go && !go.hidden && !go.disabled) {
      focusWanted = false;
      go.focus();
      return;
    }
    // Nothing to press, and nothing in flight that could produce something to press: the row
    // is painted and it is offering no act. Give the intent up rather than carrying it.
    if (!busy) focusWanted = false;
  }

  // ---- the waiting cue -------------------------------------------------------------------
  //
  // THE FACT, WITH NOTHING TO PRESS.
  //
  // CEO, 2026-09-05, on what should stand where the update button is not being shown:
  // *"we could show something like an info icon or something that shows a tooltip/popup on
  // hover and explains that while there's an update available we're not currently showing
  // the update button because there's currently work running ... instead of a regular update
  // button, they'd get some other visual cue but not an actionable thing like the update
  // button."*
  //
  // WHY IT IS NOT A BUTTON, IN THE MARKUP AND NOT ONLY IN THE STYLING. It is a `span` with
  // `role="note"`, and it has no click handler at all. A `button` that did nothing would be
  // announced as a button to every screen reader and offered to every voice user, and the
  // one property this element must have is that pressing it is not a thing that exists.
  //
  // BUT IT IS FOCUSABLE (`tabindex="0"`), and that is the requirement a tooltip alone fails.
  // Hover reaches a pointer and nobody else. The note opens on FOCUS as well as on hover, so
  // a keyboard user meets it by tabbing; and the whole sentence is the element's
  // `aria-label`, so it is announced on focus even where the note is never painted at all. A
  // fix for invisibility that is itself invisible to part of the audience is not a fix.
  //
  // THE NOTE IS `aria-hidden`, DELIBERATELY. Its text is the same string as the accessible
  // name — one value, generated once — so leaving it in the accessibility tree would make a
  // screen reader read the sentence twice. Sighted and pointer users get the painted note;
  // everyone else gets the identical words from the name. They cannot disagree because
  // there is only one of them.
  //
  // WHICH STATES: `available` and `ready`, and only while `busy` — the exact complement of
  // `cue()` above, so at any moment the chrome holds the actionable pill, or this, or
  // nothing. Never both. `downloading` and `installing` are deliberately not here: the
  // bytes are already moving, nothing is waiting on him, and the row owns the progress bar.

  /// The sentence, generated ONCE and used for both the accessible name and the painted note.
  function waitingSaid(v) {
    var version = v.availableVersion ? "RichOS " + v.availableVersion : "An update";
    var back = !!v.readyIsRollback;
    var head =
      v.state === "ready"
        ? back
          ? "RichOS will go back to " + v.availableVersion + " when you next open it."
          : v.restartsWhenIdle
            ? version + " is ready."
            : version + " is ready for the next launch."
        : version + " is ready to install.";
    var why = v.busyReason ? " " + String(v.busyReason) : "";
    var tail =
      v.state === "ready" && v.restartsWhenIdle && !back
        ? " " + RESTART_WHEN_DONE
        : v.state === "ready"
        ? back
          ? " It will go back automatically next time RichOS opens. Your work will continue uninterrupted."
          : " It will activate automatically next time RichOS opens. Your work will continue uninterrupted."
        : " I'll wait until everything has finished, so nothing will be interrupted.";
    return head + why + tail;
  }

  // A dot inside a ring: the universal "there is something to know here", and deliberately
  // NOT the download arrow the actionable pill carries. Two glyphs that differ is how the eye
  // learns the two elements are different kinds of thing.
  var WAITING_GLYPH = ["M12 3.2a8.8 8.8 0 1 0 0 17.6 8.8 8.8 0 0 0 0-17.6z", "M12 10.8v5.4", "M12 7.9h0.01"];

  function waitingCue(v) {
    var host = document.getElementById("settings");
    if (!host) return;
    var node = document.getElementById("update-waiting");
    var want =
      v && v.busy && (v.state === "available" || v.state === "ready") ? v.state : "";

    // NOTHING WAITING, NO ELEMENT. Removed from the document, exactly as `cue()` does — the
    // signal is the APPEARANCE of something that was not there, and a permanent element that
    // occasionally changes is one an eye learns to stop seeing.
    if (!want) {
      if (node) node.remove();
      return;
    }

    if (!node) {
      node = el("span", "update-waiting", {
        id: "update-waiting",
        role: "note",
        tabindex: "0",
      });
      node.appendChild(glyph(WAITING_GLYPH, "update-waiting-glyph"));
      var note = el("span", "update-waiting-note", {
        id: "update-waiting-note",
        "aria-hidden": "true",
      });
      node.appendChild(note);
      var open = function () {
        node.setAttribute("data-open", "true");
      };
      var shut = function () {
        node.removeAttribute("data-open");
      };
      node.addEventListener("mouseenter", open);
      node.addEventListener("mouseleave", shut);
      node.addEventListener("focus", open);
      node.addEventListener("blur", shut);
      node.addEventListener("keydown", function (e) {
        // Escape closes the note without moving focus — the same key that dismisses every
        // other transient thing in this shell.
        if (e.key === "Escape") shut();
      });
      // FIRST child, for the reason `cue()` is: the right edge of this cluster is fixed
      // chrome, and a settings button that moved when an update arrived is one that gets
      // mis-clicked.
      host.insertBefore(node, host.firstChild);
    }

    node.setAttribute("data-update-waiting", want);
    var said = waitingSaid(v);
    node.setAttribute("aria-label", said);
    document.getElementById("update-waiting-note").textContent = said;
  }

  // ---- talking to the shell --------------------------------------------------------------

  function apply(next) {
    if (!next || typeof next !== "object") return;
    view = next;
    // THE RELAUNCH IS SECONDS AWAY: park the draft and the scroll position NOW rather than
    // on the composer's 400 ms debounce, so the window comes back exactly where he was.
    // `parkViewStateNow` is `main.js`'s, a global of that classic script.
    if (next.state === "restarting" && typeof window.parkViewStateNow === "function") {
      try {
        window.parkViewStateNow();
      } catch (_e) {
        /* parking is best effort; the relaunch still reopens the same conversation */
      }
    }
    paint();
  }

  /// `opts.read` marks a command that CHANGES NOTHING — `update_state`, and nothing else. It
  /// does not disarm the row, because there is nothing for the CEO to be protected from while
  /// it is in flight. See the block over `busy`.
  async function call(cmd, opts) {
    opts = opts || {};
    var b = window.RichBridge;
    if (!b) return null;
    var ticket = ++callTicket;
    if (!opts.read) {
      busy = true;
      paint();
    }
    try {
      var out = await b.invoke(cmd);
      // A NEWER CALL OWNS THE ROW. An `update_state` that was in flight when the CEO pressed
      // Install would otherwise land afterwards and repaint the row from the view it left
      // with — the install undone on screen while it is happening underneath.
      if (ticket !== callTicket) return out;
      apply(out);
      return out;
    } catch (e) {
      if (ticket !== callTicket) return null;
      // A command that throws is itself a failure state, reported as one rather than leaving
      // the row frozen on "Checking…".
      apply({
        state: "failed",
        currentVersion: view ? view.currentVersion : "",
        availableVersion: view ? view.availableVersion : null,
        notes: null,
        pubDate: null,
        downloadedBytes: 0,
        totalBytes: null,
        percent: null,
        endpoint: view ? view.endpoint : "",
        endpointIsPlaceholder: view ? view.endpointIsPlaceholder : true,
        checkedAt: view ? view.checkedAt : null,
        failure: {
          kind: "other",
          headline: "The update did not complete.",
          detail: String((e && e.message) || e),
        },
      });
      return null;
    } finally {
      // An ACT's completion always repaints — it owns `busy` and has just given it up. A READ
      // that has been overtaken does not: the newer call is the one painting now.
      if (!opts.read) {
        busy = false;
        paint();
      } else if (ticket === callTicket) {
        paint();
      }
    }
  }

  // ---- registration --------------------------------------------------------------------------

  var api = {
    /// Fill the row `settings-button.js` built for us. Called on every paint, so it is cheap
    /// on the common path and rebuilds only when the menu itself was rebuilt.
    render: function (container) {
      if (!nodes || nodes.container !== container) build(container);
      paint();
    },
    /// The menu was opened: re-read, because the automatic launch check may have completed
    /// while it was closed. A READ — it must not disarm the control the CEO opened the menu to
    /// press. See the block over `busy`.
    onOpen: function () {
      if (window.RichBridge) call("update_state", { read: true });
    },
    /// The current view, for the browser suites.
    state: function () {
      return view;
    },
  };

  function start() {
    var b = window.RichBridge;
    // NO BRIDGE, NO ROW. On a page with no shell behind it (the opening screen before boot,
    // or a design mock opened on its own) there is nothing that could answer, and a dead
    // update panel that says "not checked" forever is worse than no panel. The capability is
    // simply never registered, and `buildMenu` leaves the row out.
    if (!b) return;
    if (window.RichSettings && window.RichSettings.registerUpdates) {
      window.RichSettings.registerUpdates(api);
    }
    // Every transition the Rust side makes, including the automatic launch check, arrives
    // here — so the mark on the button appears whether or not the menu was ever opened.
    if (b.listen) {
      b.listen("rich://update", function (e) {
        apply(e.payload);
        if (window.RichSettings && window.RichSettings.repaintUpdates) {
          window.RichSettings.repaintUpdates();
        }
      });
    }
    // The first read, at start-up. A READ for the same reason the menu's is: the launch check
    // is the shell's business, not something the CEO is waiting on with his hand out.
    call("update_state", { read: true });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }

  return api;
})();
