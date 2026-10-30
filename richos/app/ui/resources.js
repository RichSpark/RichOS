"use strict";
// The live Mac resources square at the foot of the sidebar, opposite the name.
// Ink is the default; the context menu offers the persisted Readout view.
//
//   Ink      a smooth CPU area gliding between one-second samples, user (green) over system
//            (red), and RAM as its own column at the right that turns amber under memory
//            pressure (red at critical).
//   Readout  Ink plus the live CPU % and RAM % beside it, each with a small key in the square's
//            own paints instead of the words.
//
// THE DATA IS THE APP'S OWN SAMPLER and nothing else: `system_resources` reads the one-second
// native sampler's cache (`crates/richos-core/src/system_resources.rs`), the same snapshot every
// agent's `system_resources` tool returns. There is no sampler, synthetic or otherwise, in this
// file. CPU user includes nice ticks and is normalized across all cores. Used RAM excludes
// file cache and includes wired pages and the physical compressor footprint. The native module
// defines and tests that formula; it does not promise exact Activity Monitor accounting.
//
// EVERY STATE IS HONEST. Measuring (no interval yet) draws no value; Not current (no sample for
// over three seconds) holds the history grayed under three dots; Unavailable holds it grayed
// under a bar; a stretch with no reading is a hatched gap, labeled "Asleep" when the Mac slept.
// None of them ever shows 0%.
//
// The square is a button named "Mac resources" and described by the card, so a screen reader
// reaches every number without color. There is no live region: nothing is announced every
// second. The card opens on hover, on keyboard focus and on a click (pinned), and never moves
// focus or changes the selected chat. Escape closes it while focus stays on the square.
// Right-click, the context-menu key or Shift+F10 opens the view menu (Ink, Readout), whose
// choice is saved in the app's settings store (`config.rs` `resources_view`).
(function () {
  const bridge = window.RichBridge;
  const footer = document.getElementById("rail-footer");
  if (!bridge || !footer) return;

  const HIST = 60;
  const POLL_MS = 500;
  const GLIDE_TICK_MS = 100;
  const VIEWS = ["ink", "readout"];
  const VIEW_NAMES = { ink: "Ink", readout: "Readout" };
  const VIEW_KEY = "richos.resources-view";
  const GIB = 1073741824;
  // The sampler's own words for "the next interval is not complete yet". Not an outage: the
  // square says Measuring, not Unavailable (system_resources.rs, the sampler loop).
  const FRESH_INTERVAL = /first CPU interval|fresh CPU interval/;
  const AFTER_SLEEP = /after sleep/;
  const reduced = () => window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ---- state --------------------------------------------------------------------------------
  const S = {
    started: false,
    // live | measuring | stale | unavailable
    status: "measuring",
    current: null,
    // oldest first, at most HIST samples, as the sampler returned them
    history: [],
    // when the newest sample was taken, on this page's clock (performance.now)
    newestAt: 0,
    // the newest sample's age when it was last read, for the card's age line
    ageMs: null,
    // wall-clock spans [from, to] the Mac slept through, seen by this page
    sleeps: [],
    lastTick: null,
  };

  // ---- the DOM ----------------------------------------------------------------------------------
  const btn = document.createElement("button");
  btn.type = "button";
  btn.id = "res-square";
  btn.className = "res";
  btn.setAttribute("aria-label", "Mac resources");
  btn.setAttribute("aria-describedby", "res-card");
  btn.setAttribute("aria-expanded", "false");
  btn.setAttribute("aria-keyshortcuts", "Shift+F10");
  btn.innerHTML =
    '<span class="res-readout" aria-hidden="true">' +
    '<span class="res-num"><i class="res-key cpu"><i></i><i></i></i><b id="res-cpu">…</b></span>' +
    '<span class="res-num"><i class="res-key ram"><i></i></i><b id="res-ram">…</b></span>' +
    "</span>" +
    '<span class="res-tile"><canvas aria-hidden="true"></canvas></span>';
  footer.appendChild(btn);
  const tileCanvas = btn.querySelector(".res-tile canvas");
  const cpuNum = btn.querySelector("#res-cpu");
  const ramNum = btn.querySelector("#res-ram");
  const ramKey = btn.querySelector(".res-key.ram");

  const card = document.createElement("div");
  card.id = "res-card";
  card.className = "res-card";
  card.setAttribute("role", "tooltip");
  card.innerHTML =
    '<div class="res-card-head"><span class="res-card-title">This Mac</span><span class="res-chip" data-k="chip"></span></div>' +
    '<div class="res-graph-wrap"><canvas class="res-graph" aria-hidden="true"></canvas></div>' +
    '<div class="res-axis" aria-hidden="true"><span>60 seconds ago</span><span>Now</span></div>' +
    '<dl class="res-rows" data-k="rows"></dl>' +
    '<p class="res-note" data-k="note" hidden></p>' +
    '<div class="res-foot"><span data-k="age"></span><span>Agents read this too</span></div>';
  document.body.appendChild(card);
  const bigCanvas = card.querySelector(".res-graph");
  const $ = (k) => card.querySelector(`[data-k="${k}"]`);

  const menu = document.createElement("div");
  menu.id = "res-menu";
  menu.className = "menu res-menu";
  menu.setAttribute("role", "menu");
  menu.setAttribute("aria-label", "Mac resources view");
  menu.setAttribute("data-dismiss", "escape");
  menu.hidden = true;
  menu.innerHTML = VIEWS.map(
    (v) =>
      `<button type="button" class="menu-item res-menu-item" role="menuitemradio" data-view="${v}" aria-checked="false">` +
      `<span class="res-check" aria-hidden="true">✓</span>${VIEW_NAMES[v]}</button>`
  ).join("");
  document.body.appendChild(menu);

  // ---- the view (Ink | Readout), a saved setting ---------------------------------------------
  function readLocalView() {
    try {
      const v = window.localStorage.getItem(VIEW_KEY);
      return VIEWS.includes(v) ? v : "ink";
    } catch (_e) {
      return "ink";
    }
  }
  let view = readLocalView();
  let viewChosen = false;
  let viewSave = Promise.resolve();
  function applyView(v) {
    view = VIEWS.includes(v) ? v : "ink";
    btn.dataset.view = view;
    for (const item of menu.querySelectorAll("[data-view]")) {
      item.setAttribute("aria-checked", String(item.dataset.view === view));
    }
    place();
  }
  /// The local mirror paints the first frame; the settings store (`config.rs`) is the source of
  /// truth, written on every choice and read back once at start, the same shape as the
  /// assertiveness dial and the splash switch in `main.js`.
  function chooseView(v) {
    viewChosen = true;
    applyView(v);
    try {
      window.localStorage.setItem(VIEW_KEY, view);
    } catch (_e) {
      /* the store below still keeps it */
    }
    const selected = view;
    viewSave = viewSave.catch(() => {}).then(() => bridge.invoke("set_resources_view", { view: selected }));
    return viewSave.catch(() => {});
  }
  async function syncViewFromStore() {
    try {
      const stored = await bridge.invoke("get_resources_view");
      if (!viewChosen && VIEWS.includes(stored)) {
        try {
          window.localStorage.setItem(VIEW_KEY, stored);
        } catch (_e) {
          /* mirror only */
        }
        applyView(stored);
      }
    } catch (_e) {
      /* the mirror already painted */
    }
  }
  applyView(view);

  // ---- reading the sampler ----------------------------------------------------------------------
  function slotsOf(history) {
    // One slot per second, newest at the right. A gap between two samples (more than one second
    // apart on the sampler's sleep-inclusive clock) becomes empty slots, never zeros.
    const slots = [];
    for (let i = 0; i < history.length; i++) {
      const s = history[i];
      if (i > 0) {
        const missing = Math.round((s.monotonic_ms - history[i - 1].monotonic_ms) / 1000) - 1;
        for (let k = 0; k < Math.min(missing, HIST); k++) {
          slots.push({ gap: true, from: history[i - 1].sampled_at_ms, to: s.sampled_at_ms });
        }
      }
      slots.push(s);
    }
    return slots.slice(-HIST);
  }

  function absorb(snapshot, readAt, transitMs = 0) {
    const status = snapshot && snapshot.status;
    const detail = (snapshot && snapshot.detail) || "";
    const history = Array.isArray(snapshot && snapshot.history) ? snapshot.history.slice(-HIST) : [];
    if (AFTER_SLEEP.test(detail)) noteSleep(Date.now() - 1000, Date.now());
    if (status === "warming_up") {
      // A sampler that has not finished its first interval has measured nothing yet, and a
      // history from before it is not its own: nothing is drawn as a value.
      S.history = [];
      S.current = null;
      S.ageMs = null;
    }
    if (history.length) {
      S.history = history;
      // The newest sample's age on the sampler's own clock, so a held history knows how old it
      // is even when the sampler no longer has a current reading.
      const newest = history[history.length - 1];
      const observed = Number(snapshot.observed_monotonic_ms);
      if (observed >= newest.monotonic_ms) {
        S.ageMs = observed - newest.monotonic_ms + transitMs;
        S.newestAt = readAt - S.ageMs;
      }
    }
    if (status === "available" && snapshot.current) {
      S.current = snapshot.current;
      S.ageMs = (snapshot.age_ms || 0) + transitMs;
      S.status = S.ageMs > 3000 ? "stale" : "live";
      S.newestAt = readAt - S.ageMs;
    } else if (status === "warming_up" || (status === "unavailable" && FRESH_INTERVAL.test(detail))) {
      S.status = "measuring";
    } else if (status === "stale") {
      S.status = "stale";
    } else {
      S.status = "unavailable";
    }
    // Held, never current: the last sample stands for the RAM column and the card's layout
    // under the gray, and every number that would come from it is drawn as a dash.
    if (!S.current && S.history.length) S.current = S.history[S.history.length - 1];
  }

  /// The Mac slept between two of this page's ticks when the wall clock moved further than the
  /// page's own clock, which stops while the Mac sleeps. A gap that overlaps such a span is
  /// labeled Asleep; any other gap stays an unlabeled hatch.
  function noteSleep(from, to) {
    S.sleeps.push([from, to]);
    while (S.sleeps.length > 8) S.sleeps.shift();
  }
  function sleptThrough(slot) {
    return S.sleeps.some(([a, b]) => slot.from <= b && slot.to >= a);
  }

  let polling = false;
  async function poll() {
    if (polling) return;
    polling = true;
    const wall = Date.now();
    const mono = performance.now();
    try {
      const snapshot = await bridge.invoke("system_resources");
      const readAt = performance.now();
      // Conservatively include bridge transit, including sleep while the answer was pending.
      absorb(snapshot, readAt, Math.max(0, readAt - mono, Date.now() - wall));
    } catch (_e) {
      absorb(null, performance.now());
    } finally {
      polling = false;
    }
    render();
  }

  // ---- paint ------------------------------------------------------------------------------------
  const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  function palette() {
    return {
      ground: css("--res-ground"),
      user: css("--res-user"),
      system: css("--res-system"),
      ram: css("--res-ram"),
      track: css("--res-track"),
      glyph: css("--res-glyph"),
      hatch: css("--res-hatch"),
      warn: css("--res-warn"),
      crit: css("--res-crit"),
    };
  }
  function sizeCanvas(c, w, h) {
    const dpr = Math.max(1, window.devicePixelRatio || 1);
    if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
      c.width = Math.round(w * dpr);
      c.height = Math.round(h * dpr);
    }
    const ctx = c.getContext("2d");
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return ctx;
  }
  function hatch(ctx, x, y, w, h, color) {
    if (w <= 0) return;
    ctx.save();
    ctx.beginPath();
    ctx.rect(x, y, w, h);
    ctx.clip();
    ctx.strokeStyle = color;
    ctx.lineWidth = 1;
    for (let k = -h; k < w + h; k += 3) {
      ctx.beginPath();
      ctx.moveTo(x + k, y + h);
      ctx.lineTo(x + k + h, y);
      ctx.stroke();
    }
    ctx.restore();
  }
  function pressureOf(c) {
    if (!c) return "";
    return c.memory_pressure === "critical" ? "crit" : c.memory_pressure === "warning" ? "warn" : "";
  }
  function phaseNow() {
    if (S.status !== "live" || reduced()) return 0;
    return Math.max(0, Math.min(1, (performance.now() - S.newestAt) / 1000));
  }

  /// The CPU history as filled areas: user + system in green, system in red under it, a crisp
  /// top edge. Runs of valid samples only; a gap is hatched and holds no value.
  function paintCpu(ctx, P, slots, x, y, w, h, step, phase) {
    const off = phase * step;
    const n = slots.length;
    for (let i = 0; i < n; i++) {
      const s = slots[n - 1 - i];
      if (s.gap) {
        const cx = x + w - (i + 1) * step - off;
        hatch(ctx, Math.max(x, cx), y, Math.min(step, x + w - cx), h, P.hatch);
      }
    }
    const pts = [];
    for (let i = 0; i < n; i++) {
      const s = slots[n - 1 - i];
      const px = x + w - i * step - off;
      if (px < x - step) break;
      pts.push(s.gap ? null : { x: px, s });
    }
    pts.reverse();
    const runs = [];
    let run = [];
    for (const p of pts) {
      if (p) run.push(p);
      else {
        if (run.length) runs.push(run);
        run = [];
      }
    }
    if (run.length) runs.push(run);
    ctx.save();
    ctx.beginPath();
    ctx.rect(x, y, w, h);
    ctx.clip();
    for (let r = 0; r < runs.length; r++) {
      const pts2 = runs[r];
      // The newest run reaches the right edge: the newest value is held there until the next.
      if (r === runs.length - 1 && !slots[n - 1].gap) pts2.push({ x: x + w + 1, s: pts2[pts2.length - 1].s });
      // A lone sample is drawn as a one-step column rather than as nothing.
      if (pts2.length === 1) pts2.unshift({ x: pts2[0].x - step, s: pts2[0].s });
      const area = (key, color) => {
        ctx.beginPath();
        ctx.moveTo(pts2[0].x, y + h);
        for (const p of pts2) ctx.lineTo(p.x, y + h - (Math.min(100, key(p.s)) / 100) * h);
        ctx.lineTo(pts2[pts2.length - 1].x, y + h);
        ctx.closePath();
        ctx.fillStyle = color;
        ctx.fill();
      };
      area((s) => s.cpu_user_percent + s.cpu_system_percent, P.user);
      area((s) => s.cpu_system_percent, P.system);
      ctx.beginPath();
      pts2.forEach((p, k) => {
        const py = y + h - (Math.min(100, p.s.cpu_user_percent + p.s.cpu_system_percent) / 100) * h + 0.5;
        if (k) ctx.lineTo(p.x, py);
        else ctx.moveTo(p.x, py);
      });
      ctx.strokeStyle = P.user;
      ctx.lineWidth = 1;
      ctx.stroke();
    }
    ctx.restore();
  }

  /// Stale (three dots) or unavailable (a bar) over the held history, which is grayed by the
  /// ground laid over it so it cannot be read as current.
  function paintGlyph(ctx, P, w, h, kind) {
    ctx.fillStyle = P.ground;
    ctx.globalAlpha = 0.62;
    ctx.fillRect(0, 0, w, h);
    ctx.globalAlpha = 1;
    ctx.fillStyle = P.glyph;
    const cx = w / 2;
    const cy = h / 2;
    const u = w / 34;
    if (kind === "stale") {
      for (const dx of [-6, 0, 6]) {
        ctx.beginPath();
        ctx.arc(cx + dx * u, cy, 1.9 * u, 0, Math.PI * 2);
        ctx.fill();
      }
    } else {
      const bw = 14 * u;
      const bh = 3 * u;
      ctx.beginPath();
      ctx.roundRect(cx - bw / 2, cy - bh / 2, bw, bh, bh / 2);
      ctx.fill();
    }
  }

  /// Measuring: no value exists yet, so none is drawn. A dotted floor and a light traveling
  /// along it, which an idle Mac never looks like.
  function paintWarm(ctx, P, x, y, w, h) {
    ctx.fillStyle = P.glyph;
    ctx.globalAlpha = 0.5;
    for (let k = 0; k < w; k += 3) ctx.fillRect(x + k, y + h - 1, 1, 1);
    ctx.globalAlpha = 1;
    const t = reduced() ? 0.5 : (performance.now() / 1100) % 1;
    ctx.beginPath();
    ctx.arc(x + w - t * w, y + h - 1, 2, 0, Math.PI * 2);
    ctx.fill();
  }

  function paintTile() {
    const ctx = sizeCanvas(tileCanvas, 34, 34);
    const P = palette();
    ctx.fillStyle = P.ground;
    ctx.fillRect(0, 0, 34, 34);
    const gx = 3;
    const gy = 3;
    const gw = 21;
    const gh = 26;
    const slots = slotsOf(S.history);
    const warming = S.status === "measuring" && !slots.length;
    if (warming) paintWarm(ctx, P, gx, gy, gw, gh);
    else paintCpu(ctx, P, slots, gx, gy, gw, gh, 1, phaseNow());
    const rx = 26;
    const rw = 3;
    ctx.fillStyle = P.track;
    ctx.beginPath();
    ctx.roundRect(rx, gy, rw, gh, 1.5);
    ctx.fill();
    const c = S.current;
    if (c && !warming && c.memory_total_bytes > 0) {
      const mc = pressureOf(c);
      const fh = (gh * c.memory_used_bytes) / c.memory_total_bytes;
      ctx.fillStyle = mc === "crit" ? P.crit : mc === "warn" ? P.warn : P.ram;
      ctx.beginPath();
      ctx.roundRect(rx, gy + gh - fh, rw, fh, 1.5);
      ctx.fill();
    }
    if (S.status === "stale") paintGlyph(ctx, P, 34, 34, "stale");
    if (S.status === "unavailable") paintGlyph(ctx, P, 34, 34, "down");
  }

  function sleepGap(slots) {
    // The widest run of gap slots that overlaps a sleep, as [first, last] slot indexes.
    let best = null;
    for (let i = 0; i < slots.length; i++) {
      if (!slots[i].gap) continue;
      let j = i;
      while (j + 1 < slots.length && slots[j + 1].gap) j++;
      if (sleptThrough(slots[i]) && (!best || j - i > best[1] - best[0])) best = [i, j];
      i = j;
    }
    return best;
  }

  function paintBig() {
    const w = 266;
    const h = 72;
    const ctx = sizeCanvas(bigCanvas, w, h);
    const P = palette();
    ctx.fillStyle = P.ground;
    ctx.fillRect(0, 0, w, h);
    // quiet quarter lines (decorative texture, declared in style.css)
    ctx.fillStyle = P.hatch;
    for (const q of [0.25, 0.5, 0.75]) ctx.fillRect(4, Math.round(4 + (h - 8) * q), w - 8, 1);
    const slots = slotsOf(S.history);
    if (S.status === "measuring" && !slots.length) {
      paintWarm(ctx, P, 4, 4, w - 8, h - 8);
      return;
    }
    const step = (w - 8) / HIST;
    const phase = phaseNow();
    paintCpu(ctx, P, slots, 4, 4, w - 8, h - 8, step, phase);
    const gap = sleepGap(slots);
    if (gap) {
      const fromRight = slots.length - 1 - gap[1];
      const xr = 4 + (w - 8) - fromRight * step - phase * step;
      const xl = xr - (gap[1] - gap[0] + 1) * step;
      if (xr - xl > 30) {
        ctx.font = "600 14px " + (css("--font") || "sans-serif");
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        const tw = ctx.measureText("Asleep").width + 10;
        ctx.fillStyle = P.ground;
        ctx.fillRect((xl + xr) / 2 - tw / 2, h / 2 - 10, tw, 20);
        ctx.fillStyle = P.glyph;
        ctx.fillText("Asleep", (xl + xr) / 2, h / 2 + 1);
      }
    }
    if (S.status === "stale" || S.status === "unavailable") {
      ctx.fillStyle = P.ground;
      ctx.globalAlpha = 0.62;
      ctx.fillRect(0, 0, w, h);
      ctx.globalAlpha = 1;
    }
  }

  // ---- words ------------------------------------------------------------------------------------
  const pct = (n) => `${Math.round(n)}%`;
  const gb = (n) => `${(n / GIB).toFixed(1)} GB`;
  const PRESSURE = { normal: "Normal", warning: "Warning", critical: "Critical", unknown: "Unknown" };
  const TONE = {
    live: ["live", "Live"],
    measuring: ["wait", "Measuring"],
    stale: ["stale", "Not current"],
    unavailable: ["down", "Unavailable"],
  };
  const NOTES = {
    measuring: "Taking the first reading. Nothing is shown until there is one.",
    stale: "The reading stopped. The graph is held where it was. Nothing here is current.",
    unavailable: "RichOS can't read this Mac's counters right now. The last minute is kept, grayed out.",
  };
  const esc = (s) => String(s).replace(/[&<>"]/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[ch]);

  function ageText() {
    if (S.status === "measuring" && !S.current) return "First reading in a second";
    if (S.ageMs == null) return "No reading yet";
    const a = Math.max(0, performance.now() - S.newestAt) / 1000;
    if (a < 1.5) return "Updated just now";
    const n = Math.round(a);
    return `Last reading ${n} ${n === 1 ? "second" : "seconds"} ago`;
  }

  let lastRows = "";
  function render() {
    const c = S.current;
    const live = S.status === "live" && c;
    const pending = S.status === "measuring";
    // The readout never says 0% for a reading it does not have: "…" while measuring, a hyphen
    // while not current or unavailable. A plain hyphen marks an unavailable value.
    cpuNum.textContent = live ? pct(c.cpu_busy_percent) : pending ? "…" : "-";
    ramNum.textContent = live ? pct((100 * c.memory_used_bytes) / c.memory_total_bytes) : pending ? "…" : "-";
    const pk = live ? pressureOf(c) : "";
    ramKey.classList.toggle("is-warn", pk === "warn");
    ramKey.classList.toggle("is-crit", pk === "crit");

    const chip = $("chip");
    const tone = TONE[S.status];
    chip.dataset.tone = tone[0];
    chip.textContent = tone[1];
    let rows;
    if (!c || (pending && !S.history.length)) {
      rows = '<dt class="res-lead">CPU</dt><dd>Measuring…</dd><dt class="res-lead">RAM</dt><dd>Measuring…</dd>';
      if (S.status === "stale" || S.status === "unavailable") {
        rows = '<dt class="res-lead">CPU</dt><dd>-</dd><dt class="res-lead">RAM</dt><dd>-</dd>';
      }
    } else {
      const v = (s) => (live ? s : pending ? "…" : "-");
      const mc = pressureOf(c);
      const cls = mc && live ? " is-" + mc : "";
      const usedPct = (100 * c.memory_used_bytes) / c.memory_total_bytes;
      rows =
        `<dt class="res-lead">CPU</dt><dd>${v(pct(c.cpu_busy_percent))}</dd>` +
        `<dt><i class="res-sw u"></i>User</dt><dd>${v(pct(c.cpu_user_percent))}</dd>` +
        `<dt><i class="res-sw s"></i>System</dt><dd>${v(pct(c.cpu_system_percent))}</dd>` +
        '<span class="res-gap"></span>' +
        `<dt class="res-lead"><i class="res-sw m${cls}"></i>RAM</dt><dd>${v(`${(c.memory_used_bytes / GIB).toFixed(1)} of ${Math.round(c.memory_total_bytes / GIB)} GB`)}</dd>` +
        `<div class="res-membar" aria-hidden="true"><i class="${cls.trim()}" style="width:${live ? usedPct.toFixed(1) : 0}%"></i></div>` +
        `<dt>Compressed</dt><dd>${v(gb(c.memory_compressed_bytes))}</dd>` +
        `<dt>Memory pressure</dt><dd>${v(esc(PRESSURE[c.memory_pressure] || "Unknown"))}</dd>` +
        `<dt>Swap used</dt><dd>${v(gb(c.swap_used_bytes))}</dd>`;
    }
    if (rows !== lastRows) {
      $("rows").innerHTML = rows;
      lastRows = rows;
    }
    const slept = S.status === "live" && sleepGap(slotsOf(S.history))
      ? "The Mac slept for a while. The gap is that time; nothing was measured."
      : "";
    const note = $("note");
    const text = NOTES[S.status] && !(S.status === "measuring" && S.history.length) ? NOTES[S.status] : slept;
    note.hidden = !text;
    note.textContent = text;
    $("age").textContent = ageText();
    btn.dataset.state = S.status;
    paintTile();
    if (isOpen()) paintBig();
  }

  // ---- open, pin, close -------------------------------------------------------------------------
  let pinned = false;
  let hovering = false;
  let hideTimer = 0;
  // Escape closes the card while focus stays on the square; it stays closed until focus leaves
  // and comes back.
  let focusDismissed = false;

  function place() {
    const r = btn.querySelector(".res-tile").getBoundingClientRect();
    const cw = card.offsetWidth || 300;
    const left = Math.max(8, Math.min(window.innerWidth - cw - 8, r.right + 8 - cw));
    card.style.left = left + "px";
    card.style.bottom = window.innerHeight - r.top + 12 + "px";
    card.style.setProperty("--res-arrow", Math.max(12, left + cw - (r.left + r.width / 2) - 7) + "px");
  }
  function focusShowsCard() {
    return !focusDismissed && btn.matches(":focus-visible");
  }
  function isOpen() {
    return menu.hidden && (pinned || hovering || focusShowsCard());
  }
  function sync() {
    const open = isOpen();
    if (open) place();
    const was = card.classList.contains("is-open");
    card.classList.toggle("is-open", open);
    btn.setAttribute("aria-expanded", String(pinned && open));
    if (open && !was) paintBig();
    scheduleGlide();
  }
  function closeCard() {
    pinned = false;
    hovering = false;
    if (document.activeElement === btn) focusDismissed = true;
    sync();
  }

  btn.addEventListener("pointerenter", () => {
    clearTimeout(hideTimer);
    hovering = true;
    sync();
  });
  btn.addEventListener("pointerleave", () => {
    hideTimer = setTimeout(() => {
      hovering = false;
      sync();
    }, 160);
  });
  card.addEventListener("pointerenter", () => {
    clearTimeout(hideTimer);
    if (card.classList.contains("is-open")) hovering = true;
  });
  card.addEventListener("pointerleave", () => {
    hideTimer = setTimeout(() => {
      hovering = false;
      sync();
    }, 160);
  });
  btn.addEventListener("focus", sync);
  btn.addEventListener("blur", () => {
    focusDismissed = false;
    setTimeout(sync, 0);
  });
  btn.addEventListener("click", () => {
    if (!menu.hidden) return;
    pinned = !pinned;
    if (pinned) focusDismissed = false;
    sync();
  });
  btn.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && card.classList.contains("is-open")) {
      // Answered here, so the window's Escape does not also close something behind the card.
      e.preventDefault();
      e.stopPropagation();
      closeCard();
      return;
    }
    if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
      e.preventDefault();
      openMenu(null);
    }
  });
  btn.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    const fromKeyboard = e.clientX === 0 && e.clientY === 0;
    openMenu(fromKeyboard ? null : { x: e.clientX, y: e.clientY });
  });
  document.addEventListener("keydown", (e) => {
    // A hovered or pinned card closes on Escape wherever focus is.
    if (e.key === "Escape" && (pinned || hovering) && document.activeElement !== btn) {
      pinned = false;
      hovering = false;
      sync();
    }
  });
  document.addEventListener("pointerdown", (e) => {
    if (pinned && !btn.contains(e.target) && !card.contains(e.target)) {
      pinned = false;
      sync();
    }
    if (!menu.hidden && !menu.contains(e.target)) closeMenu(false);
  });
  window.addEventListener("resize", () => {
    if (isOpen()) place();
    if (!menu.hidden) closeMenu(false);
  });

  // ---- the view menu ----------------------------------------------------------------------------
  function items() {
    return Array.from(menu.querySelectorAll(".res-menu-item"));
  }
  function openMenu(at) {
    hovering = false;
    pinned = false;
    clearTimeout(hideTimer);
    menu.hidden = false;
    sync();
    const mw = menu.offsetWidth;
    const mh = menu.offsetHeight;
    let left;
    let top;
    if (at) {
      left = at.x;
      top = at.y - mh;
    } else {
      // From the keyboard: above the square, its right edge on the square's.
      const r = btn.querySelector(".res-tile").getBoundingClientRect();
      left = r.right - mw;
      top = r.top - mh - 6;
    }
    menu.style.left = Math.round(Math.max(4, Math.min(left, window.innerWidth - mw - 4))) + "px";
    menu.style.top = Math.round(Math.max(4, Math.min(top, window.innerHeight - mh - 4))) + "px";
    const checked = items().find((b) => b.dataset.view === view) || items()[0];
    checked.focus();
  }
  /// Close the menu. `refocus` returns focus to the square, which is where it was when the
  /// menu opened from the keyboard and where a choice or Escape leaves it.
  function closeMenu(refocus) {
    if (menu.hidden) return;
    menu.hidden = true;
    if (refocus) {
      focusDismissed = true;
      btn.focus();
    }
    sync();
  }
  menu.addEventListener("click", (e) => {
    const item = e.target.closest(".res-menu-item");
    if (!item) return;
    chooseView(item.dataset.view);
    closeMenu(true);
  });
  menu.addEventListener("keydown", (e) => {
    const list = items();
    const at = list.indexOf(document.activeElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = (at + (e.key === "ArrowDown" ? 1 : list.length - 1)) % list.length;
      list[next].focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      list[e.key === "Home" ? 0 : list.length - 1].focus();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeMenu(true);
    } else if (e.key === "Tab") {
      e.preventDefault();
      closeMenu(true);
    }
  });
  menu.addEventListener("contextmenu", (e) => e.preventDefault());

  // ---- motion -------------------------------------------------------------------------------------
  // Ink glides: the history drifts between samples so it reads as alive rather than ticking. The
  // square repaints ten times a second while live (one step is one pixel a second, so that is a
  // tenth of a pixel a frame), the open card's graph every frame; nothing runs while the window
  // is hidden, and `prefers-reduced-motion` stops the glide and the traveling light.
  let glideTimer = 0;
  let glideFrame = 0;
  function moving() {
    if (document.hidden || reduced()) return false;
    return S.status === "live" || (S.status === "measuring" && !S.history.length);
  }
  function scheduleGlide() {
    if (!moving()) {
      clearTimeout(glideTimer);
      glideTimer = 0;
      cancelAnimationFrame(glideFrame);
      glideFrame = 0;
      return;
    }
    if (!glideTimer) {
      glideTimer = setTimeout(() => {
        glideTimer = 0;
        paintTile();
        scheduleGlide();
      }, GLIDE_TICK_MS);
    }
    if (isOpen() && !glideFrame) {
      glideFrame = requestAnimationFrame(() => {
        glideFrame = 0;
        if (isOpen()) {
          paintBig();
          const sec = Math.floor((performance.now() - S.newestAt) / 1000);
          if (sec !== lastAgeSec) {
            lastAgeSec = sec;
            $("age").textContent = ageText();
          }
        }
        scheduleGlide();
      });
    }
  }
  let lastAgeSec = -1;

  // ---- start ------------------------------------------------------------------------------------
  let pollTimer = 0;
  function loop() {
    clearTimeout(pollTimer);
    if (document.hidden) return;
    const wall = Date.now();
    const mono = performance.now();
    const sleepMs = S.lastTick ? wall - S.lastTick.wall - (mono - S.lastTick.mono) : 0;
    if (sleepMs > 2000) {
      noteSleep(S.lastTick.wall, wall);
      S.newestAt -= sleepMs;
    }
    S.lastTick = { wall, mono };
    if (S.status === "live" && mono - S.newestAt > 3000) {
      S.status = "stale";
      render();
    }
    // Freshness must keep ticking even when the one permitted bridge read is unresolved.
    void poll();
    scheduleGlide();
    pollTimer = setTimeout(loop, POLL_MS);
  }
  /// Started by `main.js` once the app is ready, so reading the sampler is never on the launch
  /// path. Idempotent.
  function start() {
    if (S.started) return;
    S.started = true;
    syncViewFromStore();
    loop();
  }
  document.addEventListener("visibilitychange", () => {
    if (!S.started) return;
    if (document.hidden) {
      clearTimeout(pollTimer);
      scheduleGlide();
    } else loop();
  });
  const themeWatch = new MutationObserver(() => {
    lastRows = "";
    render();
  });
  themeWatch.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });

  render();
  window.RichResources = {
    start,
    closeMenu: () => closeMenu(true),
    get view() {
      return view;
    },
    get status() {
      return S.status;
    },
    /// For the acceptance suite: repaint now (a screenshot must not wait for the next tick).
    repaint: render,
    pollNow: poll,
  };
  if (window.__richosResourcesReady) start();
})();
