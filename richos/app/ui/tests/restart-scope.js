// SLICE 9 of §24 — "test: verify restart, multi-thread and scope behavior."
//
// The eight slices before this one each proved ONE thread, in isolation, for the length of
// one turn. Everything §25 says about what happens BETWEEN threads, and what survives a
// restart, had no test anywhere:
//
//   * `simulateBackgroundTurn`, `simulateSlowTurn` and `simulateMidTurnCrash` have been in
//     `mock.js` since slice 5. Before this suite, NOTHING called any of them. The three
//     scenarios §25 names under Navigation, Integrity and Completion were reachable by hand
//     and by nothing else.
//   * `reviveLiveTurns` (main.js) is the only thing standing between "leave a working thread
//     and come back" and a row reading `Status unavailable` over a turn that is still
//     streaming. It had no caller in any test.
//   * The per-thread draft and scroll maps (§3.1) had no test, and the draft one is a
//     PRIVACY control, not a convenience: an entity is a boundary (§1), so a half-written
//     sentence for Northwind Traders sitting in Lumen Labs's composer is one Enter away from filing the
//     CEO's words in the wrong company.
//   * The renderer's fence had one test, on ONE of its six entry points (`workers.js`,
//     through `onActivityUpserted`). A seventh handler added tomorrow would inherit no
//     coverage at all.
//
// ---------------------------------------------------------------------------------------
// WHAT MAKES A CHECK HERE COUNT
// ---------------------------------------------------------------------------------------
//
// A verification that cannot fail proves nothing, and the two ways it silently stops being
// able to fail are both in this repository's own history:
//
//   AN EMPTY CORPUS. A scanner reported CLEAN because the set it walked was empty. So every
//   inventory this suite derives is asserted NON-EMPTY and against a second, independent
//   derivation — the handler sweep counts `accepts()` call sites in `timeline.js` on disk
//   and requires the two numbers to agree, so a handler that skips the fence fails here
//   rather than quietly widening the surface.
//
//   A POSITIVE THAT NEVER HAPPENS. "The foreign thread's text did not appear" passes
//   perfectly on a page where nothing appears at all. So every containment check carries a
//   POSITIVE PROBE: the same content, correctly scoped, IS on screen in the same run.
//
// Each check was additionally run RED once, by breaking the thing it guards in the shipped
// source and watching it fail. Those runs are transcribed in
// `docs/verification/restart-scope-2026-08-30/`.
//
// ---------------------------------------------------------------------------------------
// The shell, not a copy of it: `index.html` + `main.js` + `mock.js` + `timeline.js` +
// `style.css`, loaded from disk under WebKit, with nothing stubbed but the Tauri bridge
// `mock.js` already replaces.
// ---------------------------------------------------------------------------------------

"use strict";

const fs = require("fs");
const path = require("path");
const { leaveHome, loadPlaywright, shot, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// The shell coalesces renders into an animation frame (§15). Two frames is one scheduled
/// render plus its flush; reading the DOM in the emitting tick reads the previous frame.
async function settle(page) {
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
}

async function openApp(browser, viewport) {
  const page = await browser.newPage({ viewport: viewport || { width: 1400, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  await page.goto(APP);
  // The home screen is the landing surface now; this suite is about the app UI behind it.
  await leaveHome(page);
  await page.waitForFunction("typeof window.RichTimeline === 'object'");
  await page.waitForSelector(".nav-thread", { state: "attached" });
  page.__errors = errors;
  return page;
}

/// Wait until the store `main.js` parks the view into (`richos.view.drafts` / `richos.view.scroll`,
/// written `PARK_DEBOUNCE_MS` after the last change) says what `want` says, key by key; a key
/// whose wanted value is `null` must be ABSENT. WAITED FOR, NOT SLEPT FOR (2026-09-29, audit
/// R10): these checks used to sleep 700ms "> PARK_DEBOUNCE_MS" and read the store, which is
/// 300ms of slack for a debounced timer to fire and write on a busy host. The 10s bound is a
/// hang guard; on expiry this returns and the assertion after it names what is missing.
async function parkedAs(page, want) {
  await page
    .waitForFunction(
      (w) => {
        const read = (k) => JSON.parse(window.localStorage.getItem(k) || "{}");
        const now = { drafts: read("richos.view.drafts"), scroll: read("richos.view.scroll") };
        return Object.keys(w).every((store) =>
          Object.keys(w[store]).every((id) => (w[store][id] === null ? !(id in now[store]) : now[store][id] === w[store][id]))
        );
      },
      want,
      { timeout: 10000 }
    )
    .catch(() => {});
}

/// Record every `turn-status` the shell DELIVERS, per thread, by wrapping the handler it calls
/// (`main.js` looks `window.RichTimeline.onTurnStatus` up at delivery time, the same seam the
/// dropped-events check below uses). The wrapper calls straight through, so nothing the
/// renderer does changes; it only lets a check know that a foreign turn's whole stream has
/// ARRIVED in this page, rather than guessing it from a sleep.
async function watchTurnStatus(page) {
  await page.evaluate(() => {
    if (window.__turnStatusSeen) return;
    window.__turnStatusSeen = [];
    const real = window.RichTimeline.onTurnStatus;
    window.RichTimeline.onTurnStatus = function (model, p) {
      window.__turnStatusSeen.push({ threadId: p && p.threadId, status: p && p.status });
      return real.apply(this, arguments);
    };
  });
}

/// Wait until a turn in `threadId` has been delivered to this page all the way to `completed`
/// (its deltas and its completion are emitted before that status). A precondition, not a
/// verdict: a scope check that asserts ABSENCE is only worth something once the thing that must
/// be absent has actually arrived. The 30s bound is a hang guard and it names itself.
async function turnDelivered(page, threadId, since) {
  await page
    .waitForFunction(
      ([t, from]) => (window.__turnStatusSeen || []).slice(from).some((s) => s.threadId === t && s.status === "completed"),
      [threadId, since],
      { timeout: 30000 }
    )
    .catch((e) => {
      throw new Error(`the ${threadId} turn never reached this page as completed within 30s: ${String(e.message || e).split("\n")[0]}`);
    });
}

async function open(page, threadId) {
  await page.click(`.nav-thread[data-thread-id="${threadId}"]`);
  await settle(page);
}

/// Wait until nothing in the OPEN thread is still running. A live duration row re-renders
/// once a second and a still-streaming reply grows, so any before/after comparison of one
/// thread's content has to start from a settled one or it measures the clock. (It did: the
/// first version of the same-entity probe below failed on a 7-character difference that was
/// `Working for 5s` becoming `Worked for 6s`.)
async function waitIdle(page) {
  await page.waitForFunction(
    () => !Array.from(document.querySelectorAll(".tl-duration-label")).some((n) => /^Working/.test(n.textContent)),
    null,
    { timeout: 30000 }
  );
  await settle(page);
}

/// Every rail row with the mark it is currently carrying, read the way a screen reader
/// reads it — the accessible name, not the glyph, because §18 forbids status by shape or
/// color alone.
function railMarks(page) {
  return page.evaluate(() =>
    Array.from(document.querySelectorAll(".nav-thread")).map((b) => ({
      id: b.dataset.threadId,
      label: b.getAttribute("aria-label"),
      glyph: b.querySelector(".nav-status") ? b.querySelector(".nav-status").textContent : null,
    }))
  );
}

function markOf(rows, id) {
  const r = rows.find((x) => x.id === id);
  return r ? r.label : null;
}

/// Every turn section on screen, with the row §6.1 puts at its head.
///
/// ONE ROW IS `.tl-duration`. `.tl-duration-label` is a span INSIDE `.tl-duration-btn`,
/// which is inside `.tl-duration`, so counting the inner two double-counts every row —
/// which is exactly how the first run of this suite reported "the same turn drew 2
/// duration rows" against a renderer that had drawn one.
function turns(page) {
  return page.evaluate(() =>
    Array.from(document.querySelectorAll(".tl-turn")).map((s) => ({
      turnId: s.dataset.turnId,
      duration: ((s.querySelector(".tl-duration-label") || {}).textContent || "").trim(),
      user: (s.querySelector(".tl-user-text") || {}).textContent || "",
      text: s.innerText,
    }))
  );
}

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("§25 restart / multi-thread / scope — real shell, WebKit");

  // =====================================================================================
  // 1. MULTI-THREAD — §25 Navigation, §2, §3.1
  // =====================================================================================

  const page = await openApp(browser);

  await run.check("§25 a working thread stays visibly active while another thread is selected", async () => {
    await open(page, "acme");
    const before = await railMarks(page);
    assertEqual(markOf(before, "hiring"), "Q4 hiring", "VACUITY: the row already carried a mark before any turn started");

    // The SAME `simulateTurn` a real send runs — the mark comes from the ordinary
    // `rich://turn-started` event, not from a special path.
    await page.evaluate(() => window.__RICHOS_MOCK__.simulateBackgroundTurn("hiring", "run the numbers again"));
    await page.waitForFunction(
      () => {
        const b = document.querySelector('.nav-thread[data-thread-id="hiring"]');
        return b && /working/.test(b.getAttribute("aria-label") || "");
      },
      null,
      { timeout: 5000 }
    );
    const after = await railMarks(page);

    assertEqual(markOf(after, "hiring"), "Q4 hiring, working", "the background thread must read as working");
    assertEqual(
      (after.find((r) => r.id === "hiring") || {}).glyph,
      "◐",
      "and carry §3.2's low-motion pulse glyph"
    );
    // The selected thread is unaffected, and no OTHER row invented a mark.
    assertEqual(markOf(after, "acme"), "Acme deal", "the selected thread must not acquire a mark");
    const spurious = after.filter((r) => r.glyph && !["hiring", "partner", "ecs", "legacy"].includes(r.id));
    assertEqual(spurious.map((r) => r.id), [], "a mark appeared on a thread with no signal for one");
    return `hiring: "${markOf(before, "hiring")}" -> "${markOf(after, "hiring")}" while acme is selected`;
  });

  await run.check("§2 returning to a running thread resumes its timer — it does not restart it", async () => {
    // A turn with a reply long enough to still be streaming when we come back: the canned
    // replies finish in about two seconds, which is shorter than this check needs.
    await open(page, "acme");
    await page.evaluate(() => window.__RICHOS_MOCK__.simulateSlowTurn("hiring", "and again, with the numbers", 200));
    // Sit in another thread long enough that a timer which restarted on arrival would be
    // unmistakably wrong, then open the working thread.
    await page.waitForTimeout(3600);
    await open(page, "hiring");
    const rows = await turns(page);
    const live = rows.find((t) => /^Working/.test(t.duration));
    assert(live, "no live turn on screen after returning: " + JSON.stringify(rows.map((r) => r.duration)));

    const seconds = Number((live.duration.match(/Working for (\d+)s/) || [])[1]);
    assert(
      Number.isFinite(seconds),
      "§6.1: a turn running for over a second must read `Working for {duration}`, got: " + live.duration
    );
    assert(
      seconds >= 3,
      "the timer restarted on arrival — it reads " + seconds + "s for a turn that has been running over 3s"
    );

    // And it keeps ticking from that value rather than being a frozen snapshot. WAITED FOR, NOT
    // SLEPT FOR (2026-09-29, audit R10): this used to sleep 1,400ms and then require a bigger
    // number, which a tick delivered late on a busy host reads as the same second. The fact is
    // the row moving past `seconds`; the 20s bound is a hang guard for a row that froze, which
    // is exactly the defect, and it fails by name below.
    await page
      .waitForFunction(
        ([turnId, s]) => {
          const sec = document.querySelector(`.tl-turn[data-turn-id="${turnId}"]`);
          const label = sec && sec.querySelector(".tl-duration-label");
          const m = label && /Working for (\d+)s/.exec(label.textContent);
          return !!m && Number(m[1]) > s;
        },
        [live.turnId, seconds],
        { timeout: 20000 }
      )
      .catch(() => {});
    await settle(page);
    const later = (await turns(page)).find((t) => t.turnId === live.turnId);
    const seconds2 = Number((later.duration.match(/Working for (\d+)s/) || [])[1]);
    assert(seconds2 > seconds, `the row froze at ${seconds}s instead of ticking (now ${later.duration})`);
    return `left at 0s, returned at ${seconds}s, ticking (${seconds2}s) — derived from the turn's own startedAt`;
  });

  await run.check("§3.1/§9.2 a draft belongs to one thread and never follows the CEO out of its entity", async () => {
    await open(page, "acme"); // Northwind Traders
    await page.fill("#input", "our walk-away number on Acme is");
    await open(page, "partner"); // Lumen Labs — another entity
    const inLumen = await page.inputValue("#input");
    await page.fill("#input", "ask Hensley about the carry split");
    await open(page, "acme");
    const backInAcme = await page.inputValue("#input");
    await open(page, "partner");
    const backInLumen = await page.inputValue("#input");

    assertEqual(inLumen, "", "Northwind Traders's half-written sentence appeared in Lumen Labs's composer");
    assertEqual(backInAcme, "our walk-away number on Acme is", "the draft was lost");
    assertEqual(backInLumen, "ask Hensley about the carry split", "the second draft was lost");
    return "two drafts, two threads, two entities — each restored to its own, neither seen by the other";
  });

  await run.check("§15 each thread keeps its own scroll position across a switch", async () => {
    // A SHORT window on purpose. At 900px the seeded acme thread fits entirely on screen
    // (measured: scrollHeight 773 == clientHeight 773), and a scroll test on a pane that
    // cannot scroll passes without touching the behavior it names. The vacuity assert
    // below is what caught that, and it stays.
    const sc = await openApp(browser, { width: 1200, height: 420 });
    await open(sc, "acme");
    const parked = await sc.evaluate(() => {
      const c = document.getElementById("conversation");
      c.scrollTop = 0; // to the top of the history
      c.dispatchEvent(new Event("scroll"));
      return { top: c.scrollTop, height: c.scrollHeight, client: c.clientHeight, bottom: c.scrollHeight - c.clientHeight };
    });
    assert(
      parked.bottom > 48,
      "VACUITY: this thread is not scrollable, so 'position preserved' would pass trivially " + JSON.stringify(parked)
    );
    assert(parked.top !== parked.bottom, "VACUITY: parked at the bottom, which is also the default for a fresh thread");

    await open(sc, "partner");
    await open(sc, "acme");
    await settle(sc);
    const restored = await sc.evaluate(() => document.getElementById("conversation").scrollTop);
    assertEqual(restored, parked.top, "acme came back at a different scroll position");

    // And the OTHER thread, never scrolled, still lands where a never-opened thread lands:
    // the newest turn. A single shared scroll value would put it at acme's position.
    const partnerTop = await sc.evaluate(async () => {
      document.querySelector('.nav-thread[data-thread-id="partner"]').click();
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      const c = document.getElementById("conversation");
      return { top: c.scrollTop, bottom: c.scrollHeight - c.clientHeight };
    });
    assertEqual(partnerTop.top, partnerTop.bottom, "partner's position followed acme's instead of staying at its own");
    await sc.close();
    return `acme scrollable by ${parked.bottom}px, parked at ${parked.top}, restored to ${restored}; partner still at its own bottom (${partnerTop.top})`;
  });

  // ---- and all of that now survives a CRASH, which it did not before 2026-08-31 --------
  //
  // The two checks above prove the maps survive a thread SWITCH, which is all §3.1 asked
  // of them. The CEO's splash ruling added a second requirement in its own right — "a
  // crash-restart returns the user to exactly where they were" — and the maps were
  // `new Map()`, so they died with the process. These two close that.

  await run.check("CRASH: the sentence he was typing and the place he was reading survive the process", async () => {
    // A SHARED CONTEXT, because that is what makes two pages the same install: one origin,
    // one local store. `openApp` uses `browser.newPage()`, which gives each page its own
    // context and therefore its own storage — right for every other check here, and exactly
    // wrong for this one.
    const ctx = await browser.newContext({ viewport: { width: 1200, height: 420 } });
    const before = await ctx.newPage();
    await before.goto(APP);
    await leaveHome(before);
    await before.waitForSelector(".nav-thread", { state: "attached" });
    await open(before, "acme");
    // THE SCROLL FIRST, THE TYPING LAST, and that order is the check rather than a detail.
    // Written the other way round, the scroll handler's own park wrote the composer text
    // too — so deleting the park from the INPUT handler changed nothing and the mutation
    // ran green. The draft below can only reach disk through the path that handles typing.
    const parked = await before.evaluate(() => {
      const c = document.getElementById("conversation");
      c.scrollTop = 0;
      c.dispatchEvent(new Event("scroll"));
      return { top: c.scrollTop, bottom: c.scrollHeight - c.clientHeight };
    });
    await parkedAs(before, { scroll: { acme: parked.top } }); // the scroll is on disk, the composer is empty
    const sentence = "our walk-away number on Acme is";
    await before.fill("#input", sentence);
    assert(
      parked.bottom > 48,
      "VACUITY: this thread is not scrollable, so 'position survived' would pass trivially " + JSON.stringify(parked)
    );
    assert(parked.top !== parked.bottom, "VACUITY: parked at the bottom, where a fresh thread already lands");

    // NOTHING RUNS ON THE WAY OUT, and that is the point rather than an aside. The first
    // page is not closed, not navigated and not asked to tidy up — it is simply still there
    // while a second one opens on the same store. So whatever the second page finds was
    // written WHILE HE TYPED, which is the only kind of write a crash cannot skip.
    await parkedAs(before, { drafts: { acme: sentence }, scroll: { acme: parked.top } });
    const onDisk = await before.evaluate(() => ({
      drafts: JSON.parse(window.localStorage.getItem("richos.view.drafts") || "{}"),
      scroll: JSON.parse(window.localStorage.getItem("richos.view.scroll") || "{}"),
    }));
    assertEqual(onDisk.drafts.acme, sentence, "the typing never reached the store at all");
    assertEqual(onDisk.scroll.acme, parked.top, "the scroll position never reached the store");

    const after = await ctx.newPage();
    await after.goto(APP);
    await leaveHome(after);
    await after.waitForSelector(".nav-thread", { state: "attached" });
    await open(after, "acme");
    await settle(after);
    const restored = await after.evaluate(() => ({
      draft: document.getElementById("input").value,
      top: document.getElementById("conversation").scrollTop,
    }));
    assertEqual(restored.draft, sentence, "the half-written sentence did not survive");
    assertEqual(restored.top, parked.top, "he came back at a different place in the conversation");

    // The positive probe for the containment half: a thread he never touched is NOT given
    // his draft. A single shared key would restore it into everything.
    await open(after, "partner");
    const foreign = await after.inputValue("#input");
    assertEqual(foreign, "", "the restored draft leaked into another entity's composer");

    await ctx.close();
    return (
      'draft "' + sentence + '" and scrollTop ' + parked.top + " of " + parked.bottom +
      " both survived, restored into acme only, with nothing run on the way out"
    );
  });

  await run.check("CRASH: nothing runs on the way out, and a sent sentence never comes back", async () => {
    // THE FIRST HALF IS THE ARGUMENT the check above rests on, made against the source
    // rather than inferred: if `main.js` ever grows an exit handler, "it survived a crash"
    // quietly becomes "it survived a tidy shutdown", and the check above would keep passing.
    //
    // REGISTRATIONS, not mentions. The first version of this check matched the words and
    // failed on `main.js`'s own comment explaining why there is no `beforeunload` — and on
    // `visibilitychange`, which is a legitimate listener that recomputes the turn timers
    // when the window comes back and has nothing to do with leaving. Matching the call site
    // is the difference between a check and a word filter.
    const src = fs.readFileSync(path.join(UI_DIR, "main.js"), "utf8");
    const exitHooks = ["beforeunload", "pagehide", "unload"];
    const registered = exitHooks.filter(
      (h) => src.includes('addEventListener("' + h + '"') || src.includes("addEventListener('" + h + "'")
    );
    assertEqual(
      registered,
      [],
      "main.js registers an exit handler — the crash guarantee above now rests on something a crash would skip"
    );
    // Guard the guard: the matcher must be able to see a registration at all, or it is a
    // check that cannot fail. `visibilitychange` IS registered, and is not an exit hook.
    assert(
      src.includes('addEventListener("visibilitychange"'),
      "the registration matcher found no listener of any kind — it is looking at the wrong shape"
    );
    // ...and the writes really are continuous, IN THE HANDLER THAT HANDLES TYPING. Asserting
    // the call exists anywhere in the file is what let a mutation deleting it from the input
    // listener pass: the scroll listener's identical call satisfied the search.
    // The named question-pointer listener also observes input. Select the
    // typing callback whose responsibility is saving the draft.
    const inputAt = src.indexOf('inputEl.addEventListener("input", () => {');
    assert(inputAt >= 0, "the composer's typing listener is not in main.js");
    const inputHandler = src.slice(inputAt);
    assert(
      inputHandler.slice(0, inputHandler.indexOf("});")).includes("parkViewStateSoon();"),
      "the composer's own input handler no longer parks — a crash would cost the sentence he is typing"
    );

    // THE SECOND HALF is the one restore that would be worse than none. Words he has
    // already sent must not be handed back to him by a crash as though unsent.
    const ctx = await browser.newContext({ viewport: { width: 1200, height: 420 } });
    const before = await ctx.newPage();
    await before.goto(APP);
    await leaveHome(before);
    await before.waitForSelector(".nav-thread", { state: "attached" });
    await open(before, "acme");
    await before.fill("#input", "send this one");
    // PARKED BEFORE THE SEND, AND PROVEN PARKED. Without this wait the debounce had not yet
    // fired when Send was clicked, the composer was empty by the time it did, and nothing
    // was ever on disk to come back — so "it did not come back" passed over an empty store.
    // That is the vacuity the probe below exists to make impossible.
    await parkedAs(before, { drafts: { acme: "send this one" } });
    const parkedBeforeSend = await before.evaluate(() =>
      JSON.parse(window.localStorage.getItem("richos.view.drafts") || "{}")
    );
    assertEqual(
      parkedBeforeSend.acme,
      "send this one",
      "VACUITY: the sentence was never parked, so 'it did not come back' proves nothing"
    );
    // AND IT IS IN THE IN-MEMORY MAP TOO, which takes a navigation away and back — because
    // that is the only state in which the delete-on-send is load-bearing. Without this the
    // map never held the sentence, every later park rewrote the store from an empty map,
    // and the sentence vanished for a reason that had nothing to do with the line under
    // test: the mutation removing it ran green.
    await open(before, "partner");
    await open(before, "acme");
    assertEqual(
      await before.inputValue("#input"),
      "send this one",
      "VACUITY: the draft is not in the in-memory map, so deleting it there would be a no-op"
    );
    await before.click("#send");
    // The composer clears on acceptance and the delete-on-send reaches the store one debounce
    // later; both are waited for (hang guard), and both are asserted below.
    await before
      .waitForFunction(() => document.getElementById("input").value === "", null, { timeout: 10000 })
      .catch(() => {});
    await parkedAs(before, { drafts: { acme: null } });
    const boxAfterSend = await before.inputValue("#input");
    assertEqual(boxAfterSend, "", "the composer kept the sent words");

    const after = await ctx.newPage();
    await after.goto(APP);
    await leaveHome(after);
    await after.waitForSelector(".nav-thread", { state: "attached" });
    await open(after, "acme");
    const restored = await after.inputValue("#input");
    assertEqual(restored, "", "a sentence he had already sent came back into his composer");
    await ctx.close();
    return 'no exit handler in main.js; "send this one" was on disk AND in the map before the send, and gone from both after it';
  });

  // =====================================================================================
  // 2. SCOPE — §25 Integrity, through the whole shell
  // =====================================================================================

  await run.check("SCOPE: a turn streaming in another entity's thread renders nothing here", async () => {
    await open(page, "partner"); // Lumen Labs
    await watchTurnStatus(page);
    // `simulateSlowTurn` streams a fixed word list that contains "comparables" — a word
    // that appears nowhere in Lumen Labs's own seeded conversation, so its presence here would
    // be unambiguous.
    let since = await page.evaluate(() => window.__turnStatusSeen.length);
    await page.evaluate(() => window.__RICHOS_MOCK__.simulateSlowTurn("acme", "check the Acme numbers", 40));
    // Let the WHOLE foreign turn arrive with the wrong thread on screen. This used to sleep
    // 2,500ms and call that "a while" (2026-09-29, audit R10): on a busy host that is part of a
    // stream, and an absence check over a stream that has not arrived passes for nothing.
    await turnDelivered(page, "acme", since);
    await settle(page);
    const here = await page.evaluate(() => document.getElementById("messages").innerText);

    assert(
      !here.includes("check the Acme numbers"),
      "Northwind Traders's CEO message rendered inside Lumen Labs's thread"
    );
    assert(!here.includes("comparables"), "Northwind Traders's streamed reply rendered inside Lumen Labs's thread");
    assert(here.includes("carry split"), "POSITIVE PROBE: Lumen Labs's own conversation is not on screen either");

    // POSITIVE PROBE, the other half: the very same turn IS rendered in the thread it
    // belongs to. Without this the check above passes on a renderer that draws nothing.
    await open(page, "acme");
    await page
      .waitForFunction(() => document.getElementById("messages").innerText.includes("check the Acme numbers"), null, { timeout: 20000 })
      .catch(() => {});
    const there = await page.evaluate(() => document.getElementById("messages").innerText);
    assert(there.includes("check the Acme numbers"), "the turn did not render in its OWN thread either: " + there.slice(0, 300));

    // AND THE SAME-ENTITY CASE, which is the one the entity clause cannot catch. `hiring`
    // and `acme` are both Northwind Traders, so only the fence's `threadId` clause stands between
    // one Northwind Traders conversation and another. Deleting that clause leaves the cross-ENTITY
    // half of this check passing — measured — which is why both halves are here.
    await open(page, "acme");
    await waitIdle(page); // acme's own turn, from the probe above, must have finished first
    // CONTENT, not innerText: a live duration row in this thread ticks once a second
    // ("Working for 5s" -> "Worked for 6s"), which moves a byte count for a reason that has
    // nothing to do with scope. What must not change is what was SAID.
    const said = () =>
      page.evaluate(() => ({
        ceo: Array.from(document.querySelectorAll(".tl-user-text")).map((n) => n.textContent),
        rich: Array.from(document.querySelectorAll(".tl-rich")).map((n) => n.innerText),
      }));
    const acmeBefore = await said();
    since = await page.evaluate(() => window.__turnStatusSeen.length);
    await page.evaluate(() =>
      window.__RICHOS_MOCK__.simulateSlowTurn("hiring", "sibling thread, same entity", 40)
    );
    await turnDelivered(page, "hiring", since);
    await settle(page);
    const acmeDuring = await said();
    assert(acmeBefore.ceo.length > 0, "POSITIVE PROBE: this thread had nothing on screen to protect");
    assert(
      !JSON.stringify(acmeDuring).includes("sibling thread, same entity"),
      "a sibling Northwind Traders thread's CEO message rendered in this Northwind Traders thread"
    );
    assertEqual(acmeDuring, acmeBefore, "the sibling thread's stream changed what this thread says");
    return "contained across entities AND across threads inside one entity; present in its own";
  });

  await run.check("§25 the fence is on EVERY live handler — inventory derived, never typed", async () => {
    // A hand-typed list of handlers is the defect this repository keeps deleting. So the
    // inventory is read off the shipped object, and cross-checked against a SECOND,
    // independent derivation: the number of `accepts(` call sites in timeline.js on disk.
    // A seventh handler that forgets the fence changes one number and not the other.
    const source = fs.readFileSync(path.join(UI_DIR, "timeline.js"), "utf8");
    const callSites = (source.match(/if \(!accepts\(model, p\)\)/g) || []).length;

    const r = await page.evaluate(() => {
      const handlers = Object.keys(window.RichTimeline)
        .filter((k) => /^on[A-Z]/.test(k) && typeof window.RichTimeline[k] === "function")
        .sort();
      const MINE = { entityId: "northwind", threadId: "thr_fem", bindingRevision: 4 };
      // One payload per handler, shaped as its own event, correctly scoped.
      const ok = {
        onTurnStatus: { turnId: "turn_a", status: "working", startedAt: 1, activeDurationMs: null, visibility: "ceo" },
        onMessageStarted: { turnId: "turn_a", messageId: "turn_a:text:0", phase: "unknown", seq: 0, visibility: "ceo" },
        onMessageDelta: { turnId: "turn_a", messageId: "turn_a:text:0", seq: 1, textDelta: "hello", visibility: "ceo" },
        onMessageCompleted: { turnId: "turn_a", messageId: "turn_a:text:0", phase: "unknown", text: "hello", visibility: "ceo" },
        onActivityUpserted: {
          kind: "activity", id: "mach_1", turnId: "turn_a", createdAt: 1, sequence: 1, slot: "stream",
          visibility: "ceo", activityType: "command", state: "running", summary: "Ran a command",
        },
        onWorkerUpserted: {
          kind: "worker_activity", id: "mach_2", turnId: "turn_a", createdAt: 1, sequence: 2, slot: "stream",
          visibility: "ceo",
          worker: { agentId: "agt_x", workerName: "Sage", agentType: "architecture", observedState: "started", state: "running", eventsObserved: 2 },
        },
        // `rich://ceo-message` — HIS OWN SENTENCE, and the single worst payload in this family
        // to render across an entity boundary. It is also the newest, and this inventory found
        // it the hour it landed: the handler was written, the check listed it as unmodelled,
        // and the fence was proved rather than assumed.
        onCeoMessage: {
          turnId: "turn_a", messageId: "turn_a:user", text: "where are we on the proposal?",
          source: "text", createdAt: 1787950000000,
        },
      };
      const out = { handlers, unmodelled: [], results: {} };
      for (const h of handlers) {
        if (!ok[h]) { out.unmodelled.push(h); continue; }
        const model = window.RichTimeline.createModel();
        window.RichTimeline.bind(model, MINE.entityId, MINE.threadId, MINE.bindingRevision);
        const base = Object.assign({}, MINE, ok[h], { at: 1787950000000 });
        const call = (p) => window.RichTimeline[h](model, p).rejected === true;
        out.results[h] = {
          // POSITIVE PROBE: correctly scoped, it is accepted. A handler that refuses
          // everything would otherwise score four perfect rejections.
          accepted: window.RichTimeline[h](model, base).rejected === false,
          foreignEntity: call(Object.assign({}, base, { entityId: "lumen" })),
          foreignThread: call(Object.assign({}, base, { threadId: "thr_deeply" })),
          staleRevision: call(Object.assign({}, base, { bindingRevision: 3 })),
          laterRevision: window.RichTimeline[h](model, Object.assign({}, base, { bindingRevision: 9 })).rejected === false,
        };
      }
      return out;
    });

    assert(r.handlers.length > 0, "EMPTY INVENTORY: no live handlers were discovered — this check examined nothing");
    assertEqual(r.unmodelled, [], "a live handler exists with no payload modelled here, so it was never fenced-tested");
    assertEqual(
      r.handlers.length,
      callSites,
      `${r.handlers.length} exported handlers but ${callSites} \`accepts(model, p)\` call sites in timeline.js — ` +
        "one of them does not apply the fence"
    );
    for (const [h, v] of Object.entries(r.results)) {
      assert(v.accepted, `POSITIVE PROBE FAILED: ${h} refused a correctly-scoped payload`);
      assert(v.foreignEntity, `${h} ACCEPTED a payload from another entity`);
      assert(v.foreignThread, `${h} ACCEPTED a payload for another thread`);
      assert(v.staleRevision, `${h} ACCEPTED an event older than the current activation`);
      assert(v.laterRevision, `${h} treated bindingRevision as an equality key — a later activation was refused`);
    }
    return `${r.handlers.length} handlers, ${callSites} fence call sites: ${r.handlers.join(", ")}`;
  });

  await run.check("§25 duplicate events render once", async () => {
    const r = await page.evaluate(() => {
      const model = window.RichTimeline.createModel();
      window.RichTimeline.bind(model, "northwind", "thr_fem", 1);
      const fence = { entityId: "northwind", threadId: "thr_fem", turnId: "turn_a", bindingRevision: 1 };
      window.RichTimeline.onTurnStatus(model, Object.assign({}, fence, {
        status: "working", startedAt: 1787948100000, activeDurationMs: null, visibility: "ceo", at: 1787948100000,
      }));
      const activity = Object.assign({}, fence, {
        kind: "activity", id: "mach_dupe", createdAt: 1, sequence: 1, slot: "stream", visibility: "ceo",
        activityType: "command", state: "running", summary: "Ran a command", at: 1787948200000,
      });
      const worker = Object.assign({}, fence, {
        kind: "worker_activity", id: "mach_wdupe", createdAt: 1, sequence: 2, slot: "stream", visibility: "ceo",
        worker: { agentId: "agt_sage", workerName: "Sage", agentType: "architecture", observedState: "started", state: "running", eventsObserved: 2 },
        at: 1787948300000,
      });
      // THE SAME BYTES, five times each — §13: "repeated event IDs are idempotent".
      for (let i = 0; i < 5; i++) {
        window.RichTimeline.onActivityUpserted(model, JSON.parse(JSON.stringify(activity)));
        window.RichTimeline.onWorkerUpserted(model, JSON.parse(JSON.stringify(worker)));
        window.RichTimeline.onTurnStatus(model, Object.assign({}, fence, {
          status: "working", startedAt: 1787948100000, activeDurationMs: null, visibility: "ceo", at: 1787948100000,
        }));
      }
      model.expanded.add("turn_a");
      window.RichTimeline.render(model, document.getElementById("messages"), {
        now: 1787950000000, expandedMessages: new Set(), avatarAlreadyShown: true,
        isExpanded: () => true, toggle: () => {}, rerender: () => {}, copy: () => {}, retry: () => {}, openWorker: () => {},
      });
      const messages = document.getElementById("messages");
      return {
        items: model.items.size,
        turns: model.turnOrder.length,
        chips: messages.querySelectorAll(".tl-chip").length,
        durationRows: messages.querySelectorAll(".tl-duration").length,
        activityRows: Array.from(messages.querySelectorAll(".tl-activity-text")).map((n) => n.textContent),
      };
    });
    assert(r.chips >= 1, "POSITIVE PROBE: nothing rendered at all, so 'renders once' is meaningless");
    assertEqual(r.items, 2, "15 events, 2 distinct ids — the model holds " + r.items);
    assertEqual(r.turns, 1, "the repeated turn-status opened more than one turn");
    assertEqual(r.chips, 1, "the same worker drew " + r.chips + " chips");
    assertEqual(r.durationRows, 1, "the same turn drew " + r.durationRows + " duration rows");
    return `15 events / 3 distinct ids -> ${r.items} items, 1 turn, 1 chip, 1 duration row (${r.activityRows.join(" · ")})`;
  });

  await run.check("§25 missing stream events recover from the durable snapshot", async () => {
    const b = await openApp(browser);
    await open(b, "general"); // an empty thread: whatever appears, this turn put there

    // POSITIVE PROBE first: with the live family intact, the reply is on screen WHILE it
    // streams. Without this, the silence below proves only that the mock is broken.
    await b.evaluate(() => window.__RICHOS_MOCK__.simulateSlowTurn("general", "first, with the wire intact", 30));
    await b.waitForFunction(() => document.querySelectorAll(".tl-rich").length > 0, null, { timeout: 8000 });
    const liveWorks = await b.evaluate(() => document.getElementById("messages").innerText);
    await b.waitForFunction(
      () => !/Working/.test(document.getElementById("messages").innerText),
      null,
      { timeout: 15000 }
    );

    // NOW DROP THE WHOLE TYPED FAMILY. `main.js` calls `window.RichTimeline.on*` by property
    // lookup at delivery time, so replacing them here is exactly a webview that missed every
    // live event — the §13 case: "missed stream events recover from the durable snapshot."
    await b.evaluate(() => {
      window.__dropped = 0;
      for (const k of Object.keys(window.RichTimeline)) {
        if (/^on[A-Z]/.test(k) && typeof window.RichTimeline[k] === "function") {
          window.RichTimeline[k] = function () {
            window.__dropped++;
            return { structural: false, rejected: true };
          };
        }
      }
    });

    await b.evaluate(() => window.__RICHOS_MOCK__.simulateSlowTurn("general", "second, with every live event dropped", 30));
    // Mid-stream: the deltas are being thrown away, so nothing new can be on screen. The
    // first dropped event is the fact waited for (it used to be an 1,800ms sleep, audit R10);
    // the VACUITY assertion below still names a run where none was ever dropped.
    await b.waitForFunction(() => window.__dropped > 0, null, { timeout: 20000 }).catch(() => {});
    await settle(b);
    const midStream = await b.evaluate(() => ({
      dropped: window.__dropped,
      text: document.getElementById("messages").innerText,
    }));
    assert(midStream.dropped > 0, "VACUITY: no live event was dropped, so nothing was recovered from anything");
    assert(
      !midStream.text.includes("second, with every live event dropped"),
      "the dropped events rendered anyway — this check is not measuring what it claims"
    );

    // The turn ends. `rich://turn-completed` triggers the reconciliation reload, and
    // `get_timeline` — the durable snapshot — is the only thing that can put it on screen.
    await b.waitForFunction(
      () => document.getElementById("messages").innerText.includes("second, with every live event dropped"),
      null,
      { timeout: 20000 }
    );
    const recovered = await b.evaluate(() => document.getElementById("messages").innerText);
    const errs = b.__errors.filter((e) => !/RichTimeline/.test(e));
    assertEqual(errs, [], "page errors during the recovery: " + errs.join(" | "));
    await b.close();
    return (
      `${midStream.dropped} live events dropped, screen unchanged mid-turn, then the ` +
      `snapshot restored the whole exchange (live path proven first: ${liveWorks.length} chars on screen)`
    );
  });

  // =====================================================================================
  // 3. RESTART — §14
  // =====================================================================================

  await run.check("§14 a turn that was in flight when the app closed is unknown, never finished", async () => {
    // §14: "Never infer that a turn completed because the app was closed." The `ecs` thread
    // is the durable record of exactly that: a turn still `in_flight` on disk, in a session
    // that has never seen a live event for it.
    const fresh = await openApp(browser);
    const rows = await railMarks(fresh);
    const ecs = rows.find((r) => r.id === "ecs");
    assert(ecs, "VACUITY: there is no thread with a pending turn in this build to check");
    assertEqual(
      ecs.label,
      "ECS architecture, outcome unknown: a turn never finished",
      "§14: a record that stops mid-turn must read as unknown, and say so in the accessible name"
    );
    assertEqual(ecs.glyph, "?", "the mark must be the unknown mark");

    // The label must claim no OUTCOME. "finished" is deliberately not in this list: the
    // shipped wording is *"a turn never finished"*, which is the honest sentence, and a
    // bare-word sweep would forbid the very phrasing it exists to protect.
    const label = ecs.label.toLowerCase();
    for (const word of ["completed", "done", "working", "succeeded", "failed", "error"]) {
      assert(
        !new RegExp("(^|[ ,])" + word).test(label),
        `a turn nobody witnessed the end of is described as "${word}": ${ecs.label}`
      );
    }
    // And the conversation agrees: no turn, no duration row, no invented terminal state.
    await open(fresh, "ecs");
    const t = await turns(fresh);
    assertEqual(t.map((x) => x.duration), [], "a duration row was drawn for a turn with no recorded end");
    await fresh.close();
    return `"${ecs.label}" — the honest read of a record that stops mid-turn`;
  });

  await run.check("§25 a mid-turn crash draws the CEO's prompt exactly once", async () => {
    // §13's `supersedesTurnId` is a MERGE INSTRUCTION, not an announcement. A renderer that
    // ignores it draws the one prompt twice — once for the crashed turn, once for its
    // replay — which is the CEO watching his own words duplicate under a recovery he was
    // never told about.
    const c = await openApp(browser);
    await open(c, "general");
    const prompt = "check the Acme numbers again";
    // SAMPLE THE WHOLE WINDOW, not just the end state. The reconciliation reload at turn
    // end re-projects from the durable snapshot, where the superseded turn contributes
    // nothing — so a reader that looks only after the reload passes even with the
    // renderer's merge instruction disabled (measured). The CEO's complaint is about what
    // is ON SCREEN DURING the recovery, so that is what is measured: the high-water mark of
    // turn sections and CEO bubbles across the crash, the replay and the reload.
    await c.evaluate(() => {
      window.__peakTurns = 0;
      window.__peakBubbles = 0;
      window.__sampler = setInterval(() => {
        window.__peakTurns = Math.max(window.__peakTurns, document.querySelectorAll(".tl-turn").length);
        window.__peakBubbles = Math.max(window.__peakBubbles, document.querySelectorAll(".tl-user-text").length);
      }, 16);
    });
    await c.evaluate((p) => window.__RICHOS_MOCK__.simulateMidTurnCrash("general", p), prompt);
    // **THIS WAIT USED TO BE FOR THE CEO BUBBLE, AND THE REASON IT COULD BE IS GONE.** It
    // said: "the replay's reply arrives on the live wire; the CEO bubble arrives with the
    // reconciliation reload that `rich://turn-completed` triggers, so wait for the bubble
    // rather than for the prose". That was true while nothing live carried his words —
    // and it is exactly the defect Ray measured on his phone at 5.68 s. `rich://ceo-message`
    // now puts his sentence on screen the moment the turn is durable, so the bubble appears
    // DURING the replay and this wait returned on a half-drawn turn: the duration row still
    // read "Working" and the check failed on a screen that was about to be right.
    //
    // So it waits for what it actually needs — the replacement turn to have FINISHED — and
    // the bubble is asserted afterwards rather than used as the clock.
    await c.waitForFunction(
      () => Array.from(document.querySelectorAll(".tl-duration-label")).some((n) => /^Worked for/.test(n.textContent.trim())),
      null,
      { timeout: 20000 }
    );
    await c.waitForFunction(
      (p) => Array.from(document.querySelectorAll(".tl-user-text")).some((n) => n.textContent.includes(p)),
      prompt,
      { timeout: 20000 }
    );
    await settle(c);

    const r = await c.evaluate((p) => {
      clearInterval(window.__sampler);
      const messages = document.getElementById("messages");
      const bubbles = Array.from(messages.querySelectorAll(".tl-user-text")).map((n) => n.textContent);
      const text = messages.innerText;
      return {
        bubbles,
        copies: bubbles.filter((b) => b.includes(p)).length,
        durations: Array.from(messages.querySelectorAll(".tl-duration-label")).map((n) => n.textContent.trim()),
        peakTurns: window.__peakTurns,
        peakBubbles: window.__peakBubbles,
        text,
      };
    }, prompt);

    assert(r.peakTurns > 0, "POSITIVE PROBE: no turn was ever drawn, so the peak measures nothing");
    assertEqual(r.peakTurns, 1, "two turn sections were on screen at once during the recovery");
    assertEqual(r.peakBubbles, 1, "two CEO bubbles were on screen at once during the recovery");
    assert(r.copies > 0, "POSITIVE PROBE: the prompt is not on screen at all");
    assertEqual(r.copies, 1, `the CEO's one prompt was drawn ${r.copies} times: ` + JSON.stringify(r.bubbles));
    assertEqual(r.durations.length, 1, "the crashed turn kept its own duration row: " + JSON.stringify(r.durations));
    assert(/^Worked for/.test(r.durations[0]), "the surviving row is the replacement's completion: " + r.durations[0]);
    assert(!/stopped/i.test(r.text), "a crash was attributed to the CEO");

    // The reload agrees with the live render: a superseded turn contributes nothing.
    await open(c, "acme");
    await open(c, "general");
    const afterReload = await c.evaluate((p) =>
      Array.from(document.querySelectorAll(".tl-user-text")).filter((n) => n.textContent.includes(p)).length, prompt);
    assertEqual(afterReload, 1, "the reload drew the superseded turn's prompt a second time");
    await c.close();
    return `crash -> recovering -> replay: peak ${r.peakTurns} turn section, peak ${r.peakBubbles} CEO bubble on screen at any instant, 1 duration row, and 1 prompt after a re-read`;
  });

  // =====================================================================================
  // Evidence
  // =====================================================================================

  await run.check("SCREENSHOT: a background thread working while another is on screen", async () => {
    const s = await openApp(browser);
    await open(s, "acme");
    await s.evaluate(() => window.__RICHOS_MOCK__.simulateSlowTurn("hiring", "run the numbers again", 200));
    await s.waitForFunction(
      () => {
        const b = document.querySelector('.nav-thread[data-thread-id="hiring"]');
        return b && /working/.test(b.getAttribute("aria-label") || "");
      },
      null,
      { timeout: 5000 }
    );
    const r = await shot(s, "background-thread-working");
    await s.close();
    return `${r.file} (${r.width}x${r.height}, ${r.distinct} distinct colors)`;
  });

  await run.check("no page errors anywhere in this suite", async () => {
    assertEqual(page.__errors, [], "uncaught errors in the shell");
    return "0 uncaught errors, 0 console errors";
  });

  await page.close();
  const failed = run.report();
  await browser.close();
  return failed;
}

main().then((f) => { process.exitCode = f ? 1 : 0; });
