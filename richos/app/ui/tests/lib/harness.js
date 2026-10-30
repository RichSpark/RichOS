// The browser acceptance harness for `app/ui` — a HOME for the tests every UI slice was
// otherwise re-inventing and throwing away.
//
// THREE RULES, each one a thing an earlier slice got wrong:
//
//  1. THE REAL RENDERER, NEVER A COPY. Every page below loads `../timeline.js` and
//     `../style.css` from disk by relative path. A test that re-implements a rule proves
//     the test, not the product.
//  2. WEBKIT, NOT CHROMIUM. Tauri renders through WKWebView on macOS (§3.3 of the
//     architecture doc names the webview-consistency tradeoff explicitly). A green
//     Chromium run says nothing about what the CEO sees.
//  3. NO FAKED SCREENSHOTS. `screencapture` on this machine has returned an all-black
//     1920x1080 PNG for three slices running (display locked). Screenshots here come out
//     of WebKit's own compositor, which does not depend on a display server — and every
//     one is checked for non-uniform pixels before it counts as evidence.
//
// Playwright is a devDependency (`npm install` in this directory). It is resolved leniently
// so the harness also runs from an existing install on the machine via NODE_PATH or
// RICHOS_PLAYWRIGHT — installing a browser engine into every worktree is not free.
//
// AND SINCE 2026-09-20 A WORKTREE NEEDS NOTHING AT ALL: when neither of those resolves,
// `loadPlaywright` DERIVES the main checkout's install from git and uses it. One install per
// machine, no environment variable to remember, no `node_modules` copied by hand. See
// `mainCheckoutPlaywright` below for why git can answer this and a spawner cannot.
//
// `npm install` HAS TO INSTALL THE ENGINE TOO, and for a while it did not. Playwright 1.61
// ships no `postinstall` script (its installed package.json has no `scripts` key at all), so
// `npm install` here produced the JS API and zero browsers, and the only thing that made the
// suites run was a webkit binary some OTHER project had already put in the shared
// ~/Library/Caches/ms-playwright. That is why six consecutive runs of this directory needed
// RICHOS_PLAYWRIGHT pointed into an unrelated repository: not a preference, a missing step.
// `package.json` now carries `postinstall: playwright install webkit`, which is what makes
// the README's one-command setup true. RICHOS_PLAYWRIGHT stays as what it was meant to be —
// the deliberate opt-out for someone who does not want the download — and is no longer the
// only way in.

"use strict";

const path = require("path");
const fs = require("fs");
const png = require("./png");
const stability = require("./shot-stability");
// The checkout as this suite found it, compared at `report()` (`lib/tracked-tree.js`). Taken
// here, at the harness's first require, which is before any suite code has run.
const TREE = require("./tracked-tree").watch((abs) => stability.regenerating(abs));
// On a navigation that fails, write down what the page was doing (`lib/navigation-evidence.js`).
const navigation = require("./navigation-evidence");
// A suite prints its PASS/FAIL report in one burst and then exits; on a pipe, the FAIL lines
// must not be the part that is lost (`lib/blocking-stdio.js`).
require("./blocking-stdio")();

const UI_DIR = path.resolve(__dirname, "..", "..");
const SHOT_DIR = path.resolve(__dirname, "..", ".shots");

/// The MAIN CHECKOUT's install of this same directory, DERIVED rather than configured.
///
/// A linked git worktree shares one `.git` with the checkout it came from, so
/// `--git-common-dir` names that checkout from anywhere inside any worktree, and the parent
/// of it is the main checkout's root. The path below is then this directory's OWN position
/// inside its own checkout, re-rooted there — nothing about the repository's layout is
/// typed here, so the answer survives the tree being rearranged.
///
/// This is why a worktree does not need its own 17 MB copy of `node_modules`. Until
/// 2026-09-20 every engineer who ran a UI suite in a fresh worktree either ran `npm install`
/// again or made a symlink by hand, and a step done by hand more than twice is a step that
/// gets done differently each time. The browser ENGINE was never the cost: Playwright keeps
/// WebKit in `~/Library/Caches/ms-playwright`, which is already shared by every checkout on
/// the machine.
function mainCheckoutPlaywright() {
  const git = (args) => {
    try {
      return require("child_process")
        .execFileSync("git", args, { cwd: __dirname, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] })
        .trim();
    } catch (_e) {
      return "";
    }
  };
  const commonDir = git(["rev-parse", "--path-format=absolute", "--git-common-dir"]);
  const myRoot = git(["rev-parse", "--path-format=absolute", "--show-toplevel"]);
  if (!commonDir || !myRoot) return null;
  const mainRoot = path.dirname(commonDir);
  if (path.resolve(mainRoot) === path.resolve(myRoot)) return null; // already there
  const here = path.relative(myRoot, path.resolve(__dirname, ".."));
  const candidate = path.join(mainRoot, here, "node_modules", "playwright");
  return fs.existsSync(candidate) ? candidate : null;
}

function loadPlaywright() {
  const candidates = [];
  // An explicit path always wins — the same rule `app/scripts/lib/worktree-resource.sh`
  // states for every shared resource: a human who names one has a reason.
  if (process.env.RICHOS_PLAYWRIGHT) candidates.push(process.env.RICHOS_PLAYWRIGHT);
  candidates.push("playwright");
  const derived = mainCheckoutPlaywright();
  if (derived) candidates.push(derived);
  for (const c of candidates) {
    let pw;
    try {
      pw = require(c);
    } catch (_e) {
      continue; /* keep looking */
    }
    // EVERY SUITE'S BROWSER COMES THROUGH HERE, so this is where a failed navigation gets its
    // evidence bundle — once, rather than at eighty `page.goto` call sites. Timeouts and the
    // error a suite sees are untouched; see the header of `navigation-evidence.js`.
    // And no browser outlives its suite (`lib/browser-reaper.js`): a SIGKILLed suite used to leave
    // Playwright.app behind to crash later, with macOS's "quit unexpectedly" dialog on screen.
    // And every launch is quiet (`lib/quiet-browser.js`, outermost so a refusal starts nothing):
    // headless, notifications denied, and Chromium on the headless shell, which macOS never
    // registers with Notification Center (the 2026-10-29 "Google Chrome for Testing" prompts).
    return require("./quiet-browser").install(
      require("./browser-reaper").install(navigation.instrumentPlaywright(pw))
    );
  }
  // NAME THE FIX, not the symptom. The message this replaced offered three options and
  // left the reader to work out which one applied to the directory they were standing in.
  const mainRoot = path.dirname(
    (() => {
      try {
        return require("child_process")
          .execFileSync("git", ["rev-parse", "--path-format=absolute", "--git-common-dir"], {
            cwd: __dirname,
            encoding: "utf8",
            stdio: ["ignore", "pipe", "ignore"],
          })
          .trim();
      } catch (_e) {
        return "";
      }
    })()
  );
  throw new Error(
    "playwright not found, and neither is the main checkout's copy.\n" +
      "  Looked for: RICHOS_PLAYWRIGHT (" +
      (process.env.RICHOS_PLAYWRIGHT || "unset") +
      "), ./node_modules/playwright, and the main checkout's own install.\n" +
      "  FIX: run `npm install` once in the MAIN checkout's ui/tests — " +
      (mainRoot && mainRoot !== "." ? mainRoot + "/richos/app/ui/tests" : "<main checkout>/richos/app/ui/tests") +
      " — and every worktree on this machine will use it.\n" +
      "  Do NOT copy or symlink node_modules into this worktree by hand; that is the step this resolution removed."
  );
}

// ---------------------------------------------------------------------------------------
// The fixture page
// ---------------------------------------------------------------------------------------

/// A page that is the shipping renderer and nothing else: the real stylesheet, the real
/// `timeline.js`, and a `#messages` container shaped exactly as `index.html` shapes it.
///
/// `window.__render(snapshot, opts)` applies a `get_timeline` snapshot through
/// `RichTimeline.applySnapshot` — the SAME reload path `main.js` uses — and renders it. It
/// returns the turn projection so a test can assert on the model and the DOM together.
function fixtureHtml() {
  return `<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<link rel="stylesheet" href="file://${UI_DIR}/style.css">
</head><body>
  <div id="app"><main id="stage">
    <div id="conversation" role="log" aria-live="off" tabindex="0"><div id="messages"></div></div>
  </main></div>
  <script src="file://${UI_DIR}/timeline.js"></script>
  <script>
    window.__events = [];
    window.__model = window.RichTimeline.createModel();
    window.__opts = {};
    window.__renderOnly = function () {
      const opts = Object.assign({
        now: 1787950000000,
        expandedMessages: new Set(),
        avatarAlreadyShown: true,
        // THE SHIPPING RULE, NOT A COPY OF IT (harness rule 1). Section 6.4 has two
        // defaults - expanded while the turn is active, collapsed after it settles - and
        // the CEO's own choice overrules both. All three live in
        // RichTimeline.isTurnExpanded / toggleTurn, which is what main.js calls; a
        // re-implementation here would prove the harness rather than the product.
        // (No backticks in this comment: it sits inside the fixture page's template
        // literal, where one would end the string.)
        isExpanded: function (id) { return window.RichTimeline.isTurnExpanded(window.__model, id); },
        toggle: function (id) {
          window.RichTimeline.toggleTurn(window.__model, id);
          window.__renderOnly();
        },
        rerender: function () { window.__renderOnly(); },
        copy: function () {},
        retry: function () {},
        openWorker: function (w) { window.__events.push({ type: "openWorker", agentId: w.agentId }); },
      }, window.__opts);
      return window.RichTimeline.render(window.__model, document.getElementById("messages"), opts);
    };
    window.__render = function (snapshot, opts) {
      window.__opts = opts || {};
      window.__model = window.RichTimeline.createModel();
      window.RichTimeline.bind(window.__model, snapshot.entityId, snapshot.threadId, snapshot.bindingRevision);
      window.RichTimeline.applySnapshot(window.__model, snapshot);
      return window.__renderOnly();
    };
  </script>
</body></html>`;
}

async function openFixture(browser, viewport) {
  const page = await browser.newPage({ viewport: viewport || { width: 1280, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  // A `file://` base URL so the two relative script/stylesheet loads resolve.
  await page.goto("file://" + path.join(UI_DIR, "index.html"));
  await page.setContent(fixtureHtml(), { waitUntil: "load" });
  await page.waitForFunction("typeof window.RichTimeline === 'object'");
  page.__errors = errors;
  return page;
}

// ---------------------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------------------

/// Take a screenshot AND prove it is a real render. An all-black or all-white PNG is what a
/// locked display produces; a genuine WebKit paint has hundreds of distinct pixel values.
/// A shot that fails this check is reported as evidence of NOTHING, never counted as a pass.
///
/// THIS CHECK WAS DOCUMENTED HERE BEFORE IT WAS DONE. Until this commit the function
/// measured `fs.statSync(...).size` and nothing else — and a byte count is exactly what an
/// all-black PNG passes: `screencapture` on this machine has been returning a valid,
/// several-kilobyte, single-color (0,0,0) 1920x1080 file for three slices running. A file
/// size is not a render. So the pixels are now counted, in the browser that just painted
/// them: the PNG is handed back to WebKit, decoded, drawn to a canvas and sampled. No new
/// dependency, and the decoder is the same engine the CEO's app renders through.
///
/// `distinct` is the number of unique RGB values over a bounded grid sample. A locked
/// display returns 1. A real UI returns dozens to hundreds.
const SHOT_MIN_DISTINCT = 8;

// ---------------------------------------------------------------------------------------
// PUBLISHING A SHOT — the one place a PNG is written
// ---------------------------------------------------------------------------------------
//
// THE PROBLEM THIS SOLVES, MEASURED. On 2026-09-05, against `5e00651`, one `node run.js` left
// 96 of the 96 committed PNGs under `../shots-*/` modified, with byte deltas from -12 to
// +679,604. Seventy of them held a picture that differed in THIRTY-THREE pixels out of 1.33
// million — the antialiased left edge of the wordmark — and a handful held a genuinely
// different screen. `git status` was therefore dirty after every run; `git checkout --
// app/ui/tests/` became a habit; and a real visual regression would have arrived as one more
// modified PNG in a list of ninety-six, which is camouflage, not evidence.
//
// The rule is now: A SHOT IS WRITTEN ONLY WHEN THE PICTURE CHANGED. Not when the file bytes
// changed — two PNGs can hold the same picture and differ, and `git diff` cannot tell that
// from a regression. Both sides are decoded (`./png.js`) and every sample compared.
//
// AND THE DOUBT ALWAYS WRITES. If either side cannot be decoded, or is a PNG shape that
// decoder will not guess at, `samePicture` is false and the file is written. One redundant
// write costs a line in `git status`; a wrongly-skipped write costs a regression nobody sees.
//
// When a committed shot differs, the change is announced with the pixel count — so the
// difference arrives already explained, rather than as a binary blob.
//
// AND A COMMITTED SHOT IS NEVER WRITTEN BY A TEST RUN. Since 2026-09-29 (`lib/tracked-tree.js`
// carries the incident): the merge gate verifies a candidate IN THE MAIN CHECKOUT, and this
// function used to write a changed picture straight over its committed reference — so any
// change that legitimately moved a picture had its own verification refused as "source changed
// during verification". A test that rewrites the tree it is verifying is not evidence; it is a
// second, unreviewed commit. So:
//
//   * `.shots/` (gitignored per-run scratch) is written as always;
//   * a committed reference is written ONLY when it is being regenerated on purpose —
//     `RICHOS_SHOTS_REGENERATE=<file|dir|all>`, the same switch `lib/shot-stability.js` names —
//     which a person runs in their own worktree and then commits, with the reason;
//   * otherwise the picture this run took is kept at `.shots/changed/<same path>` and the line
//     says so, with the exact command that accepts it.
//
// AND A CHANGED PICTURE FAILS THE SUITE THAT TOOK IT (part-2 hunt section 25, 2026-09-29). The
// rule above keeps the reference from being overwritten; it used to stop there, so a screen
// that had visibly changed printed one log line and the suite went green: the comparison
// existed and could not make anything fail. Now `publishShot` remembers every picture that
// differs from its reference (or has none yet, outside a declared bound), and the suite's own
// `report()` carries one housekeeping check naming each of them. The reference is still never
// rewritten by the failing run: a person accepts the change with `RICHOS_SHOTS_REGENERATE`
// and commits it with the reason, and the next run is green.
//
// AND THE FAILING PICTURE OUTLIVES THE CHECKOUT THAT TOOK IT (verification reliability PRD,
// T07, 2026-10-20). `.shots/changed/` is the checkout's own scratch, and a merge gate that
// verifies in a temporary checkout deletes it with the checkout: on 2026-10-19 the corrections
// picture failed at the gate under load and the picture that would have explained it was gone,
// so the failure was filed as "load". Now every picture that differs from its reference (and
// every one a regeneration writes) leaves the same set of files, `keepShotEvidence` below:
//
//     actual.png      what this run drew
//     reference.png   the committed reference it was compared with (absent when there was none)
//     diff.png        the reference, dimmed, with every differing pixel in magenta
//     state.json      the measured difference and its bounding box, the declared bound if any,
//                     and the page's state at the shutter (`pageState`): every scrolled or
//                     scrollable box and its offsets, the focused element, the viewport, the
//                     lighting, running animations
//
// in `.shots/evidence/<key>/` (`.shots/regenerated/<key>/` for a regeneration) AND, when the
// runner names one, in this check's own results folder in the run's evidence
// (`RICHOS_TEST_RESULTS_ROOT`, set by `proof-run.py` for every check it launches), which no
// checkout cleanup reaches. Writing it can never change a verdict: a write that fails is said on
// the line and the verdict stands.
//
// AND A PICTURE A RUN DREW IS NEVER APPROVED BY THAT RUN (PRD R7: "generating a picture does not
// approve it"). Under `RICHOS_SHOTS_REGENERATE` the run used to pass, so the run that wrote a
// reference was also the run that compared against it. Now `report()` fails a housekeeping check
// naming every picture this run wrote: the approval is a person's look at the kept evidence,
// recorded with the commit, and the evidence that the picture is stable is the NEXT run, without
// the switch, matching it.
const CHANGED_DIR = path.join(SHOT_DIR, "changed");
const EVIDENCE_DIR = path.join(SHOT_DIR, "evidence");
const REGENERATED_DIR = path.join(SHOT_DIR, "regenerated");
const changedShots = [];
const regeneratedShots = [];

/// The page's state at the shutter, keyed by the picture's bytes so the publish that follows
/// (`publishShot` or `publishShotFile`, possibly a moment later) can find it. Bounded: a suite
/// that photographs more than this without publishing is a suite that is not comparing.
const shotStates = new Map();
const SHOT_STATES_MAX = 64;
const bytesKey = (buf) => require("crypto").createHash("sha256").update(buf).digest("hex");

function rememberShotState(buf, state) {
  shotStates.set(bytesKey(buf), state);
  while (shotStates.size > SHOT_STATES_MAX) shotStates.delete(shotStates.keys().next().value);
}

/// What the page looked like to the DOM at the instant a picture was taken. Read-only, and
/// bounded: the scrollers are what decided the two capture-state failures on record (the splash
/// Settings panel 200px short of its bottom, the corrections desk at 0 instead of 101), so each
/// one is listed with its offsets; everything else is one line.
async function pageState(page) {
  try {
    return await page.evaluate(() => {
      const name = (n) => {
        if (!n || n.nodeType !== 1) return null;
        if (n.id) return "#" + n.id;
        const cls = typeof n.className === "string" ? n.className.trim().split(/\s+/).filter(Boolean).slice(0, 3) : [];
        return n.tagName.toLowerCase() + (cls.length ? "." + cls.join(".") : "");
      };
      const scrollers = [];
      for (const n of document.querySelectorAll("*")) {
        const moved = n.scrollTop || n.scrollLeft;
        const room = n.scrollHeight > n.clientHeight + 1 || n.scrollWidth > n.clientWidth + 1;
        if (!moved && !room) continue;
        if (!moved) {
          const cs = getComputedStyle(n);
          if (!/(auto|scroll)/.test(cs.overflowY + " " + cs.overflowX)) continue;
        }
        if (!n.getClientRects().length) continue;
        scrollers.push({
          element: name(n),
          scrollTop: Math.round(n.scrollTop),
          scrollLeft: Math.round(n.scrollLeft),
          scrollHeight: n.scrollHeight,
          clientHeight: n.clientHeight,
        });
        if (scrollers.length >= 40) break;
      }
      const running = document.getAnimations ? document.getAnimations().filter((a) => a.playState === "running") : [];
      return {
        viewport: { width: window.innerWidth, height: window.innerHeight, devicePixelRatio: window.devicePixelRatio },
        page: { scrollX: Math.round(window.scrollX), scrollY: Math.round(window.scrollY), readyState: document.readyState },
        lighting: window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark OS" : "light OS",
        theme: document.documentElement.getAttribute("data-theme"),
        focused: name(document.activeElement),
        runningAnimations: running.length,
        scrollers,
      };
    });
  } catch (e) {
    return { unavailable: String((e && e.message) || e).split("\n")[0] };
  }
}

/// The reference, dimmed to grey, with every pixel that differs painted magenta: the picture a
/// person looks at to see WHERE two pictures differ. Null when the two cannot be compared (a
/// side that does not decode, or a change of size), which `state.json` then says.
function diffPicture(existing, fresh) {
  let a;
  let b;
  try {
    a = png.decode(existing);
    b = png.decode(fresh);
  } catch (_e) {
    return null;
  }
  if (a.width !== b.width || a.height !== b.height || a.channels !== b.channels) return null;
  const { width, height, channels } = a;
  const raw = Buffer.alloc(height * (width * 3 + 1));
  let box = null;
  let p = 0;
  for (let y = 0; y < height; y++) {
    raw[p++] = 0;
    for (let x = 0; x < width; x++) {
      const o = (y * width + x) * channels;
      let differs = false;
      for (let c = 0; c < channels; c++) if (a.data[o + c] !== b.data[o + c]) differs = true;
      if (differs) {
        raw[p++] = 255;
        raw[p++] = 0;
        raw[p++] = 255;
        box = box ? [Math.min(box[0], x), Math.min(box[1], y), Math.max(box[2], x), Math.max(box[3], y)] : [x, y, x, y];
      } else {
        const grey = channels >= 3 ? (a.data[o] + a.data[o + 1] + a.data[o + 2]) / 3 : a.data[o];
        const v = Math.round(24 + grey * 0.3);
        raw[p++] = v;
        raw[p++] = v;
        raw[p++] = v;
      }
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const zlib = require("zlib");
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body) >>> 0);
    return Buffer.concat([len, body, crc]);
  };
  const file = Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
  return { file, box: box ? { x: box[0], y: box[1], width: box[2] - box[0] + 1, height: box[3] - box[1] + 1 } : null };
}

let CRC_TABLE = null;
function crc32(buf) {
  if (!CRC_TABLE) {
    CRC_TABLE = new Int32Array(256);
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      CRC_TABLE[n] = c;
    }
  }
  let c = -1;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return c ^ -1;
}

/// Keep a picture's evidence (see the section comment). `kind` is "changed" or "regenerated".
/// Returns the directories written, and the reason for any that could not be.
function keepShotEvidence(kind, key, existing, buf, facts) {
  const dirs = [path.join(kind === "regenerated" ? REGENERATED_DIR : EVIDENCE_DIR, key)];
  if (process.env.RICHOS_TEST_RESULTS_ROOT) {
    dirs.push(path.join(process.env.RICHOS_TEST_RESULTS_ROOT, "shots", kind, key));
  }
  const diff = existing ? diffPicture(existing, buf) : null;
  const measured = existing ? png.measureDifference(existing, buf) : { ok: false, reason: "no committed reference yet" };
  const state = Object.assign(
    {
      picture: key,
      suite: path.basename(process.argv[1] || "unknown"),
      verdict: kind,
      difference: measured,
      differingBox: diff ? diff.box : null,
      diffPicture: diff ? "diff.png" : "not drawn: " + (existing ? measured.reason || "the two pictures cannot be compared" : "no reference"),
      pageAtShutter: shotStates.get(bytesKey(buf)) || { unavailable: "not taken through shot()/captureSettled" },
    },
    facts || {}
  );
  const written = [];
  const failed = [];
  for (const dir of dirs) {
    try {
      fs.rmSync(dir, { recursive: true, force: true });
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(path.join(dir, "actual.png"), buf);
      if (existing) fs.writeFileSync(path.join(dir, "reference.png"), existing);
      if (diff) fs.writeFileSync(path.join(dir, "diff.png"), diff.file);
      fs.writeFileSync(path.join(dir, "state.json"), JSON.stringify(state, null, 2) + "\n");
      written.push(dir);
    } catch (e) {
      failed.push(dir + " (" + String((e && e.message) || e).split("\n")[0] + ")");
    }
  }
  return { written, failed };
}

function evidenceWords(kept) {
  const here = path.resolve(__dirname, "..");
  const shown = kept.written.map((d) => (d.startsWith(here + path.sep) ? path.relative(here, d) : d));
  return (
    (shown.length ? "actual, reference, diff and page state kept at " + shown.join(" and ") : "NO evidence was kept") +
    (kept.failed.length ? " (could not write " + kept.failed.join(", ") + ")" : "")
  );
}

function publishShot(buf, file) {
  file = path.resolve(file);
  const scratch = file.startsWith(SHOT_DIR + path.sep);
  // A reference is a destination inside this tests directory that is not scratch. Anything
  // outside the tests directory is a directory somebody named explicitly, and is theirs.
  const key = scratch ? null : stability.relKey(file);
  let existing = null;
  try {
    existing = fs.readFileSync(file);
  } catch (_e) {
    /* no previous shot */
  }
  if (existing && png.samePicture(existing, buf)) {
    // The file keeps its own bytes, which can encode the same pixels differently: the page
    // state taken with THESE pixels goes with the bytes that stay, so a later publish of this
    // file (`publishShotFile`, reading those bytes) still finds it (mid-job review
    // rv-20261020T073921Z-cb26c0d0-d6f0).
    const state = shotStates.get(bytesKey(buf));
    if (state && !existing.equals(buf)) rememberShotState(existing, state);
    return { file, written: false, bytes: existing.length };
  }
  if (!key) {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, buf);
    return { file, written: true, bytes: buf.length };
  }
  const regenerating = stability.regenerating(file);
  const difference = existing ? png.describeDifference(existing, buf) : "no committed reference yet";
  // THE HANDFUL OF SURFACES THAT ARE NOT THE SAME PICTURE TWICE. `lib/shot-stability.js` is the
  // only place a file is allowed to differ without being announced as changed; it names one file
  // at a time, and every entry carries the cause, the measurement and the bound. A shot held
  // under a bound is announced exactly as loudly as one that changed.
  const rule = existing ? stability.ruleFor(file) : null;
  let outside = "";
  if (rule && !regenerating) {
    const verdict = stability.heldBy(rule, existing, buf);
    if (verdict.held) {
      console.log(
        "  shot held: " + key + " — " + rule.class + " — " + difference +
          " — inside its declared bound (" + verdict.bound + ", lib/shot-stability.js)"
      );
      return { file, written: false, bytes: existing.length, held: true };
    }
    outside = " OUTSIDE ITS DECLARED BOUND (`" + rule.class + "` at " + verdict.bound + " in lib/shot-stability.js)";
  }
  if (regenerating) {
    // A SWEEP DOES NOT KEEP A NOISE-ONLY REDRAW (`stability.regenerationWrites`): a directory or
    // `all` leaves a picture inside its declared bound as it is; naming the file rewrites it.
    const decision = stability.regenerationWrites(regenerating, rule, existing, buf);
    if (!decision.write) {
      console.log(
        "  shot held under regeneration: " + key + " — " + rule.class + " — " + difference +
          " — inside its declared bound (" + decision.bound + "), so a " + regenerating +
          " regeneration does not rewrite it; name the file to rewrite it"
      );
      return { file, written: false, bytes: existing.length, held: true };
    }
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, buf);
    const evidence = keepShotEvidence("regenerated", key, existing, buf, { regeneration: regenerating });
    console.log(
      "  shot regenerated: " + key + " — " + difference + " — NOT approved by this run: " + evidenceWords(evidence) +
        "; look at them, commit the picture with what the look found, and run the suite again without RICHOS_SHOTS_REGENERATE"
    );
    regeneratedShots.push({ key, difference, evidence });
    return { file, written: true, bytes: buf.length, regenerated: true };
  }
  const kept = path.join(CHANGED_DIR, key);
  fs.mkdirSync(path.dirname(kept), { recursive: true });
  fs.writeFileSync(kept, buf);
  const evidence = keepShotEvidence("changed", key, existing, buf, {
    bound: rule ? { class: rule.class, bound: stability.describeBound(rule.bound), outside: true } : null,
  });
  console.log(
    "  shot changed" + outside + ": " + key + " — " + difference + " — the committed reference is NOT rewritten; " +
      "this run's picture is at " + path.relative(path.resolve(__dirname, ".."), kept) + "; " + evidenceWords(evidence) +
      ". To accept it: RICHOS_SHOTS_REGENERATE=" + key + " node " + path.basename(process.argv[1] || "<suite>.js") +
      ", then commit it with one line saying what a look at it found"
  );
  changedShots.push({ key, difference, outside, kept, evidence, accept: "RICHOS_SHOTS_REGENERATE=" + key });
  return { file, written: false, bytes: existing ? existing.length : 0, changed: true, kept };
}

/// Forget the pictures recorded as changed so far. ONLY for a self-test that deliberately
/// publishes a picture different from its reference to prove the reference is not rewritten
/// (`tracked-tree.js` check 1); a suite that took a real picture must never call this.
function discardChangedShotsForSelfTest() {
  changedShots.length = 0;
  regeneratedShots.length = 0;
}

/// The same rule for a shot that already exists as a file — the `.shots/` scratch copy a suite
/// took first and now wants to keep. Replaces `fs.copyFileSync`, which always writes.
function publishShotFile(src, dest) {
  return publishShot(fs.readFileSync(src), dest);
}

// ---------------------------------------------------------------------------------------
// A LOOP CANNOT BE WAITED OUT
// ---------------------------------------------------------------------------------------
//
// A suite that photographs a fade waits for it — `corrections.js` waits 300ms for the overlay,
// `home.js` waits for the chip slide to land. That works because a transition ENDS. An
// `iteration-count: infinite` animation never does, so a shot of one is a photograph of an
// arbitrary phase and the file differs every run for no reason anybody can read.
//
// Measured 2026-09-05, two consecutive runs at `5e00651`: `shots-contrast/inspector.png` and
// `technical-view.png` differed in exactly 25 pixels each, in a 5x5 box — the pulse dot inside
// the `WORKING` pill. `shots-26/ms-02-working-for-18s.png`, 42 pixels in a 10x10 box, same dot.
// Three files of churn from one CSS keyframe.
//
// So looping animations are pinned to phase 0 for the length of the capture and released
// immediately after. FINITE ones are left exactly as they are — a suite that waited for a fade
// gets the fade it waited for, and one that deliberately catches a rise mid-flight still does.
// The pin is scoped to the screenshot because `steering.js` and `workers.js` read the computed
// opacity of animated elements, and a harness that quietly left the page frozen would be
// answering their questions about a state the product never sits in.
async function pinLoops(page) {
  const pinned = await page
    .evaluate(() => {
      window.__richosPinnedLoops = [];
      for (const a of document.getAnimations()) {
        const timing = a.effect && a.effect.getComputedTiming ? a.effect.getComputedTiming() : null;
        if (!timing || timing.iterations !== Infinity) continue;
        try {
          a.pause();
          a.currentTime = 0;
          window.__richosPinnedLoops.push(a);
        } catch (_e) {
          /* an animation that refuses to be pinned is left running and reported by the count */
        }
      }
      return window.__richosPinnedLoops.length;
    })
    .catch(() => 0);
  if (pinned) {
    // Two frames: one for the style change to be taken up, one for it to be painted.
    await page
      .evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))))
      .catch(() => {});
  }
  return pinned;
}

/// Wait for every FINITE animation and transition on the page to finish, so the shutter opens
/// on a settled surface rather than partway through one.
///
/// Every suite in this directory was already doing this by hand and by guess —
/// `corrections.js` sleeps 300ms for a 160ms overlay fade, `updates.js` sleeps 150ms after
/// opening the menu, `retention.js` sleeps 120ms after the popover. A sleep is a bet that the
/// machine is not busy, and the bet loses quietly: measured 2026-09-05, two runs of
/// `updates.js` produced `updates-mark-on-the-button.png` differing in 1,085 pixels and
/// `updates-cue-opened-the-row.png` in 3,525, all of them inside the settings panel and none by
/// more than 3 of 255 — the panel photographed at 99-point-something percent of its fade rather
/// than at the end of it.
///
/// `Animation.finished` is the end state itself, so this waits on the thing rather than on a
/// number somebody chose. Bounded, because an animation can be paused or infinite and a
/// screenshot helper must not be able to hang a suite; the bound is generous and never silent
/// about what it gave up on — anything still running is either infinite (and `pinLoops` takes
/// it next) or a real stall the shot will show.
///
/// THE BOUND IS A HANG GUARD NOW, AND IT IS NEVER SILENT (2026-09-29, audit R11). It used to be
/// a flat 2,000ms, after which this resolved as if settled. An animation's end is on the
/// document's REAL timeline, but the promise that reports it is delivered by a page and a harness
/// that a busy host slows down, so a 900ms fade could still be "running" at 2,000ms and the shot
/// was taken mid-fade: a pixel comparison failing far from its cause, or a mid-fade picture
/// published as the reference. The guard is now what the page itself declares: the longest
/// remaining time of any running finite animation, plus `SETTLE_SLACK_MS` for the report to
/// arrive. Running past that is a hang, and it throws, naming the animations still running.
///
/// A caller that passes `timeoutMs` has declared it is working against a clock of its own
/// (`splash.js`, one second of ceiling grace): that caller still gets its cap, and a cap that
/// fires is printed with what was still running rather than swallowed.
const SETTLE_SLACK_MS = 10000;
async function awaitSettled(page, timeoutMs) {
  const r = await page.evaluate(
    ({ capMs, slackMs }) =>
      new Promise((resolve) => {
        const finite = document.getAnimations().filter((a) => {
          const t = a.effect && a.effect.getComputedTiming ? a.effect.getComputedTiming() : null;
          return t && t.iterations !== Infinity && a.playState === "running";
        });
        if (!finite.length) return resolve({ count: 0, settled: true, ms: 0, still: [] });
        const describe = (a) => {
          const el = a.effect && a.effect.target;
          const who = el ? el.tagName.toLowerCase() + (el.id ? "#" + el.id : "") : "?";
          const what = a.animationName || a.transitionProperty || a.id || a.constructor.name;
          return `${what} on ${who}`;
        };
        let remaining = 0;
        for (const a of finite) {
          const t = a.effect.getComputedTiming();
          const rate = Math.abs(a.playbackRate || 1);
          const left = (Number(t.endTime) - Number(t.localTime || 0)) / rate;
          if (Number.isFinite(left)) remaining = Math.max(remaining, left);
        }
        const ms = capMs !== null ? capMs : Math.ceil(remaining) + slackMs;
        let timer = null;
        const done = Promise.all(finite.map((a) => a.finished.catch(() => {}))).then(() => true);
        const cap = new Promise((res) => {
          timer = setTimeout(() => res(false), ms);
        });
        Promise.race([done, cap]).then((settled) => {
          clearTimeout(timer);
          resolve({
            count: finite.length,
            settled,
            ms,
            still: settled ? [] : finite.filter((a) => a.playState === "running").map(describe),
          });
        });
      }),
    { capMs: typeof timeoutMs === "number" ? timeoutMs : null, slackMs: SETTLE_SLACK_MS }
  );
  if (!r.settled) {
    const msg = `awaitSettled: ${r.still.length} animation(s) still running after ${r.ms}ms: ${r.still.join(", ")}`;
    if (typeof timeoutMs !== "number") {
      throw new Error(msg + " — past their own declared end plus " + SETTLE_SLACK_MS + "ms, which is a hang, not a slow host");
    }
    process.stderr.write(`  ${msg} (the caller's own ${timeoutMs}ms cap)\n`);
  }
  return r.count;
}

// ---------------------------------------------------------------------------------------
// WHERE THE POINTER HAPPENS TO BE LYING IS NOT PART OF THE SURFACE
// ---------------------------------------------------------------------------------------
//
// A walk drives a surface by clicking it, and the pointer STAYS where the last click left it.
// If the state the click produced puts a different control under that point, the control is
// drawn hovered — and whether WebKit has re-run the hit test by the time the shutter opens is
// a race with the layout, not a fact about the screen.
//
// MEASURED, 2026-09-19, five consecutive same-source runs at `5f3a1a1e`:
// `shots-contrast/voice-model-progress.png` alternated between two pictures on every run,
// differing in 1,200 pixels at a worst channel delta of 109 — the whole rounded-rect outline
// and label of `Stop the download`. `style.css:1628` is the entire cause:
//
//     .voice-retry:hover { color: var(--ink-soft); border-color: var(--ink-soft); }
//
// and `contrast.js`'s walk reaches that state by clicking `#voice-model-get`, which the stop
// button then replaces under the same pointer. Two runs, two different buttons in the record,
// and nothing in the suite is about hovering.
//
// SO THE POINTER IS PARKED, AND THE CONDITION IS THE HOVER CHAIN ITSELF. Moving the mouse is
// not enough — the move and the hit test are separate, and `waitForTimeout` after it would be
// the same bet on the machine this directory keeps losing. What is waited for is the page
// agreeing: the deepest `:hover` element is the element actually under the pointer, or nothing
// is hovered at all.
//
// OPT-IN, NEVER AUTOMATIC. A suite that photographs a hover ON PURPOSE must keep it, so this
// is `opts.parkPointer` at the call site and not something `shot()` does behind every caller's
// back.
//
// IT SAYS SO WHEN IT GIVES UP (2026-09-29, audit R11). The wait used to be 2,000ms and then
// `.catch(() => {})`: on a busy host the hit test ran late, the helper returned anyway, and the
// shot was taken hovered — a pixel comparison failing far from its cause, or worse, a hovered
// picture published as the reference. The bound is now a hang guard (the hover chain normally
// agrees within a frame or two), and running out of it throws, naming what was hovered and what
// was under the pointer.
const PARK_POINTER_GUARD_MS = 20000;
async function parkPointer(page, at) {
  const x = at && typeof at.x === "number" ? at.x : 0;
  const y = at && typeof at.y === "number" ? at.y : 0;
  await page.mouse.move(x, y);
  try {
    await page.waitForFunction(
      ([px, py]) => {
        const hovered = document.querySelectorAll(":hover");
        if (!hovered.length) return true;
        const under = document.elementFromPoint(px, py);
        return under === hovered[hovered.length - 1];
      },
      [x, y],
      { timeout: PARK_POINTER_GUARD_MS }
    );
  } catch (e) {
    const seen = await page
      .evaluate(([px, py]) => {
        const name = (el) => (el ? el.tagName.toLowerCase() + (el.id ? "#" + el.id : "") + (el.className && typeof el.className === "string" ? "." + el.className.trim().split(/\s+/).join(".") : "") : "nothing");
        const hovered = document.querySelectorAll(":hover");
        return { hovered: name(hovered[hovered.length - 1]), under: name(document.elementFromPoint(px, py)) };
      }, [x, y])
      .catch(() => ({ hovered: "unknown", under: "unknown" }));
    throw new Error(
      `parkPointer: the pointer was parked at (${x}, ${y}) and the page's hover chain did not agree within ` +
        `${PARK_POINTER_GUARD_MS}ms (still hovered: ${seen.hovered}; under the pointer: ${seen.under}). ` +
        `A shot now would photograph a hover nobody asked for. ${String(e.message || e).split("\n")[0]}`
    );
  }
}

// ---------------------------------------------------------------------------------------
// A THREE-SECOND BACKGROUND POLL WRITES INTO THE CHROME OF EVERY CONVERSATION SURFACE
// ---------------------------------------------------------------------------------------
//
// `main.js:3098` polls `get_worker_status` every 3,000 ms and calls `renderDrillChip`, which
// begins `drillChipEl.innerHTML = ""` and rebuilds the chip from the answer — every tick,
// whether or not the answer moved. A surface photographed between a state the walk DROVE and
// the next tick therefore shows the previous answer, and two runs land on opposite sides of
// that boundary.
//
// MEASURED, 2026-09-19, five consecutive same-source runs at `5f3a1a1e`:
// `shots-26/ms-05-worker-detail-beside-thread.png` differed between runs in 962 pixels inside
// one 139x13 box — `· 1 I can't see` in one run, `· 1 done · 1 I can't see` in the next, at a
// worst channel delta of 131. `ms-03` and `ms-04` moved in the same box by 292 and 432 pixels.
// That is a difference in what the picture SAYS, in the chrome, and no suite in this directory
// was waiting for it.
//
// THE CONDITION IS THE PRODUCT'S OWN RENDER. Two consecutive re-renders of the zone carrying
// identical text prove the poll has caught up: the first is a tick that happened after this
// call, the second is the tick after it agreeing. If the zone is never re-rendered at all
// within one full poll period plus a margin, it is not being rewritten and the surface is
// already still — which is the answer on every page where no thread is open.
//
// NEVER THROWS. This is a shutter helper; it returns what it found, including "still changing",
// and the caller decides. A helper that could fail a suite on its own would be a second thing
// to debug on the day a real defect makes the chip flicker.
const WORKER_POLL_MS = 3000; // main.js:3098 — the interval this waits on, named rather than guessed
async function awaitWorkerChipSettled(page, budgetMs) {
  const budget = budgetMs || 9000;
  return page
    .evaluate(
      ({ budget: cap, quiet }) =>
        new Promise((resolve) => {
          const zone = document.getElementById("drill-chip-zone");
          if (!zone) return resolve({ settled: true, renders: 0, why: "no drill chip on this surface" });
          // SEEDED WITH WHAT IS ON SCREEN NOW, so the FIRST re-render that agrees with it
          // settles this rather than the second. The walk drives its state and then calls
          // here, so a tick that lands after this line has already read the state the walk
          // set; requiring two ticks proved the same thing at twice the price, and this
          // suite is the cheapest in the directory and should stay that way.
          let last = zone.textContent;
          let renders = 0;
          let quietTimer = null;
          let capTimer = null;
          const finish = (r) => {
            obs.disconnect();
            clearTimeout(quietTimer);
            clearTimeout(capTimer);
            resolve(r);
          };
          const armQuiet = () => {
            clearTimeout(quietTimer);
            quietTimer = setTimeout(
              () => finish({ settled: true, renders, why: "no re-render for " + quiet + "ms", text: zone.textContent }),
              quiet
            );
          };
          const obs = new MutationObserver(() => {
            renders++;
            const text = zone.textContent;
            if (last !== null && text === last) {
              return finish({ settled: true, renders, why: "two consecutive re-renders agreed", text });
            }
            last = text;
            armQuiet();
          });
          obs.observe(zone, { childList: true, subtree: true, characterData: true, attributes: true });
          capTimer = setTimeout(
            () => finish({ settled: false, renders, why: "still changing after " + cap + "ms", text: zone.textContent }),
            cap
          );
          armQuiet();
        }),
      { budget, quiet: WORKER_POLL_MS + 400 }
    )
    .catch((e) => ({ settled: false, renders: 0, why: "could not observe the chip: " + String(e).split("\n")[0] }));
}

// ---------------------------------------------------------------------------------------
// A LIVE COUNTER IN THE PICTURE COUNTS THE HARNESS, NOT THE SCENARIO
// ---------------------------------------------------------------------------------------
//
// `main.js:1437` ticks `updateTimers(..., Date.now())` once a second for as long as any row is
// live, so a turn that is still working renders an elapsed time that keeps going up. A suite
// that photographs such a turn photographs ITS OWN RUNTIME: the §26 fixture anchors the clock
// so the row reads `Working for 18s`, and by the fourth screenshot the same row read 34s in one
// run and 35s in the next, purely because this directory's own walk takes a slightly different
// number of milliseconds every time.
//
// MEASURED, 2026-09-19 at `5f3a1a1e`: five of `shots-26`'s nine shots moved between two
// same-source runs, 176 to 617 pixels each at a worst channel delta of 135, every one of them
// inside the two text runs that carry that counter. The number in those pictures was never
// evidence of anything — it was the stopwatch on the harness.
//
// SO THE CLOCK IS PINNED FOR THE LENGTH OF THE CAPTURE, TO A VALUE THE FIXTURE DEFINES, and the
// row is recomputed THROUGH THE PRODUCT'S OWN PATH rather than by this file working out what
// the text should say. `main.js:1446` recomputes every timer from its timestamps the instant
// the document becomes visible — that handler is the recompute, so dispatching the event is how
// a pinned clock reaches the screen without a second implementation of the formatter and
// without waiting out a 1,000 ms tick.
//
// NARROW ON PURPOSE. It is a capture-length pin at ONE call site, restored in a `finally`, and
// it is not something `shot()` does to every caller: a suite whose subject IS the clock must
// keep the clock it drove.
async function pinClock(page, atMs) {
  return page
    .evaluate((at) => {
      if (!window.__richosRealDateNow) window.__richosRealDateNow = Date.now.bind(Date);
      Date.now = () => at;
      document.dispatchEvent(new Event("visibilitychange"));
      return at;
    }, atMs)
    .catch(() => null);
}

async function unpinClock(page) {
  return page
    .evaluate(() => {
      if (!window.__richosRealDateNow) return false;
      Date.now = window.__richosRealDateNow;
      window.__richosRealDateNow = null;
      document.dispatchEvent(new Event("visibilitychange"));
      return true;
    })
    .catch(() => false);
}

async function releaseLoops(page) {
  await page
    .evaluate(() => {
      for (const a of window.__richosPinnedLoops || []) {
        try {
          a.play();
        } catch (_e) {
          /* nothing to restore */
        }
      }
      window.__richosPinnedLoops = null;
    })
    .catch(() => {});
}

/// One capture, settled. Everything that opens a shutter in this directory goes through here:
/// the finite animations are waited out, the looping ones are pinned, the frame is taken, and
/// the page is handed back exactly as it was.
///
/// `opts.onShutter` runs at the INSTANT of capture, before anything slow. `splash.js` has to
/// prove its photograph is of the curtain and not of the screen behind it, and it was proving
/// that after a decode and a file comparison had finished — a second or more later, on a
/// surface whose ceiling is one second past its hold. That is a claim about a different moment.
async function captureSettled(page, opts) {
  opts = opts || {};
  // `opts.settleMs` shortens the wait for a caller that has ALREADY proved the surface is
  // settled some other way and is working against a clock. `splash.js` is the one: it waits for
  // the curtain's own bar to report it has landed, and then has exactly the one second of
  // ceiling grace to take the picture in.
  if (opts.parkPointer) await parkPointer(page, opts.parkPointer === true ? null : opts.parkPointer);
  await awaitSettled(page, opts.settleMs);
  await pinLoops(page);
  try {
    const buf = await page.screenshot(opts.screenshot || {});
    if (opts.onShutter) await opts.onShutter();
    // The page's state as the picture was taken, for the evidence a differing picture leaves
    // (`keepShotEvidence`). Read after `onShutter`, which is the time-critical read, and before
    // the loops are released.
    rememberShotState(buf, await pageState(page));
    return buf;
  } finally {
    await releaseLoops(page);
  }
}

async function shot(page, name, opts) {
  opts = opts || {};
  fs.mkdirSync(SHOT_DIR, { recursive: true });
  const file = path.join(SHOT_DIR, name + ".png");
  // Captured to a BUFFER and published, never straight to `path`: `page.screenshot({ path })`
  // writes every time it is called, which is the habit this replaces.
  const buf = await captureSettled(page, {
    screenshot: { fullPage: opts.fullPage !== false },
    settleMs: opts.settleMs,
    onShutter: opts.onShutter,
    parkPointer: opts.parkPointer,
  });
  const bytes = publishShot(buf, file).bytes;

  const stats = await page.evaluate(async (b64) => {
    const img = new Image();
    await new Promise((res, rej) => {
      img.onload = res;
      img.onerror = () => rej(new Error("the PNG did not decode"));
      img.src = "data:image/png;base64," + b64;
    });
    const c = document.createElement("canvas");
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    const ctx = c.getContext("2d");
    ctx.drawImage(img, 0, 0);
    // A bounded grid — a 120x120 sample is enough to tell a painted UI from a flat fill and
    // costs nothing on a 2000px-tall full-page shot.
    const stepX = Math.max(1, Math.floor(c.width / 120));
    const stepY = Math.max(1, Math.floor(c.height / 120));
    const seen = new Set();
    const d = ctx.getImageData(0, 0, c.width, c.height).data;
    let sampled = 0;
    for (let y = 0; y < c.height; y += stepY) {
      for (let x = 0; x < c.width; x += stepX) {
        const i = (y * c.width + x) * 4;
        seen.add((d[i] << 16) | (d[i + 1] << 8) | d[i + 2]);
        sampled++;
      }
    }
    return { width: c.width, height: c.height, distinct: seen.size, sampled };
  }, buf.toString("base64"));

  if (stats.distinct < SHOT_MIN_DISTINCT) {
    throw new Error(
      `${name}.png is ${stats.width}x${stats.height} with only ${stats.distinct} distinct ` +
        `color(s) across ${stats.sampled} samples — that is a flat fill, not a render. ` +
        `Evidence of NOTHING (${file}).`
    );
  }

  // THE TWO DECODERS ARE JOINED HERE, on every shot. `publishShot` decides whether to write a
  // file by reading it with `./png.js`; the check above reads the SAME bytes with WebKit's own
  // decoder. If the two ever disagree about what image this is, the skip-when-unchanged rule
  // is being applied to a picture nobody has seen — so they are compared rather than each
  // trusted alone. Dimensions, because that is what both sides report about the whole image
  // without either one having to sample it the other's way.
  const local = png.decode(buf);
  if (local.width !== stats.width || local.height !== stats.height) {
    throw new Error(
      `${name}.png decodes as ${local.width}x${local.height} in lib/png.js and ` +
        `${stats.width}x${stats.height} in WebKit — the two decoders disagree, so no ` +
        `screenshot comparison in this directory can be trusted.`
    );
  }

  return Object.assign({ file, bytes }, stats);
}

// ---------------------------------------------------------------------------------------
// A very small test runner. No framework: one dependency is enough for a harness whose
// whole point is that it is cheap enough to keep.
// ---------------------------------------------------------------------------------------

// ---------------------------------------------------------------------------------------
// The evidence ledger
// ---------------------------------------------------------------------------------------
//
// A suite prints PASS/FAIL lines for a human. `run.js` needs the same facts as NUMBERS, so
// it can refuse to report green over a suite that ran and checked nothing — the failure this
// repository has now produced twice: a scanner reporting CLEAN over an empty corpus, and a
// `run.js` reporting "all 4 suites passed" over a suite it was not running. Neither was
// detectable from a suite's exit code, because both exited 0 honestly.
//
// So every `report()` appends one JSON line naming how many checks it actually ran, and
// `skipSuite()` writes the other kind of record: this suite produced no evidence, and here
// is why. `run.js` reads them back and gates on them.
//
// Written ONLY when RICHOS_UI_TESTS_LEDGER is set, which `run.js` does for its children.
// `node workers.js` on its own is byte-for-byte unchanged.

const LEDGER = process.env.RICHOS_UI_TESTS_LEDGER || "";

function recordEvidence(rec) {
  if (!LEDGER) return;
  const suite = path.basename(process.argv[1] || "unknown");
  fs.appendFileSync(LEDGER, JSON.stringify(Object.assign({ suite }, rec)) + "\n");
}

/// A suite that CANNOT run says so here rather than returning 0 quietly. The distinction is
/// the whole point: "did not run" and "ran and found nothing wrong" are different facts, and
/// only one of them is evidence. `run.js` fails on any skip it was not told to expect.
function skipSuite(label, reason) {
  console.log("\n== " + label + " ==");
  console.log("  SKIP  this suite did not run");
  console.log("        " + reason);
  console.log("        SKIPPED, not passed.");
  recordEvidence({ label, skipped: reason });
}

const TREE_CHECK = "the tracked tree is exactly as this suite found it";
const SHOT_CHECK = "every picture this suite took matches its committed reference";
const REGEN_CHECK = "no picture was approved by the run that drew it";

function createRun(label) {
  const results = [];
  return {
    label,
    async check(name, fn) {
      // A navigation that fails inside this check names it in its evidence bundle.
      navigation.setCurrentCheck(label, name);
      try {
        const detail = await fn();
        results.push({ name, ok: true, detail: detail || "" });
      } catch (e) {
        results.push({ name, ok: false, detail: (e && e.message) || String(e) });
      } finally {
        navigation.setCurrentCheck(label, null);
      }
    },
    report() {
      // NO TEST WRITES INTO THE TREE IT IS VERIFYING (`lib/tracked-tree.js`). Every suite that
      // reports carries this check, so a writer is red in its OWN report, by name, on its own
      // branch — instead of refusing whichever merge gate later verifies it in the main checkout.
      const tree = TREE.verify();
      const seen = results.findIndex((r) => r.name === TREE_CHECK);
      if (seen >= 0) results.splice(seen, 1); // a second report() re-measures, never double-counts
      // Housekeeping checks are appended by this function, not declared in the suite's source,
      // so `run.js`'s coverage floor must not count them as the suite's own (hunt section 26):
      // they carry `housekeeping: true` and `productChecks` below leaves them out.
      const seenShot = results.findIndex((r) => r.name === SHOT_CHECK);
      if (seenShot >= 0) results.splice(seenShot, 1);
      if (changedShots.length) {
        results.push({
          name: SHOT_CHECK,
          ok: false,
          housekeeping: true,
          detail:
            changedShots.length + " picture(s) differ from the committed reference (or have none); " +
            "the references were NOT rewritten:\n            " +
            changedShots
              .map(
                (c) =>
                  c.key + " — " + c.difference + c.outside + " — this run's picture: " +
                  path.relative(path.resolve(__dirname, ".."), c.kept) + " — " + evidenceWords(c.evidence) +
                  " — to accept: " + c.accept
              )
              .join("\n            ") +
            "\n          First decide from the kept evidence whether each change is intended, an unsettled capture or a " +
            "regression. To regenerate exactly the pictures that failed, in your own worktree: RICHOS_SHOTS_REGENERATE=" +
            changedShots.map((c) => c.key).join(",") + " node " + path.basename(process.argv[1] || "<suite>.js") +
            "; then commit each with one line saying what a look at it found.",
        });
      }
      const seenRegen = results.findIndex((r) => r.name === REGEN_CHECK);
      if (seenRegen >= 0) results.splice(seenRegen, 1);
      if (regeneratedShots.length) {
        results.push({
          name: REGEN_CHECK,
          ok: false,
          housekeeping: true,
          detail:
            regeneratedShots.length + " picture(s) were drawn by this run and written as their own reference, so " +
            "this run is not evidence that they are right:\n            " +
            regeneratedShots.map((r) => r.key + " — " + r.difference + " — " + evidenceWords(r.evidence)).join("\n            ") +
            "\n          Look at each (reference, new picture and difference are kept), commit it with one line per " +
            "picture saying what the look found, then run " + path.basename(process.argv[1] || "<suite>.js") +
            " again without RICHOS_SHOTS_REGENERATE: that run is the evidence.",
        });
      }
      if (tree.checked) {
        const ok = tree.changed.length === 0;
        results.push({
          name: TREE_CHECK,
          housekeeping: true,
          ok,
          detail: ok
            ? ""
            : "changed while this suite ran, in " + path.join(tree.root, tree.scope || ".") +
              " (the UI tree; edits elsewhere in the checkout are not watched here):\n            " +
              tree.changed.map((c) => c.file + "  (" + c.before + " -> " + c.after + ")").join("\n            ") +
              "\n          A test never writes a tracked path: a committed shot is replaced only under " +
              "RICHOS_SHOTS_REGENERATE, and everything else a run produces belongs in .shots/ (gitignored). " +
              "If a person or another process was editing this checkout during the run, that is the other writer.",
        });
      } else {
        console.log("  tracked tree: NOT CHECKED — " + tree.why);
      }
      let failed = 0;
      console.log("\n== " + label + " ==");
      for (const r of results) {
        if (!r.ok) failed++;
        console.log(`  ${r.ok ? "PASS" : "FAIL"}  ${r.name}${r.detail ? "\n          " + r.detail : ""}`);
      }
      // A RED CHECK NAMES ITSELF IN THE RECORD, not only on stdout. Run 20260928T233221Z-56bcde43's
      // receipt said home.js ran 38 checks and 1 failed, the log had lost the FAIL line, and
      // nothing a build keeps could say which check or why. The receipt (`run.js`) and the
      // nightly's own FAILED line (`nightly-local.py` red_ui_suites) now carry these.
      const failures = results
        .filter((r) => !r.ok)
        .map((r) => ({ check: r.name, message: String(r.detail || "").slice(0, 2000) }));
      // `checks` is everything reported; `productChecks` is only what the suite's source declares
      // (housekeeping excluded), the number `run.js` measures against the declared count.
      const productChecks = results.filter((r) => !r.housekeeping).length;
      // `declared` is what the suite's own source promises (the number `run.js` measures the
      // run against), so a suite run DIRECTLY, without run.js, can be held to the same floor by
      // whoever reads the ledger (recheck R26). Omitted when the source cannot be read.
      let declared;
      try {
        declared = require("./ui-sources").declaredChecks(fs.readFileSync(process.argv[1], "utf8"));
      } catch (_e) {
        declared = undefined;
      }
      recordEvidence(
        Object.assign({ label, checks: results.length, productChecks, failed }, declared === undefined ? {} : { declared }, failed ? { failures } : {})
      );
      return failed;
    },
  };
}

/// The verbatim body of a `const NAME: &str = "…";` in Rust source, with rustc's
/// line-continuation rules applied — the same way `lib/state-strings.js` reads them, and for
/// the same reason: every CEO-facing sentence in `main.rs` is written across two or three
/// lines with a trailing backslash.
///
/// It lives here because two suites now join `mock.js`'s copy of a product sentence to the
/// const the product ships (`corrections.js` check 12, `feedback.js` check 8), and a
/// subtle parser copied into both is the drift those checks exist to catch, one level out.
///
/// `marker` is any text inside the literal. The scan goes BACKWARD to the literal's opening
/// quote: forward would find its CLOSING one and read the code after it, which is what the
/// first version of this did — it reported `.to_string() }) .map(|d| d.lock()…` as the
/// sentence the CEO sees.
function rustSentenceAfter(src, marker) {
  const at = src.indexOf(marker);
  assert(at >= 0, "marker not found in the Rust source: " + marker);
  const open = src.lastIndexOf('"', at);
  assert(open >= 0, "no opening quote before the marker: " + marker);
  let out = "";
  for (let i = open + 1; i < src.length; i++) {
    const c = src[i];
    if (c === "\\") {
      const n = src[i + 1];
      if (n === "\n") {
        i++;
        while (src[i + 1] === " " || src[i + 1] === "\t") i++;
        continue;
      }
      out += n;
      i++;
      continue;
    }
    if (c === '"') break;
    out += c;
  }
  return out.replace(/\s+/g, " ").trim();
}

function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}

function assertEqual(actual, expected, msg) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${msg}\n          expected ${e}\n          actual   ${a}`);
}


// ---------------------------------------------------------------------------------------
// HOLDING THE OPENING CURTAIN, INCLUDING AGAINST THE PRODUCT'S OWN CEILING
// ---------------------------------------------------------------------------------------

/// An init script that keeps the opening curtain up for the WHOLE of a walk, however long
/// that walk takes. Pass it to `page.addInitScript` before `goto`.
///
/// WHAT WAS HERE BEFORE, AND WHY IT WAS NOT A HOLD. `appearance.js` and `contrast.js` each
/// carried a copy of the same few lines: intercept the assignment of `window.RichSplash` and
/// replace the EXPORTED `yieldNow` with a no-op, so `main.js`'s `app-ready` yield and
/// `leaveSplash`'s own call both become nothing. That is two of the ways out and not all of
/// them. `start()` also arms a CEILING — `setTimeout(function () { yieldNow("ceiling"); },
/// holdMs + CEILING_GRACE_MS)` — over the INTERNAL reference, which no override of the export
/// can reach. So the curtain came down 4,000 ms after boot whatever the suite was doing, and
/// every "held" walk was really a walk on a four-second clock that nothing declared, nothing
/// measured, and nothing failed on until the machine got slower.
///
/// MEASURED on this machine at `934f127`, in appearance.js check 4's exact shape:
///
///   3,972ms  theme=dark   splash=true   reason=null
///   4,054ms  theme=light  splash=true   reason=ceiling   <- the clamp is gone, the node is not
///   4,300ms  theme=light  splash=false  reason=ceiling
///
/// The middle row is `expected "dark" / actual "light"` with `#splash` still on screen, which
/// is the exact pair run 34010691469 reported from `macos-latest`. Nothing was wrong with the
/// product and nothing was intermittent about the mechanism: a runner spends about 2,000 ms
/// more than this machine getting to the assertion, so it lands inside that 270 ms window
/// instead of a second short of it.
///
/// SO THE CEILING IS TAKEN OUT, NOT WAITED OUT. A longer timeout would make a slow runner fail
/// less often; it would not make a held curtain held, and a check that fails less often is not
/// a check that passes. `window.RichSplash = {...}` is assigned on the line BEFORE `start()`
/// runs, so the setter below is a guaranteed pre-start hook: it swaps `setTimeout` for the
/// duration of `start()` and takes back every timer that call arms. `start()` arms exactly one
/// — splash.js's other two `setTimeout` calls both live inside `yieldNow`, which cannot have
/// run yet — and the swap is undone by a microtask, which is drained the moment splash.js's
/// own script finishes and before any other script or timer runs. No other code is inside that
/// window.
///
/// EVERY TIMER TAKEN IS RECORDED RATHER THAN SWALLOWED, on `window.__heldCurtain`, and
/// `assertCurtainHeld` refuses any count but one. An edit that arms a second timer in `start()`
/// therefore arrives as a named failure in the suites that hold the curtain, rather than as a
/// curtain that quietly starts dropping again.
///
/// NOTHING ELSE IS TOUCHED, AND THE CEILING IS STILL THE CEILING EVERYWHERE ELSE. The
/// composition renders exactly as it ships, the always-dark clamp is the product's own, and
/// `splash.js`'s suite — which is where the ceiling is a subject rather than a confound — does
/// not use this and still watches it fire.
/// SEED A THEME PREFERENCE INTO A PAGE — BOTH SIDES OF IT — BEFORE ANY OF ITS SCRIPTS RUN.
///
/// Use as `await page.addInitScript(SEED_THEME, "dark")`.
///
/// WHY EVERY SHOT-TAKING SUITE SUDDENLY NEEDS THIS. Until 2026-09-19 the shipped default
/// preference was `dark` (§15), so a walk that said nothing about theming rendered dark
/// deterministically, and the committed reference PNGs in `shots-*` were all taken that way
/// without a single suite having to mention it. CEO §63 makes the default `system`, which is
/// correct for the product and changes what "said nothing" MEANS in here: the palette is now
/// decided by the `colorScheme` of whatever browser context the page was made in, and
/// Playwright's default for that is light. Thirty-odd reference shots flipped palette in one
/// run, every suite still exiting 0, because nothing they assert is about lighting.
///
/// That is not a failure to suppress — it is a question each suite now has to answer out
/// loud. A reference shot is a picture a person looks at, and a picture whose palette
/// changed because of a default nobody was thinking about is a change to the record that
/// nobody decided. So a suite that PHOTOGRAPHS a surface, or asserts a color, states its
/// lighting; a suite that does neither can follow the host and lose nothing.
///
/// BOTH THE MIRROR AND THE STORE, AND THE SECOND IS THE ONE THAT DECIDES. The mirror
/// (`richos-theme`) is what `theme-boot.js` reads before first paint; the store
/// (`richos-mock-config`, mock.js's stand-in for config.rs) is what
/// `syncAppearanceFromBackend` reconciles against a few hundred milliseconds later, and the
/// BACKEND WINS. Seeding the mirror alone is overwritten by the store's answer mid-walk, and
/// the walk then measures the store's theme while claiming the seed's — the product behaving
/// exactly as designed. `contrast.js` found that out the hard way and its note says so.
///
/// The store is MERGED, never replaced: several suites drive `richos-mock-config` for their
/// own reasons, and a seed that flattened `user_name` or `font_scale` on the way past would
/// be a second, silent change of state wearing a theme's clothes.
const SEED_THEME = (theme) => {
  try {
    window.localStorage.setItem("richos-theme", theme);
    let prev = {};
    try {
      prev = JSON.parse(window.localStorage.getItem("richos-mock-config")) || {};
    } catch (e) {
      prev = {};
    }
    prev.theme = theme;
    if (prev.font_scale === undefined) prev.font_scale = 100;
    if (prev.user_name === undefined) prev.user_name = null;
    window.localStorage.setItem("richos-mock-config", JSON.stringify(prev));
  } catch (e) {
    /* storage unavailable: the §63 default applies, and the shot will say so honestly */
  }
};

/// THE APP'S FRAME NEVER SCROLLS — WATCHED ON EVERY PAGE THAT INSTALLS THIS.
///
/// Use as `await page.addInitScript(FRAME_STILL)`, next to the suite's page-error listener.
///
/// The shell's frame is the document, `#app` and the two side panes. None of them is meant to
/// scroll; only the regions inside them do (`#conversation`, `#rail-nav`, `.op-body`, the
/// inspector's body). A frame box that CAN scroll will, the first time anything asks for an
/// element to be shown: `scrollIntoView` aligns it in every scrollable ancestor, a focus without
/// `preventScroll` does the same, and so does the test driver's own click. Nightly `a1a26a615`
/// photographed exactly that: `#app` (overflow: hidden, so still a scroll container) scrolled
/// 417px by "Wrote N files", the whole shell shifted up, a blank band at the bottom, 39.96% of
/// `output-file-md-dark.png` changed — and no check said why, because nothing looked.
///
/// So a scroll of any frame box to a non-zero offset is reported the way every suite here already
/// fails on: as a console error, which each suite's `pageerror`/`console` listener collects and
/// its "no page errors" checks refuse. Scroll events do not bubble, so this listens in the
/// capture phase on the document, which every element's scroll passes through.
const FRAME_STILL = () => {
  const FRAME = ["app", "stage", "outpanel", "inspector", "rail"];
  document.addEventListener(
    "scroll",
    (e) => {
      const t = e.target;
      let what = null;
      let box = null;
      if (t === document) {
        what = "the document";
        box = document.scrollingElement;
      } else if (t && t.id && FRAME.includes(t.id)) {
        what = "#" + t.id;
        box = t;
      }
      if (!box || (box.scrollTop === 0 && box.scrollLeft === 0)) return;
      console.error(
        "FRAME SCROLLED: " + what + " scrolled to top " + box.scrollTop + "px, left " + box.scrollLeft +
          "px; the app's frame never scrolls (style.css `#app`, tests/lib/harness.js FRAME_STILL)"
      );
    },
    true
  );
};

const HOLD_CURTAIN = () => {
  let real;
  // `armedAt` is the page's own clock at the instant the ceiling would have been armed, which
  // is inside `start()` and therefore within a millisecond of the curtain going up. It is what
  // lets a check ask "is the ceiling's moment behind us?" instead of "has the harness been
  // slow enough?", which is the same question on this machine and a different one on a runner.
  window.__heldCurtain = { disarmed: [], armedAt: null };
  Object.defineProperty(window, "RichSplash", {
    configurable: true,
    get: () => real,
    set: (v) => {
      real = v;
      if (v && typeof v.yieldNow === "function") v.yieldNow = function () {};
      const realSetTimeout = window.setTimeout;
      window.setTimeout = function (fn, ms) {
        const id = realSetTimeout.apply(window, arguments);
        window.__heldCurtain.disarmed.push(ms);
        if (window.__heldCurtain.armedAt === null) window.__heldCurtain.armedAt = performance.now();
        window.clearTimeout(id);
        return id;
      };
      Promise.resolve().then(() => {
        window.setTimeout = realSetTimeout;
      });
    },
  });
};

/// Prove the hold above actually held, rather than assuming it did. Reads the record the init
/// script leaves behind and accepts one shape only: exactly one timer taken, armed for longer
/// than the hold it was guarding — which is the ceiling and can be nothing else — and a
/// curtain that is still on screen with `state.reason` still null.
///
/// Returns a sentence a check can report, so the number that makes the walk safe is in the log
/// rather than in a claim.
async function assertCurtainHeld(page) {
  const held = await page.evaluate(() => ({
    record: window.__heldCurtain || null,
    seconds: window.RichSplash && window.RichSplash.state ? window.RichSplash.state.seconds : null,
    shown: !!(window.RichSplash && window.RichSplash.state && window.RichSplash.state.shown),
    declined: (window.RichSplash && window.RichSplash.state && window.RichSplash.state.declined) || null,
    reason: (window.RichSplash && window.RichSplash.state && window.RichSplash.state.reason) || null,
    onScreen: !!document.getElementById("splash"),
  }));
  assert(held.record, "this page was not opened with HOLD_CURTAIN — the init script never ran");
  assert(held.shown, "the curtain never drew, so there is nothing to hold: declined " + held.declined);
  assertEqual(
    held.record.disarmed.length,
    1,
    "start() armed " + held.record.disarmed.length + " timer(s) and the hold expects exactly one, the " +
      "ceiling — delays " + JSON.stringify(held.record.disarmed) + ". A timer added to start() has to be " +
      "accounted for here rather than taken out unread"
  );
  const grace = held.record.disarmed[0] - held.seconds * 1000;
  assert(
    grace > 0,
    "the timer taken out was armed for " + held.record.disarmed[0] + "ms against a " + held.seconds * 1000 +
      "ms hold, so it is not the ceiling"
  );
  assertEqual(held.reason, null, "the curtain yielded anyway, reason: " + held.reason);
  assert(held.onScreen, "#splash is not on screen, so whatever is being measured is not the opening screen");
  return "ceiling disarmed at " + held.record.disarmed[0] + "ms (" + held.seconds + "s hold + " + grace +
    "ms grace), curtain up and state.reason still null";
}

// ---------------------------------------------------------------------------------------
// LEAVING THE HOME SCREEN
// ---------------------------------------------------------------------------------------

/// THE APP'S LANDING SURFACE MOVED (CEO, 2026-09-01): "This start screen/home screen ... must
/// be shown in the app after the splash screen." So `index.html` now opens on `#home`, which
/// covers the shell and marks `#app` inert — and every suite in this directory that drives the
/// APP UI has to do first what the CEO does first, which is leave it.
///
/// IT LEAVES THROUGH `RichHome.hide` AND NOT BY CLICKING THE CONTROL, deliberately. The visible
/// switch on the home screen is PROVISIONAL — the CEO is choosing the real affordance from six
/// designs (`round-11.2`) — and a suite that clicked it would go red the day he picks one,
/// with nothing about that suite's own subject having changed. `home.js` is where the control
/// itself is tested; this is only how the other suites get past it.
///
/// It also restores the theme: the home screen holds §15's always-dark clamp while it is up,
/// and `hide()` drops it, so a suite that asserts on light mode sees the CEO's own preference
/// rather than the clamp.
/// THE CURTAIN IS THE OTHER LAUNCH SURFACE, and it has to go the same way.
///
/// `splash.js` holds the opening screen for THREE SECONDS (`SPLASH_SECONDS`) and then fades it
/// over 180ms. It is `pointer-events: none`, so it is invisible to a hit test and to
/// `waitForSelector` — a suite that never mentions it drives the app perfectly well UNDERNEATH
/// it, and photographs it.
///
/// THAT IS NOT A COSMETIC PROBLEM, and it is the second half of what row r4 was about. Measured
/// 2026-09-05 across two consecutive runs on one machine at `5e00651`:
/// `shots-7-2/7-2-01-the-window-is-a-setting.png` differed in 83.8% of its pixels and
/// `7-2-03-a-window-that-is-on-no-menu.png` in 94.4% — not encoder noise, not antialiasing:
/// one run photographed the whole opening screen sitting over the settings panel and the next
/// did not. `retention.js` reaches its first shot either side of the three-second mark
/// depending on how busy the machine is, so the committed evidence flipped between two
/// completely different pictures, and a `git diff` could not tell that from a regression.
///
/// `appearance.js`, `contrast.js` and `updates.js` had each already written their own wait for
/// this, one line at a time, and the other twelve app-driving suites had not. So it moves here,
/// where the surface is left rather than where somebody remembered.
///
/// A SUITE THAT HOLDS THE CURTAIN ON PURPOSE IS NOT OVERRIDDEN. `appearance.js` and
/// `contrast.js` hold it with `HOLD_CURTAIN` below, precisely so the composition can be
/// photographed as it ships. That is detected — the yield leaves `state.reason` null — and
/// this returns immediately rather than waiting out a timeout for a curtain that was asked to
/// stay.
async function leaveSplash(page) {
  const outcome = await page
    .evaluate(() => {
      const s = window.RichSplash;
      if (!s || !s.state) return "absent";
      // It declined to draw (switched off, or not a fresh start), or it has already gone.
      if (!s.state.shown) return "not-drawn";
      if (s.state.reason) return "already-yielding";
      // A reason of our own, never "app-ready": that one WAITS OUT the remainder of the hold
      // by design, which is the three seconds this is here to stop paying.
      s.yieldNow("acceptance-suite");
      return s.state.reason ? "yielded" : "held";
    })
    .catch(() => "absent");
  if (outcome === "absent" || outcome === "not-drawn" || outcome === "held") return outcome;
  // The end state, not a timer: `removeSelf` takes the node out 220ms after the yield.
  await page.waitForFunction(() => !document.getElementById("splash"), null, { timeout: 30000 });
  return outcome;
}

async function leaveHome(page) {
  // The curtain first: it is ABOVE the home screen, and `RichHome.hide()` deliberately keeps
  // §15's always-dark clamp raised while it is still up (`home.js`'s `splashStillUp`). Clearing
  // it first is what lets one call leave a CEO on light mode actually on light mode.
  await leaveSplash(page);
  // WHETHER THIS PAGE HAS A HOME SCREEN IS DECIDED BY THE DOCUMENT, NOT BY A TIMER (2026-09-29,
  // audit R11). This used to wait 5,000ms for `window.RichHome` and quietly return `false` when it
  // had not appeared, and all but one of the ~60 callers ignore the return: on a busy host a
  // slow parse handed the suite a page whose home screen was still covering an inert `#app`, and
  // the failure surfaced checks later, far from its cause. `home.js` is a classic synchronous
  // script, so once the document has been parsed its export exists or never will. So: wait for
  // the parse (a hang guard, which names itself), then read the answer. No `home.js` on the page
  // is a real "there is nothing to leave" and still returns `false`; `home.js` on the page and no
  // export is a home screen that failed to load, and that is said, not handed on.
  await page
    .waitForFunction(() => document.readyState !== "loading", null, { timeout: 30000 })
    .catch((e) => {
      throw new Error("leaveHome: the document never finished parsing within 30s: " + String(e.message || e).split("\n")[0]);
    });
  const seen = await page.evaluate(() => ({
    present: typeof window.RichHome === "object" && window.RichHome !== null,
    script: !!document.querySelector('script[src$="home.js"]'),
  }));
  if (!seen.present) {
    if (seen.script) {
      throw new Error(
        "leaveHome: home.js is on this page but window.RichHome never appeared after the document was parsed; " +
          "the home screen failed to load (read the page's errors), so there is no app UI to hand back"
      );
    }
    return false;
  }
  await page.evaluate(() => {
    if (window.RichHome && window.RichHome.isOpen()) window.RichHome.hide("acceptance-suite");
  });
  // `hide()` fades before it un-mounts, so the wait is on the end state and not on a timer.
  await page
    .waitForFunction(
      () => {
        const h = document.getElementById("home");
        return !h || h.hidden;
      },
      null,
      { timeout: 30000 }
    )
    .catch((e) => {
      throw new Error("leaveHome: the home screen was asked to hide and was still up after 30s: " + String(e.message || e).split("\n")[0]);
    });
  return true;
}

// ---------------------------------------------------------------------------------------
// A DELIBERATE SLOW RUNNER
// ---------------------------------------------------------------------------------------

/// An init script that puts `ms` of latency on EVERY bridge call the page makes. Pass it to
/// `page.addInitScript` before `goto`.
///
/// WHY THIS IS THE VERIFICATION LEVER AND TWENTY GREEN RUNS ARE NOT. A blind
/// `waitForTimeout(300)` after a click passes twenty times out of twenty on this machine and
/// fails on a `macos-latest` runner that reaches the same line two seconds later — so a green
/// standalone count proves the machine, not the check. This reproduces the runner's condition
/// on demand, at the seam that actually decides it: `main.js` awaits six bridge calls between
/// a thread row being clicked and its turns being on screen, so 120 ms here is 720 ms of extra
/// distance for a driver to be wrong about.
///
/// A CHECK THAT IS GREEN AT ZERO AND GREEN AT 120 IS A CHECK THAT WAITED FOR A SIGNAL. One
/// that is green at zero and reports the previous screen at 120 was measuring the clock, and
/// that is the difference this exists to make visible.
///
/// Wrapped at the ASSIGNMENT of `window.RichBridge` rather than after load, so every caller in
/// the page is behind it — including the ones a driver sets off and returns from. `contrast.js`
/// wrote this first, inline; it is here so the other suites can ask the same question.
const SLOW_BRIDGE = (ms) => {
  let real;
  Object.defineProperty(window, "RichBridge", {
    configurable: true,
    get: () => real,
    set: (v) => {
      real = v;
      const invoke = v.invoke.bind(v);
      v.invoke = async (name, args) => {
        await new Promise((r) => setTimeout(r, ms));
        return invoke(name, args);
      };
    },
  });
};

// ---------------------------------------------------------------------------------------
// ARRIVING ON A THREAD — THE END STATE, NOT THE CLICK
// ---------------------------------------------------------------------------------------

/// Two animation frames: one for a style or DOM change to be taken up, one for it to be
/// painted. Every "and now it is on screen" in this directory needs exactly this, and several
/// places were spelling it out inline.
///
/// It is NOT a substitute for `awaitSettled`. A frame flush proves the browser got as far as a
/// paint; it says nothing about a 160 ms fade still being 40% through it.
async function flushFrames(page) {
  await page
    .evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))))
    .catch(() => {});
}

/// Wait until `main.js`'s `init()` has decided which screen the CEO lands on.
///
/// THE RAIL IS CLICKABLE BEFORE THE APP HAS FINISHED STARTING, and that is not a harness
/// detail — it is a measured property of the shipped boot. `init()` calls
/// `refreshNavigation()`, which renders real `.nav-thread` buttons wired to real handlers, and
/// THEN makes six more bridge calls (`renderHistoryNotice`, `refreshEntityChoice`,
/// `refreshMemory`, `refreshVoiceReadiness`, `refreshSetup`, `active_thread`) before its own
/// `await openThread(active)`. A thread opened inside that window is opened, and then
/// overwritten, because `openThread`'s early return only fires when the ids MATCH.
///
/// MEASURED 2026-09-06 at 120 ms of bridge latency, clicking `hiring` as soon as its row
/// exists:
///
///     {"active":"general","bound":"general","crumb":"Running","entity":"Harbor Analytics","turns":0}
///
/// ...against what the same click produces once this wait is in front of it:
///
///     {"active":"hiring","bound":"hiring","crumb":"Q4 hiring","entity":"Northwind Traders","turns":1}
///
/// That first line is the wrong-screen failure in its natural habitat, and it is the reason
/// this exists: a driver that clicks a thread before the app has finished starting is not
/// driving the app, it is racing it, and the app wins about as often as the machine is busy.
///
/// THE END STATE IS `init()` REACHING ITS LAST STATEMENT, not a number. Either it took one of
/// the arms that ASK — the entity picker, the memory question, the setup sheet, §21's refusal
/// — and put a dialog on screen, or it opened the active thread and ran to the end.
///
/// AND "THE MODEL IS BOUND" IS NOT THE END, WHICH COST A CHECK TO LEARN. `openThread` calls
/// `RichTimeline.bind()` and THEN awaits `refreshTechy` and `loadTimeline`, so a wait that
/// stopped at the binding returns with the conversation still unpainted. Measured 2026-09-06 at
/// 120 ms of bridge latency, sampling the moment such a wait returned and again two seconds
/// later:
///
///     lag=0   at return: {"bound":"general","prose":18,   "rail":16}
///     lag=120 at return: {"bound":"general","prose":null, "rail":16}
///     lag=120 2s later : {"bound":"general","prose":18,   "rail":16}
///
/// `appearance.js` check 13 reads `.tl-prose` straight out of `openApp` and failed on that
/// `null` — correctly, because the surface was not there yet.
///
/// AND THE COMPOSER HOLDING FOCUS IS NOT THE END EITHER, which was the second guess and is
/// worth recording so it is not made a third time. `init()` does end with `inputEl.focus()` —
/// but something earlier on the boot path focuses the composer too. Traced 2026-09-06 at 120 ms
/// of latency, every `focusin` on the document from `goto` onward:
///
///     {"at":46, "id":"home-enter","messages":0,"prose":false}
///     {"at":293,"id":"input",     "messages":0,"prose":false}
///
/// `#input` takes focus at 293 ms with `#messages` still empty, which is hundreds of
/// milliseconds before `loadTimeline` can have painted anything.
///
/// SO THE SIGNAL IS THE CONVERSATION ITSELF. `flushRender` writes into `#messages`, and the
/// conversation view is never empty — a thread with no items still renders Rich's opening
/// greeting (`messages children: 1` on the boot thread, whose model holds zero items). One
/// element in `#messages` is therefore "a conversation has been painted", which is the thing
/// every caller of this actually needs and the thing neither earlier guess was.
async function bootSettled(page, opts) {
  opts = opts || {};
  await page.waitForFunction(
    () => {
      // The arms that ASK rather than open. `startNewThreadFlow` opens the entity picker;
      // `maybeAskAboutSetup` and `maybeAskAboutMemory` open their own sheets; §21's refusal
      // paints `#unbound-view`. None of them paints a conversation, so they are accepted on
      // their own terms.
      const asking = ["entity-picker", "memory-setup", "setup-sheet", "unbound-view"].some((id) => {
        const n = document.getElementById(id);
        return n && !n.hidden;
      });
      if (asking) return true;
      const m = window.__RICHOS_TIMELINE__ ? window.__RICHOS_TIMELINE__() : null;
      if (!m || !m.threadId) return false;
      const messages = document.getElementById("messages");
      return !!messages && messages.children.length > 0;
    },
    undefined,
    { timeout: opts.timeout || 15000 }
  );
  await flushFrames(page);
}

/// Wait until `init()` has finished ENTIRELY — including the four reads it deliberately does
/// after the app is usable.
///
/// `bootSettled` above is "a conversation is painted", which is the moment the CEO can start
/// reading. It is NOT the last thing `init()` does. Five statements come after it, and one of
/// them takes focus:
///
///     if (entityPickerEl.hidden && memorySetupEl.hidden) inputEl.focus();
///     if (window.RichSplash) window.RichSplash.yieldNow("app-ready");
///     await refreshDesk();          // fills #nav-corrections-count
///     syncSplashFromBackend(); noteSplashShown(); readLaunchRecord();
///     syncRetentionFromBackend();   // fills #retention-hint
///
/// THAT `inputEl.focus()` IS A FOCUS STEAL FROM ANYTHING A CHECK HAS FOCUSED, and at 300 ms of
/// bridge latency it lands hundreds of milliseconds after `bootSettled` returns, because
/// `hydrateRunningTurn` sits between them. `updates.js` check 16 — "the waiting cue explains
/// itself by keyboard" — focuses `#update-waiting`, and at that latency init took the focus
/// back before the check could read it: `focused` came out false over a product that had done
/// nothing wrong.
///
/// So this waits on the two elements the LAST reads fill. `#nav-corrections-count` is
/// `refreshDesk`'s, one statement after the focus; `#retention-hint` is
/// `syncRetentionFromBackend`'s, dead last. Both non-empty means every statement in `init()`
/// has run and nothing else is going to move.
///
/// `contrast.js` wrote this first and for a different reason — those two text nodes are ink it
/// has to measure, and both were still empty when its drivers returned at 60 ms per call. It is
/// the same wait for the same shell, so it lives here now rather than in one suite.
async function shellSettled(page, opts) {
  opts = opts || {};
  await page.waitForFunction(
    () => {
      const count = document.getElementById("nav-corrections-count");
      const hint = document.getElementById("retention-hint");
      return !!count && count.textContent.trim() !== "" && !!hint && hint.textContent.trim() !== "";
    },
    undefined,
    { timeout: opts.timeout || 15000 }
  );
}

/// Wait until the conversation on screen IS the thread whose row was clicked.
///
/// WHY THIS IS A FUNCTION AND NOT A SLEEP, and the reason is the worst defect this directory
/// has produced. `main.js`'s `openThread` is a chain of six awaits — `switch_thread`,
/// `refreshActiveContext`, `showConversationView`, `renderRail`, `refreshTechy`,
/// `loadTimeline` — and `renderRail()` runs FIFTH. So the rail marks the new row active while
/// the previous thread's turns are still the ones in `#messages`, and a driver that clicked and
/// then slept was betting the whole chain finished inside its number. At 120 ms of bridge
/// latency it does not: measured 2026-09-06, a walk that had slept 400 ms was reading
/// *Harbor Analytics / "Running"* under a check whose own title said *Northwind Traders / Q4
/// hiring*, with sixteen nodes of the intended thread missing and eight of the previous one
/// present. It reported PASS. Two fewer nodes and it would have reported a clean sweep of the
/// wrong screen.
///
/// THE END STATE IS BOTH HALVES, and the second one is the half a sleep cannot see: the rail
/// row is active AND the timeline model is bound to this thread AND its turns are painted.
/// `window.__RICHOS_TIMELINE__()` is `main.js`'s own read-only handle on the model it renders
/// from, so this asks the product what it is showing rather than inferring it from the rail.
///
/// §21'S REFUSAL IS AN END STATE TOO. An unbound thread never reaches `loadTimeline` — it
/// paints `#unbound-view` and returns — so waiting for turns on `legacy` would wait out a
/// timeout for a screen that is correct. Both branches are accepted and the caller is told
/// which one it got.
///
/// Returns `"conversation"` or `"unbound"`, so a check that names one of them can assert it
/// rather than assume it.
async function settleOnThread(page, threadId, opts) {
  opts = opts || {};
  const expectTurns = opts.expectTurns !== false;
  try {
    await page.waitForFunction(
      ([id, wantTurns]) => {
        const active = document.querySelector(".nav-thread-row.is-active .nav-thread");
        if (!active || active.dataset.threadId !== id) return false;
        const unbound = document.getElementById("unbound-view");
        if (unbound && !unbound.hidden) return true;
        const m = window.__RICHOS_TIMELINE__ ? window.__RICHOS_TIMELINE__() : null;
        if (!m || m.threadId !== id) return false;
        return wantTurns ? document.querySelectorAll("#messages .tl-turn").length > 0 : true;
      },
      [threadId, expectTurns],
      { timeout: opts.timeout || 15000 }
    );
  } catch (e) {
    // A BARE TIMEOUT IS A BAD FAILURE MESSAGE, and this one has the whole diagnosis in reach:
    // which thread the rail marks, which one the model is bound to, and what the scope line
    // says. "Timeout 15000ms exceeded" sends the next reader to the harness; this sends them
    // to the screen.
    const on = await page
      .evaluate(() => {
        const m = window.__RICHOS_TIMELINE__ ? window.__RICHOS_TIMELINE__() : null;
        return {
          active: (document.querySelector(".nav-thread-row.is-active .nav-thread") || { dataset: {} })
            .dataset.threadId,
          bound: m ? m.threadId : null,
          entity: ((document.getElementById("scope-entity") || {}).textContent || "").trim(),
          crumb: ((document.getElementById("scope-thread") || {}).textContent || "").trim(),
          turns: document.querySelectorAll("#messages .tl-turn").length,
        };
      })
      .catch(() => null);
    throw new Error(
      "the app never arrived on thread " + JSON.stringify(threadId) + ". It is showing " +
        (on
          ? JSON.stringify(on.entity + " / " + on.crumb) + " — the rail marks " +
            JSON.stringify(on.active) + ", the model is bound to " + JSON.stringify(on.bound) +
            ", " + on.turns + " turn(s) painted"
          : "a page that could not be read") +
        ". A check that went on from here would be measuring that screen under this one's title."
    );
  }
  await flushFrames(page);
  const unbound = await page.evaluate(() => {
    const u = document.getElementById("unbound-view");
    return !!(u && !u.hidden);
  });
  return unbound ? "unbound" : "conversation";
}

/// Click a thread's row and return when the app is actually showing it. The one-line form of
/// the pair every driver in this directory was writing by hand, half of them without the wait.
async function openThread(page, threadId, opts) {
  await page.click('.nav-thread[data-thread-id="' + threadId + '"]');
  return settleOnThread(page, threadId, opts);
}

/// The same fact as `settleOnThread`, ASSERTED rather than waited for — for the end of a
/// driver, where a late answer arriving from an earlier click and re-rendering the stage has to
/// be a named failure and not a node count that happens to clear its floor.
async function assertOnThread(page, threadId, surface) {
  const on = await page.evaluate(() => {
    const m = window.__RICHOS_TIMELINE__ ? window.__RICHOS_TIMELINE__() : null;
    const u = document.getElementById("unbound-view");
    return {
      active: (document.querySelector(".nav-thread-row.is-active .nav-thread") || { dataset: {} }).dataset
        .threadId,
      bound: m ? m.threadId : null,
      crumb: ((document.getElementById("scope-thread") || {}).textContent || "").trim(),
      turns: document.querySelectorAll("#messages .tl-turn").length,
      unbound: !!(u && !u.hidden),
    };
  });
  const ok = on.active === threadId && (on.unbound || (on.bound === threadId && on.turns > 0));
  assert(
    ok,
    (surface || "this walk") +
      " is not on the thread it is named for: asked for " +
      JSON.stringify(threadId) +
      ", the rail marks " +
      JSON.stringify(on.active) +
      ", the model is bound to " +
      JSON.stringify(on.bound) +
      ", the scope line reads " +
      JSON.stringify(on.crumb) +
      ", " +
      on.turns +
      " turn(s) painted. Whatever this measured, it is not the surface in its title"
  );
  return on;
}

/// THE GROUND, READ OFF THE ELEMENT THAT PAINTS IT. Since the speckled ground (style.css "THE
/// SPECKLED GROUND", 2026-09-29) `body` is transparent ON PURPOSE: a non-positioned block's
/// background paints above a negative z-index layer, so a painted body would cover the speckle
/// canvas completely. The ruled ground is `html`'s. A check that kept reading `body` reads
/// `rgba(0, 0, 0, 0)` in every theme and fails for a reason that says nothing about the theme.
///
/// This returns the FIRST of `body`, `html` that paints anything at all, not "html" by name. So
/// it still fails when the ground is wrong: a body that regains a paint of its own is what gets
/// read (and a wrong one is named), an html that paints the same color in both themes is
/// caught by the caller's comparison, and if neither paints, the transparent value comes back
/// and the caller's assertion fails on it rather than on a guess.
async function paintedGround(page) {
  return page.evaluate(() => {
    const clear = (c) => c === "transparent" || /^rgba\([^)]*,\s*0\)$/.test(c);
    for (const el of [document.body, document.documentElement]) {
      const c = getComputedStyle(el).backgroundColor;
      if (!clear(c)) return c;
    }
    return getComputedStyle(document.documentElement).backgroundColor;
  });
}

module.exports = {
  loadPlaywright,
  openFixture,
  paintedGround,
  leaveHome,
  leaveSplash,
  awaitSettled,
  parkPointer,
  awaitWorkerChipSettled,
  pinClock,
  unpinClock,
  flushFrames,
  bootSettled,
  shellSettled,
  openThread,
  settleOnThread,
  assertOnThread,
  SLOW_BRIDGE,
  SEED_THEME,
  FRAME_STILL,
  HOLD_CURTAIN,
  assertCurtainHeld,
  skipSuite,
  shot,
  captureSettled,
  publishShot,
  publishShotFile,
  discardChangedShotsForSelfTest,
  createRun,
  assert,
  assertEqual,
  rustSentenceAfter,
  UI_DIR,
  SHOT_DIR,
};
