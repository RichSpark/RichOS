// `gate-honesty.js` check 1 must neither touch another suite's kept candidate (recheck R27) nor
// pass over a child that rewrote the reference (recheck N04). No browser.
//
// R27: stand a sentinel at the key `corrections.js` really uses, run `gate-honesty.js`, fail if the
// sentinel was replaced or deleted. It restores whatever was there before it started.
// N04: ask the gate's child to also rewrite the reference (the seam RICHOS_GATE_HONESTY_MODE),
// and fail unless the gate itself exits non-zero naming the rewrite.
"use strict";

const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

// `GATE_HONESTY_TESTS_DIR` points the test at another copy of the suites (to prove it red on a
// tree that still has the defect); by default it tests the suites next to it.
const TESTS = process.env.GATE_HONESTY_TESTS_DIR || path.resolve(__dirname, "..");
const KEY = path.join("shots-5c", "5c-02-after-a-plain-decline.png");
const candidate = path.join(TESTS, ".shots", "changed", KEY);
const SENTINEL = Buffer.from("a real candidate kept by another suite\n");

function gate(extra) {
  const env = Object.assign({}, process.env, extra || {});
  delete env.RICHOS_UI_TESTS_LEDGER;
  delete env.RICHOS_SHOTS_REGENERATE;
  const r = spawnSync(process.execPath, [path.join(TESTS, "gate-honesty.js")], { env, encoding: "utf8", timeout: 180000 });
  return { code: r.status, out: (r.stdout || "") + (r.stderr || "") };
}

const prior = fs.existsSync(candidate) ? fs.readFileSync(candidate) : null;
fs.mkdirSync(path.dirname(candidate), { recursive: true });
fs.writeFileSync(candidate, SENTINEL);

let verdict = 0;
try {
  // R27
  const clean = gate();
  const now = fs.existsSync(candidate) ? fs.readFileSync(candidate) : null;
  if (!now) {
    console.log("FAIL  gate-honesty.js deleted the candidate " + candidate);
    verdict = 1;
  } else if (!now.equals(SENTINEL)) {
    console.log("FAIL  gate-honesty.js overwrote the candidate " + candidate);
    verdict = 1;
  } else if (clean.code !== 0) {
    console.log("FAIL  gate-honesty.js itself failed (exit " + clean.code + "):\n" + clean.out);
    verdict = 1;
  } else {
    console.log("PASS  gate-honesty.js left another suite's kept candidate alone and passed");
  }
  // N04
  const bad = gate({ RICHOS_GATE_HONESTY_MODE: "changed-and-rewrite" });
  if (bad.code === 0 || !bad.out.includes("the reference was rewritten by a test run")) {
    console.log("FAIL  a child that rewrote the reference did not fail gate-honesty.js check 1 (exit " + bad.code + "):\n" + bad.out);
    verdict = 1;
  } else {
    console.log("PASS  a child that rewrote the reference fails gate-honesty.js check 1");
  }
} finally {
  if (prior) fs.writeFileSync(candidate, prior);
  else fs.rmSync(candidate, { force: true });
}
process.exitCode = verdict;
