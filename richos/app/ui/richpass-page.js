"use strict";
// THE RICHPASS PAGE (design round 58, version A "Hairline", its "Document" page; the CEO's pick,
// 2026-11-06), reached from the navigation rail's RichPass item.
//
// The title mark and page copy follow round 65.1 in index.html.
//
// NOTHING NATIVE IS NEW. Every press is a command `richpass.js` already uses
// (`richpass_status`, `richpass_save`, `richpass_choose`, `richpass_check`, `richpass_show`,
// `richpass_stop`), and every protection question is the native consent dialog behind them.
//
//   Download RichPass saves the bundled verified copy through the native save panel.
//   The same copy is available in Settings or through the protection flow.
//   Enable protection the switch, or "Save RichPass and protect it…", saves and protects;
//                       "Choose an existing file…" protects one already on this Mac; protected,
//                       it shows the state with Check now, Show in Finder and Stop protecting.
//
// DECLARED DIFFERENCE FROM THE ROUND: with a protected copy the download becomes
// "Open RichPass · Download again · Your protected copy is in Documents", and Open RichPass would
// open the protected file in the browser. RichOS has no command
// that opens a file in the browser (`open_external` opens only a fixed table of addresses,
// `opener.rs`), so the page shows "Show in Finder" there instead, the protected file selected,
// one double-click from open. It is said in the handover, not hidden.
(function () {
  const bridge = window.RichBridge;
  const page = document.getElementById("richpass-page");
  if (!bridge || !page) return;
  const call = (name, args) => bridge.invoke(name, args || {});
  const $ = (id) => document.getElementById(id);
  let view = null;
  let busy = false;

  const PHASES = {
    idle: "No protected file",
    checking: "Checking",
    verified: "Protected",
    repairing: "Restoring the verified copy",
    waiting: "Waiting for its folder",
    paused: "Automatic repairs paused",
    blocked: "Protection needs attention",
    cache_unavailable: "Verified repair copy unavailable",
  };
  const protectedNow = () => !!(view && view.path);
  /// "Documents" for ~/Documents/RichPass.html: the folder's own name, as the copy says it.
  const folderOf = (p) => {
    const parts = String(p || "").split("/").filter(Boolean);
    return parts.length > 1 ? parts[parts.length - 2] : "its folder";
  };

  function paint() {
    const on = protectedNow();
    const dl = $("rp-download");
    dl.hidden = on;
    $("rp-download-cap").hidden = on;
    $("rp-reveal").hidden = !on;
    $("rp-again").hidden = !on;
    $("rp-where").hidden = !on;
    if (on) $("rp-where").textContent = "Your protected copy is in " + folderOf(view.path) + ".";
    const sw = $("rp-switch");
    sw.setAttribute("aria-checked", String(on));
    sw.disabled = busy || (view && view.supported === false);
    $("rp-off-acts").hidden = on || (view && view.supported === false);
    $("rp-on").hidden = !on;
    $("rp-unsupported").hidden = !(view && view.supported === false);
    if (on) {
      const restored = view.phase === "verified" && view.notice;
      $("rp-state-title").textContent = restored ? "Restored" : PHASES[view.phase] || "Protected";
      const when = view.lastChecked ? " Last checked " + new Date(view.lastChecked * 1000).toLocaleString() + "." : "";
      $("rp-state-line").textContent = (view.message ? view.message + " " : "") + (view.release ? "Release " + view.release + "." : "") + when;
      $("rp-state-path").textContent = view.path;
    }
    for (const b of page.querySelectorAll("button[data-rp]")) b.disabled = busy;
  }

  function error(e) {
    const n = $("rp-error");
    n.textContent = String(e);
    n.hidden = false;
  }
  async function read() {
    try {
      view = await call("richpass_status");
      paint();
    } catch (e) {
      error(e);
    }
  }

  const ACT = {
    why: () => call("open_external", { target: "github.com/RichSpark/RichOS/tree/main/richos/tools/richpass" }),
    download: () => call("richpass_save", { protect: false }),
    protect: () => call("richpass_save", { protect: true }),
    choose: () => call("richpass_choose"),
    check: () => call("richpass_check"),
    show: () => call("richpass_show").then(() => view),
    stop: () => call("richpass_stop"),
  };
  page.addEventListener("click", async (e) => {
    const b = e.target.closest("[data-rp]");
    if (!b) return;
    if (b.dataset.rp === "why") e.preventDefault();
    if (busy) return;
    let action = b.dataset.rp;
    if (action === "switch") action = protectedNow() ? "stop" : "protect";
    if (!ACT[action]) return;
    $("rp-error").hidden = true;
    busy = true;
    paint();
    try {
      const next = await ACT[action]();
      if (next) view = next;
    } catch (err) {
      error(err);
    }
    busy = false;
    paint();
  });

  function show() {
    page.hidden = false;
    document.body.classList.add("page-open");
    $("page-crumb").hidden = false;
    read();
    const t = $("rp-title");
    if (t) t.focus({ preventScroll: true });
  }
  function hide() {
    page.hidden = true;
    document.body.classList.remove("page-open");
    $("page-crumb").hidden = true;
  }

  if (window.RichNavRail) window.RichNavRail.registerPage("pass", { show, hide });
  if (bridge.listen) bridge.listen("rich://richpass", ({ payload }) => {
    view = payload;
    if (!page.hidden) paint();
  });
})();
