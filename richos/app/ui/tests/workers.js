// §25 "AI workers" — every criterion, one at a time, against the REAL renderer under WebKit.
//
// Plus the two things a slice in this sequence is not finished without:
//   * THE REGRESSION, proven closed with a before/after on the same payload; and
//   * a CROSS-ENTITY NEGATIVE CONTROL on the rendered worker row, with a positive probe,
//     that FAILS when the guard is removed.

"use strict";

const { loadPlaywright, openFixture, shot, createRun, assert, assertEqual } = require("./lib/harness");
const F = require("./lib/fixtures");

async function main() {
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const run = createRun("§25 AI workers — WebKit");

  const page = await openFixture(browser);

  // =====================================================================================
  // THE REGRESSION
  // =====================================================================================

  await run.check("REGRESSION before/after: a delegated worker renders in the CEO timeline", async () => {
    const snap = F.threeWorkers();

    // BEFORE — the renderer exactly as main ships it. main's `RENDERED_STREAM_KINDS` omits
    // `worker_activity`, so those items never enter `turn.stream` and nothing else in the
    // file consumes them: the DOM main produces for this payload is identical to the DOM
    // this build produces for the payload with the worker items removed. That is what is
    // rendered below — a faithful simulation, not a claim.
    const before = await page.evaluate((s) => {
      const asMainDraws = { ...s, items: s.items.filter((i) => i.kind !== "worker_activity") };
      window.__render(asMainDraws, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      const drawn = { chips: document.querySelectorAll(".tl-chip").length, bodyText: document.body.innerText };

      // And the model main actually holds: the rows ARE in the payload, and main files them
      // under `turn.unrendered` rather than dropping them silently. Reported is not drawn,
      // and the CEO does not read `turn.unrendered`.
      window.__render(s, {});
      const turn = window.RichTimeline.turnsOf(window.__model)[0];
      return {
        drawn,
        unrendered: turn.unrendered.map((i) => i.kind),
        inStream: turn.stream.filter((i) => i.kind === "worker_activity").length,
      };
    }, snap);

    assertEqual(before.drawn.chips, 0, "main draws no chip");
    for (const name of ["Sage", "Frank", "Clark"]) {
      assert(
        !before.drawn.bodyText.includes(name),
        `THE REGRESSION: "${name}" appears nowhere in the CEO timeline main renders`
      );
    }
    assertEqual(before.inStream, 3, "this build DOES route the three worker rows into the lane");

    // AFTER — this build, expanded so the work transcript is open.
    const after = await page.evaluate(() => {
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      const chips = Array.from(document.querySelectorAll(".tl-chip"));
      return {
        groups: document.querySelectorAll(".tl-workers").length,
        chips: chips.length,
        names: chips.map((c) => c.querySelector(".tl-chip-name").textContent),
        states: chips.map((c) => c.querySelector(".tl-chip-state").textContent),
        summary: document.querySelector(".tl-workers-head .tl-activity-text").textContent,
        bodyText: document.body.innerText,
      };
    });

    assertEqual(after.groups, 1, "three consecutive worker rows are ONE group, not three rows (§7.1)");
    assertEqual(after.names, ["Sage", "Frank", "Clark"], "every delegated worker is named");
    for (const n of ["Sage", "Frank", "Clark"]) {
      assert(after.bodyText.includes(n), `${n} must be visible in the CEO timeline`);
    }
    return (
      `before: 0 chips, 0 worker rows drawn; the three names appear nowhere in the render\n` +
      `          after:  ${after.groups} group, ${after.chips} chips — ${after.names.join(", ")}\n` +
      `          summary line: "${after.summary}"\n` +
      `          states: ${after.states.join(" | ")}`
    );
  });

  // =====================================================================================
  // §25 "AI workers", criterion by criterion
  // =====================================================================================

  await run.check("Sage, Frank and Clark can start concurrently", async () => {
    const r = await page.evaluate((s) => {
      window.__render(s, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      return {
        chips: document.querySelectorAll(".tl-chip").length,
        summary: document.querySelector(".tl-workers-head .tl-activity-text").textContent,
      };
    }, F.allRunning());
    assertEqual(r.chips, 3, "three chips");
    assertEqual(r.summary, "Sage, Frank and Clark started working", "§7.1's own example, verbatim");
    return `"${r.summary}"`;
  });

  await run.check("their states update independently", async () => {
    const r = await page.evaluate((s) => {
      window.__render(s, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      return Array.from(document.querySelectorAll(".tl-chip")).map((c) => ({
        name: c.querySelector(".tl-chip-name").textContent,
        state: c.querySelector(".tl-chip-state").textContent,
        tone: c.dataset.state,
        qualifier: c.querySelector(".tl-chip-qualifier") ? c.querySelector(".tl-chip-qualifier").textContent : null,
      }));
    }, F.threeWorkers());
    assertEqual(
      r,
      [
        { name: "Sage", state: "Starting", tone: "starting", qualifier: null },
        { name: "Frank", state: "Working", tone: "active", qualifier: null },
        { name: "Clark", state: "Ended", tone: "ended", qualifier: "outcome not recorded" },
      ],
      "one group, three independent states"
    );
    return r.map((x) => `${x.name}=${x.state}`).join(" ");
  });

  await run.check("grouped start and completion summaries avoid repetitive rows", async () => {
    const started = await page.evaluate((s) => {
      window.__render(s, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      return {
        groups: document.querySelectorAll(".tl-workers").length,
        text: document.querySelector(".tl-workers-head .tl-activity-text").textContent,
      };
    }, F.allRunning());
    const ended = await page.evaluate((s) => {
      window.__render(s, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      return {
        groups: document.querySelectorAll(".tl-workers").length,
        text: document.querySelector(".tl-workers-head .tl-activity-text").textContent,
      };
    }, F.allEnded());
    assertEqual(started.groups, 1, "one row for three starts");
    assertEqual(ended.groups, 1, "one row for three endings");
    assertEqual(started.text, "Sage, Frank and Clark started working");
    assertEqual(ended.text, "Sage, Frank and Clark are no longer running");
    return `start: "${started.text}"\n          end:   "${ended.text}"`;
  });

  await run.check("no worker state is invented when the task source is unavailable", async () => {
    const r = await page.evaluate(() => {
      const spec = window.RichTimeline.workerStateSpec;
      return {
        // The three the engine can witness.
        pending: spec("pending_init").label,
        running: spec("running").label,
        unknown: spec("unknown").label,
        unknownQualifier: spec("unknown").qualifier,
        // The four §7.1 names that have NO signal. If one ever arrives it must not be
        // dressed as a verdict this build cannot support.
        waiting: spec("waiting").label,
        completed: spec("completed").label,
        interrupted: spec("interrupted").label,
        failed: spec("failed").label,
        // And a name-less worker falls back through what was OBSERVED, never to a raw id.
        noName: window.RichTimeline.workerDisplayName({ agentId: "agt_x1y2" }),
        typeOnly: window.RichTimeline.workerDisplayName({ agentId: "agt_x", agentType: "sage" }),
        // A named teammate as the engine starts it (proto-teammate shelf plan §7): its name.
        named: window.RichTimeline.workerDisplayName({ agentId: "agt_m", workerName: "mark-sonnet-1a2b3c4d5e6f" }),
        namedOpus: window.RichTimeline.workerDisplayName({ agentId: "agt_f", workerName: "frank-opus-0f1e2d3c4b5a" }),
        otherShape: window.RichTimeline.workerDisplayName({ agentId: "agt_o", workerName: "deeply-analyst" }),
      };
    });
    assertEqual(r.named, "Mark", "a named teammate reads as its name, not the engine's agent name");
    assertEqual(r.namedOpus, "Frank");
    assertEqual(r.otherShape, "deeply-analyst", "a name of another shape is shown as it came");
    assertEqual(r.unknown, "Ended", "run_ended must not read as Done/Finished/Completed");
    assertEqual(r.unknownQualifier, "outcome not recorded");
    for (const [k, v] of Object.entries(r)) {
      if (["waiting", "completed", "interrupted", "failed"].includes(k)) {
        assertEqual(v, "Status unavailable", `${k} has no signal and must claim nothing`);
      }
    }
    assert(!r.noName.includes("agt_"), "an opaque harness id is machinery, not a display name");
    assertEqual(r.noName, "A teammate");
    assertEqual(r.typeOnly, "sage");
    return `unknown -> "${r.unknown} · ${r.unknownQualifier}"; the four unwitnessed states -> "Status unavailable"`;
  });

  await run.check("`Ended` never carries a success, failure or broken word", async () => {
    const text = await page.evaluate((s) => {
      window.__render(s, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      return document.body.innerText.toLowerCase();
    }, F.allEnded());
    for (const banned of [
      "done",
      "finished",
      "completed",
      "complete",
      "success",
      "succeeded",
      "failed",
      "failure",
      "error",
      "crashed",
      "unavailable",
      "unknown",
      "stopped",
      "interrupted",
    ]) {
      assert(!text.includes(banned), `the ended-run wording must not contain "${banned}" — found it in the render`);
    }
    return "checked 14 words that would imply success, failure or breakage";
  });

  await run.check("one worker can end and another restart without resetting the parent turn", async () => {
    // The half of §25's "one worker can fail and restart" that this build can actually
    // witness. FAILURE IS NOT DETECTABLE (no payload carries an outcome), so the fixture
    // uses the observable analogue: a run that ENDED, and a second worker whose run
    // reopened — the engine's stream is a SEQUENCE, so a later `started` reopens a run.
    const snap = F.snapshot([
      F.userMessage(),
      F.richMessage(0, "Sage stopped reporting. Clark is picking the unfinished pass back up.", 0),
      F.worker("m_w_sage", 1, {
        agentId: "agt_sage",
        workerName: "Sage",
        agentType: "architecture",
        observedState: "run_ended",
        state: "unknown",
        eventsObserved: 3,
      }),
      F.worker("m_w_clark", 2, {
        agentId: "agt_clark",
        workerName: "Clark",
        agentType: "architecture",
        observedState: "started",
        state: "running",
        eventsObserved: 4,
      }),
      F.duration("working", null),
    ]);
    const r = await page.evaluate((s) => {
      const read = () => ({
        label: document.querySelector(".tl-duration-label").textContent,
        tone: document.querySelector(".tl-duration").dataset.tone,
      });
      // The turn WITHOUT any worker rows — the control.
      window.__render({ ...s, items: s.items.filter((i) => i.kind !== "worker_activity") }, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      const control = read();

      window.__render(s, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      const chips = Array.from(document.querySelectorAll(".tl-chip"));
      return {
        control,
        withWorkers: read(),
        states: chips.map(
          (c) => c.querySelector(".tl-chip-name").textContent + "=" + c.querySelector(".tl-chip-state").textContent
        ),
        prose: Array.from(document.querySelectorAll(".tl-prose")).map((p) => p.textContent),
      };
    }, snap);
    assertEqual(r.states, ["Sage=Ended", "Clark=Working"], "one ended, one running, in one group");
    assertEqual(
      r.withWorkers,
      r.control,
      "THE PARENT TURN IS UNTOUCHED: its row is byte-identical with and without a worker that ended"
    );
    assert(r.prose[0].includes("picking the unfinished pass back up"), "Rich's recovery commentary stays readable");
    return `${r.states.join(", ")}; parent row identical with and without the ended worker: "${r.control.label}" (${r.control.tone})`;
  });

  await run.check("worker activity collapses with the rest of the work, prose does not", async () => {
    const r = await page.evaluate((s) => {
      window.__render(s, {});
      window.__model.expanded.delete("turn_ok"); // collapsed — the post-completion default
      window.__renderOnly();
      const collapsed = {
        chips: document.querySelectorAll(".tl-chip").length,
        prose: Array.from(document.querySelectorAll(".tl-prose")).map((p) => p.textContent),
        summary: document.querySelector(".tl-collapsed-summary").textContent,
      };
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      const expanded = {
        chips: document.querySelectorAll(".tl-chip").length,
        prose: Array.from(document.querySelectorAll(".tl-prose")).map((p) => p.textContent),
      };
      return { collapsed, expanded };
    }, F.threeWorkers());
    assertEqual(r.collapsed.chips, 0, "§6.4: worker lifecycle rows are inside the collapsed transcript");
    assertEqual(r.expanded.chips, 3, "and they come back when it is reopened");
    assertEqual(r.collapsed.prose, r.expanded.prose, "prose is identical in both states — the collapse never hides it");
    assert(
      r.collapsed.summary.includes("Sage, Frank and Clark"),
      "the one-line collapsed summary still names the workers: " + r.collapsed.summary
    );
    return `collapsed summary: "${r.collapsed.summary}"`;
  });

  await run.check("§18: chips are buttons whose accessible name carries name, role and state", async () => {
    const r = await page.evaluate((s) => {
      window.__render(s, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      const chips = Array.from(document.querySelectorAll(".tl-chip"));
      return chips.map((c) => ({ tag: c.tagName, type: c.type, label: c.getAttribute("aria-label") }));
    }, F.threeWorkers());
    for (const c of r) {
      assertEqual(c.tag, "BUTTON", "§18: worker chips are buttons");
      assertEqual(c.type, "button", "never a submit");
    }
    assert(r[0].label.startsWith("Sage, architecture, Starting"), "got: " + r[0].label);
    assert(
      r[2].label.includes("Ended, outcome not recorded") && r[2].label.includes("latest update:"),
      "the ended chip's caveat and its authored update reach a screen reader: " + r[2].label
    );
    return r.map((c) => `"${c.label}"`).join("\n          ");
  });

  await run.check("§18: every chip is reachable and operable by keyboard alone", async () => {
    await page.evaluate((s) => {
      window.__render(s, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      window.__events = [];
    }, F.threeWorkers());
    // The renderer's actual keyboard contract, measured rather than assumed: a chip is a
    // NATIVE button, in document order, with no `tabindex` override — so it sits in the tab
    // order the platform builds — and BOTH Enter and Space activate it.
    //
    // MEASURED PLATFORM NOTE, not a product defect: pressing Tab from a focused chip in
    // headless WebKit moves focus to BODY, because macOS ships "Full Keyboard Access" OFF
    // and WebKit honours it — under that setting Tab reaches text fields and lists only, and
    // no button in this app is tabbable, including the Copy and duration controls that
    // predate this slice. Asserting "Tab reaches the next chip" would be asserting a system
    // preference. What IS the renderer's to guarantee is asserted below.
    const structure = await page.evaluate(() => {
      const chips = Array.from(document.querySelectorAll(".tl-chip"));
      const focusable = Array.from(
        document.getElementById("messages").querySelectorAll("button, [tabindex]:not([tabindex='-1'])")
      );
      return {
        tags: chips.map((c) => c.tagName),
        tabindexOverrides: chips.filter((c) => c.hasAttribute("tabindex")).length,
        // Every chip is in the focusable set, in the same order it is drawn.
        orderMatches:
          JSON.stringify(chips.map((c) => c.id)) ===
          JSON.stringify(focusable.filter((f) => f.classList.contains("tl-chip")).map((f) => f.id)),
        ids: chips.map((c) => c.id),
      };
    });
    assertEqual(structure.tags, ["BUTTON", "BUTTON", "BUTTON"], "native buttons, not clickable divs");
    assertEqual(structure.tabindexOverrides, 0, "nothing is pulled out of, or forced into, the tab order");
    assert(structure.orderMatches, "focus order must match visual order");

    const opened = [];
    for (const [id, key] of [
      ["chip\\:agt_sage", "Enter"],
      ["chip\\:agt_frank", "Space"],
      ["chip\\:agt_clark", "Enter"],
    ]) {
      await page.focus("#" + id);
      const label = await page.evaluate(() => document.activeElement.getAttribute("aria-label"));
      assert(label && label.length > 0, "a focused chip announces itself: " + label);
      await page.keyboard.press(key);
    }
    const fired = await page.evaluate(() => window.__events.map((e) => e.agentId));
    assertEqual(fired, ["agt_sage", "agt_frank", "agt_clark"], "Enter AND Space both open the inspector");
    void opened;
    return (
      "3 native buttons, no tabindex overrides, focus order == visual order; Enter and Space both activate.\n" +
      "          Measured: Tab from a chip lands on BODY in headless WebKit — macOS Full Keyboard Access is a system\n" +
      "          preference and no button in this app is tabbable without it, including pre-existing ones."
    );
  });

  await run.check("§18/§17.4: reduced motion replaces the pulse with a static mark", async () => {
    const motion = await openFixture(browser);
    await motion.emulateMedia({ reducedMotion: "reduce" });
    const r = await motion.evaluate((s) => {
      window.__render(s, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
      const pulse = document.querySelector(".tl-chip-pulse");
      const st = getComputedStyle(pulse);
      const chip = document.querySelector('.tl-chip[data-state="active"]');
      return {
        pulses: document.querySelectorAll(".tl-chip-pulse").length,
        animation: st.animationName,
        chipTransition: getComputedStyle(chip).transitionDuration,
        // Status must never rely on color alone (§18): the state word is real text.
        stateWord: chip.querySelector(".tl-chip-state").textContent,
      };
    }, F.threeWorkers());
    assertEqual(r.pulses, 1, "exactly ONE pulse — §17.4 forbids multiple simultaneous spinners");
    assertEqual(r.animation, "none", "the pulse stops moving under prefers-reduced-motion");
    assert(/^0s(, 0s)*$/.test(r.chipTransition), "chip transitions are off too: " + r.chipTransition);
    assertEqual(r.stateWord, "Working", "the mark stays — it is the status");
    await motion.close();
    return `1 pulse, animation-name: ${r.animation}, transitions: ${r.chipTransition}`;
  });

  // =====================================================================================
  // THE NEGATIVE CONTROL
  // =====================================================================================

  await run.check("CROSS-ENTITY: a worker row stamped with another entity never renders", async () => {
    // The renderer's fence is `accepts()` — entityId + threadId equality, with
    // bindingRevision as a STALENESS fence and never an equality key. A worker row is the
    // richest thing on this wire (a name, a role and a worker's own authored words), so it
    // is the row a leak would be most visible on.
    const r = await page.evaluate(() => {
      const model = window.RichTimeline.createModel();
      window.RichTimeline.bind(model, "northwind", "thr_fem", 3);

      const foreign = {
        kind: "worker_activity",
        id: "m_leak",
        entityId: "lumen", // <-- another entity
        threadId: "thr_fem",
        turnId: "turn_ok",
        bindingRevision: 3,
        createdAt: 1,
        sequence: 1,
        slot: "stream",
        visibility: "ceo",
        worker: {
          agentId: "agt_shared", // the SAME id — agent_id is not globally unique
          workerName: "deeply-analyst",
          agentType: "lumen",
          observedState: "updated",
          state: "running",
          latestUpdate: "deeply's Q4 term sheet numbers",
          eventsObserved: 3,
        },
      };
      const mine = Object.assign({}, foreign, {
        id: "m_ok",
        entityId: "northwind",
        worker: Object.assign({}, foreign.worker, { workerName: "Sage", agentType: "architecture", latestUpdate: null }),
      });

      // POSITIVE PROBE: the identical event, correctly scoped, IS accepted — so a rejection
      // below cannot pass merely because the whole path is broken.
      const okAccepted = window.RichTimeline.onActivityUpserted(model, mine).rejected === false;
      const leakRejected = window.RichTimeline.onActivityUpserted(model, foreign).rejected === true;

      // And with the guard REMOVED — the exact clause, re-implemented inline — it leaks.
      const withoutGuard = (m, p) => p.threadId === m.threadId; // entity clause deleted
      const wouldLeak = withoutGuard(model, foreign);

      window.__model = model;
      model.expanded.add("turn_ok");
      const container = document.getElementById("messages");
      window.RichTimeline.render(model, container, {
        now: 1787950000000,
        expandedMessages: new Set(),
        avatarAlreadyShown: true,
        isExpanded: () => true,
        toggle: () => {},
        rerender: () => {},
        copy: () => {},
        retry: () => {},
        openWorker: () => {},
      });
      return { okAccepted, leakRejected, wouldLeak, body: document.body.innerText };
    });

    assert(r.okAccepted, "POSITIVE PROBE FAILED — the correctly-scoped worker row was refused too");
    assert(r.leakRejected, "a worker row from another entity was ACCEPTED into this thread");
    assert(r.wouldLeak, "the control is vacuous: without the entity clause the row is admitted");
    assert(!r.body.includes("deeply-analyst"), "leaked worker name into the render: " + r.body);
    assert(!r.body.includes("Q4 term sheet"), "leaked another entity's authored update: " + r.body);
    assert(r.body.includes("Sage"), "and the legitimate worker still rendered");
    return "guard on: rejected. guard off (entity clause deleted): admitted. Neither name nor authored update reached the DOM.";
  });

  await run.check("bindingRevision is a staleness fence, not an equality key", async () => {
    // The trap slice 5 named: comparing the revision for EQUALITY rejects every event after
    // a thread switch, because a live event carries the revision of the ACTIVATION that
    // produced it, which legitimately runs ahead of a re-projection's.
    const r = await page.evaluate(() => {
      const model = window.RichTimeline.createModel();
      window.RichTimeline.bind(model, "northwind", "thr_fem", 3);
      const at = (rev) => ({ entityId: "northwind", threadId: "thr_fem", bindingRevision: rev });
      return {
        equal: window.RichTimeline.accepts(model, at(3)),
        higher: window.RichTimeline.accepts(model, at(9)),
        older: window.RichTimeline.accepts(model, at(2)),
      };
    });
    assertEqual(r, { equal: true, higher: true, older: false });
    return "rev 3 accepted, rev 9 accepted (a later activation), rev 2 refused (stale)";
  });

  // =====================================================================================
  // Evidence
  // =====================================================================================

  await run.check("SCREENSHOT: three workers, three states, real WebKit pixels", async () => {
    await page.evaluate((s) => {
      window.__render(s, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
    }, F.threeWorkers());
    const s = await shot(page, "three-workers-expanded");
    assert(s.bytes > 3000, "a suspiciously small PNG is not evidence: " + s.bytes + " bytes");
    return `${s.file} (${s.bytes} bytes)`;
  });

  await run.check("SCREENSHOT: every run ended, and nothing claims how", async () => {
    await page.evaluate((s) => {
      window.__render(s, {});
      window.__model.expanded.add("turn_ok");
      window.__renderOnly();
    }, F.allEnded());
    const s = await shot(page, "all-ended");
    assert(s.bytes > 3000, "too small to be a render: " + s.bytes);
    return `${s.file} (${s.bytes} bytes)`;
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
