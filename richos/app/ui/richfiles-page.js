"use strict";

(function () {
  const bridge = window.RichBridge;
  const rail = window.RichNavRail;
  const button = document.getElementById("nr-richfiles");
  const page = document.getElementById("richfiles-page");
  const pane = document.getElementById("richfiles-pane");
  const error = document.getElementById("richfiles-pane-error");
  const loading = document.getElementById("richfiles-loading");
  if (!bridge || !rail || !button || !page || !pane || !error || !loading) return;

  const hiddenBounds = { left: 0, top: 0, width: 0, height: 0, visible: false };
  let last = "";
  let frame = 0;
  let waiting = false;
  let again = false;
  let showRequested = false;
  // RichFiles' pane exists: it was built ahead of the click, or a click has shown it.
  let ready = false;
  // Two frames on: what the page changed has been drawn.
  const painted = () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));

  function intersects(first, second) {
    return first.left < second.right && second.left < first.right
      && first.top < second.bottom && second.top < first.bottom;
  }

  function bounds() {
    if (page.hidden || document.getElementById("app")?.inert || window.RichDismiss?.question?.()) return hiddenBounds;
    const box = pane.getBoundingClientRect();
    if (box.width <= 0 || box.height <= 0 || !innerWidth || !innerHeight) return hiddenBounds;
    const popups = window.RichDismiss?.open?.() || [];
    if (popups.some((node) => intersects(box, node.getBoundingClientRect()))) return hiddenBounds;
    const limit = (number) => Math.round(Math.min(1, Math.max(0, number)) * 1000000) / 1000000;
    const left = limit(box.left / innerWidth);
    const top = limit(box.top / innerHeight);
    const width = limit(box.right / innerWidth) - left;
    const height = limit(box.bottom / innerHeight) - top;
    return width > 0 && height > 0 ? { left, top, width, height, visible: true } : hiddenBounds;
  }

  function schedule() {
    if (!frame) frame = requestAnimationFrame(() => { frame = 0; refresh(); });
  }

  async function refresh() {
    if (waiting) { again = true; return; }
    const next = bounds();
    const key = JSON.stringify(next);
    if (key === last && (!next.visible || !showRequested)) return;
    waiting = true;
    try {
      if (key !== last) {
        // A CLICK NEVER LOOKS DEAD (the CEO, 2026-11-15: "when technically not possible to
        // eliminate the wait, then at least show a basic FUCKING LOADING SCREEN!!!"). A click that
        // comes before RichFiles is ready makes the shell load and build it, which holds the
        // window still until it is done. So the loading line (`show`) is drawn first, and only
        // then is the shell asked.
        if (next.visible && !ready) await painted();
        await bridge.invoke("richfiles_place_pane", { bounds: next });
        last = key;
        if (next.visible) {
          ready = true;
          loading.hidden = true;
        }
      }
      if (next.visible && showRequested) {
        showRequested = false;
        await bridge.invoke("richfiles_accept_request", {});
      }
      error.hidden = true;
    } catch (reason) {
      loading.hidden = true;
      if (!page.hidden) {
        error.textContent = String(reason);
        error.hidden = false;
      }
    } finally {
      waiting = false;
      if (again) { again = false; schedule(); }
    }
  }

  const size = new ResizeObserver(schedule);
  const overlays = new MutationObserver(schedule);
  function show() {
    prepared = true; // showing it builds it: nothing is left to make ready
    loading.hidden = ready;
    page.hidden = false;
    document.body.classList.add("page-open");
    size.observe(pane);
    overlays.observe(document.body, { subtree: true, attributes: true, attributeFilter: ["hidden", "inert", "open", "class"] });
    window.addEventListener("resize", schedule);
    schedule();
  }

  function hide() {
    loading.hidden = true;
    page.hidden = true;
    document.body.classList.remove("page-open");
    size.disconnect();
    overlays.disconnect();
    window.removeEventListener("resize", schedule);
    refresh();
  }

  function requestPane() {
    showRequested = true;
    if (!window.RichDismiss?.question?.()) window.RichHome?.hide?.("richfiles");
    window.RichOutput?.close?.({ keepFocus: true });
    rail.showPage("richfiles");
    schedule();
  }

  // RICHFILES IS MADE READY BEFORE THE FIRST CLICK ON ITS ICON. The CEO, 2026-11-15: "whatever
  // indexing or loading needs to be done to make RichFiles appear instantly after nav rail button
  // click can and should be done in those few seconds that we are almost always guaranteed to
  // have before the user click on the RichFiles icon for the first time."
  //
  // The first click used to load RichFiles and build its pane while the window stood still. The
  // shell now does both ahead of it (`richfiles_prepare_pane`), after `main.js` says the app is
  // usable, so none of it is on the launch path, and in two steps:
  //
  // THE LIBRARY, as soon as the opening screen has left. It loads on a background thread and
  // holds nothing up, so it waits for nothing else.
  //
  // THE PANE, which holds the window still for a fraction of a second while it is built (the
  // same work the first click used to wait for). So it waits for the home screen to leave as
  // well, since that picture moves and would be seen to stop, and is built SETTLE_MS after it
  // has: the home screen's own fade is over, nothing on the conversation screen is moving, and
  // a hand that pressed its way here has not reached the keys yet. It does not wait for typing
  // to stop, so typing cannot put it off. The rail is only on screen once both screens have
  // left, so this is always ahead of the first possible click on the icon. A click that comes
  // first is served as before.
  const SETTLE_MS = 300;
  let usable = false;
  let offered = false;
  let loaded = false;
  let prepared = false;
  let clear = null; // when the opening screen and the home screen were first seen gone
  function prepare() {
    if (prepared || !usable || !offered) return;
    if (!window.RichSplash?.isUp?.()) {
      if (!loaded) {
        loaded = true;
        bridge.invoke("richfiles_prepare_pane", { pane: false }).catch(() => {});
      }
      const now = performance.now();
      if (window.RichHome?.isOpen?.()) clear = null;
      else if (clear === null) clear = now;
      if (clear !== null && now - clear >= SETTLE_MS) {
        prepared = true;
        bridge.invoke("richfiles_prepare_pane", { pane: true }).then(() => { ready = true; }).catch(() => {});
        return;
      }
    }
    setTimeout(prepare, 100);
  }
  window.RichFilesReady = { start() { usable = true; prepare(); } };
  if (window.__richosRichFilesReady) window.RichFilesReady.start();

  bridge.invoke("richfiles_pane_available", {}).then(async (available) => {
    if (available !== true) return;
    rail.registerPage("richfiles", { show, hide });
    button.hidden = false;
    window.RichFilesPage = { available: true };
    offered = true;
    prepare();
    try {
      await bridge.listen?.("rich://richfiles-show", requestPane);
    } catch {
      // The initial pending request can still be handled.
    }
    const pending = await bridge.invoke("richfiles_request_pending", {});
    if (pending === true) requestPane();
  }).catch(() => {});
})();
