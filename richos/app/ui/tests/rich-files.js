// A FILE FROM RICH — round 36 screen 3 (§138, build plan item 12), driven through the REAL
// SHELL: `index.html`, `main.js`, `timeline.js`, `rich-files.js`, `mock.js` and `style.css`,
// loaded from disk, with nothing stubbed but the Tauri bridge (which `mock.js` replaces).
//
// The CEO, 2026-10-21: "yes, the regular app needs to have EVERYTHING I USE AND HAVE." Rich (or
// Stu) puts a file in front of the user: it shows as a card under Rich's words; a click opens it
// with the Mac's default app through `rich_file_open` (which `opener.rs` resolves); "Show in
// Finder" sits beside it; a file that has gone shows so and offers "Ask Rich again".
//
// THE FIXTURE is `mock.js`'s `{ richFiles: "round-36" }` preset: the round's own turn ("send me
// the final counter as a PDF, and the comps") with its two files, `Acme-counteroffer.pdf`
// (Preview) and `Acme-comparables.xlsx` (Numbers). `round-36-missing` is the same turn with the
// PDF gone from the Mac.
//
// TWO COMMITTED PICTURE PAIRS, `shots-36/files-{ready,missing}-{dark,light}.png`, published through
// `publishShotFile`: written only under `RICHOS_SHOTS_REGENERATE`, otherwise compared.
//
// Run: node rich-files.js

"use strict";

const path = require("path");
const {
  loadPlaywright,
  leaveHome,
  createRun,
  assert,
  assertEqual,
  shot,
  publishShotFile,
  openThread,
  FRAME_STILL,
  UI_DIR,
} = require("./lib/harness");
const { parseCssColor, compositeOver, contrastRatio, hex } = require("./lib/contrast");

const APP = "file://" + path.join(UI_DIR, "index.html");
const SHOTS = path.join(__dirname, "shots-36");
const FIXED = new Date("2026-10-21T09:05:00-04:00");

async function openApp(browser, opts) {
  opts = opts || {};
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    timezoneId: "America/New_York",
    locale: "en-US",
  });
  const page = await context.newPage();
  await page.clock.setFixedTime(FIXED);
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  const theme = opts.theme || "dark";
  await page.addInitScript((t) => {
    try {
      window.localStorage.setItem("richos-theme", t);
      window.localStorage.setItem("richos-font-scale", "100");
      window.localStorage.setItem("richos-mock-config", JSON.stringify({ theme: t, font_scale: 100, user_name: null }));
    } catch (e) {
      /* storage unavailable: the shipped default */
    }
  }, theme);
  await page.addInitScript(FRAME_STILL);
  await page.addInitScript(() => {
    let state = 0x6a09e667;
    Math.random = () => {
      state ^= state << 13;
      state ^= state >>> 17;
      state ^= state << 5;
      return (state >>> 0) / 4294967296;
    };
  });
  await page.addInitScript((v) => {
    window.__RICHOS_MOCK_PRESET__ = v;
  }, { richFiles: opts.missing ? "round-36-missing" : "round-36" });
  await page.goto(APP);
  if (opts.prepare) await page.evaluate(opts.prepare);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  await openThread(page, "acme");
  await page.waitForSelector(".tl-rfile");
  page.__errors = errors;
  return page;
}

function mediaMomentsFixture() {
  const localId = "mm_0000000000000001";
  const youtubeId = "mm_0000000000000002";
  const durationMs = 102007;
  // A seekable PCM WAV with a real metadata event. No codec, private recording or
  // playback-property stub is needed to observe the browser's actual seek.
  const sampleRate = 8000;
  const samples = Math.round(durationMs * sampleRate / 1000);
  const headerBytes = 44;
  const audio = new Uint8Array(headerBytes + samples);
  const encoder = new TextEncoder();
  for (const [offset, tag] of [[0, "RIFF"], [8, "WAVEfmt "], [36, "data"]]) audio.set(encoder.encode(tag), offset);
  audio.fill(128, headerBytes);
  const header = new DataView(audio.buffer);
  header.setUint32(4, audio.length - 8, true);
  header.setUint32(16, 16, true);
  header.setUint16(20, 1, true);
  header.setUint16(22, 1, true);
  header.setUint32(24, sampleRate, true);
  header.setUint32(28, sampleRate, true);
  header.setUint16(32, 1, true);
  header.setUint16(34, 8, true);
  header.setUint32(40, samples, true);
  const url = URL.createObjectURL(new Blob([audio], { type: "audio/wav" }));
  const localFile = {
    id: localId, name: "Founder interview.wav", source: "local", exists: true,
    moments: { durationMs, points: [
      { startMs: 7140, label: "Mission", quote: "Put the customer first." },
      { startMs: 53280, label: "Customer focus", quote: "Stay focused on the customer." },
    ] },
  };
  const youtubeFile = {
    id: youtubeId, name: "Product interview", source: "youtube", exists: true,
    moments: { durationMs, points: [
      { startMs: 12000, label: "First question", quote: "Here is what people asked for." },
      { startMs: 26170, label: "First useful task", quote: "Give a new customer one real task." },
    ] },
  };
  const invoke = window.RichBridge.invoke.bind(window.RichBridge);
  window.mediaMomentCalls = [];
  window.RichBridge.invoke = async (cmd, args, options) => {
    if (cmd === "media_moment_open" || (cmd === "output_preview" && args.outputId === localId)) {
      window.mediaMomentCalls.push({ cmd, args });
      return cmd === "output_preview" ? { view: "audio", url, durationMs, bytes: audio.length } : undefined;
    }
    const payload = await invoke(cmd, args, options);
    if (args?.threadId === "acme" && cmd === "get_timeline") {
      const row = payload.items.find((item) => item.kind === "rich_file");
      for (const file of [localFile, youtubeFile]) payload.items.push({ ...row, id: file.id, text: file.name, file });
    }
    return payload;
  };
}

/// The cards as the user sees them, read from the DOM.
async function cards(page) {
  return page.evaluate(() =>
    [...document.querySelectorAll(".tl-rfile")].map((c) => {
      const shown = (sel) => {
        const n = c.querySelector(sel);
        return !!n && getComputedStyle(n).display !== "none";
      };
      return {
        id: c.dataset.fileId,
        state: c.dataset.s,
        name: c.querySelector(".tl-rfile-name").textContent,
        meta: c.querySelector(".tl-rfile-meta").textContent,
        ext: c.querySelector(".tl-rfile-ext").textContent,
        open: c.querySelector(".tl-rfile-open").textContent,
        label: c.querySelector(".tl-rfile-main").getAttribute("aria-label"),
        openShown: shown(".tl-rfile-open"),
        finderShown: shown(".tl-rfile-finder"),
        askShown: shown(".tl-rfile-ask"),
      };
    })
  );
}

async function main() {
  const run = createRun("A file from Rich — round 36 screen 3 (real shell, WebKit)");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();

  const page = await openApp(browser);

  await run.check("a library card tries the bookmark even when its recorded path is stale", async () => {
    await page.evaluate(() => {
      const original = window.__libraryTestBridge = window.RichBridge;
      window.RichBridge = {
        ...original,
        invoke(command, args) {
          if (command === "cleaner_action") {
            window.__libraryOpen = args;
            if (args.action !== "open") return Promise.resolve({});
            window.__libraryOpenCount++;
            return new Promise((_, reject) => { window.__libraryReject = reject; });
          }
          return original.invoke(command, args);
        },
      };
      window.__libraryOpenCount = 0;
      window.__libraryRender = () => window.RichFiles.render([{
        id: "bookmark-test", file: { id: "saved-note", library: true, exists: false, name: "Moved.txt" },
      }]);
      document.querySelector(".tl-rfiles").after(window.__libraryRender());
    });
    const card = page.locator('.tl-rfile[data-file-id="saved-note"]');
    await card.locator(".tl-rfile-finder").click();
    assertEqual(await page.evaluate(() => window.__libraryOpen.action), "reveal");
    await card.locator(".tl-rfile-main").click();
    await card.evaluate((el) => {
      window.RichFiles.downloading("saved-note");
      el.closest(".tl-rfiles").replaceWith(window.__libraryRender());
    });
    assertEqual(await card.locator(".tl-rfile-meta").textContent(), "Downloading from iCloud…");
    await card.locator(".tl-rfile-main").click();
    assertEqual(await page.evaluate(() => window.__libraryOpenCount), 1, "a rebuilt card must not open twice");
    await page.evaluate(() => window.__libraryReject("Locate the original file"));
    await page.waitForFunction(() =>
      document.querySelector('[data-file-id="saved-note"]').dataset.s === "missing"
    );
    assertEqual(await page.evaluate(() => window.__libraryOpen), {
      id: "saved-note", action: "open", note: null,
    });
    assertEqual(await card.locator(".tl-rfile-ask").textContent(), "Locate original");
    await card.locator(".tl-rfile-ask").click();
    assertEqual(await page.evaluate(() => window.__libraryOpen.action), "locate");
    assertEqual(await page.evaluate(() => window.__libraryOpen.id), "saved-note");
    await card.evaluate((el) => el.closest(".tl-rfiles").remove());
    await page.evaluate(() => {
      window.RichBridge = window.__libraryTestBridge;
      delete window.__libraryTestBridge;
      delete window.__libraryOpen;
      delete window.__libraryOpenCount;
      delete window.__libraryRender;
      delete window.__libraryReject;
    });
  });

  await run.check("the files sit under Rich's words in his reply, one card each, built like the attachment chip", async () => {
    const got = await cards(page);
    assertEqual(got.length, 2, "cards in the conversation");
    assertEqual(got[0].name, "Acme-counteroffer.pdf");
    assertEqual(got[0].meta, "PDF · 184 KB · 2 pages", "a PDF's card says its pages, as the round writes it");
    assertEqual(got[0].ext, "PDF");
    assertEqual(got[0].open, "Open in Preview");
    // The name says what pressing does, then the card's line: the name stands in for the button's
    // children, so without the line a screen reader never said the type, size or pages.
    assertEqual(got[0].label, "Open Acme-counteroffer.pdf in Preview, PDF · 184 KB · 2 pages");
    assertEqual(got[1].label, "Open Acme-comparables.xlsx in Numbers, Spreadsheet · 46 KB");
    assertEqual(got[1].name, "Acme-comparables.xlsx");
    assertEqual(got[1].meta, "Spreadsheet · 46 KB");
    assertEqual(got[1].open, "Open in Numbers");
    for (const c of got) {
      assert(c.openShown && c.finderShown && !c.askShown, c.name + ": Open and Show in Finder, no Ask Rich again");
    }
    const place = await page.evaluate(() => {
      const list = document.querySelector(".tl-rfiles");
      const lane = list.closest(".tl-lane");
      const prose = [...lane.querySelectorAll(".tl-rich")];
      const last = prose[prose.length - 1];
      return {
        role: list.getAttribute("role"),
        label: list.getAttribute("aria-label"),
        underProse: !!last && (last.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0,
        proseText: last ? last.textContent : "",
        ask: list.closest(".tl-turn").querySelector(".tl-user-text").textContent,
      };
    });
    assertEqual(place.role, "list");
    assertEqual(place.label, "2 files from Rich");
    assert(place.underProse, "the cards are not under Rich's words");
    assert(place.proseText.includes("Here they are."), "the cards are not in the reply that sent them");
    assertEqual(place.ask, "send me the final counter as a PDF, and the comps");
    return got.map((c) => c.name + " · " + c.open).join(" | ");
  });

  await run.check("a click anywhere on the card opens it through the opener; Show in Finder shows it", async () => {
    await page.click('.tl-rfile[data-file-id="rf_0a1b2c3d4e5f6071"] .tl-rfile-name');
    await page.click('.tl-rfile[data-file-id="rf_8192a3b4c5d6e7f8"] .tl-rfile-open');
    await page.click('.tl-rfile[data-file-id="rf_0a1b2c3d4e5f6071"] .tl-rfile-finder');
    await page.waitForFunction(() => window.__RICHOS_MOCK__.richFileActions().length >= 3);
    const acts = await page.evaluate(() => window.__RICHOS_MOCK__.richFileActions());
    assertEqual(
      acts.map((a) => a.cmd + " " + a.threadId + " " + a.id),
      [
        "rich_file_open acme rf_0a1b2c3d4e5f6071",
        "rich_file_open acme rf_8192a3b4c5d6e7f8",
        "rich_file_reveal acme rf_0a1b2c3d4e5f6071",
      ],
      "what the cards asked the shell to do (ids, never paths)"
    );
    // The keyboard reaches the same open.
    await page.focus('.tl-rfile[data-file-id="rf_8192a3b4c5d6e7f8"] .tl-rfile-main');
    await page.waitForFunction(
      () => document.querySelector('.tl-rfile[data-file-id="rf_8192a3b4c5d6e7f8"]').dataset.s !== "opening"
    );
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => window.__RICHOS_MOCK__.richFileActions().length >= 4);
    assertEqual(page.__errors, [], "the page reported errors");
  });

  await run.check("a file attached between turns shows at once, without a reload", async () => {
    const turn = await page.evaluate(() => document.querySelectorAll(".tl-turn")[document.querySelectorAll(".tl-turn").length - 1].dataset.turnId);
    const id = await page.evaluate((t) => window.__RICHOS_MOCK__.richFileArrive("acme", t, "Acme-signature-page.pdf"), turn);
    await page.waitForSelector('.tl-rfile[data-file-id="' + id + '"]');
    const got = await cards(page);
    assertEqual(got.length, 3, "cards after the live arrival");
    assertEqual(got[2].name, "Acme-signature-page.pdf");
    assertEqual(got[2].meta, "PDF · 12 KB · 1 page");
    // Round 36's arrival: the card that just came rises into place once (`r36-file-in`, 0.7 s,
    // starting 0.25 s after it arrives); the cards a load drew stay still. The page's clock is
    // fixed, so the delay still to run reads exactly 250 ms.
    const motion = () =>
      page.evaluate(() =>
        [...document.querySelectorAll(".tl-rfile")].map((c) => {
          const s = getComputedStyle(c);
          return {
            arriving: c.classList.contains("is-arriving"),
            name: s.animationName,
            duration: s.animationDuration,
            delay: c.style.getPropertyValue("--arrival-delay"),
          };
        })
      );
    const moving = await motion();
    assertEqual(moving.map((m) => m.arriving), [false, false, true], "only the card that arrived rises in");
    assertEqual([moving[2].name, moving[2].duration, moving[2].delay], ["tl-rfile-in", "0.7s", "250ms"], "its rise");
    // A rebuild of the turn while it runs (the reply streaming, a reload) carries its start, never restarts it.
    await page.evaluate((t) => window.__RICHOS_MOCK__.richFileArrive("acme", t, "Acme-annex.pdf"), turn);
    await page.waitForFunction(() => document.querySelectorAll(".tl-rfile").length === 4);
    const again = await motion();
    assertEqual(again.map((m) => m.arriving), [false, false, true, true], "both arrivals still rising after the rebuild");
    assertEqual([again[2].delay, again[3].delay], ["250ms", "370ms"], "the second starts 0.12 s after the first");
    // Reduced motion: no rise at all.
    await page.emulateMedia({ reducedMotion: "reduce" });
    assertEqual((await motion())[3].name, "none", "with reduced motion the card is simply there");
    assertEqual(page.__errors, [], "the page reported errors");
    await page.context().close();
  });

  await run.check("a file that is gone says so and offers Ask Rich again, which asks Rich in his words", async () => {
    const p = await openApp(browser, { missing: true });
    const got = await cards(p);
    assertEqual(got[0].state, "missing");
    assertEqual(got[0].meta, "No longer on this Mac. It was moved or deleted.");
    assert(!got[0].openShown && !got[0].finderShown && got[0].askShown, "gone: only Ask Rich again");
    assertEqual(got[1].state, "ready", "the other file is untouched");
    await p.evaluate(() => {
      const invoke = RichBridge.invoke.bind(RichBridge);
      window.generatedFileRequests = [];
      RichBridge.invoke = (name, args, ...options) => {
        if (name === "send_message") window.generatedFileRequests.push(args);
        return invoke(name, args, ...options);
      };
    });
    // A click on the gone card's body opens nothing.
    await p.click('.tl-rfile[data-file-id="rf_0a1b2c3d4e5f6071"] .tl-rfile-name');
    await p.click('.tl-rfile[data-file-id="rf_0a1b2c3d4e5f6071"] .tl-rfile-ask');
    await p.waitForFunction(() => generatedFileRequests.length === 1);
    const request = await p.evaluate(() => generatedFileRequests[0]);
    assertEqual(request.text, "Please send Acme-counteroffer.pdf again.", "same generated file request");
    assertEqual(request.pressed, true, "button-generated filename is not human-authored text");
    assertEqual(await p.locator(".tl-user-text").filter({ hasText: request.text }).count(), 0, "no invented human message");
    const acts = await p.evaluate(() => window.__RICHOS_MOCK__.richFileActions());
    assertEqual(acts, [], "nothing was opened for a gone file");
    assertEqual(p.__errors, [], "the page reported errors");
    await p.context().close();
  });

  await run.check("with Techy Mode on the cards stay through a turn's end and through reopening the conversation", async () => {
    // Review rv-20261021T083350Z-8ca9bacf-e592: Techy Mode reloads from `get_machinery`, which
    // carried no cards, so the reload at every turn's end took them away.
    const p = await openApp(browser);
    await p.evaluate(() => window.RichBridge.invoke("set_techy_mode", { threadId: "acme", enabled: true }));
    await openThread(p, "hiring");
    await openThread(p, "acme");
    await p.waitForSelector(".tl-rfile");
    assertEqual((await cards(p)).length, 2, "cards after reopening in Techy Mode");
    // Count Techy Mode's snapshot reads, so the check waits for the re-read itself.
    await p.evaluate(() => {
      window.__machineryReads = 0;
      const invoke = window.RichBridge.invoke.bind(window.RichBridge);
      window.RichBridge.invoke = (cmd, args, options) => {
        const answer = invoke(cmd, args, options);
        if (cmd !== "get_machinery") return answer;
        return answer.then((payload) => {
          window.__machineryReads++;
          window.__machineryCards = payload.timeline.items.filter((i) => i.kind === "rich_file").length;
          return payload;
        });
      };
    });
    const turnsBefore = await p.evaluate(() => document.querySelectorAll(".tl-turn").length);
    await p.fill("#input", "and the signed copy when it comes back");
    await p.keyboard.press("Enter");
    await p.waitForFunction((n) => document.querySelectorAll(".tl-turn").length > n, turnsBefore);
    // The reply streams, the turn ends and the conversation is re-read from Techy Mode's snapshot.
    await p.waitForFunction(() => window.__machineryReads > 0, null, { timeout: 30000 });
    assertEqual(await p.evaluate(() => window.__machineryCards), 2, "Techy Mode's snapshot carries the cards");
    await p.waitForFunction(() =>
      [...window.__RICHOS_TIMELINE__().items.values()].filter((i) => i.kind === "rich_file").length === 2 &&
      document.querySelectorAll(".tl-rfile").length === 2);
    const got = await cards(p);
    assertEqual(got.map((c) => c.name), ["Acme-counteroffer.pdf", "Acme-comparables.xlsx"], "the cards after the turn ended");
    assertEqual(p.__errors, [], "the page reported errors");
    await p.context().close();
  });

  await run.check("media moments play and pause in Output or open the selected online moment", async () => {
    const p = await openApp(browser, { prepare: mediaMomentsFixture });
    try {
      const local = p.getByRole("button", { name: /^0?0:53 Customer focus$/ });
      const youtube = p.getByRole("button", { name: /^0?0:26 First useful task$/ });
      await local.click();
      const player = p.locator("#op-viewer audio");
      await player.waitFor();
      await p.waitForFunction(() => {
        const audio = document.querySelector("#op-viewer audio");
        return audio && audio.readyState >= 1 && !audio.paused && !audio.seeking && audio.currentTime >= 53.28 && audio.currentTime < 55;
      });
      assertEqual(await p.evaluate(() => window.RichOutput.snapshot().view), "media", "Output is showing the media preview");
      assertEqual(await player.getAttribute("aria-label"), "Founder interview.wav", "the selected recording owns the player");
      await local.click();
      await p.waitForFunction(() => document.querySelector("#op-viewer audio")?.paused);
      assertEqual(await local.getAttribute("aria-current"), "true", "the paused moment stays selected");
      assert((await p.locator("#op-sub").textContent()).includes("Customer focus"), "the panel names the current moment");
      const scrub = p.locator('.tl-media input[type="range"]');
      await scrub.fill("26000");
      await p.waitForFunction(() => {
        const audio = document.querySelector("#op-viewer audio");
        return audio && !audio.paused && !audio.seeking && audio.currentTime >= 26.35 && audio.currentTime < 28;
      });
      const wave = await scrub.boundingBox();
      const position = { x: wave.width * 0.18, y: wave.height / 2 };
      await scrub.click({ position });
      const requested = Number(await scrub.inputValue()) / 1000;
      const playingFrom = (time) => {
        const audio = document.querySelector("#op-viewer audio");
        // Observe decoded playback past the seek before issuing another seek on the same player.
        return audio && !audio.paused && !audio.seeking && audio.currentTime >= time + 0.35 && audio.currentTime < time + 3;
      };
      await p.waitForFunction(playingFrom, requested);
      await player.evaluate((audio) => { audio.currentTime = 70; });
      await p.waitForFunction(playingFrom, 70);
      assertEqual(Number(await scrub.inputValue()) / 1000, requested, "the focused slider still holds the clicked value");
      await scrub.click({ position });
      await p.waitForFunction(playingFrom, requested);
      await player.evaluate((audio) => { audio.currentTime = audio.duration - 0.8; });
      await p.waitForFunction(() => {
        const audio = document.querySelector("#op-viewer audio");
        return audio && audio.ended && audio.paused && !audio.seeking;
      });
      await local.click();
      await p.waitForFunction(playingFrom, 53.28);
      await p.locator("#op-close").click();
      await player.waitFor({ state: "detached" });
      assertEqual(await p.locator(".tl-media-playhead:visible").count(), 0, "closing Output clears the playhead");
      await youtube.click();
      await p.waitForFunction(() => window.mediaMomentCalls.some(({ cmd }) => cmd === "media_moment_open"));
      const calls = await p.evaluate(() => window.mediaMomentCalls);
      assert(calls.some(({ cmd, args }) => cmd === "output_preview" && args.outputId === "mm_0000000000000001"),
        "the player gets its URL from the selected Output id");
      assertEqual(calls.filter(({ cmd }) => cmd === "media_moment_open"), [
        { cmd: "media_moment_open", args: { fileId: "mm_0000000000000002", momentIndex: 1 } },
      ], "the shell receives the scoped id and selected index");
      await openThread(p, "hiring");
      assertEqual(await p.locator(".tl-media-moment").count(), 0, "another conversation does not show the moments");
      await openThread(p, "acme");
      await local.waitFor();
      assertEqual(await p.locator(".tl-media-moment").count(), 4, "reopening draws each moment once");
      assertEqual(p.__errors, [], "the page reported errors");
    } finally {
      await p.context().close();
    }
  });

  await run.check("a changed recording offers Ask Rich again beside the refused moments", async () => {
    const p = await openApp(browser, { prepare: mediaMomentsFixture });
    try {
      await p.evaluate(() => {
        const invoke = RichBridge.invoke.bind(RichBridge);
        window.mediaRequests = [];
        RichBridge.invoke = (name, args, ...options) => {
          if (name === "output_preview" && args.outputId === "mm_0000000000000001") {
            return Promise.resolve({
              view: "none", why: "refused", reason: "The recording changed. Ask Rich to find the moments again.",
            });
          }
          if (name === "send_message") window.mediaRequests.push(args);
          return invoke(name, args, ...options);
        };
      });
      await p.getByRole("button", { name: /^0?0:53 Customer focus$/ }).click();
      await p.locator("#op-viewer").getByText("The recording changed. Ask Rich to find the moments again.").waitFor();
      const card = p.locator('.tl-media[data-file-id="mm_0000000000000001"][data-s="missing"]');
      await card.getByText("The recording was moved, deleted or changed. Ask Rich to find the moments again.").waitFor();
      assert(await card.getByRole("button", { name: /^0?0:53 Customer focus$/ }).isDisabled(), "stale timestamp is disabled");
      await card.getByRole("button", { name: "Ask Rich again", exact: true }).click();
      await p.waitForFunction(() => window.mediaRequests.length === 1);
      const request = await p.evaluate(() => window.mediaRequests[0]);
      assertEqual(request.text, "Please find the moments in Founder interview.wav again.");
      assertEqual(request.pressed, true, "the generated request is attributed to the button");
      assertEqual(p.__errors, [], "the page reported errors");
    } finally {
      await p.context().close();
    }
  });

  await run.check("a recording scope refusal reopens the conversation that owns the card", async () => {
    const p = await openApp(browser, { prepare: mediaMomentsFixture });
    try {
      await p.evaluate(() => {
        const invoke = RichBridge.invoke.bind(RichBridge);
        window.mediaScopeSwitches = [];
        window.mediaScopePreviews = 0;
        window.mediaScopeRefused = true;
        RichBridge.invoke = (name, args, ...options) => {
          if (name === "switch_thread") window.mediaScopeSwitches.push(args.threadId);
          if (name === "output_preview" && args.outputId === "mm_0000000000000001") {
            window.mediaScopePreviews++;
            if (window.mediaScopeRefused) {
              return Promise.resolve({
                view: "none", why: "outOfScope", reason: "This conversation does not have those moments.",
              });
            }
          }
          return invoke(name, args, ...options);
        };
      });
      const card = p.locator('.tl-media[data-file-id="mm_0000000000000001"]');
      const timestamp = card.getByRole("button", { name: /^0?0:53 Customer focus$/ });
      const reopen = card.getByRole("button", { name: "Reopen conversation", exact: true });
      await timestamp.click();
      await p.locator("#op-viewer").getByText("This conversation does not have those moments.").waitFor();
      await card.getByText("Reopen this conversation to play the recording.").waitFor();
      assertEqual(await card.getAttribute("data-s"), "ready", "scope refusal does not mark the recording missing");
      assertEqual(await card.getByRole("button", { name: "Ask Rich again" }).count(), 0);
      assertEqual(await timestamp.getAttribute("aria-current"), null, "refused playback leaves no active timestamp");
      await timestamp.click();
      await reopen.waitFor();
      await reopen.click();
      await p.waitForFunction(() => window.mediaScopeSwitches.length === 1 &&
        document.getElementById("composer-row").dataset.mode !== "opening");
      await reopen.waitFor({ state: "hidden" });

      await p.evaluate(() => {
        window.mediaScopeRefused = false;
        RichOutput.setThread("hiring");
      });
      await timestamp.click();
      await reopen.waitFor();
      assertEqual(await p.evaluate(() => window.mediaScopePreviews), 1, "the wrong thread issues no preview request");
      await reopen.click();
      await p.waitForFunction(() => window.mediaScopeSwitches.length === 2 &&
        document.getElementById("composer-row").dataset.mode !== "opening");
      await reopen.waitFor({ state: "hidden" });
      assertEqual(await p.evaluate(() => window.mediaScopeSwitches), ["acme", "acme"], "both paths reopen the card's thread");
      await timestamp.click();
      await p.waitForFunction(() => {
        const audio = document.querySelector("#op-viewer audio");
        return audio && !audio.paused && !audio.seeking && audio.currentTime >= 53.28;
      });
      assertEqual(p.__errors, [], "the page reported errors");
    } finally {
      await p.context().close();
    }
  });

  for (const theme of ["dark", "light"]) {
    await run.check(theme + " — the card's text and edges clear WCAG AA", async () => {
      const p = await openApp(browser, { theme });
      const read = await p.evaluate(() => {
        const card = document.querySelector(".tl-rfile");
        const ground = (n) => {
          for (let e = n; e; e = e.parentElement) {
            const bg = getComputedStyle(e).backgroundColor;
            if (bg && bg !== "rgba(0, 0, 0, 0)" && bg !== "transparent") return bg;
          }
          return getComputedStyle(document.body).backgroundColor;
        };
        const fg = (sel) => getComputedStyle(card.querySelector(sel)).color;
        return {
          card: getComputedStyle(card).backgroundColor,
          chat: ground(card.parentElement),
          name: fg(".tl-rfile-name"),
          meta: fg(".tl-rfile-meta"),
          open: fg(".tl-rfile-open"),
          finder: fg(".tl-rfile-finder"),
          edge: getComputedStyle(card).borderTopColor,
          ext: fg(".tl-rfile-ext"),
          glyph: getComputedStyle(card.querySelector(".tl-rfile-glyph .pg")).fill,
        };
      });
      const solid = (c, under) => {
        const v = parseCssColor(c);
        return v.a < 1 ? compositeOver(v, under) : v;
      };
      const chat = solid(read.chat, parseCssColor(theme === "dark" ? "rgb(12, 19, 34)" : "rgb(234, 230, 221)"));
      const card = solid(read.card, chat);
      const lines = [];
      for (const [what, color, ground, floor] of [
        ["file name", read.name, card, 4.5],
        ["type and size", read.meta, card, 4.5],
        ["Open in …", read.open, card, 4.5],
        ["Show in Finder", read.finder, card, 4.5],
        ["file type in the glyph (declared 14px, skippable; floor kept anyway)", read.ext, solid(read.glyph, card), 4.5],
        ["card edge against the chat", read.edge, chat, 3],
      ]) {
        const fg = solid(color, ground);
        const ratio = Math.round(contrastRatio(fg, ground) * 100) / 100;
        lines.push(what + " " + hex(fg) + " on " + hex(ground) + " " + ratio + ":1");
        assert(ratio >= floor, theme + ": " + what + " is " + ratio + ":1 against " + floor + ":1");
      }
      await p.context().close();
      return lines.join(" | ");
    });

    await run.check(theme + " — the committed pictures: the cards, and a file that is gone", async () => {
      const p = await openApp(browser, { theme });
      await p.locator(".tl-rfiles").scrollIntoViewIfNeeded();
      const ready = await shot(p, "files-ready-" + theme, { fullPage: false, parkPointer: true });
      publishShotFile(ready.file, path.join(SHOTS, "files-ready-" + theme + ".png"));
      assertEqual(p.__errors, [], "the page reported errors");
      await p.context().close();
      const q = await openApp(browser, { theme, missing: true });
      await q.locator(".tl-rfiles").scrollIntoViewIfNeeded();
      const missing = await shot(q, "files-missing-" + theme, { fullPage: false, parkPointer: true });
      publishShotFile(missing.file, path.join(SHOTS, "files-missing-" + theme + ".png"));
      assertEqual(q.__errors, [], "the page reported errors");
      await q.context().close();
      return "shots-36/files-ready-" + theme + ".png, shots-36/files-missing-" + theme + ".png";
    });
  }

  await browser.close();
  process.exitCode = run.report() > 0 ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
