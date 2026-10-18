// Adversarial lifecycle regressions from Andreas's first-client audit. These drive the
// shipping renderer through its bridge listeners, including command failure without a
// terminal event. Native IPC and core cancellation are tested separately.
"use strict";
const { loadPlaywright, createRun, assert, assertEqual } = require("./lib/harness");
const { openApp, startTurn, goWorking, advance, tick, band } = require("./waiting-state");

async function activity(page, fence, turnId, values) {
  await page.evaluate(({ fence, turnId, values }) => window.__emit("rich://activity-upserted", {
    ...fence, kind: "activity", id: "audit_activity", turnId, createdAt: Date.now(),
    updatedAt: Date.now(), sequence: 1, slot: "stream", visibility: "ceo",
    activityType: "command", state: "running", at: Date.now(), ...values,
  }), { fence, turnId, values });
}

async function controllableSend(page) {
  await page.evaluate(() => {
    const original = window.RichBridge.invoke.bind(window.RichBridge);
    window.RichBridge.invoke = (cmd, args) => cmd === "send_message"
      ? new Promise((resolve, reject) => { window.__rejectSend = reject; })
      : original(cmd, args);
  });
}

async function main() {
  const run = createRun("waiting lifecycle: acceptance, failure, evidence and accessibility");
  const browser = await loadPlaywright().webkit.launch();
  try {
    await run.check("returning to the working thread names this conversation while activation waits", async () => {
      for (const theme of ["dark", "light"]) {
        const page = await openApp(browser, theme);
        try {
          const fence = await startTurn(page, "returning_work");
          await goWorking(page, fence, "returning_work");
          await page.evaluate(() => {
            const original = window.RichBridge.invoke.bind(window.RichBridge);
            window.__stopTargets = [];
            window.RichBridge.invoke = (cmd, args) => {
              if (cmd === "switch_thread") return new Promise(() => {});
              if (cmd === "stop_turn") {
                window.__stopTargets.push(args.expectedThreadId);
                return window.__allowStop
                  ? Promise.resolve({ stopped: true, turnId: "returning_work", reachedLease: true })
                  : Promise.reject("temporary disk error");
              }
              return original(cmd, args);
            };
          });
          await page.click('.nav-thread[data-thread-id="acme"]');
          assert((await band(page)).detail.includes("previous conversation"));
          await page.locator(`.nav-thread[data-thread-id="${fence.threadId}"]`).click();
          assertEqual((await band(page)).detail, "This conversation is still working. Press Stop to stop that work.");
          assert(!(await page.isDisabled("#send")), "returning keeps Send available during ordered activation");
          await page.click("#stop");
          assertEqual((await band(page)).detail, "I couldn't stop this conversation. Press Stop again.");
          await page.evaluate(() => { window.__allowStop = true; });
          await page.click("#stop");
          assertEqual((await band(page)).detail, "Stopping work in this conversation");
          assertEqual(await page.evaluate(() => window.__stopTargets), [fence.threadId, fence.threadId]);
          assert(await page.isDisabled("#stop"));
          await page.evaluate(fence => window.__emit("rich://turn-status", {
            ...fence, turnId: "returning_work", status: "stopped", at: Date.now(), visibility: "ceo",
          }), fence);
          assert(await page.isHidden("#stop"));
          assert(!(await band(page)).detail.includes("still working"));
          assertEqual(page.__errors.length, 0, page.__errors.join("\n"));
        } finally { await page.close(); }
      }
    });
    await run.check("opening another company hides old messages, sends directly and keeps Stop usable", async () => {
      for (const theme of ["dark", "light"]) {
        const page = await openApp(browser, theme);
        const fence = await startTurn(page, "opening_previous");
        await goWorking(page, fence, "opening_previous");
        await controllableSend(page);
        await page.evaluate(() => {
          const original = window.RichBridge.invoke.bind(window.RichBridge);
          window.__openingCalls = [];
          window.RichBridge.invoke = (cmd, args) => {
            window.__openingCalls.push({ cmd, args });
            if (cmd === "switch_thread") return new Promise(resolve => {
              window.__finishOpeningSwitch = () => original(cmd, args).then(resolve);
            });
            if (cmd === "get_timeline") return new Promise(resolve => {
              window.__finishOpeningSnapshot = () => original(cmd, args).then(resolve);
            });
            if (cmd === "stop_turn") return Promise.reject("temporary disk error");
            return original(cmd, args);
          };
        });
        await page.click('.nav-thread[data-thread-id="acme"]');
        await page.waitForFunction(() => !!window.__finishOpeningSwitch);
        assert(!(await page.locator("#messages").innerText()).includes("Draft the Q4 board memo"),
          "previous company's messages are hidden while the destination pane paints");
        assertEqual((await band(page)).head, "Opening conversation");
        assert((await band(page)).detail.includes("previous conversation"));
        assert(!(await page.isDisabled("#send")));
        assert(await page.isVisible("#stop"));
        await page.fill("#input", "New company's draft");
        await page.press("#input", "Enter");
        const submitted = await page.evaluate(() => window.__openingCalls.filter(c => c.cmd === "send_message" || c.cmd === "steer_turn"));
        assertEqual(submitted.length, 1);
        assertEqual(submitted[0].cmd, "send_message");
        assertEqual(submitted[0].args.threadId, "acme", "Send targets the destination before its switch completes");
        assertEqual(submitted[0].args.text, "New company's draft");
        await page.click("#stop");
        assertEqual((await band(page)).detail, "I couldn't stop the previous conversation. Press Stop again.");
        assertEqual(await page.evaluate(() => window.__openingCalls.find(c => c.cmd === "stop_turn").args.expectedThreadId), fence.threadId);
        assert(!(await page.isDisabled("#stop")), "failed Stop is actionable while navigation waits");
        await page.evaluate(() => {
          const original = window.RichBridge.invoke.bind(window.RichBridge);
          window.RichBridge.invoke = (cmd, args) => cmd === "stop_turn"
            ? Promise.resolve({ stopped: true, turnId: "opening_previous", reachedLease: true }) : original(cmd, args);
        });
        await page.click("#stop");
        assertEqual((await band(page)).detail, "Stopping work in the previous conversation");
        assert(await page.isDisabled("#stop"));
        await page.evaluate(fence => window.__emit("rich://turn-status", { ...fence, turnId: "opening_previous", status: "stopped", at: Date.now(), visibility: "ceo" }), fence);
        assert(await page.isHidden("#stop"), "previous terminal event removes its Stop");
        await page.evaluate(() => window.__finishOpeningSwitch());
        await page.waitForFunction(() => !!window.__finishOpeningSnapshot);
        assert(!(await page.isDisabled("#send")), "Send remains available while saved messages load");
        await page.evaluate(() => window.__finishOpeningSnapshot());
        await page.waitForSelector('#conversation:not([hidden])');
        // The send already reached the destination while navigation was gated.
        await page.waitForFunction(
          () => Array.from(document.querySelectorAll(".tl-user-text")).some(n => n.textContent.trim() === "New company's draft"), null,
          { timeout: 8000 }
        );
        assertEqual(await page.inputValue("#input"), "", "the submitted words are still in the composer");
        assert(!(await page.isDisabled("#send")));
        assertEqual(await page.evaluate(() => window.__RICHOS_TIMELINE__().threadId), "acme");
        // The band is no longer null here and that is the point rather than a tolerance: the
        // held send is genuinely in flight on the new conversation, so "Sending your message /
        // Waiting for Rich to accept it" is the honest thing to be saying. It used to read null
        // because his sentence had been thrown away and nothing was happening at all.
        const after = await band(page);
        assertEqual(after && after.head, "Sending your message", "the replayed send is not described on screen: " + JSON.stringify(after));
        assertEqual(after.detail, "Waiting for Rich to accept it");
        await page.close();
      }
    });

    await run.check("a slow invocation is visible before acceptance and queued silence ages honestly", async () => {
      const page = await openApp(browser);
      await page.evaluate(() => { window.__TAP.hang = true; });
      await page.fill("#input", "Please help with the board memo.");
      await page.press("#input", "Enter");
      await advance(page, 40000); await tick(page);
      let b = await band(page);
      assertEqual(b.head, "Sending your message");
      assertEqual(b.detail, "Waiting for Rich to accept it");
      assertEqual(b.tone, "quiet");
      assert(await page.isHidden("#stop"), "no accepted operation exists to stop yet");
      await page.evaluate(() => {
        const m = window.__RICHOS_TIMELINE__();
        window.__emit("rich://turn-status", { entityId: m.entityId, threadId: m.threadId,
          bindingRevision: m.bindingRevision, turnId: "queued_slow", status: "queued",
          startedAt: null, visibility: "ceo", at: Date.now() });
      });
      await advance(page, 40000); await tick(page);
      b = await band(page);
      assertEqual(b.head, "Rich is thinking");
      assertEqual(b.detail, "Nothing new for 40s");
      assertEqual(b.tone, "quiet");
      assertEqual(await page.locator(".tl-duration-label").last().textContent(), "Waiting to start");
      await page.close();
    });

    await run.check("queued then working cannot consume a second message awaiting its own acceptance", async () => {
      const page = await openApp(browser);
      await page.evaluate(() => { window.__TAP.hang = true; });
      await page.fill("#input", "First request"); await page.press("#input", "Enter");
      await page.fill("#input", "Second request"); await page.press("#input", "Enter");
      await page.evaluate(() => {
        const m = window.__RICHOS_TIMELINE__();
        const fence = { entityId: m.entityId, threadId: m.threadId, bindingRevision: m.bindingRevision };
        for (const status of ["queued", "working"]) window.__emit("rich://turn-status", {
          ...fence, turnId: "first_request", status, startedAt: status === "working" ? Date.now() : null,
          at: Date.now(), visibility: "ceo",
        });
      });
      await tick(page);
      assertEqual(await page.evaluate(() => window.__RICHOS_TIMELINE__().pendingUser.length), 1);
      const text = await page.locator("#messages").innerText();
      assert(text.includes("First request") && text.includes("Second request"), "both requests remain visible");
      await page.close();
    });

    await run.check("a queued message stopped with its provider is restorable without being sent", async () => {
      const page = await openApp(browser);
      await page.evaluate(() => {
        const original = window.RichBridge.invoke.bind(window.RichBridge);
        window.__sendCount = 0;
        window.RichBridge.invoke = (cmd, args) => {
          if (cmd === "send_message") window.__sendCount++;
          return original(cmd, args);
        };
      });
      const fence = await startTurn(page, "stop_first");
      await goWorking(page, fence, "stop_first");
      const words = "Keep these exact words: café\nand the second line.";
      await page.fill("#input", words); await page.press("#input", "Enter");
      await page.evaluate(f => {
        window.__emit("rich://turn-status", { ...f, turnId: "stop_queued", status: "queued", startedAt: null,
          at: Date.now(), visibility: "ceo" });
        window.__emit("rich://message-completed", { ...f, turnId: "stop_first", messageId: "stop_first:text:0",
          text: "I will check that invoice.", phase: "unknown", at: Date.now(), visibility: "ceo" });
        for (const turnId of ["stop_first", "stop_queued"]) {
          window.__emit("rich://turn-status", { ...f, turnId, status: "stopped", at: Date.now(), visibility: "ceo" });
        }
      }, fence);
      await tick(page);
      const queued = page.locator('.tl-turn[data-turn-id="stop_queued"]');
      assertEqual(await queued.locator('.tl-duration').getAttribute("data-tone"), "ceo-stopped");
      assertEqual(await queued.locator('.tl-intervention').count(), 0, "Stop is not a failure");
      assertEqual(await page.inputValue("#input"), "");
      assertEqual(await page.evaluate(() => window.__sendCount), 2);
      await queued.getByRole("button", { name: "Restore my message", exact: true }).click();
      assertEqual(await page.inputValue("#input"), words, "restoring preserves the complete saved text");
      assertEqual(await page.evaluate(() => window.__sendCount), 2, "restoring must not execute the message");
      const partial = page.locator('.tl-turn[data-turn-id="stop_first"]');
      assert((await partial.innerText()).includes("I will check that invoice."), "partial prose stays visible");
      await page.fill("#input", "");
      await partial.getByRole("button", { name: "Restore my message", exact: true }).click();
      assertEqual(await page.inputValue("#input"), "Draft the Q4 board memo from the numbers in the folder.");
      assertEqual(await page.evaluate(() => window.__sendCount), 2, "partial output must not hide restore or trigger a send");
      assertEqual(page.__errors.length, 0, page.__errors.join("\n"));
      await page.close();
    });

    await run.check("a rejected accepted send ends its waiting state even when the terminal event is missing", async () => {
      const page = await openApp(browser);
      await controllableSend(page);
      await startTurn(page, "prime_failed");
      await page.evaluate(() => window.__rejectSend("cognition io: priming process exited"));
      await page.waitForFunction(() => window.__RICHOS_WAIT__() === null);
      await advance(page, 300000); await tick(page);
      assertEqual(await band(page), null, "five minutes cannot revive a rejected invocation");
      assert(await page.isHidden("#stop"), "no misleading Stop after failure");
      const text = await page.locator("#messages").innerText();
      assert(text.includes("Draft the Q4 board memo"), "accepted user words survive");
      assert(text.includes("Restore my message"), "the failure has an actionable control");
      assert(!text.includes("cognition io"), "raw machinery is not a user explanation");
      await page.close();
    });

    await run.check("a terminal failure followed by command rejection is explained once", async () => {
      const page = await openApp(browser);
      await controllableSend(page);
      const fence = await startTurn(page, "failed_once");
      await goWorking(page, fence, "failed_once");
      await page.evaluate(f => {
        window.__emit("rich://turn-status", { ...f, turnId: "failed_once", status: "failed",
          startedAt: Date.now(), activeDurationMs: 0, at: Date.now(), visibility: "ceo" });
        window.__rejectSend("cognition io: broken pipe");
      }, fence);
      await tick(page);
      assertEqual(await band(page), null);
      assertEqual(await page.inputValue("#input"), "", "accepted text is not duplicated into a resend draft");
      assert(!(await page.locator("#messages").innerText()).includes("broken pipe"));
      assertEqual(await page.getByRole("button", { name: "Restore my message", exact: true }).count(), 1);
      await page.close();
    });

    await run.check("a refusal preserves text typed while the invocation was pending", async () => {
      const page = await openApp(browser);
      await controllableSend(page);
      await page.fill("#input", "The original request"); await page.press("#input", "Enter");
      await page.fill("#input", "A newer unsent draft");
      await page.evaluate(() => window.__rejectSend("I couldn't accept that request."));
      await page.waitForFunction(() => document.querySelector("#input").value.includes("The original request"));
      assertEqual(await page.inputValue("#input"), "The original request\n\nA newer unsent draft");
      await page.close();
    });

    await run.check("a send refused after navigation restores its draft only to the original thread", async () => {
      const page = await openApp(browser);
      await controllableSend(page);
      const origin = await page.evaluate(() => window.__RICHOS_TIMELINE__().threadId);
      await page.fill("#input", "Original company request"); await page.press("#input", "Enter");
      await page.click('.nav-thread[data-thread-id="acme"]');
      await page.waitForFunction(() => window.__RICHOS_TIMELINE__().threadId === "acme");
      await page.fill("#input", "New company draft");
      await page.evaluate(() => window.__rejectSend("The selected task changed."));
      await page.waitForTimeout(100);
      assertEqual(await page.inputValue("#input"), "New company draft");
      assert(!(await page.locator("#messages").innerText()).includes("Original company request"));
      await page.click('.nav-thread[data-thread-id="' + origin + '"]');
      await page.waitForFunction(id => window.__RICHOS_TIMELINE__().threadId === id, origin);
      await page.waitForFunction(() => document.querySelector("#input").value === "Original company request");
      assertEqual(await page.inputValue("#input"), "Original company request");
      await page.close();
    });

    await run.check("a worker heartbeat cannot change the age of an earlier activity", async () => {
      const page = await openApp(browser);
      const fence = await startTurn(page, "activity_age");
      await goWorking(page, fence, "activity_age");
      await activity(page, fence, "activity_age", { summary: "Read the Q3 board pack", state: "completed" });
      await advance(page, 23000);
      await page.evaluate(f => window.__emit("rich://worker-upserted", { ...f,
        kind: "worker_activity", id: "sage", turnId: "activity_age", createdAt: Date.now(),
        sequence: 2, slot: "stream", visibility: "ceo", at: Date.now(), worker: {
          agentId: "sage", workerName: "Sage", agentType: "research", observedState: "updated",
          state: "running", latestUpdate: null, eventsObserved: 3,
        } }), fence);
      await advance(page, 3000); await tick(page);
      assertEqual((await band(page)).detail, "Read the Q3 board pack · 26s ago");
      assertEqual((await band(page)).tone, "working", "recent liveness still counts separately");
      await page.close();
    });

    await run.check("compaction start and ending are announced once, with no heartbeat or tick chatter", async () => {
      const page = await openApp(browser);
      const fence = await startTurn(page, "accessible_compaction");
      await goWorking(page, fence, "accessible_compaction");
      await page.waitForTimeout(100);
      await page.evaluate(() => {
        window.__said = [];
        new MutationObserver(() => {
          const text = document.querySelector("#live-region").textContent;
          if (text) window.__said.push(text);
        }).observe(document.querySelector("#live-region"), { childList: true, subtree: true, characterData: true });
      });
      const startedAt = await page.evaluate(() => Date.now());
      for (const offset of [0, 30000, 30000]) {
        await advance(page, offset);
        await activity(page, fence, "accessible_compaction", { summary: "Making room to keep going", startedAt, measuredMinMs: 38138 });
        await tick(page);
      }
      await activity(page, fence, "accessible_compaction", { summary: "Made room to keep going", state: "completed", startedAt, measuredMinMs: 38138 });
      await tick(page);
      assertEqual(await page.evaluate(() => window.__said), ["Making room to keep going", "Made room to keep going"]);
      await page.close();
    });

    await run.check("a mid-turn snapshot restores the activity age and paced bar without restarting either", async () => {
      const page = await openApp(browser);
      const fence = await startTurn(page, "snapshot_compaction");
      await goWorking(page, fence, "snapshot_compaction");
      const startedAt = await page.evaluate(() => Date.now());
      await activity(page, fence, "snapshot_compaction", { summary: "Making room to keep going", startedAt, measuredMinMs: 38138 });
      await advance(page, 20000); await tick(page);
      const before = await page.evaluate(() => window.__RICHOS_WAIT__().copy.pace.position);
      await page.evaluate(f => {
        const m = window.__RICHOS_TIMELINE__();
        const original = window.RichBridge.invoke.bind(window.RichBridge);
        const snapshot = { ...f, mode: "ceo", items: [...m.items.values(), {
          ...f, kind: "work_duration", id: "snapshot_compaction:duration", turnId: "snapshot_compaction",
          state: "working", startedAt: m.turns.get("snapshot_compaction").startedAt, activeMs: null,
          visibility: "ceo", slot: "terminal", createdAt: Date.now(),
        }] };
        window.RichBridge.invoke = (cmd, args) => cmd === "get_timeline" ? Promise.resolve(snapshot) : original(cmd, args);
        window.__emit("rich://proactive-message", { threadId: m.threadId });
      }, fence);
      await tick(page);
      assertEqual((await band(page)).detail, "Making room to keep going · 20s ago");
      assertEqual(await page.evaluate(() => window.__RICHOS_WAIT__().copy.pace.position), before);
      await page.close();
    });
  } finally { await browser.close(); }
  process.exitCode = run.report() ? 1 : 0;
}
main().catch(error => { console.error(error); process.exitCode = 1; });
