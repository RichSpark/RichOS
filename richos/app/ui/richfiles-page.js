"use strict";

(function () {
  const bridge = window.RichBridge;
  const rail = window.RichNavRail;
  const button = document.getElementById("nr-richfiles");
  const page = document.getElementById("richfiles-page");
  const pane = document.getElementById("richfiles-pane");
  const error = document.getElementById("richfiles-pane-error");
  if (!bridge || !rail || !button || !page || !pane || !error) return;

  const hiddenBounds = { left: 0, top: 0, width: 0, height: 0, visible: false };
  let last = "";
  let frame = 0;
  let waiting = false;
  let again = false;
  let showRequested = false;

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
        await bridge.invoke("richfiles_place_pane", { bounds: next });
        last = key;
      }
      if (next.visible && showRequested) {
        showRequested = false;
        await bridge.invoke("richfiles_accept_request", {});
      }
      error.hidden = true;
    } catch (reason) {
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
    page.hidden = false;
    document.body.classList.add("page-open");
    size.observe(pane);
    overlays.observe(document.body, { subtree: true, attributes: true, attributeFilter: ["hidden", "inert", "open", "class"] });
    window.addEventListener("resize", schedule);
    schedule();
  }

  function hide() {
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

  bridge.invoke("richfiles_pane_available", {}).then(async (available) => {
    if (available !== true) return;
    rail.registerPage("richfiles", { show, hide });
    button.hidden = false;
    window.RichFilesPage = { available: true };
    try {
      await bridge.listen?.("rich://richfiles-show", requestPane);
    } catch {
      // The initial pending request can still be handled.
    }
    const pending = await bridge.invoke("richfiles_request_pending", {});
    if (pending === true) requestPane();
  }).catch(() => {});
})();
