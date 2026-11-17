// Run every browser suite in this directory, in order, and fail loudly.
//
// Each suite is a plain node script that exits non-zero on any failure, so it is runnable on
// its own (`node workers.js`) when you are working on one thing.
//
// THE INVENTORY IS DISCOVERED FROM DISK, NEVER TYPED. This file used to carry a hand-written
// SUITES array, and slice 6 landed `steering.js` that the array did not name — so a passing
// `run.js` would have reported "all 4 suites passed" while silently running none of its 24
// checks. That is the same defect, in its fifth costume, that produced "13/13 guards" over a
// typed list, "18/18 suites" over one directory's glob, and an install.sh whose HOOK_FILES had
// drifted from the registration. Add a suite and it runs; there is no second place to update.
//
// Zero suites found is exit 2, not "all 0 suites passed" — an empty inventory reporting green
// is exactly how an unreachable CI workflow looked green for months.
//
// AND AN EXIT CODE IS NOT EVIDENCE. Discovery fixed which suites are STARTED. It says nothing
// about what they DID, and every failure this repository has had in this family exited 0
// honestly: a scanner that reported CLEAN over an empty corpus did run, and a suite that
// checks nothing passes perfectly. A CI runner reporting green over that is worse than no CI,
// because it converts "nobody checked" into "somebody checked and it was fine".
//
// So each suite now reports how many checks it ran, through the ledger in `lib/harness.js`,
// and this file gates on the numbers:
//
//   * a suite that produced NO ledger record at all did not check anything — FAIL, whatever
//     it exited;
//   * a suite that ran FEWER checks than its own source declares stopped early — FAIL. The
//     declared count is `run.check(` counted in the suite's own source, so BOTH sides are
//     read off disk and there is no number typed anywhere for someone to forget to update;
//   * a suite that SKIPPED is named and fails the run, unless it was explicitly allowed with
//     `--allow-skip=<file>` — which is a visible argument at the call site, not a silent
//     branch inside the suite;
//   * a suite that RECORDED FAILED CHECKS fails the run whatever it exited;
//   * zero total checks is a failure even if every suite exited 0.
//
// THAT THIRD CLAUSE IS NEW, AND A REAL RUN EARNED IT. On 2026-09-04 the first public
// `ui-suite-ci` run printed, in its own evidence table:
//
//     FAIL            home.js — 29 check(s) run, 28 declared, 2 failed (exit 0)
//
// ...and then reported "2 suite(s) FAILED: scale.js, splash.js". home.js is the one suite in
// this directory that calls `run.report()` and throws the number away, so its process exited
// 0 with two checks red — and this file gated on exit codes alone, so two failures on the
// CEO's own home screen were printed as FAIL and counted as fine. That is this file's entire
// thesis ("AND AN EXIT CODE IS NOT EVIDENCE") failing on the one path where the evidence and
// the exit code disagreed, and the ledger already held the number needed to catch it.
//
// The suite is fixed too — `home.js` exits on its own failures now, as the other twenty do —
// but a gate that depends on twenty-one authors each remembering one line is not a gate.
//
// The relation is `observed >= declared`, not `==`: several suites drive their checks from a
// loop (affordances.js declares 16 and runs 46). A scanner mistake can therefore only make
// this gate STRICTER — it cannot manufacture a green run.

"use strict";

const { spawnSync } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");
const SOURCES = require("./lib/ui-sources");
// Every byte this process prints reaches its reader before it exits (`lib/blocking-stdio.js`).
require("./lib/blocking-stdio")();

// ---------------------------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------------------------

const USAGE =
  "usage: node run.js [--allow-skip=<suite.js> ...]        every suite, serially, gate in-process\n" +
  "       node run.js --shard=<i>/<N> --receipts=<dir>     one shard, writing a receipt per suite\n" +
  "       node run.js --coverage=<dir>                     reconcile every shard's receipts\n" +
  "       node run.js --shards=<N>                         run N shards in parallel, then reconcile\n" +
  "       node run.js --suite=<suite.js> --receipts=<dir>  one suite alone, rewriting its receipt\n" +
  "       node run.js --weigh=<dir>                        rewrite suite-weights.tsv from receipts\n" +
  "       node run.js --plan[=<N>]                         print the packing and exit\n" +
  "  with --coverage: [--quarantine=<suite.js> ...]        report these red, do not fail on them\n" +
  "                   [--proof-out=<file>]                 write a coverage proof for a later gate";

const ALLOWED_SKIPS = new Set();
const QUARANTINED = new Set();
let SHARD = null;
let RECEIPTS = null;
let COVERAGE = null;
let WEIGH = null;
let SHARDS = null;
let PROOF_OUT = null;
let PLAN = null;
let SUITE_ONLY = null;
for (const arg of process.argv.slice(2)) {
  if (arg.startsWith("--allow-skip=")) {
    ALLOWED_SKIPS.add(arg.slice("--allow-skip=".length));
    continue;
  }
  if (arg.startsWith("--shard=")) {
    const m = arg.slice("--shard=".length).match(/^([0-9]+)\/([0-9]+)$/);
    if (!m) {
      console.error("--shard wants <i>/<N>, 1-based, got: " + arg + "\n" + USAGE);
      process.exit(2);
    }
    SHARD = { index: Number(m[1]), total: Number(m[2]) };
    if (SHARD.index < 1 || SHARD.total < 1 || SHARD.index > SHARD.total) {
      console.error("--shard out of range: " + arg + "\n" + USAGE);
      process.exit(2);
    }
    continue;
  }
  if (arg.startsWith("--receipts=")) {
    RECEIPTS = arg.slice("--receipts=".length);
    continue;
  }
  if (arg.startsWith("--coverage=")) {
    COVERAGE = arg.slice("--coverage=".length);
    continue;
  }
  if (arg.startsWith("--shards=")) {
    SHARDS = Number(arg.slice("--shards=".length));
    if (!Number.isInteger(SHARDS) || SHARDS < 1) {
      console.error("--shards wants a positive shard count: " + arg + "\n" + USAGE);
      process.exit(2);
    }
    continue;
  }
  // ONE SUITE, ALONE: the build gate's solo attempt (`nightly-local.py` ui_suite). It runs the
  // suite this file's own way and writes its receipt over the one the sharded run left, so the
  // coverage job that follows reconciles the same directory and stays the only verdict.
  if (arg.startsWith("--suite=")) {
    SUITE_ONLY = arg.slice("--suite=".length);
    continue;
  }
  if (arg.startsWith("--weigh=")) {
    WEIGH = arg.slice("--weigh=".length);
    continue;
  }
  // QUARANTINE IS A VISIBLE ARGUMENT AT THE CALL SITE, exactly as `--allow-skip` is and for
  // the same reason: a list of "known red" suites kept inside this directory is a list that
  // grows quietly and is never read again. Passed from the caller, it is printed in that
  // caller's own output on every single run, next to whether it was NEEDED.
  if (arg.startsWith("--quarantine=")) {
    QUARANTINED.add(arg.slice("--quarantine=".length));
    continue;
  }
  if (arg.startsWith("--proof-out=")) {
    PROOF_OUT = arg.slice("--proof-out=".length);
    continue;
  }
  if (arg === "--plan" || arg.startsWith("--plan=")) {
    PLAN = arg === "--plan" ? 0 : Number(arg.slice("--plan=".length));
    if (!Number.isInteger(PLAN) || PLAN < 0) {
      console.error("--plan wants a shard count: " + arg + "\n" + USAGE);
      process.exit(2);
    }
    continue;
  }
  // A mistyped flag must not be ignored. `--allow-skips=x` quietly doing nothing is the same
  // class of bug as everything else in this file's header.
  console.error("unknown argument: " + arg + "\n" + USAGE);
  process.exit(2);
}
if (SHARD && COVERAGE) {
  console.error("--shard and --coverage are the two ends of one run; pass one.\n" + USAGE);
  process.exit(2);
}
// A shard that runs and writes nothing down cannot be reconciled afterwards, and a sharded
// run whose coverage cannot be proven is twelve green ticks and a hope. So the two travel
// together rather than one being an optional extra somebody forgets in the YAML.
if (SHARD && !RECEIPTS) {
  console.error("--shard needs --receipts=<dir>: an unreceipted shard cannot be reconciled.\n" + USAGE);
  process.exit(2);
}
if (WEIGH && (SHARD || COVERAGE)) {
  console.error("--weigh reads a finished run's receipts; it does not run or reconcile one.\n" + USAGE);
  process.exit(2);
}
// A QUARANTINE A SHARD OBSERVED WOULD BE A GREEN SHARD OVER A RED SUITE, and the shard is
// not the thing that gets to say the run passed. Only the coverage job — the one process
// that has seen every receipt — may hold a failure back from the verdict, so only it takes
// the flag. Anywhere else the flag is a mistake, and a mistaken flag is never ignored here.
if (SHARDS && (SHARD || COVERAGE || WEIGH)) {
  console.error("--shards runs the whole sharded job; it is not combined with one end of it.\n" + USAGE);
  process.exit(2);
}
if (SUITE_ONLY !== null && (SHARD || COVERAGE || WEIGH || SHARDS !== null || PLAN !== null || !RECEIPTS)) {
  console.error("--suite runs one suite alone and needs --receipts=<dir>; it is not combined with a sharded or coverage run.\n" + USAGE);
  process.exit(2);
}
if ((QUARANTINED.size || PROOF_OUT) && !COVERAGE && !SHARDS) {
  console.error(
    "--quarantine and --proof-out are the coverage job's arguments: only the process that " +
      "reconciles every receipt may excuse a suite or certify a set.\n" + USAGE
  );
  process.exit(2);
}

// ---------------------------------------------------------------------------------------
// The inventory
// ---------------------------------------------------------------------------------------

// `lib/` holds shared harness code, not suites. Everything else ending in .js is a suite.
const SUITES = fs
  .readdirSync(__dirname)
  .filter((f) => f.endsWith(".js") && f !== "run.js")
  .filter((f) => fs.statSync(path.join(__dirname, f)).isFile())
  .sort();

if (SUITES.length === 0) {
  console.error("no browser suites discovered in " + __dirname + " — refusing to report green over an empty inventory");
  process.exit(2);
}
if (SUITE_ONLY !== null && !SUITES.includes(SUITE_ONLY)) {
  console.error(`--suite=${SUITE_ONLY} names no suite in ${__dirname}.\n` + USAGE);
  process.exit(2);
}

// ---------------------------------------------------------------------------------------
// How many checks a suite CLAIMS, read off its own source
// ---------------------------------------------------------------------------------------

/// Count `run.check(` outside comments, string literals and regex literals.
///
/// NOT reusing `lib/state-strings.js`'s scanner, deliberately: that one returns string
/// LITERALS with `+`-concatenation folded, which is a different job, and bending it to also
/// emit code positions would put two callers on a parser written for one. It IS the source of
/// the regex rule below — the prev-significant-token heuristic is documented there and this
/// applies it rather than inventing a second one.
///
/// THE REGEX CASE IS NOT OPTIONAL, which the first version of this function assumed it was.
/// Skipping only comments and strings, it read `docs-claims.js` as declaring 2 checks where
/// the file declares 6: `/^\|\s*`([a-z0-9-]+\.js)`\s*\|/gm` contains a backtick, which opened
/// a template literal that swallowed the next four calls. An under-count does not manufacture
/// a green run — `observed >= declared` still holds — but it lowers the floor to a height
/// nothing trips over, which is the same as not having one. A suite that quietly stopped
/// after its third check would have passed.
///
/// THE SCANNER ITSELF NOW LIVES IN `lib/ui-sources.js`, and this file is one of its two
/// consumers rather than the only copy of it. It moved on 2026-09-05 because five checks
/// elsewhere in this directory needed the SAME comment-stripping — an absence claim that
/// reads a comment quoting the code it removed is a false positive, and `setup.js:497` had
/// already hand-rolled a weaker line-based version of it for that exact reason. Two
/// strippers with one job is how the outage card ended up with its own WCAG arithmetic.
///
/// The self-test below is unchanged and now proves the SHARED function, which is the point:
/// the harder consumer's fixture is the one that keeps it honest.
function declaredChecks(src) {
  // The count itself lives in `lib/ui-sources.js` so the harness's evidence ledger records the
  // same number for a suite run directly (recheck R26); this is the one scanner.
  return SOURCES.declaredChecks(src);
}

// The self-test. Five things a naive `grep -c` gets wrong, asserted on every run rather than
// trusted: a live call counts, a commented-out one does not, a block comment does not close
// the file, the string forms are not calls, and a template literal nested inside another
// template's `${ ... }` does not end the outer one.
(function selfTestTheScanner() {
  const fixture = [
    'await run.check("one", async () => {});',
    '// await run.check("a commented-out check", async () => {});',
    '/* run.check( inside a block comment */ await run.check("two", async () => {});',
    'const s = "run.check(";',
    "const t = `run.check(`;",
    // The regression that made this self-test worth having: a regex carrying a quote or a
    // backtick used to open a string literal here and swallow every call after it.
    "const r = /[\"'`]/g;",
    'await run.check("three", async () => {});',
    // ...and its other half, a division that must not be read as a regex opening.
    "const d = (a) / 2; const e = b / c;",
    'await run.check("four", async () => {});',
    // MEASURED, 2026-09-06, on `home.js:2013` and `home/field-engine.js:719`. A template
    // whose interpolation opens another template used to end at the INNER backtick, which
    // inverted string/code polarity for the rest of the file and swallowed the call below —
    // home.js was read as declaring 32 checks where it declares 33.
    "const u = `a ${x ? `b ${y}c` : `d ${y}e`} f`;",
    'await run.check("five", async () => {});',
  ].join("\n");
  const got = declaredChecks(fixture);
  if (got !== 5) {
    console.error(`the declared-check scanner is broken: expected 5 on its own fixture, got ${got}`);
    process.exit(2);
  }
})();

const DECLARED = new Map();
for (const suite of SUITES) {
  DECLARED.set(suite, declaredChecks(fs.readFileSync(path.join(__dirname, suite), "utf8")));
}

// ---------------------------------------------------------------------------------------
// Packing, for the sharded run
// ---------------------------------------------------------------------------------------
//
// WHY THIS EXISTS AT ALL. Every suite launches its own WebKit and this file ran them one at
// a time, on one core of a multi-core runner. Measured on run 34435558131: setup totalled 24
// seconds and the single suite step took 1792 — thirty minutes to learn that a front-end
// suite is red, which is thirty minutes after anybody stopped waiting for the answer.
//
// THE DESIGN IS NOT NEW HERE. `engine/docs/ci-sharding.md` records the same problem solved
// against the engine's own suites eight hours earlier: discovery from disk, weights measured
// in ONE run, longest-first packing, a receipt per unit, and a coverage job that refuses
// unless the union of receipts equals the planned inventory exactly, at one commit. This is
// that design at a second scope, deliberately rather than a second invention of it.
//
// A WEIGHT IS NEVER LOAD-BEARING. It decides which shard a suite lands in and nothing else:
// coverage is proven from receipts, so a stale weight costs wall clock and can never cost
// correctness. That is why a missing weight is a NOTE rather than a failure — but it is not
// silence either, and an unweighted suite is packed as if it were the heaviest one known, so
// a new suite lands in the emptiest shard rather than on top of an already-full one.

const WEIGHTS_FILE = path.join(__dirname, "suite-weights.tsv");

function loadWeights() {
  const w = new Map();
  if (!fs.existsSync(WEIGHTS_FILE)) return w;
  for (const line of fs.readFileSync(WEIGHTS_FILE, "utf8").split("\n")) {
    const t = line.trim();
    if (!t || t.startsWith("#")) continue;
    const [name, secs] = t.split("\t");
    const n = Number(secs);
    // `>= 0`, NOT `> 0`, AND THE DIFFERENCE COST TEN MINUTES A RUN. A suite that finishes in
    // under a second is measured as `0`, and the `> 0` this replaced threw that row away —
    // so `dialect.js` and `docs-claims.js`, the two CHEAPEST suites in the inventory, were
    // read as having no weight at all and then packed as if each were the most expensive one
    // known (305 s). A measurement is not made unusable by being small; only a row that says
    // nothing (missing, blank, non-numeric) is, and `Number.isFinite` is what rejects those.
    // A negative weight is still refused: it is not a duration.
    if (name && Number.isFinite(n) && n >= 0) w.set(name, n);
  }
  return w;
}

/// Longest-processing-time first: sort heaviest first, drop each onto whichever shard is
/// lightest so far. Deterministic — ties in weight break on the name, ties in shard load
/// break on the lowest index — so the same tree packs the same way on every runner, which is
/// what lets the coverage job compare a plan against receipts at all.
function pack(suites, weights, n) {
  const known = [...weights.values()];
  const heaviest = known.length ? Math.max(...known) : 1;
  const weigh = (s) => (weights.has(s) ? weights.get(s) : heaviest);
  const ordered = suites.slice().sort((a, b) => weigh(b) - weigh(a) || (a < b ? -1 : a > b ? 1 : 0));
  const shards = [];
  for (let i = 0; i < n; i++) shards.push({ index: i + 1, suites: [], load: 0 });
  for (const suite of ordered) {
    let best = shards[0];
    for (const sh of shards) if (sh.load < best.load) best = sh;
    best.suites.push(suite);
    best.load += weigh(suite);
  }
  for (const sh of shards) sh.suites.sort();
  return shards;
}

/// The commit every receipt carries. A union of receipts across two trees certifies neither,
/// so this is written into each one and the coverage job refuses a set that disagrees.
function commitSha() {
  if (process.env.GITHUB_SHA) return process.env.GITHUB_SHA;
  const r = spawnSync("git", ["rev-parse", "HEAD"], { cwd: __dirname, encoding: "utf8" });
  return r.status === 0 ? r.stdout.trim() : "";
}

// ---------------------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------------------

const WEIGHTS = loadWeights();

// --plan: print the packing and stop. This is how a shard count is CHOSEN rather than
// guessed, and how the YAML's matrix is checked against what the planner actually does.
if (PLAN !== null) {
  const n = PLAN || 1;
  const shards = pack(SUITES, WEIGHTS, n);
  const total = SUITES.reduce((a, x) => a + (WEIGHTS.has(x) ? WEIGHTS.get(x) : 0), 0);
  console.log(`${SUITES.length} suite(s), ${total.toFixed(0)} s serial by the committed weights, packed into ${n}:`);
  for (const sh of shards) {
    console.log(`  shard ${sh.index}/${n}  ${sh.load.toFixed(0).padStart(5)} s  ${sh.suites.length} suite(s): ${sh.suites.join(", ")}`);
  }
  const longest = Math.max(...shards.map((x) => x.load));
  console.log(`\n  longest shard ${longest.toFixed(0)} s; the floor is the heaviest single suite, ` +
    `${Math.max(...SUITES.map((x) => (WEIGHTS.has(x) ? WEIGHTS.get(x) : 0))).toFixed(0)} s — no shard count gets under it.`);
  const unweighted = SUITES.filter((x) => !WEIGHTS.has(x));
  if (unweighted.length) console.log(`  NOTE  no committed weight, packed as heaviest: ${unweighted.join(", ")}`);
  process.exit(0);
}

// --shards=N: THE WHOLE SHARDED JOB AS ONE COMMAND. Spawn N shards, wait, reconcile.
//
// WHY IT LIVES HERE AND NOT IN THE CALLER. The fan-out was first written inside
// `nightly-local.py`'s build gate, and its own tests refused it: every other gate in that
// file goes through one `command()` helper that owns env, cwd, logging and failure, and a
// second launcher written beside it is exactly the "two implementations of one job" this
// directory keeps paying for. Sharding is THIS file's subject — it already owns the
// packing, the receipts and the coverage gate — so the caller asks for the job, not for
// its pieces, and a build gate is one ordinary subprocess again.
//
// THE SHARDS' EXIT CODES ARE NOT THE VERDICT. They are reported because a reader wants
// them; the verdict is the coverage job's, which is the only thing that has seen every
// receipt and the only thing that can tell a failed suite from an absent one.
// WHERE A FAILED NAVIGATION'S EVIDENCE GOES (`lib/navigation-evidence.js`). Beside the
// receipts, because the receipts directory is the one a build keeps and names when the UI gate
// refuses it ("the receipts are in …", `nightly-local.py`'s `ui_suite`). Every suite and every
// shard inherits it through the environment. With no receipts directory, the harness's own
// default applies: `.shots/navigation-failures/`, gitignored scratch in this directory.
if (!process.env.RICHOS_UI_NAV_EVIDENCE_DIR && RECEIPTS) {
  process.env.RICHOS_UI_NAV_EVIDENCE_DIR = path.join(path.resolve(RECEIPTS), "navigation");
}

if (SHARDS !== null) {
  const { spawn } = require("child_process");
  const dir = RECEIPTS || fs.mkdtempSync(path.join(os.tmpdir(), "richos-ui-receipts-"));
  const owned = !RECEIPTS; // only clean up what this process created
  fs.mkdirSync(dir, { recursive: true });
  // A LEFTOVER RECEIPT IS A GREEN TICK FOR A RUN THAT DID NOT HAPPEN. Coverage compares the
  // commits of the receipts it finds, but only among those present, so the directory starts
  // empty rather than being trusted to be overwritten file for file.
  for (const f of fs.readdirSync(dir)) {
    if (f.endsWith(".receipt.json")) fs.rmSync(path.join(dir, f));
  }
  // AND A LEFTOVER NAVIGATION BUNDLE IS A FAILURE FROM A RUN THAT IS NOT THIS ONE, sitting
  // beside this run's receipts. Same rule, same moment.
  if (!owned) fs.rmSync(path.join(dir, "navigation"), { recursive: true, force: true });
  // A directory this process made is deleted when it exits, so its shards' evidence must not
  // be written inside it: they get the harness's own default, which outlives the run.
  if (owned && !process.env.RICHOS_UI_NAV_EVIDENCE_DIR) {
    process.env.RICHOS_UI_NAV_EVIDENCE_DIR = path.join(__dirname, ".shots", "navigation-failures");
  }
  console.log(`${SUITES.length} suite(s) over ${SHARDS} shard(s), receipts in ${dir}\n`);
  // EACH SHARD IS A WORKER, AND UNDER A WORKER BUDGET IT HOLDS A TOKEN OF IT.
  //
  // The nightly runs every gate through `worker_tokens.py machine`, which holds ONE token of
  // the machine budget (80% of the cores) for the gate's command and exports the budget to it.
  // Every other wide gate spends that budget per worker: a mutation pool takes one token per
  // mutant, a proof run one per check. This command used to run its four WebKit shards on the
  // one token it was given, so the budget admitted a full mutation pool and a full script-suite
  // run BESIDE four shards it never counted, and the host sat at 95-100% CPU. Tonight's three
  // failed nightlies (20260928T224842Z-c3cce554, 20260928T230511Z-20aa349a,
  // 20260928T233221Z-56bcde43) each failed a different wall-clock check in whichever gate ran
  // beside this one, and each of those suites passed alone on the same commit.
  //
  // So under a budget, a shard is started the way `mutation-pool.sh` starts a mutant: through
  // `worker_tokens.py run <budget> --free <the caller's own slot>`. The first shard runs on the
  // token this command already holds; every other shard waits for a token of the budget, so the
  // whole machine never admits more workers than it has. With the Mac otherwise free, all four
  // get a token at once and the suite costs what it always did. Without a budget (a person
  // running `node run.js --shards=4`), nothing changes.
  const budget =
    process.env.RICHOS_WORKER_TOKENS &&
    process.env.RICHOS_WORKER_TOKENS_TOOL &&
    process.env.RICHOS_WORKER_SLOT_HELD === "1" &&
    process.env.RICHOS_WORKER_BORROW_LOCK
      ? {
          dir: process.env.RICHOS_WORKER_TOKENS,
          tool: process.env.RICHOS_WORKER_TOKENS_TOOL,
          free: process.env.RICHOS_WORKER_BORROW_LOCK,
        }
      : null;
  if (budget) {
    console.log(
      `each shard holds one worker token of ${budget.dir}; the first runs on this command's own, ` +
        `the others wait for the budget\n`
    );
  }
  const started = Date.now();
  const children = [];
  for (let i = 1; i <= SHARDS; i++) {
    const t0 = Date.now();
    const args = [__filename, `--shard=${i}/${SHARDS}`, `--receipts=${dir}`];
    for (const s of ALLOWED_SKIPS) args.push(`--allow-skip=${s}`);
    const [file, argv] = budget
      ? ["python3", [budget.tool, "run", budget.dir, "--free", budget.free, "--", process.execPath, ...args]]
      : [process.execPath, args];
    const child = spawn(file, argv, { stdio: ["ignore", "pipe", "pipe"] });
    const chunks = [];
    child.stdout.on("data", (d) => chunks.push(d));
    child.stderr.on("data", (d) => chunks.push(d));
    children.push(
      new Promise((resolve) =>
        // OUTPUT IS HELD AND PRINTED WHOLE, per shard. Four concurrent writers on one
        // stream interleave line by line, and a log nobody can read is the same as no log
        // on the morning the gate refuses something.
        child.on("close", (code) =>
          resolve({ index: i, code, seconds: (Date.now() - t0) / 1000, out: Buffer.concat(chunks).toString() })
        )
      )
    );
  }
  Promise.all(children).then((results) => {
    for (const r of results.sort((a, b) => a.index - b.index)) {
      console.log(`--- shard ${r.index}/${SHARDS} (exit ${r.code}, ${r.seconds.toFixed(0)}s) ---`);
      console.log(r.out.trimEnd());
    }
    const wall = (Date.now() - started) / 1000;
    const longest = Math.max(...results.map((r) => r.seconds));
    console.log(
      `\n${SHARDS} shard(s) finished in ${wall.toFixed(0)} s wall clock; per shard ` +
        results.sort((a, b) => a.index - b.index).map((r) => `${r.index}:${r.seconds.toFixed(0)}s`).join(" ") +
        `. The run costs its LONGEST shard (${longest.toFixed(0)} s), never its average — ` +
        `an unbalanced pack is paid for here.\n`
    );
    // runCoverage exits the process; clean the directory this process made before it does.
    if (owned) process.on("exit", () => fs.rmSync(dir, { recursive: true, force: true }));
    runCoverage(dir);
  });
  return;
}

// --weigh: this process runs no suite either. It turns a finished run's receipts into the
// packing weights, so the file that decides shard balance is measured rather than typed.
if (WEIGH) {
  runWeigh(WEIGH);
  // runWeigh never returns.
}

// --coverage: this process runs no suite. It reconciles what the shards wrote down.
if (COVERAGE) {
  runCoverage(COVERAGE);
  // runCoverage never returns.
}

// WHICH SUITES THIS PROCESS RUNS. Unsharded, that is all of them and this file behaves
// exactly as it always has. Sharded, it is one packed subset — and the subset is derived
// from the same disk discovery, so a suite added to the directory joins a shard without
// anybody editing a list.
const MINE = SUITE_ONLY ? [SUITE_ONLY] : SHARD ? pack(SUITES, WEIGHTS, SHARD.total)[SHARD.index - 1].suites : SUITES;

// A SHARD THAT VERIFIES NOTHING MUST NEVER EXIT 0. With 31 suites and a sane shard count
// this cannot happen, but "cannot happen" is how the empty-inventory green run happened.
if (MINE.length === 0) {
  console.error(
    `shard ${SHARD.index}/${SHARD.total} was packed with no suites at all — refusing to ` +
      `report green over an empty shard. Reduce the shard count.`
  );
  process.exit(2);
}

const ledgerDir = fs.mkdtempSync(path.join(os.tmpdir(), "richos-ui-evidence-"));
const ledgerFile = path.join(ledgerDir, "evidence.jsonl");
fs.writeFileSync(ledgerFile, "");

// A RUN THAT IS KILLED LEAVES NOTHING BEHIND — SWEPT AT STARTUP, NOT CAUGHT AT EXIT.
//
// The normal path removes this directory once the records have been read, which covers a
// pass and a failure and covered nothing on the two ways a run actually ends early: a
// Ctrl-C, and a build gate terminating its shards. MEASURED 2026-09-20: `$TMPDIR` held
// fourteen `richos-ui-evidence-*` directories, ten of them from 2026-09-19, every one the
// residue of a run somebody stopped. Cleaning up is not conditional on finishing (CEO §54).
//
// A SIGNAL HANDLER IS THE OBVIOUS FIX AND IT IS THE WRONG ONE. It was written, measured and
// removed within the hour: this file runs its suites with `spawnSync`, so the event loop is
// blocked for the whole of a suite — up to 150 s — and a queued SIGTERM is not delivered
// until that suite returns. Registering a handler therefore does not clean up promptly, it
// makes the shard UNKILLABLE for the length of a suite. Measured: `kill -TERM` left the
// process alive and running, and it took `kill -9`. A cleanup that hangs the thing it is
// cleaning up after is worse than the leak it fixes.
//
// So the sweep happens at startup, where there is nothing to block it, and it is precise
// rather than time-based: each directory records the pid that owns it, and a directory
// whose recorded owner is confirmed gone is residue. Never a heuristic about age — a long
// legitimate run must not have its ledger deleted out from under it by the next one.
fs.writeFileSync(path.join(ledgerDir, "owner.pid"), String(process.pid));
(function sweepAbandonedLedgers() {
  for (const entry of fs.readdirSync(os.tmpdir())) {
    if (!entry.startsWith("richos-ui-evidence-")) continue;
    const dir = path.join(os.tmpdir(), entry);
    if (dir === ledgerDir) continue;
    let owner;
    try {
      const recorded = fs.readFileSync(path.join(dir, "owner.pid"), "utf8").trim();
      if (!/^[1-9][0-9]*$/.test(recorded)) continue;
      owner = Number(recorded);
      if (!Number.isSafeInteger(owner)) continue;
    } catch (_e) {
      // Another shard can be between mkdtemp and writing its owner, including
      // the moment when owner.pid exists but is still empty. Missing, unreadable
      // or invalid ownership is unknown, not dead. The enclosing scratch owner
      // handles any residue that never acquired a readable PID.
      continue;
    }
    if (owner) {
      // ONLY `ESRCH` MEANS GONE, and the first version of this treated every throw as gone.
      // `process.kill(pid, 0)` also throws `EPERM` — the process EXISTS and this user may
      // not signal it — so "any error means dead" deletes the live ledger of a run started
      // by another user. Measured: a directory owned by pid 1 was swept away while launchd
      // was plainly running. Anything that is not a definite "no such process" is treated
      // as alive, because the cost of keeping residue one run longer is a directory, and
      // the cost of the other mistake is a live run losing its evidence mid-flight.
      let gone = false;
      try {
        process.kill(owner, 0); // Signal 0 tests for existence; it delivers nothing.
      } catch (e) {
        gone = e.code === "ESRCH";
      }
      if (!gone) continue;
    }
    fs.rmSync(dir, { recursive: true, force: true });
  }
})();

if (SUITE_ONLY) {
  console.log(`${SUITES.length} suite(s) discovered; running ${SUITE_ONLY} alone\n`);
} else if (SHARD) {
  console.log(
    `${SUITES.length} suite(s) discovered; shard ${SHARD.index}/${SHARD.total} runs ` +
      `${MINE.length}: ${MINE.join(", ")}\n`
  );
} else {
  console.log(`${SUITES.length} suite(s) discovered: ${SUITES.join(", ")}\n`);
}

// EVERY SUITE HAS A LIMIT OF ITS OWN, so one that never exits costs that limit, not the gate.
// Run 20261117T180249Z-3ac38bbc: rich-files.js, 26 s alone, sat on an unbounded await in
// shard 3 from about 18:34 until the build's 3300 s deadline killed the whole UI gate at 18:58.
// Shard 3 wrote no receipts, seven suites never ran, and the build's solo retry of a red suite
// never started, because it runs only after the sharded command returns. Stopped here, the
// suite is red with exit 124 in its receipt, the shard goes on, and the retry gets its turn.
// 900 s: the slowest suite measured is contrast.js at 662 s (2026-11-17 05:53Z run, all gates
// running at once), and the longest shard is about 2,110 s, so a stopped suite plus its retry
// still fits inside the gate's 3300 s. SIGKILL, because a suite stuck in a Playwright call may
// not act on SIGTERM; its browser exits when the suite's end of the pipe closes.
const SUITE_LIMIT_SECONDS = (() => {
  const v = Number(process.env.RICHOS_UI_SUITE_LIMIT_SECONDS);
  return Number.isFinite(v) && v > 0 ? v : 900;
})();
const exited = new Map();
const seconds = new Map();
const stopped = new Map();
for (const suite of MINE) {
  const t0 = Date.now();
  const r = spawnSync(process.execPath, [path.join(__dirname, suite)], {
    stdio: "inherit",
    env: Object.assign({}, process.env, { RICHOS_UI_TESTS_LEDGER: ledgerFile }),
    timeout: SUITE_LIMIT_SECONDS * 1000,
    killSignal: "SIGKILL",
  });
  seconds.set(suite, (Date.now() - t0) / 1000);
  if (r.error && r.error.code === "ETIMEDOUT") {
    stopped.set(suite, `no exit within its limit of ${SUITE_LIMIT_SECONDS} s; run.js stopped it`);
    console.log(`\n  STOPPED  ${suite}: ${stopped.get(suite)}\n`);
    exited.set(suite, 124);
  } else {
    exited.set(suite, r.status === null ? 1 : r.status);
  }
}

// ---------------------------------------------------------------------------------------
// The evidence gate
// ---------------------------------------------------------------------------------------
//
// ONE IMPLEMENTATION, THREE CALLERS: the serial run, each shard, and the coverage job that
// reconciles the shards. That is deliberate. `engine/docs/ci-sharding.md` names the failure
// this shape avoids — "a shard that runs FEWER units than it was given still exits 0" — and
// a second copy of the gate written for the sharded path would be a second thing to keep
// true, which is the defect this whole file exists to end.
//
// `planned` is the set of suites the caller was responsible for. `data` maps each suite that
// actually ran to its exit code, its ledger records and its wall clock. Everything the gate
// concludes is derived from those two, so a suite that never started is a hole in `data`
// rather than an absence nobody can see.

// QUARANTINE — WHY IT IS HERE AND WHAT IT DELIBERATELY DOES NOT COVER.
//
// A gate that turns test rot into a failed BUILD stops the build and does not fix the app.
// The CEO's question, 2026-09-19: "Will that get me back to that endless fixing of millions
// of bugs without doing any actual work?" A quarantined suite still RUNS, is still counted,
// and is still printed in the build's own output on every single run — it just does not
// hold the build. That is the difference between knowing a suite is rotten and pretending
// it is green.
//
// TWO THINGS KEEP THE LIST FROM ROTTING, and they matter more than the excusing does:
// a quarantined suite that PASSES says so, loudly, as a note naming the row to delete; and
// the list is supplied by the CALLER, so it is printed where the caller's output is read
// rather than living in this directory where nobody re-reads it.
//
// A MISSING RECEIPT IS NEVER EXCUSED. Quarantine is a statement about a suite's own
// assertions being stale. "Nothing was written down" is not that: it means a shard died, or
// never started, or its artifact vanished — an integrity failure of the run itself, which
// is precisely the class this whole file exists to refuse. Excusing it would let a
// quarantine entry silently hide a broken harness.
function gate(planned, data, opts) {
  const label = (opts && opts.label) || "";
  const quarantined = (opts && opts.quarantined) || new Set();
  const problems = [];
  const excused = [];
  const notes = [];
  // Route a suite's own defect to the verdict, or to the excused list when it is
  // quarantined. Everything about a suite goes through here EXCEPT the missing-receipt case.
  const fault = (suite, text) => (quarantined.has(suite) ? excused : problems).push(text);
  let ran = 0;
  let skipped = 0;
  let observedTotal = 0;
  let declaredTotal = 0;

  console.log("\n== evidence ==" + (label ? "  " + label : ""));
  for (const suite of planned) {
    const entry = data.get(suite);
    const recs = entry ? entry.records : [];
    const skips = recs.filter((r) => typeof r.skipped === "string");
    const runs = recs.filter((r) => typeof r.checks === "number");
    const declared = DECLARED.get(suite);
    // THE SUITE'S OWN CHECKS ONLY. `checks` also counts the housekeeping checks `report()`
    // appends (the tracked-tree guard, the changed-picture guard); those are not in the
    // declared count, so counting them let a suite skip a declared check and still meet the
    // floor (part-2 hunt section 26). A record written before `productChecks` existed falls
    // back to `checks`.
    const observed = runs.reduce((a, r) => a + (typeof r.productChecks === "number" ? r.productChecks : r.checks), 0);
    const failedChecks = runs.reduce((a, r) => a + r.failed, 0);
    const status = entry ? entry.exit : null;

    if (declared === 0) {
      fault(suite, `${suite}: its source declares no \`run.check(\` at all — a suite that cannot fail`);
    }

    // NO RECEIPT AT ALL is a sharded-run failure the serial runner could not have. A matrix
    // entry that never started, a job a concurrency rule stopped, an artifact that did not
    // upload: each leaves a suite with nothing written down, and each would be a green tick
    // over an unverified commit if this said nothing.
    if (!entry) {
      problems.push(
        `${suite}: NOTHING WAS WRITTEN DOWN for it. It was planned and no shard reported it — ` +
          `a suite that did not run cannot be counted as one that passed.`
      );
      console.log(`  NO RECEIPT      ${suite} (${declared} declared)`);
      continue;
    }

    // STOPPED AT ITS LIMIT: red under its own name, so the build's solo retry picks it up.
    if (entry.stopped) {
      fault(suite, `${suite}: STOPPED, ${entry.stopped}. A suite that never exits has an await nothing bounds.`);
      console.log(`  STOPPED         ${suite} — ${entry.stopped} (exit ${status})`);
      continue;
    }

    if (skips.length && !runs.length) {
      skipped++;
      if (ALLOWED_SKIPS.has(suite)) {
        console.log(`  SKIP (allowed)  ${suite} — ${skips[0].skipped.split("\n")[0]}`);
      } else {
        console.log(`  SKIP            ${suite} — ${skips[0].skipped.split("\n")[0]}`);
        fault(
          suite,
          `${suite}: did not run and was not allowed to skip. Pass --allow-skip=${suite} at the ` +
            `call site if that is deliberate, so the gap is visible where the run is started.`
        );
      }
      continue;
    }

    if (!runs.length) {
      fault(
        suite,
        `${suite}: produced NO evidence — it exited ${status} without reporting a single check. ` +
          `Its source declares ${declared}.`
      );
      console.log(`  NO EVIDENCE     ${suite} (exit ${status}, ${declared} declared)`);
      continue;
    }

    ran++;
    observedTotal += observed;
    declaredTotal += declared;
    if (observed < declared) {
      fault(
        suite,
        `${suite}: ran ${observed} check(s) but its source declares ${declared} — it stopped early ` +
          `or a check was never reached.`
      );
    }
    if (ALLOWED_SKIPS.has(suite)) {
      // Not a failure. An allowance is permission to skip, not an instruction to — and whether
      // realbytes.js can run depends on whether the machine has cargo, which is a fact about
      // the machine. Turning a coverage GAIN red would be its own kind of wrong answer.
      notes.push(`--allow-skip=${suite} was passed and was not needed: ${suite} ran here.`);
    }
    // SHORT, not ok: the suite's own checks all passed and it exited 0, and it still did less
    // than it says it does. That is the case worth a word of its own — it is the one that reads
    // as green everywhere else.
    const isRed = failedChecks || status !== 0;
    // QUAR, not FAIL: the suite IS red and the reader must see that it is red. The word only
    // says who the redness stops — the build, or nobody.
    const verdict = isRed
      ? quarantined.has(suite)
        ? "QUAR "
        : "FAIL "
      : observed < declared
        ? "SHORT"
        : "ok   ";
    if (!isRed && observed >= declared && quarantined.has(suite)) {
      // THE LIST CANNOT ROT SILENTLY. A quarantine that is no longer needed is a suite whose
      // failures nobody would see if it went red again, so the day it goes green is the day
      // the row must come out — and this is where that gets said, every run, in the caller's
      // own output.
      notes.push(
        `--quarantine=${suite} was passed and is NO LONGER NEEDED: ${suite} passed here. ` +
          `Delete the row — a stale quarantine hides the next real failure in this suite.`
      );
    }
    const where = entry.shard ? `  [shard ${entry.shard}]` : "";
    const took = entry.seconds ? `  ${entry.seconds.toFixed(0)}s` : "";
    console.log(
      `  ${verdict}           ${suite} — ${observed} check(s) run, ${declared} declared, ` +
        `${failedChecks} failed (exit ${status})${took}${where}`
    );
    // WHICH CHECK, AND WHY, read off the receipt (`lib/harness.js` report()). The coverage job
    // prints this from what the shards wrote down, so the verdict names the check even when the
    // shard's own FAIL line is somewhere else in a long log.
    for (const f of runs.flatMap((r) => (Array.isArray(r.failures) ? r.failures : []))) {
      console.log(`                    ✗ ${f.check}: ${String(f.message || "").split("\n")[0]}`);
    }
  }

  // FAILED BY EITHER WITNESS. The exit code is one; the ledger is the other, and where they
  // disagree the ledger is the one that saw a check go red.
  const redSuites = planned.filter((s) => {
    const entry = data.get(s);
    if (!entry) return false; // already a problem above; not also a "failed suite"
    const recs = entry.records.filter((r) => typeof r.checks === "number");
    return entry.exit !== 0 || recs.reduce((a, r) => a + r.failed, 0) > 0;
  });
  // The split the verdict turns on: red suites that hold the run, and red suites the caller
  // has declared it already knows about. Both are printed; only the first is fatal.
  const failedSuites = redSuites.filter((s) => !quarantined.has(s));
  const quarantinedRed = redSuites.filter((s) => quarantined.has(s));
  for (const s of redSuites) {
    if (data.get(s).exit === 0) {
      fault(
        s,
        `${s}: reported failed check(s) and still exited 0 — its own failures did not reach its ` +
          `exit code. The run is failed on the LEDGER; fix the suite to exit on \`run.report()\`.`
      );
    }
  }

  if (observedTotal === 0) {
    problems.push("zero checks were run across the whole tree — refusing to report green over nothing");
  }

  console.log(
    `\n  ${planned.length} planned, ${ran} ran, ${skipped} skipped, ` +
      `${observedTotal} checks observed against ${declaredTotal} declared`
  );

  for (const n of notes) console.log("  NOTE  " + n);

  // QUARANTINE IS REPORTED BEFORE THE VERDICT, NEVER FOLDED INTO IT. A reader who skims to
  // the last line must still be told, in this run's own output, which suites are red and
  // excused — otherwise "all green" is doing exactly the work this file was written to stop.
  if (quarantinedRed.length || excused.length) {
    console.log(
      `\n== ${quarantinedRed.length} QUARANTINED suite(s): red, reported, not holding this run ==`
    );
    for (const s of quarantinedRed) console.log("  ! " + s);
    for (const ex of excused) console.log("  ! " + ex);
    console.log(
      "  These are excused because the caller named them, not because they passed. " +
        "A quarantine is a debt with a date on it, not a verdict."
    );
  }

  if (problems.length) {
    console.log("\n== the evidence gate REFUSES this run ==");
    for (const pr of problems) console.log("  ✗ " + pr);
  }

  if (failedSuites.length) {
    console.log(`\n${failedSuites.length} suite(s) FAILED: ${failedSuites.join(", ")}`);
  }

  return {
    ok: !failedSuites.length && !problems.length,
    ran,
    observedTotal,
    failedSuites,
    problems,
    quarantinedRed,
  };
}

// ---------------------------------------------------------------------------------------
// Coverage — the job that turns N green ticks into one statement about a set
// ---------------------------------------------------------------------------------------
//
// THE CLAIM THIS EXISTS TO MAKE. Twelve green shard jobs say twelve things. They do not say
// "every suite in this directory ran, at this commit, and here is what each one checked" —
// and that sentence is the whole reason `ui-suite-ci.yml` was written. Its header's promise,
// "HOW THIS CANNOT REPORT A GREEN RUN IT DID NOT EARN", was a whole-run fact while the run
// was one process. Split across a matrix it becomes N partial facts plus a job that has to
// add them up, and the header said so in advance: "A matrix that reports six greens and no
// total is a weaker gate than one serial red." This is that job.
//
// It refuses unless, taking the same four conditions the engine's own coverage job uses:
//   - the union of receipted suites EQUALS the discovered inventory — a missing one is named,
//     an unplanned one is named;
//   - every receipt carries the SAME commit, and it is this checkout's commit, so a union
//     stitched across two trees certifies neither;
//   - no suite is receipted twice;
//   - and the evidence gate passes over the aggregate, which is the same gate the serial run
//     has always applied.

/// The three refusals that are NOT about any individual suite: a suite receipted twice, a
/// receipt for something this checkout does not have, and receipts that disagree about which
/// tree they were taken from. Pure, so the self-test below can prove each one fires without
/// spawning a runner or fabricating a directory.
function reconcile(state) {
  const { duplicates, unplanned, commits, here } = state;
  const refusals = [];
  if (duplicates.length) {
    refusals.push(`a suite is receipted more than once: ${duplicates.join(", ")}`);
  }
  if (unplanned.length) {
    refusals.push(
      `receipted but not in this checkout's inventory: ${unplanned.join(", ")} — the shards ran ` +
        `a tree this job is not looking at`
    );
  }
  if (commits.size > 1) {
    refusals.push(
      `receipts disagree about the commit (${[...commits.entries()].map(([c, n]) => `${c.slice(0, 12)} x${n}`).join(", ")}) — ` +
        `a union across two trees certifies neither`
    );
  } else if (here && [...commits.keys()][0] !== here) {
    refusals.push(
      `the shards ran ${[...commits.keys()][0].slice(0, 12)} and this job is at ${here.slice(0, 12)} — ` +
        `the verdict would be about a tree nobody is looking at`
    );
  }
  return refusals;
}

// THE SELF-TEST, on every invocation, for the same reason the scanner above has one: a
// refusal nobody has ever seen fire is a refusal nobody knows is wired up. Each case below
// is one of the ways a sharded run can certify a tree it did not verify, and a green run of
// this file is a run in which all four were proven able to fail — plus the case that matters
// most, the ordinary one, proven NOT to.
(function selfTestTheReconciliation() {
  const ok = (name, got, want) => {
    if (got !== want) {
      console.error(`the coverage reconciliation is broken: ${name} expected ${want} refusal(s), got ${got}`);
      process.exit(2);
    }
  };
  const clean = new Map([["a".repeat(40), 3]]);
  ok("a clean set refuses nothing",
    reconcile({ duplicates: [], unplanned: [], commits: clean, here: "a".repeat(40) }).length, 0);
  ok("a duplicated suite is refused",
    reconcile({ duplicates: ["x.js (shards 1/6 and 2/6)"], unplanned: [], commits: clean, here: "a".repeat(40) }).length, 1);
  ok("a receipt outside the inventory is refused",
    reconcile({ duplicates: [], unplanned: ["ghost.js"], commits: clean, here: "a".repeat(40) }).length, 1);
  ok("receipts from two trees are refused",
    reconcile({ duplicates: [], unplanned: [], commits: new Map([["a".repeat(40), 2], ["b".repeat(40), 1]]), here: "a".repeat(40) }).length, 1);
  ok("receipts from a tree this job is not at are refused",
    reconcile({ duplicates: [], unplanned: [], commits: clean, here: "b".repeat(40) }).length, 1);
  // And no `here` — a checkout with no git and no GITHUB_SHA — must not invent a refusal it
  // cannot substantiate.
  ok("an unknowable commit refuses nothing on that ground",
    reconcile({ duplicates: [], unplanned: [], commits: clean, here: "" }).length, 0);
})();

/// Every `*.receipt.json` under `dir`, sorted. Shared by `--coverage` and `--weigh` so the
/// two never disagree about what a receipt directory contains.
function receiptFiles(dir, flag) {
  if (!fs.existsSync(dir) || !fs.statSync(dir).isDirectory()) {
    console.error(`${flag}=${dir} is not a directory. No receipts means nothing was proven.`);
    process.exit(1);
  }
  // Receipts arrive as one directory per shard artifact, or flat. Both are walked, because
  // how the CI provider chooses to lay out downloaded artifacts is not a thing this file
  // should have an opinion about.
  const files = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const full = path.join(d, e.name);
      if (e.isDirectory()) walk(full);
      else if (e.name.endsWith(".receipt.json")) files.push(full);
    }
  };
  walk(dir);

  // NO RECEIPTS AT ALL IS A REFUSAL, never "0 of 0 passed". This is the empty-inventory green
  // run in its newest costume: an artifact step that silently uploaded nothing.
  if (files.length === 0) {
    console.error(
      `no receipts under ${dir} — refusing to certify a plan against nothing. ` +
        `Every shard writes one file per suite; finding none means no shard reported.`
    );
    process.exit(1);
  }
  return files.sort();
}

// ---------------------------------------------------------------------------------------
// --weigh — the weights are MEASURED on the machine that runs them, never estimated
// ---------------------------------------------------------------------------------------
//
// WHY THIS COMMAND EXISTS. `suite-weights.tsv` is the only input to packing, and on
// 2026-09-20 it named 32 of 55 suites. `pack()` weighs an unweighted suite as the heaviest
// one known — a deliberately safe default — so 25 suites (23 absent, plus `dialect.js` and
// `docs-claims.js` whose measured `0` the loader was discarding) were each packed as 305 s.
// That is 7625 s of load that does not exist, against 2155 s that does, so the packer was
// balancing four shards of mostly fiction and the balance it reported was an artifact:
// every shard showed 2446 s while the REAL measured load was 311 / 311 / 920 / 613 s. One
// shard carried three times another, the run took as long as its longest shard, and nobody
// could see it because the plan said the shards were even.
//
// A weights file maintained by hand drifts the moment a suite is added, which is exactly
// what happened. This command regenerates it from a real run's receipts — the same receipts
// the coverage job reconciles — so the fix is re-derivable in one command instead of being
// a number somebody remembered to update.
//
// IT REFUSES A PARTIAL SET. Writing weights for the suites that happen to have receipts
// would silently DELETE the weight of every suite that did not, turning each of them back
// into a 305 s phantom. The whole inventory, or nothing.
function runWeigh(dir) {
  const files = receiptFiles(dir, "--weigh");
  const seconds = new Map();
  for (const f of files) {
    let r;
    try {
      r = JSON.parse(fs.readFileSync(f, "utf8"));
    } catch (e) {
      console.error(`unreadable receipt ${f}: ${e.message}`);
      process.exit(1);
    }
    if (!r.suite || typeof r.seconds !== "number") {
      console.error(`receipt ${f} carries no suite/seconds pair to weigh`);
      process.exit(1);
    }
    seconds.set(r.suite, r.seconds);
  }
  const missing = SUITES.filter((s) => !seconds.has(s));
  if (missing.length) {
    console.error(
      `refusing to rewrite ${path.basename(WEIGHTS_FILE)} from a partial run: no receipt for ` +
        `${missing.join(", ")}. Weighing only what ran would delete these suites' weights and ` +
        `pack each of them as the heaviest suite known, which is the defect this command exists ` +
        `to end. Run every shard, then weigh.`
    );
    process.exit(1);
  }
  const stamp = new Date().toISOString().slice(0, 10);
  const total = SUITES.reduce((a, s) => a + seconds.get(s), 0);
  const lines = [
    `# Measured wall clock per suite, in seconds, on the machine that runs them.`,
    `# REGENERATED BY \`node run.js --weigh=<receipts>\` — do not hand-edit: a weight typed`,
    `# here drifts the moment a suite changes, and a MISSING row is not neutral. \`pack()\``,
    `# weighs an unknown suite as the heaviest one known, so an absent row costs wall clock`,
    `# on every run until somebody measures it.`,
    `#`,
    `# A weight is never load-bearing for correctness: it decides which shard a suite lands`,
    `# in and nothing else. Coverage is proven from receipts.`,
    `#`,
    `# ${SUITES.length} suite(s), ${total.toFixed(0)} s serial, measured ${stamp}.`,
    `# A suite under a second is written as 0 and that is a measurement, not a missing row.`,
  ];
  for (const s of SUITES) lines.push(`${s}\t${Math.round(seconds.get(s))}`);
  fs.writeFileSync(WEIGHTS_FILE, lines.join("\n") + "\n");
  console.log(
    `${path.basename(WEIGHTS_FILE)} rewritten from ${files.length} receipt(s): ` +
      `${SUITES.length} suite(s), ${total.toFixed(0)} s serial.`
  );
  const heaviest = SUITES.slice().sort((a, b) => seconds.get(b) - seconds.get(a)).slice(0, 5);
  console.log(`  heaviest: ${heaviest.map((s) => `${s} ${seconds.get(s).toFixed(0)}s`).join(", ")}`);
  console.log(`  the floor for ANY shard count is the heaviest single suite: ${seconds.get(heaviest[0]).toFixed(0)} s.`);
  process.exit(0);
}

function runCoverage(dir) {
  const files = receiptFiles(dir, "--coverage");

  const data = new Map();
  const duplicates = [];
  const commits = new Map();
  const unplanned = [];
  for (const f of files) {
    let r;
    try {
      r = JSON.parse(fs.readFileSync(f, "utf8"));
    } catch (e) {
      console.error(`unreadable receipt ${f}: ${e.message}`);
      process.exit(1);
    }
    if (!r.suite) {
      console.error(`receipt ${f} names no suite`);
      process.exit(1);
    }
    if (data.has(r.suite)) duplicates.push(`${r.suite} (shards ${data.get(r.suite).shard} and ${r.shard})`);
    if (!SUITES.includes(r.suite)) unplanned.push(`${r.suite} (shard ${r.shard})`);
    commits.set(r.commit || "(none)", (commits.get(r.commit || "(none)") || 0) + 1);
    data.set(r.suite, {
      exit: r.exit,
      records: r.records || [],
      seconds: r.seconds,
      shard: r.shard,
      commit: r.commit,
      stopped: r.stopped,
    });
  }

  const here = commitSha();
  const refusals = reconcile({ duplicates, unplanned, commits, here });

  const commit = [...commits.keys()][0];
  console.log(
    `${files.length} receipt(s) from ${new Set([...data.values()].map((d) => d.shard)).size} shard(s), ` +
      `against ${SUITES.length} discovered suite(s), at ${String(commit).slice(0, 12)}`
  );

  const result = gate(SUITES, data, {
    label: "reconciled across every shard",
    quarantined: QUARANTINED,
  });

  // A QUARANTINE NAMING A SUITE THAT IS NOT HERE is a row nobody will ever delete, because
  // nothing will ever report it as unneeded. Said as a note rather than a refusal: a renamed
  // suite must not stop a build, only be noticed.
  for (const q of QUARANTINED) {
    if (!SUITES.includes(q)) {
      console.log(
        `  NOTE  --quarantine=${q} names no suite in this checkout — it was renamed or deleted. ` +
          `Delete the row; it excuses nothing and will never report itself stale.`
      );
    }
  }

  if (refusals.length) {
    console.log("\n== coverage REFUSES this run ==");
    for (const r of refusals) console.log("  ✗ " + r);
  }

  if (!result.ok || refusals.length) process.exit(1);

  // THE PROOF IS WRITTEN ONLY ON A PASS, AND IT CARRIES THE COMMIT. A later gate is allowed
  // to skip this whole suite when it holds one of these for the exact sha it fetched — the
  // same shape `--checks-done-at-land` and `--gui-proof` already use in nightly-local.py,
  // and for the same reason: a proof that cannot name the tree it was taken against proves
  // nothing about any tree. It also records what was EXCUSED, so a build cannot inherit a
  // quarantine silently from a run it did not watch.
  if (PROOF_OUT) {
    fs.mkdirSync(path.dirname(path.resolve(PROOF_OUT)), { recursive: true });
    fs.writeFileSync(
      PROOF_OUT,
      JSON.stringify(
        {
          proof: "ui-suite-coverage",
          commit,
          at: new Date().toISOString(),
          suites: SUITES.length,
          ran: result.ran,
          checks: result.observedTotal,
          quarantined: [...QUARANTINED],
          quarantined_red: result.quarantinedRed,
        },
        null,
        2
      ) + "\n"
    );
    console.log(`  coverage proof written to ${PROOF_OUT} (commit ${String(commit).slice(0, 12)})`);
  }

  const wall = [...data.values()].reduce((a, d) => a + (d.seconds || 0), 0);
  // "ALL GREEN" HAS TO STOP BEING TRUE THE MOMENT IT IS NOT. A quarantined suite is red and
  // excused, and a summary line that still said "all green" would be this file's own thesis
  // failing in the last sentence it prints — the place a reader is most likely to stop.
  const greenness = result.quarantinedRed.length
    ? `all green except ${result.quarantinedRed.length} QUARANTINED (${result.quarantinedRed.join(", ")})`
    : "all green";
  console.log(
    `\n✓ ui-suite: ${result.ran}/${SUITES.length} discovered suite(s) ran, ${greenness}, all at ${commit}.\n` +
      `  ${result.observedTotal} checks observed. serial cost ${wall.toFixed(0)} s across ` +
      `${new Set([...data.values()].map((d) => d.shard)).size} shard(s).\n` +
      `  ${SUITES.join(", ")}`
  );
  process.exit(0);
}

// ---------------------------------------------------------------------------------------
// This process's own verdict
// ---------------------------------------------------------------------------------------

const records = fs
  .readFileSync(ledgerFile, "utf8")
  .split("\n")
  .filter((l) => l.trim())
  .map((l) => JSON.parse(l));
fs.rmSync(ledgerDir, { recursive: true, force: true });

const bySuite = new Map(MINE.map((s) => [s, []]));
for (const rec of records) {
  if (!bySuite.has(rec.suite)) bySuite.set(rec.suite, []);
  bySuite.get(rec.suite).push(rec);
}

const data = new Map();
for (const suite of MINE) {
  data.set(suite, {
    exit: exited.get(suite),
    records: bySuite.get(suite) || [],
    seconds: seconds.get(suite),
    shard: SHARD ? `${SHARD.index}/${SHARD.total}` : SUITE_ONLY ? "alone" : null,
    stopped: stopped.get(suite),
  });
}

// THE RECEIPT IS WRITTEN BEFORE THE VERDICT, and for a red suite as well as a green one.
// A shard that fails and writes nothing down leaves the coverage job unable to tell "this
// suite failed" from "this suite never ran", and those two deserve different sentences.
if (RECEIPTS) {
  fs.mkdirSync(RECEIPTS, { recursive: true });
  const commit = commitSha();
  for (const suite of MINE) {
    const d = data.get(suite);
    fs.writeFileSync(
      path.join(RECEIPTS, suite.replace(/\.js$/, "") + ".receipt.json"),
      JSON.stringify(
        {
          suite,
          commit,
          shard: d.shard,
          exit: d.exit,
          seconds: Number((d.seconds || 0).toFixed(1)),
          declared: DECLARED.get(suite),
          records: d.records,
          ...(d.stopped ? { stopped: d.stopped } : {}),
        },
        null,
        2
      ) + "\n"
    );
  }
  console.log(`\n${MINE.length} receipt(s) written to ${RECEIPTS}`);
}

const result = gate(MINE, data, {
  label: SHARD ? `shard ${SHARD.index}/${SHARD.total}` : SUITE_ONLY ? `${SUITE_ONLY} alone` : "",
});

if (!result.ok) process.exit(1);

// A SHARD DOES NOT GET TO SAY THE RUN PASSED. It ran a subset and it says so; the sentence
// about the whole directory belongs to the coverage job, which is the only thing that has
// seen every receipt.
const skipTail = ALLOWED_SKIPS.size ? `, skips allowed for (${[...ALLOWED_SKIPS].join(", ")})` : "";
if (SUITE_ONLY) {
  console.log(`\n${SUITE_ONLY} passed alone — ${result.observedTotal} checks. Whether the DIRECTORY passed is the coverage job's sentence.`);
} else if (SHARD) {
  console.log(
    `\nshard ${SHARD.index}/${SHARD.total} passed — ${result.observedTotal} checks over ` +
      `${result.ran} of this tree's ${SUITES.length} suites${skipTail}. ` +
      `Whether the DIRECTORY passed is the coverage job's sentence, not this one's.`
  );
} else {
  console.log(`\nall ${result.ran} suites passed — ${result.observedTotal} checks over ${result.ran} suites${skipTail}`);
}
process.exit(0);
