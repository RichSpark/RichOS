// THE LIVE HALF OF §7, and §6.4's opening line — against the REAL renderer under WebKit.
//
// `workers.js` proves what a worker row LOOKS like once it is in the model. It says nothing
// about how it got there, and until 2026-08-29 the answer was: only through a `get_timeline`
// snapshot. `rich://worker-upserted` was deferred in the emitter (live.rs:26) and had no
// listener in `main.js`, so during a turn — exactly when the CEO wants to know Rich has
// delegated — a delegation showed as a nameless "Worked" row. The §26 fixture measured it:
// 0 chips live, 3 after the snapshot.
//
// So this suite never calls `applySnapshot` to get a worker on screen. Every chip below
// arrives through `RichTimeline.onWorkerUpserted`, the function `main.js`'s listener calls,
// with the payload `LiveEvent::WorkerUpserted::payload` puts on the wire.
//
// Four things it proves, in order of importance:
//
//   1. A live turn shows the delegation AS IT HAPPENS, with no snapshot read.
//   2. THE LIVE ROW AND THE RELOADED ROW AGREE — same worker, same wording, same state,
//      same DOM. A row that changes when the turn ends would be a new defect.
//   3. THE CROSS-ENTITY NEGATIVE CONTROL on the live payload, with a positive probe and a
//      vacuity check, so it fails when the guard is removed.
//   4. §6.4's two defaults: a running turn is expanded, a completed one collapses, and the
//      CEO beats both.

"use strict";

const { loadPlaywright, openFixture, shot, createRun, assert, assertEqual } = require("./lib/harness");
const F = require("./lib/fixtures");

/// The wire payload for one `rich://worker-upserted`, byte-shaped like the Rust emitter's:
/// the flattened `worker_activity` timeline item (fence included, `detail` already removed)
/// plus §13's `at` label.
function workerEvent(id, seq, w, at) {
  return Object.assign(F.worker(id, seq, w), { at: at || 1787949000000 });
}

/// The three delegations §7.1 uses as its own example, as three live events.
const SAGE = { agentId: "agt_sage", workerName: "Sage", agentType: "architecture" };
const FRANK = { agentId: "agt_frank", workerName: "Frank", agentType: "red team" };
const CLARK = { agentId: "agt_clark", workerName: "Clark", agentType: "research" };

function started(w, events) {
  return Object.assign({}, w, { observedState: "started", state: "running", eventsObserved: events || 2 });
}

function created(w) {
  return Object.assign({}, w, { observedState: "created", state: "pending_init", eventsObserved: 1 });
}

function ended(w, update) {
  return Object.assign({}, w, {
    observedState: "run_ended",
    state: "unknown",
    eventsObserved: 5,
    latestUpdate: update,
  });
}

/// A LIVE turn in the model, built the way the shell builds one: `rich://turn-status`
/// first, so `turns.get(id).live` is true, then the §13 content events. No snapshot.
const LIVE_TURN_SETUP = `
  window.__model = window.RichTimeline.createModel();
  window.RichTimeline.bind(window.__model, "${F.ENTITY}", "${F.THREAD}", 1);
  window.RichTimeline.onTurnStatus(window.__model, {
    entityId: "${F.ENTITY}", threadId: "${F.THREAD}", turnId: "${F.TURN}", bindingRevision: 1,
    status: "working", startedAt: 1787948100000, activeDurationMs: null, visibility: "ceo",
    at: 1787948100000,
  });
`;

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("§7 live workers + §6.4 disclosure — WebKit");
  const page = await openFixture(browser);

  // =====================================================================================
  // 1. THE DEFECT: a delegation on screen DURING the turn
  // =====================================================================================

  await run.check("a delegation reaches the CEO live — 0 chips before the event, 3 after", async () => {
    const r = await page.evaluate(
      ([setup, events]) => {
        eval(setup);
        window.RichTimeline.onMessageCompleted(window.__model, {
          entityId: "northwind", threadId: "thr_fem", turnId: "turn_ok", bindingRevision: 1,
          messageId: "turn_ok:text:0", phase: "unknown", visibility: "ceo",
          text: "Splitting this three ways — architecture, red team and research.", at: 1787948500000,
        });
        window.__renderOnly();
        // BEFORE: the turn is live and Rich has spoken; no worker event has arrived.
        const before = {
          chips: document.querySelectorAll(".tl-chip").length,
          groups: document.querySelectorAll(".tl-workers").length,
          body: document.body.innerText,
        };
        // AFTER: three `rich://worker-upserted` events, and nothing else.
        const accepted = events.map((e) => window.RichTimeline.onWorkerUpserted(window.__model, e));
        window.__renderOnly();
        const chips = Array.from(document.querySelectorAll(".tl-chip"));
        return {
          before,
          rejected: accepted.filter((a) => a.rejected).length,
          chips: chips.map((c) => ({
            id: c.id,
            name: c.querySelector(".tl-chip-name").textContent,
            role: c.querySelector(".tl-chip-role") ? c.querySelector(".tl-chip-role").textContent : null,
            state: c.querySelector(".tl-chip-state").textContent,
          })),
          groups: document.querySelectorAll(".tl-workers").length,
          summary: document.querySelector(".tl-workers-head .tl-activity-text").textContent,
        };
      },
      [
        LIVE_TURN_SETUP,
        [
          workerEvent("m_w_sage", 1, started(SAGE)),
          workerEvent("m_w_frank", 2, started(FRANK)),
          workerEvent("m_w_clark", 3, started(CLARK)),
        ],
      ]
    );

    assertEqual(r.before.chips, 0, "the control: nothing delegated yet, nothing drawn");
    for (const n of ["Sage", "Frank", "Clark"]) {
      assert(!r.before.body.includes(n), `"${n}" must not be on screen before his event arrives`);
    }
    assertEqual(r.rejected, 0, "all three live events were accepted by the fence");
    assertEqual(
      r.chips.map((c) => `${c.name}/${c.role}/${c.state}`),
      ["Sage/architecture/Working", "Frank/red team/Working", "Clark/research/Working"],
      "three chips, three roles, live — with no `get_timeline` call in this test at all"
    );
    assertEqual(r.groups, 1, "§7.1: one group, not three identical rows");
    assertEqual(r.summary, "Sage, Frank and Clark started working", "§7.1's grouped summary, verbatim");
    return `before: 0 chips. after 3 live events: ${r.chips.length} chips in 1 group — "${r.summary}"`;
  });

  await run.check("a live turn needs no click to show its work (§6.4 first line)", async () => {
    const r = await page.evaluate(() => ({
      expanded: document.querySelector(".tl-duration-btn").getAttribute("aria-expanded"),
      chips: document.querySelectorAll(".tl-chip").length,
    }));
    // Nothing in the check above touched the disclosure. The chips are on screen because
    // §6.4's live default put them there, not because a test opened the transcript.
    assertEqual(r.expanded, "true", "while active, expanded by default");
    assertEqual(r.chips, 3, "and the work is genuinely drawn, not merely marked open");
    return "aria-expanded=true with 3 chips, disclosure never clicked";
  });

  // =====================================================================================
  // 2. THE LIVE ROW AND THE RELOADED ROW AGREE
  // =====================================================================================

  await run.check("the live DOM and the reloaded DOM are byte-identical", async () => {
    // The property that makes a live row safe to draw. The backend proves it on the wire
    // (`the_live_worker_row_and_the_reloaded_worker_row_are_the_same_row`); this proves the
    // renderer does not then draw the two differently — a chip whose wording changed when
    // the snapshot landed would be exactly the defect this slice was told not to create.
    const r = await page.evaluate(
      ([setup, events, snap]) => {
        eval(setup);
        for (const e of events) window.RichTimeline.onWorkerUpserted(window.__model, e);
        window.__renderOnly();
        const live = document.getElementById("messages").innerHTML;
        const liveText = document.body.innerText;

        // Now the reload path, with the SAME items — which is what `get_timeline` returns
        // for these records — through `applySnapshot`, the function `main.js` calls.
        window.RichTimeline.applySnapshot(window.__model, snap);
        window.__renderOnly();
        return {
          live,
          liveText,
          reloaded: document.getElementById("messages").innerHTML,
          reloadedText: document.body.innerText,
          items: window.__model.items.size,
        };
      },
      [
        LIVE_TURN_SETUP,
        [
          workerEvent("m_w_sage", 1, created(SAGE)),
          workerEvent("m_w_frank", 2, started(FRANK)),
          workerEvent("m_w_clark", 3, ended(CLARK, "Pulled 14 sources on Claude Code memory")),
        ],
        F.snapshot([
          F.worker("m_w_sage", 1, created(SAGE)),
          F.worker("m_w_frank", 2, started(FRANK)),
          F.worker("m_w_clark", 3, ended(CLARK, "Pulled 14 sources on Claude Code memory")),
          F.duration("working", null),
        ]),
      ]
    );

    assertEqual(r.reloadedText, r.liveText, "the reloaded render says something different from the live one");
    assertEqual(r.reloaded, r.live, "and it is not merely the same words — it is the same DOM");
    assertEqual(r.items, 3, "three ids, three rows: the reload RE-STATES what was live, it does not duplicate");
    assert(r.liveText.includes("Ended"), "the ended run reads `Ended` in both");
    assert(r.liveText.includes("outcome not recorded"), "with the same caveat in both");
    return "identical innerHTML and innerText across the live path and the snapshot path; 3 items, not 6";
  });

  await run.check("an upgraded row REPLACES its activity row rather than merging over it", async () => {
    // THE RACE, at the renderer. The `agentId` is only extractable from the tool RESULT and
    // the engine's `created` hook writes at about that same instant, so a delegation can be
    // drawn as an ordinary activity row first and upgraded to a worker row under the SAME
    // machinery id one record later (live.rs). The upsert must not leave the activity row's
    // `summary`, `state` and `activityType` clinging to the worker row: those are a
    // different vocabulary over the same field names, and the values would be stale.
    const r = await page.evaluate(
      ([setup, act, worker]) => {
        eval(setup);
        window.RichTimeline.onActivityUpserted(window.__model, act);
        window.__renderOnly();
        const asActivity = {
          chips: document.querySelectorAll(".tl-chip").length,
          rows: Array.from(document.querySelectorAll(".tl-activity-text")).map((n) => n.textContent),
        };
        window.RichTimeline.onWorkerUpserted(window.__model, worker);
        window.__renderOnly();
        const item = window.__model.items.get("m_race");
        return {
          asActivity,
          items: window.__model.items.size,
          kind: item.kind,
          leftovers: ["summary", "state", "activityType"].filter((k) => k in item),
          chips: Array.from(document.querySelectorAll(".tl-chip")).map((c) => c.querySelector(".tl-chip-name").textContent),
          rows: Array.from(document.querySelectorAll(".tl-activity-text")).map((n) => n.textContent),
        };
      },
      [
        LIVE_TURN_SETUP,
        Object.assign(F.activity("m_race", "Worked", 1, "unknown"), { at: 1787949000000 }),
        workerEvent("m_race", 1, started(CLARK)),
      ]
    );

    assertEqual(r.asActivity.chips, 0, "the control: before the lifecycle row landed it WAS a nameless row");
    assert(r.asActivity.rows.includes("Worked"), "and it read exactly that: " + r.asActivity.rows.join(" | "));
    assertEqual(r.items, 1, "ONE machinery record is ONE row — the upgrade is not a second item");
    assertEqual(r.kind, "worker_activity");
    assertEqual(r.leftovers, [], "no field of the activity vocabulary survived the upgrade");
    assertEqual(r.chips, ["Clark"], "and the row is now the worker it always was");
    assert(!r.rows.includes("Worked"), "the nameless row is gone, not stacked under the chip");
    return `"Worked" (0 chips) -> Clark (1 chip), 1 item throughout, 0 stale fields`;
  });

  // =====================================================================================
  // 3. THE CROSS-ENTITY NEGATIVE CONTROL, ON THE LIVE PAYLOAD
  // =====================================================================================

  await run.check("CROSS-ENTITY: a live worker event from another entity never renders", async () => {
    // A worker row is the richest thing on this wire — a name, a role and a worker's own
    // authored words — so it is the row a leak would be most visible on. And `agentId` is
    // NOT globally unique, which is why the same id appears on both payloads here: the
    // engine's own residue reuses ids across sessions.
    const r = await page.evaluate(
      ([setup, mine, foreign]) => {
        eval(setup);
        // POSITIVE PROBE first: the identical event, correctly scoped, IS accepted — so the
        // refusal below cannot pass merely because the whole path is broken.
        const okAccepted = window.RichTimeline.onWorkerUpserted(window.__model, mine).rejected === false;
        const leakRejected = window.RichTimeline.onWorkerUpserted(window.__model, foreign).rejected === true;
        // THE GUARD, DELETED: the entity clause removed, re-implemented inline.
        const withoutEntityClause = (m, p) => p.threadId === m.threadId;
        const wouldLeak = withoutEntityClause(window.__model, foreign);
        window.__renderOnly();
        return {
          okAccepted,
          leakRejected,
          wouldLeak,
          items: window.__model.items.size,
          body: document.body.innerText,
        };
      },
      [
        LIVE_TURN_SETUP,
        workerEvent("m_ok", 1, started(SAGE)),
        Object.assign(
          workerEvent("m_leak", 2, {
            agentId: "agt_sage", // the SAME id
            workerName: "deeply-analyst",
            agentType: "lumen",
            observedState: "updated",
            state: "running",
            latestUpdate: "deeply's Q4 term sheet numbers",
            eventsObserved: 3,
          }),
          { entityId: "lumen" } // <-- another entity, same thread id
        ),
      ]
    );

    assert(r.okAccepted, "POSITIVE PROBE FAILED — the correctly-scoped live worker event was refused too");
    assert(r.leakRejected, "a worker event stamped with another entity was ACCEPTED into this thread");
    assert(r.wouldLeak, "the control is vacuous: without the entity clause the payload is admitted");
    assertEqual(r.items, 1, "only the legitimate row entered the model");
    assert(!r.body.includes("deeply-analyst"), "leaked worker name into the render: " + r.body);
    assert(!r.body.includes("Q4 term sheet"), "leaked another entity's authored update into the render");
    assert(r.body.includes("Sage"), "and the legitimate worker still rendered");
    return "guard on: rejected. guard off (entity clause deleted): admitted. Neither name nor authored words reached the DOM.";
  });

  await run.check("a live worker event below the activation revision is stale and refused", async () => {
    // The fence's other half, on this event: `bindingRevision` is a STALENESS fence and
    // never an equality key — an equality check would reject every live event after a
    // thread switch, which is how an app goes silent the first time the CEO changes threads.
    const r = await page.evaluate(
      ([setup, at2, at3, at9]) => {
        eval(setup);
        window.RichTimeline.bind(window.__model, "northwind", "thr_fem", 3);
        return {
          older: window.RichTimeline.onWorkerUpserted(window.__model, at2).rejected,
          equal: window.RichTimeline.onWorkerUpserted(window.__model, at3).rejected,
          higher: window.RichTimeline.onWorkerUpserted(window.__model, at9).rejected,
        };
      },
      [
        LIVE_TURN_SETUP,
        Object.assign(workerEvent("m_r2", 1, started(SAGE)), { bindingRevision: 2 }),
        Object.assign(workerEvent("m_r3", 2, started(FRANK)), { bindingRevision: 3 }),
        Object.assign(workerEvent("m_r9", 3, started(CLARK)), { bindingRevision: 9 }),
      ]
    );
    assertEqual(r, { older: true, equal: false, higher: false });
    return "rev 2 refused (stale), rev 3 accepted, rev 9 accepted (a later activation)";
  });

  // =====================================================================================
  // 4. §6.4's TWO DEFAULTS, AND THE CEO BEATING BOTH
  // =====================================================================================

  await run.check("§6.4: active is expanded, completed is collapsed — same turn, no clicks", async () => {
    const r = await page.evaluate(
      ([setup, events]) => {
        eval(setup);
        for (const e of events) window.RichTimeline.onWorkerUpserted(window.__model, e);
        window.__renderOnly();
        const active = {
          expanded: document.querySelector(".tl-duration-btn").getAttribute("aria-expanded"),
          chips: document.querySelectorAll(".tl-chip").length,
        };
        // The turn completes. `main.js` runs the 180ms settle; here the same two lines are
        // applied directly, because the settle timer is the shell's and this page is the
        // renderer in isolation.
        window.RichTimeline.onTurnStatus(window.__model, {
          entityId: "northwind", threadId: "thr_fem", turnId: "turn_ok", bindingRevision: 1,
          status: "completed", startedAt: 1787948100000, activeDurationMs: 461000, visibility: "ceo",
          at: 1787948561000,
        });
        window.__renderOnly();
        const settling = {
          expanded: document.querySelector(".tl-duration-btn").getAttribute("aria-expanded"),
          chips: document.querySelectorAll(".tl-chip").length,
        };
        window.__model.settled.add("turn_ok");
        window.__model.expanded.delete("turn_ok");
        window.__renderOnly();
        return {
          active,
          settling,
          settled: {
            expanded: document.querySelector(".tl-duration-btn").getAttribute("aria-expanded"),
            chips: document.querySelectorAll(".tl-chip").length,
            summary: document.querySelector(".tl-collapsed-summary").textContent,
          },
        };
      },
      [LIVE_TURN_SETUP, [workerEvent("m_w_sage", 1, started(SAGE)), workerEvent("m_w_frank", 2, started(FRANK))]]
    );

    assertEqual(r.active.expanded, "true", "§6.4 line 1: while active, expanded by default");
    assertEqual(r.active.chips, 2, "with the work drawn");
    // THE INTERACTION THAT MATTERS. The terminal status must not slam the transcript shut —
    // §6.4 asks for a settling TRANSITION, and a collapse that has already happened has
    // nothing left to transition. The live default is carried forward until the settle runs.
    assertEqual(r.settling.expanded, "true", "the terminal status alone does not collapse it");
    assertEqual(r.settling.chips, 2, "the work is still on screen for the settle to close over");
    assertEqual(r.settled.expanded, "false", "§6.4 line 2: the settle collapses it");
    assertEqual(r.settled.chips, 0, "and the worker rows go inside the collapsed transcript");
    assert(r.settled.summary.includes("Sage and Frank"), "the one-line summary still names them: " + r.settled.summary);
    return `active: expanded, 2 chips -> terminal status: still expanded (the transition survives) -> settled: collapsed, "${r.settled.summary}"`;
  });

  await run.check("the CEO beats BOTH defaults, in both directions", async () => {
    const r = await page.evaluate(
      ([setup, events]) => {
        eval(setup);
        for (const e of events) window.RichTimeline.onWorkerUpserted(window.__model, e);
        window.__renderOnly();
        const read = () => document.querySelector(".tl-duration-btn").getAttribute("aria-expanded");
        const openByDefault = read();

        // (a) HE CLOSES IT MID-TURN. A rule that re-derived openness from liveness on every
        // render would re-open this on the very next event, which is the half a naive fix
        // gets wrong.
        document.querySelector(".tl-duration-btn").click();
        const closedByCeo = read();
        window.RichTimeline.onWorkerUpserted(window.__model, Object.assign({}, events[0], {
          worker: Object.assign({}, events[0].worker, { observedState: "run_ended", state: "unknown" }),
        }));
        window.__renderOnly();
        const stillClosed = read();

        // (b) HE OPENS IT, then the turn completes and the settle runs. `main.js` skips the
        // collapse for a turn he has touched (`settled`), and `toggleTurn` marks it.
        document.querySelector(".tl-duration-btn").click();
        const openedByCeo = read();
        const settledAfterOpen = window.__model.settled.has("turn_ok");
        window.RichTimeline.onTurnStatus(window.__model, {
          entityId: "northwind", threadId: "thr_fem", turnId: "turn_ok", bindingRevision: 1,
          status: "completed", startedAt: 1787948100000, activeDurationMs: 461000, visibility: "ceo",
          at: 1787948561000,
        });
        window.__renderOnly();
        return {
          openByDefault,
          closedByCeo,
          stillClosed,
          openedByCeo,
          settledAfterOpen,
          afterCompletion: read(),
          chipsAfterCompletion: document.querySelectorAll(".tl-chip").length,
        };
      },
      [LIVE_TURN_SETUP, [workerEvent("m_w_sage", 1, started(SAGE)), workerEvent("m_w_frank", 2, started(FRANK))]]
    );

    assertEqual(r.openByDefault, "true", "the live default, before he touches anything");
    assertEqual(r.closedByCeo, "false", "he closed it while the turn was running");
    assertEqual(r.stillClosed, "false", "and a further live event did NOT re-open it over him");
    assertEqual(r.openedByCeo, "true", "he opened it again");
    assert(r.settledAfterOpen, "a deliberate open marks the turn settled, so the collapse leaves it alone");
    assertEqual(r.afterCompletion, "true", "and completion did not close what he deliberately opened");
    assertEqual(r.chipsAfterCompletion, 2, "his work stays on screen");
    return "default open -> CEO closes (holds across events) -> CEO opens -> completion leaves it open";
  });

  await run.check("a REPLAYED turn's disclosure state does not follow the dead turn id", async () => {
    // `supersedesTurnId` merges a crashed turn into its replacement. A stale entry in
    // `collapsed` would silently suppress §6.4's live default under the new id, with nothing
    // on screen to say why.
    const r = await page.evaluate(([setup]) => {
      eval(setup);
      window.RichTimeline.toggleTurn(window.__model, "turn_ok"); // the CEO closes the doomed turn
      const closed = window.__model.collapsed.has("turn_ok");
      window.RichTimeline.onTurnStatus(window.__model, {
        entityId: "northwind", threadId: "thr_fem", turnId: "turn_new", bindingRevision: 1,
        supersedesTurnId: "turn_ok", status: "working", startedAt: 1787948200000,
        activeDurationMs: null, visibility: "ceo", at: 1787948200000,
      });
      return {
        closed,
        deadStillTracked:
          window.__model.collapsed.has("turn_ok") ||
          window.__model.expanded.has("turn_ok") ||
          window.__model.settled.has("turn_ok"),
        replacementExpanded: window.RichTimeline.isTurnExpanded(window.__model, "turn_new"),
      };
    }, [LIVE_TURN_SETUP]);
    assert(r.closed, "the control: the CEO's collapse was recorded on the turn that then died");
    assert(!r.deadStillTracked, "no disclosure state survives the dropped turn id");
    assert(r.replacementExpanded, "and the replacement, being live, opens by default like any live turn");
    return "collapse recorded -> turn superseded -> all three sets cleared; the replacement opens by default";
  });

  // =====================================================================================
  // Evidence
  // =====================================================================================

  await run.check("SCREENSHOT: a live turn with its delegations on screen, unclicked", async () => {
    await page.evaluate(
      ([setup, events]) => {
        eval(setup);
        window.RichTimeline.onMessageCompleted(window.__model, {
          entityId: "northwind", threadId: "thr_fem", turnId: "turn_ok", bindingRevision: 1,
          messageId: "turn_ok:text:0", phase: "unknown", visibility: "ceo",
          text: "Splitting this three ways — architecture, red team and research.", at: 1787948500000,
        });
        for (const e of events) window.RichTimeline.onWorkerUpserted(window.__model, e);
        window.__renderOnly();
      },
      [
        LIVE_TURN_SETUP,
        [
          workerEvent("m_w_sage", 1, created(SAGE)),
          workerEvent("m_w_frank", 2, started(FRANK)),
          workerEvent("m_w_clark", 3, ended(CLARK, "Pulled 14 sources on Claude Code memory")),
        ],
      ]
    );
    // What the shot must actually CONTAIN, asserted before it counts as evidence — a
    // pixel-verified PNG of the wrong screen is still the wrong screen.
    const drawn = await page.evaluate(() => ({
      chips: Array.from(document.querySelectorAll(".tl-chip")).map((c) => c.querySelector(".tl-chip-name").textContent),
      expanded: document.querySelector(".tl-duration-btn").getAttribute("aria-expanded"),
    }));
    assertEqual(drawn.chips, ["Sage", "Frank", "Clark"], "the three delegations are in the frame");
    assertEqual(drawn.expanded, "true", "and the transcript is open by §6.4's live default, not by a click");
    const s = await shot(page, "live-workers-during-a-turn");
    assert(s.bytes > 3000, "a suspiciously small PNG is not evidence: " + s.bytes + " bytes");
    return `${s.file} — ${s.width}x${s.height}, ${s.distinct} distinct colors, ${s.bytes} bytes; Sage, Frank, Clark in frame`;
  });

  await run.check("no page errors anywhere in this suite", async () => {
    assertEqual(page.__errors, [], "the renderer threw");
    return "0 uncaught errors, 0 console errors";
  });

  await page.close();
  await browser.close();
  return run.report();
}

main().then(
  (failed) => { process.exitCode = failed ? 1 : 0; },
  (e) => {
    console.error(e);
    process.exit(1);
  }
);
