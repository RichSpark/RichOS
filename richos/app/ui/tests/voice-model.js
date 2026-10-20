"use strict";
// GETTING THE SPEECH MODEL — the renderer's half, under the engine Tauri actually ships.
//
// `.github/README.md` said: "Voice does not work yet. Typing does. Speech needs a model this
// build does not download for you." The backend downloads it now
// (`crates/richos-voice/src/provision.rs` + `src-tauri/src/voice_provision.rs`, both covered by
// their own Rust tests). This suite is about the only part those cannot see: what a person is
// shown, what he can press, and the one thing that must NOT happen.
//
// THE INVARIANT THIS SUITE EXISTS FOR, above every control it checks: **the offer path opens no
// microphone.** On published v1.0.0 the talk button asked for the microphone, said "listening…",
// lit macOS's orange recording indicator and never transcribed — for 25+ seconds (ray-opus-a1,
// 2026-09-04). The fix was to stop offering the button. This work puts the button back on a
// machine where it now leads somewhere, which reopens exactly that hole, so check 2 asserts that
// `start_voice_capture` is never invoked on the way to the offer. A feature that made the app able
// to hear by re-shipping a hot mic would be a bad trade.
//
// Every refusal check below carries a positive control, for the reason the Rust suite states: a
// "nothing happened" assertion passes identically when the refusal worked and when the whole
// surface stopped responding.

const path = require("path");
const { loadPlaywright, createRun, assert, assertEqual, UI_DIR, leaveHome } = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");

/// Open the shell with a mock preset, recording every command the surface issues.
async function open(browser, preset) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  // EVERY INVOKE, IN ORDER. Recorded by wrapping the bridge before the page's own scripts see
  // it — the only way to assert that a command was NOT issued, which is what the hot-mic
  // invariant needs. `mock.js`'s own `voiceModelCalls()` sees the two provisioning commands and
  // nothing else, so it cannot answer "was the microphone asked for?".
  await page.addInitScript(() => {
    let real = null;
    window.__cmds = [];
    Object.defineProperty(window, "RichBridge", {
      configurable: true,
      get() { return real; },
      set(v) {
        real = v;
        const origInvoke = v.invoke.bind(v);
        v.invoke = function (cmd, args) {
          window.__cmds.push(cmd);
          return origInvoke(cmd, args);
        };
      },
    });
  });
  if (preset) await page.addInitScript((v) => { window.__RICHOS_MOCK_PRESET__ = v; }, preset);
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForFunction("typeof window.RichTimeline === 'object'");
  page.__errors = errors;
  return page;
}

const cmds = (page) => page.evaluate(() => window.__cmds.slice());
const shown = (page, id) => page.evaluate((i) => {
  const e = document.getElementById(i);
  return !!e && !e.hidden;
}, id);

/// Which of the four model rows is on screen. Asserting on this rather than on one `hidden` at a
/// time is how "downloading" and "that download failed" are stopped from rendering together.
const rows = (page) => page.evaluate(() =>
  ["offer", "progress", "failed", "installed"].filter((k) => {
    const e = document.getElementById("voice-state-model-" + k);
    return e && !e.hidden;
  })
);

const emit = (page, payload) => page.evaluate(
  (p) => window.__RICHOS_MOCK__.voiceModelEmit(p), payload
);

/// The shape the shell really emits, with the real `small.en` figures from
/// `engine/voice/models/model-pins.json`. A fixture with invented sizes rehearses a different
/// product.
function event(phase, extra) {
  return Object.assign({
    phase,
    modelId: "small.en",
    received: 0,
    total: 487614201,
    totalLabel: "487.6 MB",
    message: null,
    askAgain: false,
    at: Date.now(),
  }, extra || {});
}

async function main() {
  const run = createRun("getting the speech model, in the shipping renderer");
  const browser = await loadPlaywright().webkit.launch();

  // ---- the three readiness states --------------------------------------------------------
  await run.check("NEGATIVE CONTROL: a machine that can already hear shows no model row at all", async () => {
    const page = await open(browser, {});
    assert(!(await shown(page, "talk-toggle")) === false, "a ready machine offers the talk button");
    assertEqual(await rows(page), [], "nothing about a download belongs on a machine that has one");
    await page.click("#talk-toggle");
    // The preview has no `start_voice_capture`, so this is the mock's refusal — which is the
    // point: the READY path goes to the microphone, and it is the mic that is unavailable here.
    assert((await cmds(page)).includes("start_voice_capture"), "the ready path asks for the microphone");
    assertEqual(await rows(page), [], "and still renders no model row");
    await page.close();
    return "ready: talk offered, mic asked for, no model row";
  });

  await run.check("a machine with a decoder and NO weights is offered the download, and NO microphone is opened", async () => {
    const page = await open(browser, { voice: "model-missing" });
    assert(await shown(page, "talk-toggle"), "the button must be offered: it now leads somewhere real");
    await page.click("#talk-toggle");
    await page.waitForSelector("#voice-state-model-offer:not([hidden])");

    // THE INVARIANT. Not one command that could open a device.
    const issued = await cmds(page);
    assertEqual(
      issued.filter((c) => c === "start_voice_capture"),
      [],
      "the offer path must never ask for the microphone — that is the 2026-09-04 defect"
    );
    assert(issued.includes("voice_readiness"), "and it must have asked what this machine can do");

    const text = await page.textContent("#voice-model-offer-label");
    assert(text.includes("487.6 MB"), "the offer must say what it costs: " + text);
    assert(text.includes("one-time"), "and that it happens once: " + text);
    assertEqual(await rows(page), ["offer"], "exactly one row");
    await page.close();
    return "offer shown with its size; start_voice_capture never invoked";
  });

  await run.check("a machine with NO decoder is offered nothing — there is nothing RichOS could fetch", async () => {
    const page = await open(browser, { voice: "unavailable" });
    assertEqual(await shown(page, "talk-toggle"), false, "a control that cannot work is not offered");
    assertEqual(await rows(page), [], "and no offer is rendered for a gap a download cannot close");
    await page.close();
    return "toolchain-missing: no button, no offer";
  });

  // ---- the download ------------------------------------------------------------------------
  await run.check("pressing Download starts one, and the bar tracks the bytes", async () => {
    const page = await open(browser, { voice: "model-missing" });
    await page.click("#talk-toggle");
    await page.waitForSelector("#voice-state-model-offer:not([hidden])");
    await page.click("#voice-model-get");
    await page.waitForSelector("#voice-state-model-progress:not([hidden])");

    assert(
      (await page.evaluate(() => window.__RICHOS_MOCK__.voiceModelCalls())).includes("provision_speech_model"),
      "the button must actually start a download"
    );

    await emit(page, event("progress", { received: 121903550 })); // 25.0%
    await page.waitForFunction(() =>
      document.getElementById("voice-model-progress-label").textContent.includes("25%")
    );
    const label = await page.textContent("#voice-model-progress-label");
    assert(label.includes("487.6 MB"), "the line names the whole size, not just the fraction: " + label);
    const width = await page.evaluate(() => document.getElementById("voice-model-bar").style.width);
    assertEqual(width, "25%", "the bar is driven by the measured bytes, not by a timer");
    assertEqual(await rows(page), ["progress"], "exactly one row");
    await page.close();
    return "download started; 121,903,550 / 487,614,201 renders as 25% and a 25% bar";
  });

  await run.check("verification is its own sentence, and Stop goes quiet because there is nothing left to stop", async () => {
    const page = await open(browser, { voice: "model-missing" });
    await page.click("#talk-toggle");
    await page.click("#voice-model-get");
    await page.waitForSelector("#voice-state-model-progress:not([hidden])");

    await emit(page, event("progress", { received: 243807100 }));
    await page.waitForFunction(() =>
      document.getElementById("voice-model-progress-label").textContent.includes("Downloading")
    );
    assertEqual(await page.isDisabled("#voice-model-stop"), false, "mid-transfer, stopping is possible");

    await emit(page, event("verifying", { received: 487614201 }));
    await page.waitForFunction(() =>
      document.getElementById("voice-model-progress-label").textContent.includes("Checking")
    );
    const label = await page.textContent("#voice-model-progress-label");
    assert(!label.includes("Downloading"), "the download's words must not survive into the check: " + label);
    assertEqual(await page.evaluate(() => document.getElementById("voice-model-bar").style.width), "100%");
    assertEqual(await page.isDisabled("#voice-model-stop"), true, "there is no transfer left to stop");
    await page.close();
    return "verifying has its own line and its own Stop state";
  });

  await run.check("Stop asks the backend to stop and claims nothing until it answers", async () => {
    const page = await open(browser, { voice: "model-missing" });
    await page.click("#talk-toggle");
    await page.click("#voice-model-get");
    await page.waitForSelector("#voice-state-model-progress:not([hidden])");
    await emit(page, event("progress", { received: 48761420 }));

    await page.click("#voice-model-stop");
    await page.waitForFunction(() =>
      window.__RICHOS_MOCK__.voiceModelCalls().includes("cancel_speech_model_download")
    );
    // STILL THE PROGRESS ROW. A panel that flipped to "stopped" on the press would be claiming
    // an outcome the backend has not reported — and the transfer may well still be finishing.
    assertEqual(await rows(page), ["progress"], "the row waits for the backend's own word");
    assertEqual(await page.isDisabled("#voice-model-stop"), true, "and a second press cannot queue a second stop");
    await page.close();
    return "cancel issued; the row holds until rich://voice-model says otherwise";
  });

  // ---- failure, and the control that belongs with each sentence ---------------------------
  await run.check("a failure the CEO can do something about renders WITH the control that does it", async () => {
    const page = await open(browser, { voice: "model-missing" });
    await page.click("#talk-toggle");
    await page.click("#voice-model-get");
    await page.waitForSelector("#voice-state-model-progress:not([hidden])");

    const portal =
      "The network sent me a sign-in page instead of my speech model. That's what hotel, " +
      "airport and conference wifi does. Sign in to the network, then ask me again. Nothing was installed.";
    await emit(page, event("failed", { received: 3104, message: portal, askAgain: true }));
    await page.waitForSelector("#voice-state-model-failed:not([hidden])");

    assertEqual(await page.textContent("#voice-model-failed-label"), portal);
    assertEqual(
      await shown(page, "voice-model-retry"),
      true,
      "the sentence says 'then ask me again' — the button that does that must be here"
    );
    assertEqual(await rows(page), ["failed"], "exactly one row");

    // And pressing it really does start another attempt.
    await page.click("#voice-model-retry");
    await page.waitForSelector("#voice-state-model-progress:not([hidden])");
    const calls = await page.evaluate(() => window.__RICHOS_MOCK__.voiceModelCalls());
    assertEqual(
      calls.filter((c) => c === "provision_speech_model").length,
      2,
      "the retry is a second real attempt, not a re-render"
    );
    await page.close();
    return "captive portal: named, with a working retry beside it";
  });

  await run.check("and a failure asking again cannot fix offers no control to press", async () => {
    const page = await open(browser, { voice: "model-missing" });
    await page.click("#talk-toggle");
    await page.click("#voice-model-get");
    await page.waitForSelector("#voice-state-model-progress:not([hidden])");

    // `askAgain: false` is what the backend sends for a gap no amount of asking changes. The UI
    // does not decide this from the wording — see `Finding::worth_asking_again`.
    await emit(page, event("failed", {
      message: "I don't have a way to check that this speech model is genuine, so I won't install it — " +
        "whoever set RichOS up can put that right. I can still read what you type.",
      askAgain: false,
    }));
    await page.waitForSelector("#voice-state-model-failed:not([hidden])");
    assertEqual(await shown(page, "voice-model-retry"), false, "no button for a refusal that cannot change");
    assert(
      (await page.textContent("#voice-model-failed-label")).includes("whoever set RichOS up"),
      "a state he cannot fix must say who can"
    );
    await page.close();
    return "unfixable-by-him: the party is named and no dead control is offered";
  });

  // ---- the finish --------------------------------------------------------------------------
  await run.check("when it installs, the microphone still waits for one deliberate press", async () => {
    const page = await open(browser, { voice: "model-missing" });
    await page.click("#talk-toggle");
    await page.click("#voice-model-get");
    await page.waitForSelector("#voice-state-model-progress:not([hidden])");

    await emit(page, event("installed", { received: 487614201 }));
    await page.waitForSelector("#voice-state-model-installed:not([hidden])");

    // NOT OPENED FOR HIM. He pressed the talk button minutes and half a gigabyte ago; acting on
    // that now would be acting on consent that has gone stale.
    assertEqual(
      (await cmds(page)).filter((c) => c === "start_voice_capture"),
      [],
      "installing a model must not open a microphone by itself"
    );
    assertEqual(await shown(page, "voice-model-listen"), true, "and the press that would is on screen");
    assertEqual(await rows(page), ["installed"], "exactly one row");

    // THE POSITIVE CONTROL for the same invariant: the press DOES open it.
    await page.click("#voice-model-listen");
    await page.waitForFunction(() => window.__cmds.includes("start_voice_capture"));
    await page.close();
    return "installed: no mic until Start listening is pressed, and then exactly one";
  });

  await run.check("a download the window was not watching is not lost when he opens the panel", async () => {
    const page = await open(browser, { voice: "model-missing" });
    // No panel open: he pressed nothing, and an event arrives because a download is running
    // from an earlier press (or the window reloaded under it).
    await emit(page, event("progress", { received: 292568520 }));
    assertEqual(await rows(page), [], "nothing is forced on screen while the panel is closed");
    await page.click("#talk-toggle");
    await page.waitForSelector("#voice-state-model-progress:not([hidden])");
    const label = await page.textContent("#voice-model-progress-label");
    assert(label.includes("60%"), "opening the panel shows where the download actually is: " + label);
    await page.close();
    return "state survives a closed panel; 292,568,520 / 487,614,201 renders as 60%";
  });

  // =========================================================================================
  // D3 — A FAILED INTEGRITY CHECK MUST NOT REMOVE THE CONTROL WITHOUT A WORD
  //
  // `docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md` §D3. With a
  // corrupted model in place — 17 bytes changed at an identical file size, so a size check
  // would have waved it through — the backend refused, correctly, with the best-written line
  // in the product. On screen the talk control was simply GONE: the audit's own accessibility
  // sweep for `checkbox|speech|listening|download|hearing` returned nothing, against a
  // positive control of 111 readable nodes in the same tree.
  // =========================================================================================

  await run.check("a refused model keeps the control on screen instead of deleting it", async () => {
    const page = await open(browser, { voice: "refused" });
    assert(await shown(page, "talk-toggle"), "the control vanished again — this IS D3");
    const state = await page.evaluate(() => {
      const b = document.getElementById("talk-toggle");
      return {
        disabled: b.disabled,
        ariaDisabled: b.getAttribute("aria-disabled"),
        name: b.getAttribute("aria-label"),
      };
    });
    assertEqual(state.disabled, true, "present but inert, not present and lying");
    assertEqual(state.ariaDisabled, "true", "and announced as unavailable rather than skipped");
    assertEqual(state.name, "Talk to Rich", "it keeps its name while it is disabled");
    assertEqual(await rows(page), [], "no model row: there is nothing to download here");
    await page.close();
    return "talk control: present, disabled, named, aria-disabled";
  });

  await run.check("the reason is on screen, in the backend's own words", async () => {
    const page = await open(browser, { voice: "refused" });
    const note = await page.evaluate(() => {
      const e = document.getElementById("voice-unavailable");
      return { hidden: e.hidden, text: e.textContent, role: e.getAttribute("role") };
    });
    assertEqual(note.hidden, false, "the explanation must be visible, not merely present");
    assertEqual(note.role, "status", "it appears without him doing anything, so it is announced");
    // The sentence is `SttError::ToolchainRefused::ceo_message()`, relayed. It names the
    // party who can fix it and says what still works — both are the point of showing it.
    assert(
      note.text.includes("Something about my hearing changed on this machine"),
      "not the backend's sentence: " + note.text
    );
    assert(note.text.includes("Whoever set RichOS up"), "the fix is not offered: " + note.text);
    assert(note.text.includes("read what you type"), "what still works is not said: " + note.text);
    await page.close();
    return "the refusal sentence, verbatim, in a status region";
  });

  await run.check("the refused state meets WCAG AA in BOTH themes, computed", async () => {
    // THE CONTRAST FLOOR IS CLEARED BEFORE THIS IS HANDED OVER, not raised afterwards. Two
    // things on this screen are new and neither had been measured until here: the
    // explanation line (`.composer-voice-note`, `--ink` at 1rem — the 4.5:1 tier) and the
    // DISABLED talk control, whose dot and border are a non-text indicator at the 3:1 tier.
    //
    // The disabled state is styled as a SHAPE change (dashed border) rather than the usual
    // fade for exactly this reason: dimming is what takes an indicator under its floor, and
    // the rule does not exempt a control because it is inactive.
    const C = require("./lib/contrast");
    const out = [];
    for (const theme of ["dark", "light"]) {
      const page = await open(browser, { voice: "refused" });
      await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
      await page.waitForSelector("#voice-unavailable:not([hidden])");
      await page.evaluate(C.pageScript());
      const r = await page.evaluate((o) => window.__contrastProbe(o), {
        surface: "voice-refused",
        theme,
      });
      const measured = r.measuredPaths || [];
      const failures = Object.values(r.failures);
      const unresolvable = Object.values(r.unresolvable);
      // The note must be among what was measured, or "the page is clean" would be a claim
      // about a screen the new line was not on.
      assert(
        measured.some((m) => m.indexOf("#voice-unavailable") >= 0),
        `${theme}: the explanation line was not measured at all — ${measured.length} node(s) seen`
      );
      assertEqual(
        unresolvable.length,
        0,
        `${theme}: a color the probe could not resolve is a failure to prove: ${JSON.stringify(unresolvable)}`
      );
      assertEqual(failures.length, 0, `${theme}: WCAG AA failures ${JSON.stringify(failures)}`);
      out.push(`${theme}: ${measured.length} node(s), 0 failures`);
      await page.close();
    }
    return out.join(" · ");
  });

  await run.check("NEGATIVE CONTROL: no other state grows that line or a dead control", async () => {
    for (const preset of [{}, { voice: "model-missing" }, { voice: "unavailable" }]) {
      const page = await open(browser, preset);
      const seen = await page.evaluate(() => {
        const n = document.getElementById("voice-unavailable");
        const b = document.getElementById("talk-toggle");
        return { note: !n.hidden, disabled: b.disabled, hidden: b.hidden };
      });
      const label = JSON.stringify(preset);
      assertEqual(seen.note, false, `${label} must not show the refusal line`);
      assertEqual(seen.disabled, false, `${label} must not render an inert control`);
      // `unavailable` is still NOT-OFFERED, unchanged: a machine with no decoder at all
      // cannot be helped by anyone here, and a disabled button beside that fact is clutter.
      assertEqual(seen.hidden, preset.voice === "unavailable", `${label}: offered-ness changed`);
      await page.close();
    }
    return "ready / model-missing / unavailable: no note, no inert control, offering unchanged";
  });

  // =========================================================================================
  // D8 — THE CONTROL HAS AN ACCESSIBLE NAME, AND IT TRACKS THE STATE
  //
  // Audit §D8: VoiceOver announced the primary affordance of a voice-first product as a bare
  // "checkbox". `title` is a tooltip and is not an accessible name.
  // =========================================================================================

  await run.check("the talk control is NAMED, and the name follows what pressing it does", async () => {
    // `model-missing` rather than the ready preset, for cause: `enterVoiceMode`'s ready path
    // calls `start_voice_capture`, which the preview refuses, so `voiceMode` never turns on
    // and the pressed state never changes. The OFFER path enters voice mode while opening no
    // device at all — the hot-mic invariant the suite above pins — which is the one route
    // that reaches the pressed state in a browser.
    const page = await open(browser, { voice: "model-missing" });
    const read = () => page.evaluate(() => {
      const b = document.getElementById("talk-toggle");
      return { name: b.getAttribute("aria-label"), pressed: b.getAttribute("aria-pressed") };
    });

    const off = await read();
    assertEqual(off.name, "Talk to Rich", "no accessible name at rest — this IS D8");
    assertEqual(off.pressed, "false", "and the state is still reported beside it");

    await page.click("#talk-toggle");
    await page.waitForFunction(
      () => document.getElementById("talk-toggle").getAttribute("aria-pressed") === "true"
    );
    const on = await read();
    assertEqual(on.name, "Stop talking", "the name must say what pressing it will DO now");
    assertEqual(on.pressed, "true", "the state is carried by aria-pressed, never said twice");

    await page.close();
    return "off: Talk to Rich/false — on: Stop talking/true";
  });

  await run.check("a nameless talk control fails: its ONLY naming source is aria-label", async () => {
    // WHY THIS IS NOT JUST "aria-label IS SET". The audit read the accessibility TREE and got
    // a bare `checkbox 1`. Playwright 1.61 exposes no `page.accessibility` API to read that
    // tree back, so the check is made structurally instead, and it is the stronger claim of
    // the two: the control has NO text content, NO element naming it and NO wrapping
    // label, so `aria-label` is the only thing between it and the bare role the audit heard.
    // Remove the attribute and this check fails — which is exactly the regression it exists
    // to catch.
    const page = await open(browser, {});
    const shape = await page.evaluate(() => {
      const b = document.getElementById("talk-toggle");
      return {
        label: b.getAttribute("aria-label"),
        text: (b.textContent || "").trim(),
        labelledBy: b.getAttribute("aria-labelledby"),
        inLabel: !!b.closest("label"),
        role: b.getAttribute("role") || b.tagName.toLowerCase(),
      };
    });
    assertEqual(shape.text, "", "the control is a glyph — it has no text to be named by");
    assertEqual(shape.labelledBy, null, "and nothing else names it");
    assertEqual(shape.inLabel, false, "and it is not wrapped in a label");
    assert(
      typeof shape.label === "string" && shape.label.trim().length > 0,
      "so with no aria-label a screen reader gets the bare role — this IS D8: " +
        JSON.stringify(shape)
    );
    assertEqual(shape.label, "Talk to Rich", "the name must be the one we set");
    await page.close();
    return `<${shape.role}> text="" -> name from aria-label only: ${JSON.stringify(shape.label)}`;
  });

  await browser.close();
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((e) => { console.error(e); process.exit(1); });
