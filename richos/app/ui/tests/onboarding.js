// THE FIRST-RUN NOTICE — the offer he can SEE, and the two answers it can take.
//
// WHY THIS SUITE EXISTS. `docs/verification/onboarding-honesty-2026-09-06/` measured the
// bootstrap interview being offered — inside a REPLY (cell D1), which requires him to type
// something first and to read what comes back. Its own "What is NOT built" list names the
// consequence: *"Rich makes the offer in conversation; a CEO who does not read the first reply
// never sees it."* Driven on the shipping shell, what a first-run user lands on after the
// three setup dialogs is an empty conversation pane. Nothing on it says RichOS knows nothing
// about his business, that there is a twenty-minute interview, or that he may decline.
//
// WHAT IT HOLDS, and each is a clause of the contract rather than a nicety:
//
//   1. THE OFFER IS ON SCREEN WITHOUT HIM TYPING ANYTHING, and it is not on screen for an
//      install that has been described. A surface that painted in both states would be one
//      nobody could turn off.
//   2. "NOT NOW" REACHES DISK. The command is issued, and its ARGUMENTS and its call count are
//      read rather than inferred from what changed on screen — `record_declination` shipped
//      with no caller at all, so "the panel closed" is exactly the evidence that would have
//      passed over the defect this closes.
//   3. A REFUSED WRITE KEEPS THE NOTICE OPEN AND SAYS SO. Closing it over a failed write is
//      reporting success over work that did not happen.
//   4. "START" PUTS A REAL TURN IN THE CONVERSATION, through the same `send()` the composer
//      uses. There is no private route into the interview for a second thing to drift from.
//   5. THE UNUSABLE STATE DRAWS NO BUTTON, because no button could fix it, and its sentences
//      come from Rust rather than from this window.
//   6. NO SENTENCE ON THIS SURFACE PROMISES WHAT THE CHAIN DOES NOT DO — no staffing, and
//      nothing about his own data appearing on the home screen.
//   7. THE MOCK'S COPIES OF THE BACKEND'S SENTENCES STILL MATCH `main.rs`.
//   8. TYPE AND CONTRAST FLOORS on every line of it, computed in the page in both themes.
//
// Run: node onboarding.js   (or `npm test` for every suite in this directory)

"use strict";

const fs = require("fs");
const path = require("path");
const {
  leaveHome,
  loadPlaywright,
  createRun,
  assert,
  assertEqual,
  rustSentenceAfter,
  shot,
  UI_DIR,
} = require("./lib/harness");
const { parseCssColor, compositeOver, contrastRatio, round2 } = require("./lib/contrast");
const { paneGround } = require("./lib/pane-ground");

const APP = "file://" + path.join(UI_DIR, "index.html");
const MAIN_RS = fs.readFileSync(path.join(UI_DIR, "..", "src-tauri", "src", "main.rs"), "utf8");
const MOCK_JS = fs.readFileSync(path.join(UI_DIR, "mock.js"), "utf8");
const MAIN_JS = fs.readFileSync(path.join(UI_DIR, "main.js"), "utf8");

/// Open the shell with an onboarding preset, recording every `invoke` so a press can be read
/// from its ARGUMENTS rather than from what changed on screen.
async function openApp(browser, preset, theme) {
  theme = theme || "dark";
  const page = await browser.newPage({ viewport: { width: 1400, height: 950 }, colorScheme: theme });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  await page.addInitScript(() => {
    let real = null;
    window.__calls = [];
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      get() {
        return real;
      },
      set(v) {
        real = v;
        const orig = v.invoke.bind(v);
        v.invoke = function (cmd, args) {
          window.__calls.push({ cmd, args });
          return orig(cmd, args);
        };
      },
    });
  });
  await page.addInitScript((p) => {
    window.__RICHOS_MOCK_PRESET__ = p;
  }, preset);
  // The app's own preference decides the palette and the STORE wins over the mirror — the
  // reasoning is in contrast.js, and seeding only `richos-theme` measures the wrong theme.
  await page.addInitScript((t) => {
    window.localStorage.setItem("richos-theme", t);
    window.localStorage.setItem("richos-font-scale", "100");
    window.localStorage.setItem(
      "richos-mock-config",
      JSON.stringify({ theme: t, font_scale: 100, user_name: null })
    );
  }, theme);
  await page.goto(APP);
  await leaveHome(page);
  page.__errors = errors;
  return page;
}

const noticeText = (page) =>
  page.evaluate(() =>
    (document.getElementById("first-run").innerText || "").replace(/\s+/g, " ").trim()
  );

async function main() {
  const run = createRun("the first-run notice — the offer he can see, and the two answers it takes");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  let assertions = 0;
  const bump = (n) => {
    assertions += n;
    return n;
  };

  // -------------------------------------------------------------------------------------
  // 1. It is on screen without him typing — AND the negative control comes with it.
  // -------------------------------------------------------------------------------------

  await run.check("0  the pane ground is composited through transparent layers to html, never read as black", async () => {
    const g = paneGround(["rgba(0, 0, 0, 0)", "rgba(0, 0, 0, 0)", "rgb(234, 230, 221)"]);
    assertEqual([g.r, g.g, g.b], [234, 230, 221], "transparent pane and body over an html ground");
    assertEqual(paneGround(["rgb(10, 20, 30)", "rgb(234, 230, 221)"]).r, 10, "an opaque pane wins");
    return "transparent layers show the html ground behind them";
  });

  await run.check("1  an un-described company is offered the interview, unprompted", async () => {
    const page = await openApp(browser, { onboarding: "not-yet" });
    await page.waitForSelector("#first-run:not([hidden])");
    // NOTHING WAS TYPED AND NOTHING WAS SENT. That is the whole difference from the offer
    // that already exists: cell D1's arrives in a reply to a message the CEO wrote.
    assertEqual(await page.inputValue("#input"), "", "he typed nothing");
    assert(
      !(await page.evaluate(() => window.__calls.some((c) => c.cmd === "send_message"))),
      "and nothing was sent — the notice must not depend on a turn"
    );
    const text = await noticeText(page);
    assert(/twenty minutes/.test(text), "it names the cost in his own units: " + text);
    assert(/stop partway/.test(text), "and that stopping partway is fine: " + text);
    assert(
      await page.isVisible("#first-run-start"),
      "with a control that starts it, or it is a statement rather than an offer"
    );
    bump(5);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await shot(page, "first-run-offer");
    await page.close();
    return "offered on arrival, with nothing typed and nothing sent";
  });

  await run.check("1b  a described company is NOT offered, and neither is a declined one", async () => {
    // THE NEGATIVE CONTROL, and it is what makes check 1 mean anything: a notice that painted
    // in every state would pass check 1 and be a permanent bar nobody could clear.
    for (const state of ["described", "declined", "no-central-folder"]) {
      const page = await openApp(browser, { onboarding: state });
      await page.waitForFunction(() => window.__calls.some((c) => c.cmd === "onboarding_view"));
      assert(
        await page.isHidden("#first-run"),
        "state `" + state + "` must render nothing at all, and it rendered: " + (await noticeText(page))
      );
      bump(1);
      await page.close();
    }
    return "described, declined and no-central-folder each render nothing";
  });

  // -------------------------------------------------------------------------------------
  // 2. "Not now" reaches disk — read from the call, never from the panel closing.
  // -------------------------------------------------------------------------------------

  await run.check('2  "Not now" issues the write, exactly once, and says what it did', async () => {
    const page = await openApp(browser, { onboarding: "not-yet" });
    await page.waitForSelector("#first-run:not([hidden])");
    // WHAT THE BUTTON PROMISES, BEFORE IT IS PRESSED. The label is two syllables and the
    // effect is durable, so the effect is stated in words beside it or the label is a lie.
    const before = await noticeText(page);
    assert(/stop offering/.test(before), "the consequence must be on screen: " + before);
    assert(/any time/.test(before), "and so must the way back: " + before);

    await page.click("#first-run-later");
    await page.waitForFunction(() => window.__calls.some((c) => c.cmd === "decline_onboarding"));
    const declines = await page.evaluate(
      () => window.__calls.filter((c) => c.cmd === "decline_onboarding").length
    );
    assertEqual(declines, 1, "one press, one write");
    // AND THE PRODUCT AGREES IT HAPPENED — the mock moves the same state the real command
    // moves, so this reads the backend's answer rather than the window's opinion of it.
    assertEqual(
      await page.evaluate(async () => (await window.RichBridge.invoke("onboarding_view")).state),
      "declined",
      "the backend must report the declination the press claimed to make"
    );
    const after = await noticeText(page);
    assert(/Ask me any time/.test(after), "the receipt names the way back: " + after);
    assert(await page.isHidden("#first-run-actions"), "and it offers nothing further");
    bump(6);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await shot(page, "first-run-declined");
    await page.close();
    return "one press, one write, a receipt that names the way back";
  });

  // -------------------------------------------------------------------------------------
  // 3. A refused write is said out loud and the offer stays live.
  // -------------------------------------------------------------------------------------

  await run.check("3  a write that failed keeps the offer open and reports itself", async () => {
    const page = await openApp(browser, { onboarding: "not-yet", onboardingDeclineFails: true });
    await page.waitForSelector("#first-run:not([hidden])");
    await page.click("#first-run-later");
    await page.waitForSelector("#first-run-error:not([hidden])");
    const said = (await page.textContent("#first-run-error")).trim();
    assert(/isn't recorded/.test(said), "it must say the thing did not happen: " + said);
    assert(
      !/Library\/Application Support/.test(said) && !/onboarding\.json/.test(said),
      "and it must not put a path on his screen — that half goes to the operator: " + said
    );
    assert(
      await page.isVisible("#first-run-actions"),
      "the offer stays live: a closed notice over a failed write is success reported over work that did not happen"
    );
    assert(
      !(await page.isDisabled("#first-run-later")),
      "and the control he just used is usable again"
    );
    bump(4);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "the notice stays open, names the failure, and re-arms";
  });

  // -------------------------------------------------------------------------------------
  // 4. "Start" is an ordinary message on the ordinary path.
  // -------------------------------------------------------------------------------------

  await run.check("4  Start sends a real turn through send_message, and no private route exists", async () => {
    const page = await openApp(browser, { onboarding: "not-yet" });
    await page.waitForSelector("#first-run:not([hidden])");
    await page.click("#first-run-start");
    await page.waitForFunction(() => window.__calls.some((c) => c.cmd === "send_message"));
    const sent = await page.evaluate(
      () => window.__calls.filter((c) => c.cmd === "send_message").map((c) => c.args.text)
    );
    assertEqual(sent.length, 1, "one press, one message");
    assert(/twenty minutes/.test(sent[0]), "and it is the acceptance, in his voice: " + sent[0]);
    // IT LANDS IN THE RECORD AS SOMETHING HE SAID, because he did say it — he pressed a
    // button whose label is exactly this.
    await page.waitForFunction((t) => document.getElementById("messages").innerText.includes(t), sent[0]);
    assert(await page.isHidden("#first-run"), "and the notice steps out of the way");
    assertEqual(await page.inputValue("#input"), "", "leaving no draft behind in the composer");
    // NO SECOND DOOR. `main.js` may reach the interview through the composer and nowhere
    // else; a private invoke would be a second thing to keep in step with `OFFER_BLOCK`.
    assertEqual(
      (MAIN_JS.match(/bootstrap-interview/g) || []).length,
      0,
      "the window must not name the skill: the offer block already tells Rich what to use"
    );
    bump(6);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "one ordinary message, one turn in the record, no private route";
  });

  // -------------------------------------------------------------------------------------
  // 5. The one state that needs a person.
  // -------------------------------------------------------------------------------------

  await run.check("5  unusable notes are stated, and no button is drawn for them", async () => {
    const page = await openApp(browser, { onboarding: "unusable" });
    await page.waitForSelector("#first-run:not([hidden])");
    const text = await noticeText(page);
    assert(/couldn't read/.test(text), "it says what happened: " + text);
    assert(/won't guess/.test(text), "and that nothing was invented in its place: " + text);
    assert(/Whoever set RichOS up/.test(text), "and whose job the fix is: " + text);
    assert(await page.isHidden("#first-run-actions"), "no control for a thing no control can do");
    assert(
      !/twenty minutes/.test(text),
      "and it must not offer an interview over notes it cannot read: " + text
    );
    bump(5);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await shot(page, "first-run-unusable");
    await page.close();
    return "stated, attributed, and offering nothing it cannot do";
  });

  // -------------------------------------------------------------------------------------
  // 6. It promises nothing the built chain does not do.
  // -------------------------------------------------------------------------------------

  await run.check("6  no sentence here promises staffing or a corpus he will not get", async () => {
    const page = await openApp(browser, { onboarding: "not-yet" });
    await page.waitForSelector("#first-run:not([hidden])");
    const text = await noticeText(page);
    // STAFFING IS THE CEO'S OWN CONSTRAINT ON THIS WORK: a first run that reports it
    // onboarded somebody while having staffed nobody is worse than no onboarding at all.
    // Nothing in the app can spawn a teammate today, so nothing here may imply one.
    for (const word of ["hire", "hiring", "staff", "team of", "specialists"]) {
      assert(!new RegExp(word, "i").test(text), 'the offer must not imply staffing ("' + word + '"): ' + text);
      bump(1);
    }
    // AND HIS OWN DATA WILL NOT APPEAR ON THE HOME SCREEN. What is there is a worked example;
    // a new customer's corpus compiles to zero. Any copy implying otherwise is a lie the
    // product cannot cover.
    for (const word of ["home screen", "dashboard", "appear", "fill in", "populate"]) {
      assert(
        !new RegExp(word, "i").test(text),
        'the offer must not imply his data will show up ("' + word + '"): ' + text
      );
      bump(1);
    }
    // WHAT IT DOES PROMISE, stated positively so this check cannot pass by the copy being
    // empty: the answers get written down and used.
    assert(/write your answers down/.test(text), "the one promise must be there: " + text);
    bump(1);
    await page.close();
    return "no staffing, no corpus, and the one promise it can keep";
  });

  // -------------------------------------------------------------------------------------
  // 7. The mock still quotes the product.
  // -------------------------------------------------------------------------------------

  await run.check("7  the mock's copies of the backend's sentences still match main.rs", async () => {
    const pairs = [
      ["I couldn't read your notes about this company.", "ONBOARDING_UNUSABLE_HEADLINE"],
      ["I'm working without them", "ONBOARDING_UNUSABLE_MESSAGE"],
      ["I couldn't write that down", "ONBOARDING_DECLINE_REFUSED"],
    ];
    for (const [marker, name] of pairs) {
      const shipped = rustSentenceAfter(MAIN_RS, marker);
      const inMock = MOCK_JS.replace(/"\s*\+\s*\n?\s*"/g, "").replace(/\s+/g, " ");
      assert(
        inMock.includes(shipped),
        name + " has drifted from the mock. main.rs says:\n    " + shipped
      );
      bump(1);
    }
    return "3 backend sentences, quoted verbatim by the harness";
  });

  // -------------------------------------------------------------------------------------
  // 8. Type and contrast, computed in the page, in BOTH themes.
  // -------------------------------------------------------------------------------------

  await run.check("8  every line of it clears the type and contrast floors, both themes", async () => {
    // COMPUTED, NOT EYEBALLED, and computed HERE as well as in `contrast.js` — that suite
    // walks this surface as its own driver, and this one is the arithmetic standing beside
    // the copy it belongs to, so a change to either file is answered by a red run in the file
    // that changed. The math is the same WCAG formula, applied to `getComputedStyle`.
    const lines = [];
    for (const theme of ["dark", "light"]) {
      const page = await openApp(browser, { onboarding: "not-yet" }, theme);
      await page.waitForSelector("#first-run:not([hidden])");
      const measured = await page.evaluate(() => {
        const parse = (c) => {
          const m = c.match(/rgba?\(([^)]+)\)/);
          if (!m) return null;
          const p = m[1].split(",").map((x) => parseFloat(x));
          return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 };
        };
        const over = (fg, bg) => ({
          r: fg.a * fg.r + (1 - fg.a) * bg.r,
          g: fg.a * fg.g + (1 - fg.a) * bg.g,
          b: fg.a * fg.b + (1 - fg.a) * bg.b,
          a: 1,
        });
        const lum = (c) => {
          const f = (v) => {
            v /= 255;
            return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
          };
          return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
        };
        const ratio = (a, b) => {
          const la = lum(a);
          const lb = lum(b);
          return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
        };
        // The paint stack behind the notice, resolved the way it is actually painted.
        const panelBg = parse(getComputedStyle(document.getElementById("first-run")).backgroundColor);
        const out = [];
        const nodes = [
          ["#first-run-headline", "text"],
          ["#first-run-body", "text"],
          ["#first-run-consequence", "text"],
          ["#first-run-start", "text"],
          ["#first-run-later", "text"],
        ];
        for (const [sel, kind] of nodes) {
          const el = document.querySelector(sel);
          const cs = getComputedStyle(el);
          const own = parse(cs.backgroundColor);
          const bg = own && own.a > 0.99 ? own : over(own || { r: 0, g: 0, b: 0, a: 0 }, panelBg);
          out.push({
            sel,
            kind,
            px: parseFloat(cs.fontSize),
            weight: cs.fontWeight,
            ratio: Math.round(ratio(over(parse(cs.color), bg), bg) * 100) / 100,
          });
        }
        // The panel's own edge, against the pane behind it — the one non-text indicator on
        // this surface that is not identified by a label.
        const panel = getComputedStyle(document.getElementById("first-run"));
        out.push({
          sel: "#first-run border",
          kind: "indicator",
          px: null,
          weight: null,
          border: panel.borderTopColor,
          layers: [
            getComputedStyle(document.getElementById("conversation")).backgroundColor,
            getComputedStyle(document.body).backgroundColor,
            getComputedStyle(document.documentElement).backgroundColor,
          ],
        });
        return out;
      });
      for (const m of measured) {
        // The edge is measured here, against the ground composited through the transparent
        // layers down to html (hunt R50), not in the page against a body that paints nothing.
        if (m.layers) {
          const ground = paneGround(m.layers);
          m.ratio = round2(contrastRatio(compositeOver(parseCssColor(m.border), ground), ground));
        }
        // 4.5:1 for normal text; 3:1 for a non-text indicator and for LARGE text, which is
        // 24px, or 18.66px at 700+. Nothing on this surface is large, and rounding a tier
        // down is how a "large text" pass gets claimed for text that is not large.
        const large = m.px && (m.px >= 24 || (m.px >= 18.66 && parseInt(m.weight, 10) >= 700));
        const floor = m.kind === "indicator" || large ? 3.0 : 4.5;
        assert(
          m.ratio >= floor,
          theme + " " + m.sel + " is " + m.ratio + ":1 against a floor of " + floor + ":1"
        );
        // AND THE TYPE SCALE (ceo-decisions §15): 16px is the floor for text meant to be
        // easily read, and every line here is meant to be read. Nothing on this surface is
        // declared skippable, so the 14px tier is not available to it.
        if (m.kind === "text") {
          assert(
            m.px >= 16,
            theme + " " + m.sel + " is " + m.px + "px — under the 16px floor for readable text"
          );
        }
        lines.push(theme + " " + m.sel + " " + m.ratio + ":1" + (m.px ? " @" + m.px + "px" : ""));
        bump(m.kind === "text" ? 2 : 1);
      }
      // NOTHING HERE IS EXCUSED. A `data-contrast-exempt` on this surface would be a claim
      // that one of six short lines is not meant to be read closely, and none of them is.
      assertEqual(
        await page.evaluate(() => document.querySelectorAll("#first-run [data-contrast-exempt]").length),
        0,
        "no line of the first-run notice may claim a contrast exemption"
      );
      bump(1);
      await page.close();
    }
    return lines.join("\n          ");
  });

  // -------------------------------------------------------------------------------------
  // 9. The first run, in the order a first-run user actually meets it.
  // -------------------------------------------------------------------------------------

  await run.check("9  it is silent until a company is bound, and arrives when he adds one", async () => {
    // THE ORDER IS THE TEST. Checks 1 and 1b drive states directly; this one walks the chain
    // — no company, the picker, his first company, the conversation — because that is where
    // the two defects this closes were found, and neither was reachable from a preset:
    // the notice painted OVER the company picker, and it did not arrive at all after the
    // picker was answered until `addCompany` was given its own call to the renderer.
    const page = await openApp(browser, { onboarding: "not-yet", chosenEntity: null, memory: "none" });
    await page.waitForSelector("#memory-setup:not([hidden])");
    assert(
      await page.isHidden("#first-run"),
      "onboarding is a question about ONE company, so with none bound there is nothing to ask " +
        "about and nothing may be drawn over the questions that are being asked"
    );
    await page.click("#memory-setup-later");
    await page.waitForSelector("#entity-picker:not([hidden])");
    assert(await page.isHidden("#first-run"), "and still nothing while the picker is open");
    await page.evaluate(() => {
      const b = [...document.querySelectorAll("#entity-picker button")].find((x) => /add/i.test(x.textContent));
      b.click();
    });
    await page.fill("#entity-add-name", "Harborline");
    await page.click("#entity-add-go");
    // AND NOW IT ARRIVES, with no relaunch — the boot-time read found no binding, so this can
    // only come from the company path calling the renderer itself.
    await page.waitForSelector("#first-run:not([hidden])", { timeout: 8000 });
    assert(/twenty minutes/.test(await noticeText(page)), "and it is the offer");
    bump(4);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await shot(page, "first-run-after-adding-a-company");
    await page.close();
    return "silent with no company, silent over the picker, present the moment one is added";
  });

  // -------------------------------------------------------------------------------------
  // 9b. The company's name is shown exactly as he typed it (CEO, 2026-10-13: "I must be able
  //     to spell something like "RchOS" and the capitalization must be preserved correctly").
  // -------------------------------------------------------------------------------------

  await run.check("9b  the company he adds is named exactly as he typed it, in the rail and in search", async () => {
    // `innerText`, not `textContent`: it is the text AS PAINTED, so a CSS `text-transform`
    // that re-capitalizes the name shows up here and not only on screen. A synthetic name
    // with a mixed-case word, the shape of the one he typed.
    const NAME = "Brightwave LABS";
    const page = await openApp(browser, { onboarding: "not-yet", chosenEntity: null, memory: "none" });
    await page.waitForSelector("#memory-setup:not([hidden])");
    await page.click("#memory-setup-later");
    await page.waitForSelector("#entity-picker:not([hidden])");
    await page.evaluate(() => {
      const b = [...document.querySelectorAll("#entity-picker button")].find((x) => /add/i.test(x.textContent));
      b.click();
    });
    await page.fill("#entity-add-name", NAME);
    await page.click("#entity-add-go");
    await page.waitForSelector("#first-run:not([hidden])");
    const rail =await page.evaluate((name) => {
      const label = [...document.querySelectorAll(".nav-group-label")].find((n) => n.textContent.trim() === name);
      return label ? label.innerText.trim() : null;
    }, NAME);
    assertEqual(rail, NAME, "the rail paints the company's name exactly as he typed it");
    await page.keyboard.press("Meta+k");
    await page.waitForSelector("#search-input");
    await page.fill("#search-input", "Brightwave");
    await page.waitForSelector("#search-results .result-group");
    const group = (await page.innerText("#search-results .result-group")).trim();
    assertEqual(group, NAME, "search names the company exactly as he typed it");
    bump(2);
    assert(page.__errors.length === 0, "the shell logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "rail and search both paint " + JSON.stringify(NAME);
  });

  // -------------------------------------------------------------------------------------
  // 10. It sits in the conversation's column, at every width the app has.
  // -------------------------------------------------------------------------------------

  await run.check("10  its edges land on the same verticals as the words below it", async () => {
    // MEASURED, at the three widths `applyBreakpoint` distinguishes. A `max-width` alone put
    // the panel 28px proud of every sentence under it on BOTH sides at 1000px, and hard
    // against the window edge — chrome bolted on above the product rather than the first
    // thing in the conversation. It is a layout assertion because it was a layout defect, and
    // reading it off the boxes is the only way to catch it.
    const lines = [];
    for (const width of [760, 1000, 1400]) {
      const page = await browser.newPage({ viewport: { width, height: 900 }, colorScheme: "dark" });
      page.on("pageerror", (e) => {
        throw e;
      });
      await page.addInitScript(() => {
        window.__RICHOS_MOCK_PRESET__ = { onboarding: "not-yet" };
      });
      await page.goto(APP);
      await leaveHome(page);
      await page.waitForSelector("#first-run:not([hidden])");
      const box = await page.evaluate(() => {
        const panel = document.getElementById("first-run").getBoundingClientRect();
        const msgs = document.getElementById("messages");
        const m = msgs.getBoundingClientRect();
        const pad = parseFloat(getComputedStyle(msgs).paddingLeft);
        const conv = document.getElementById("conversation");
        return {
          left: Math.round(panel.left),
          right: Math.round(panel.right),
          colLeft: Math.round(m.left + pad),
          colRight: Math.round(m.right - pad),
          overflows: conv.scrollWidth > conv.clientWidth,
        };
      });
      assertEqual(box.left, box.colLeft, width + "px: the panel's left edge is off the text column");
      assertEqual(box.right, box.colRight, width + "px: the panel's right edge is off the text column");
      assert(!box.overflows, width + "px: the conversation scrolls sideways");
      lines.push(width + "px " + box.left + "-" + box.right);
      bump(3);
      await page.close();
    }
    return lines.join(", ");
  });

  await browser.close();

  // A suite that verifies little and reports green is worse than no suite — the same floor
  // `memory.js` holds itself to, for the same reason.
  const floor = 40;
  if (assertions < floor) {
    console.log("\nonly " + assertions + " assertions ran, floor is " + floor);
    process.exit(1);
  }
  const failed = run.report();
  console.log(
    failed
      ? "\n" + failed + " check(s) FAILED"
      : "\nthe offer is on screen without a word typed, both answers reach disk, and nothing " +
          "on it promises what the chain cannot do (" + assertions + " assertions)."
  );
  process.exit(failed ? 1 : 0);
}

module.exports = { openApp, noticeText };

if (require.main === module) main().catch((e) => {
  console.error(e);
  process.exit(1);
});

// EVERY CHECK HERE WAS RUN RED, by breaking the shipped source and running this file. Nine
// mutations, nine red runs, each restored afterwards; the check each one actually turned is
// named, because the one that surprised me is the useful entry.
//
//  1   main.js `renderFirstRunNotice`: `box.hidden = true; return;` before the branch
//        -> the offer is invisible again, which is the defect this closes.
//           RED as "only 6 assertions ran, floor is 40" — the assertion floor caught it
//           before any single check did, which is what that floor is for.
//  1b  main.js `renderFirstRunNotice`: the `state !== "not-yet"` guard replaced by `false`
//        -> a described install is offered an interview about material it is already using.
//           RED on check 1b.
//  2   main.js `declineFirstRunInterview`: the invoke no longer awaited
//        -> the panel says it was recorded before anything is written. RED on CHECK 3, not
//           check 2 — the un-awaited promise cannot reject into the catch, so what breaks is
//           the refusal arm. Worth writing down: check 2 alone would have passed this, and
//           `record_declination` spent a day with no caller at all.
//  3   main.js `declineFirstRunInterview`: catch closes the notice instead of reporting
//        -> a failed write reads as success. RED on check 3.
//  4   main.js `startFirstRunInterview`: fills the composer and never calls `send()`
//        -> the acceptance sits in the box and no turn reaches the record. RED on check 4.
//  5   main.js: `first-run-actions` left visible in the `unusable` arm
//        -> a button for a thing no button can do. RED on check 5.
//  6   main.js FIRST_RUN_BODY: "and hire the team you describe"
//        -> a promise the chain cannot keep, which is the CEO's own stated blocker on this
//           work. RED on check 6.
//  7   main.rs ONBOARDING_DECLINE_REFUSED: "write that down" -> "save that", mock untouched
//        -> the window rehearses a sentence the product no longer raises. RED on check 7.
//  8   style.css `.first-run-consequence`: 0.875rem and `--ink-faint`
//        -> 14px under a 16px floor, and the effect of a control in the skippable tier.
//           RED on check 8.
