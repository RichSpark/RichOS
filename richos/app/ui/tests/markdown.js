// §5.4 MARKDOWN IN RICH'S ANSWERS — the subset, the escaping, and the degradation.
//
// WHAT THIS SUITE EXISTS BECAUSE OF. On the published v1.0.1, the first message ever sent on
// a clean install came back reading, on screen, as literal characters:
//
//     **1. Tell me about Lakeside Advisory.**
//     **2. Authorize the connectors.**
//
// Three such lines in one answer (ray-opus-a2, 2026-09-04). Nothing was broken: the engine
// emits Markdown and `renderRichMessage` set `textContent`, so the asterisks are exactly
// what the model wrote. `timeline.js` now renders a deliberately small subset — bold,
// italic, code spans, ordered and unordered lists, headings, paragraph breaks — and this
// file is what says so.
//
// THREE THINGS IT HAS TO PROVE, and the third is the one that matters most:
//
//   1. THE SUBSET RENDERS. Real elements, and the markers gone from the text.
//   2. AN UNMATCHED MARKER IS LITERAL AND HARMLESS. `**` with no closer, a lone backtick, a
//      `*` used as punctuation — each falls through to its own characters, and NONE of them
//      may eat the rest of the message. A renderer that swallows the tail of an answer is a
//      worse defect than the asterisks it was written to remove.
//   3. MODEL OUTPUT NEVER BECOMES MARKUP. `<img src=x onerror=alert(1)>` and a `<script>`
//      tag are driven through the real renderer, and both the characters on screen AND the
//      absence of the elements are asserted — because either one alone can pass while the
//      other fails. There is no `innerHTML` anywhere on this path, which is checked here as
//      a property of the SOURCE too: an escaping step that does not exist cannot be
//      forgotten, reordered or regressed.
//
// Both write sites are covered: the structural path (`renderRichMessage`, which runs on
// every full render) and the streaming path (`updateProse`, which runs once per animation
// frame per live message). They must agree, or an answer would change shape at the moment
// it completed.
//
// THE DOCUMENT EXTENSION (checks 11-14). A Markdown FILE previewed in the Output panel (output
// side-panel PRD §7, slice S5) is drawn by the same renderer with `{ document: true }`: tables,
// quotations, rules and fenced code, links drawn as their text. The checks prove it renders, that
// Rich's answers (no option) draw the same four blocks but keep links as written, that the
// escaping guarantee holds inside every block, and that no line costs more than linear time.
// Check 15 is the nightly 64 field report: a STOPPED answer's table drawn like a finished one's.
//
// Run: node markdown.js   (or `npm test` for every suite in this directory)

"use strict";

const { loadPlaywright, openFixture, createRun, assert, assertEqual } = require("./lib/harness");
const F = require("./lib/fixtures");

/// Render `text` through the SHIPPING renderer into a `.tl-prose` host and report what came
/// out — the element inventory, the visible text, and the raw markup for the escaping
/// checks. Never re-implements a rule; `renderMarkdownInto` is the exported function
/// `renderRichMessage` and `updateProse` both call.
async function md(page, text) {
  return page.evaluate((t) => {
    let host = document.getElementById("md-host");
    if (!host) {
      host = document.createElement("div");
      host.id = "md-host";
      host.className = "tl-prose";
      document.getElementById("messages").appendChild(host);
    }
    window.RichTimeline.renderMarkdownInto(host, t);
    return {
      text: host.innerText,
      html: host.innerHTML,
      strong: Array.from(host.querySelectorAll("strong")).map((n) => n.textContent),
      em: Array.from(host.querySelectorAll("em")).map((n) => n.textContent),
      code: Array.from(host.querySelectorAll("code")).map((n) => n.textContent),
      ul: Array.from(host.querySelectorAll("ul")).map((n) =>
        Array.from(n.querySelectorAll("li")).map((li) => li.textContent)
      ),
      ol: Array.from(host.querySelectorAll("ol")).map((n) =>
        Array.from(n.querySelectorAll("li")).map((li) => li.textContent)
      ),
      headings: Array.from(host.querySelectorAll('[role="heading"]')).map(
        (n) => n.getAttribute("aria-level") + ":" + n.textContent
      ),
      images: host.querySelectorAll("img").length,
      scripts: host.querySelectorAll("script").length,
      anyElement: host.querySelectorAll("*").length,
    };
  }, text);
}

/// The same renderer with `opts` (the Output panel's `{ document: true }`, or nothing), and an
/// inventory of the blocks the document extension can build.
async function doc(page, text, opts) {
  return page.evaluate(
    ({ t, o }) => {
      const host = document.createElement("div");
      host.className = "of-md";
      document.body.appendChild(host);
      window.RichTimeline.renderMarkdownInto(host, t, null, o || undefined);
      const out = {
        text: host.textContent,
        tables: Array.from(host.querySelectorAll("table")).map((tb) =>
          Array.from(tb.querySelectorAll("tr")).map((tr) => Array.from(tr.children).map((c) => c.textContent))
        ),
        align: Array.from(host.querySelectorAll("table th")).map((th) => th.style.textAlign),
        quotes: Array.from(host.querySelectorAll("blockquote")).map((q) => q.textContent),
        quoteStrong: host.querySelectorAll("blockquote strong").length,
        rules: host.querySelectorAll("hr").length,
        fences: Array.from(host.querySelectorAll("pre > code")).map((c) => ({ text: c.textContent, lang: c.dataset.lang || null })),
        anchors: host.querySelectorAll("a").length,
        images: host.querySelectorAll("img").length,
        scripts: host.querySelectorAll("script").length,
        withHandlers: Array.from(host.querySelectorAll("*")).filter((n) =>
          Array.from(n.attributes).some((at) => /^on/i.test(at.name) || at.name === "href" || at.name === "src")
        ).length,
      };
      host.remove();
      return out;
    },
    { t: text, o: opts || null }
  );
}

async function main() {
  const run = createRun("§5.4 Markdown — the subset, the escaping, the degradation");
  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const page = await openFixture(browser);

  await run.check("1  the field report's own three lines: bold renders, asterisks gone", async () => {
    const r = await md(page, "**1. Tell me about Lakeside Advisory.**\n\n**2. Authorize the connectors.**");
    assertEqual(
      r.strong,
      ["1. Tell me about Lakeside Advisory.", "2. Authorize the connectors."],
      "both lines must be `strong` elements"
    );
    assert(r.text.indexOf("**") === -1, "the asterisks are still on screen: " + JSON.stringify(r.text));
    assert(/^1\. Tell me about Lakeside Advisory\./.test(r.text), "the words themselves changed: " + r.text);
    return "two `strong` elements, zero asterisks — " + JSON.stringify(r.text.split("\n")[0]);
  });

  await run.check("2  italic and code spans, and a code span keeps its own characters", async () => {
    const r = await md(page, "Scoped to the *advisory* label, under `/Lakeside/contacts.csv`.");
    assertEqual(r.em, ["advisory"], "one `em`");
    assertEqual(r.code, ["/Lakeside/contacts.csv"], "one `code`, with its slashes and dot intact");
    assertEqual(
      r.text,
      "Scoped to the advisory label, under /Lakeside/contacts.csv.",
      "the sentence must read as prose once the markers are gone"
    );
    return "em=advisory, code=/Lakeside/contacts.csv";
  });

  await run.check("3  both list kinds, as real lists, with the markers gone from the text", async () => {
    const r = await md(
      page,
      ["Three connectors:", "", "- Calendar", "- Mail", "", "Then, in order:", "", "1. Read the letter", "2. Reconcile", "3. Draft"].join("\n")
    );
    assertEqual(r.ul, [["Calendar", "Mail"]], "one unordered list of two");
    assertEqual(r.ol, [["Read the letter", "Reconcile", "Draft"]], "one ordered list of three");
    assert(r.text.indexOf("- Calendar") === -1, "the hyphen markers are still in the text: " + r.text);
    return "ul(2) + ol(3), markers drawn by the list rather than typed in the text";
  });

  await run.check("4  a heading is a heading, and it is announced as one", async () => {
    const r = await md(page, "## Lakeside Advisory\n\nHere is where things stand.");
    assertEqual(r.headings, ["4:Lakeside Advisory"], "`##` is aria-level 4 and carries the words");
    assert(r.text.indexOf("#") === -1, "the hashes are still on screen: " + r.text);
    return "role=heading aria-level=4, no hashes";
  });

  await run.check("5  ESCAPING: model output never becomes markup", async () => {
    const hostile = '<img src=x onerror=alert(1)> and <script>alert(2)</script> and <b>not bold</b>';
    const r = await md(page, hostile);
    assertEqual(r.images, 0, "an `img` element was created from model text");
    assertEqual(r.scripts, 0, "a `script` element was created from model text");
    assert(
      r.text.indexOf("<img src=x onerror=alert(1)>") !== -1,
      "the characters must be ON SCREEN, verbatim: " + JSON.stringify(r.text)
    );
    assert(r.text.indexOf("<script>alert(2)</script>") !== -1, "the script tag must read as characters: " + r.text);
    assert(r.text.indexOf("<b>not bold</b>") !== -1, "the bold tag must read as characters: " + r.text);
    // The serialized markup is the second half of the proof: the angle brackets have to be
    // ENTITIES in the DOM's own output, which is what proves they are text nodes rather
    // than elements that merely failed to load.
    assert(r.html.indexOf("&lt;img src=x onerror=alert(1)&gt;") !== -1, "not stored as text: " + r.html);
    assertEqual(page.__errors, [], "a page error means something executed");
    return "0 img, 0 script, all three tags on screen as characters, angle brackets escaped in the DOM";
  });

  await run.check("5b escaping holds INSIDE every span the subset can build", async () => {
    const r = await md(
      page,
      ["**<img src=x onerror=alert(1)>**", "", "- `<script>alert(2)</script>`", "", "# <b>heading</b>"].join("\n")
    );
    assertEqual(r.images, 0, "an `img` was created inside a bold span");
    assertEqual(r.scripts, 0, "a `script` was created inside a code span");
    assertEqual(r.strong, ["<img src=x onerror=alert(1)>"], "the bold span holds the characters");
    assertEqual(r.code, ["<script>alert(2)</script>"], "the code span holds the characters");
    assertEqual(r.headings, ["3:<b>heading</b>"], "the heading holds the characters");
    assertEqual(page.__errors, [], "a page error means something executed");
    return "hostile text inside bold, code and a heading — still 0 elements from it";
  });

  await run.check("5c there is no `innerHTML` on the prose path, in the SOURCE", async () => {
    const fs = require("fs");
    const path = require("path");
    const { UI_DIR } = require("./lib/harness");
    const src = fs.readFileSync(path.join(UI_DIR, "timeline.js"), "utf8");
    // Comments say the word; code must not. Strip line comments and block comments first,
    // so the prose above this check cannot make it pass or fail.
    const code = src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "").replace(/^\s*\/\/\/.*$/gm, "");
    const hits = code.split("\n").filter((l) => /innerHTML|insertAdjacentHTML|outerHTML/.test(l));
    assertEqual(hits, [], "an HTML-string sink appeared in timeline.js: " + hits.join(" | "));
    return "timeline.js has no innerHTML / outerHTML / insertAdjacentHTML in code";
  });

  await run.check("6  an unmatched marker is literal, and never eats the rest of the message", async () => {
    const tail = "and the rest of this sentence must still be here.";
    for (const [name, text] of [
      ["unclosed bold", "**never closed " + tail],
      ["unclosed italic", "*never closed " + tail],
      ["unclosed code", "`never closed " + tail],
      ["empty bold", "**** " + tail],
      ["asterisk as punctuation", "3 * 4 * 5 " + tail],
      ["stray hash", "#nothashtag " + tail],
    ]) {
      const r = await md(page, text);
      assert(r.text.indexOf(tail) !== -1, name + " ate the tail: " + JSON.stringify(r.text));
      assertEqual(r.text, text, name + " changed the characters: " + JSON.stringify(r.text));
    }
    return "six malformed inputs, all rendered character-for-character, none truncated";
  });

  await run.check("7  plain prose is untouched, and its own newlines survive", async () => {
    const plain = "They are a twelve-person firm in Bristol.\nI have their filings.\n\nSay the word.";
    const r = await md(page, plain);
    assertEqual(r.strong, [], "no spans invented");
    // THE ONE DELIBERATE CHANGE THIS RENDERER MAKES TO PLAIN TEXT, stated rather than
    // hidden: a blank line between two paragraphs used to be drawn as an empty line by
    // `pre-wrap`, and is now the 0.7em margin between two `.tl-md-p` blocks. So the gap
    // survives as SPACE and not as a newline character. Every OTHER newline is untouched —
    // the single break inside the first paragraph is still a character, which is what the
    // first assertion below measures.
    assert(
      r.text.indexOf("in Bristol.\nI have their filings.") !== -1,
      "a single newline inside a paragraph must still be a line break: " + JSON.stringify(r.text)
    );
    assertEqual(
      r.text.replace(/\s+/g, " "),
      plain.replace(/\s+/g, " "),
      "every word must survive, in order: " + JSON.stringify(r.text)
    );
    const gap = await page.evaluate(() => {
      const ps = document.getElementById("md-host").querySelectorAll(".tl-md-p");
      return { count: ps.length, marginTop: ps.length > 1 ? getComputedStyle(ps[1]).marginTop : null };
    });
    assertEqual(gap.count, 2, "the blank line must produce two paragraph blocks");
    assert(parseFloat(gap.marginTop) > 8, "and the gap must be real space, not nothing: " + gap.marginTop);
    return "words preserved verbatim; the in-paragraph newline is a character, the blank line is a " + gap.marginTop + " margin";
  });

  await run.check("8  the two write sites agree — streaming and structural build the same DOM", async () => {
    const text = "## Next\n\n**1. Read the letter.**\n\n- one\n- two\n\nDone with `contacts.csv`.";
    const r = await page.evaluate((t) => {
      // The STRUCTURAL path: a real snapshot through the real renderer.
      window.__render(
        {
          entityId: "northwind",
          threadId: "thr_fem",
          mode: "ceo",
          bindingRevision: 1,
          items: [
            {
              bindingRevision: 1,
              createdAt: 1787948500000,
              entityId: "northwind",
              id: "turn_ok:text:0",
              kind: "rich_message",
              phase: "unknown",
              sequence: 0,
              slot: "stream",
              text: t,
              threadId: "thr_fem",
              turnId: "turn_ok",
              visibility: "ceo",
            },
          ],
        },
        {}
      );
      const structural = document.querySelector('[id="prose:turn_ok:text:0"]').innerHTML;
      // The STREAMING path: the same text through `updateProse`, which is what runs while
      // the answer is arriving.
      window.RichTimeline.updateProse(document.getElementById("messages"), "turn_ok:text:0", t, false);
      const streaming = document.querySelector('[id="prose:turn_ok:text:0"]').innerHTML;
      return { structural, streaming, strong: document.querySelectorAll('[id="prose:turn_ok:text:0"] strong').length };
    }, text);
    assertEqual(r.streaming, r.structural, "an answer would change shape at the moment it completed");
    assertEqual(r.strong, 1, "the bold span must survive the streaming path too");
    assert(r.structural.indexOf("**") === -1, "asterisks reached the real render path: " + r.structural);
    return "identical markup from both write sites, " + r.structural.length + " bytes";
  });

  await run.check("9  no new color, and the 18px prose scale is untouched", async () => {
    // Check 8 renders a real snapshot, and a full render clears `#messages` — so the host is
    // re-made through the same helper every other check uses rather than assumed present.
    await md(page, "# H\n\n**b** *i* `c`\n\n- item");
    const r = await page.evaluate(() => {
      const host = document.getElementById("md-host");
      const cs = (sel) => {
        const n = host.querySelector(sel);
        return n ? getComputedStyle(n).color : null;
      };
      return {
        prose: getComputedStyle(host).color,
        proseSize: parseFloat(getComputedStyle(host).fontSize),
        strong: cs("strong"),
        em: cs("em"),
        code: cs("code"),
        heading: cs('[role="heading"]'),
        item: cs("li"),
        codeFamily: getComputedStyle(host.querySelector("code")).fontFamily,
      };
    });
    for (const k of ["strong", "em", "code", "heading", "item"]) {
      assertEqual(r[k], r.prose, k + " introduced a color of its own — it must inherit `--ink`");
    }
    assertEqual(r.proseSize, 18, "§17.2: Rich's prose is 18px, and Markdown must not move it");
    assert(/mono/i.test(r.codeFamily), "a code span is distinguished by its face: " + r.codeFamily);
    return "every Markdown node inherits the prose ink (" + r.prose + ") at 18px; code differs by face only";
  });

  // ---- THE DOCUMENT EXTENSION (output side-panel PRD §7, slice S5) ---------------------------
  // A Markdown FILE in the Output panel is drawn with `{ document: true }`: tables, quotations,
  // rules and fenced code, links as their text. Rich's answers never pass it, and the subset
  // above must not move — so each check below renders the same text both ways.
  const DOC = [
    "| Deal | Size |",
    "|---|---:|",
    "| Northwind | $1.9M |",
    "",
    "> quoted **firmly**",
    "",
    "---",
    "",
    "```js",
    "const a = '<b>';",
    "```",
    "",
    "[the ledger](https://example.com/ledger) and ![a chart](chart.png) and \\*not em\\*",
  ].join("\n");

  await run.check("11 a document: a table, a quotation, a rule, fenced code; a link is its text", async () => {
    const r = await doc(page, DOC, { document: true });
    assertEqual(r.tables, [[["Deal", "Size"], ["Northwind", "$1.9M"]]], "one table, header and one row");
    assertEqual(r.align, ["", "right"], "`---:` right-aligns its column");
    assertEqual(r.quotes, ["quoted firmly"], "one quotation, its inline Markdown rendered");
    assertEqual(r.quoteStrong, 1, "the bold inside the quotation is a `strong`");
    assertEqual(r.rules, 1, "one rule");
    assertEqual(r.fences, [{ text: "const a = '<b>';", lang: "js" }], "the fenced block, verbatim, as one code node");
    assertEqual(r.anchors, 0, "a link must not be an `a` — nothing navigates out of the panel (§7)");
    assertEqual(r.images, 0, "an image is its alt text, never an `img`");
    assert(r.text.indexOf("the ledger and a chart and *not em*") !== -1, "the link, the alt text and the escapes read as words: " + JSON.stringify(r.text));
    assert(r.text.indexOf("](") === -1 && r.text.indexOf("|") === -1, "Markdown syntax is still on screen: " + JSON.stringify(r.text));
    return "table 2x2 (right-aligned Size), quotation with bold, 1 rule, fenced js block, link and image as their words";
  });

  // Rich's answers draw the same four BLOCKS (nightly 64, s5: Rich answered with a table and a
  // quotation, and both read on screen as raw `|` and `>` lines). What stays document-only is
  // INLINE: a link and a backslash escape in an answer still read as written.
  await run.check("12 Rich's answers draw the same blocks; links stay as written", async () => {
    const r = await doc(page, DOC);
    assertEqual(r.tables, [[["Deal", "Size"], ["Northwind", "$1.9M"]]], "an answer's table is a table");
    assertEqual(r.align, ["", "right"], "`---:` right-aligns its column in an answer too");
    assertEqual(r.quotes, ["quoted firmly"], "an answer's quotation is a quotation");
    assertEqual([r.rules, r.fences.length], [1, 1], "an answer's rule and fenced code are drawn");
    assert(r.text.indexOf("| Deal") === -1 && r.text.indexOf("> quoted") === -1, "raw `|`/`>` lines still on screen: " + r.text);
    assert(r.text.indexOf("[the ledger](https://example.com/ledger)") !== -1, "a link in an answer reads as written: " + r.text);
    assertEqual([r.anchors, r.images, r.scripts, r.withHandlers], [0, 0, 0, 0], "an answer's blocks made an element that can act");
    return "table, quotation, rule and fence drawn in an answer; the link still reads as written";
  });

  await run.check("13 ESCAPING holds in every block the extension adds", async () => {
    const hostile = [
      "| <img src=x onerror=alert(1)> |",
      "|---|",
      "| <script>alert(2)</script> |",
      "",
      "> <img src=y onerror=alert(3)>",
      "",
      "```",
      "<script>alert(4)</script>",
      "```",
      "",
      "[<img src=z onerror=alert(5)>](javascript:alert(6))",
    ].join("\n");
    const r = await doc(page, hostile, { document: true });
    assertEqual([r.images, r.scripts, r.anchors, r.withHandlers], [0, 0, 0, 0], "an element, a link or a handler was made from file text");
    for (const s of ["<img src=x onerror=alert(1)>", "<script>alert(2)</script>", "<img src=y onerror=alert(3)>", "<script>alert(4)</script>", "<img src=z onerror=alert(5)>"]) {
      assert(r.text.indexOf(s) !== -1, "the characters must be on screen: " + s + " in " + JSON.stringify(r.text));
    }
    assert(r.text.indexOf("javascript:") === -1, "a link's target is dropped, never drawn or followed");
    assertEqual(page.__errors, [], "a page error means something executed");
    return "hostile text in a table cell, a quotation, a fence and a link label: 0 img, 0 script, 0 a, 0 handlers";
  });

  await run.check("14 unclosed or half-formed blocks lose nothing, and no line costs more than linear time", async () => {
    const tail = "and the rest of the file must still be here.";
    const open = await doc(page, "intro\n\n```\nconst x = 1;\n" + tail, { document: true });
    assertEqual(open.fences.map((f) => f.text), ["const x = 1;\n" + tail], "an unclosed fence holds the rest as code, every character kept");
    const pipe = await doc(page, "a | b with no delimiter row\n" + tail, { document: true });
    assertEqual(pipe.tables.length, 0, "a `|` line with no delimiter row under it is a paragraph");
    assert(pipe.text.indexOf("a | b with no delimiter row") !== -1 && pipe.text.indexOf(tail) !== -1, "the paragraph lost words: " + pipe.text);
    // A 200,000-character fence line with a run of spaces inside it, and the same as a rule-like
    // line: a backtracking pattern takes seconds on these; the shipped patterns are linear.
    const dashes = "- ".repeat(100000) + "x";
    const delim = "a|b\n" + "|-".repeat(100000) + "x";
    const long = "~~~ a" + " ".repeat(200000) + "b";
    const ms = await page.evaluate(
      (lines) => {
        const host = document.createElement("div");
        const t0 = performance.now();
        window.RichTimeline.renderMarkdownInto(host, lines.join("\n\n"), null, { document: true });
        return performance.now() - t0;
      },
      [dashes, delim, long]
    );
    assert(ms < 1500, "three long lines took " + Math.round(ms) + " ms");
    return "unclosed fence keeps the tail; a lone `|` line is prose; 600,000 characters of worst case in " + Math.round(ms) + " ms";
  });

  // THE FIELD REPORT (Ray, nightly 64 s5, shots/s01-stopped-03s.jpg): Rich was stopped four
  // seconds into an answer whose text held a table and a quotation, and the stopped answer read
  // as raw `| ... |` and `> ...` lines. The same answer is rendered twice through the REAL
  // snapshot path (`__render`), once stopped by the CEO and once finished, and both must draw a
  // table with no pipe left on screen, with identical markup.
  await run.check("15 a STOPPED answer renders its table and quotation the same as a finished one", async () => {
    const answer = [
      "Here is a structure for the first call.",
      "",
      "| Segment | Time | Purpose |",
      "|---|---|---|",
      "| Current situation | 8-10 min | Understand where they are now |",
      "| Fit and next steps | 5-10 min | Discuss working together |",
      "",
      "**Opening and Agenda**",
      "",
      '> "Thanks for making the time. Here is what I would suggest for our conver',
    ].join("\n");
    const snapshot = (state) => ({
      entityId: "northwind",
      threadId: "thr_table",
      mode: "ceo",
      bindingRevision: 1,
      items: [
        { bindingRevision: 1, createdAt: 1787995711820, entityId: "northwind", id: "turn_tbl:user", kind: "user_message", sequence: null, slot: "opening", source: "text", text: "Give me a structure for a first coaching call.", threadId: "thr_table", turnId: "turn_tbl", visibility: "ceo" },
        { bindingRevision: 1, createdAt: 1787995711856, entityId: "northwind", id: "turn_tbl:text:0", kind: "rich_message", phase: "unknown", sequence: 0, slot: "stream", text: answer, threadId: "thr_table", turnId: "turn_tbl", visibility: "ceo" },
        { activeMs: 4000, bindingRevision: 1, createdAt: 1787995711820, endedAt: 1787995715856, entityId: "northwind", id: "turn_tbl:duration", kind: "work_duration", sequence: null, slot: "terminal", startedAt: 1787995711856, state, threadId: "thr_table", turnId: "turn_tbl", visibility: "ceo" },
      ],
    });
    const drawn = async (state) =>
      page.evaluate((snap) => {
        window.__render(snap, {});
        const prose = document.querySelector('[id="prose:turn_tbl:text:0"]');
        return {
          label: Array.from(document.querySelectorAll(".tl-duration-label")).map((n) => n.textContent),
          html: prose.innerHTML,
          text: prose.innerText,
          rows: Array.from(prose.querySelectorAll("table tr")).map((tr) => Array.from(tr.children).map((c) => c.textContent)),
          quotes: prose.querySelectorAll("blockquote").length,
          strong: prose.querySelectorAll("strong").length,
        };
      }, snapshot(state));
    const stopped = await drawn("stopped");
    const finished = await drawn("completed");
    assertEqual(stopped.label, ["You stopped after 4s"], "the fixture is a CEO-stopped turn");
    assertEqual(
      stopped.rows,
      [["Segment", "Time", "Purpose"], ["Current situation", "8-10 min", "Understand where they are now"], ["Fit and next steps", "5-10 min", "Discuss working together"]],
      "the stopped answer's table must be a table"
    );
    assert(stopped.text.indexOf("|") === -1, "raw `|` lines in the stopped answer: " + JSON.stringify(stopped.text));
    assertEqual([stopped.quotes, stopped.strong], [1, 1], "the cut-off quotation and the bold heading are rendered");
    assert(stopped.text.indexOf("> ") === -1, "a raw `>` line in the stopped answer: " + JSON.stringify(stopped.text));
    assertEqual(stopped.html, finished.html, "the stopped answer is drawn differently from the finished one");
    return "stopped and finished: one 3x3 table, one quotation, no raw `|`; identical markup (" + stopped.html.length + " bytes)";
  });

  await run.check("10 no page errors anywhere in this suite", async () => {
    assertEqual(page.__errors, [], "the renderer logged errors");
    return "0 uncaught errors, 0 console errors";
  });

  await page.close();
  await browser.close();
  return run.report();
}

main().then(
  (failed) => process.exit(failed ? 1 : 0),
  (e) => {
    console.error(e);
    process.exit(1);
  }
);
