"use strict";

// THE SEARCH PALETTE'S DOCUMENTS SECTION, SUPER FINDER AND REQUIRE QUOTED TEXT (search PRD v3.1,
// richos-hq `docs/prds/2026-10-13-qdrant-edge-semantic-search.md`; the CEO's chosen design is
// richos-hq `design/mockups/rounds/round-26/`), driven through the shipping shell against the
// mock's synthetic folder (`mock.js`, `MOCK_DOCS`). What a real folder and a real model do is
// `crates/richos-search`'s to prove; this suite proves what the palette does with the answers:
// the initial checkboxes, a toggle that keeps the query, the four combinations' labels, load on
// scroll with no page controls, request identity, Open original by source ID, the truthful
// fallback and no-folder lines, and their contrast in both themes.

const path = require("path");
const { spawnSync } = require("child_process");
const { loadPlaywright, leaveHome, createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const APP = "file://" + path.join(UI_DIR, "index.html");

async function openApp(browser, preset) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  page.__errors = [];
  page.on("pageerror", (error) => page.__errors.push(String(error)));
  if (preset) await page.addInitScript((v) => { window.__RICHOS_MOCK_PRESET__ = v; }, preset);
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  await page.keyboard.press("Meta+k");
  await page.waitForSelector("#search-overlay:not([hidden])");
  return page;
}

// A wait that says which fact never arrived, instead of a bare timeout.
async function waitFor(page, label, fn, arg) {
  try {
    await page.waitForFunction(fn, arg);
  } catch (error) {
    throw new Error(label + " (" + String(error.message || error).split("\n")[0] + ")");
  }
}

async function docTitles(page) {
  return page.evaluate(() => [...document.querySelectorAll("#search-results .srch-doc .srch-title")].map((t) => t.textContent));
}

function ratio(colors) {
  const contrast = spawnSync("python3", [path.resolve(UI_DIR, "../scripts/qa/contrast.py"), ...colors], { encoding: "utf8" });
  assertEqual(contrast.status, 0, contrast.stdout + contrast.stderr);
  return contrast.stdout.trim();
}

async function main() {
  const run = createRun("Search palette: Documents, RichFinder and Require quoted text");
  const browser = await loadPlaywright().webkit.launch();
  try {
    const page = await openApp(browser);

    await run.check("the palette opens with RichFinder checked and Require quoted text unchecked", async () => {
      const state = await page.evaluate(() => ({
        superFinder: document.getElementById("srch-super").checked,
        strict: document.getElementById("srch-strict").checked,
        hint: !document.getElementById("search-hint").hidden,
        status: document.getElementById("srch-status-text").textContent,
      }));
      assertEqual(state.superFinder, true, "RichFinder");
      assertEqual(state.strict, false, "Require quoted text");
      assert(state.hint, "the empty palette explains the two boxes");
      assert(state.status.startsWith("RichFinder ready"), state.status);
    });

    await run.check("a document found only by meaning is marked so, with its passage and Open original", async () => {
      await page.fill("#search-input", "the app felt slow on first open");
      await page.waitForSelector("#search-results .srch-doc");
      const row = await page.evaluate(() => {
        const r = document.querySelector("#search-results .srch-doc");
        return { why: r.querySelector(".srch-why").textContent, passage: r.querySelector(".srch-passage").textContent, mode: document.querySelector(".srch-group-mode").textContent };
      });
      assertEqual(row.why, "meaning", "why it matched");
      assert(row.passage.length > 40, "a useful passage under the title");
      assertEqual(row.mode, "meaning and words", "the Documents heading says the mode");
      await page.keyboard.press("ArrowDown");
      await page.waitForSelector("#search-results .srch-doc.is-active .srch-open-btn", { state: "visible" });
    });

    await run.check("unchecking RichFinder keeps the query and reruns as text search; checking it brings the match back", async () => {
      await page.click("label.srch-check:has(#srch-super)");
      await page.waitForFunction(() => document.getElementById("srch-status-text").textContent.startsWith("Text search"));
      await page.waitForFunction(() => !document.querySelector("#search-results .srch-doc"));
      assertEqual(await page.inputValue("#search-input"), "the app felt slow on first open", "the query");
      assertEqual(await page.evaluate(() => document.querySelector(".srch-group-mode").textContent), "words only", "the mode");
      const note = await page.evaluate(() => document.querySelector("#search-results .srch-note").textContent);
      assert(note.includes("Text search needs one of your words"), note);
      await page.click("label.srch-check:has(#srch-super)");
      await page.waitForSelector("#search-results .srch-doc");
    });

    await run.check("Require quoted text keeps only the exact span, and unchecking it lets related wording in below it", async () => {
      await page.fill("#search-input", 'cancel "30 days"');
      await page.waitForSelector("#search-results .srch-doc");
      await page.click("label.srch-check:has(#srch-strict)");
      await page.waitForFunction(() => document.querySelector(".srch-group-mode").textContent.endsWith("quoted text required"));
      await page.waitForFunction(() => document.querySelectorAll("#search-results .srch-doc").length === 1);
      const why = await page.evaluate(() => document.querySelector("#search-results .srch-doc .srch-why").textContent);
      assertEqual(why, "exact “30 days”", "the exact tag");
      await page.click("label.srch-check:has(#srch-strict)");
      await page.waitForFunction(() => document.querySelectorAll("#search-results .srch-doc").length > 1);
      const titles = await docTitles(page);
      assertEqual(titles[0], "Refund window for annual plans", "the exact one stays first");
      assert(titles.includes("Early draft of the refund rule"), "the 'thirty days' draft arrives below it");
    });

    await run.check("an unpaired quote under Require quoted text is said, never relaxed", async () => {
      await page.click("label.srch-check:has(#srch-strict)");
      await page.fill("#search-input", 'cancel "30 days');
      await page.waitForFunction(() => /quote marks don’t match/.test((document.querySelector("#search-results .srch-note") || {}).textContent || ""));
      assertEqual((await docTitles(page)).length, 0, "no documents on a guess");
      await page.click("label.srch-check:has(#srch-strict)");
    });

    await run.check("documents load as the list is scrolled, with no page controls", async () => {
      await page.fill("#search-input", "subscription pricing");
      await page.waitForFunction(() => document.querySelectorAll("#search-results .srch-doc").length === 12);
      for (let i = 0; i < 8; i++) {
        if (await page.evaluate(() => (document.getElementById("srch-tail") || {}).className === "srch-end")) break;
        // Counted BEFORE the scroll: a page that arrives between the scroll and a later count
        // would be counted already and the wait would wait for one more that never comes.
        const shown = await page.evaluate(() => document.querySelectorAll("#search-results .srch-doc").length);
        await page.evaluate(() => { const r = document.getElementById("search-results"); r.scrollTop = r.scrollHeight; });
        await page.waitForFunction((n) => document.querySelectorAll("#search-results .srch-doc").length > n
          || (document.getElementById("srch-tail") || {}).className === "srch-end", shown);
      }
      const end = await page.evaluate(() => ({ n: document.querySelectorAll("#search-results .srch-doc").length, text: document.getElementById("srch-tail").textContent }));
      assertEqual(end.n, 31, "every document arrived");
      assertEqual(end.text, "That’s all 31 documents.", "the end of the list says so");
      const words = await page.evaluate(() => document.getElementById("search-overlay").innerText);
      assert(!/\bpage \d|next page|previous page/i.test(words), "no pagination words");
      return end.n + " documents in three scroll loads";
    });

    await run.check("a late reply for an older query never replaces the newer one", async () => {
      // The older query's reply is held until the newer list is on screen, then let go, and the
      // check waits for that reply to have come back: facts, never a clock.
      await page.evaluate(() => {
        const invoke = window.RichBridge.invoke.bind(window.RichBridge);
        window.__asked = [];
        window.__cancellations = [];
        window.__searchTickets = {};
        window.__lateReturned = false;
        window.__releaseLate = new Promise((release) => { window.__letGo = release; });
        window.RichBridge.invoke = async (cmd, args) => {
          if (cmd === "cancel_search_documents") window.__cancellations.push(args.generation);
          if (cmd !== "search_documents") return invoke(cmd, args);
          window.__asked.push(args.request.query);
          window.__searchTickets[args.request.query] = args.generation;
          if (args.request.query !== "refund") return invoke(cmd, args);
          await window.__releaseLate;
          const reply = await invoke(cmd, args);
          window.__lateReturned = true;
          return reply;
        };
      });
      await page.fill("#search-input", "");
      await page.waitForFunction(() => !document.querySelector("#search-results .srch-doc"));
      await page.fill("#search-input", "refund");
      await page.waitForFunction(() => window.__asked.includes("refund"));
      await page.fill("#search-input", "subscription pricing");
      await page.waitForFunction(() => window.__asked.includes("subscription pricing"));
      await page.waitForFunction(() => [...document.querySelectorAll("#search-results .srch-doc .srch-title")]
        .some((t) => t.textContent.startsWith("Subscription pricing")));
      const tickets = await page.evaluate(() => ({ cancelled: window.__cancellations, searches: window.__searchTickets }));
      assert(tickets.cancelled.some((ticket) => ticket > tickets.searches.refund && ticket < tickets.searches["subscription pricing"]),
        "typing sends a cancellation newer than the superseded request before submitting the next query");
      await page.evaluate(() => window.__letGo());
      await page.waitForFunction(() => window.__lateReturned);
      // Two frames, so the palette's own handler for that late reply has run and drawn.
      await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
      const titles = await docTitles(page);
      // The list is still the newer query's: its own first result, and no refund document that
      // only the older query's reply holds.
      assert(titles.length > 0 && titles[0].startsWith("Subscription pricing"), titles.join(" | "));
      assert(!titles.some((t) => /refund/i.test(t)), "the older reply was drawn: " + titles.join(" | "));
    });

    await run.check("Enter on a document opens it by its source ID and closes the palette", async () => {
      await page.evaluate(() => {
        const invoke = window.RichBridge.invoke.bind(window.RichBridge);
        window.__opened = [];
        window.RichBridge.invoke = async (cmd, args) => {
          if (cmd === "open_search_document") window.__opened.push(args);
          return invoke(cmd, args);
        };
      });
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => document.getElementById("search-overlay").hidden);
      const opened = await page.evaluate(() => window.__opened);
      assertEqual(opened.length, 1, "one open");
      assertEqual(Object.keys(opened[0]).join(","), "sourceId", "only a source ID crosses to the host");
      assert(!opened[0].sourceId.includes("/"), "no path");
    });

    await run.check("no browser errors", () => assertEqual(page.__errors.length, 0, page.__errors.join("\n")));
    await page.close();

    await run.check("a newer index generation restarts the list instead of mixing rankings", async () => {
      // While the index is built the service's generation moves after every batch, and a newer
      // generation can rank a newly searchable document first. Here generation N puts N - 1
      // "Forgotten answer" documents ahead of the folder's own ranking, so a page from one
      // generation appended to another's would repeat a document and lose one.
      const p = await openApp(browser);
      await p.evaluate(() => {
        const invoke = window.RichBridge.invoke.bind(window.RichBridge);
        window.__statusGen = 1;
        window.__resultGen = 1;
        window.__statusState = "indexing";
        window.RichBridge.invoke = async (cmd, args) => {
          if (cmd === "search_documents_status") {
            const status = await invoke(cmd, args);
            return { ...status, generation: window.__statusGen, meaning: { ...status.meaning, state: window.__statusState } };
          }
          if (cmd !== "search_documents") return invoke(cmd, args);
          const shift = window.__resultGen - 1;
          const off = args.request.offset;
          const reply = await invoke(cmd, { request: { ...args.request, offset: Math.max(0, off - shift), limit: args.request.limit } });
          const ahead = [];
          for (let i = Math.max(1, off + 1); i <= shift && ahead.length < args.request.limit; i++) {
            ahead.push({ ...reply.hits[0], source_id: "forgotten-" + i, title: "Forgotten answer " + i });
          }
          const hits = ahead.concat(reply.hits).slice(0, args.request.limit);
          const dropped = off < shift ? 0 : reply.dropped;
          return { ...reply, offset: off, total: reply.total + shift, hits, dropped,
            status: { ...reply.status, generation: window.__resultGen } };
        };
      });
      // Reopen, so the status line is read through the stand-in and keeps being read.
      await p.keyboard.press("Escape");
      await p.keyboard.press("Meta+k");
      await p.waitForSelector("#search-overlay:not([hidden])");
      await p.fill("#search-input", "subscription pricing");
      await p.waitForFunction(() => document.querySelectorAll("#search-results .srch-doc").length === 12);
      assert((await docTitles(p))[0].startsWith("Subscription pricing"), "generation 1 first");

      // Indexing finishes a batch: the next status read carries generation 2, and the list is
      // asked again from its first page.
      await p.evaluate(() => { window.__statusGen = 2; window.__resultGen = 2; });
      await waitFor(p, "generation 2 never redrew the list", () => (document.querySelector("#search-results .srch-doc .srch-title") || {}).textContent === "Forgotten answer 1");

      // The status read lags while a scroll asks for the next page and generation 3 answers:
      // that page is not appended; the list starts again from generation 3's first page.
      await p.evaluate(() => { window.__resultGen = 3; });
      await p.evaluate(() => { const r = document.getElementById("search-results"); r.scrollTop = r.scrollHeight; });
      // Generation 3 ranks "Forgotten answer 1" and "Forgotten answer 2" first. Wait for either
      // its first page (the second title is answer 2) or an appended page (24 rows).
      await waitFor(p, "the generation-3 page was neither appended nor restarted", () => {
        const titles = [...document.querySelectorAll("#search-results .srch-doc .srch-title")].map((t) => t.textContent);
        return titles[1] === "Forgotten answer 2" || titles.length >= 24;
      });
      const afterScroll = await docTitles(p);
      assertEqual(afterScroll.slice(0, 2).join(" | "), "Forgotten answer 1 | Forgotten answer 2",
        "generation 3's first page: " + afterScroll.slice(0, 3).join(" | "));

      // Indexing done; scroll to the end: every document once, and the count is generation 3's.
      await p.evaluate(() => { window.__statusGen = 3; window.__statusState = "ready"; });
      for (let i = 0; i < 8; i++) {
        if (await p.evaluate(() => (document.getElementById("srch-tail") || {}).className === "srch-end")) break;
        const shown = await p.evaluate(() => document.querySelectorAll("#search-results .srch-doc").length);
        await p.evaluate(() => { const r = document.getElementById("search-results"); r.scrollTop = r.scrollHeight; });
        await p.waitForFunction((n) => document.querySelectorAll("#search-results .srch-doc").length > n
          || (document.getElementById("srch-tail") || {}).className === "srch-end", shown);
      }
      const keys = await p.evaluate(() => [...document.querySelectorAll("#search-results .srch-doc")].map((n) => n.dataset.key));
      const tail = await p.evaluate(() => document.getElementById("srch-tail").textContent);
      const repeated = keys.filter((k, i) => keys.indexOf(k) !== i);
      assertEqual(repeated.length, 0, "repeated: " + repeated.join(", "));
      assertEqual(keys.length, 33, "every document of generation 3");
      assertEqual(tail, "That’s all 33 documents.", "the end of the list");
      assertEqual(p.__errors.length, 0, p.__errors.join("\n"));
      await p.close();
      return keys.length + " documents, none repeated";
    });

    await run.check("results whose files left the folder never hide the ones after them", async () => {
      // The service fills a page past results whose files changed or left the folder
      // (`crates/richos-search`'s own test); this is the palette's half. The stand-in answers
      // as an unfilled page would: results ranked before `__goneBefore` have left the folder.
      // A page with none left whose list goes on loads on, and "No documents match." is said
      // only when nothing is left at all (fifth review: it was said with two matches behind it).
      const p = await openApp(browser);
      await p.evaluate(() => {
        const invoke = window.RichBridge.invoke.bind(window.RichBridge);
        window.__goneBefore = 12;
        window.RichBridge.invoke = async (cmd, args) => {
          if (cmd !== "search_documents") return invoke(cmd, args);
          const reply = await invoke(cmd, args);
          const gone = Math.max(0, Math.min(reply.hits.length, window.__goneBefore - args.request.offset));
          return { ...reply, hits: reply.hits.slice(gone), dropped: reply.dropped + gone };
        };
      });
      await p.fill("#search-input", "subscription pricing");
      await waitFor(p, "the documents after a gone first page never arrived", () => document.querySelectorAll("#search-results .srch-doc").length > 0
        || /No documents match/.test((document.querySelector("#search-results .srch-note") || {}).textContent || ""));
      const shown = await p.evaluate(() => ({
        rows: document.querySelectorAll("#search-results .srch-doc").length,
        note: (document.querySelector("#search-results .srch-note") || {}).textContent || "",
      }));
      assertEqual(shown.note, "", "no empty-list note while documents remain");
      assertEqual(shown.rows, 12, "the second page's twelve documents");

      // Every result gone: the empty pages are passed through and the list then says so,
      // never a blank list or "That's all N documents." over no rows.
      await p.evaluate(() => { window.__goneBefore = Infinity; });
      await p.fill("#search-input", "subscription pricing notes");
      await waitFor(p, "the empty list was never said", () => /No documents match/.test((document.querySelector("#search-results .srch-note") || {}).textContent || ""));
      assertEqual(await p.evaluate(() => document.querySelectorAll("#search-results .srch-doc").length), 0, "no rows");
      assertEqual(await p.evaluate(() => !!document.getElementById("srch-tail")), false, "no tail under the note");
      assertEqual(p.__errors.length, 0, p.__errors.join("\n"));
      await p.close();
      return "12 documents after a gone first page; an all-gone list says No documents match.";
    });

    await run.check("a conversation result opens the exact message in its own thread", async () => {
      const p = await openApp(browser, { documentSearch: "none" });
      await p.evaluate(async () => {
        const invoke = window.RichBridge.invoke.bind(window.RichBridge);
        const timeline = await invoke("get_timeline", { threadId: "acme" });
        const target = timeline.items.find((item) => item.kind === "rich_message" && item.text.includes("holding at list"));
        if (!target) throw new Error("The Acme search fixture has no target message");
        const anchor = { thread_id: "acme", turn_id: target.turnId, message_id: target.id };
        const locate = window.RichTimeline.searchMessageAnchor;
        window.RichTimeline.searchMessageAnchor = (container, turnId, messageId) => {
          const node = locate(container, turnId, messageId);
          if (node && turnId === anchor.turn_id && messageId === anchor.message_id) {
            document.documentElement.dataset.searchJump = node.dataset.messageId;
          }
          return node;
        };
        window.RichBridge.invoke = async (cmd, args) => {
          if (cmd === "resolve_search_message") return args.sourceId === target.id ? anchor : null;
          if (cmd === "search_nav") return [
            { kind: "message", thread_id: "acme", thread_title: "Indexed exact match", entity_label: "Acme",
              at: 1, excerpt: "An exact match already in the navigation rows.", message_source_id: "exact-match" },
            { kind: "message", thread_id: "acme", thread_title: "Unindexed substring", entity_label: "Acme",
              at: 2, excerpt: "The exact fragment can still be found." },
          ];
          if (cmd === "search_documents") return {
            requested: args.request, effective: args.request, words: [], spans: [], offset: 0, dropped: 0, total: 1,
            status: { generation: 2, folder: { state: "ready", files: 1 }, meaning: { state: "ready", files: 1 } },
            hits: [{ source_id: "document", title: "Price sheet", rel_path: "pricing.md", why: "meaning", exact: [],
              passage: { heading: "", text: "The contract lists annual pricing.", line: 1 } }],
          };
          if (cmd !== "search_conversations") return invoke(cmd, args);
          await new Promise((resolve) => { window.__deliverConversation = resolve; });
          return {
            requested: args.request, effective: args.request, words: [], spans: [],
            status: { folder: { state: "ready" }, meaning: { state: "ready" } },
            hits: [
              { source_id: target.id, title: "Acme", rel_path: "Acme", why: "meaning", exact: [],
                passage: { heading: "", text: target.text, line: 1 } },
              { source_id: "exact-match", title: "Duplicate indexed row", rel_path: "Acme", why: "words", exact: [],
                passage: { heading: "", text: "An exact match already in the navigation rows.", line: 1 } },
            ],
          };
        };
      });
      await p.fill("#search-input", "negotiated discount");
      await p.waitForSelector(".srch-doc");
      await p.evaluate(() => {
        window.__documentSearchRow = document.querySelector(".srch-doc");
        window.__deliverConversation();
      });
      await p.waitForSelector(".srch-message");
      assert(await p.evaluate(() => document.querySelector(".srch-doc") === window.__documentSearchRow),
        "the original document row stays mounted when the conversation result arrives");
      assertEqual(await p.getByText("Unindexed substring", { exact: true }).count(), 1, "keep an exact legacy match without an indexed replacement");
      assertEqual(await p.getByText("Indexed exact match", { exact: true }).count(), 1, "keep the exact row in its original place");
      assertEqual(await p.getByText("Duplicate indexed row", { exact: true }).count(), 0, "deduplicate the same indexed message");
      await p.locator(".srch-message").dblclick();
      await waitFor(p, "the Acme message was never located", () => !!document.documentElement.dataset.searchJump);
      assertEqual(p.__errors.length, 0, p.__errors.join("\n"));
      await p.close();
    });

    await run.check("Company memory opens a verified record with its citation and predecessor", async () => {
      const p = await openApp(browser, { documentSearch: "none" });
      await p.evaluate(() => {
        const invoke = window.RichBridge.invoke.bind(window.RichBridge);
        const text = "The refund deadline was moved to the end of the month.";
        window.RichBridge.invoke = async (cmd, args) => {
          if (cmd === "resolve_search_memory") {
            if (args.sourceId !== "fixture-record-revision") return null;
            return { citation: "companies/acme/records/refund.md", text, supersedes: "refund-v1" };
          }
          if (cmd !== "search_company_memory") return invoke(cmd, args);
          if (args.request.query === "unavailable") throw new Error("The memory projection could not be read");
          return {
            requested: args.request, effective: args.request, words: [], spans: [],
            status: { folder: { state: "ready" }, meaning: { state: "ready" } },
            hits: [{ source_id: "fixture-record-revision", title: "Refund decision", rel_path: "Acme", why: "meaning", exact: [],
              passage: { heading: "", text, line: 1 } }],
          };
        };
      });
      await p.fill("#search-input", "return the money");
      await p.waitForSelector(".srch-memory");
      await p.locator(".srch-memory").dblclick();
      const record = await p.locator(".srch-memory .srch-note").textContent();
      assert(record.includes("companies/acme/records/refund.md"), record);
      assert(record.includes("refund-v1"), record);
      await p.fill("#search-input", "unavailable");
      await waitFor(p, "the unavailable memory state never arrived", () =>
        document.querySelector("#search-results").textContent.includes("Company memory is unavailable."));
      assertEqual(await p.locator(".srch-memory").count(), 0, "the earlier result was cleared");
      assertEqual(p.__errors.length, 0, p.__errors.join("\n"));
      await p.close();
    });

    for (const [preset, line, mode] of [
      ["unavailable", "RichFinder unavailable; showing text matches", "words only, RichFinder unavailable"],
      ["none", "No search folder configured", ""],
    ]) {
      await run.check("status '" + line + "' is said, readable in both themes", async () => {
        const p = await openApp(browser, { documentSearch: preset });
        await p.fill("#search-input", "refund");
        await p.waitForFunction((l) => document.getElementById("srch-status-text").textContent === l, line);
        await p.waitForFunction(() => !!document.querySelector("#search-results .srch-doc, #search-results .srch-note"));
        assertEqual(await p.evaluate(() => document.querySelector(".srch-group-mode").textContent), mode, "the Documents heading");
        if (preset === "none") {
          const note = await p.evaluate(() => document.querySelector("#search-results .srch-note").textContent);
          assert(note.includes("Companies, threads and conversations are still searched above."), note);
          assert(await p.locator(".srch-sources").getByRole("button", { name: "Choose folders…" }).isVisible(), "folder settings can be opened from this state");
        }
        const out = [];
        for (const theme of ["light", "dark"]) {
          const colors = await p.evaluate(async (theme) => {
            document.documentElement.setAttribute("data-theme", theme);
            await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
            // The status line's ink is translucent (--ink-soft), as are some grounds: composite
            // each over what is under it, down to the first opaque ground, the way it is seen.
            const parse = (c) => {
              const n = c.match(/[\d.]+/g).map(Number);
              return { r: n[0], g: n[1], b: n[2], a: n.length > 3 ? n[3] : 1 };
            };
            const over = (top, under) => ({
              r: top.r * top.a + under.r * (1 - top.a),
              g: top.g * top.a + under.g * (1 - top.a),
              b: top.b * top.a + under.b * (1 - top.a),
              a: 1,
            });
            const text = document.getElementById("srch-status-text");
            const layers = [];
            for (let el = text; el; el = el.parentElement) {
              const c = parse(getComputedStyle(el).backgroundColor);
              if (c.a > 0) layers.push(c);
              if (c.a === 1) break;
            }
            let ground = { r: 255, g: 255, b: 255, a: 1 };
            for (const layer of layers.reverse()) ground = over(layer, ground);
            const ink = over(parse(getComputedStyle(text).color), ground);
            const rgb = (c) => [c.r, c.g, c.b].map((v) => Math.round(v)).join(",");
            return [rgb(ink), rgb(ground)];
          }, theme);
          out.push(theme + ": " + ratio(colors));
        }
        assertEqual(p.__errors.length, 0, p.__errors.join("\n"));
        await p.close();
        return out.join("; ");
      });
    }
    await run.check("The Sources column narrows the current query and leaves saved folders alone", async () => {
      const scoped = await openApp(browser);
      try {
        const sources = scoped.locator(".srch-sources");
        const docs = sources.getByRole("checkbox", { name: "Documents" });
        const archive = sources.getByRole("checkbox", { name: "Archive" });
        await docs.waitFor();
        assert(await docs.isChecked() && await archive.isChecked(), "all granted folders start selected");
        await scoped.fill("#search-input", "refund");
        await waitFor(scoped, "both folders match the query", () => document.querySelectorAll(".srch-doc").length === 2);
        await docs.uncheck();
        await waitFor(scoped, "only the archived draft remains", () => {
          const rows = document.querySelectorAll(".srch-doc");
          return rows.length === 1 && rows[0].textContent.includes("Early draft of the refund rule");
        });
        await archive.uncheck();
        await scoped.getByText("No selected folders are available for this search.").waitFor();
        assertEqual((await docTitles(scoped)).length, 0, "explicit empty scope cannot fall back to all folders");
        await scoped.keyboard.press("Escape");
        await scoped.keyboard.press("Meta+k");
        await docs.waitFor();
        assert(await docs.isChecked() && await archive.isChecked(), "the next search starts with all granted folders again");
        assertEqual(scoped.__errors.length, 0, scoped.__errors.join("\n"));
      } finally {
        await scoped.close();
      }
    });
    await run.check("Folder map starts off, refuses locked folders and saves a chosen folder without restart", async () => {
      const folders = await openApp(browser);
      try {
        await folders.keyboard.press("Escape");
        await folders.click("#set-btn");
        await folders.click("#set-search-folders-open");
        await folders.waitForSelector(".rf-folders-page");
        const off = folders.getByRole("radio", { name: "Off", exact: true });
        await waitFor(folders, "the first-run mode is Off", () => document.querySelector('[name="rf-folder-mode"][value="off"]').checked);
        await folders.getByRole("button", { name: "Expand Home" }).click();
        const chosen = folders.getByRole("checkbox", { name: "Search Documents" });
        await chosen.waitFor();
        assert(await folders.getByRole("checkbox", { name: "Search Library" }).isDisabled(), "app data stays locked");
        await chosen.click();
        await waitFor(folders, "the app saved the folder choice", () => document.getElementById("rf-folder-choice-fixture-docs").checked);
        await folders.getByRole("button", { name: "Reindex" }).click();
        await folders.getByText("Reindex started.", { exact: true }).waitFor();
        await chosen.click();
        await waitFor(folders, "removing the last folder turns document search off", () => document.querySelector('[name="rf-folder-mode"][value="off"]').checked);
        assert(await off.isChecked(), "the control reflects the saved Off mode");
        await folders.keyboard.press("Escape");
        await folders.waitForSelector(".rf-folders-scrim", { state: "hidden" });
        assertEqual(folders.__errors.length, 0, folders.__errors.join("\n"));
      } finally {
        await folders.close();
      }
    });
    await run.check("Partial coverage and cleared folder choices are visible in the palette", async () => {
      const page = await openApp(browser, { documentSearch: "partial" });
      try {
        const sources = page.locator(".srch-sources");
        const offline = sources.getByRole("checkbox", { name: /Travel archive.*Offline/ });
        await page.getByText(/1 folder was not searched/).waitFor();
        assert(await offline.isDisabled(), "an offline source cannot be searched");
        assert(await sources.getByRole("checkbox", { name: /Documents\s*\d+/ }).isChecked(), "an available source shows its file count");
        assert((await sources.boundingBox()).x < (await page.locator(".srch-results-pane").boundingBox()).x,
          "Sources is to the left of the results");
        await page.fill("#search-input", "refund");
        await waitFor(page, "documents are shown before changing folders", () => document.querySelectorAll(".srch-doc").length === 2);
        await page.click("#srch-source-settings");
        await page.getByRole("radio", { name: /All eligible/ }).check();
        await page.locator("#rf-folder-done").click();
        await page.getByText(/Your search folders changed/).waitFor();
        assert(await sources.getByRole("checkbox", { name: /Archive\s*\d+/ }).isChecked(), "All eligible still offers individual folders");
        await page.click("#srch-source-settings");
        await page.locator("#rf-folder-clear").click();
        await waitFor(page, "Clear selection switched the folder map off", () => document.getElementById("rf-folder-mode-off").checked);
        await page.locator("#rf-folder-done").click();
        await page.getByText("Search folders cleared. Document results were removed.", { exact: true }).waitFor();
        assertEqual((await docTitles(page)).length, 0, "cleared grants remove stale document results");
        assertEqual(page.__errors.length, 0, page.__errors.join("\n"));
      } finally {
        await page.close();
      }
    });
    await run.check("A company limit narrows Rich without changing the global folder choice", async () => {
      const page = await openApp(browser);
      try {
        await page.keyboard.press("Escape");
        await page.click("#set-btn");
        await page.click("#set-search-folders-open");
        await page.getByRole("button", { name: "Expand Home" }).click();
        const docs = page.getByRole("checkbox", { name: "Search Documents" });
        const archive = page.getByRole("checkbox", { name: "Search Archive" });
        await docs.check();
        await archive.check();
        const company = page.locator(".rf-company-row").filter({ hasText: "Northwind Traders" });
        await company.getByRole("button", { name: "Edit folder limit for Northwind Traders" }).click();
        const choice = company.getByRole("checkbox", { name: "Documents" });
        assert(await choice.isChecked(), "Rich inherits the global choice until a limit is saved");
        await choice.uncheck();
        await company.getByText("1 folder", { exact: true }).waitFor();
        assert(await docs.isChecked() && await archive.isChecked(), "the user's global settings stay selected");
        await company.getByRole("button", { name: "Remove limit" }).click();
        await company.getByText("All search folders", { exact: true }).waitFor();
        assert(await choice.isChecked(), "removing the limit restores inheritance");
        assertEqual(page.__errors.length, 0, page.__errors.join("\n"));
      } finally {
        await page.close();
      }
    });
  } finally {
    await browser.close();
  }
  process.exitCode = run.report() ? 1 : 0;
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
