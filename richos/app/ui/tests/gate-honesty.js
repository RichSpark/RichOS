// THE UI GATES MUST BE ABLE TO FAIL — part-2 hunt (2026-09-29), sections 25, 26 and 42.
//
// Three places where the UI test machinery reported green over a defect it was built to catch:
//
//   25. a picture that differs from its committed reference printed one log line and the suite
//       passed, so screenshot comparison could not fail anything;
//   26. `run.js`'s coverage floor counted the harness-appended housekeeping check (the
//       tracked-tree guard) as one of the suite's own, so a suite could skip a declared check
//       and still meet `observed >= declared`;
//   42. `appearance.js` copied each page's error array into its aggregate once, right after the
//       page opened, so its "no page errors anywhere" check never saw an error raised later.
//
// And two the verification reliability PRD named (2026-10-20, T07 and R7):
//
//    7. a failing picture was kept only in the checkout's own scratch, which the merge gate's
//       temporary checkout deletes, so the corrections failure of 2026-10-19 could not be read;
//    8. a run under `RICHOS_SHOTS_REGENERATE` passed, so the run that drew a reference was also
//       the run that approved it.
//
// The suite needs no browser: the picture comparison and the ledger are exercised through child
// suites (`fixtures/changed-shot-suite.js`) that run the real harness, and `run.js`'s floor
// through its real `--coverage` command over fabricated receipts. The throwaway reference this
// file makes is never rewritten by a test run (check 1 would notice); check 8 asks for it to be
// regenerated on purpose and puts it back.

"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { EventEmitter } = require("events");
const { spawnSync } = require("child_process");
const { createRun, assert, assertEqual } = require("./lib/harness");
const png = require("./lib/png");
const { collectPageErrors, createErrorTracker } = require("./lib/page-errors");
const SOURCES = require("./lib/ui-sources");

const FIXTURE = path.join(__dirname, "fixtures", "changed-shot-suite.js");
const { tinyPng } = require("./fixtures/tiny-png");
// A key of this file's own, unique per process so concurrent runs share nothing (recheck R27).
// It sits inside the tests directory (so `publishShot` treats it as a reference) but is created
// here and removed here; no committed reference or other suite's kept candidate is ever read,
// overwritten or deleted by this gate.
// It sits under `.generated-references/`, a declared output directory (recheck R07 v2): a
// directory of its own beside the committed pictures was an input another proof could bind.
const FIXTURE_KEY = path.join(".generated-references", "gate-honesty-" + process.pid, "fixture-reference.png");
const REFERENCE = path.join(__dirname, FIXTURE_KEY);
const KEPT = path.join(__dirname, ".shots", "changed", FIXTURE_KEY);
const REFERENCE_BYTES = tinyPng(24); // differs from the picture the `changed` child publishes
const SHOT_CHECK = "every picture this suite took matches its committed reference";
const REGEN_CHECK = "no picture was approved by the run that drew it";
// The harness's own scratch copies of the evidence a differing picture leaves (checks 7 and 8).
const EVIDENCE_SCRATCH = ["evidence", "regenerated"].map((d) => path.join(__dirname, ".shots", d, path.dirname(FIXTURE_KEY)));

function runFixture(mode, ledger, extra) {
  const env = Object.assign({}, process.env, { RICHOS_GATE_HONESTY_REFERENCE: REFERENCE });
  delete env.RICHOS_SHOTS_REGENERATE; // a regeneration asked of THIS process is not asked of the child
  delete env.RICHOS_UI_TESTS_LEDGER;
  delete env.RICHOS_TEST_RESULTS_ROOT; // the runner's folder for THIS check is not the child's
  if (ledger) env.RICHOS_UI_TESTS_LEDGER = ledger;
  Object.assign(env, extra || {});
  const r = spawnSync(process.execPath, [FIXTURE, mode], { env, encoding: "utf8", timeout: 60000 });
  return { code: r.status, out: (r.stdout || "") + (r.stderr || "") };
}

/// A page that is only what `collectPageErrors` touches: `on`, and something to emit from.
function fakePage() {
  const page = new EventEmitter();
  return page;
}

async function main() {
  const run = createRun("the UI gates can fail — a changed picture, the coverage floor, page errors, kept evidence, a regeneration");
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "richos-gate-honesty-"));
  fs.mkdirSync(path.dirname(REFERENCE), { recursive: true });
  fs.writeFileSync(REFERENCE, REFERENCE_BYTES);
  try {
    await run.check("1  a picture that differs from its reference FAILS the suite that took it, and the reference is not rewritten", async () => {
      const before = fs.readFileSync(REFERENCE);
      // RICHOS_GATE_HONESTY_MODE is a seam for this gate's own self-test
      // (`lib/gate-honesty-own-key.test.js`): it asks the child to ALSO rewrite the reference,
      // the write this check exists to catch.
      const r = runFixture(process.env.RICHOS_GATE_HONESTY_MODE || "changed");
      // Observe the reference as the child left it BEFORE putting anything back (recheck N04):
      // restoring first and asserting afterward makes the assertion true by construction.
      const after = fs.readFileSync(REFERENCE);
      const rewritten = !after.equals(before);
      if (rewritten) fs.writeFileSync(REFERENCE, before); // put it back so later checks start clean
      fs.rmSync(KEPT, { force: true });
      assert(!rewritten, "the reference was rewritten by a test run");
      assert(r.out.includes(SHOT_CHECK), "the child's report has no line about the changed picture:\n" + r.out);
      assert(new RegExp("FAIL\\s+" + SHOT_CHECK).test(r.out), "the changed picture did not fail the suite:\n" + r.out);
      assert(r.out.includes(FIXTURE_KEY.split(path.sep).join("/")), "the failure does not name the picture:\n" + r.out);
      assert(r.out.includes("RICHOS_SHOTS_REGENERATE="), "the failure does not say how to accept the change:\n" + r.out);
      assertEqual(r.code, 1, "the child suite's exit code");
      return "exit 1, FAIL names the picture and how to accept it; reference untouched";
    });

    await run.check("2  negative control: a picture identical to its reference passes", async () => {
      const r = runFixture("same");
      assert(!r.out.includes(SHOT_CHECK), "an unchanged picture was blamed:\n" + r.out);
      assertEqual(r.code, 0, "the child suite's exit code");
      return "exit 0, no picture check raised";
    });

    await run.check("3  the ledger keeps housekeeping out of the suite's own check count", async () => {
      const ledger = path.join(scratch, "ledger.jsonl");
      fs.writeFileSync(ledger, "");
      const r = runFixture("same", ledger);
      assertEqual(r.code, 0, "the child suite's exit code");
      const rec = fs.readFileSync(ledger, "utf8").trim().split("\n").map((l) => JSON.parse(l)).pop();
      assertEqual(rec.productChecks, 1, "the fixture declares and runs ONE check; the tree guard is housekeeping");
      assert(rec.checks >= rec.productChecks, "checks (everything reported) is below productChecks: " + JSON.stringify(rec));
      return "productChecks 1 of checks " + rec.checks;
    });

    await run.check("4  run.js's coverage floor does not let a housekeeping check stand in for a declared one", async () => {
      // tracked-tree.js declares 6 checks. A receipt says it reported 6 in all, but one of them
      // was housekeeping: it ran 5 of its own, and one declared check was never reached.
      const receipt = (productChecks, checks) => ({
        suite: "tracked-tree.js",
        commit: "a".repeat(40),
        shard: "1/1",
        exit: 0,
        seconds: 1,
        declared: 6,
        records: [{ suite: "tracked-tree.js", label: "x", checks, productChecks, failed: 0 }],
      });
      const coverage = (rec) => {
        const dir = fs.mkdtempSync(path.join(scratch, "receipts-"));
        fs.writeFileSync(path.join(dir, "tracked-tree.receipt.json"), JSON.stringify(rec));
        const r = spawnSync(process.execPath, [path.join(__dirname, "run.js"), "--coverage=" + dir], {
          env: Object.assign({}, process.env, { GITHUB_SHA: "a".repeat(40) }),
          encoding: "utf8",
          timeout: 60000,
        });
        return (r.stdout || "") + (r.stderr || "");
      };
      const short = coverage(receipt(5, 6));
      assert(
        short.includes("tracked-tree.js: ran 5 check(s) but its source declares 6"),
        "5 own checks + 1 housekeeping met the floor of 6:\n" + short
      );
      const full = coverage(receipt(6, 7));
      assert(!/tracked-tree\.js: ran \d+ check\(s\) but/.test(full), "6 own checks + housekeeping was refused:\n" + full);
      return "5 of 6 declared (+1 housekeeping): refused · 6 of 6 (+1 housekeeping): accepted";
    });

    await run.check("5  page errors raised AFTER a page is tracked reach the suite-wide aggregate", async () => {
      const tracker = createErrorTracker();
      const page = tracker.track(fakePage());
      collectPageErrors(page);
      assertEqual(tracker.all(), [], "no errors yet");
      // An error from a later click or shortcut — after the page was tracked.
      page.emit("pageerror", new Error("boom during the walk"));
      page.emit("console", { type: () => "error", text: () => "late console error" });
      page.emit("console", { type: () => "log", text: () => "not an error" });
      const all = tracker.all();
      assertEqual(all.length, 2, "the two late errors (the log line is not one)");
      assert(all[0].includes("boom during the walk"), "the page error is missing: " + JSON.stringify(all));
      assertEqual(all[1], "console: late console error", "the console error");
      // And a second page tracked later adds to the same aggregate.
      const other = tracker.track(fakePage());
      collectPageErrors(other);
      other.emit("pageerror", new Error("second page"));
      assertEqual(tracker.all().length, 3, "errors across two pages");
      return "3 errors across 2 pages, all raised after tracking";
    });

    await run.check("6  appearance.js uses the tracker and no longer snapshots a page's errors", async () => {
      const src = SOURCES.stripJsComments(fs.readFileSync(path.join(__dirname, "appearance.js"), "utf8"), { strings: false });
      assert(/createErrorTracker\s*\(/.test(src), "appearance.js does not build its aggregate with createErrorTracker");
      assert(!/allErrors\.push\s*\(/.test(src), "appearance.js still copies a page's errors into an array at track time");
      assert(/tracker\.all|allErrors\s*\(\s*\)/.test(src), "check 18 does not read the live aggregate");
      return "createErrorTracker in use; no snapshot push";
    });

    await run.check("7  a picture that differs leaves its actual, reference, diff and state in the run's own evidence", async () => {
      // T07 (verification reliability PRD, 2026-10-20): the gate's failing picture lived only in
      // the checkout's `.shots/changed/`, which a temporary land checkout deletes with itself.
      const results = path.join(scratch, "results");
      const r = runFixture("changed", null, { RICHOS_TEST_RESULTS_ROOT: results });
      fs.rmSync(KEPT, { force: true });
      assertEqual(r.code, 1, "the child suite's exit code");
      const dir = path.join(results, "shots", "changed", FIXTURE_KEY);
      const read = (f) => (fs.existsSync(path.join(dir, f)) ? fs.readFileSync(path.join(dir, f)) : null);
      const actual = read("actual.png");
      assert(actual && actual.equals(tinyPng(40)), "the run's own evidence has no actual.png holding the picture it drew:\n" + r.out);
      assert(read("reference.png") && read("reference.png").equals(REFERENCE_BYTES), "the evidence has no copy of the reference it was compared with");
      const diff = read("diff.png");
      assert(diff, "the evidence has no diff.png");
      const drawn = png.decode(diff);
      assertEqual([drawn.width, drawn.height], [4, 4], "the difference picture is the pictures' own size");
      const state = JSON.parse(read("state.json").toString("utf8"));
      assertEqual(state.picture, FIXTURE_KEY.split(path.sep).join("/"), "state.json names the picture");
      assert(state.difference && state.difference.ok && state.difference.differing > 0, "state.json carries the measured difference: " + JSON.stringify(state.difference));
      assert(r.out.includes(results), "the failure line does not say where the evidence is:\n" + r.out);
      return "actual, reference, diff (4x4) and state kept under the runner's results folder, and the line names it";
    });

    await run.check("8  a picture a regeneration writes is not approved by the run that drew it", async () => {
      // PRD R7: "generating a picture does not approve it". A regeneration used to pass, so the
      // run that wrote a reference was also the one that compared against it.
      const before = fs.readFileSync(REFERENCE);
      const r = runFixture("changed", null, { RICHOS_SHOTS_REGENERATE: FIXTURE_KEY.split(path.sep).join("/") });
      const after = fs.readFileSync(REFERENCE);
      fs.writeFileSync(REFERENCE, before); // put it back so the checks after this start clean
      assert(!after.equals(before), "the named regeneration did not write the reference, so this check proves nothing:\n" + r.out);
      assert(new RegExp("FAIL\\s+" + REGEN_CHECK).test(r.out), "the regeneration run passed over the picture it drew:\n" + r.out);
      assert(r.out.includes(FIXTURE_KEY.split(path.sep).join("/")), "the failure does not name the picture:\n" + r.out);
      assertEqual(r.code, 1, "the child suite's exit code");
      return "the reference was written, and the run that wrote it failed naming it";
    });
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
    fs.rmSync(path.dirname(REFERENCE), { recursive: true, force: true });
    fs.rmSync(path.dirname(KEPT), { recursive: true, force: true });
    for (const d of EVIDENCE_SCRATCH) fs.rmSync(d, { recursive: true, force: true });
  }

  process.exit(run.report() ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(2);
});
