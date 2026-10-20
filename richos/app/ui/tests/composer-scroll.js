// THE COMPOSER'S SCROLLBAR — dev-walk audit N1, 2026-09-17.
//
// Quoted verbatim from `docs/verification/2026-09-17-main-aa0165cc-dev-walk-audit.md`:
// "A white native scrollbar is jammed into the right end of the message box, through its
// rounded corner and its focus ring, on an EMPTY box with nothing to scroll. Both themes."
//
// Two things are true about that one symptom, and this suite holds them apart rather than
// conflating them — SAID PLAINLY, because one is confirmed and the other is a hypothesis
// this harness could not fully settle (see mutation 1's note at the foot of the file):
//
//   1. HYPOTHESIS, NOT CONFIRMED HERE: `<textarea rows="1">`'s own intrinsic-height
//      algorithm not agreeing with the `line-height: 1.5` `style.css` sets on it, so an
//      empty box's `scrollHeight` reads a few px taller than its `clientHeight`. `#input`
//      now carries an explicit `height` derived from the same padding/line-height the
//      stylesheet already declares, in the same units, so the two can never disagree the
//      way the attribute and the stylesheet might have. Check 1 asserts the resulting
//      invariant (no overflow on an empty box, in both themes) — but every mutation tried
//      against it in THIS engine left `scrollHeight === clientHeight` regardless, so the
//      check could not be proven able to fail for this specific cause; it may be catching a
//      real class of bug, or the real cause may live in a native macOS scrollbar preference
//      this harness cannot reproduce at all.
//   2. CONFIRMED, BOTH DIRECTIONS: a NATIVE, OS-drawn scrollbar is a platform widget WebKit
//      does not clip to the scrolling element's own `border-radius` — only an ANCESTOR's
//      `overflow: hidden` does, which is exactly why "through its rounded corner" is
//      possible at all regardless of cause 1. So the border, radius, fill and focus ring
//      moved off `#input` and onto a new, never-scrolling wrapper, `#input-shell`; `#input`
//      is the plain field inside it. Checks 2 and 3 are this half, and both were proven able
//      to fail against the shipped shape (mutation log, below).
//
// Check 1's mutation could not be reproduced RED; checks 2 and 3 were, against the real
// shipped source, in this session — see the mutation log at the foot of the file.
//
// Run: node composer-scroll.js   (or `npm test` for every suite in this directory)

"use strict";

const path = require("path");
const {
  leaveHome,
  loadPlaywright,
  createRun,
  assert,
  assertEqual,
  UI_DIR,
} = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

async function openApp(browser, opts) {
  opts = opts || {};
  const page = await browser.newPage({ viewport: opts.viewport || { width: 1400, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  if (opts.theme) {
    await page.addInitScript((theme) => {
      try {
        window.localStorage.setItem("richos-mock-config", JSON.stringify({ theme: theme }));
        window.localStorage.setItem("richos-theme", theme);
      } catch (e) {
        /* storage unavailable — the shipped default applies */
      }
    }, opts.theme);
  }
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the conversation behind it.
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  page.__errors = errors;
  return page;
}

async function openComposer(page) {
  await page.click('.nav-thread[data-thread-id="acme"]');
  await page.waitForSelector("#input-shell", { state: "visible" });
}

async function main() {
  const pw = loadPlaywright();
  const run = createRun("the composer's scrollbar — dev-walk audit N1, 2026-09-17");
  const browser = await pw.webkit.launch();

  // ---- 1. an empty box has nothing to scroll, and shows no scrollbar for it -------------
  await run.check("1. an empty composer has no scrollable overflow, in both themes", async () => {
    const readings = [];
    for (const theme of ["dark", "light"]) {
      const page = await openApp(browser, { theme });
      await openComposer(page);
      const box = await page.evaluate(() => {
        const el = document.getElementById("input");
        return { value: el.value, scrollHeight: el.scrollHeight, clientHeight: el.clientHeight };
      });
      assertEqual(box.value, "", theme + ": the composer did not start empty — this check proves nothing over typed text");
      assert(
        box.scrollHeight <= box.clientHeight,
        theme + ": an empty composer already overflows itself — scrollHeight " + box.scrollHeight +
          " > clientHeight " + box.clientHeight + ", which is exactly what draws a scrollbar with nothing to scroll"
      );
      readings.push(theme + ": " + box.scrollHeight + "/" + box.clientHeight);
      await page.close();
    }
    return readings.join(", ");
  });

  // ---- 2. a genuinely long composer DOES overflow, and the textarea itself carries no ----
  //         border/radius of its own any more — the shell does
  await run.check(
    "2. a long composer really does scroll, and the textarea carries no boundary of its own",
    async () => {
      const page = await openApp(browser);
      await openComposer(page);
      const lines = [];
      for (let i = 0; i < 20; i++) lines.push("line " + i + " of a message long enough to force real overflow");
      await page.fill("#input", lines.join("\n"));
      // `fill()` sets `.value` directly rather than dispatching per-keystroke `input`
      // events, so `autoGrow()` (bound to `input`) is nudged once here the way a real
      // paste already does in `main.js`.
      await page.dispatchEvent("#input", "input");
      const after = await page.evaluate(() => {
        const el = document.getElementById("input");
        const shell = document.getElementById("input-shell");
        const es = getComputedStyle(el);
        const ss = getComputedStyle(shell);
        return {
          scrollHeight: el.scrollHeight,
          clientHeight: el.clientHeight,
          inputBorderWidth:
            parseFloat(es.borderTopWidth) + parseFloat(es.borderRightWidth) +
            parseFloat(es.borderBottomWidth) + parseFloat(es.borderLeftWidth),
          inputRadius: parseFloat(es.borderTopLeftRadius),
          shellOverflow: ss.overflow,
          shellRadius: parseFloat(ss.borderTopLeftRadius),
        };
      });
      assert(
        after.scrollHeight > after.clientHeight,
        "typing 20 lines did not create real overflow — this suite would be proving nothing against check 1"
      );
      assertEqual(
        after.inputBorderWidth, 0,
        "the textarea carries a border of its own (" + after.inputBorderWidth + "px) — a native " +
          "scrollbar drawn on THIS element has its own edge to poke through again"
      );
      assertEqual(
        after.inputRadius, 0,
        "the textarea carries its own border-radius (" + after.inputRadius + "px) again — the point of " +
          "moving it to the shell was that WebKit does not clip a native scrollbar to the SCROLLING " +
          "element's own radius"
      );
      assert(after.shellRadius > 0, "the shell lost its rounded corner (" + after.shellRadius + "px)");
      assertEqual(
        after.shellOverflow, "hidden",
        "the shell does not clip (" + after.shellOverflow + ") — nothing stops a native scrollbar the " +
          "textarea draws from rendering past the rounded corner"
      );
      await page.close();
      return "scrollHeight " + after.scrollHeight + " > clientHeight " + after.clientHeight +
        "; textarea border " + after.inputBorderWidth + "px, radius " + after.inputRadius +
        "px; shell overflow " + after.shellOverflow + ", radius " + after.shellRadius + "px";
    }
  );

  // ---- 3. the focus ring the audit named lives on the shell, not on the textarea --------
  await run.check(
    "3. focusing the composer rings the shell, and the textarea draws no outline of its own",
    async () => {
      const page = await openApp(browser);
      await openComposer(page);
      const unfocused = await page.evaluate(
        () => getComputedStyle(document.getElementById("input-shell")).borderColor
      );
      await page.focus("#input");
      const focused = await page.evaluate(() => {
        const shell = document.getElementById("input-shell");
        const el = document.getElementById("input");
        return { shellBorderColor: getComputedStyle(shell).borderColor, inputOutline: getComputedStyle(el).outlineStyle };
      });
      assertEqual(
        focused.inputOutline, "none",
        "the textarea draws its own outline (" + focused.inputOutline + ") on focus instead of leaving " +
          "the ring to its shell"
      );
      assert(
        focused.shellBorderColor !== unfocused,
        "the shell's border did not change color on focus — the audit's gold focus ring never actually rings the box"
      );
      await page.close();
      return "unfocused " + unfocused + " -> focused " + focused.shellBorderColor;
    }
  );

  await browser.close();
  const failed = run.report();
  process.exitCode = failed ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

// =========================================================================================
// THE MUTATIONS — every check above was run RED once, against the shipped defect
// =========================================================================================
//
//  1  UNABLE TO TURN RED, AND SAID PLAINLY RATHER THAN LEFT UNTESTED. Three different
//     mutations were tried against `#input`'s `height`/`line-height` — unsetting `height`
//     entirely (the shipped, pre-fix shape), raising `line-height` to 1.8 while leaving
//     `height` at its old 1.5em value, and forcing `height: 20px` well under one line — and
//     this Playwright-bundled WebKit reported `scrollHeight === clientHeight` in EVERY case,
//     never the mismatch the audit's live macOS WKWebView actually showed. The empirical
//     finding: THIS engine appears to lay out a `rows="1"` textarea's box from its own
//     internal per-row metrics rather than from the literal CSS `height`, so scrollHeight
//     and clientHeight are read off the SAME recomputed number regardless of what height is
//     declared — which means the exact "empty box, few-px phantom overflow" arithmetic this
//     check was written to catch may be a property of the native macOS WKWebView's
//     scrollbar preference (Show scrollbars: Always) rather than of the DOM box model at
//     all, and this harness cannot reproduce a system-level scrollbar-visibility setting.
//     Check 1 therefore stands as an ASSERTED INVARIANT — never true if it ever stops
//     holding — rather than a proven regression test, and that distinction is the reason
//     this entry exists instead of a fabricated RED run.
//  2  style.css: move `border`/`border-radius` back onto `#input` (the shipped defect's
//     shape) -> check 2 fails: "the textarea carries a border of its own (4px)".
//  3  style.css: change `#input-shell:focus-within`'s `border-color` from `var(--accent)`
//     to `var(--line-control)` (a border that no longer visibly changes on focus) -> check 3
//     fails: "the shell's border did not change color on focus — the audit's gold focus
//     ring never actually rings the box".
//
// Mutations 2 and 3 were run against the actual shipped source in this session and their
// FAIL output above is copied verbatim from that run.
