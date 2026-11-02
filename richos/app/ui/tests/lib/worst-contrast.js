// The worst contrast between each element's text and the pixels actually behind its glyphs, read
// off a captured frame of the page. Lifted out of `home-team.js` unchanged so the home screen's
// ghost suite (`home-ghost.js`) measures with the same rule rather than a copy of it.

"use strict";

const contrastLib = require("./contrast");

/// The worst contrast between each element's text and the pixels actually behind its glyphs:
/// every glyph is made transparent (layout unchanged), the frame is captured, and each text
/// line box is sampled for the pixel that contrasts least with the element's own ink.
async function worstContrast(page, selectors) {
  await page.addScriptTag({ content: contrastLib.pageScript() });
  const targets = await page.evaluate((sels) => {
    const out = [];
    for (const sel of sels) {
      for (const el of document.querySelectorAll(sel)) {
        const rects = [];
        const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
        for (let n = walker.nextNode(); n; n = walker.nextNode()) {
          if (!n.textContent.trim() || n.parentElement.closest("[hidden]")) continue;
          const r = document.createRange();
          r.selectNodeContents(n);
          const cs = getComputedStyle(n.parentElement);
          for (const b of r.getClientRects()) {
            if (b.width > 0.5 && b.height > 0.5)
              rects.push({ x: b.left, y: b.top, w: b.width, h: b.height, ink: cs.color, px: parseFloat(cs.fontSize), weight: cs.fontWeight, text: n.textContent.trim() });
          }
        }
        out.push({ sel, rects });
      }
    }
    const style = document.createElement("style");
    style.id = "__blank";
    style.textContent = "#home *{transition:none!important;animation:none!important;color:transparent!important}";
    document.head.appendChild(style);
    return out;
  }, selectors);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  const buf = await page.screenshot({ fullPage: false });
  await page.evaluate(() => document.getElementById("__blank").remove());
  return page.evaluate(
    async ({ b64, targets }) => {
      const M = window.__contrastMath;
      const img = new Image();
      await new Promise((res, rej) => {
        img.onload = res;
        img.onerror = rej;
        img.src = "data:image/png;base64," + b64;
      });
      const c = document.createElement("canvas");
      c.width = img.width;
      c.height = img.height;
      const g = c.getContext("2d");
      g.drawImage(img, 0, 0);
      const scale = img.width / innerWidth;
      const rows = [];
      for (const t of targets) {
        for (const r of t.rects) {
          const ink = M.parseCssColor(r.ink);
          const data = g.getImageData(Math.floor(r.x * scale), Math.floor(r.y * scale), Math.max(1, Math.ceil(r.w * scale)), Math.max(1, Math.ceil(r.h * scale))).data;
          let worst = Infinity;
          for (let i = 0; i < data.length; i += 4) {
            const bg = { r: data[i], g: data[i + 1], b: data[i + 2], a: 1 };
            const fg = ink.a < 1 ? M.compositeOver(ink, bg) : ink;
            worst = Math.min(worst, M.contrastRatio(fg, bg));
          }
          rows.push({ sel: t.sel, text: r.text, ratio: M.round2(worst), needs: M.isLargeText(r.px, r.weight) ? 3 : 4.5 });
        }
      }
      return rows;
    },
    { b64: buf.toString("base64"), targets }
  );
}

module.exports = { worstContrast };
