// A LOCAL NOTICE IS THE APP TALKING ABOUT ITSELF, NEVER AN ANSWER FROM RICH.
//
// Ray's candidate-.4 walk, finding #8: a voice notice was written into the model as
// `rich_message`, so the shipping renderer stamped it with Rich's byline and avatar and gave
// it a **Copy Rich's message** button (`timeline.js`'s `addLocalNotice` and
// `renderRichMessage`). Reading back, a microphone status line was indistinguishable from
// something Rich said — and the two are not the same kind of thing at all: one is in the
// ledger and is evidence of what he was told, the other is gone on the next snapshot.
//
// WHAT THIS SUITE PINS, and every negative carries a positive control in the same check,
// because a scan for an absent byline passes trivially on a page with nothing on it:
//
//   1. A notice renders with NO Rich identity and NO copy control — and a real Rich message
//      rendered in the SAME page does have both, so the absence is a fact about the notice.
//   2. A screen reader is told what it is, in the slot where a message says "Rich said".
//   3. The words are unchanged. This fix moves a sentence out of speech; it does not edit it.
//   4. CONTRAST, computed from WebKit's own resolved colors in BOTH themes: the body against
//      the surface it sits on (4.5:1, normal text) and the left rule that marks it (3:1,
//      non-text indicator). Nothing here is declared exempt — a line telling him his words
//      are back in the box is a line he is expected to read.

"use strict";

const { loadPlaywright, openFixture, createRun, assert, assertEqual } = require("./lib/harness");
const F = require("./lib/fixtures");

/// The smallest real snapshot: one turn, one thing Rich actually said. It is the POSITIVE
/// CONTROL for every absence below.
function snapshotWithRichSpeaking() {
  return {
    entityId: "northwind",
    threadId: "thr_fem",
    mode: "ceo",
    bindingRevision: 1,
    items: [F.richMessage(0, "Pulling the comparables now.", 0)],
  };
}

/// The ratio WebKit actually paints, alpha-composited against the first opaque ancestor —
/// never the value the stylesheet names. The same arithmetic `background-work.js` uses.
function ratioHelper() {
  return `
    window.__ratio = (selector, prop) => {
      const node = document.querySelector(selector);
      if (!node) throw new Error("no node for " + selector);
      const parse = (value) => {
        const n = value.match(/[\\d.]+/g).map(Number);
        return { r: n[0], g: n[1], b: n[2], a: n.length > 3 ? n[3] : 1 };
      };
      let background = null;
      for (let el = node.parentElement; el; el = el.parentElement) {
        const c = parse(getComputedStyle(el).backgroundColor);
        if (c.a === 1) { background = c; break; }
      }
      if (!background) background = { r: 255, g: 255, b: 255, a: 1 };
      const own = parse(getComputedStyle(node).backgroundColor);
      if (own.a === 1) background = own;
      const fg = parse(getComputedStyle(node)[prop || "color"]);
      const flat = {
        r: fg.r * fg.a + background.r * (1 - fg.a),
        g: fg.g * fg.a + background.g * (1 - fg.a),
        b: fg.b * fg.a + background.b * (1 - fg.a),
      };
      const channel = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
      const lum = (c) => 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
      const a = lum(flat), b = lum(background);
      return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
    };
    window.__fontPx = (selector) => parseFloat(getComputedStyle(document.querySelector(selector)).fontSize);
    window.__noticeRule = () => {
      const node = document.querySelector(".tl-notice");
      const s = getComputedStyle(node);
      return { color: s.borderLeftColor, width: parseFloat(s.borderLeftWidth) };
    };
  `;
}

const NOTICE = "I can't hear anything. Check your mic isn't muted, then try again.";

/// Render one real Rich message and one local notice on the same page, in `theme`.
async function withBoth(browser, theme) {
  const page = await openFixture(browser);
  await page.addScriptTag({ content: ratioHelper() });
  await page.evaluate(
    ([snapshot, text, mode]) => {
      document.documentElement.dataset.theme = mode;
      window.__render(snapshot, {});
      window.RichTimeline.addLocalNotice(window.__model, text, 1787950000000);
      window.__renderOnly();
    },
    [snapshotWithRichSpeaking(), NOTICE, theme]
  );
  return page;
}

async function main() {
  const run = createRun("a local notice is a status line, never something Rich said");
  const browser = await loadPlaywright().webkit.launch();

  await run.check("a notice carries no Rich identity and no copy control — and a message does", async () => {
    const page = await withBoth(browser, "dark");
    const notice = page.locator(".tl-notice");
    assertEqual(await notice.count(), 1, "the notice did not render at all");

    // THE DEFECT, refused: no byline, no avatar, no Copy — inside the notice.
    assertEqual(await notice.locator(".tl-who").count(), 0, "the notice still carries Rich's byline");
    assertEqual(await notice.locator(".tl-avatar").count(), 0, "the notice still carries Rich's avatar");
    assertEqual(await notice.locator("button").count(), 0, "the notice still offers a copy control");
    assertEqual(await page.locator(".tl-notice.tl-rich, .tl-rich .tl-notice").count(), 0, "it is still a message");

    // POSITIVE CONTROL, on the same page and the same selectors: a real Rich message DOES
    // have all three, so the three absences above are facts about the notice rather than
    // about a page where nothing rendered.
    const message = page.locator("article.tl-rich");
    assertEqual(await message.count(), 1, "the control message did not render");
    assertEqual(await message.locator(".tl-who").count(), 1, "the control message has no byline — it proves nothing");
    assert(
      (await message.locator("button.tl-mini-btn").count()) === 1,
      "the control message has no copy control — the absence above proves nothing"
    );
    assertEqual(page.__errors.length, 0, "the renderer logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "notice: 0 bylines, 0 avatars, 0 controls; the Rich message beside it: 1 byline, 1 copy control";
  });

  await run.check("a screen reader is told what it is, where a message says Rich said", async () => {
    const page = await withBoth(browser, "dark");
    const label = await page.locator(".tl-notice .sr-only").first().innerText();
    assertEqual(label.trim(), "RichOS status", "the notice's accessible label");
    const said = await page.locator("article.tl-rich .sr-only").first().innerText();
    assertEqual(said.trim(), "Rich said", "the message's label changed — the pair is the point");
    const role = await page.locator(".tl-notice").getAttribute("role");
    assertEqual(role, "note", "a status line is a note, not a log entry");
    await page.close();
    return `"RichOS status" beside "Rich said", role=note`;
  });

  await run.check("the words are unchanged — this moves a sentence, it does not edit one", async () => {
    const page = await withBoth(browser, "dark");
    const text = (await page.locator(".tl-notice").innerText()).replace("RichOS status", "").trim();
    assertEqual(text, NOTICE, "the notice's own words were rewritten on the way out of the message lane");
    await page.close();
    return "the sentence renders verbatim";
  });

  await run.check("a background result SURVIVES his next sentence — Frank's defect (a)", async () => {
    // **THE DEFECT, in the shipped build.** A background result is handed over ONCE:
    // `take_work_notices` marks it delivered in the durable register (`assignment.rs`), the
    // shell renders it through `addLocalNotice`, and his next sentence ends in
    // `rich://turn-completed`, which calls `loadTimeline()` -> `applySnapshot`, which cleared
    // `model.items` outright. The result was off the screen AND already marked delivered, so
    // nothing would ever show it again.
    const page = await openFixture(browser);
    const first = snapshotWithRichSpeaking();
    await page.evaluate(
      ([snapshot, text]) => {
        window.__render(snapshot, {});
        window.RichTimeline.addLocalNotice(window.__model, text, 1787948600000);
      },
      // The sentence the backend actually produces for a finished job since the CEO's ruling
      // §52 (`assignment::says::settled`): he hears the OUTCOME. A fixture quoting the old
      // "is ready for you to approve" would have been a notice this product can no longer
      // raise, which is a worse thing for a survival test to be carrying than a short one.
      // Re-quoted 2026-09-19 for Ray's candidate-.11 §1.1: the title now CLOSES and the
      // verdict is its own sentence about "It", because a title in his own terms is usually
      // an imperative and cannot be the subject of one ("and land it is finished").
      [first, "Landing the three branches. It is finished. It landed on cc/echo-1 in project."]
    );
    await page.evaluate(() => window.__renderOnly());
    assertEqual(await page.locator(".tl-notice").count(), 1, "the result did not render at all");

    // HIS NEXT SENTENCE: a new turn arrives and the thread is reloaded, exactly as
    // `rich://turn-completed` does it.
    await page.evaluate(() => {
      const later = {
        entityId: "northwind",
        threadId: "thr_fem",
        mode: "ceo",
        bindingRevision: 1,
        items: [
          {
            id: "turn_two:text:0",
            entityId: "northwind",
            threadId: "thr_fem",
            turnId: "turn_two",
            bindingRevision: 1,
            createdAt: 1787949900000,
            sequence: 0,
            slot: "stream",
            visibility: "ceo",
            kind: "rich_message",
            phase: "unknown",
            text: "On it.",
          },
        ],
      };
      window.RichTimeline.applySnapshot(window.__model, later);
      window.__renderOnly();
    });
    assertEqual(
      await page.locator(".tl-notice").count(),
      1,
      "his next sentence destroyed a background result he had already been handed"
    );
    assert(
      (await page.locator(".tl-notice").innerText()).includes("It landed on cc/echo-1 in project"),
      "the result survived as something else"
    );

    // AND IT IS IN TIME ORDER, not shunted to the end: the notice was raised before that
    // turn, so it renders before it. Appending would have said "this just happened".
    const order = await page.evaluate(() =>
      Array.from(document.querySelectorAll("#messages .tl-notice, #messages article.tl-rich")).map((n) =>
        n.classList.contains("tl-notice") ? "notice" : "message"
      )
    );
    // The reload's snapshot is the truth for LEDGER rows, so the first turn's message is gone
    // with it — the notice and the new turn are what remain, and the notice comes first
    // because it happened first. Appending would have put it under "On it." and said "this
    // just happened".
    assertEqual(JSON.stringify(order), JSON.stringify(["notice", "message"]), "order: " + order.join(","));
    assertEqual(page.__errors.length, 0, "the renderer logged errors: " + page.__errors.join(" | "));
    await page.close();
    return "the result is still on screen after a reload, and it sits where it happened";
  });

  await run.check("after a relaunch a held result lands where it happened, not under the newest answer", async () => {
    // **AUDIT-7 ROW 9, and it is the RELAUNCH case rather than the reload case above.** Within
    // one session a notice is CARRIED across the snapshot and keeps the `createdAt` it was born
    // with, which the check above pins. Across a relaunch nothing is carried: the thread is
    // rebuilt from the ledger and `take_work_notices` hands back everything he has not been
    // told, each `PendingNotice` carrying its own `raisedAtMs`. `main.js` read `.text` and threw
    // the rest away, so every one was stamped `Date.now()` and `addLocalNotice` pushed it to the
    // END of `turnOrder`.
    //
    // What Ray saw: two assignments failed at `08:13:10Z` and about `08:14`; he asked "name the
    // capital of France" and was answered at about `08:16`; the app was relaunched at
    // `08:19:35Z`; and both failure cards came back AFTER "Paris." — "his scrollback no longer
    // matches the order of events". This is that, at the same three timestamps.
    const page = await openFixture(browser);
    const T_FIRST = 1787948400000; // he asks, and is answered
    const T_RAISED = 1787948590000; // the job fails, three minutes later
    const T_SECOND = 1787948760000; // he asks something else, and is answered
    const say = (turnId, at, text) => ({
      id: turnId + ":text:0",
      entityId: "northwind",
      threadId: "thr_fem",
      turnId,
      bindingRevision: 1,
      createdAt: at,
      sequence: 0,
      slot: "stream",
      visibility: "ceo",
      kind: "rich_message",
      phase: "unknown",
      text,
    });
    // The thread as a relaunch rebuilds it: both answers from the ledger, no notices at all.
    await page.evaluate(
      ([items]) => {
        window.__render(
          { entityId: "northwind", threadId: "thr_fem", mode: "ceo", bindingRevision: 1, items },
          {}
        );
      },
      [[say("turn_one", T_FIRST, "Twelve."), say("turn_two", T_SECOND, "Paris.")]]
    );
    assertEqual(await page.locator("article.tl-rich").count(), 2, "the rebuilt thread does not hold both answers");

    // ...and then the durable drain hands the held failure over, with the time it was RAISED.
    await page.evaluate(
      ([text, at]) => {
        window.RichTimeline.addLocalNotice(window.__model, text, at);
        window.__renderOnly();
      },
      ["Landing the three branches stopped before it finished.", T_RAISED]
    );

    // The message's own words, not `innerText`'s first line — that is the screen-reader-only
    // "Rich said", which would have made this read `Rich said | notice | Rich said` and told a
    // reader nothing about WHICH answer the card landed between.
    const order = await page.evaluate(() =>
      Array.from(document.querySelectorAll("#messages .tl-notice, #messages article.tl-rich")).map((n) =>
        n.classList.contains("tl-notice")
          ? "notice"
          : ((n.querySelector(".tl-prose") || {}).textContent || "").trim()
      )
    );
    assertEqual(
      JSON.stringify(order),
      JSON.stringify(["Twelve.", "notice", "Paris."]),
      "the held failure is not where it happened — his scrollback does not match the order of events. Got: " +
        order.join(" | ")
    );

    // AND A NOTICE RAISED NOW STILL GOES AT THE END, which is the ordinary live case and the
    // thing a fix for the above could easily have broken.
    await page.evaluate(() => {
      window.RichTimeline.addLocalNotice(window.__model, "I'm listening now.", Date.now());
      window.__renderOnly();
    });
    const withLive = await page.evaluate(() =>
      Array.from(document.querySelectorAll("#messages .tl-notice, #messages article.tl-rich")).map((n) =>
        (
          (n.querySelector(".tl-notice-body") || n.querySelector(".tl-prose") || {}).textContent || ""
        ).trim()
      )
    );
    assertEqual(
      withLive[withLive.length - 1],
      "I'm listening now.",
      "a notice raised NOW no longer lands at the end of the thread: " + withLive.join(" | ")
    );
    assertEqual(page.__errors.length, 0, "the renderer logged errors: " + page.__errors.join(" | "));
    await page.close();
    return (
      "a failure raised between two answers comes back between them after a relaunch " +
      "(Twelve. / notice / Paris.), and one raised now still comes last"
    );
  });

  for (const theme of ["dark", "light"]) {
    await run.check(`the notice clears WCAG AA in ${theme} mode, computed`, async () => {
      const page = await withBoth(browser, theme);
      const body = await page.evaluate(() => window.__ratio(".tl-notice-body"));
      const px = await page.evaluate(() => window.__fontPx(".tl-notice-body"));
      // The left rule is what marks this as a notice rather than a message: a non-text
      // indicator, floor 3:1.
      const rule = await page.evaluate(() => window.__ratio(".tl-notice", "borderLeftColor"));
      const width = (await page.evaluate(() => window.__noticeRule())).width;
      assert(body >= 4.5, `${theme}: the notice body is ${body.toFixed(2)}:1, under the 4.5:1 floor`);
      assert(px >= 16, `${theme}: the notice body is ${px}px, under the 16px readable floor`);
      assert(rule >= 3, `${theme}: the notice's left rule is ${rule.toFixed(2)}:1, under the 3:1 floor`);
      assert(width >= 2, `${theme}: the left rule is ${width}px — too thin to be the marker it is`);
      await page.close();
      return `body ${body.toFixed(2)}:1 at ${px}px; left rule ${rule.toFixed(2)}:1 at ${width}px`;
    });
  }

  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
