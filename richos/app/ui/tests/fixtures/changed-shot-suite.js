// A child "suite" for `gate-honesty.js`: it loads the real harness, publishes ONE picture through
// the real `publishShot` against the real committed reference `shots-5c/5c-02-…png`, and reports
// exactly as a real suite does. `changed` publishes a picture that differs from the reference;
// `same` publishes the reference's own bytes. Neither can rewrite the reference (a test run
// never does; `RICHOS_SHOTS_REGENERATE` is removed from the child's environment by its parent).
// Not discovered by `run.js` (it only runs top-level files).

"use strict";

const fs = require("fs");
const path = require("path");
const { createRun, publishShot, publishShotFile, shot, SHOT_DIR } = require(path.join(__dirname, "..", "lib", "harness"));
const { tinyPng } = require("./tiny-png");

/// The same pixels in other bytes: an ancillary text chunk after IHDR, which `lib/png.js` skips.
function reencoded(png) {
  const text = Buffer.from("Comment\0an earlier encoding");
  const length = Buffer.alloc(4);
  length.writeUInt32BE(text.length);
  return Buffer.concat([png.subarray(0, 33), length, Buffer.from("tEXt"), text, Buffer.alloc(4), png.subarray(33)]);
}

/// A stand-in for a Playwright page, enough for `shot()`: it "paints" `bytes`, reports nothing
/// left to settle, a page state with one scrolled column, and the picture's own size. Each
/// answer is chosen by what the harness asks (the evaluated function's text), because there
/// is no DOM here to run it against.
function standInPage(bytes) {
  return {
    screenshot: async () => bytes,
    async evaluate(fn) {
      const text = String(fn);
      if (text.includes("const scrollers = []")) return { focused: "#show", scrollers: [{ element: "#column", scrollTop: 101 }] };
      if (text.includes("new Image")) return { width: 4, height: 4, distinct: 16, sampled: 16 };
      if (text.includes("__richosPinnedLoops")) return 0; // no looping animation to pin or release
      if (text.includes("getAnimations")) return { count: 0, settled: true, ms: 0, still: [] };
      return 0;
    },
  };
}

const mode = process.argv[2];
// Always a throwaway reference `gate-honesty.js` made and named, never a committed one (recheck R27).
const reference = process.env.RICHOS_GATE_HONESTY_REFERENCE;

async function main() {
  if (!reference) throw new Error("RICHOS_GATE_HONESTY_REFERENCE is not set; refusing to touch a committed reference");
  const run = createRun("changed-shot fixture (" + mode + ")");
  await run.check("the fixture's own action: " + mode, async () => {
    switch (mode) {
      case "changed":
        publishShot(tinyPng(40), reference);
        return "published a picture that differs from the committed reference";
      case "changed-and-rewrite":
        // The deliberate violation, for the gate's own self-test only.
        publishShot(tinyPng(40), reference);
        fs.writeFileSync(reference, tinyPng(40));
        return "published a differing picture, then rewrote the reference";
      case "same":
        publishShot(fs.readFileSync(reference), reference);
        return "published the reference's own bytes";
      case "state-none":
      case "state-identical":
      case "state-reencoded": {
        // `shot()` into a scratch file that does not exist yet, already holds these bytes, or
        // holds these pixels in other bytes; then the scratch file is published over the
        // reference, as corrections.js does (`publishShotFile`).
        const fresh = tinyPng(40);
        const scratch = path.join(SHOT_DIR, "gate-honesty-state-" + process.pid + ".png");
        fs.mkdirSync(SHOT_DIR, { recursive: true });
        if (mode === "state-identical") fs.writeFileSync(scratch, fresh);
        if (mode === "state-reencoded") fs.writeFileSync(scratch, reencoded(fresh));
        try {
          const s = await shot(standInPage(fresh), "gate-honesty-state-" + process.pid, { fullPage: false });
          publishShotFile(s.file, reference);
        } finally {
          fs.rmSync(scratch, { force: true });
        }
        return "published a shot taken through shot() from a " + mode.slice(6) + " scratch file";
      }
      default:
        throw new Error("unknown fixture mode " + JSON.stringify(mode));
    }
  });
  process.exit(run.report() ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(2);
});
