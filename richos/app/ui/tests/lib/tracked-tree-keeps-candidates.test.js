// `tracked-tree.js` must not touch a real suite's kept candidate picture (hunt part 2, finding 27).
//
// A changed real picture is kept at `.shots/changed/<key>` for a person to look at. This test
// stands a sentinel at the key `corrections.js` really uses, runs `tracked-tree.js`, and fails
// if the sentinel was replaced or deleted. It restores whatever was there before it started.
"use strict";

const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const TESTS = path.resolve(__dirname, "..");
const KEY = path.join("shots-5c", "5c-02-after-a-plain-decline.png");
const candidate = path.join(TESTS, ".shots", "changed", KEY);
const SENTINEL = Buffer.from("a real candidate kept by another suite\n");

const prior = fs.existsSync(candidate) ? fs.readFileSync(candidate) : null;
fs.mkdirSync(path.dirname(candidate), { recursive: true });
fs.writeFileSync(candidate, SENTINEL);

let verdict = 0;
try {
  const env = Object.assign({}, process.env);
  delete env.RICHOS_UI_TESTS_LEDGER;
  delete env.RICHOS_SHOTS_REGENERATE;
  const r = spawnSync(process.execPath, [path.join(TESTS, "tracked-tree.js")], { env, encoding: "utf8", timeout: 120000 });
  const now = fs.existsSync(candidate) ? fs.readFileSync(candidate) : null;
  if (!now) {
    console.log("FAIL  tracked-tree.js deleted the candidate " + candidate + " (exit " + r.status + ")");
    verdict = 1;
  } else if (!now.equals(SENTINEL)) {
    console.log("FAIL  tracked-tree.js overwrote the candidate " + candidate + " (exit " + r.status + ")");
    verdict = 1;
  } else if (r.status !== 0) {
    console.log("FAIL  tracked-tree.js itself failed (exit " + r.status + "):\n" + (r.stdout || "") + (r.stderr || ""));
    verdict = 1;
  } else {
    console.log("PASS  tracked-tree.js left another suite's kept candidate alone and passed");
  }
} finally {
  if (prior) fs.writeFileSync(candidate, prior);
  else fs.rmSync(candidate, { force: true });
}
process.exitCode = verdict;
