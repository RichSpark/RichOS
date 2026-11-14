// THE PASS THAT COLORS EVERY THREAD STAYS A FUNCTION OF ITS OWN, OUTSIDE THE ENGINE'S FRAME FUNCTION.
//
// Round 75 (richos-hq `a4c16a665`, `design/mockups/rounds/round-75/README.md`, "Frame rate"): in
// WebKit the loro picture ran at 17 to 26 frames a second for the rest of a visit after a change of
// the memory count. A timer at each section of the frame found the pass that colors every thread
// taking 43 ms against under 3: it sat inside `frame`, which is very long, and WebKit sometimes left
// it unoptimized there. Taken out unchanged into a function of its own, it is compiled by itself and
// the picture measured 60. `home/field-engine.js` carries it as `colorThreads`; this suite fails the
// moment the pass is written back into `frame`.
//
// It reads the engine's source and runs no browser. `frame` and every other function in the engine
// are declared at the start of a line and end at the first `}` at the start of a line, which is how
// a function's own text is cut out here; check 4 is the positive control that the cut finds an
// inline pass when one is there.
//
// Run: node home-frame-pass.js   (or `npm test` for every suite in this directory)

"use strict";

const fs = require("fs");
const path = require("path");
const { createRun, assert, UI_DIR } = require("./lib/harness");

const ENGINE = path.join(UI_DIR, "home", "field-engine.js");

/// The text of the function declared as `function <name>(` at the start of a line, through the first
/// `}` at the start of a line after it; null when there is no such declaration.
function functionText(source, name) {
  const at = source.search(new RegExp("^function " + name + "\\(", "m"));
  if (at < 0) return null;
  const end = source.indexOf("\n}", at);
  return end < 0 ? null : source.slice(at, end + 2);
}

/// What only the thread-color pass writes: the threads' colors and the ribbons' colors, per vertex.
const PASS_WRITES = [/\blcol\[q\] = /, /\brcol\[q\] = /];

function writesOf(text) {
  return PASS_WRITES.filter((re) => re.test(text)).map(String);
}

(async () => {
  const run = createRun("home-frame-pass", "the thread-color pass is a function of its own");
  const source = fs.readFileSync(ENGINE, "utf8");

  await run.check("1  the engine declares `colorThreads`, and it is the pass that writes every thread's color", async () => {
    const own = functionText(source, "colorThreads");
    assert(own, "home/field-engine.js declares no `function colorThreads(` at the start of a line");
    assert(own.startsWith("function colorThreads(hoverActive, domActive) {"), "colorThreads takes (hoverActive, domActive): " + own.split("\n")[0]);
    const writes = writesOf(own);
    assert(writes.length === PASS_WRITES.length, "colorThreads does not write both the threads' and the ribbons' colors: " + writes.join(", "));
    return own.split("\n").length + " lines, writing " + writes.join(" and ");
  });

  await run.check("2  `frame` calls `colorThreads` and does not carry the pass itself", async () => {
    const frame = functionText(source, "frame");
    assert(frame, "home/field-engine.js declares no `function frame(` at the start of a line");
    assert(/\bcolorThreads\(hoverActive, domActive\);/.test(frame), "frame never calls colorThreads(hoverActive, domActive)");
    const inline = writesOf(frame);
    assert(inline.length === 0, "frame writes the threads' colors itself again (" + inline.join(", ") + "): the pass is back inside the frame function");
    return "frame (" + frame.split("\n").length + " lines) calls it once and writes no thread color";
  });

  await run.check("3  `colorThreads` is declared outside `frame`, not nested in it", async () => {
    const frame = functionText(source, "frame");
    assert(!/function colorThreads\(/.test(frame), "colorThreads is declared inside frame, where it is part of frame's own code");
    return "declared at the start of a line, before frame";
  });

  await run.check("4  positive control: a pass written back into `frame` is found", async () => {
    const own = functionText(source, "colorThreads");
    const body = own.slice(own.indexOf("{\n") + 2, own.lastIndexOf("\n}"));
    const inlined = source.replace(own + "\n", "").replace("  colorThreads(hoverActive, domActive);\n", body + "\n");
    const frame = functionText(inlined, "frame");
    assert(frame && writesOf(frame).length === PASS_WRITES.length, "the cut did not find the pass after it was planted back inside frame");
    assert(functionText(inlined, "colorThreads") === null, "the planted copy still declares colorThreads");
    return "planted inline: both writes found inside frame";
  });

  const failed = run.report();
  console.log(failed ? "\n" + failed + " check(s) FAILED" : "\nthe thread-color pass is colorThreads, outside frame");
  process.exitCode = failed ? 1 : 0;
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
