// **THE FAILURE CARD, WHEN THE FAILURE IS LOCAL** — the 2026-09-17 nightly's D2, rendered
// by the REAL renderer under WebKit, the engine Tauri ships on macOS.
//
// `docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md` §D2. The
// published nightly held this in its own ledger:
//
//     "reason":"cognition protocol: \"Not logged in · Please run /login\""
//
// and showed him this:
//
//     Stopped after 1s
//     I hit a snag mid-thought and had to stop — say the word and I'll pick it back up.
//     Everything I'd already written above is saved.
//     [Pick it back up]
//
// Three untruths in four lines: a permanent condition called a passing hiccup, a promise
// about saved work when nothing had been written, and a retry control for a state no number
// of presses can clear. This suite makes each one unreachable on the surface the CEO sees.
//
// **THE BYTES ARE THE BACKEND'S, NOT THIS FILE'S.** Every sentence asserted below is scraped
// out of `crates/richos-core/src/interruption.rs` at run time by `lib/state-strings.js` —
// the same scrape `outage.js` and `affordances.js` use. Nothing here types a sentence, so a
// reworded `ceo_message()` moves this suite with it instead of leaving it green over copy
// the product no longer says.
//
// Run: node interruption.js   (or `npm test` for every suite in this directory)

"use strict";

const { loadPlaywright, openFixture, createRun, assert, assertEqual } = require("./lib/harness");
const { rustStrings, normalize } = require("./lib/state-strings");
const C = require("./lib/contrast");

const THREAD = "thr_interruption";
const TURN = "turn_interruption";

// ---------------------------------------------------------------------------------------
// THE SENTENCES, READ OUT OF THE RUST SOURCE
// ---------------------------------------------------------------------------------------

/// Find the one scraped `ceo_message` literal containing `needle`. Throws if there is not
/// exactly one — an ambiguous match means the source moved and this suite would otherwise
/// keep asserting against whichever it happened to pick.
function rustSentence(needle) {
  const hits = rustStrings()
    .map((s) => normalize(s.text))
    .filter((t) => t.includes(needle));
  const unique = Array.from(new Set(hits));
  assertEqual(
    unique.length,
    1,
    `expected exactly one Rust CEO sentence containing ${JSON.stringify(needle)}, found ${unique.length}`
  );
  return unique[0];
}

const NOT_SIGNED_IN = rustSentence("no Claude account is signed in");
const TRANSIENT = rustSentence("RichOS lost its connection to Claude Code");

// The exact sentence `InterruptionRecord::new` writes when Rich had written nothing. It is
// a `format`-free literal in Rust, but it lives in `InterruptionRecord::new` rather than in
// a `ceo_message()` body, so the scraper does not see it and this is the one string the
// suite states. The core test `nothing_written_means_no_claim_about_a_saved_answer_is_made`
// pins the same bytes on the Rust side, so a change breaks there first.
const ONLY_HIS_WORDS = "Your message is saved. Rich hadn't written any of his answer yet.";

// The two generic sentences, which must NOT appear when a cause is known.
const GENERIC_BODY = "I hit a snag mid-thought and had to stop";
// The one label for the one action, after D6. `timeline.js`'s `RETRY_LABEL`.
const RETRY_LABEL = "Restore my message";
const GENERIC_NOTE = "Everything I'd already written above is saved.";

// ---------------------------------------------------------------------------------------
// FIXTURES
// ---------------------------------------------------------------------------------------

function snapshot(interruption, opts) {
  const withPartial = !!(opts && opts.withPartial);
  // A turn first witnessed mid-flight has no message of his at all. It is the negative half
  // of "there are words to put back" and the one case where a put-back control would be a
  // button that puts nothing back.
  const withoutHisWords = !!(opts && opts.withoutHisWords);
  const b = (id, extra) =>
    Object.assign(
      {
        id,
        entityId: "northwind",
        threadId: THREAD,
        turnId: TURN,
        bindingRevision: 1,
        createdAt: 1789000000000,
        sequence: null,
        slot: "stream",
        visibility: "ceo",
      },
      extra || {}
    );
  const items = withoutHisWords
    ? []
    : [
        Object.assign(b(TURN + ":user", { slot: "opening", createdAt: 1788000000000 }), {
          kind: "user_message",
          text: "What did I decide about nightly releases?",
          source: "text",
        }),
      ];
  // THE AUDIT'S CASE HAS NO PROSE ABOVE THE CARD, and that is load-bearing: the old note
  // claimed "everything I'd already written above is saved" with nothing above it at all.
  if (withPartial) {
    items.push(
      Object.assign(b(TURN + ":text:0", { sequence: 0 }), {
        kind: "rich_message",
        phase: "unknown",
        text: "Pulling the decision record now.",
      })
    );
  }
  items.push(
    Object.assign(b(TURN + ":duration", { slot: "terminal", createdAt: 1788000000000 }), {
      kind: "work_duration",
      state: (opts && opts.state) || "interrupted",
      startedAt: 1788000000000,
      endedAt: 1788000001000,
      activeMs: 1000,
    })
  );
  if (interruption) {
    items.push(
      Object.assign(b(TURN + ":interruption", { slot: "terminal", createdAt: 1788000001000 }), interruption)
    );
  }
  return { entityId: "northwind", threadId: THREAD, mode: "ceo", bindingRevision: 1, items };
}

/// The nightly's own turn, exactly as `richos-core` now projects it.
function notSignedInItem() {
  return {
    kind: "turn_interruption",
    cause: "not-signed-in",
    ceoMessage: NOT_SIGNED_IN,
    lossMessage: ONLY_HIS_WORDS,
    offersRetry: false,
  };
}

/// A lost pipe — the one class where asking again is honest advice.
function transientItem() {
  return {
    kind: "turn_interruption",
    cause: "transient",
    ceoMessage: TRANSIENT,
    lossMessage:
      "On disk: what you asked for is saved, the 31 characters of the answer that had " +
      "already arrived are saved. Not on disk: everything the session had worked out in " +
      "its head and had not yet said. Asking again starts that part over.",
    offersRetry: true,
  };
}

const cardText = (page) =>
  page.evaluate(() => {
    const card = document.querySelector(".tl-intervention");
    if (!card) return null;
    return {
      body: Array.from(card.querySelectorAll(".tl-intervention-body")).map((n) => n.textContent),
      notes: Array.from(card.querySelectorAll(".tl-intervention-note")).map((n) => n.textContent),
      action: card.querySelector(".tl-intervention-action")
        ? card.querySelector(".tl-intervention-action").textContent
        : null,
      actionCount: card.querySelectorAll(".tl-intervention-action").length,
    };
  });

const CARD_PARTS = ["tl-intervention-body", "tl-intervention-note", "tl-intervention-action"];

/// The whole fixture page, measured by `lib/contrast.js`'s shared probe — never arithmetic
/// of this file's own. Same reasoning as `outage.js`'s header records at length: a second
/// implementation that agrees today is one that will disagree silently later.
async function contrastOfPage(page, theme) {
  await page.evaluate(C.pageScript());
  const out = await page.evaluate((o) => window.__contrastProbe(o), {
    surface: "interruption-card",
    theme: theme,
  });
  return {
    measured: out.measuredPaths || [],
    failures: Object.values(out.failures),
    unresolvable: Object.values(out.unresolvable),
  };
}

/// Which of the card's own parts the probe measured, matched against the DOM rather than
/// against a guess at the probe's path format — so "the page is clean" can never mean "the
/// card was not on it".
async function cardNodes(page, measured) {
  const wanted = await page.evaluate((parts) => {
    const sel = parts.map((c) => "." + c).join(", ");
    return Array.from(document.querySelectorAll(sel)).map((el) => {
      const tag = el.tagName.toLowerCase();
      return el.id ? tag + "#" + el.id : tag + "." + String(el.className).split(/\s+/)[0];
    });
  }, CARD_PARTS);
  const missing = wanted.filter((w) => !measured.some((m) => m.indexOf(w) >= 0));
  return { wanted, missing };
}

// ---------------------------------------------------------------------------------------

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("nightly D2 — a failed turn says the cause the app is holding");

  const page = await openFixture(browser);

  // =====================================================================================
  // THE NIGHTLY'S CASE
  // =====================================================================================

  await run.check("the sign-in case replaces the generic card with the backend's sentence", async () => {
    await page.evaluate((s) => window.__render(s), snapshot(notSignedInItem()));
    await page.waitForSelector(".tl-intervention");
    const c = await cardText(page);
    assert(c, "no failure card rendered at all");
    assertEqual(c.body, [NOT_SIGNED_IN], "the body is the authored sentence, verbatim");
    assert(
      !c.body.concat(c.notes).some((t) => t.includes(GENERIC_BODY)),
      "UNTRUTH 1 — the generic snag sentence must be replaced, not appended to"
    );
    return "body = the authored sign-in sentence";
  });

  await run.check("it names the account and the route that exists", async () => {
    const c = await cardText(page);
    const whole = c.body.concat(c.notes).join(" ");
    assert(whole.includes("Claude account"), "the account is never mentioned: " + whole);
    // The route is IN the authored sentence rather than beside it — see the Rust comment on
    // `NotSignedIn`'s arm. A second string returned by a `route()` method would have reached
    // his screen while being invisible to the affordance registry.
    assert(
      whole.includes("Settings") && whole.includes("Claude accounts"),
      "the route the app actually has must be offered: " + whole
    );
    assert(!whole.includes("\u2192"), "an arrow is read aloud as nothing: " + whole);
    return "names the account and the Settings route, in one spoken-safe sentence";
  });

  await run.check("UNTRUTH 2 — it claims no saved answer when none was written", async () => {
    const c = await cardText(page);
    assert(
      !c.notes.some((n) => n.includes(GENERIC_NOTE)),
      '"Everything I\'d already written above is saved" must be gone: ' + JSON.stringify(c.notes)
    );
    assert(
      !c.notes.some((n) => n.includes("already written above")),
      "no claim about prose above the card, because there is none: " + JSON.stringify(c.notes)
    );
    assert(
      c.notes.some((n) => n.includes(ONLY_HIS_WORDS)),
      "the one true thing — HIS message is saved — must still be said: " + JSON.stringify(c.notes)
    );
    // And the screen agrees with the claim: there is genuinely no Rich prose above it.
    const prose = await page.evaluate(() => document.querySelectorAll(".tl-rich, .tl-message--rich").length);
    assertEqual(prose, 0, "the fixture must have no answer above the card for this to mean anything");
    return "no saved-answer claim; his message named; 0 prose rows above";
  });

  // =====================================================================================
  // UNTRUTH 3, AND IT IS RESTATED HERE RATHER THAN RELAXED.
  //
  // D2's rule is "a button that CANNOT WORK is worse than no button", and this check used to
  // enforce it as `actionCount === 0` on the sign-in card. That was right when the control
  // was called **Pick it back up** and really did promise a resume. D6 renamed it to **Put it
  // back in the box**, which starts nothing and always works, and the gate in `timeline.js`
  // was never revisited — so the card kept telling him to *"send this to me again"* over an
  // empty composer with no way to get his sentence back (candidate-.2 §4 defect #8).
  //
  // So the invariant is the one D2 actually protects, stated against the control that exists
  // now: NOTHING ON THIS CARD OFFERS OR PERFORMS A RETRY. The card may hand his words back —
  // that is not a retry, it costs nothing, and it is the one thing he can do next.
  // =====================================================================================

  await run.check("UNTRUTH 3 — nothing on this card offers or performs a retry", async () => {
    const c = await cardText(page);
    assertEqual(c.actionCount, 1, "exactly one control, and it is not a retry");
    assertEqual(c.action, RETRY_LABEL, "the only control is the put-back control");
    assert(
      !/try again|retry|pick (it|this) back up|ask me again|send it again/i.test(c.action),
      "the control's label offers a retry: " + c.action
    );
    // AND IT DOES NOT SECRETLY DO ONE. The handler is recorded rather than assumed: pressing
    // it must call `opts.retry` — which `main.js`'s `retryTurn` implements as "put the text
    // back and focus the box" — and must not reach `opts.send` or start a turn.
    await page.evaluate((s) => {
      window.__retryCalls = [];
      window.__render(s, { retry: (t) => window.__retryCalls.push(t.turnId) });
    }, snapshot(notSignedInItem()));
    await page.waitForSelector(".tl-intervention-action");
    await page.click(".tl-intervention-action");
    const calls = await page.evaluate(() => window.__retryCalls.slice());
    assertEqual(calls, [TURN], "one press, one put-back call on this turn");
    // The card's own prose still refuses to promise the failure has cleared.
    const whole = c.body.concat(c.notes).join(" ");
    assert(/send your message again/.test(whole), "the sentence that makes this control necessary is gone: " + whole);
    return `1 control, ${JSON.stringify(RETRY_LABEL)}, one press -> one put-back call, no retry promised anywhere on the card`;
  });

  await run.check("D2 STILL HOLDS: no control at all when there is nothing to put back", async () => {
    // The other half, and the one that keeps this from being a relaxation. A turn first
    // witnessed mid-flight carries no message of his, so the put-back control would put
    // nothing back — `renderUnknownCard`'s own rule, applied to this card.
    await page.evaluate((s) => window.__render(s), snapshot(notSignedInItem(), { withoutHisWords: true }));
    await page.waitForSelector(".tl-intervention");
    const c = await cardText(page);
    assertEqual(c.body, [NOT_SIGNED_IN], "the authored sentence is still the body");
    assertEqual(c.actionCount, 0, "a control that puts nothing back is worse than no control");
    assertEqual(c.action, null, "no control at all, not a disabled one with the same promise");
    const users = await page.evaluate(() => document.querySelectorAll(".tl-message--ceo, .tl-user").length);
    assertEqual(users, 0, "the fixture still has a message of his, so this proves nothing");
    // Put the ordinary fixture back for the checks below.
    await page.evaluate((s) => window.__render(s), snapshot(notSignedInItem()));
    await page.waitForSelector(".tl-intervention");
    return "0 messages of his on screen, 0 controls on the card";
  });

  // =====================================================================================
  // THE POSITIVE CONTROL — without it, "no button" could be "the card never renders one"
  // =====================================================================================

  await run.check("a transient failure DOES get the retry control, and the one verb", async () => {
    await page.evaluate((s) => window.__render(s), snapshot(transientItem(), { withPartial: true }));
    await page.waitForSelector(".tl-intervention");
    const c = await cardText(page);
    assertEqual(c.body, [TRANSIENT], "the body is the authored transient sentence");
    assertEqual(c.actionCount, 1, "exactly one control");
    assertEqual(c.action, RETRY_LABEL, "same verb as every other failure — one thing to learn");
    assert(
      c.notes.some((n) => n.includes("31 characters of the answer")),
      "what IS on disk is counted: " + JSON.stringify(c.notes)
    );
    return RETRY_LABEL + " — present, and the partial reply counted";
  });

  // =====================================================================================
  // D6 — THE CONTROL SAYS WHAT IT DOES
  //
  // Audit §D6: the message said "say the word and I'll pick it back up" and the button said
  // "Pick it back up". Pressing it put his text back in the composer — no new turn, no log
  // line. Returning the text is the RIGHT behavior (resending spends his subscription and
  // starts work, so it is his to trigger); the words were the defect.
  // =====================================================================================

  await run.check("the control promises only what it does, and then does it", async () => {
    // Rendered with a RECORDING handler in place of the fixture's no-op, so the behavior
    // half of this check is a real observation rather than an assumption about the label.
    await page.evaluate((s) => {
      window.__retryCalls = [];
      window.__render(s, { retry: (t) => window.__retryCalls.push(t.turnId) });
    }, snapshot(transientItem(), { withPartial: true }));
    await page.waitForSelector(".tl-intervention-action");

    const label = await page.textContent(".tl-intervention-action");
    assertEqual(label, RETRY_LABEL, "the label must be what pressing it does");
    assert(!/pick it back up/i.test(label), "the old promise must be gone: " + label);

    // NOTHING ON THE CARD PROMISES A RESUME either — the label was only half of D6.
    const c = await cardText(page);
    const whole = c.body.concat(c.notes).join(" ");
    assert(
      !/pick (it|this) back up|I'll resume|carry on from where/i.test(whole),
      "a sentence still promises that Rich resumes: " + whole
    );

    // THE POSITIVE CONTROL. A label that matched a control which no longer worked would be
    // the same defect wearing better copy, so the press is made and observed.
    await page.click(".tl-intervention-action");
    const calls = await page.evaluate(() => window.__retryCalls.slice());
    assertEqual(calls.length, 1, "one press, one call");
    assertEqual(calls[0], TURN, "and it acts on this turn");
    return `label=${JSON.stringify(label)}, one press -> one handler call, no resume promise`;
  });

  await run.check("an unclassified interrupted or failed turn speaks as RichOS and claims no saved answer", async () => {
    // Restart recovery has no interruption/outage record. Exercise that fallback with
    // no answer and a partial answer, independently of the structured Rust notices.
    for (const state of ["interrupted", "failed"]) {
      for (const withPartial of [false, true]) {
        await page.evaluate((s) => window.__render(s), snapshot(null, { state, withPartial }));
        await page.waitForSelector(".tl-intervention");
        const c = await cardText(page);
        const whole = c.body.concat(c.notes).join(" ");
        assert(!/\bI\b/.test(whole), "the app must not speak as Rich: " + whole);
        assert(c.body.some((t) => /\bRich\b/.test(t)), "the fallback must say whose answer was cut off: " + whole);
        assert(!whole.includes(GENERIC_NOTE), "the fallback cannot claim an unchecked saved answer: " + whole);
        assert(!/snag|already written above/.test(whole), "the fallback cannot invent a failure or saved work: " + whole);
        assertEqual(c.actionCount, 1, "the original message remains restorable");
        assertEqual(c.action, RETRY_LABEL, "the control describes restoration only");
      }
    }
    return "both fallback states, with and without partial prose: app-owned notice and Restore my message";
  });

  // Nightly 67 p3: Rich's turn for Stu's status message was cut off, and the Mac said "Press
  // Restore my message ... Your message is saved" about a message he never sent.
  await run.check("a turn that carried no words of his never offers to restore them, and keeps a cause he can act on (nightly 67)", async () => {
    const UNKNOWN = rustSentence("can't tell you why");
    const record = (hisWords) => ({ kind: "turn_interruption", cause: "unknown", ceoMessage: UNKNOWN,
      lossMessage: ONLY_HIS_WORDS, offersRetry: true, hisWords });
    await page.evaluate((s) => window.__render(s), snapshot(record(false), { withoutHisWords: true }));
    await page.waitForSelector(".tl-duration-label");
    const seen = await page.evaluate(() => ({
      card: !!document.querySelector(".tl-intervention"),
      text: document.getElementById("messages").innerText,
      label: Array.from(document.querySelectorAll(".tl-duration-label")).map((n) => n.textContent),
    }));
    assertEqual(seen.card, false, "a failure card about his message on a turn he did not send");
    assert(!/Restore my message|Your message is saved/.test(seen.text), "his message is mentioned: " + seen.text);
    assertEqual(seen.label, ["Stopped after 1s"], "the duration row still says the turn stopped");
    // A cause he can act on stays (review rv-20261019T113734Z-68e37b15-cc60#1: a "Start the
    // questions" press whose sign-in had expired lost the Settings / Claude accounts route):
    // the sentence only, without the loss note about his message and without Restore.
    await page.evaluate((s) => window.__render(s),
      snapshot(Object.assign(notSignedInItem(), { hisWords: false }), { withoutHisWords: true }));
    await page.waitForSelector(".tl-intervention");
    const signIn = await cardText(page);
    assertEqual([signIn.body, signIn.notes, signIn.actionCount], [[NOT_SIGNED_IN], [], 0],
      "a button's failed turn keeps the sign-in route and nothing about his message");
    // The positive control: the same record on a turn of his still draws the whole card.
    await page.evaluate((s) => window.__render(s), snapshot(record(true)));
    await page.waitForSelector(".tl-intervention");
    const c = await cardText(page);
    assertEqual([c.body, c.action], [[UNKNOWN], RETRY_LABEL], "his own turn keeps its card");
    return "no card and no mention of his message on a turn he did not send; " + JSON.stringify(seen.label[0]) + " stays; his own turn keeps the card";
  });

  // =====================================================================================
  // CONTRAST — WCAG AA in BOTH themes, computed, never eyeballed
  // =====================================================================================

  for (const theme of ["dark", "light"]) {
    await run.check(`the card meets WCAG AA in the ${theme} theme`, async () => {
      await page.evaluate((t) => {
        document.documentElement.setAttribute("data-theme", t);
      }, theme);
      await page.evaluate((s) => window.__render(s), snapshot(notSignedInItem()));
      await page.waitForSelector(".tl-intervention");
      const r = await contrastOfPage(page, theme);
      const nodes = await cardNodes(page, r.measured);
      assertEqual(
        nodes.missing.length,
        0,
        `the card's own parts were not measured: ${JSON.stringify(nodes.missing)}`
      );
      assertEqual(
        r.unresolvable.length,
        0,
        `a color the probe could not resolve is a failure to prove: ${JSON.stringify(r.unresolvable)}`
      );
      assertEqual(
        r.failures.length,
        0,
        `WCAG AA failures in ${theme}: ${JSON.stringify(r.failures)}`
      );
      return `${theme}: ${r.measured.length} node(s) measured, ${nodes.wanted.length} card part(s), 0 failures`;
    });
  }

  // =====================================================================================
  // THE TYPE FLOOR — §15: nothing a person is expected to read below 14px
  // =====================================================================================

  await run.check("no part of the card sits below the 14px floor", async () => {
    const sizes = await page.evaluate((parts) => {
      const sel = parts.map((c) => "." + c).join(", ");
      return Array.from(document.querySelectorAll(sel)).map((el) => ({
        cls: String(el.className),
        px: parseFloat(getComputedStyle(el).fontSize),
      }));
    }, CARD_PARTS);
    assert(sizes.length > 0, "nothing measured");
    const small = sizes.filter((s) => s.px < 14);
    assertEqual(small.length, 0, `below the floor: ${JSON.stringify(small)}`);
    return sizes.map((s) => `${s.cls.split(/\s+/)[0]}=${s.px}px`).join(" ");
  });

  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
  await browser.close();
  // `report()` returns the FAILED COUNT, not a verdict. Same form as every other suite here.
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
