// NO HOME-NETWORK PATH IS EVER BUILT FOR THE PHONE AGAIN. This suite is the mechanism.
//
// **THE CEO, 2026-09-19, ~15:05Z, in full:** *"How many more times will any 'home network'
// related shit be associated with a phone or built for phone?"*
//
// The answer is zero, and it is zero by mechanism rather than by anyone remembering. He had to
// ask twice on the same day — *"What did I say previously about phone and 'home network'?"* and
// then *"What was unclear about what I said regarding the phone?"* — after a build that kept the
// route chooser, the self-signed certificate, the trust page and the sixteen certificate taps
// through §61, and after briefs that asserted §61 kept both paths. Nothing was unclear. What was
// missing was something that fails when the words come back.
//
// **CEO §61, 2026-09-18, which is the ruling this enforces:** *"the new working definition for a
// mobile app or PWA is this: it's an app that lets the user use RichOS (in some way) while being
// on the go and away from office i.e. outside the home network. Because any mobile app or PWA is
// utterly useless within the home network. The desktop app is a much better tool in that case.
// Now, here are the 2 things: 1) For non-technical CEOs I'll use the same approach that T3 Code
// is using i.e. the approach where I have to pay some money to run it. … 2) The RichOS desktop
// app will guide technical users (with the help of crystal-clear and ultra-simple how-to screens
// in the app) to their Tailscale setup. That's it. The Tailscale setup is where we start now."*
//
// ---------------------------------------------------------------------------------------
// WHAT IT READS, AND WHAT IT DELIBERATELY DOES NOT
// ---------------------------------------------------------------------------------------
//
// **It reads code and the strings a person sees. It does not read comments.** Every file under
// scan is stripped of its comments before a single pattern runs, because the record of a ruling
// is written in comments and a check that could not tell a ruling from a violation would be a
// check whose first correct answer is a false positive. `phone.js` opens by quoting §61 and
// naming the trust QR at `http://<name>.local:8444/ca` as the thing that was removed; that
// sentence must pass, and it does — this file's own header must pass too, and check 6 proves it.
//
// **The stripper respects strings, because `//` is inside every URL in this codebase.** A naive
// line-comment rule would cut `https://…` in half and hide the rest of the line from every
// pattern — a scanner that silently stops scanning, which is the exact failure mode
// `feedback_negative_tests_pass_for_wrong_reason.md` is about. Check 5 is the positive control:
// it plants each banned string in a scratch file, in code position, and requires this suite to
// find every one of them.
//
// ---------------------------------------------------------------------------------------
// WHY A SUITE AND NOT A GIT HOOK
// ---------------------------------------------------------------------------------------
//
// It runs in `ui/tests/run.js`, which is discovered from disk and runs at every land. A hook
// would live on one operator's machine, would be invisible to a fresh clone, and would be waived
// the first time it fired on a false positive — this project has three recorded instances of
// that pattern in one day. A suite that fails is a land that stops.
//
// Run: node no-home-network.js   (or `npm test` for every suite in this directory)

"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const APP_DIR = path.resolve(UI_DIR, "..");
const RICHOS_DIR = path.resolve(APP_DIR, "..");

/// **THE SURFACES THE RULING BINDS**, each named because a glob over the repository would put
/// this file's own header, the verification records and the design documents under a scanner
/// that has no business reading them. These four are where a phone path would have to be built.
const SURFACES = [
  { what: "the phone flow's own screen", file: path.join(UI_DIR, "phone.js"), kind: "js" },
  { what: "the Mac's half of the channel", dir: path.join(APP_DIR, "src-tauri", "src", "phone"), ext: ".rs", kind: "rust" },
  { what: "the phone commands on the bridge", file: path.join(APP_DIR, "src-tauri", "src", "main.rs"), kind: "rust" },
  {
    what: "the phone app itself",
    dir: path.join(RICHOS_DIR, "web", "web-app"),
    ext: [".js", ".html", ".css"],
    kind: "web",
    skip: ["node_modules"],
  },
];

/// **THE STRINGS, AND WHAT EACH ONE IS THE TELL FOR.**
///
/// Every one of these was in the product on 2026-09-19 and is not now. The pattern is what a
/// reintroduction would have to write; the `why` is what it would mean.
///
/// **`\.local:8443` and `\.local:8444` rather than `.local`**, and that is a deliberate
/// narrowing with a reason: the certificate this Mac presents to anything that does not ask for
/// its tailnet name still carries `<host>.local` as a subject name, and `listen.rs`'s own tests
/// reach it by that name on an ephemeral port. What is banned is the PAIRING ORIGIN — that name
/// on the product's fixed ports — which is the thing a phone was ever pointed at.
const BANNED = [
  {
    pattern: /at[\s-]?home/i,
    why: "the route chooser's second option, or a screen that belongs to it",
  },
  { pattern: /AtHome/, why: "the `Route::AtHome` variant, or its equivalent" },
  { pattern: /home[\s-]?network/i, why: "a home network named as a way to reach this Mac" },
  { pattern: /home[\s-]?plan/i, why: "`serving_plan`'s home branch, or its equivalent" },
  { pattern: /\.local:844[34]/, why: "the `.local` pairing origin or its trust page" },
  { pattern: /8444/, why: "the trust port" },
  { pattern: /trust[\s-]?qr/i, why: "the QR code that pointed at the profile" },
  { pattern: /self[\s-]?signed/i, why: "a certificate this Mac signs for a phone to install" },
  { pattern: /certificate[\s-]?profile/i, why: "the Apple profile the trust page served" },
  { pattern: /Remove Profile/i, why: "walking the user out of a profile nothing installs" },
  {
    /// **NARROWED 2026-09-20, and the narrowing is the finding.** The bare `/sixteen/i` was
    /// the only pattern here that banned an ENGLISH NUMBER rather than a thing, and a number
    /// belongs to whoever writes it. It matched `web/web-app/test/queue.test.js:209` —
    /// *"the retry schedule doubles from a second and stops at sixteen"* — which is the
    /// outbox's retry cap, argued in `lib/queue.js:96-98`, months older than the trust walk
    /// and unrelated to a certificate in every way but the numeral. Measured on pristine
    /// `049d8790`: `node no-home-network.js` → check `1.web-app` FAIL, one hit, that line.
    ///
    /// **A check whose only failure in a clean tree is a false one gets waived, and a waived
    /// check is a dead check** — this project has three recorded instances of that in a day.
    /// So the number is tied to the thing it counted: the TAPS of the trust walk. A
    /// reintroduction has to write them, and every other shape of it — the port, the `.local`
    /// origin, the trust QR, the profile, the self-signed certificate — is banned by name in
    /// the ten patterns above and does not depend on this one at all.
    pattern: /\b(?:sixteen|16)\b[^\n]{0,40}?\btaps?\b|\bcertificate[-\s]taps?\b/i,
    why: "the sixteen certificate taps of the trust walk",
  },
];

/// **Strip comments, and nothing else.** String and template literals survive intact, because
/// what is being scanned is the code and the strings a person reads — and because `//` lives
/// inside every URL here, so a stripper that did not know a string from a comment would eat the
/// rest of any line with an address on it.
///
/// It returns a string of the SAME LENGTH, with comment bytes replaced by spaces, so a line and
/// column computed from an index is the real one in the real file.
///
/// **THREE THINGS IT HAS TO KNOW, and each one cost a false positive on the first run:**
///
///  1. **A template literal can hold MARKUP, and markup's comments are comments.** `phone.js`
///     builds the whole phone sheet as one backtick string with `<!-- … -->` notes through it,
///     including the one that names the trust QR as the thing §61 removed. So an HTML comment is
///     stripped wherever it is found, inside a template literal as well as in a `.html` file.
///  2. **Rust's `'` is usually NOT a string.** `'static`, `'a` and `'_` are lifetimes, and a
///     stripper that read one as an opening quote would run to the next apostrophe — through
///     doc comments, through code, through anything — and everything it swallowed would go
///     unscanned. That is how `api_base.rs`, `device.rs` and `routes.rs` each reported a hit
///     inside a comment on the first run: not a comment that survived, but a comment the
///     scanner never saw as one.
///  3. **A `/` can be a division sign or a regular expression.** Neither appears in these files
///     in a position that could open a comment, and the cost of being wrong is a FALSE POSITIVE
///     rather than a miss, which is the safe direction for a scanner whose job is refusal.
function stripComments(source, kind) {
  const out = source.split("");
  const n = source.length;
  let i = 0;
  const blank = (from, to) => {
    for (let k = from; k < to && k < n; k++) if (out[k] !== "\n") out[k] = " ";
  };
  // An HTML comment, wherever it is — including inside a template literal that carries markup.
  const htmlComment = () => {
    const end = source.indexOf("-->", i + 4);
    const to = end === -1 ? n : end + 3;
    blank(i, to);
    i = to;
  };
  while (i < n) {
    const c = source[i];
    const next = source[i + 1];
    if (source.startsWith("<!--", i)) {
      htmlComment();
      continue;
    }
    if (c === "/" && next === "/") {
      let end = source.indexOf("\n", i);
      if (end === -1) end = n;
      blank(i, end);
      i = end;
      continue;
    }
    if (c === "/" && next === "*") {
      const end = source.indexOf("*/", i + 2);
      const to = end === -1 ? n : end + 2;
      blank(i, to);
      i = to;
      continue;
    }
    // RUST'S APOSTROPHE, before the generic string branch, because it is usually a lifetime.
    // A char literal is `'x'` or `'\n'` — two or three bytes before the closing quote. Anything
    // else is a lifetime, and the right move is to step over the quote alone.
    if (kind === "rust" && c === "'") {
      if (source[i + 2] === "'") i += 3;
      else if (next === "\\") {
        const close = source.indexOf("'", i + 2);
        i = close === -1 ? i + 2 : close + 1;
      } else i += 1;
      continue;
    }
    // A string, of any of the three kinds. Skipped over rather than blanked: its contents are
    // exactly what this suite is looking for. A template literal is walked rather than jumped,
    // so the markup comments inside it are still found.
    if (c === '"' || c === "'" || c === "`") {
      const quote = c;
      i += 1;
      while (i < n) {
        if (source[i] === "\\") {
          i += 2;
          continue;
        }
        if (source[i] === quote) {
          i += 1;
          break;
        }
        if (quote === "`" && source.startsWith("<!--", i)) {
          htmlComment();
          continue;
        }
        i += 1;
      }
      continue;
    }
    i += 1;
  }
  return out.join("");
}

/// Every file a surface names, resolved from disk.
function filesOf(surface) {
  if (surface.file) return [surface.file];
  const exts = Array.isArray(surface.ext) ? surface.ext : [surface.ext];
  const skip = new Set(surface.skip || []);
  const found = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (skip.has(entry.name)) continue;
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (exts.some((e) => entry.name.endsWith(e))) found.push(full);
    }
  };
  walk(surface.dir);
  return found.sort();
}

/// Where in the file, in the form an editor opens.
function whereIs(source, index) {
  const before = source.slice(0, index);
  const line = before.split("\n").length;
  const column = index - (before.lastIndexOf("\n") + 1) + 1;
  return line + ":" + column;
}

/// Every banned string in one file's code, with its position and the line it sits on.
function hitsIn(file, kind) {
  const source = fs.readFileSync(file, "utf8");
  const code = stripComments(source, kind);
  const out = [];
  for (const banned of BANNED) {
    const rx = new RegExp(banned.pattern.source, banned.pattern.flags.includes("g") ? banned.pattern.flags : banned.pattern.flags + "g");
    let m;
    while ((m = rx.exec(code)) !== null) {
      const lineStart = source.lastIndexOf("\n", m.index) + 1;
      let lineEnd = source.indexOf("\n", m.index);
      if (lineEnd === -1) lineEnd = source.length;
      out.push({
        file,
        at: whereIs(source, m.index),
        found: m[0],
        why: banned.why,
        line: source.slice(lineStart, lineEnd).trim(),
      });
      if (m.index === rx.lastIndex) rx.lastIndex += 1;
    }
  }
  return out;
}

(async () => {
  const run = createRun("no-home-network", "no home-network path is ever built for the phone again");

  let filesScanned = 0;
  let bytesScanned = 0;

  for (const surface of SURFACES) {
    await run.check("1." + path.basename(surface.file || surface.dir) + "  " + surface.what, async () => {
      const files = filesOf(surface);
      assert(files.length > 0, "no files found for " + surface.what + " — the path moved and this scan went quiet");
      const hits = [];
      for (const file of files) {
        filesScanned += 1;
        bytesScanned += fs.statSync(file).size;
        hits.push(...hitsIn(file, surface.kind));
      }
      assertEqual(
        hits.map((h) => path.relative(RICHOS_DIR, h.file) + ":" + h.at + "  " + JSON.stringify(h.found) + " — " + h.why + "\n      " + h.line),
        [],
        "a home-network phone path is back in " + surface.what + " (CEO §61, and his question of 2026-09-19: " +
          '"How many more times will any \'home network\' related shit be associated with a phone or built for phone?")'
      );
      return files.length + " file(s) clean, " + BANNED.length + " patterns each";
    });
  }

  await run.check("2  the scan reached real files rather than an empty list", async () => {
    // A scanner that reports CLEAN over an empty corpus DID run, and is worse than no scanner.
    // Both numbers are floored so a moved directory fails loudly rather than passing quietly.
    assert(filesScanned >= 20, "only " + filesScanned + " file(s) were scanned — a surface's path has moved");
    assert(bytesScanned >= 200000, "only " + bytesScanned + " byte(s) were scanned — a surface is not being read");
    return filesScanned + " files, " + bytesScanned + " bytes";
  });

  await run.check("3  the comment stripper leaves strings whole", async () => {
    // The property the whole scan rests on. A `//` inside a URL must not start a comment, or
    // every pattern after it on that line is scanned against nothing.
    const source = [
      'const a = "https://mm1.local:8444/ca";',
      "// https://mm1.local:8444/ca in a comment",
      'const b = `at home`;',
      "/* at home in a block */",
      "const c = 'sixteen';",
    ].join("\n");
    const code = stripComments(source, "js");
    assertEqual(code.length, source.length, "the stripper changed the length, so positions would lie");
    assert(code.includes("https://mm1.local:8444/ca"), "a URL in a string was eaten as a comment");
    assert(code.includes("at home`"), "a template literal was eaten");
    assert(code.includes("sixteen"), "a single-quoted string was eaten");
    assert(!/in a comment/.test(code), "a line comment survived");
    assert(!/in a block/.test(code), "a block comment survived");
    return "5 lines: 3 strings kept, 2 comments blanked, length unchanged";
  });

  await run.check("4  a comment that records the ruling passes", async () => {
    // The negative control, and it is this file's whole reason for stripping comments: the
    // sentences that say WHY these strings are gone have to be allowed to name them.
    const source = [
      "// CEO §61: the At-home route, the trust QR at http://mm1.local:8444/ca and the sixteen",
      "// certificate taps are removed. Nothing self-signed is installed on the phone.",
      "/// `Route::AtHome` lived here for one day. The home plan in `serving_plan` went with it.",
      "let phone = 1;",
    ].join("\n");
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "richos-nohome-"));
    const file = path.join(dir, "ruling.js");
    fs.writeFileSync(file, source);
    const hits = hitsIn(file, "js");
    fs.rmSync(dir, { recursive: true, force: true });
    assertEqual(
      hits.map((h) => h.found),
      [],
      "a comment recording §61 was read as a violation of it, which would make the first " +
        "correct answer this suite gives a false one"
    );
    return "3 comment lines naming 7 of the banned strings, 0 hits";
  });

  await run.check("5  every banned string is found when it is in code", async () => {
    // THE POSITIVE PROBE. A negative result cannot tell "the thing is absent" from "the check
    // could not see it" — this project's own rule, written 2026-05-08 for tests. So each pattern
    // is planted in code position, one per line, and every one must come back.
    const planted = [
      'const a = "At home only";',
      "const b = AtHome;",
      'const c = "your own home network";',
      "const d = homePlan;",
      'const e = "https://mm1.local:8443/#pair=X";',
      "const f = 8444;",
      'const g = "trust QR";',
      'const h = "a self-signed certificate";',
      'const i = "certificate profile";',
      'const j = "then Remove Profile";',
      // FOUR SHAPES OF THE TAP COUNT, because the pattern that catches it was narrowed on
      // 2026-09-20 and a narrowing is only safe when the things it still has to catch are
      // written down. The first two are the product's own words; the last two are what a
      // reintroduction would plausibly write instead.
      'const k = "sixteen taps";',
      'const l = "the sixteen certificate taps";',
      'const m = "16 more taps in Settings";',
      'const n = "one certificate tap after another";',
    ].join("\n");
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "richos-nohome-"));
    const file = path.join(dir, "planted.js");
    fs.writeFileSync(file, planted);
    const hits = hitsIn(file, "js");
    fs.rmSync(dir, { recursive: true, force: true });
    const caught = new Set(hits.map((h) => h.why));
    const missed = BANNED.filter((b) => !caught.has(b.why)).map((b) => b.why);
    assertEqual(missed, [], "a banned string was planted in code and this suite did not see it");
    return BANNED.length + " patterns planted in code, " + hits.length + " hit(s), all " + BANNED.length + " caught";
  });

  await run.check("6  this file's own header is not a violation of what it enforces", async () => {
    // It quotes §61 and names every string it bans. If the stripper were wrong about this file
    // it would be wrong about `phone.js`, whose header does the same thing — so the two fail
    // together or pass together, and this is the cheaper one to read.
    const hits = hitsIn(__filename, "js");
    const inHeader = hits.filter((h) => Number(h.at.split(":")[0]) < 60);
    assertEqual(
      inHeader.map((h) => h.at + " " + JSON.stringify(h.found)),
      [],
      "this suite's own header trips it, so a comment recording the ruling is not safe anywhere"
    );
    return "header clean; the patterns and probes below it are code and are expected to match";
  });

  await run.check("7  a number that counted something else is not a violation", async () => {
    // THE NEGATIVE CONTROL FOR THE 2026-09-20 NARROWING, and the counterpart to check 5.
    // Check 5 proves the scanner can still see a reintroduction; this proves it has stopped
    // seeing the outbox's retry cap, which is the false positive that made check 1 red on a
    // clean tree at 049d8790. The first line is `web/web-app/test/queue.test.js:209`
    // verbatim — the exact byte sequence that failed — so this check goes red again if the
    // pattern is ever widened back over it.
    const benign = [
      "test('the retry schedule doubles from a second and stops at sixteen — the arithmetic, not a sleep', async () => {",
      "const WORDS_PER_ROW = 16;",
      "const cap = Math.min(next, sixteen);",
      "const rows = 16; // sixteen rows of sixteen, countable by eye",
    ].join("\n");
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "richos-nohome-"));
    const file = path.join(dir, "benign.js");
    fs.writeFileSync(file, benign);
    const hits = hitsIn(file, "js");
    fs.rmSync(dir, { recursive: true, force: true });
    assertEqual(
      hits.map((h) => h.at + " " + JSON.stringify(h.found) + " — " + h.why),
      [],
      "a number that counts retries or word-list rows was read as the trust walk's tap count"
    );
    return "4 lines of arithmetic naming the number, 0 hits";
  });

  const failed = run.report();
  console.log(
    failed
      ? "\n" + failed + " check(s) FAILED"
      : "\nno home-network phone path in " + filesScanned + " file(s) across " + SURFACES.length +
        " surfaces — and the scanner is proved able to find all " + BANNED.length + " of them."
  );
  process.exitCode = failed ? 1 : 0;
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
